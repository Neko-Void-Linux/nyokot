use nyokot::github::{parse_event, truncate};

#[test]
fn push_formats_commits_with_links() {
    let p = serde_json::json!({
        "ref": "refs/heads/main",
        "compare": "https://github.com/o/r/compare/a...b",
        "repository": {"id": 1, "full_name": "Neko-Void-Linux/kasha"},
        "sender": {"login": "dev"},
        "commits": [
            {"id": "a1b2c3d4e5", "url": "https://x/1", "message": "fix thing\nlong body"},
            {"id": "f6e5d4c3b2", "url": "https://x/2", "message": "add stuff"}
        ]
    });
    let ev = parse_event("push", &p).unwrap();
    assert_eq!(ev.kind, "co");
    assert_eq!(ev.title, "[Neko-Void-Linux/kasha:main] 2 new commit(s)");
    assert!(ev.text.contains("[a1b2c3d](https://x/1) fix thing"));
    assert!(ev.text.contains("[f6e5d4c](https://x/2) add stuff"));
}

#[test]
fn push_without_commits_ignored() {
    let p = serde_json::json!({
        "ref": "refs/heads/main",
        "repository": {"id": 1, "full_name": "o/r"},
        "commits": []
    });
    assert!(parse_event("push", &p).is_none());
}

#[test]
fn pr_open_merge_close() {
    let base = |action: &str, merged: bool| {
        serde_json::json!({
            "action": action,
            "repository": {"id": 2, "full_name": "o/r"},
            "sender": {"login": "dev"},
            "pull_request": {
                "number": 7, "title": "Cool", "body": "desc",
                "html_url": "https://x/pr/7", "merged": merged,
                "head": {"ref": "feat"}, "base": {"ref": "main"}
            }
        })
    };
    let e = parse_event("pull_request", &base("opened", false)).unwrap();
    assert!(e.title.contains("[Open]"), "{}", e.title);
    assert_eq!(e.color, nyokot::github::COLOR_PR_OPEN);
    assert!(e.text.contains("`feat` → `main`"));
    let e = parse_event("pull_request", &base("closed", true)).unwrap();
    assert!(e.title.contains("[Merged]"), "{}", e.title);
    assert_eq!(e.color, nyokot::github::COLOR_PR_MERGED);
    let e = parse_event("pull_request", &base("closed", false)).unwrap();
    assert_eq!(e.color, nyokot::github::COLOR_PR_CLOSED);
    // synchronize = noise, never notifies
    assert!(parse_event("pull_request", &base("synchronize", false)).is_none());
}

#[test]
fn issues_and_comments() {
    let issue = serde_json::json!({
        "action": "opened",
        "repository": {"id": 3, "full_name": "o/r"},
        "sender": {"login": "user"},
        "issue": {"number": 9, "title": "Bug", "body": "boom",
                  "html_url": "https://x/i/9",
                  "labels": [{"name": "bug"}, {"name": "urgent"}]}
    });
    let e = parse_event("issues", &issue).unwrap();
    assert_eq!(e.kind, "is");
    assert!(
        e.title.contains("[bug, urgent]"),
        "labels must be visible: {}",
        e.title
    );
    let comment = serde_json::json!({
        "action": "created",
        "repository": {"id": 3, "full_name": "o/r"},
        "sender": {"login": "user"},
        "issue": {"number": 9, "title": "Bug"},
        "comment": {"body": "any update?", "html_url": "https://x/c/1"}
    });
    let e = parse_event("issue_comment", &comment).unwrap();
    assert_eq!(e.kind, "is");
    assert!(e.title.contains("#9"));
}

#[test]
fn unknown_events_ignored() {
    assert!(parse_event("star", &serde_json::json!({})).is_none());
    assert!(parse_event("issues", &serde_json::json!({"action": "assigned"})).is_none());
}

#[test]
fn truncate_respects_multibyte() {
    let s = "a".repeat(100) + &"🦀".repeat(50); // crab = 4 bytes each
    let t = truncate(s, 110);
    assert!(t.ends_with("..."));
    assert!(t.len() <= 114);
    assert!(std::str::from_utf8(t.as_bytes()).is_ok());
}
