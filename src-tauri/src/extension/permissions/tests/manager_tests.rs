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
    identities_matching_status, normalize_passwords_grant_tags, parse_passwords_default_marker,
    passwords_session_scope, resolve_identities_decision, resolve_passwords_tags_scope,
    rw_resource_matching_status, rw_resource_session_status,
    shell_matching_has_constraint_violation, shell_matching_status, spaces_matching_status,
    spaces_session_status, web_matching_status, IdentitiesDecision, PasswordsGrantRow,
};
use crate::extension::permissions::session::SessionPermissionStore;
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, FsAction, FsConstraints, IdentityAction,
    PasswordsAction, PasswordsScope, PermissionConstraints, PermissionStatus, ResourceType,
    RwAction, ShellAction, ShellConstraints, SpaceAction, WebAction,
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
                bookmarks: None,
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

/// Deliberate hardening over pre-refactor `iter().find()` first-match behavior:
/// a constraint-violating specific row resolves to `Denied` and deny-first
/// precedence makes that terminal, even when a sibling wildcard would otherwise
/// grant. Mirrors Task 3's filesystem precedent. See PR #525 follow-ups.
#[test]
fn constraint_violation_on_specific_row_poisons_wildcard_grant() {
    // Wildcard-first ordering: pre-refactor `iter().find()` would have returned
    // the `*` row's Granted; post-refactor scans the full matching set and the
    // `git` row's `forbidden_args` violation flips the resolution to Denied.
    let wildcard_first = vec![
        shell_permission("*", PermissionStatus::Granted),
        shell_permission_with_forbidden_args("git", PermissionStatus::Granted, vec!["push"]),
    ];
    assert_eq!(
        shell_matching_status(&wildcard_first, "git", &["push".to_string()]),
        Some(PermissionStatus::Denied),
        "wildcard-first ordering must still deny when a specific row's constraints fail",
    );

    // Specific-first ordering: symmetric — same Denied outcome regardless of
    // insertion order, locking in the deny-first invariant.
    let specific_first = vec![
        shell_permission_with_forbidden_args("git", PermissionStatus::Granted, vec!["push"]),
        shell_permission("*", PermissionStatus::Granted),
    ];
    assert_eq!(
        shell_matching_status(&specific_first, "git", &["push".to_string()]),
        Some(PermissionStatus::Denied),
        "specific-first ordering must deny on constraint violation",
    );
}

// ---------------------------------------------------------------------------
// Spaces precedence + RW->R session-permission escalation
//
// Mirrors the shell/web/db deny-first regression tests AND covers the
// pre-refactor session-fallback semantics: a session `ReadWrite` grant must
// also satisfy a later `Read` check (since a writer trivially reads),
// matching the DB-side `action_allows` predicate. Implemented at the
// call-site (not in `SessionPermissionStore`), so the generic
// "exact match" session contract still holds for other resources.
// ---------------------------------------------------------------------------

