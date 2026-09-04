//! Discord adapter — the only fully live gateway (serenity 0.12).
//!
//! Ingress: `messageCreate` → shared [`crate::command_exec::handle_text`]
//! (admin-only `connect`; DMs ignored — server channels only).
//! Egress: paced [`Outbox`] → rich embed, truncated to Discord's hard limits
//! (title 256 / description 4096 / field 1024 / total 6000 / 25 fields).

use super::Adapter;
use crate::command_exec::{handle_text, RepoSource, Target};
use crate::db::Db;
use crate::error::Result;
use crate::github::truncate;
use crate::notifier::{Deliver, Outbox};
use crate::umf::Envelope;
use serenity::all::{
    ChannelId, Client, Colour, Context, CreateEmbed, CreateEmbedAuthor, CreateMessage,
    EventHandler, GatewayIntents, GuildId, Http, Message, Ready, ShardManager, UserId,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct DiscordAdapter {
    token: Option<String>,
    prefixes: Vec<String>,
    db: Db,
    gh: Arc<crate::github::GithubApp>,
    notify_interval: Duration,
    http: Mutex<Option<Arc<Http>>>,
    outbox: Mutex<Option<Outbox>>,
    shards: Mutex<Option<Arc<ShardManager>>>,
}

impl DiscordAdapter {
    pub fn new(
        token: Option<String>,
        prefixes: Vec<String>,
        db: Db,
        gh: Arc<crate::github::GithubApp>,
        notify_interval: Duration,
    ) -> Self {
        Self {
            token,
            prefixes,
            db,
            gh,
            notify_interval,
            http: Mutex::new(None),
            outbox: Mutex::new(None),
            shards: Mutex::new(None),
        }
    }

    fn outbox_clone(&self) -> Option<Outbox> {
        self.outbox.lock().ok().and_then(|o| o.clone())
    }
}

struct Handler {
    prefixes: Vec<String>,
    db: Db,
    gh: Arc<crate::github::GithubApp>,
}

async fn is_admin(ctx: &Context, guild: GuildId, user: UserId) -> bool {
    let (Ok(pg), Ok(member)) = (
        guild.to_partial_guild(&ctx.http).await,
        guild.member(&ctx.http, user).await,
    ) else {
        // Fail-closed: unverifiable caller is not an admin.
        return false;
    };
    if pg.owner_id == user {
        return true;
    }
    // Guild-wide roles (ADMINISTRATOR bypasses channel overwrites anyway).
    let everyone = serenity::all::RoleId::new(guild.get());
    member
        .roles
        .iter()
        .chain(std::iter::once(&everyone))
        .filter_map(|r| pg.roles.get(r))
        .any(|role| role.permissions.administrator() || role.permissions.manage_guild())
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        let Some(guild_id) = msg.guild_id else { return }; // server channels only
        let admin = is_admin(&ctx, guild_id, msg.author.id).await;
        let target = Target {
            platform: "discord",
            guild_id: &guild_id.to_string(),
            channel_id: &msg.channel_id.to_string(),
        };
        let reply = handle_text(
            &self.db,
            self.gh.as_ref(),
            &self.prefixes,
            &target,
            admin,
            &msg.content,
        )
        .await;
        if let Some(text) = reply {
            if let Err(e) = msg.channel_id.say(&ctx.http, text).await {
                tracing::error!("[discord] reply failed: {e}");
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        tracing::info!("[discord] connected as {}", ready.user.tag());
    }
}

impl RepoSource for crate::github::GithubApp {
    async fn repo_exists(&self, name: &str) -> Result<Option<(i64, String)>> {
        crate::github::GithubApp::repo_exists(self, name).await
    }
}

fn build_message(env: &Envelope) -> CreateMessage {
    // Hard limits: title<=256, desc<=4096. Upstream text is pre-truncated.
    let title = truncate(
        env.body
            .title
            .clone()
            .unwrap_or_else(|| "GitHub update".into()),
        250,
    );
    let mut embed = CreateEmbed::new()
        .title(title)
        .description(truncate(env.body.text.clone(), 4000))
        .colour(Colour::new(env.body.color.unwrap_or(0x5865F2)));
    if let Some(url) = &env.body.url {
        if !url.is_empty() {
            embed = embed.url(url);
        }
    }
    embed = embed.author(CreateEmbedAuthor::new(format!(
        "{} / {}",
        env.head.source.platform, env.head.source.username
    )));
    CreateMessage::new().embed(embed)
}

impl Adapter for DiscordAdapter {
    fn platform_name(&self) -> &'static str {
        "discord"
    }

    async fn start(&self) -> Result<()> {
        let Some(token) = self.token.clone() else {
            tracing::warn!("DISCORD_TOKEN not set — discord adapter disabled.");
            return Ok(());
        };
        let http = Arc::new(Http::new(&token));
        *self.http.lock().unwrap() = Some(http.clone());

        let deliver: Deliver = Arc::new(move |env: Envelope| {
            let http = http.clone();
            Box::pin(async move {
                let ch: std::result::Result<u64, _> = env.head.source.channel_id.parse();
                match ch {
                    Ok(id) => {
                        if let Err(e) = ChannelId::new(id)
                            .send_message(&http, build_message(&env))
                            .await
                        {
                            tracing::error!("[discord] egress send failed: {e}");
                        }
                    }
                    Err(_) => tracing::error!("[discord] bad channel id in envelope"),
                }
            })
        });
        *self.outbox.lock().unwrap() = Some(Outbox::spawn(self.notify_interval, deliver));

        let handler = Handler {
            prefixes: self.prefixes.clone(),
            db: self.db.clone(),
            gh: self.gh.clone(),
        };
        let intents = GatewayIntents::GUILDS
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;
        let mut client = Client::builder(token, intents)
            .event_handler(handler)
            .await?;
        *self.shards.lock().unwrap() = Some(client.shard_manager.clone());
        tokio::spawn(async move {
            if let Err(e) = client.start().await {
                tracing::error!("[discord] gateway ended: {e}");
            }
        });
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let shards = self.shards.lock().unwrap().clone();
        if let Some(shards) = shards {
            shards.shutdown_all().await;
        }
        tracing::info!("[discord] stopped");
        Ok(())
    }

    async fn process_egress(&self, envelope: &Envelope) -> Result<()> {
        match self.outbox_clone() {
            Some(o) => o.push(envelope.clone()).await,
            None => tracing::warn!("[discord] egress with no outbox (not started?)"),
        }
        Ok(())
    }

    async fn send_message(&self, envelope: &Envelope) -> Result<Vec<String>> {
        let http = self.http.lock().unwrap().clone();
        let Some(http) = http else { return Ok(vec![]) };
        let id: u64 = envelope.head.source.channel_id.parse().unwrap_or(0);
        if id == 0 {
            return Ok(vec![]);
        }
        let m = ChannelId::new(id)
            .send_message(&http, build_message(envelope))
            .await?;
        Ok(vec![m.id.to_string()])
    }
}
