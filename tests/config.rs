use nyokot::commands::parse;

fn with_prefixes(prefixes: &str) -> Vec<String> {
    // Mirrors Config::from_env parsing (pipe-separated so `,` can be a prefix).
    prefixes
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn pipe_separator_keeps_comma_prefix() {
    let px = with_prefixes("!|¡|.|,");
    assert_eq!(
        px,
        vec![
            "!".to_string(),
            "¡".to_string(),
            ".".to_string(),
            ",".to_string()
        ]
    );
    for p in [&px[0], &px[1], &px[2], &px[3]] {
        let cmd = format!("{p}help");
        assert!(parse(&px, &cmd).unwrap().is_ok(), "{cmd}");
    }
    // Old `/` prefix is gone with this config.
    assert!(parse(&px, "/help").is_none());
}
