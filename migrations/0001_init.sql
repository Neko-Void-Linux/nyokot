CREATE TABLE IF NOT EXISTS guilds(
    platform TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    PRIMARY KEY(platform, guild_id)
);

CREATE TABLE IF NOT EXISTS github_repos(
    repo_id INTEGER PRIMARY KEY,
    full_name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS channel_configs(
    channel_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    paused INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(channel_id, platform)
);

CREATE TABLE IF NOT EXISTS event_subscriptions(
    channel_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    event_type TEXT NOT NULL,
    PRIMARY KEY(channel_id, platform, event_type)
);

CREATE TABLE IF NOT EXISTS repo_filters(
    channel_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    repo_id INTEGER NOT NULL,
    PRIMARY KEY(channel_id, platform, repo_id)
);

CREATE INDEX IF NOT EXISTS idx_subs_type ON event_subscriptions(event_type);
