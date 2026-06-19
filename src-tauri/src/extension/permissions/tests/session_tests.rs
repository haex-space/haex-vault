// src-tauri/src/extension/permissions/tests/session_tests.rs
//!
//! Unit tests for the in-memory session permission store. The key invariant
//! under test is that session grants are scoped by `action` — an "allow once"
//! Read must never satisfy a later Write check on the same target.

use crate::extension::permissions::session::SessionPermissionStore;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, IdentityAction, PermissionStatus, ResourceType, RwAction,
};

fn perm(
    action: Action,
    resource_type: ResourceType,
    status: PermissionStatus,
) -> ExtensionPermission {
    ExtensionPermission {
        id: uuid::Uuid::new_v4().to_string(),
        principal_id: "test_ext".to_string(),
        resource_type,
        action,
        target: "*".to_string(),
        constraints: None,
        status,
        raw_constraints: None,
    }
}

#[test]
fn read_grant_does_not_satisfy_write_check() {
    let store = SessionPermissionStore::new();
    store.set_permission(perm(
        Action::Identities(IdentityAction::Read),
        ResourceType::Identities,
        PermissionStatus::Granted,
    ));

    // The Read grant is honored only for Read.
    assert!(store.is_granted(
        "test_ext",
        &Action::Identities(IdentityAction::Read),
        ResourceType::Identities,
        "*",
    ));
    // It must NOT leak into a Write check — the whole point of keying by action.
    assert!(!store.is_granted(
        "test_ext",
        &Action::Identities(IdentityAction::Write),
        ResourceType::Identities,
        "*",
    ));
}

#[test]
fn distinct_actions_coexist_for_same_resource_and_target() {
    let store = SessionPermissionStore::new();
    store.set_permission(perm(
        Action::SyncServers(RwAction::Read),
        ResourceType::SyncServers,
        PermissionStatus::Granted,
    ));
    store.set_permission(perm(
        Action::SyncServers(RwAction::ReadWrite),
        ResourceType::SyncServers,
        PermissionStatus::Denied,
    ));

    assert!(store.is_granted(
        "test_ext",
        &Action::SyncServers(RwAction::Read),
        ResourceType::SyncServers,
        "*",
    ));
    assert!(store.is_denied(
        "test_ext",
        &Action::SyncServers(RwAction::ReadWrite),
        ResourceType::SyncServers,
        "*",
    ));
}

#[test]
fn remove_for_target_clears_every_action() {
    let store = SessionPermissionStore::new();
    store.set_permission(perm(
        Action::Identities(IdentityAction::Read),
        ResourceType::Identities,
        PermissionStatus::Granted,
    ));
    store.set_permission(perm(
        Action::Identities(IdentityAction::Write),
        ResourceType::Identities,
        PermissionStatus::Granted,
    ));

    store.remove_permissions_for_target("test_ext", ResourceType::Identities, "*");

    assert!(!store.is_granted(
        "test_ext",
        &Action::Identities(IdentityAction::Read),
        ResourceType::Identities,
        "*",
    ));
    assert!(!store.is_granted(
        "test_ext",
        &Action::Identities(IdentityAction::Write),
        ResourceType::Identities,
        "*",
    ));
}