fn spaces_permission(action: SpaceAction, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-spaces".to_string(),
        resource_type: ResourceType::Spaces,
        action: Action::Spaces(action),
        target: "*".to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

#[test]
fn deny_wins_spaces_granted_first() {
    // SECURITY: a Denied row for the Read action MUST win even when a broad
    // ReadWrite Granted row is inserted first.
    let permissions = vec![
        spaces_permission(SpaceAction::ReadWrite, PermissionStatus::Granted),
        spaces_permission(SpaceAction::Read, PermissionStatus::Denied),
    ];

    let resolved = spaces_matching_status(&permissions, SpaceAction::Read);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn deny_wins_spaces_denied_first() {
    // Symmetric: Denied-first order still yields Denied.
    let permissions = vec![
        spaces_permission(SpaceAction::Read, PermissionStatus::Denied),
        spaces_permission(SpaceAction::ReadWrite, PermissionStatus::Granted),
    ];

    let resolved = spaces_matching_status(&permissions, SpaceAction::Read);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn session_rw_grant_covers_read_request() {
    // A session ReadWrite grant must satisfy a Read request — matches the
    // DB-side `action_allows` semantics (writer trivially reads).
    let session = SessionPermissionStore::new();
    let extension_id = "test-ext-precedence-spaces";
    session.set_permission(ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: extension_id.to_string(),
        resource_type: ResourceType::Spaces,
        action: Action::Spaces(SpaceAction::ReadWrite),
        target: "*".to_string(),
        constraints: None,
        status: PermissionStatus::Granted,
        raw_constraints: None,
    });

    let resolved = spaces_session_status(&session, extension_id, SpaceAction::Read);
    assert_eq!(resolved, Some(PermissionStatus::Granted));
}

// ---------------------------------------------------------------------------
// rw_resource (SyncServers / CloudStorage / SyncRules) deny-first precedence
// and session RW⇒R escalation
//
// `rw_resource.rs` is the shared helper for the three action-level Read/
// ReadWrite resources. Deny-first precedence and the session-level RW⇒R
// escalation apply uniformly across all three — covered here by exercising
// the helper directly with one row of each resource type.
// ---------------------------------------------------------------------------

fn rw_resource_permission(
    resource_type: ResourceType,
    action: RwAction,
    status: PermissionStatus,
) -> ExtensionPermission {
    let perm_action = match resource_type {
        ResourceType::SyncServers => Action::SyncServers(action),
        ResourceType::CloudStorage => Action::CloudStorage(action),
        ResourceType::SyncRules => Action::SyncRules(action),
        other => panic!("rw_resource_permission only handles RW resources, got {other:?}"),
    };
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test-ext-precedence-rw".to_string(),
        resource_type,
        action: perm_action,
        target: "*".to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

#[test]
fn deny_wins_rw_resource_granted_first() {
    // SECURITY: a Denied row for the Read action MUST win even when a broad
    // ReadWrite Granted row is inserted first. Exercises SyncServers, which
    // shares the helper with CloudStorage and SyncRules.
    let permissions = vec![
        rw_resource_permission(
            ResourceType::SyncServers,
            RwAction::ReadWrite,
            PermissionStatus::Granted,
        ),
        rw_resource_permission(
            ResourceType::SyncServers,
            RwAction::Read,
            PermissionStatus::Denied,
        ),
    ];

    let resolved =
        rw_resource_matching_status(&permissions, ResourceType::SyncServers, RwAction::Read);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn deny_wins_rw_resource_denied_first() {
    // Symmetric: Denied-first order still yields Denied.
    let permissions = vec![
        rw_resource_permission(
            ResourceType::CloudStorage,
            RwAction::Read,
            PermissionStatus::Denied,
        ),
        rw_resource_permission(
            ResourceType::CloudStorage,
            RwAction::ReadWrite,
            PermissionStatus::Granted,
        ),
    ];

    let resolved =
        rw_resource_matching_status(&permissions, ResourceType::CloudStorage, RwAction::Read);
    assert_eq!(resolved, Some(PermissionStatus::Denied));
}

#[test]
fn session_rw_covers_read_for_cloud_storage() {
    // A session ReadWrite grant must satisfy a Read request — matches the
    // DB-side `action_allows` semantics (writer trivially reads). Verifies
    // the RW⇒R escalation works for CloudStorage independently of the
    // SyncServers / SyncRules variants.
    let session = SessionPermissionStore::new();
    let extension_id = "test-ext-precedence-rw-cloud";
    session.set_permission(ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: extension_id.to_string(),
        resource_type: ResourceType::CloudStorage,
        action: Action::CloudStorage(RwAction::ReadWrite),
        target: "*".to_string(),
        constraints: None,
        status: PermissionStatus::Granted,
        raw_constraints: None,
    });

    let resolved = rw_resource_session_status(
        &session,
        extension_id,
        ResourceType::CloudStorage,
        RwAction::Read,
    );
    assert_eq!(resolved, Some(PermissionStatus::Granted));
}

// ---------------------------------------------------------------------------
// normalize_passwords_grant_tags — cleans/validates a dialog-submitted tag
// grant before it's persisted (DB and session paths share this).
// ---------------------------------------------------------------------------

#[test]
fn normalize_trims_dedupes_and_drops_empty_tags() {
    let (tags, is_wildcard) = normalize_passwords_grant_tags(
        &[
            "  work ".to_string(),
            "".to_string(),
            "work".to_string(),
            "personal".to_string(),
        ],
        None,
        PasswordsAction::Read,
        PermissionStatus::Granted,
        "ext",
    )
    .unwrap();
    assert!(!is_wildcard);
    assert_eq!(tags, vec!["work".to_string(), "personal".to_string()]);
}

#[test]
fn normalize_wildcard_collapses_everything() {
    let (tags, is_wildcard) = normalize_passwords_grant_tags(
        &["work".to_string(), "*".to_string()],
        None,
        PasswordsAction::ReadWrite,
        PermissionStatus::Granted,
        "ext",
    )
    .unwrap();
    assert!(is_wildcard);
    assert_eq!(tags, vec!["*".to_string()]);
}

#[test]
fn normalize_rejects_all_empty_tags() {
    let err = normalize_passwords_grant_tags(
        &["  ".to_string()],
        None,
        PasswordsAction::Read,
        PermissionStatus::Granted,
        "ext",
    )
    .unwrap_err();
    assert!(matches!(err, ExtensionError::ValidationError { .. }));
}

#[test]
fn normalize_requires_default_for_multi_tag_write_grant() {
    let err = normalize_passwords_grant_tags(
        &["work".to_string(), "personal".to_string()],
        None,
        PasswordsAction::ReadWrite,
        PermissionStatus::Granted,
        "ext",
    )
    .unwrap_err();
    assert!(matches!(err, ExtensionError::ValidationError { .. }));

    // A defaultTag not among the granted tags is equally invalid.
    let err = normalize_passwords_grant_tags(
        &["work".to_string(), "personal".to_string()],
        Some("other"),
        PasswordsAction::ReadWrite,
        PermissionStatus::Granted,
        "ext",
    )
    .unwrap_err();
    assert!(matches!(err, ExtensionError::ValidationError { .. }));

    // A valid defaultTag among the granted tags succeeds.
    let (tags, _) = normalize_passwords_grant_tags(
        &["work".to_string(), "personal".to_string()],
        Some("personal"),
        PasswordsAction::ReadWrite,
        PermissionStatus::Granted,
        "ext",
    )
    .unwrap();
    assert_eq!(tags, vec!["work".to_string(), "personal".to_string()]);
}

#[test]
fn normalize_no_default_required_when_read_only_single_tag_or_denied() {
    // Read-only multi-tag: no default required.
    assert!(normalize_passwords_grant_tags(
        &["work".to_string(), "personal".to_string()],
        None,
        PasswordsAction::Read,
        PermissionStatus::Granted,
        "ext",
    )
    .is_ok());

    // Single tag: implicit default, no marker required.
    assert!(normalize_passwords_grant_tags(
        &["work".to_string()],
        None,
        PasswordsAction::ReadWrite,
        PermissionStatus::Granted,
        "ext",
    )
    .is_ok());

    // Denied decision: default-label concerns only apply to grants.
    assert!(normalize_passwords_grant_tags(
        &["work".to_string(), "personal".to_string()],
        None,
        PasswordsAction::ReadWrite,
        PermissionStatus::Denied,
        "ext",
    )
    .is_ok());
}

// ---------------------------------------------------------------------------
// passwords_session_scope — session-store counterpart of check_passwords_permission
// ---------------------------------------------------------------------------

fn passwords_session_permission(
    extension_id: &str,
    action: PasswordsAction,
    target: &str,
    status: PermissionStatus,
    is_default: bool,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: extension_id.to_string(),
        resource_type: ResourceType::Passwords,
        action: Action::Passwords(action),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: if is_default {
            Some(json!({ "default": true }))
        } else {
            None
        },
    }
}

#[test]
fn passwords_session_scope_none_when_no_session_entry() {
    let session = SessionPermissionStore::new();
    let resolved = passwords_session_scope(&session, "ext-none", PasswordsAction::Read);
    assert!(resolved.is_none());
}

#[test]
fn passwords_session_scope_denied_blocks_access() {
    let session = SessionPermissionStore::new();
    let extension_id = "ext-session-denied";
    session.set_permission(passwords_session_permission(
        extension_id,
        PasswordsAction::Read,
        "*",
        PermissionStatus::Denied,
        false,
    ));

    let resolved = passwords_session_scope(&session, extension_id, PasswordsAction::Read)
        .expect("session row should resolve")
        .unwrap_err();
    assert!(matches!(resolved, ExtensionError::PermissionDenied { .. }));
}

#[test]
fn passwords_session_scope_wildcard_grants_all() {
    let session = SessionPermissionStore::new();
    let extension_id = "ext-session-wildcard";
    session.set_permission(passwords_session_permission(
        extension_id,
        PasswordsAction::Read,
        "*",
        PermissionStatus::Granted,
        false,
    ));

    let resolved = passwords_session_scope(&session, extension_id, PasswordsAction::Read)
        .expect("session row should resolve")
        .expect("wildcard grant should resolve Ok");
    assert_eq!(resolved, PasswordsScope::All);
}

#[test]
fn passwords_session_scope_resolves_multi_tag_default_from_raw_constraints() {
    let session = SessionPermissionStore::new();
    let extension_id = "ext-session-multi-tag";
    session.set_permission(passwords_session_permission(
        extension_id,
        PasswordsAction::ReadWrite,
        "work",
        PermissionStatus::Granted,
        false,
    ));
    session.set_permission(passwords_session_permission(
        extension_id,
        PasswordsAction::ReadWrite,
        "personal",
        PermissionStatus::Granted,
        true,
    ));

    let resolved = passwords_session_scope(&session, extension_id, PasswordsAction::ReadWrite)
        .expect("session rows should resolve")
        .expect("multi-tag grant with a default marker should resolve Ok");
    match resolved {
        PasswordsScope::Tags { tags, default } => {
            assert_eq!(tags.len(), 2);
            assert_eq!(default, Some("personal".to_string()));
        }
        other => panic!("expected Tags scope, got {other:?}"),
    }
}

#[test]
fn passwords_session_scope_read_write_escalation_covers_read() {
    // A session ReadWrite grant must satisfy a Read request, same as the
    // DB-side action_allows semantics (writer trivially reads).
    let session = SessionPermissionStore::new();
    let extension_id = "ext-session-rw-escalation";
    session.set_permission(passwords_session_permission(
        extension_id,
        PasswordsAction::ReadWrite,
        "*",
        PermissionStatus::Granted,
        false,
    ));

    let resolved = passwords_session_scope(&session, extension_id, PasswordsAction::Read)
        .expect("session row should resolve")
        .expect("ReadWrite grant should cover a Read request");
    assert_eq!(resolved, PasswordsScope::All);
}

// ---------------------------------------------------------------------------
// set_passwords_grant — persisting a dialog decision must not let a deselected
// tag block the tags the user actually granted (default-deny model).
// ---------------------------------------------------------------------------

#[test]
fn set_passwords_grant_partial_grant_does_not_deny_deselected() {
    // User was offered "work" + "personal" but granted only "work". "personal"
    // must simply be out of scope — NOT an explicit deny that would drop
    // "work" too via the deny-first read check.
    let session = SessionPermissionStore::new();
    let extension_id = "ext-partial-grant";
    session
        .set_passwords_grant(
            extension_id,
            PasswordsAction::ReadWrite,
            &["work".to_string()],
            Some("work"),
            PermissionStatus::Granted,
        )
        .expect("partial grant should persist");

    let resolved = passwords_session_scope(&session, extension_id, PasswordsAction::ReadWrite)
        .expect("session row should resolve")
        .expect("granted tag must stay usable, not be cancelled by the deselected one");
    match resolved {
        PasswordsScope::Tags { tags, default } => {
            assert_eq!(tags, vec!["work".to_string()]);
            assert_eq!(default, Some("work".to_string()));
        }
        other => panic!("expected Tags scope with only 'work', got {other:?}"),
    }
}

#[test]
fn set_passwords_grant_deny_blocks_all() {
    // A deny rejects the whole request via a single wildcard deny row.
    let session = SessionPermissionStore::new();
    let extension_id = "ext-deny-all";
    session
        .set_passwords_grant(
            extension_id,
            PasswordsAction::ReadWrite,
            &["work".to_string()],
            None,
            PermissionStatus::Denied,
        )
        .expect("deny should persist");

    let resolved = passwords_session_scope(&session, extension_id, PasswordsAction::ReadWrite)
        .expect("session row should resolve")
        .unwrap_err();
    assert!(matches!(resolved, ExtensionError::PermissionDenied { .. }));
}
