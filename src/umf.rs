//! Universal Message Format — port of Chatry's `src/core/types/umf.ts`.
//!
//! Every cross-platform message travels as an [`Envelope`]. The `trace_path`
//! records each platform the envelope visited so the router never loops.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

pub const PLATFORM_DISCORD: &str = "discord";
pub const PLATFORM_STOAT: &str = "stoat";
pub const PLATFORM_FLUXER: &str = "fluxer";

pub const EVENT_PR: &str = "pr";
pub const EVENT_ISSUE: &str = "is";
pub const EVENT_COMMIT: &str = "co";

/// All notifiable event kinds (`-n` flag values).
pub const EVENT_KINDS: [&str; 3] = [EVENT_PR, EVENT_ISSUE, EVENT_COMMIT];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub correlation_id: String,
    pub timestamp: i64,
    pub platform: String,
    pub channel_id: String,
    pub user_id: String,
    pub username: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Head {
    pub id: String,
    pub correlation_id: String,
    pub timestamp: i64,
    pub kind: String,
    pub source: Source,
    pub trace_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body {
    pub text: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub color: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub head: Head,
    pub body: Body,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Parameters for [`create_envelope`] (one struct instead of ten args).
#[derive(Debug, Clone, Default)]
pub struct EnvelopeParams {
    pub platform: String,
    pub channel_id: String,
    pub user_id: String,
    pub username: String,
    pub avatar: Option<String>,
    pub kind: String,
    pub title: Option<String>,
    pub text: String,
    pub url: Option<String>,
    pub color: Option<u32>,
}

/// Port of `createEnvelope()`: `platform` is lowercased and
/// `trace_path` starts with the origin platform.
pub fn create_envelope(p: EnvelopeParams) -> Result<Envelope> {
    if p.platform.trim().is_empty() || p.channel_id.trim().is_empty() {
        return Err(Error::Umf(
            "source.platform and source.channelId are required".into(),
        ));
    }
    let platform = p.platform.to_lowercase();
    let id = uuid();
    let ts = now_ms();
    Ok(Envelope {
        head: Head {
            id: id.clone(),
            correlation_id: id.clone(),
            timestamp: ts,
            kind: p.kind,
            source: Source {
                id: id.clone(),
                correlation_id: id,
                timestamp: ts,
                platform: platform.clone(),
                channel_id: p.channel_id,
                user_id: p.user_id,
                username: p.username,
                avatar: p.avatar,
            },
            trace_path: vec![platform],
        },
        body: Body {
            text: p.text,
            title: p.title,
            url: p.url,
            color: p.color,
        },
    })
}

fn uuid() -> String {
    // ponytail: no uuid crate for one id — 128 bits from nanos + pid + counter.
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let t = now_ms() as u64;
    let c = CTR.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{:x}", t, std::process::id() as u64, c)
}

/// Port of `validateEnvelope()`: cheap structural guard for ingress/egress.
pub fn validate_envelope(env: &Envelope) -> bool {
    !env.head.id.is_empty()
        && !env.head.source.platform.is_empty()
        && !env.head.source.channel_id.is_empty()
        && !env.head.trace_path.is_empty()
}
