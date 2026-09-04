use crate::error::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

pub type Db = SqlitePool;

pub async fn connect(database_url: &str) -> Result<Db> {
    let opts = SqliteConnectOptions::from_str(database_url)
        .map_err(|e| crate::error::Error::InvalidConfig(format!("bad DATABASE_URL: {e}")))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Channels that must receive `event_type` for `repo_id`:
/// subscribed, not paused, and (no repo filters OR repo explicitly allowed).
pub async fn resolve_channels(
    db: &Db,
    event_type: &str,
    repo_id: i64,
) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT s.channel_id, s.platform
         FROM event_subscriptions s
         JOIN channel_configs c
           ON c.channel_id = s.channel_id AND c.platform = s.platform
         WHERE s.event_type = ?1
           AND c.paused = 0
           AND (NOT EXISTS (SELECT 1 FROM repo_filters f
                            WHERE f.channel_id = s.channel_id AND f.platform = s.platform)
                OR EXISTS (SELECT 1 FROM repo_filters f
                           WHERE f.channel_id = s.channel_id AND f.platform = s.platform
                             AND f.repo_id = ?2))",
    )
    .bind(event_type)
    .bind(repo_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn ensure_guild_channel(
    db: &Db,
    platform: &str,
    guild_id: &str,
    channel_id: &str,
) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO guilds(platform, guild_id) VALUES(?1, ?2)")
        .bind(platform)
        .bind(guild_id)
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO channel_configs(channel_id, guild_id, platform, paused)
         VALUES(?1, ?2, ?3, 0)",
    )
    .bind(channel_id)
    .bind(guild_id)
    .bind(platform)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn set_subscriptions(
    db: &Db,
    platform: &str,
    channel_id: &str,
    kinds: &[String],
) -> Result<()> {
    sqlx::query("DELETE FROM event_subscriptions WHERE channel_id = ?1 AND platform = ?2")
        .bind(channel_id)
        .bind(platform)
        .execute(db)
        .await?;
    for k in kinds {
        sqlx::query(
            "INSERT OR IGNORE INTO event_subscriptions(channel_id, platform, event_type)
             VALUES(?1, ?2, ?3)",
        )
        .bind(channel_id)
        .bind(platform)
        .bind(k)
        .execute(db)
        .await?;
    }
    Ok(())
}

pub async fn set_repo_filters(
    db: &Db,
    platform: &str,
    channel_id: &str,
    repo_ids: &[i64],
) -> Result<()> {
    sqlx::query("DELETE FROM repo_filters WHERE channel_id = ?1 AND platform = ?2")
        .bind(channel_id)
        .bind(platform)
        .execute(db)
        .await?;
    for id in repo_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO repo_filters(channel_id, platform, repo_id) VALUES(?1, ?2, ?3)",
        )
        .bind(channel_id)
        .bind(platform)
        .bind(id)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Per-channel pause/resume/disconnect (approved semantics).
pub async fn set_paused(db: &Db, platform: &str, channel_id: &str, paused: bool) -> Result<bool> {
    let r = sqlx::query(
        "UPDATE channel_configs SET paused = ?1 WHERE channel_id = ?2 AND platform = ?3",
    )
    .bind(if paused { 1 } else { 0 })
    .bind(channel_id)
    .bind(platform)
    .execute(db)
    .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn disconnect_channel(db: &Db, platform: &str, channel_id: &str) -> Result<bool> {
    for t in ["repo_filters", "event_subscriptions", "channel_configs"] {
        sqlx::query(&format!(
            "DELETE FROM {t} WHERE channel_id = ?1 AND platform = ?2"
        ))
        .bind(channel_id)
        .bind(platform)
        .execute(db)
        .await?;
    }
    Ok(true)
}

pub async fn channel_state(
    db: &Db,
    platform: &str,
    channel_id: &str,
) -> Result<Option<(bool, Vec<String>, i64)>> {
    let paused: Option<i32> = sqlx::query_scalar(
        "SELECT paused FROM channel_configs WHERE channel_id = ?1 AND platform = ?2",
    )
    .bind(channel_id)
    .bind(platform)
    .fetch_optional(db)
    .await?;
    let paused = match paused {
        Some(p) => p != 0,
        None => return Ok(None),
    };
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM event_subscriptions WHERE channel_id = ?1 AND platform = ?2 ORDER BY 1",
    )
    .bind(channel_id)
    .bind(platform)
    .fetch_all(db)
    .await?;
    let nfilters: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM repo_filters WHERE channel_id = ?1 AND platform = ?2",
    )
    .bind(channel_id)
    .bind(platform)
    .fetch_one(db)
    .await?;
    Ok(Some((paused, kinds, nfilters)))
}
