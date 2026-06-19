// src-tauri/src/passwords/tests.rs
//!
//! Unit tests for pure helpers in the passwords bridge commands.

use crate::extension::error::ExtensionError;
use crate::extension::permissions::types::PasswordsScope;
use crate::passwords::commands::{resolve_create_tags, validate_tags_in_scope};

fn tags_scope(tags: &[&str], default: Option<&str>) -> PasswordsScope {
    PasswordsScope::Tags {
        tags: tags.iter().map(|s| s.to_string()).collect(),
        default: default.map(String::from),
    }
}

// ---------------------------------------------------------------------------
// resolve_create_tags — injects the scope's default label on create.
// ---------------------------------------------------------------------------

#[test]
fn resolve_create_tags_all_scope_is_passthrough() {
    // Unscoped access injects nothing — the caller's tags are written as-is.
    let scope = PasswordsScope::All;
    assert_eq!(
        resolve_create_tags(&["custom".to_string()], &scope),
        vec!["custom".to_string()]
    );
}

#[test]
fn resolve_create_tags_all_scope_empty_stays_empty() {
    let scope = PasswordsScope::All;
    let empty: Vec<String> = vec![];
    assert_eq!(resolve_create_tags(&empty, &scope), empty);
}

#[test]
fn resolve_create_tags_injects_default_when_no_tags_given() {
    // No tags submitted → the entry still gets the resolved default label.
    let scope = tags_scope(&["work", "personal"], Some("work"));
    assert_eq!(resolve_create_tags(&[], &scope), vec!["work".to_string()]);
}

#[test]
fn resolve_create_tags_keeps_allowed_tag_alongside_default() {
    // Caller passes an allowed non-default tag. The default is ALWAYS applied,
    // so the entry ends up with BOTH the passed tag and the default label.
    let scope = tags_scope(&["work", "personal"], Some("work"));
    let result = resolve_create_tags(&["personal".to_string()], &scope);
    assert!(result.contains(&"personal".to_string()));
    assert!(result.contains(&"work".to_string()));
    assert_eq!(result.len(), 2);
}

#[test]
fn resolve_create_tags_does_not_duplicate_default_if_already_passed() {
    // Passing the default label itself must not duplicate it.
    let scope = tags_scope(&["work", "personal"], Some("work"));
    let result = resolve_create_tags(&["work".to_string()], &scope);
    assert_eq!(result, vec!["work".to_string()]);
}

#[test]
fn resolve_create_tags_single_label_injects_implicit_default() {
    // A single-label scope resolves the lone label as its default.
    let scope = tags_scope(&["work"], Some("work"));
    assert_eq!(resolve_create_tags(&[], &scope), vec!["work".to_string()]);
}

#[test]
fn resolve_create_tags_preserves_out_of_scope_tags_for_later_validation() {
    // resolve_create_tags only injects the default; it does NOT filter. An
    // out-of-scope tag survives here and is rejected earlier by
    // validate_tags_in_scope (which runs before this helper in the create
    // path); the contract is that this helper never drops caller tags.
    let scope = tags_scope(&["work"], Some("work"));
    let result = resolve_create_tags(&["work".to_string(), "evil".to_string()], &scope);
    assert!(result.contains(&"work".to_string()));
    assert!(result.contains(&"evil".to_string()));
}

#[test]
fn resolve_create_tags_no_default_injects_nothing() {
    // A Tags scope with no resolved default (e.g. multi-label read-only) has
    // nothing to inject.
    let scope = tags_scope(&["work", "personal"], None);
    assert_eq!(
        resolve_create_tags(&["work".to_string()], &scope),
        vec!["work".to_string()]
    );
}

// ---------------------------------------------------------------------------
// validate_tags_in_scope — the guard the create path runs on SUBMITTED tags
// BEFORE default injection. These lock in that an out-of-scope-only submission
// is rejected (so injecting the default cannot mask it).
// ---------------------------------------------------------------------------

#[test]
fn validate_tags_in_scope_rejects_no_overlap_submission() {
    // SECURITY: a submission with no in-scope tag must be rejected. The create
    // path runs this on the caller's submitted tags before injecting the
    // default, so this rejection is never masked.
    let scope = tags_scope(&["work"], Some("work"));
    let err = validate_tags_in_scope(&["evil".to_string()], &scope).unwrap_err();
    assert!(matches!(err, ExtensionError::SecurityViolation { .. }));
}

#[test]
fn validate_tags_in_scope_accepts_overlapping_submission() {
    let scope = tags_scope(&["work"], Some("work"));
    assert!(validate_tags_in_scope(&["work".to_string()], &scope).is_ok());
}

#[test]
fn validate_tags_in_scope_rejects_empty_submission() {
    // Empty is a ValidationError for any scope. The create path skips this
    // call for Tags scope (default injection covers it) but keeps it for All.
    let scope = PasswordsScope::All;
    let err = validate_tags_in_scope(&[], &scope).unwrap_err();
    assert!(matches!(err, ExtensionError::ValidationError { .. }));
}

#[test]
fn validate_tags_in_scope_all_accepts_any_nonempty() {
    let scope = PasswordsScope::All;
    assert!(validate_tags_in_scope(&["anything".to_string()], &scope).is_ok());
}
