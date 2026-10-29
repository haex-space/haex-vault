use super::sites::{Site, SiteError};

#[test]
fn a_website_is_scheme_host_and_a_non_default_port() {
    let cases = [
        (
            "https://Shop.Example.de/kasse?x=1#a",
            "https://shop.example.de",
        ),
        ("https://shop.example.de:443/", "https://shop.example.de"),
        (
            "https://shop.example.de:8443",
            "https://shop.example.de:8443",
        ),
        ("http://intranet.example:80/a", "http://intranet.example"),
        ("https://bücher.example/", "https://xn--bcher-kva.example"),
        ("  https://example.com  ", "https://example.com"),
        ("https://127.0.0.1:8443/x", "https://127.0.0.1:8443"),
    ];
    for (address, site) in cases {
        assert_eq!(
            Site::parse(address).map(|s| s.to_string()),
            Ok(site.to_owned()),
            "{address}"
        );
    }
}

#[test]
fn addresses_that_are_no_website_are_refused() {
    for (address, error) in [
        ("file:///etc/passwd", SiteError::Scheme),
        ("javascript:alert(1)", SiteError::Scheme),
        ("data:text/html,hi", SiteError::Scheme),
        ("chrome://settings", SiteError::Scheme),
        ("https://user:pw@example.com", SiteError::UserInfo),
        ("https://user@example.com", SiteError::UserInfo),
        ("no address", SiteError::Unparsable),
    ] {
        assert_eq!(Site::parse(address), Err(error), "{address}");
    }
}

#[test]
fn a_lookalike_host_is_another_website() {
    let real = Site::parse("https://example.com").expect("site");
    for other in [
        "https://example.com.evil.test",
        "https://login.example.com",
        "http://example.com",
        "https://example.com:8443",
    ] {
        assert_ne!(Site::parse(other).expect("site"), real, "{other}");
    }
}

#[test]
fn a_website_reads_back_from_json_and_refuses_a_bad_one() {
    let site: Site = serde_json::from_str("\"https://EXAMPLE.com/a\"").expect("json");
    assert_eq!(site.as_str(), "https://example.com");
    assert!(serde_json::from_str::<Site>("\"file:///x\"").is_err());
    assert!(Site::parse("http://example.com")
        .expect("site")
        .is_plain_http());
}
