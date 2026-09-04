// Errors are cold paths; boxing them would only add noise.
#![allow(clippy::result_large_err)]

use nyokot::adapters::{egress_loop, Adapter, DiscordAdapter, FluxerAdapter, StoatAdapter};
use nyokot::{bus, config, db, github, router, webhook};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    dotenvy::dotenv().ok();

    if let Err(e) = run().await {
        tracing::error!("fatal: {e}");
        std::process::exit(1);
    }
}

async fn run() -> nyokot::error::Result<()> {
    let cfg = config::Config::from_env()?;
    let db = db::connect(&cfg.database_url).await?;
    let gh = Arc::new(github::GithubApp::new(
        cfg.github_app_id,
        &cfg.github_private_key_path,
        cfg.github_org.clone(),
    )?);

    let (bus, _) = bus::new_bus();
    let interval = Duration::from_secs(cfg.notify_interval_secs);

    // Router: ingress fan-out with trace_path loop prevention.
    tokio::spawn(router::run(bus.clone(), bus.subscribe()));

    // Webhook listener (verified BEFORE use — see webhook.rs).
    let wstate = webhook::WebhookState {
        secret: cfg.github_webhook_secret.clone(),
        db: db.clone(),
        bus: bus.clone(),
    };
    let listener = tokio::net::TcpListener::bind(&cfg.webhook_bind).await?;
    tracing::info!("webhook listening on {}", cfg.webhook_bind);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, webhook::router(wstate)).await {
            tracing::error!("webhook server ended: {e}");
        }
    });

    // Adapters (each starts only if its token is configured).
    let discord = Arc::new(DiscordAdapter::new(
        cfg.discord_token.clone(),
        cfg.prefixes.clone(),
        db.clone(),
        gh.clone(),
        interval,
    ));
    let stoat = Arc::new(StoatAdapter::new(
        cfg.stoat_token.clone(),
        cfg.stoat_base_url.clone(),
        cfg.prefixes.clone(),
        db.clone(),
        interval,
    ));
    let fluxer = Arc::new(FluxerAdapter::new(
        cfg.fluxer_token.clone(),
        cfg.fluxer_base_url.clone(),
        cfg.prefixes.clone(),
        db.clone(),
        interval,
    ));

    discord.start().await?;
    stoat.start().await?;
    fluxer.start().await?;
    tokio::spawn(egress_loop(discord.clone(), bus.clone()));
    tokio::spawn(egress_loop(stoat.clone(), bus.clone()));
    tokio::spawn(egress_loop(fluxer.clone(), bus.clone()));
    tracing::info!("Ñyokot started.");

    // Graceful shutdown (port of Chatry's SIGINT/SIGTERM handling).
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutdown signal received");
    discord.stop().await?;
    stoat.stop().await?;
    fluxer.stop().await?;
    Ok(())
}
