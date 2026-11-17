//! The stand-in catalog of the e2e suite (spec 094, T036): what it accepts and what it refuses.
//! These tests never call `install` with `allowed = true`, which would replace the catalog every
//! other test of this process reads.

use super::{active, install};
use crate::cli_tools::catalog::Catalog;

const HASH: &str = "0000000000000000000000000000000000000000000000000000000000000001";

fn catalog(url: &str) -> String {
    format!(
        r#"{{"schema":1,"vendors":[{{"vendor":"claude","display_name":"Claude Code",
        "download_hosts":["downloads.claude.ai"],"executable":{{"linux-x64":"claude"}},
        "config_env":"CLAUDE_CONFIG_DIR","versions":[{{"version":"9.9.9","platforms":
        {{"linux-x64":{{"url":"{url}","sha256":"{HASH}","size":10,"kind":"file"}}}}}}]}}]}}"#
    )
}

#[test]
fn a_release_build_refuses_a_stand_in() {
    let error = install(&catalog("http://127.0.0.1:1/claude"), false).unwrap_err();
    assert!(format!("{error:?}").contains("debug builds"), "{error:?}");
    assert!(active().is_none(), "nothing was put in place");
}

#[test]
fn only_a_stand_in_may_download_from_this_device_over_plain_http() {
    let local = catalog("http://127.0.0.1:4010/claude");
    assert!(Catalog::parse_loopback(&local).is_ok());
    assert!(Catalog::parse_loopback(&catalog("http://localhost:4010/claude")).is_ok());
    // The catalog that ships is never allowed this.
    assert!(Catalog::parse(&local).is_err());
    // Not a stand-in for the open internet either.
    assert!(Catalog::parse_loopback(&catalog("http://example.com/claude")).is_err());
    assert!(Catalog::parse_loopback(&catalog("http://127.0.0.1.evil.org/claude")).is_err());
    assert!(Catalog::parse_loopback(&catalog("https://evil.example.org/claude")).is_err());
}

#[test]
fn a_stand_in_is_still_a_valid_catalog() {
    let broken = catalog("http://127.0.0.1:4010/claude").replace(HASH, "not-a-hash");
    assert!(Catalog::parse_loopback(&broken).is_err());
}
