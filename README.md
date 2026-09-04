# Ñyokot

GPL-3.0-only GitHub organization notifier bot for **Discord**, **Stoat** and **Fluxer**.
Listens to one central GitHub App webhook and routes pull-request / issue /
push events to the channels each server linked — nothing more, nothing less.

## Commands

Prefixes are configured in `NYOKOT_PREFIXES` (`|`-separated, e.g. `!|¡|.|,`); every prefix
works on every platform. All commands run **in the channel they target** and
`connect` requires **server admin** (`Administrator` / `Manage Guild` or the
platform equivalent). `disconnect` / `pause` / `resume` are strictly
**per-channel**.

| Command | Effect |
|---|---|
| `<p>help` | This help |
| `<p>connect -r <repos…\|all> -n <pr\|is\|co\|all…>` | Link this channel. Repos are validated against the org first — unknown names abort with no partial state |
| `<p>disconnect` | Unlink this channel |
| `<p>pause` | Pause notifications in this channel |
| `<p>resume` | Resume notifications in this channel |

Example: `/connect -r kasha cnr nk-web -n pr is co` notifies PRs, issues and
pushes for those repos. `/connect -r all -n all` is the catch-all.

## Setup

1. Create a **GitHub App** on the org with read-only **Contents, Issues, Pull
   requests, Metadata**, subscribed to **push, pull_request, issues,
   issue_comment**. Install it on the org.
2. `cp .env.example .env` and fill it in (private key is referenced **by path**,
   never pasted). `cp` the PEM next to it with `chmod 600`.
3. Point the App's webhook at `https://<host>/webhook` with the same secret as
   `GITHUB_WEBHOOK_SECRET`.
4. `cargo run --release` (or Docker / systemd, see below).

## Security model

- Webhook payloads verified with **HMAC-SHA256 in constant time over raw
  bytes** before any JSON parsing; missing/invalid signature → `401`.
- GitHub App least privilege (read-only) + short-lived installation tokens.
- Admin-only `connect`; fail-closed permission checks; secrets only via env,
  never logged.
- Proactive per-channel pacing avoids Discord 429s; embeds truncated to
  Discord's hard limits (multibyte-safe).

## Deploy

- **Docker**: multi-stage build (`cargo-chef` cache), static musl binary,
  distroless `nonroot` runtime — `docker build -t nyokot .`.
- **systemd**: see `bot.service` (sandboxed: `NoNewPrivileges`, `PrivateTmp`,
  `ProtectSystem=full`).

## Layout

`src/umf.rs` (message envelope + `trace_path` loop prevention),
`src/adapters/` (one `Adapter` trait impl per platform),
`src/github.rs` (HMAC, event parsers, App client), `src/commands.rs` +
`src/command_exec.rs` (shared grammar, identical on all platforms),
`src/router.rs`, `src/db.rs` (SQLite WAL + migrations), `src/notifier.rs`
(paced outbox), `src/webhook.rs` (`POST /webhook`).

## License

GNU General Public License v3.0 only — see `LICENSE`.
