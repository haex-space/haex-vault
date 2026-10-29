//! Tests for the site of a host (spec 049, T041).

use super::site::{is_public_suffix, site_of};

#[test]
fn subdomains_share_their_registrable_domain() {
    assert_eq!(site_of("www.example.de").as_deref(), Some("example.de"));
    assert_eq!(site_of("login.example.de").as_deref(), Some("example.de"));
    assert_eq!(site_of("example.de").as_deref(), Some("example.de"));
    assert_eq!(site_of("WWW.Example.DE.").as_deref(), Some("example.de"));
}

#[test]
fn a_lookalike_is_another_site() {
    assert_eq!(site_of("bad-example.de").as_deref(), Some("bad-example.de"));
    assert_ne!(site_of("bad-example.de"), site_of("example.de"));
}

#[test]
fn multi_label_suffixes_count_as_one_suffix() {
    assert_eq!(site_of("example.co.uk").as_deref(), Some("example.co.uk"));
    assert_eq!(site_of("a.example.co.uk").as_deref(), Some("example.co.uk"));
    assert_eq!(site_of("co.uk"), None);
    assert!(is_public_suffix("co.uk"));
    assert!(is_public_suffix("com"));
    assert!(!is_public_suffix("example.com"));
}

#[test]
fn ip_addresses_and_localhost_are_their_own_site() {
    assert_eq!(site_of("127.0.0.1").as_deref(), Some("127.0.0.1"));
    assert_eq!(site_of("[::1]").as_deref(), Some("::1"));
    assert_eq!(site_of("localhost").as_deref(), Some("localhost"));
    assert_eq!(site_of("app.localhost").as_deref(), Some("app.localhost"));
    assert_ne!(site_of("127.0.0.1"), site_of("127.0.0.2"));
    assert_eq!(site_of(""), None);
}

#[test]
fn an_address_without_a_scheme_means_https() {
    use super::site::{parse_address, site_of_address};
    assert_eq!(
        parse_address("shop.example.de/login").map(|url| url.to_string()),
        Some("https://shop.example.de/login".to_owned())
    );
    assert_eq!(parse_address("  "), None);
    assert_eq!(
        site_of_address("https://login.example.de/x").as_deref(),
        Some("example.de")
    );
    assert_eq!(site_of_address("co.uk"), None);
}
