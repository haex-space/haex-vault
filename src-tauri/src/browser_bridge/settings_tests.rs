//! Tests for the bridge settings (spec 049, T020): off by default on port 19455, ports below 1024
//! refused. Reading and writing runs in `tests/browser_bridge.rs` against a real vault.

use super::settings::{validate_port, BridgeSettings, InvalidPort, DEFAULT_PORT};

#[test]
fn the_bridge_is_off_on_the_port_of_haex_vault_2_by_default() {
    let settings = BridgeSettings::default();
    assert!(!settings.enabled);
    assert_eq!(settings.port, 19455);
    assert_eq!(DEFAULT_PORT, 19455);
}

#[test]
fn only_ports_from_1024_are_accepted() {
    assert_eq!(validate_port(1024), Ok(1024));
    assert_eq!(validate_port(65535), Ok(65535));
    assert_eq!(validate_port(1023), Err(InvalidPort));
    assert_eq!(validate_port(0), Err(InvalidPort));
}
