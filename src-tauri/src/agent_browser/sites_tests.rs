use super::sites::{with_scheme, Site, SiteError};

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

    use super::super::sites::{check_document, may_operate, Site, Verdict};
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
            assert_eq!(
                check_document(address, &rights(), None, &none, true),
                Verdict::Go
            );
        }
        assert_eq!(
            check_document("https://news.example:8443/", &rights(), None, &none, true),
            Verdict::Ask(site("https://news.example:8443")),
            "another port is another website"
        );
    }

    #[test]
    fn an_unapproved_website_asks_or_is_refused_in_plan() {
        let none = BTreeSet::new();
        assert_eq!(
            check_document("https://partner.example/x", &rights(), None, &none, true),
            Verdict::Ask(site("https://partner.example"))
        );
        assert_eq!(
            check_document("https://partner.example/x", &rights(), None, &none, false),
            Verdict::Refuse("site_not_allowed")
        );
        let once: BTreeSet<Site> = [site("https://partner.example")].into_iter().collect();
        assert_eq!(
            check_document("https://partner.example/y", &rights(), None, &once, false),
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
                check_document(address, &rights(), None, &none, true),
                Verdict::Refuse("blocked_address"),
                "{address}"
            );
        }
        assert_eq!(
            check_document("about:blank", &rights(), None, &none, false),
            Verdict::Go
        );
    }

    /// FR-043: the level for all websites decides for every website that is not listed.
    #[test]
    fn the_level_for_all_websites_loads_every_website_that_is_not_listed() {
        let none = BTreeSet::new();
        for all in [SiteLevel::Read, SiteLevel::Operate] {
            for address in ["https://partner.example/x", "http://other.example/"] {
                assert_eq!(
                    check_document(address, &rights(), Some(all), &none, false),
                    Verdict::Go,
                    "{address} with {all:?} for all"
                );
            }
        }
        // Blocked addresses stay blocked, whatever the level.
        assert_eq!(
            check_document(
                "file:///etc/passwd",
                &rights(),
                Some(SiteLevel::Operate),
                &none,
                true
            ),
            Verdict::Refuse("blocked_address")
        );
    }

    #[test]
    fn a_listed_website_is_an_exception_to_the_level_for_all() {
        let none = BTreeSet::new();
        // news.example is listed for reading only: operating stays refused although all may operate.
        assert!(!may_operate(
            "https://news.example/",
            &rights(),
            Some(SiteLevel::Operate),
            &none
        ));
        assert!(may_operate(
            "https://partner.example/",
            &rights(),
            Some(SiteLevel::Operate),
            &none
        ));
        assert!(!may_operate(
            "https://partner.example/",
            &rights(),
            Some(SiteLevel::Read),
            &none
        ));
        assert!(!may_operate(
            "https://partner.example/",
            &rights(),
            None,
            &none
        ));
    }

    /// FR-047: a blocked website never loads, whatever the level for all websites says, also when
    /// the user allowed it once or the agent may ask.
    #[test]
    fn a_blocked_website_never_loads() {
        let mut sites = rights();
        sites.insert(
            site("https://bank.example"),
            SiteGrant {
                level: SiteLevel::Blocked,
                fill_remembered: false,
            },
        );
        let once: BTreeSet<Site> = [site("https://bank.example")].into_iter().collect();
        for (all, once, ask) in [
            (Some(SiteLevel::Operate), BTreeSet::new(), true),
            (Some(SiteLevel::Operate), BTreeSet::new(), false),
            (Some(SiteLevel::Read), once.clone(), true),
            (None, once, true),
        ] {
            assert_eq!(
                check_document("https://bank.example/login", &sites, all, &once, ask),
                Verdict::Refuse("site_blocked"),
                "{all:?}, ask {ask}"
            );
        }
        // It is never operated either, and other websites are not touched by it.
        assert!(!may_operate(
            "https://bank.example/",
            &sites,
            Some(SiteLevel::Operate),
            &BTreeSet::new()
        ));
        assert_eq!(
            check_document(
                "https://partner.example/",
                &sites,
                Some(SiteLevel::Operate),
                &BTreeSet::new(),
                true
            ),
            Verdict::Go
        );
        // A blocked level for all websites opens nothing.
        assert_eq!(
            check_document(
                "https://partner.example/",
                &sites,
                Some(SiteLevel::Blocked),
                &BTreeSet::new(),
                false
            ),
            Verdict::Refuse("site_not_allowed")
        );
    }

    #[test]
    fn an_http_website_is_never_operated_whatever_the_level_for_all() {
        let none = BTreeSet::new();
        assert!(!may_operate(
            "http://other.example/",
            &rights(),
            Some(SiteLevel::Operate),
            &none
        ));
    }
}

