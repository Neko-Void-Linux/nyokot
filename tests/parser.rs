use nyokot::commands::{parse, Command, ConnectArgs, ParseError};

fn px() -> Vec<String> {
    vec!["/".to_string(), "!".to_string(), "?".to_string()]
}

fn ok(text: &str) -> Command {
    parse(&px(), text)
        .expect("must be a command")
        .expect("must parse")
}

#[test]
fn all_prefixes_work() {
    assert_eq!(parse(&px(), "/help").unwrap().unwrap(), Command::Help);
    assert_eq!(parse(&px(), "!help").unwrap().unwrap(), Command::Help);
    assert_eq!(parse(&px(), "?help").unwrap().unwrap(), Command::Help);
    assert_eq!(
        parse(&px(), "/disconnect").unwrap().unwrap(),
        Command::Disconnect
    );
    assert_eq!(parse(&px(), "!pause").unwrap().unwrap(), Command::Pause);
    assert_eq!(parse(&px(), "?resume").unwrap().unwrap(), Command::Resume);
}

#[test]
fn non_commands_ignored() {
    assert!(parse(&px(), "hello world").is_none());
    assert!(parse(&px(), "").is_none());
    assert!(parse(&px(), "/").is_none());
}

#[test]
fn connect_full_example() {
    // Mirrors the approved spec example: /connect -r <repos> -n pr is co
    assert_eq!(
        ok("/connect -r kasha cnr nk-web -n pr is co"),
        Command::Connect(ConnectArgs {
            all_repos: false,
            repos: vec!["kasha".into(), "cnr".into(), "nk-web".into()],
            all_kinds: false,
            kinds: vec!["pr".into(), "is".into(), "co".into()],
        })
    );
}

#[test]
fn connect_all_expands() {
    assert_eq!(
        ok("/connect -r all -n all"),
        Command::Connect(ConnectArgs {
            all_repos: true,
            repos: vec![],
            all_kinds: true,
            kinds: vec!["pr".into(), "is".into(), "co".into()],
        })
    );
}

#[test]
fn connect_errors() {
    let e = parse(&px(), "/connect -n pr").unwrap().unwrap_err();
    assert_eq!(e, ParseError::MissingRepos);
    let e = parse(&px(), "/connect -r kasha").unwrap().unwrap_err();
    assert_eq!(e, ParseError::MissingKinds);
    let e = parse(&px(), "/connect -r kasha -n foo")
        .unwrap()
        .unwrap_err();
    assert_eq!(e, ParseError::UnknownKind("foo".into()));
    let e = parse(&px(), "/frobnicate").unwrap().unwrap_err();
    assert_eq!(e, ParseError::UnknownCommand("frobnicate".into()));
    // Bare word before any flag is rejected (flags are mandatory).
    let e = parse(&px(), "/connect kasha -n pr").unwrap().unwrap_err();
    assert_eq!(e, ParseError::MissingRepos);
}
