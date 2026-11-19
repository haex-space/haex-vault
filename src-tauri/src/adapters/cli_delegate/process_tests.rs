//! Tests for `map_spawn_error` and `ensure_available` (tasks.md T019; spec 094).

use super::process::{ensure_available, map_spawn_error};
use super::DelegateVendor;
use crate::adapters::AdapterError;

#[test]
fn a_program_that_vanished_from_its_installation_needs_a_reinstall() {
    let error = std::io::Error::from(std::io::ErrorKind::NotFound);
    match map_spawn_error(DelegateVendor::Claude, error) {
        AdapterError::Unavailable { reason } => assert_eq!(reason, "needs reinstall: claude"),
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn a_folder_that_cannot_run_programs_is_not_runnable_and_is_not_worked_around() {
    let error = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    match map_spawn_error(DelegateVendor::Codex, error) {
        AdapterError::Unavailable { reason } => {
            assert_eq!(reason, "not runnable: codex: permission denied");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn other_spawn_failures_stay_generic_and_name_no_install_command() {
    let error = std::io::Error::from(std::io::ErrorKind::OutOfMemory);
    match map_spawn_error(DelegateVendor::Codex, error) {
        AdapterError::Http { reason } => {
            assert!(reason.contains("codex"), "reason: {reason}");
            assert!(!reason.contains("npm"), "reason: {reason}");
            assert!(!reason.contains("PATH"), "reason: {reason}");
        }
        other => panic!("expected Http, got {other:?}"),
    }
}

#[test]
fn without_delegates_on_the_device_a_start_is_refused_for_the_platform() {
    assert!(ensure_available(true).is_ok());
    match ensure_available(false) {
        Err(AdapterError::Unavailable { reason }) => assert!(reason.starts_with("platform")),
        other => panic!("{other:?}"),
    }
}
