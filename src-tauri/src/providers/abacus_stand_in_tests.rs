//! The stand-in sign-in address (spec 096, T024): debug builds only, this device only.

use super::{active, install};

#[test]
fn a_release_build_refuses_a_stand_in() {
    assert!(install("http://127.0.0.1:4000", false).is_err());
}

#[test]
fn only_an_http_address_on_this_device_with_a_port_is_taken() {
    for refused in [
        "https://apps.abacus.ai",
        "http://example.com:80",
        "http://127.0.0.1",
        "http://127.0.0.1.evil.example:4000",
        "not a url",
        "ftp://127.0.0.1:4000",
    ] {
        assert!(install(refused, true).is_err(), "{refused}");
    }
}

#[test]
fn an_address_on_this_device_is_taken_without_its_trailing_slash() {
    install("http://localhost:4123/", true).unwrap();
    assert_eq!(active().as_deref(), Some("http://localhost:4123"));
}
