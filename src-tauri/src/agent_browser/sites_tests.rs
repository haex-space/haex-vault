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

// ---------------------------------------------------------------------------
// The check before a document loads (spec 065, T024)
// ---------------------------------------------------------------------------

mod documents {
    use std::collections::{BTreeMap, BTreeSet};

    use super::super::sites::{check_document, Site, Verdict};
    use crate::mcp_server::grants::{SiteGrant, SiteLevel};

    fn site(address: &str) -> Site {
        Site::parse(address).unwrap()
    }

    fn rights() -> BTreeMap<Site, SiteGrant> {
        [
            (
                site("https://news.example"),
                SiteGrant {
                    level: SiteLevel::Read,
                    fill_remembered: false,
                },
            ),
            (
                site("http://intranet.example"),
                SiteGrant {
                    level: SiteLevel::Read,
                    fill_remembered: false,
                },
            ),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn an_allowed_website_loads_as_page_redirect_or_frame() {
        let none = BTreeSet::new();
        for address in [
            "https://news.example/",
            "https://news.example/artikel?id=3#oben",
            "http://intranet.example/plan",
        ] {
            assert_eq!(check_document(address, &rights(), &none, true), Verdict::Go);
        }
        assert_eq!(
            check_document("https://news.example:8443/", &rights(), &none, true),
            Verdict::Ask(site("https://news.example:8443")),
            "another port is another website"
        );
    }

    #[test]
    fn an_unapproved_website_asks_or_is_refused_in_plan() {
        let none = BTreeSet::new();
        assert_eq!(
            check_document("https://partner.example/x", &rights(), &none, true),
            Verdict::Ask(site("https://partner.example"))
        );
        assert_eq!(
            check_document("https://partner.example/x", &rights(), &none, false),
            Verdict::Refuse("site_not_allowed")
        );
        let once: BTreeSet<Site> = [site("https://partner.example")].into_iter().collect();
        assert_eq!(
            check_document("https://partner.example/y", &rights(), &once, false),
            Verdict::Go,
            "allowed once in this session"
        );
    }

    #[test]
    fn other_schemes_internal_pages_and_user_info_never_load() {
        let none = BTreeSet::new();
        for address in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<h1>x</h1>",
            "blob:https://news.example/123",
            "chrome://settings",
            "edge://settings",
            "brave://settings",
            "devtools://devtools/bundled/inspector.html",
            "chrome-extension://abc/page.html",
            "about:config",
            "https://user:pass@news.example/",
            "not an address",
        ] {
            assert_eq!(
                check_document(address, &rights(), &none, true),
                Verdict::Refuse("blocked_address"),
                "{address}"
            );
        }
        assert_eq!(
            check_document("about:blank", &rights(), &none, false),
            Verdict::Go
        );
    }
}
