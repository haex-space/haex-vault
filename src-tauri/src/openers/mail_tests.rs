use super::*;

fn ok(link: &str) -> MailRequest {
    parse(link).unwrap_or_else(|e| panic!("{link:?} should parse, got {e:?}"))
}

fn rejected(link: &str) -> HolziError {
    parse(link).expect_err(&format!("{link:?} should be refused"))
}

fn is_invalid(error: &HolziError, expected: &str) -> bool {
    matches!(error, HolziError::OpenerLinkInvalid { reason } if reason == expected)
}

fn addresses(count: usize) -> String {
    (0..count)
        .map(|n| format!("u{n}@example.org"))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn one_recipient() {
    let request = ok("mailto:ben@example.org");
    assert_eq!(request.to, ["ben@example.org"]);
    assert!(request.cc.is_empty() && request.bcc.is_empty());
    assert_eq!((request.subject.as_str(), request.body.as_str()), ("", ""));
}

#[test]
fn several_recipients_from_the_path_and_from_to() {
    let request =
        ok("mailto:a@example.org,b@example.org?to=c@example.org,d@example.org&to=e@example.org");
    assert_eq!(
        request.to,
        [
            "a@example.org",
            "b@example.org",
            "c@example.org",
            "d@example.org",
            "e@example.org"
        ]
    );
}

#[test]
fn copy_and_blind_copy() {
    let request = ok("mailto:a@example.org?cc=b@example.org&bcc=c@example.org,d@example.org");
    assert_eq!(request.cc, ["b@example.org"]);
    assert_eq!(request.bcc, ["c@example.org", "d@example.org"]);
}

#[test]
fn subject_and_body_are_percent_decoded() {
    let request = ok("mailto:a@example.org?subject=Hello%20World%21&body=Line%20one%0ALine%20two");
    assert_eq!(request.subject, "Hello World!");
    assert_eq!(request.body, "Line one\nLine two");
}

#[test]
fn a_plus_stays_a_plus() {
    assert_eq!(ok("mailto:a@example.org?subject=a+b").subject, "a+b");
}

#[test]
fn non_ascii_is_read_as_utf8() {
    let request = ok("mailto:a@example.org?subject=Gr%C3%BC%C3%9Fe%20%E2%82%AC");
    assert_eq!(request.subject, "Grüße €");
}

#[test]
fn a_link_without_recipient_is_valid() {
    let request = ok("mailto:?subject=x");
    assert!(request.to.is_empty());
    assert_eq!(request.subject, "x");
}

#[test]
fn the_scheme_ignores_case() {
    assert_eq!(ok("MAILTO:a@example.org").to, ["a@example.org"]);
    assert_eq!(ok("MailTo:a@example.org").to, ["a@example.org"]);
}

#[test]
fn field_names_ignore_case() {
    let request = ok("mailto:a@example.org?Subject=S&BODY=B&CC=b@example.org");
    assert_eq!(
        (request.subject.as_str(), request.body.as_str()),
        ("S", "B")
    );
    assert_eq!(request.cc, ["b@example.org"]);
}

#[test]
fn unknown_fields_are_dropped() {
    let request = ok("mailto:a@example.org?in-reply-to=%3Cid%40x%3E&x-custom=1&subject=S&from=evil%40example.org");
    assert_eq!(request.subject, "S");
    assert_eq!(request.to, ["a@example.org"]);
}

#[test]
fn the_first_subject_and_body_count() {
    let request = ok("mailto:a@example.org?subject=first&subject=second&body=one&body=two");
    assert_eq!(
        (request.subject.as_str(), request.body.as_str()),
        ("first", "one")
    );
}

#[test]
fn empty_entries_are_skipped() {
    let request = ok("mailto:a@example.org,?to=&cc=,b@example.org&subject");
    assert_eq!(request.to, ["a@example.org"]);
    assert_eq!(request.cc, ["b@example.org"]);
}

#[test]
fn escaped_addresses_are_decoded() {
    assert_eq!(ok("mailto:ben%40example.org").to, ["ben@example.org"]);
}

#[test]
fn line_breaks_in_the_body_become_newlines() {
    let request = ok("mailto:a@example.org?body=a%0D%0Ab%0D%0A%0D%0Ac");
    assert_eq!(request.body, "a\nb\n\nc");
}

#[test]
fn line_breaks_in_headers_are_refused() {
    for link in [
        "mailto:a@example.org?subject=x%0d%0aBcc:%20evil@example.org",
        "mailto:a@example.org?subject=x%0aBcc:%20evil@example.org",
        "mailto:a@example.org?subject=x%0D%0A",
        "mailto:a@example.org%0d%0aBcc:b@example.org",
        "mailto:a@example.org?to=b@example.org%0a",
        "mailto:a@example.org?cc=b@example.org%0d",
        "mailto:a@example.org?bcc=b@example.org%0A",
    ] {
        assert!(is_invalid(&rejected(link), "line_break"), "{link}");
    }
}

#[test]
fn a_broken_escape_is_refused() {
    for link in [
        "mailto:a@example.org?subject=%zz",
        "mailto:a@example.org?subject=100%",
        "mailto:a@example.org?subject=%4",
        "mailto:?subject=OK&subject=%zz",
        "mailto:?x=%zz",
        "mailto:a%2@example.org",
        "mailto:a@example.org?body=%g1",
    ] {
        assert!(is_invalid(&rejected(link), "escape"), "{link}");
    }
}

#[test]
fn invalid_utf8_is_refused() {
    assert!(is_invalid(
        &rejected("mailto:a@example.org?subject=%FF%FE"),
        "utf8"
    ));
    assert!(is_invalid(
        &rejected("mailto:a@example.org?subject=%C3"),
        "utf8"
    ));
}

#[test]
fn a_bad_address_is_refused() {
    for link in [
        "mailto:nobody",
        "mailto:@example.org",
        "mailto:ben@",
        "mailto:a@b@example.org",
        "mailto:ben%20k@example.org",
        "mailto:a@example.org?to=has%09tab@example.org",
        "mailto:a@example.org?cc=no-at-sign",
    ] {
        assert!(is_invalid(&rejected(link), "address"), "{link}");
    }
}

#[test]
fn another_scheme_is_refused() {
    for link in [
        "",
        "mail:a@example.org",
        "https://example.org",
        "a@example.org",
        "mailt",
    ] {
        assert!(is_invalid(&rejected(link), "scheme"), "{link:?}");
    }
}

#[test]
fn a_multibyte_start_does_not_panic() {
    assert!(is_invalid(&rejected("mäilto:a@example.org"), "scheme"));
}

#[test]
fn fifty_recipients_fit_per_field_fifty_one_do_not() {
    assert_eq!(ok(&format!("mailto:{}", addresses(50))).to.len(), 50);
    assert_eq!(ok(&format!("mailto:?cc={}", addresses(50))).cc.len(), 50);
    assert_eq!(ok(&format!("mailto:?bcc={}", addresses(50))).bcc.len(), 50);
    for link in [
        format!("mailto:{}", addresses(51)),
        format!("mailto:?cc={}", addresses(51)),
        format!("mailto:?bcc={}", addresses(51)),
        format!("mailto:{}?to={}", addresses(30), addresses(21)),
    ] {
        assert!(matches!(rejected(&link), HolziError::OpenerLinkTooLarge));
    }
}

#[test]
fn the_subject_limit_is_998_characters() {
    assert_eq!(
        ok(&format!("mailto:?subject={}", "x".repeat(998)))
            .subject
            .len(),
        998
    );
    assert!(matches!(
        rejected(&format!("mailto:?subject={}", "x".repeat(999))),
        HolziError::OpenerLinkTooLarge
    ));
    // Characters, not bytes: 998 umlauts are 1,996 bytes.
    assert_eq!(
        ok(&format!("mailto:?subject={}", "%C3%BC".repeat(998)))
            .subject
            .chars()
            .count(),
        998
    );
}

#[test]
fn the_body_limit_is_16_kib() {
    assert_eq!(
        ok(&format!("mailto:?body={}", "x".repeat(16 * 1024)))
            .body
            .len(),
        16 * 1024
    );
    assert!(matches!(
        rejected(&format!("mailto:?body={}", "x".repeat(16 * 1024 + 1))),
        HolziError::OpenerLinkTooLarge
    ));
}

#[test]
fn the_link_limit_is_32_kib() {
    let head = "mailto:?body=";
    // Percent-encoded text is 3 bytes per byte of body, so 16 KiB of body fits in 48 KiB of link:
    // the whole link is what is bounded here, not the decoded body.
    let at_limit = format!("{head}{}", "%41".repeat((32 * 1024 - head.len()) / 3));
    assert!(at_limit.len() <= 32 * 1024);
    assert!(parse(&at_limit).is_ok());
    let over = format!("{}{}", "mailto:?subject=", "x".repeat(32 * 1024 - 15));
    assert_eq!(over.len(), 32 * 1024 + 1);
    assert!(matches!(rejected(&over), HolziError::OpenerLinkTooLarge));
}
