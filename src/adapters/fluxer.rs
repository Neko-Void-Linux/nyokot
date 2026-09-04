//! Fluxer adapter.
//!
//! Status: EGRESS LIVE (generic JSON POST), INGRESS PENDING platform docs.
//!
//! Fluxer is a Discord-like bot platform (bot token + channels). Until its
//! gateway/polling API is confirmed, outbound delivery POSTs
//! `{text,title,url}` to `{FLUXER_BASE_URL}/channels/{id}/messages` with
//! `Authorization: Bot <token>`. If that shape differs from the real API,
//! only `post_message` below changes.
//!
//! Wiring live ingress later is one call:
//! `command_exec::handle_text(&db, &gh, &prefixes, &target, is_admin, text)`.

use super::Adapter;
use crate::command_exec::{handle_text, RepoSource, Target};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::notifier::{Deliver, Outbox};
use crate::umf::Envelope;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct FluxerAdapter {
    token: Option<String>,
    base_url: String,
    prefixes: Vec<String>,
    db: Db,
    notify_interval: Duration,
    http: reqwest::Client,
    outbox: Mutex<Option<Outbox>>,
}

impl FluxerAdapter {
    pub fn new(
        token: Option<String>,
        base_url: String,
        prefixes: Vec<String>,
        db: Db,
        notify_interval: Duration,
    ) -> Self {
        Self {
            token,
            base_url: base_url.trim_end_matches('/').to_string(),
            prefixes,
            db,
            notify_interval,
            http: reqwest::Client::new(),
            outbox: Mutex::new(None),
        }
    }

    /// Ingress entry point: the platform gateway calls this per inbound
    /// message once the gateway API is confirmed. (Covered by tests.)
    pub async fn inbound_text<S: RepoSource>(
        &self,
        gh: &S,
        guild_id: &str,
        channel_id: &str,
        is_admin: bool,
        text: &str,
    ) -> Option<String> {
        let target = Target {
            platform: "fluxer",
            guild_id,
            channel_id,
        };
        handle_text(&self.db, gh, &self.prefixes, &target, is_admin, text).await
    }

    async fn post_message(&self, channel: &str, env: &Envelope) -> Result<Vec<String>> {
        let Some(token) = self.token.as_ref() else {
            return Ok(vec![]);
        };
        let url = format!("{}/channels/{channel}/messages", self.base_url);
        let res = self
            .http
            .post(&url)
            .header("Authorization", format!("Bot {token}"))
            .json(&serde_json::json!({
                "text": env.body.text,
                "title": env.body.title,
                "url": env.body.url,
            }))
            .send()
            .await
            .map_err(|e| Error::Http(format!("[fluxer] post failed: {e}")))?;
        if !res.status().is_success() {
            return Err(Error::Http(format!(
                "[fluxer] post rejected: {}",
                res.status()
            )));
        }
        Ok(vec![])
    }
}

impl Adapter for FluxerAdapter {
    fn platform_name(&self) -> &'static str {
        "fluxer"
    }

    async fn start(&self) -> Result<()> {
        if self.token.is_none() {
            tracing::warn!("FLUXER_TOKEN not set — fluxer adapter disabled.");
            return Ok(());
        }
        // Egress path is live; ingress gateway wiring awaits platform docs.
        tracing::warn!("[fluxer] live ingress pending API docs; egress delivery active.");
        let (token, base_url, http) =
            (self.token.clone(), self.base_url.clone(), self.http.clone());
        let deliver: Deliver = Arc::new(move |env: Envelope| {
            let (token, base_url, http) = (token.clone(), base_url.clone(), http.clone());
            Box::pin(async move {
                let Some(t) = token.as_ref() else { return };
                let url = format!(
                    "{}/channels/{}/messages",
                    base_url, env.head.source.channel_id
                );
                let res = http
                    .post(&url)
                    .header("Authorization", format!("Bot {t}"))
                    .json(&serde_json::json!({
                        "text": env.body.text,
                        "title": env.body.title,
                        "url": env.body.url,
                    }))
                    .send()
                    .await;
                match res {
                    Ok(r) if r.status().is_success() => {}
                    Ok(r) => tracing::error!("[fluxer] post rejected: {}", r.status()),
                    Err(e) => tracing::error!("[fluxer] post failed: {e}"),
                }
            })
        });
        *self.outbox.lock().unwrap() = Some(Outbox::spawn(self.notify_interval, deliver));
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("[fluxer] stopped");
        Ok(())
    }

    async fn process_egress(&self, envelope: &Envelope) -> Result<()> {
        let outbox = self.outbox.lock().unwrap().clone();
        match outbox {
            Some(o) => o.push(envelope.clone()).await,
            None => tracing::warn!("[fluxer] egress with no outbox (not started?)"),
        }
        Ok(())
    }

    async fn send_message(&self, envelope: &Envelope) -> Result<Vec<String>> {
        let ch = envelope.head.source.channel_id.clone();
        self.post_message(&ch, envelope).await
    }
}
