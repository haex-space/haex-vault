//! Tests for the place of a model (spec 055 R6).

use super::place::{place_of, Place};
use crate::storage::providers::ProviderKind::{ApiKey, CliDelegate, Local};

#[test]
fn a_local_provider_is_this_device_and_a_delegate_is_the_cloud() {
    assert_eq!(place_of(Local, None), Place::ThisDevice);
    assert_eq!(place_of(CliDelegate, Some("claude")), Place::Cloud);
    assert_eq!(place_of(CliDelegate, None), Place::Cloud);
}

#[test]
fn loopback_addresses_are_this_device() {
    for address in [
        "http://localhost:11434/v1",
        "http://LOCALHOST/v1",
        "http://127.0.0.1:8000/v1",
        "http://127.1.2.3/v1",
        "http://[::1]:8080/v1",
        "http://[::ffff:127.0.0.1]:8080/v1",
        "http://0.0.0.0:8000/v1",
    ] {
        assert_eq!(
            place_of(ApiKey, Some(address)),
            Place::ThisDevice,
            "{address}"
        );
    }
}

#[test]
fn private_and_local_names_are_the_own_network() {
    for address in [
        "http://192.168.1.20:8000/v1",
        "http://10.0.0.5/v1",
        "http://172.16.4.9/v1",
        "http://172.31.255.1/v1",
        "http://169.254.10.10/v1",
        "http://[fd12:3456::1]/v1",
        "http://[fe80::1]/v1",
        "http://school-server/v1",
        "http://gpu-box.local:8000/v1",
        "http://ollama.lan/v1",
        "http://school-server./v1",
        "http://[::ffff:10.0.0.1]/v1",
        "http://100.101.102.103:11434/v1",
        // Not every resolver keeps `*.localhost` on this device: consent is asked.
        "http://app.localhost/v1",
    ] {
        assert_eq!(place_of(ApiKey, Some(address)), Place::Network, "{address}");
    }
}

#[test]
fn everything_else_is_the_cloud() {
    for address in [
        "https://api.anthropic.com",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "http://8.8.8.8/v1",
        "http://172.32.0.1/v1",
        "http://100.128.0.1/v1",
        "http://localhost.example.com/v1",
        "http://[2001:db8::1]/v1",
        "https://models.school.example/v1",
    ] {
        assert_eq!(place_of(ApiKey, Some(address)), Place::Cloud, "{address}");
    }
}

#[test]
fn an_address_that_cannot_be_read_counts_as_the_cloud() {
    assert_eq!(place_of(ApiKey, None), Place::Cloud);
    assert_eq!(place_of(ApiKey, Some("not an address")), Place::Cloud);
    assert_eq!(place_of(ApiKey, Some("")), Place::Cloud);
}

#[test]
fn only_this_device_needs_no_consent() {
    assert!(!Place::ThisDevice.needs_consent());
    assert!(Place::Network.needs_consent());
    assert!(Place::Cloud.needs_consent());
}

#[test]
fn a_place_is_named_as_it_is_serialised() {
    for place in [Place::ThisDevice, Place::Network, Place::Cloud] {
        assert_eq!(serde_json::to_value(place).unwrap(), place.as_str());
    }
}
