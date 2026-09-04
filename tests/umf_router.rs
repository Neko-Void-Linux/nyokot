use nyokot::router::fanout_targets;
use nyokot::umf::{create_envelope, validate_envelope, EnvelopeParams};

fn params(platform: &str, channel: &str) -> EnvelopeParams {
    EnvelopeParams {
        platform: platform.into(),
        channel_id: channel.into(),
        user_id: "u1".into(),
        username: "nick".into(),
        avatar: None,
        kind: "pr".into(),
        title: Some("T".into()),
        text: "hello".into(),
        url: None,
        color: None,
    }
}

#[test]
fn envelope_defaults_mirror_chatry() {
    let e = create_envelope(params("Discord", "chan1")).unwrap();
    // platform lowercased, trace starts with origin
    assert_eq!(e.head.source.platform, "discord");
    assert_eq!(e.head.trace_path, vec!["discord".to_string()]);
    assert!(!e.head.id.is_empty());
    assert!(validate_envelope(&e));
}

#[test]
fn envelope_rejects_empty_platform_or_channel() {
    assert!(create_envelope(params("", "c")).is_err());
    assert!(create_envelope(params("discord", "")).is_err());
}

#[test]
fn fanout_skips_origin_and_visited() {
    // Port of index.ts router semantics.
    assert_eq!(
        fanout_targets("discord", &["discord".into()]),
        vec!["stoat", "fluxer"]
    );
    assert_eq!(
        fanout_targets("github", &["github".into(), "discord".into()]),
        vec!["stoat", "fluxer"]
    );
    assert!(fanout_targets(
        "discord",
        &["discord".into(), "stoat".into(), "fluxer".into()]
    )
    .is_empty());
}
