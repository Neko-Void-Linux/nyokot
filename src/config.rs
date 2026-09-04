use crate::error::{Error, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    /// Command prefixes (`NYOKOT_PREFIXES`, `|`-separated — `|` was chosen
    /// because `,` itself is a valid prefix).
    pub prefixes: Vec<String>,
    pub discord_token: Option<String>,
    pub stoat_token: Option<String>,
    pub stoat_base_url: String,
    pub fluxer_token: Option<String>,
    pub fluxer_base_url: String,
    pub github_app_id: u64,
    pub github_private_key_path: String,
    pub github_webhook_secret: String,
    pub github_org: String,
    pub database_url: String,
    pub webhook_bind: String,
    pub notify_interval_secs: u64,
}

fn required(name: &'static str) -> Result<String> {
    env::var(name)
        .map_err(|_| Error::MissingEnv(name))
        .and_then(|v| {
            if v.trim().is_empty() {
                Err(Error::MissingEnv(name))
            } else {
                Ok(v)
            }
        })
}

fn optional(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    /// Fail-closed: required values must be present and non-empty.
    pub fn from_env() -> Result<Self> {
        let prefixes: Vec<String> = env::var("NYOKOT_PREFIXES")
            .unwrap_or_else(|_| "/".to_string())
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if prefixes.is_empty() {
            return Err(Error::InvalidConfig(
                "NYOKOT_PREFIXES must list at least one prefix".into(),
            ));
        }

        Ok(Self {
            prefixes,
            discord_token: optional("DISCORD_TOKEN"),
            stoat_token: optional("STOAT_TOKEN"),
            stoat_base_url: env::var("STOAT_BASE_URL")
                .unwrap_or_else(|_| "https://stoat.example/api".into()),
            fluxer_token: optional("FLUXER_TOKEN"),
            fluxer_base_url: env::var("FLUXER_BASE_URL")
                .unwrap_or_else(|_| "https://fluxer.example/api".into()),
            github_app_id: required("GITHUB_APP_ID")?
                .parse::<u64>()
                .map_err(|_| Error::InvalidConfig("GITHUB_APP_ID must be a number".into()))?,
            github_private_key_path: required("GITHUB_PRIVATE_KEY_PATH")?,
            github_webhook_secret: required("GITHUB_WEBHOOK_SECRET")?,
            github_org: env::var("GITHUB_ORG").unwrap_or_else(|_| "Neko-Void-Linux".into()),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:nyokot.db?mode=rwc".into()),
            webhook_bind: env::var("WEBHOOK_BIND").unwrap_or_else(|_| "0.0.0.0:3000".into()),
            notify_interval_secs: env::var("NOTIFY_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .max(1),
        })
    }
}
