// src-tauri/src/extension/permissions/tests/manager_tests.rs
//!
//! Unit tests for pure helpers in `permissions::manager` — specifically the
//! security-critical passwords default-label resolver.

use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::error::ExtensionError;
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::manager::{
    database_matching_status, filesystem_matching_status, identities_matching_status,
    parse_passwords_default_marker, resolve_identities_decision, resolve_passwords_tags_scope,
    IdentitiesDecision, PasswordsGrantRow,
};
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, FsAction, IdentityAction, PasswordsScope,
    PermissionStatus, ResourceType,
};
use serde_json::json;
use std::path::PathBuf;

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

// ---------------------------------------------------------------------------
// check_identities_permission decision logic (pure helpers).
//
// Read and Write are DISTINCT capabilities — Write does NOT imply Read. The
// matching is exact-action; the decision precedence mirrors
// check_notifications_permission (DB row wins; else session; else prompt).
// ---------------------------------------------------------------------------

fn identity_permission(action: IdentityAction, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "ext".to_string(),
        resource_type: ResourceType::Identities,
        action: Action::Identities(action),
        target: "*".to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

#[test]
fn identities_read_grant_allows_read_denies_write() {
    let perms = vec![identity_permission(
        IdentityAction::Read,
        PermissionStatus::Granted,
    )];

    // Read grant → Read is allowed.
    let read_status = identities_matching_status(&perms, IdentityAction::Read);
    assert_eq!(read_status, Some(PermissionStatus::Granted));
    assert_eq!(
        resolve_identities_decision(read_status, false, false),
        IdentitiesDecision::Allow
    );

    // Write has NO matching permission (Read does not imply Write) → prompt.
    let write_status = identities_matching_status(&perms, IdentityAction::Write);
    assert_eq!(write_status, None);
    assert_eq!(
        resolve_identities_decision(write_status, false, false),
        IdentitiesDecision::Prompt
    );
}

#[test]
fn identities_write_grant_allows_write_denies_read() {
    let perms = vec![identity_permission(
        IdentityAction::Write,
        PermissionStatus::Granted,
    )];

    // Write grant → contact-insert (Write) is allowed.
    let write_status = identities_matching_status(&perms, IdentityAction::Write);
    assert_eq!(write_status, Some(PermissionStatus::Granted));
    assert_eq!(
        resolve_identities_decision(write_status, false, false),
        IdentitiesDecision::Allow
    );

    // Read has NO matching permission (Write does not imply Read) → prompt.
    let read_status = identities_matching_status(&perms, IdentityAction::Read);
    assert_eq!(read_status, None);
    assert_eq!(
        resolve_identities_decision(read_status, false, false),
        IdentitiesDecision::Prompt
    );
}

#[test]
fn identities_no_grant_prompts() {
    let perms: Vec<ExtensionPermission> = vec![];
    let status = identities_matching_status(&perms, IdentityAction::Read);
    assert_eq!(status, None);
    assert_eq!(
        resolve_identities_decision(status, false, false),
        IdentitiesDecision::Prompt
    );
}

#[test]
fn identities_explicit_denied_denies() {
    let perms = vec![identity_permission(
        IdentityAction::Read,
        PermissionStatus::Denied,
    )];
    let status = identities_matching_status(&perms, IdentityAction::Read);
    assert_eq!(status, Some(PermissionStatus::Denied));
    assert_eq!(
        resolve_identities_decision(status, false, false),
        IdentitiesDecision::Deny
    );
}

#[test]
fn identities_ask_status_prompts() {
    let perms = vec![identity_permission(
        IdentityAction::Write,
        PermissionStatus::Ask,
    )];
    let status = identities_matching_status(&perms, IdentityAction::Write);
    assert_eq!(status, Some(PermissionStatus::Ask));
    assert_eq!(
        resolve_identities_decision(status, false, false),
        IdentitiesDecision::Prompt
    );
}

#[test]
fn identities_session_grant_allows_when_no_db_permission() {
    // No DB permission, but a session grant ("allow once") → allowed.
    assert_eq!(
        resolve_identities_decision(None, true, false),
        IdentitiesDecision::Allow
    );
}

#[test]
fn identities_session_deny_denies_when_no_db_permission() {
    assert_eq!(
        resolve_identities_decision(None, false, true),
        IdentitiesDecision::Deny
    );
}

// ---------------------------------------------------------------------------
// database_matching_status — deny-first precedence for DB permissions.
//
// A `Denied` row for the same (table, action) MUST override a `Granted` row
// regardless of insertion order — otherwise the first-match behaviour of the
// previous `iter().find()` implementation would let a broad grant shadow a
// more specific deny.
// ---------------------------------------------------------------------------

fn test_extension_for_db() -> Extension {
    Extension {
        id: "test-ext-precedence-db".to_string(),
        manifest: ExtensionManifest {
            name: "test-ext-precedence-db".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            entry: Some("index.html".to_string()),
            icon: None,
            public_key: "test_pk".to_string(),
            signature: "test_sig".to_string(),
            permissions: ExtensionPermissions {
                database: None,
                filesystem: None,
                http: None,
                shell: None,
                sync_servers: None,
                cloud_storage: None,
                sync_rules: None,
                spaces: None,
                identities: None,
                passwords: None,
                mail: None,
                notifications: None,
            },
            homepage: None,
            description: None,
            single_instance: None,
            display_mode: Some(DisplayMode::Iframe),
            migrations_dir: None,
            i18n: None,
        },
        source: ExtensionSource::Production {
            path: PathBuf::from("/tmp/test-ext-precedence-db"),
            version: "0.1.0".to_string(),
        },
        enabled: true,
        last_accessed: std::time::SystemTime::now(),
    }
}

fn db_permission(target: &str, action: DbAction, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-db".to_string(),
        resource_type: ResourceType::Db,
        action: Action::Database(action),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

#[test]
fn deny_wins_db_granted_first() {
    // SECURITY: a Denied row for the same (table, action) MUST win even when
    // the Granted row is inserted first.
    let extension = test_extension_for_db();
    let permissions = vec![
        db_permission("users", DbAction::Read, PermissionStatus::Granted),
        db_permission("users", DbAction::Read, PermissionStatus::Denied),
    ];
    let checker = PermissionChecker::new(extension, permissions.clone());

    let resolved = database_matching_status(&permissions, "users", DbAction::Read, &checker);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn deny_wins_db_denied_first() {
    // Symmetric to the above: Denied-first order still yields Denied.
    let extension = test_extension_for_db();
    let permissions = vec![
        db_permission("users", DbAction::Read, PermissionStatus::Denied),
        db_permission("users", DbAction::Read, PermissionStatus::Granted),
    ];
    let checker = PermissionChecker::new(extension, permissions.clone());

    let resolved = database_matching_status(&permissions, "users", DbAction::Read, &checker);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

// ---------------------------------------------------------------------------
// filesystem_matching_status — deny-first precedence for FS permissions.
//
// A `Denied` row for the same (path, action) MUST override a `Granted` row
// regardless of insertion order — otherwise the first-match behaviour of the
// previous `iter().find()` implementation would let a broad grant shadow a
// more specific deny.
// ---------------------------------------------------------------------------

fn fs_permission(target: &str, action: FsAction, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-fs".to_string(),
        resource_type: ResourceType::Fs,
        action: Action::Filesystem(action),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

#[test]
fn deny_wins_fs_granted_first() {
    // SECURITY: a Denied row for the same (path, action) MUST win even when
    // the Granted row is inserted first.
    let permissions = vec![
        fs_permission("/tmp/x", FsAction::Read, PermissionStatus::Granted),
        fs_permission("/tmp/x", FsAction::Read, PermissionStatus::Denied),
    ];

    let resolved = filesystem_matching_status(
        &permissions,
        "/tmp/x",
        &Action::Filesystem(FsAction::Read),
        std::path::Path::new("/tmp/x"),
    );
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn deny_wins_fs_denied_first() {
    // Symmetric to the above: Denied-first order still yields Denied.
    let permissions = vec![
        fs_permission("/tmp/x", FsAction::Read, PermissionStatus::Denied),
        fs_permission("/tmp/x", FsAction::Read, PermissionStatus::Granted),
    ];

    let resolved = filesystem_matching_status(
        &permissions,
        "/tmp/x",
        &Action::Filesystem(FsAction::Read),
        std::path::Path::new("/tmp/x"),
    );
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}
