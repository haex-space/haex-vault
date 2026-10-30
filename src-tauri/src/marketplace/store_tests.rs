//! The pure rules of the store (T012): addresses, the form's checks, the preset's address.

use super::*;

fn input(url: &str) -> MarketplaceInput {
    MarketplaceInput {
        id: None,
        name: "Team".to_owned(),
        base_url: url.to_owned(),
        auth_type: MarketplaceAuthType::None,
        credentials_item_id: None,
        new_credentials: None,
    }
}

fn reason(error: HolziError) -> String {
    match error {
        HolziError::Marketplace { reason } => reason,
        other => panic!("a marketplace error, got {other:?}"),
    }
}

#[test]
fn addresses_are_normalized() {
    for (given, kept) in [
        ("https://example.org", "https://example.org"),
        ("https://example.org/", "https://example.org"),
        (
            "  https://example.org/market/  ",
            "https://example.org/market",
        ),
        (
            "HTTP://Example.ORG:8080/a/b/",
            "http://example.org:8080/a/b",
        ),
        (
            "https://example.org/market?x=1#frag",
            "https://example.org/market",
        ),
        ("https://example.org:443", "https://example.org"),
    ] {
        assert_eq!(normalize_base_url(given).expect(given), kept, "{given}");
    }
}

#[test]
fn addresses_that_are_not_a_marketplace_are_refused() {
    for bad in [
        "",
        "example.org",
        "ftp://example.org",
        "file:///etc/passwd",
        "https://",
        "https://user:secret@example.org",
        "javascript:alert(1)",
        "//example.org",
    ] {
        let error = normalize_base_url(bad).expect_err(bad);
        assert_eq!(reason(error), kinds::INVALID_URL, "{bad}");
    }
}

#[test]
fn a_path_prefix_survives_for_the_protocol_paths_to_hang_behind() {
    assert_eq!(
        normalize_base_url("https://example.org/a/market").expect("ok"),
        "https://example.org/a/market"
    );
}

#[test]
fn the_form_needs_a_name() {
    let mut blank = input("https://example.org");
    blank.name = "   ".to_owned();
    assert!(matches!(
        check(&blank),
        Err(HolziError::InvalidInput { .. })
    ));
}

#[test]
fn a_login_needs_an_entry_and_https() {
    let mut bearer = input("https://example.org");
    bearer.auth_type = MarketplaceAuthType::Bearer;
    assert_eq!(
        reason(check(&bearer).expect_err("no entry")),
        kinds::CREDENTIALS_REQUIRED
    );
    bearer.credentials_item_id = Some("  ".to_owned());
    assert_eq!(
        reason(check(&bearer).expect_err("blank entry")),
        kinds::CREDENTIALS_REQUIRED
    );
    bearer.credentials_item_id = Some("item".to_owned());
    bearer.base_url = "http://example.org".to_owned();
    assert_eq!(
        reason(check(&bearer).expect_err("http")),
        kinds::INSECURE_CREDENTIALS
    );
    bearer.auth_type = MarketplaceAuthType::Basic;
    bearer.base_url = "https://example.org".to_owned();
    let checked = check(&bearer).expect("basic over https");
    assert_eq!(checked.credentials_item_id.as_deref(), Some("item"));
}

#[test]
fn a_marketplace_without_login_has_no_entry() {
    let mut plain = input("http://192.168.1.5");
    plain.credentials_item_id = Some("item".to_owned());
    let checked = check(&plain).expect("http without a login is fine");
    assert_eq!(checked.credentials_item_id, None);
}

#[test]
fn the_preset_address_follows_the_variable_only_in_a_debug_build() {
    let local = Some("http://127.0.0.1:9000".to_owned());
    assert_eq!(preset_url_for(true, local.clone()), "http://127.0.0.1:9000");
    assert_eq!(
        preset_url_for(false, local),
        PRESET_URL,
        "a release build ignores it"
    );
    assert_eq!(preset_url_for(true, None), PRESET_URL);
    assert_eq!(preset_url_for(true, Some("  ".to_owned())), PRESET_URL);
}

#[test]
fn stored_auth_types_read_back_and_an_unknown_one_reads_as_none() {
    for kind in [
        MarketplaceAuthType::None,
        MarketplaceAuthType::Bearer,
        MarketplaceAuthType::Basic,
    ] {
        assert_eq!(MarketplaceAuthType::from_stored(kind.as_str()), kind);
    }
    assert_eq!(
        MarketplaceAuthType::from_stored("did"),
        MarketplaceAuthType::None
    );
}

#[test]
fn a_login_goes_over_https_and_in_a_debug_build_to_loopback_only() {
    for (debug, url, ok) in [
        (false, "https://market.example", true),
        (true, "https://market.example", true),
        (false, "http://market.example", false),
        (true, "http://market.example", false),
        (true, "http://192.168.1.5:3000", false),
        (true, "http://127.0.0.1:8080/team", true),
        (true, "http://localhost:8080", true),
        (true, "http://[::1]:8080", true),
        (false, "http://127.0.0.1:8080", false),
        (true, "ftp://127.0.0.1", false),
        (true, "not a url", false),
    ] {
        assert_eq!(login_transport_ok_for(debug, url), ok, "{debug} {url}");
    }
}
