// src-tauri/src/extension/permissions/tests/silent_read_tests.rs
//!
//! Silent filesystem-read predicate tests.
//!
//! Exercises `PermissionChecker::can_read_path_silently` — the predicate the
//! broadcast layer uses to decide which extensions may receive a file-change
//! event without triggering a permission prompt.
//!
//! The matrix has three independent dimensions:
//!   1. DB permission state   — none / Granted / Denied / Ask / constraint-violating
//!   2. Session permission    — neutral / session-granted / session-denied
//!   3. Path match            — exact / prefix-wildcard / extension-wildcard / no-match
//!
//! Every allow-path must respect "no prompts"; every deny-path must also fail
//! closed when the DB is unreadable or the extension is unknown (covered by
//! the async wrapper, not here).

use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, FsAction, FsConstraints, PermissionConstraints, PermissionStatus,
    ResourceType,
};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_extension() -> Extension {
    Extension {
        id: "pubkey_myext".to_string(),
        manifest: ExtensionManifest {
            name: "myext".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            entry: Some("index.html".to_string()),
            icon: None,
            public_key: "pubkey".to_string(),
            signature: "sig".to_string(),
            permissions: ExtensionPermissions {
                database: None,
                filesystem: None,
                http: None,
                shell: None,
                filesync: None,
                spaces: None,
                identities: None,
            },
            homepage: None,
            description: None,
            single_instance: None,
            display_mode: Some(DisplayMode::Iframe),
            migrations_dir: None,
            i18n: None,
        },
        source: ExtensionSource::Production {
            path: PathBuf::from("/tmp/test"),
            version: "0.1.0".to_string(),
        },
        enabled: true,
        last_accessed: std::time::SystemTime::now(),
    }
}

fn fs_perm(action: FsAction, target: &str, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: "pubkey_myext".to_string(),
        resource_type: ResourceType::Fs,
        action: Action::Filesystem(action),
        target: target.to_string(),
        constraints: None,
        status,
    }
}

fn fs_perm_with_extension_constraint(
    action: FsAction,
    target: &str,
    status: PermissionStatus,
    allowed_extensions: Vec<&str>,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: "pubkey_myext".to_string(),
        resource_type: ResourceType::Fs,
        action: Action::Filesystem(action),
        target: target.to_string(),
        constraints: Some(PermissionConstraints::Filesystem(FsConstraints {
            allowed_extensions: Some(allowed_extensions.into_iter().map(String::from).collect()),
            ..Default::default()
        })),
        status,
    }
}

// ---------------------------------------------------------------------------
// No DB permission
// ---------------------------------------------------------------------------

#[test]
fn no_db_permission_and_no_session_state_denies() {
    let checker = PermissionChecker::new(make_extension(), vec![]);
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), false, false));
}

#[test]
fn session_grant_alone_is_sufficient_when_no_db_entry() {
    let checker = PermissionChecker::new(make_extension(), vec![]);
    assert!(checker.can_read_path_silently(Path::new("/data/file.txt"), true, false));
}

#[test]
fn session_denial_overrides_missing_db_entry() {
    let checker = PermissionChecker::new(make_extension(), vec![]);
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), true, true));
}

// ---------------------------------------------------------------------------
// DB Granted
// ---------------------------------------------------------------------------

#[test]
fn db_granted_exact_match_allows() {
    let perms = vec![fs_perm(FsAction::Read, "/data/file.txt", PermissionStatus::Granted)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(checker.can_read_path_silently(Path::new("/data/file.txt"), false, false));
}

#[test]
fn db_granted_prefix_wildcard_allows_subpath() {
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Granted)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(checker.can_read_path_silently(Path::new("/data/sub/file.txt"), false, false));
}

#[test]
fn db_granted_wildcard_allows_any_path() {
    let perms = vec![fs_perm(FsAction::Read, "*", PermissionStatus::Granted)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(checker.can_read_path_silently(Path::new("/whatever/path.rs"), false, false));
}

#[test]
fn db_granted_readwrite_also_allows_read() {
    let perms = vec![fs_perm(
        FsAction::ReadWrite,
        "/data/*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(checker.can_read_path_silently(Path::new("/data/file.txt"), false, false));
}

#[test]
fn db_granted_does_not_match_outside_prefix() {
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Granted)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(Path::new("/other/file.txt"), false, false));
}

#[test]
fn session_denial_blocks_even_when_db_granted() {
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Granted)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), false, true));
}

// ---------------------------------------------------------------------------
// DB Denied
// ---------------------------------------------------------------------------

#[test]
fn db_denied_blocks_even_with_session_grant() {
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Denied)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), true, false));
}

// ---------------------------------------------------------------------------
// DB Ask
// ---------------------------------------------------------------------------

#[test]
fn db_ask_without_session_grant_denies() {
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Ask)];
    let checker = PermissionChecker::new(make_extension(), perms);
    // Broadcast must NOT prompt — neutral session state means deny.
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), false, false));
}

#[test]
fn db_ask_with_session_grant_allows() {
    // "Ask" can mean "user approved once this session, not persistently"
    // — session grant lifts the DB Ask into silent allow.
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Ask)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(checker.can_read_path_silently(Path::new("/data/file.txt"), true, false));
}

#[test]
fn db_ask_with_session_denial_denies() {
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Ask)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), false, true));
}

// ---------------------------------------------------------------------------
// Constraints
// ---------------------------------------------------------------------------

#[test]
fn constraint_extension_mismatch_denies_even_when_granted() {
    let perms = vec![fs_perm_with_extension_constraint(
        FsAction::Read,
        "/data/*",
        PermissionStatus::Granted,
        vec![".md"],
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    // .txt not in allow-list
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), false, false));
}

