// src-tauri/src/extension/permissions/tests/manager_tests.rs
//!
//! Unit tests for pure helpers in `permissions::manager` — specifically the
//! security-critical passwords default-label resolver.

use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::error::ExtensionError;
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::manager::{
    database_matching_status, filesystem_matching_has_constraint_violation,
    filesystem_matching_status, format_filesystem_denied_target, format_shell_denied_target,
    identities_matching_status, parse_passwords_default_marker, resolve_identities_decision,
    resolve_passwords_tags_scope, shell_matching_has_constraint_violation, shell_matching_status,
    web_matching_status, IdentitiesDecision, PasswordsGrantRow,
};
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, FsAction, FsConstraints, IdentityAction, PasswordsScope,
    PermissionConstraints, PermissionStatus, ResourceType, ShellAction, ShellConstraints,
    WebAction,
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

// ---------------------------------------------------------------------------
// Constraint-violation diagnostic suffix.
//
// Pre-refactor, `check_filesystem_permission` differentiated explicit-deny
// rows from constraint-violating rows in the error message via a trailing
// "(constraint violation)" discriminator. The deny-first refactor accidentally
// dropped that discriminator. These tests lock in:
//   - `filesystem_matching_has_constraint_violation` flags constraint-violating
//     rows within the matching set, and
//   - `format_filesystem_denied_target` appends the suffix iff that flag is set.
// ---------------------------------------------------------------------------

fn fs_permission_with_extension_constraint(
    target: &str,
    action: FsAction,
    status: PermissionStatus,
    allowed_extensions: Vec<&str>,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-fs".to_string(),
        resource_type: ResourceType::Fs,
        action: Action::Filesystem(action),
        target: target.to_string(),
        constraints: Some(PermissionConstraints::Filesystem(FsConstraints {
            allowed_extensions: Some(allowed_extensions.into_iter().map(String::from).collect()),
            ..Default::default()
        })),
        status,
        raw_constraints: None,
    }
}

#[test]
fn constraint_violation_flagged_for_denied_path() {
    // Row matches by path + action but its extension allow-list excludes the
    // file's extension → constraint-violating row in the matching set.
    let permissions = vec![fs_permission_with_extension_constraint(
        "/tmp/*",
        FsAction::Read,
        PermissionStatus::Granted,
        vec![".md"],
    )];
    assert!(filesystem_matching_has_constraint_violation(
        &permissions,
        "/tmp/file.txt",
        &Action::Filesystem(FsAction::Read),
        std::path::Path::new("/tmp/file.txt"),
    ));
}

#[test]
fn constraint_violation_not_flagged_for_plain_denied_row() {
    // A Denied row with no constraints must NOT be flagged as a
    // constraint violation — the suffix is reserved for the (allow-list)
    // failure case.
    let permissions = vec![fs_permission(
        "/tmp/x",
        FsAction::Read,
        PermissionStatus::Denied,
    )];
    assert!(!filesystem_matching_has_constraint_violation(
        &permissions,
        "/tmp/x",
        &Action::Filesystem(FsAction::Read),
        std::path::Path::new("/tmp/x"),
    ));
}

#[test]
fn denied_target_string_appends_constraint_violation_suffix() {
    // Byte-identical wording match to pre-refactor diagnostics.
    let target = format_filesystem_denied_target("/tmp/file.txt", true);
    assert_eq!(
        target,
        "filesystem path '/tmp/file.txt' (constraint violation)"
    );
}

#[test]
fn denied_target_string_omits_suffix_when_no_constraint_violation() {
    let target = format_filesystem_denied_target("/tmp/file.txt", false);
    assert_eq!(target, "filesystem path '/tmp/file.txt'");
}

// ---------------------------------------------------------------------------
// web_matching_status — deny-first precedence for web permissions.
//
// A `Denied` row for the same URL/domain MUST override a `Granted` row
// regardless of insertion order — otherwise the first-match behaviour of the
// previous `iter().find()` implementation would let a broad `*` grant shadow
// a more specific deny.
// ---------------------------------------------------------------------------

