// src-tauri/src/extension/permissions/tests/manager_tests.rs
//!
//! Unit tests for pure helpers in `permissions::manager` — specifically the
//! security-critical passwords default-label resolver.

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::{
    parse_passwords_default_marker, resolve_passwords_tags_scope, PasswordsGrantRow,
};
use crate::extension::permissions::types::PasswordsScope;
use serde_json::json;

fn row(target: &str, is_default: bool) -> PasswordsGrantRow {
    PasswordsGrantRow {
        target: target.to_string(),
        is_default,
    }
}

#[test]
fn single_label_is_implicit_default() {
    // Exactly one allowed label → it is the default without any marker.
    let scope = resolve_passwords_tags_scope(vec![row("work", false)], true, "ext").unwrap();
    assert_eq!(
        scope,
        PasswordsScope::Tags {
            tags: vec!["work".to_string()],
            default: Some("work".to_string()),
        }
    );
}

#[test]
fn single_label_implicit_default_even_when_marked() {
    // A lone label is the default regardless of an explicit marker.
    let scope = resolve_passwords_tags_scope(vec![row("work", true)], true, "ext").unwrap();
    assert_eq!(scope.default_label(), Some("work"));
}

#[test]
fn multi_label_write_one_marked_uses_that_default() {
    // Multiple labels + write + exactly one marked → that marked label is the
    // default; all labels remain in scope.
    let scope =
        resolve_passwords_tags_scope(vec![row("work", false), row("personal", true)], true, "ext")
            .unwrap();
    match scope {
        PasswordsScope::Tags { tags, default } => {
            assert_eq!(default, Some("personal".to_string()));
            assert!(tags.contains(&"work".to_string()));
            assert!(tags.contains(&"personal".to_string()));
            assert_eq!(tags.len(), 2);
        }
        other => panic!("expected Tags, got {other:?}"),
    }
}

#[test]
fn multi_label_write_none_marked_is_rejected() {
    // SECURITY: multiple labels + write + no marker → ambiguous default →
    // reject the grant entirely.
    let err = resolve_passwords_tags_scope(
        vec![row("work", false), row("personal", false)],
        true,
        "ext",
    )
    .unwrap_err();
    assert!(matches!(err, ExtensionError::SecurityViolation { .. }));
}

#[test]
fn multi_label_write_two_marked_is_rejected() {
    // SECURITY: multiple labels + write + two markers → ambiguous default →
    // reject the grant entirely.
    let err =
        resolve_passwords_tags_scope(vec![row("work", true), row("personal", true)], true, "ext")
            .unwrap_err();
    assert!(matches!(err, ExtensionError::SecurityViolation { .. }));
}

#[test]
fn multi_label_read_only_needs_no_default() {
    // Read-only access never creates entries, so no default is required even
    // with multiple labels and no marker.
    let scope = resolve_passwords_tags_scope(
        vec![row("work", false), row("personal", false)],
        false,
        "ext",
    )
    .unwrap();
    match scope {
        PasswordsScope::Tags { tags, default } => {
            assert_eq!(default, None);
            assert_eq!(tags.len(), 2);
        }
        other => panic!("expected Tags, got {other:?}"),
    }
}

#[test]
fn multi_label_read_only_ignores_marker() {
    // A marker on a read-only multi-label grant is simply ignored (default
    // only matters for create/write).
    let scope = resolve_passwords_tags_scope(
        vec![row("work", true), row("personal", false)],
        false,
        "ext",
    )
    .unwrap();
    assert_eq!(scope.default_label(), None);
}

// ---------------------------------------------------------------------------
// parse_passwords_default_marker — reads the raw `{"default":bool}` marker.
// ---------------------------------------------------------------------------

#[test]
fn parse_marker_true() {
    assert!(parse_passwords_default_marker(Some(
        &json!({ "default": true })
    )));
}

#[test]
fn parse_marker_false() {
    assert!(!parse_passwords_default_marker(Some(
        &json!({ "default": false })
    )));
}

#[test]
fn parse_marker_absent_key_is_false() {
    assert!(!parse_passwords_default_marker(Some(
        &json!({ "other": 1 })
    )));
}

#[test]
fn parse_marker_none_is_false() {
    assert!(!parse_passwords_default_marker(None));
}
