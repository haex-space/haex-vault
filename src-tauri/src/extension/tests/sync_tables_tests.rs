//! Unit tests for the pure per-table permission resolution helpers in
//! [`crate::extension::sync_tables`].
//!
//! These exercise the deny-first precedence at the row-set level — the
//! filter at the call-site only treats a table as visible to an extension
//! when [`table_resolution`] returns `Some(PermissionStatus::Granted)`.
//!
//! The historical bug these guard against: the previous `iter().any(...)`
//! filter ignored `perm.status`, so a row with `status = Denied` matching
//! a table name was indistinguishable from a `Granted` row — letting sync
//! events leak for tables the extension was explicitly forbidden to see.

use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, PermissionStatus, ResourceType,
};
use crate::extension::sync_tables::{permission_allows_table, table_resolution};

fn db_perm(target: &str, status: PermissionStatus) -> ExtensionPermission {
    ExtensionPermission {
        id: format!("test-{target}-{}", status.as_str()),
        principal_id: "extension:test".to_string(),
        resource_type: ResourceType::Db,
        action: Action::Database(DbAction::Read),
        target: target.to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

// ---------- permission_allows_table ----------------------------------------

#[test]
fn permission_allows_table_rejects_non_db_resource() {
    let mut perm = db_perm("haex_logs", PermissionStatus::Granted);
    perm.resource_type = ResourceType::Fs;
    assert_eq!(permission_allows_table(&perm, "haex_logs"), None);
}

#[test]
fn permission_allows_table_wildcard_matches_any_table() {
    let perm = db_perm("*", PermissionStatus::Granted);
    assert_eq!(
        permission_allows_table(&perm, "any_table"),
        Some(PermissionStatus::Granted)
    );
}

#[test]
fn permission_allows_table_prefix_matches_table_with_prefix() {
    let perm = db_perm("haex_*", PermissionStatus::Granted);
    assert_eq!(
        permission_allows_table(&perm, "haex_logs"),
        Some(PermissionStatus::Granted)
    );
    assert_eq!(permission_allows_table(&perm, "other_table"), None);
}

#[test]
fn permission_allows_table_exact_target_matches_exact_table() {
    let perm = db_perm("haex_logs", PermissionStatus::Granted);
    assert_eq!(
        permission_allows_table(&perm, "haex_logs"),
        Some(PermissionStatus::Granted)
    );
    assert_eq!(permission_allows_table(&perm, "haex_logs_other"), None);
}

#[test]
fn permission_allows_table_returns_denied_status_when_matched() {
    // The helper returns the status as-is; it does NOT filter to `Granted`.
    let perm = db_perm("haex_logs", PermissionStatus::Denied);
    assert_eq!(
        permission_allows_table(&perm, "haex_logs"),
        Some(PermissionStatus::Denied)
    );
}

// ---------- table_resolution (deny-first precedence) -----------------------

#[test]
fn denied_row_blocks_matching_prefix_grant() {
    // Bug repro: Granted prefix `haex_*` + Denied exact `haex_logs`
    // → `haex_logs` MUST NOT be in the allowed set.
    let permissions = vec![
        db_perm("haex_*", PermissionStatus::Granted),
        db_perm("haex_logs", PermissionStatus::Denied),
    ];

    assert_eq!(
        table_resolution(&permissions, "haex_logs"),
        Some(PermissionStatus::Denied),
        "Denied row must beat broader Granted prefix"
    );
    // Sibling table only matched by the prefix → still granted.
    assert_eq!(
        table_resolution(&permissions, "haex_other_table"),
        Some(PermissionStatus::Granted)
    );
}

#[test]
fn granted_only_returns_granted() {
    let permissions = vec![db_perm("haex_*", PermissionStatus::Granted)];
    assert_eq!(
        table_resolution(&permissions, "haex_logs"),
        Some(PermissionStatus::Granted)
    );
}

#[test]
fn denied_only_returns_denied_no_grant() {
    let permissions = vec![db_perm("haex_logs", PermissionStatus::Denied)];
    // No grant exists; the resolution reflects the deny, not a grant.
    assert_eq!(
        table_resolution(&permissions, "haex_logs"),
        Some(PermissionStatus::Denied)
    );
    // And the filter's allow-check (Some(Granted)) would reject it.
    assert_ne!(
        table_resolution(&permissions, "haex_logs"),
        Some(PermissionStatus::Granted)
    );
}

#[test]
fn wildcard_target_with_specific_deny() {
    let permissions = vec![
        db_perm("*", PermissionStatus::Granted),
        db_perm("secret_table", PermissionStatus::Denied),
    ];

    assert_eq!(
        table_resolution(&permissions, "secret_table"),
        Some(PermissionStatus::Denied)
    );
    assert_eq!(
        table_resolution(&permissions, "any_other_table"),
        Some(PermissionStatus::Granted)
    );
}

#[test]
fn deny_first_holds_regardless_of_row_order() {
    let denied_first = vec![
        db_perm("haex_logs", PermissionStatus::Denied),
        db_perm("haex_*", PermissionStatus::Granted),
    ];
    let granted_first = vec![
        db_perm("haex_*", PermissionStatus::Granted),
        db_perm("haex_logs", PermissionStatus::Denied),
    ];

    assert_eq!(
        table_resolution(&denied_first, "haex_logs"),
        Some(PermissionStatus::Denied)
    );
    assert_eq!(
        table_resolution(&granted_first, "haex_logs"),
        Some(PermissionStatus::Denied)
    );
}

#[test]
fn empty_permissions_returns_none() {
    let permissions: Vec<ExtensionPermission> = Vec::new();
    assert_eq!(table_resolution(&permissions, "haex_logs"), None);
}

#[test]
fn non_matching_permissions_returns_none() {
    let permissions = vec![db_perm("other_table", PermissionStatus::Granted)];
    assert_eq!(table_resolution(&permissions, "haex_logs"), None);
}

#[test]
fn non_db_permission_is_ignored_even_if_target_matches() {
    let mut perm = db_perm("haex_logs", PermissionStatus::Granted);
    perm.resource_type = ResourceType::Fs;
    let permissions = vec![perm];
    assert_eq!(table_resolution(&permissions, "haex_logs"), None);
}