fn web_permission(
    target: &str,
    action: WebAction,
    status: PermissionStatus,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-web".to_string(),
        resource_type: ResourceType::Web,
        action: Action::Web(action),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

#[test]
fn deny_wins_web_granted_first() {
    // SECURITY: a Denied row for a specific URL MUST win even when a broad `*`
    // Granted row is inserted first.
    let permissions = vec![
        web_permission("*", WebAction::Get, PermissionStatus::Granted),
        web_permission(
            "https://evil.example.com",
            WebAction::Get,
            PermissionStatus::Denied,
        ),
    ];

    let resolved =
        web_matching_status(&permissions, "https://evil.example.com", "evil.example.com");
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn deny_wins_web_denied_first() {
    // Symmetric to the above: Denied-first order still yields Denied.
    let permissions = vec![
        web_permission(
            "https://evil.example.com",
            WebAction::Get,
            PermissionStatus::Denied,
        ),
        web_permission("*", WebAction::Get, PermissionStatus::Granted),
    ];

    let resolved =
        web_matching_status(&permissions, "https://evil.example.com", "evil.example.com");
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

// ---------------------------------------------------------------------------
// shell_matching_status — deny-first precedence for shell permissions.
//
// A `Denied` row for the same command MUST override a `Granted` row regardless
// of insertion order — otherwise the first-match behaviour of the previous
// `iter().find()` implementation would let a broad `*` grant shadow a more
// specific deny.
//
// Constraint-violating rows (e.g. forbidden_args/allowed_subcommands rejected)
// preserve the pre-refactor semantics: they resolve to `Denied` within the
// matching set (NOT silently excluded), so the diagnostic
// `(constraint violation)` discriminator is reachable.
// ---------------------------------------------------------------------------

fn shell_permission(target: &str, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-shell".to_string(),
        resource_type: ResourceType::Shell,
        action: Action::Shell(ShellAction::Execute),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

fn shell_permission_with_forbidden_args(
    target: &str,
    status: PermissionStatus,
    forbidden_args: Vec<&str>,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-shell".to_string(),
        resource_type: ResourceType::Shell,
        action: Action::Shell(ShellAction::Execute),
        target: target.to_string(),
        constraints: Some(PermissionConstraints::Shell(ShellConstraints {
            forbidden_args: Some(forbidden_args.into_iter().map(String::from).collect()),
            ..Default::default()
        })),
        status,
        raw_constraints: None,
    }
}

#[test]
fn deny_wins_shell_granted_first() {
    // SECURITY: a Denied row for a specific shell command MUST win even when a
    // broad `*` Granted row is inserted first.
    let permissions = vec![
        shell_permission("*", PermissionStatus::Granted),
        shell_permission("git", PermissionStatus::Denied),
    ];

    let resolved = shell_matching_status(&permissions, "git", &["push".to_string()]);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn deny_wins_shell_denied_first() {
    // Symmetric to the above: Denied-first order still yields Denied.
    let permissions = vec![
        shell_permission("git", PermissionStatus::Denied),
        shell_permission("*", PermissionStatus::Granted),
    ];

    let resolved = shell_matching_status(&permissions, "git", &["push".to_string()]);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn constraint_violation_flagged_for_denied_shell_command() {
    // Row matches by command but its `forbidden_args` includes one of the args
    // → constraint-violating row in the matching set.
    let permissions = vec![shell_permission_with_forbidden_args(
        "git",
        PermissionStatus::Granted,
        vec!["--force"],
    )];
    assert!(shell_matching_has_constraint_violation(
        &permissions,
        "git",
        &["push".to_string(), "--force".to_string()],
    ));
}

#[test]
fn constraint_violation_not_flagged_for_plain_denied_shell_row() {
    // A Denied row with no constraints must NOT be flagged as a constraint
    // violation — the suffix is reserved for the typed-constraint failure case.
    let permissions = vec![shell_permission("git", PermissionStatus::Denied)];
    assert!(!shell_matching_has_constraint_violation(
        &permissions,
        "git",
        &["push".to_string()],
    ));
}

#[test]
fn shell_denied_target_string_appends_constraint_violation_suffix() {
    // Byte-identical wording match to pre-refactor diagnostics.
    let target = format_shell_denied_target("git", &["push".to_string()], true);
    assert_eq!(
        target,
        "shell command 'git' with args [\"push\"] (constraint violation)"
    );
}

#[test]
fn shell_denied_target_string_omits_suffix_when_no_constraint_violation() {
    let target = format_shell_denied_target("git", &["push".to_string()], false);
    assert_eq!(target, "shell command 'git' with args [\"push\"]");
}
