//! Shared command grammar — identical on Discord, Stoat and Fluxer.
//!
//! `<prefix>(help|connect|disconnect|pause|resume)`
//! `connect -r <repo...|all> -n <pr|is|co|all...>`
//! Prefixes come from `NYOKOT_PREFIXES` (CSV); all work on all platforms.

use crate::umf::EVENT_KINDS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectArgs {
    /// `true` when `-r all` was given (catch-all: no repo_filters rows).
    pub all_repos: bool,
    pub repos: Vec<String>,
    /// `true` when `-n all` was given.
    pub all_kinds: bool,
    pub kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Connect(ConnectArgs),
    Disconnect,
    Pause,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnknownCommand(String),
    MissingRepos,
    MissingKinds,
    UnknownKind(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCommand(c) => write!(f, "unknown command `{c}`. Try `help`."),
            Self::MissingRepos => write!(f, "missing `-r <repos...|all>`."),
            Self::MissingKinds => write!(f, "missing `-n <pr|is|co|all...>`."),
            Self::UnknownKind(k) => {
                write!(f, "unknown notification `{k}` (use pr, is, co or all).")
            }
        }
    }
}

/// Strip any configured prefix; returns the remainder if prefixed.
pub fn strip_prefix<'a>(prefixes: &[String], text: &'a str) -> Option<&'a str> {
    let t = text.trim();
    prefixes
        .iter()
        .find_map(|p| t.strip_prefix(p.as_str()).map(str::trim))
}

pub fn parse(prefixes: &[String], text: &str) -> Option<std::result::Result<Command, ParseError>> {
    let rest = strip_prefix(prefixes, text)?;
    let mut words: Vec<&str> = rest.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    let cmd = words.remove(0).to_lowercase();
    Some(match cmd.as_str() {
        "help" => Ok(Command::Help),
        "disconnect" => Ok(Command::Disconnect),
        "pause" => Ok(Command::Pause),
        "resume" => Ok(Command::Resume),
        "connect" => parse_connect(&words),
        other => Err(ParseError::UnknownCommand(other.to_string())),
    })
}

fn parse_connect(words: &[&str]) -> std::result::Result<Command, ParseError> {
    let mut repos: Vec<String> = vec![];
    let mut kinds: Vec<String> = vec![];
    let mut cur: Option<&str> = None;
    for w in words {
        match *w {
            "-r" => cur = Some("r"),
            "-n" => cur = Some("n"),
            v => match cur {
                Some("r") => repos.push(v.to_string()),
                Some("n") => kinds.push(v.to_lowercase()),
                _ => return Err(ParseError::MissingRepos),
            },
        }
    }
    if repos.is_empty() {
        return Err(ParseError::MissingRepos);
    }
    if kinds.is_empty() {
        return Err(ParseError::MissingKinds);
    }
    let all_repos = repos.iter().any(|r| r.eq_ignore_ascii_case("all"));
    let all_kinds = kinds.iter().any(|k| k == "all");
    for k in &kinds {
        if k != "all" && !EVENT_KINDS.contains(&k.as_str()) {
            return Err(ParseError::UnknownKind(k.clone()));
        }
    }
    Ok(Command::Connect(ConnectArgs {
        all_repos,
        repos: if all_repos { vec![] } else { repos },
        all_kinds,
        kinds: if all_kinds {
            EVENT_KINDS.iter().map(|s| s.to_string()).collect()
        } else {
            kinds
        },
    }))
}

pub fn help_text(prefix: &str) -> String {
    format!(
        "**Ñyokot** — GitHub org notifier.\n\
         `{p}connect -r <repos...|all> -n <pr|is|co|all...>` — link THIS channel (admins only).\n\
         `  -r` repos of the org, e.g. `-r kasha cnr nk-web`; `-r all` = every repo.\n\
         `  -n` what to notify: `pr` pull requests, `is` issues, `co` pushes/merges; `-n all` = everything.\n\
         `{p}disconnect` — unlink THIS channel.\n\
         `{p}pause` — pause notifications in THIS channel. `{p}resume` — resume.\n\
         `{p}help` — this message.",
        p = prefix
    )
}