// ---------------------------------------------------------------------------
// Remembering a fill (FR-021) under the level for all websites (FR-043)
// ---------------------------------------------------------------------------

mod fill {
    use super::super::sites::{remember_fill, Site};
    use crate::mcp_server::grants::{BrowserRights, SiteGrant, SiteLevel};

    fn site(address: &str) -> Site {
        Site::parse(address).unwrap()
    }

    fn grant(level: SiteLevel, fill_remembered: bool) -> SiteGrant {
        SiteGrant {
            level,
            fill_remembered,
        }
    }

    /// A website operated through the level for all becomes an exception at operating that keeps
    /// the fill, so lowering the level later does not lose it.
    #[test]
    fn a_fill_on_a_website_operated_through_the_level_for_all_is_kept() {
        let mut rights = BrowserRights::switched_on();
        let shop = site("https://shop.example");
        assert!(remember_fill(&mut rights, &shop));
        assert_eq!(
            rights.sites.get(&shop),
            Some(&grant(SiteLevel::Operate, true))
        );
        assert!(!remember_fill(&mut rights, &shop), "already remembered");
    }

    #[test]
    fn a_fill_is_never_kept_where_the_agent_may_not_operate() {
        let news = site("https://news.example");
        let mut rights = BrowserRights::switched_on();
        rights
            .sites
            .insert(news.clone(), grant(SiteLevel::Read, false));
        assert!(!remember_fill(&mut rights, &news), "listed for reading");
        assert!(
            !remember_fill(&mut rights, &site("http://plain.example")),
            "plain http"
        );
        for all in [None, Some(SiteLevel::Read)] {
            let mut rights = BrowserRights {
                all,
                ..BrowserRights::default()
            };
            assert!(!remember_fill(&mut rights, &site("https://shop.example")));
            assert!(rights.sites.is_empty(), "{all:?} adds no website");
        }
        assert_eq!(
            rights.sites.get(&news),
            Some(&grant(SiteLevel::Read, false))
        );
        assert_eq!(rights.sites.len(), 1);
    }

    #[test]
    fn a_listed_website_at_operating_keeps_its_fill() {
        let bank = site("https://bank.example");
        let mut rights = BrowserRights::default();
        rights
            .sites
            .insert(bank.clone(), grant(SiteLevel::Operate, false));
        assert!(remember_fill(&mut rights, &bank));
        assert_eq!(
            rights.sites.get(&bank),
            Some(&grant(SiteLevel::Operate, true))
        );
    }
}

#[test]
fn a_bare_domain_gets_a_scheme_and_anything_else_stays() {
    for (given, want) in [
        ("serienstream.to", "https://serienstream.to"),
        (" serienstream.to ", "https://serienstream.to"),
        (
            "www.youtube.com/watch?v=1",
            "https://www.youtube.com/watch?v=1",
        ),
        ("example.com:8080/x", "https://example.com:8080/x"),
        ("localhost:3000", "https://localhost:3000"),
        ("https://example.com", "https://example.com"),
        ("http://example.com", "http://example.com"),
        ("file:///etc/passwd", "file:///etc/passwd"),
        ("chrome://settings", "chrome://settings"),
        ("holzi://settings/ai", "holzi://settings/ai"),
        ("javascript:alert(1)", "javascript:alert(1)"),
        ("mobland erste folge", "mobland erste folge"),
        ("serienstream", "serienstream"),
        ("", ""),
    ] {
        assert_eq!(with_scheme(given), want, "{given:?}");
    }
}
