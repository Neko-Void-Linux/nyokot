//! Command execution shared by all adapters.
//!
//! `handle_text` is the single entry point: platform gateways parse their
//! native event into (guild, channel, user, admin?, text) and call it.
//! `disconnect`/`pause`/`resume` are strictly per-channel.

use crate::commands::{help_text, parse, Command};
use crate::db::Db;
use crate::error::Result;
use std::future::Future;

/// Anything that can validate org repos (real [`crate::github::GithubApp`]
/// in prod, a fake in tests).
#[allow(async_fn_in_trait)] // single-crate binary; Send-ness verified at spawn sites
pub trait RepoSource: Send + Sync {
    fn repo_exists(&self, name: &str)
        -> impl Future<Output = Result<Option<(i64, String)>>> + Send;
}

pub struct Target<'a> {
    pub platform: &'a str,
    pub guild_id: &'a str,
    pub channel_id: &'a str,
}

/// Full inbound flow. Returns `None` when the text is not a command.
pub async fn handle_text<S: RepoSource>(
    db: &Db,
    gh: &S,
    prefixes: &[String],
    target: &Target<'_>,
    is_admin: bool,
    text: &str,
) -> Option<String> {
    let cmd = parse(prefixes, text)?;
    match cmd {
        Ok(Command::Help) => Some(help_text(
            prefixes.first().map(String::as_str).unwrap_or("/"),
        )),
        Ok(Command::Disconnect) => {
            match crate::db::disconnect_channel(db, target.platform, target.channel_id).await {
                Ok(_) => Some("Channel unlinked. No further notifications here.".into()),
                Err(e) => Some(format!("Failed to disconnect: {e}")),
            }
        }
        Ok(Command::Pause) => {
            match crate::db::set_paused(db, target.platform, target.channel_id, true).await {
                Ok(true) => Some("Notifications paused in this channel.".into()),
                Ok(false) => Some("This channel is not linked. Use `connect` first.".into()),
                Err(e) => Some(format!("Failed to pause: {e}")),
            }
        }
        Ok(Command::Resume) => {
            match crate::db::set_paused(db, target.platform, target.channel_id, false).await {
                Ok(true) => Some("Notifications resumed in this channel.".into()),
                Ok(false) => Some("This channel is not linked. Use `connect` first.".into()),
                Err(e) => Some(format!("Failed to resume: {e}")),
            }
        }
        Ok(Command::Connect(args)) => {
            if !is_admin {
                return Some("Only server admins can use this command.".into());
            }
            Some(execute_connect(db, gh, target, args).await)
        }
        Err(e) => Some(format!("Error: {e}")),
    }
}

async fn execute_connect<S: RepoSource>(
    db: &Db,
    gh: &S,
    target: &Target<'_>,
    args: crate::commands::ConnectArgs,
) -> String {
    // Validate BEFORE writing anything (no partial state on unknown repos).
    let mut repo_ids: Vec<i64> = vec![];
    let mut missing: Vec<String> = vec![];
    if !args.all_repos {
        for r in &args.repos {
            // Accept `org/repo` or bare `repo`.
            let bare = r.split('/').next_back().unwrap_or(r);
            match gh.repo_exists(bare).await {
                Ok(Some((id, full))) => {
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO github_repos(repo_id, full_name) VALUES(?1, ?2)",
                    )
                    .bind(id)
                    .bind(&full)
                    .execute(db)
                    .await;
                    repo_ids.push(id);
                }
                Ok(None) => missing.push(r.clone()),
                Err(e) => return format!("GitHub lookup failed: {e}"),
            }
        }
        if !missing.is_empty() {
            return format!(
                "Unknown repos in this org: {}. Nothing was saved.",
                missing.join(", ")
            );
        }
    }
    if let Err(e) =
        crate::db::ensure_guild_channel(db, target.platform, target.guild_id, target.channel_id)
            .await
    {
        return format!("Failed to link channel: {e}");
    }
    if let Err(e) =
        crate::db::set_subscriptions(db, target.platform, target.channel_id, &args.kinds).await
    {
        return format!("Failed to save subscriptions: {e}");
    }
    if let Err(e) =
        crate::db::set_repo_filters(db, target.platform, target.channel_id, &repo_ids).await
    {
        return format!("Failed to save repo filters: {e}");
    }
    let repos = if args.all_repos {
        "all repos".into()
    } else {
        args.repos.join(", ")
    };
    format!(
        "Linked! Notifying [{}] about {}.",
        args.kinds.join(", "),
        repos
    )
}