#[test]
fn constraint_extension_match_allows_when_granted() {
    let perms = vec![fs_perm_with_extension_constraint(
        FsAction::Read,
        "/data/*",
        PermissionStatus::Granted,
        vec![".md", ".txt"],
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(checker.can_read_path_silently(Path::new("/data/file.txt"), false, false));
}

#[test]
fn constraint_denies_extensionless_path_with_allow_list() {
    let perms = vec![fs_perm_with_extension_constraint(
        FsAction::Read,
        "/data/*",
        PermissionStatus::Granted,
        vec![".md"],
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    // No extension on the file path → constraint cannot be satisfied
    assert!(!checker.can_read_path_silently(Path::new("/data/noext"), false, false));
}

// ---------------------------------------------------------------------------
// Action type
// ---------------------------------------------------------------------------

#[test]
fn non_fs_resource_type_does_not_match_fs_check() {
    // A DB permission targeting "/data/*" must not satisfy a filesystem read,
    // even if the target string happens to look like a path.
    let perms = vec![ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        extension_id: "pubkey_myext".to_string(),
        resource_type: ResourceType::Db,
        action: Action::Database(crate::extension::permissions::types::DbAction::Read),
        target: "/data/*".to_string(),
        constraints: None,
        status: PermissionStatus::Granted,
    }];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(Path::new("/data/file.txt"), false, false));
}

// ---------------------------------------------------------------------------
// Attack-scenario tests
// ---------------------------------------------------------------------------
//
// These cover the integration of path-matching into the public predicate. The
// underlying `matches_path_pattern` has its own exhaustive path-traversal
// suite in `path_traversal_tests.rs`; these tests confirm the silent predicate
// inherits those guarantees and fail-closes on malicious input.

#[test]
fn attack_path_traversal_sibling_dir_is_blocked() {
    // Extension granted "/home/user/docs/*" — attacker-crafted path tries
    // to escape via ../ to a sibling directory.
    let perms = vec![fs_perm(
        FsAction::Read,
        "/home/user/docs/*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(
        Path::new("/home/user/docs/../secrets/key.pem"),
        false,
        false
    ));
}

#[test]
fn attack_path_traversal_absolute_escape_is_blocked() {
    let perms = vec![fs_perm(
        FsAction::Read,
        "/home/user/docs/*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(
        Path::new("/home/user/docs/../../etc/passwd"),
        false,
        false
    ));
}

#[test]
fn attack_url_encoded_traversal_is_blocked() {
    // %2e%2e%2f == "../" — the matcher URL-decodes before normalising.
    let perms = vec![fs_perm(
        FsAction::Read,
        "/home/user/docs/*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(
        Path::new("/home/user/docs/%2e%2e%2fetc/passwd"),
        false,
        false
    ));
}

#[test]
fn attack_null_byte_injection_is_blocked() {
    // Null bytes can truncate strings in some C-level APIs — must not match.
    let perms = vec![fs_perm(FsAction::Read, "/data/*", PermissionStatus::Granted)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(
        Path::new("/data/file.txt\0.secret"),
        false,
        false
    ));
}

#[test]
fn attack_extension_constraint_cannot_be_bypassed_by_path_match() {
    // Even with a matching granted permission, the extension-allow-list
    // constraint must gate the decision. A `.key` file pretending to be `.md`
    // via double extension must not be allowed (the predicate looks at the
    // real final extension).
    let perms = vec![fs_perm_with_extension_constraint(
        FsAction::Read,
        "/data/*",
        PermissionStatus::Granted,
        vec![".md"],
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(
        Path::new("/data/fake.md.key"),
        false,
        false
    ));
}

#[test]
fn attack_unrelated_prefix_does_not_leak_access() {
    // "/data/public/*" must not match "/data/private/secret.txt".
    // Particularly important for prefix-wildcard matchers.
    let perms = vec![fs_perm(
        FsAction::Read,
        "/data/public/*",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(
        Path::new("/data/private/secret.txt"),
        false,
        false
    ));
}

#[test]
fn attack_exact_match_does_not_leak_sibling_files() {
    // Pattern is exact match on a file — must not imply access to siblings.
    let perms = vec![fs_perm(
        FsAction::Read,
        "/data/public.txt",
        PermissionStatus::Granted,
    )];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(
        Path::new("/data/private.txt"),
        false,
        false
    ));
    // Also not directories that happen to start with the same name.
    assert!(!checker.can_read_path_silently(
        Path::new("/data/public.txt.bak"),
        false,
        false
    ));
}

#[test]
fn attack_empty_permission_list_denies_everything() {
    let checker = PermissionChecker::new(make_extension(), vec![]);
    // Even system paths, with no permission, must stay denied.
    assert!(!checker.can_read_path_silently(Path::new("/etc/passwd"), false, false));
    assert!(!checker.can_read_path_silently(Path::new("/"), false, false));
    assert!(!checker.can_read_path_silently(Path::new(""), false, false));
}

#[test]
fn attack_session_denial_overrides_wildcard_grant() {
    // A wildcard grant gives broad access, but the user can still veto a
    // specific path via session-denial. This prevents overly-eager grants
    // from being irrevocable mid-session.
    let perms = vec![fs_perm(FsAction::Read, "*", PermissionStatus::Granted)];
    let checker = PermissionChecker::new(make_extension(), perms);
    assert!(!checker.can_read_path_silently(Path::new("/any/path.txt"), false, true));
}
