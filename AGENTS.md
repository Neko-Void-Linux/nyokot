# Ñyokot - Agent Guide

## Commands
- **Run**: `cargo run` (needs `.env`, see `.env.example`)
- **Verify**: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- **Release**: `cargo build --release`

## Architecture
- **Entry**: `src/main.rs` (thin) — lib crate (`src/lib.rs`); integration tests in `tests/` use `nyokot::…`
- **Chatry port**: `umf.rs` (envelope + `trace_path`), `adapters/` (`Adapter` trait = `BaseAdapter`), `router.rs` (ingress → `egress.<platform>`), `bus.rs`
- **Commands**: `commands.rs` (pure grammar, multi-prefix from `NYOKOT_PREFIXES`, `|`-separated) + `command_exec.rs` (`handle_text` = single entry for all 3 gateways; `RepoSource` trait fakes GitHub in tests)
- **GitHub**: `github.rs` (HMAC-SHA256 over raw bytes, event parsers, App client via `get_org_installation`); `webhook.rs` resolves DB channels and emits `Egress` directly
- **DB**: SQLite WAL (`db.rs` + `migrations/`); `disconnect`/`pause`/`resume` are per-channel by design
- **Discord** (`adapters/discord.rs`): only live gateway (serenity, manual prefix dispatch — no poise); admin = owner or role with `ADMINISTRATOR`/`MANAGE_GUILD`, fail-closed
- **Stoat/Fluxer**: egress POST shim (`{BASE}/channels/{id}/messages` — shape provisional); ingress = one `inbound_text()` call once gateway docs land

## Gotchas
- Fail-closed boot: missing `GITHUB_APP_ID`/`GITHUB_PRIVATE_KEY_PATH`/`GITHUB_WEBHOOK_SECRET` → exit(1), no side effects
- `octocrab 0.44`: `builder().app(id, EncodingKey)`, `apps().get_org_installation(org)`, `installation(id)` is sync
- serenity 0.12: `Member::permissions` is deprecated — role lookup via `to_partial_guild`
- `sqlx::migrate!()` embeds `migrations/`; tests use temp-file DBs (never `sqlite::memory:` with a pool)
- No `.env`/`*.pem`/`*.db` in repo (gitignored); license is GPL-3.0-only — keep deps MIT/Apache/BSD
