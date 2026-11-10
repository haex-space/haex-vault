use super::*;

fn config() -> ServerConfig {
    ServerConfig {
        nostr_relays: vec!["wss://relay.example.org".to_string()],
        iroh_relays: vec!["https://iroh.example.org".to_string()],
        disabled: vec![
            "wss://nos.lol".to_string(),
            "https://iroh.example.org".to_string(),
        ],
    }
}

fn link(query: &str) -> String {
    format!("haex-vault://link?{query}")
}

/// A valid code: the two bits left over by 26 characters are zero, so the last one is A, E, I, M, Q, U, Y or 4.
const CODE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYA";

#[test]
fn a_built_text_reads_back_with_everything_it_carried() {
    let code = LinkCode::generate();

    let text = build(&code, "Privat", &config());
    let parsed = parse(&text).expect("parses");

    assert_eq!(parsed.code.display(), code.display());
    assert_eq!(parsed.vault_name.as_deref(), Some("Privat"));
    assert_eq!(parsed.servers, Some(config()));
    assert_eq!(parsed.dropped_servers, 0);
}

#[test]
fn a_text_without_servers_or_name_reads_as_none() {
    let code = LinkCode::generate();

    let parsed = parse(&build(&code, "", &ServerConfig::default())).expect("parses");

    assert_eq!(parsed.vault_name, None);
    assert_eq!(parsed.servers, None);
}

#[test]
fn a_name_with_spaces_and_umlauts_survives() {
    let text = build(
        &LinkCode::generate(),
        "Mein Tresor für Ärger",
        &ServerConfig::default(),
    );

    assert_eq!(
        parse(&text).expect("parses").vault_name.as_deref(),
        Some("Mein Tresor für Ärger")
    );
}

#[test]
fn a_plain_code_reads_as_a_text_without_servers() {
    let code = LinkCode::generate();
    for typed in [
        code.display(),
        code.display().replace('-', " ").to_lowercase(),
        format!("  {}\n", code.display().replace('-', "")),
    ] {
        let parsed = parse(&typed).expect("parses");

        assert_eq!(parsed.code.display(), code.display());
        assert_eq!(parsed.servers, None);
        assert_eq!(parsed.vault_name, None);
    }
}

#[test]
fn a_later_version_is_refused_as_such() {
    assert_eq!(
        parse(&link(&format!("v=2&c={CODE}"))).err(),
        Some(LinkTextError::UnsupportedVersion)
    );
    assert_eq!(
        parse(&link(&format!("v=0&c={CODE}"))).err(),
        Some(LinkTextError::UnsupportedVersion)
    );
    // Whatever else a later version changed, the version is looked at first.
    assert_eq!(
        parse(&link("v=7&c=not-a-code")).err(),
        Some(LinkTextError::UnsupportedVersion)
    );
}

#[test]
fn a_missing_or_broken_version_is_no_link_code() {
    for query in [
        format!("c={CODE}"),
        format!("v=x&c={CODE}"),
        format!("v=1&v=1&c={CODE}"),
    ] {
        assert_eq!(
            parse(&link(&query)).err(),
            Some(LinkTextError::NotALinkCode)
        );
    }
}

#[test]
fn a_missing_doubled_or_bad_code_is_no_link_code() {
    for query in [
        "v=1".to_string(),
        format!("v=1&c={CODE}&c={CODE}"),
        "v=1&c=ABCD".to_string(),
        format!("v=1&c={}1", &CODE[..25]),
        // 26 characters of the alphabet, but the two bits left over are not zero.
        format!("v=1&c={}2", &CODE[..25]),
    ] {
        assert_eq!(
            parse(&link(&query)).err(),
            Some(LinkTextError::NotALinkCode)
        );
    }
}

#[test]
fn other_texts_are_no_link_code() {
    for text in [
        "",
        "   ",
        "hello",
        "https://example.org/?c=ABCDEFGHIJKLMNOPQRSTUVWXYA",
        "haex-vault://contact/abc",
        "haex-vault://links?v=1",
    ] {
        assert_eq!(
            parse(text).err(),
            Some(LinkTextError::NotALinkCode),
            "{text}"
        );
    }
    let too_long = link(&format!("v=1&c={CODE}&n={}", "a".repeat(49_153)));
    assert_eq!(parse(&too_long).err(), Some(LinkTextError::NotALinkCode));
}

