//! GitHub side: HMAC webhook verification, event parsers, App API client.
//!
//! Security model (per SPEC): GitHub App with read-only
//! Contents / Issues / Pull requests / Metadata, short-lived installation
//! tokens, one central `/webhook` endpoint verified with HMAC-SHA256 in
//! constant time over the RAW request bytes.

use crate::error::{Error, Result};
use crate::umf::{EVENT_COMMIT, EVENT_ISSUE, EVENT_PR};
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyEvent {
    /// `-n` kind: `pr` | `is` | `co`.
    pub kind: &'static str,
    pub repo_id: i64,
    pub repo_full_name: String,
    pub title: String,
    pub text: String,
    pub url: String,
    pub author: String,
    pub color: u32,
}

pub const COLOR_COMMIT: u32 = 0x5865F2;
pub const COLOR_PR_OPEN: u32 = 0x2EA043;
pub const COLOR_PR_MERGED: u32 = 0x8957E5;
pub const COLOR_PR_CLOSED: u32 = 0xDA3633;
pub const COLOR_ISSUE: u32 = 0xD29922;

/// Constant-time HMAC-SHA256 verification over raw bytes.
/// Returns `false` (never panics) on missing prefix, bad hex, or mismatch.
pub fn verify_signature(secret: &[u8], body: &[u8], header: &str) -> bool {
    let hex_part = match header.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };
    let expected = match hex::decode(hex_part.trim()) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn repo_of(payload: &serde_json::Value) -> (i64, String) {
    let r = payload.get("repository");
    let id = r
        .and_then(|x| x.get("id"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    let name = r.map(|x| str_field(x, "full_name")).unwrap_or_default();
    (id, name)
}

fn sender_of(payload: &serde_json::Value) -> String {
    payload
        .get("sender")
        .map(|x| str_field(x, "login"))
        .unwrap_or_default()
}

/// Dispatch on `X-GitHub-Event`. Returns `None` for actions we don't notify.
pub fn parse_event(event: &str, payload: &serde_json::Value) -> Option<NotifyEvent> {
    match event {
        "push" => parse_push(payload),
        "pull_request" => parse_pr(payload),
        "issues" => parse_issue(payload),
        "issue_comment" => parse_comment(payload),
        _ => None,
    }
}

fn parse_push(p: &serde_json::Value) -> Option<NotifyEvent> {
    let (repo_id, full_name) = repo_of(p);
    if full_name.is_empty() {
        return None;
    }
    let branch = p
        .get("ref")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .replace("refs/heads/", "");
    let commits = p
        .get("commits")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    if commits.is_empty() {
        return None;
    }
    let mut lines = vec![];
    let mut total = 0usize;
    let mut hidden = 0usize;
    for c in &commits {
        let sha = str_field(c, "id");
        let short = sha.get(..7).unwrap_or(&sha);
        let url = str_field(c, "url");
        let msg = str_field(c, "message")
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        let line = format!("[{short}]({url}) {msg}");
        if total + line.len() > 3800 {
            hidden += 1;
            continue;
        }
        total += line.len();
        lines.push(line);
    }
    if hidden > 0 {
        lines.push(format!("... and {hidden} more commits"));
    }
    Some(NotifyEvent {
        kind: EVENT_COMMIT,
        repo_id,
        repo_full_name: full_name.clone(),
        title: format!("[{full_name}:{branch}] {} new commit(s)", commits.len()),
        text: lines.join("\n"),
        url: str_field(p, "compare"),
        author: sender_of(p),
        color: COLOR_COMMIT,
    })
}

fn parse_pr(p: &serde_json::Value) -> Option<NotifyEvent> {
    let action = str_field(p, "action");
    let pr = p.get("pull_request")?;
    let merged = pr.get("merged").and_then(|m| m.as_bool()).unwrap_or(false);
    let (label, color) = match (action.as_str(), merged) {
        ("opened", _) | ("reopened", _) => ("Open", COLOR_PR_OPEN),
        ("synchronize", _) => return None, // noise: every push to a PR branch
        ("closed", true) => ("Merged", COLOR_PR_MERGED),
        ("closed", false) => ("Closed", COLOR_PR_CLOSED),
        _ => return None,
    };
    let (repo_id, full_name) = repo_of(p);
    let num = pr.get("number").and_then(|n| n.as_u64()).unwrap_or(0);
    let title = str_field(pr, "title");
    let head = pr
        .get("head")
        .map(|h| str_field(h, "ref"))
        .unwrap_or_default();
    let base = pr
        .get("base")
        .map(|b| str_field(b, "ref"))
        .unwrap_or_default();
    Some(NotifyEvent {
        kind: EVENT_PR,
        repo_id,
        repo_full_name: full_name,
        title: format!("Pull Request [{label}]: #{num} {title}"),
        text: format!(
            "`{head}` → `{base}`\n{}",
            truncate(str_field(pr, "body"), 2000)
        ),
        url: str_field(pr, "html_url"),
        author: sender_of(p),
        color,
    })
}

fn parse_issue(p: &serde_json::Value) -> Option<NotifyEvent> {
    match str_field(p, "action").as_str() {
        "opened" | "closed" | "reopened" | "labeled" => {}
        _ => return None,
    }
    let issue = p.get("issue")?;
    let (repo_id, full_name) = repo_of(p);
    let num = issue.get("number").and_then(|n| n.as_u64()).unwrap_or(0);
    let action = str_field(p, "action");
    let labels: Vec<String> = issue
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|ls| {
            ls.iter()
                .map(|l| str_field(l, "name"))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let tag = if labels.is_empty() {
        String::new()
    } else {
        format!(" [{}]", labels.join(", "))
    };
    Some(NotifyEvent {
        kind: EVENT_ISSUE,
        repo_id,
        repo_full_name: full_name,
        title: format!(
            "Issue [{action}]: #{num} {}{tag}",
            str_field(issue, "title")
        ),
        text: truncate(str_field(issue, "body"), 3000),
        url: str_field(issue, "html_url"),
        author: sender_of(p),
        color: COLOR_ISSUE,
    })
}

fn parse_comment(p: &serde_json::Value) -> Option<NotifyEvent> {
    if str_field(p, "action") != "created" {
        return None;
    }
    let issue = p.get("issue")?;
    let comment = p.get("comment")?;
    let (repo_id, full_name) = repo_of(p);
    let num = issue.get("number").and_then(|n| n.as_u64()).unwrap_or(0);
    Some(NotifyEvent {
        kind: EVENT_ISSUE,
        repo_id,
        repo_full_name: full_name,
        title: format!("Comment on #{}: {}", num, str_field(issue, "title")),
        text: truncate(str_field(comment, "body"), 3000),
        url: str_field(comment, "html_url"),
        author: sender_of(p),
        color: COLOR_ISSUE,
    })
}

/// Truncate on char boundaries (never split multibyte/emoji) + suffix.
pub fn truncate(s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

/// GitHub App client (installation token). Read-only by App configuration.
pub struct GithubApp {
    app_id: u64,
    private_key_pem: Vec<u8>,
    org: String,
}

impl GithubApp {
    pub fn new(app_id: u64, private_key_path: &str, org: String) -> Result<Self> {
        let pem = std::fs::read(private_key_path).map_err(|e| {
            Error::InvalidConfig(format!("cannot read GITHUB_PRIVATE_KEY_PATH: {e}"))
        })?;
        if !pem.starts_with(b"-----BEGIN") {
            return Err(Error::InvalidConfig("private key file is not PEM".into()));
        }
        Ok(Self {
            app_id,
            private_key_pem: pem,
            org,
        })
    }

    async fn installation_octocrab(&self) -> Result<octocrab::Octocrab> {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(&self.private_key_pem)
            .map_err(|e| Error::InvalidConfig(format!("bad private key PEM: {e}")))?;
        let app = octocrab::Octocrab::builder()
            .app(self.app_id.into(), key)
            .build()?;
        let inst = match app.apps().get_org_installation(&self.org).await {
            Ok(i) => i,
            Err(e) if is_not_found(&e) => {
                // Personal-account installation: GET /users/{user}/installation.
                app.get::<octocrab::models::Installation, _, ()>(
                    format!("/users/{}/installation", self.org),
                    None,
                )
                .await
                .map_err(|_| {
                    Error::GithubMsg(format!(
                        "github app is not installed on {} (org or user)",
                        self.org
                    ))
                })?
            }
            Err(e) => return Err(Error::Github(e)),
        };
        Ok(app.installation(inst.id)?)
    }

    /// Validate that `name` exists in the org. Returns (repo_id, full_name).
    pub async fn repo_exists(&self, name: &str) -> Result<Option<(i64, String)>> {
        let crab = self.installation_octocrab().await?;
        match crab.repos(&self.org, name).get().await {
            Ok(r) => Ok(Some((
                r.id.0 as i64,
                r.full_name
                    .unwrap_or_else(|| format!("{}/{}", self.org, name)),
            ))),
            Err(octocrab::Error::GitHub { source, .. }) if source.message.contains("Not Found") => {
                Ok(None)
            }
            Err(e) => Err(Error::Github(e)),
        }
    }

    /// All account repos — org or personal user — for `-r all` cache.
    pub async fn list_org_repos(&self) -> Result<Vec<(i64, String)>> {
        let crab = self.installation_octocrab().await?;
        let mut out = vec![];
        // Org listing first; user listing for personal-account installations.
        let mut page = match crab.orgs(&self.org).list_repos().per_page(100).send().await {
            Ok(p) => p,
            Err(e) if is_not_found(&e) => {
                crab.users(&self.org).repos().per_page(100).send().await?
            }
            Err(e) => return Err(Error::Github(e)),
        };
        loop {
            out.extend(page.items.into_iter().map(|r| {
                (
                    r.id.0 as i64,
                    r.full_name
                        .unwrap_or_else(|| format!("{}/{}", self.org, r.name)),
                )
            }));
            match crab
                .get_page::<octocrab::models::Repository>(&page.next)
                .await?
            {
                Some(next) => page = next,
                None => break,
            }
        }
        Ok(out)
    }
}

fn is_not_found(e: &octocrab::Error) -> bool {
    matches!(e, octocrab::Error::GitHub { source, .. } if source.message.contains("Not Found"))
}
