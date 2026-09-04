//! End-to-end command + routing flow against a temp SQLite file,
//! driven through the shared `handle_text` entry point with a fake GitHub.

use nyokot::command_exec::{handle_text, RepoSource, Target};
use nyokot::db::{self, Db};
use nyokot::error::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Fake {
    repos: HashMap<String, (i64, String)>,
}

impl RepoSource for Fake {
    async fn repo_exists(&self, name: &str) -> Result<Option<(i64, String)>> {
        Ok(self.repos.get(name).cloned())
    }
}

fn fake() -> Fake {
    Fake {
        repos: [
            ("kasha", (1, "Neko-Void-Linux/kasha")),
            ("cnr", (2, "Neko-Void-Linux/cnr")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), (v.0, v.1.to_string())))
        .collect(),
    }
}

static CTR: AtomicU64 = AtomicU64::new(0);

async fn test_db() -> (Db, std::path::PathBuf) {
    let p = std::env::temp_dir().join(format!(
        "nyokot-test-{}-{}.db",
        std::process::id(),
        CTR.fetch_add(1, Ordering::Relaxed)
    ));
    let db = db::connect(&format!("sqlite:{}?mode=rwc", p.display()))
        .await
        .unwrap();
    (db, p)
}

fn px() -> Vec<String> {
    vec!["/".to_string()]
}

fn target<'a>(platform: &'a str, channel: &'a str) -> Target<'a> {
    Target {
        platform,
        guild_id: "g1",
        channel_id: channel,
    }
}

#[tokio::test]
async fn connect_pause_resume_disconnect_per_channel() {
    let (db, path) = test_db().await;
    let gh = fake();

    // connect channel A (discord) for pr on kasha
    let r = handle_text(
        &db,
        &gh,
        &px(),
        &target("discord", "A"),
        true,
        "/connect -r kasha -n pr",
    )
    .await
    .unwrap();
    assert!(r.contains("Linked"), "{r}");

    // routes pr/kasha -> A, but not issues, not other repos
    assert_eq!(
        db::resolve_channels(&db, "pr", 1).await.unwrap(),
        vec![("A".to_string(), "discord".to_string())]
    );
    assert!(db::resolve_channels(&db, "is", 1).await.unwrap().is_empty());
    assert!(db::resolve_channels(&db, "pr", 2).await.unwrap().is_empty());

    // pause is per-channel: A stops receiving
    let r = handle_text(&db, &gh, &px(), &target("discord", "A"), true, "/pause")
        .await
        .unwrap();
    assert!(r.contains("paused"), "{r}");
    assert!(db::resolve_channels(&db, "pr", 1).await.unwrap().is_empty());

    // resume restores
    let r = handle_text(&db, &gh, &px(), &target("discord", "A"), true, "/resume")
        .await
        .unwrap();
    assert!(r.contains("resumed"), "{r}");
    assert_eq!(db::resolve_channels(&db, "pr", 1).await.unwrap().len(), 1);

    // disconnect wipes the channel only
    let r = handle_text(
        &db,
        &gh,
        &px(),
        &target("discord", "A"),
        true,
        "/disconnect",
    )
    .await
    .unwrap();
    assert!(r.contains("unlinked"), "{r}");
    assert!(db::resolve_channels(&db, "pr", 1).await.unwrap().is_empty());

    // pause/resume on an unlinked channel say so
    let r = handle_text(&db, &gh, &px(), &target("discord", "A"), true, "/pause")
        .await
        .unwrap();
    assert!(r.contains("not linked"), "{r}");

    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn connect_all_is_catch_all_and_unknown_repos_save_nothing() {
    let (db, path) = test_db().await;
    let gh = fake();

    // unknown repo -> error, nothing saved
    let r = handle_text(
        &db,
        &gh,
        &px(),
        &target("stoat", "B"),
        true,
        "/connect -r nope -n all",
    )
    .await
    .unwrap();
    assert!(r.contains("Unknown repos"), "{r}");
    assert!(db::channel_state(&db, "stoat", "B")
        .await
        .unwrap()
        .is_none());

    // -r all -> catch-all across kinds and repos
    let r = handle_text(
        &db,
        &gh,
        &px(),
        &target("stoat", "B"),
        true,
        "/connect -r all -n all",
    )
    .await
    .unwrap();
    assert!(r.contains("Linked"), "{r}");
    for kind in ["pr", "is", "co"] {
        for repo in [1, 2, 999] {
            assert_eq!(
                db::resolve_channels(&db, kind, repo).await.unwrap(),
                vec![("B".to_string(), "stoat".to_string())],
                "kind={kind} repo={repo}"
            );
        }
    }

    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn non_admin_cannot_connect_but_can_read_help() {
    let (db, path) = test_db().await;
    let gh = fake();

    let r = handle_text(
        &db,
        &gh,
        &px(),
        &target("fluxer", "C"),
        false,
        "/connect -r kasha -n pr",
    )
    .await
    .unwrap();
    assert!(r.contains("admin"), "{r}");
    assert!(db::channel_state(&db, "fluxer", "C")
        .await
        .unwrap()
        .is_none());

    let r = handle_text(&db, &gh, &px(), &target("fluxer", "C"), false, "/help")
        .await
        .unwrap();
    assert!(r.contains("connect"), "{r}");

    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn shim_ingress_entry_points_share_grammar() {
    // Stoat/Fluxer gateways will call exactly this once their APIs land.
    let (db, path) = test_db().await;
    let gh = fake();
    let stoat = nyokot::adapters::StoatAdapter::new(
        None,
        "http://x".into(),
        px(),
        db.clone(),
        Duration::from_secs(1),
    );
    let fluxer = nyokot::adapters::FluxerAdapter::new(
        None,
        "http://x".into(),
        px(),
        db.clone(),
        Duration::from_secs(1),
    );
    let r = stoat
        .inbound_text(&gh, "g", "S1", true, "/connect -r cnr -n is co")
        .await
        .unwrap();
    assert!(r.contains("Linked"), "{r}");
    let r = fluxer
        .inbound_text(&gh, "g", "F1", true, "/connect -r all -n pr")
        .await
        .unwrap();
    assert!(r.contains("Linked"), "{r}");
    assert_eq!(db::resolve_channels(&db, "is", 2).await.unwrap().len(), 1);
    assert_eq!(db::resolve_channels(&db, "pr", 999).await.unwrap().len(), 1);

    std::fs::remove_file(path).ok();
}