#[test]
fn scheme_and_host_ignore_case_and_unknown_keys_are_ignored() {
    let parsed = parse(&format!("HAEX-Vault://LINK?v=1&c={CODE}&x=1&future=yes")).expect("parses");

    assert_eq!(parsed.code.display().replace('-', ""), CODE);
}

#[test]
fn a_bad_server_is_dropped_and_counted_but_the_code_is_kept() {
    let text = link(&format!(
        "v=1&c={CODE}&nr=https%3A%2F%2Fnot-a-nostr-server&nr=wss%3A%2F%2Fok.example.org&ir=ftp%3A%2F%2Fx"
    ));

    let parsed = parse(&text).expect("parses");

    assert_eq!(parsed.dropped_servers, 2);
    assert_eq!(
        parsed.servers.expect("one server is left").nostr_relays,
        vec!["wss://ok.example.org".to_string()]
    );
}

#[test]
fn servers_beyond_the_limits_are_counted_not_kept() {
    let mut query = format!("v=1&c={CODE}");
    for n in 0..11 {
        query.push_str(&format!("&nr=wss%3A%2F%2Fr{n}.example.org"));
    }

    let parsed = parse(&link(&query)).expect("parses");

    assert_eq!(parsed.dropped_servers, 1);
    assert_eq!(parsed.servers.expect("servers").nostr_relays.len(), 10);
}

#[test]
fn a_name_over_the_limit_is_not_taken_over() {
    let long = "x".repeat(MAX_NAME_CHARS + 1);

    let parsed = parse(&link(&format!("v=1&c={CODE}&n={long}"))).expect("parses");

    assert_eq!(parsed.vault_name, None);
}

#[test]
fn building_cuts_a_long_name_at_a_character_boundary() {
    let name = format!("{}ä", "a".repeat(MAX_NAME_CHARS - 1)) + "überlang";

    let parsed = parse(&build(
        &LinkCode::generate(),
        &name,
        &ServerConfig::default(),
    ))
    .expect("parses");

    let kept = parsed.vault_name.expect("a name");
    assert_eq!(kept.chars().count(), MAX_NAME_CHARS);
    assert!(kept.ends_with('ä'));
}

#[test]
fn the_text_carries_only_code_name_and_servers() {
    let text = build(&LinkCode::generate(), "Privat", &config());
    let url = Url::parse(&text).expect("a url");

    let mut keys: Vec<String> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
    keys.sort();
    keys.dedup();

    assert_eq!(keys, ["c", "ir", "n", "nr", "off", "v"]);
}

#[test]
fn the_debug_form_does_not_show_the_code() {
    let parsed = parse(&link(&format!("v=1&c={CODE}"))).expect("parses");

    assert!(!format!("{parsed:?}").contains(CODE));
}

#[test]
fn only_a_short_text_is_shown_as_a_qr_code() {
    let usual = build(&LinkCode::generate(), "Privat", &config());
    let mut many = ServerConfig::default();
    for n in 0..10 {
        many.nostr_relays
            .push(format!("wss://{}{n}.example.org", "r".repeat(200)));
        many.iroh_relays
            .push(format!("https://{}{n}.example.org", "i".repeat(200)));
    }
    let huge = build(&LinkCode::generate(), "Privat", &many);

    assert!(fits_qr(&usual));
    assert!(!fits_qr(&huge));
    // Nothing was shortened to make it fit: all twenty servers are in the text.
    assert_eq!(parse(&huge).expect("parses").dropped_servers, 0);
}

#[test]
fn building_twice_gives_the_same_text() {
    let code = LinkCode::generate();

    assert_eq!(
        build(&code, "Privat", &config()),
        build(&code, "Privat", &config())
    );
}

#[test]
fn a_text_up_to_the_limit_gets_a_qr_code_and_a_longer_one_does_not() {
    let at_limit = "a".repeat(QR_MAX_TEXT_BYTES);
    let over = "a".repeat(QR_MAX_TEXT_BYTES + 1);

    let svg = qr_svg(&at_limit).expect("a QR code");

    assert!(svg.contains("<svg") && svg.contains("</svg>"));
    assert_eq!(qr_svg(&over), None);
}

#[test]
fn the_usual_text_gets_a_qr_code() {
    let text = build(&LinkCode::generate(), "Privat", &config());

    assert!(qr_svg(&text).is_some());
}
