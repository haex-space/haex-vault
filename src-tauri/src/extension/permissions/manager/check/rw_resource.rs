use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::check::deny_first_precedence;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::session::SessionPermissionStore;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PermissionStatus, Principal, ResourceType, RwAction,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Gemeinsame Logik für die action-level Read/ReadWrite-Ressourcen
    /// (`SyncServers`, `CloudStorage`, `SyncRules`, `Bookmarks`). `target` ist
    /// immer "*"; es wird kein Sub-Target-Matching mehr durchgeführt.
    pub(super) async fn check_rw_resource_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: RwAction,
        resource_type: ResourceType,
        resource_label: &str,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        let action_str = action.as_str();

        match rw_resource_matching_status(&permissions, resource_type, action) {
            Some(PermissionStatus::Granted) => Ok(()),
            Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                extension_id,
                action_str,
                &format!("{resource_label}:*"),
            )),
            Some(PermissionStatus::Ask) => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                resource_label,
                action_str,
                "*",
            )),
            // No matching permission in database — check session permissions.
            None => match rw_resource_session_status(
                &app_state.session_permissions,
                extension_id,
                resource_type,
                action,
            ) {
                Some(PermissionStatus::Granted) => Ok(()),
                Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                    extension_id,
                    action_str,
                    &format!("{resource_label}:*"),
                )),
                // Session permission is Ask or no session entry — prompt the user.
                Some(PermissionStatus::Ask) | None => {
                    Err(ExtensionError::permission_prompt_required(
                        extension_id,
                        &extension.manifest.name,
                        resource_label,
                        action_str,
                        "*",
                    ))
                }
            },
        }
    }
}

/// Resolves the matching RW-resource permission status for `action` on
/// `resource_type` with **deny-first precedence**. Returns `None` when no
/// wildcard row matches.
///
/// A `ReadWrite` row implicitly covers a `Read` request (writer trivially
/// reads), mirroring the pre-refactor `action_allows` predicate. A
/// `ReadWrite` deny does NOT block a separately granted `Read` — denials are
/// exact-action to keep precedence symmetric with `database`/`spaces`.
///
/// These resources are wildcard-only by design: a sub-target row never
/// matches, even if present in the table.
///
/// Pure helper (no `State<AppState>`) so the deny-wins precedence is
/// unit-testable, mirroring `database_matching_status` /
/// `filesystem_matching_status` / `shell_matching_status` /
/// `web_matching_status` / `spaces_matching_status`.
pub(crate) fn rw_resource_matching_status(
    permissions: &[ExtensionPermission],
    resource_type: ResourceType,
    action: RwAction,
) -> Option<PermissionStatus> {
    deny_first_precedence(permissions.iter().filter_map(|perm| {
        if perm.resource_type != resource_type || perm.target != "*" {
            return None;
        }
        let matches = match perm.status {
            PermissionStatus::Denied => action_denies(&perm.action, resource_type, action),
            PermissionStatus::Granted | PermissionStatus::Ask => {
                action_allows(&perm.action, resource_type, action)
            }
        };
        if matches {
            Some(perm.status)
        } else {
            None
        }
    }))
}

/// Resolves the RW-resource session-permission status for `action` on
/// `resource_type`.
///
/// Implements **RW→R escalation**: when the request is `Read`, a session
/// `ReadWrite` grant is also probed before falling through to `None`. This
/// matches the DB-side `action_allows` semantics (writer trivially reads),
/// while leaving the generic [`SessionPermissionStore`] exact-match contract
/// untouched for other resources.
///
/// The escalation applies independently to `SyncServers`, `CloudStorage`,
/// `SyncRules`, and `Bookmarks` — each is keyed on its own [`Action`] variant.
///
/// Returns:
/// - `Some(Granted)` if the session has Granted for `action` (or `ReadWrite`
///   when `action == Read`).
/// - `Some(Denied)` if the session has Denied for `action`. The RW→R probe
///   never widens a denial — a session `Read`-specific Denied must NOT be
///   lifted by a separate `ReadWrite` entry.
/// - `None` otherwise.
pub(crate) fn rw_resource_session_status(
    session: &SessionPermissionStore,
    extension_id: &str,
    resource_type: ResourceType,
    action: RwAction,
) -> Option<PermissionStatus> {
    let session_action = make_action(resource_type, action);
    if session.is_granted(extension_id, &session_action, resource_type, "*") {
        return Some(PermissionStatus::Granted);
    }
    // RW->R escalation: a session ReadWrite grant implicitly covers Read.
    if action == RwAction::Read {
        let rw_action = make_action(resource_type, RwAction::ReadWrite);
        if session.is_granted(extension_id, &rw_action, resource_type, "*") {
            return Some(PermissionStatus::Granted);
        }
    }
    if session.is_denied(extension_id, &session_action, resource_type, "*") {
        return Some(PermissionStatus::Denied);
    }
    None
}

/// Builds the [`Action`] variant carrying `action` for `resource_type`.
///
/// Panics for non-RW resources — `check_rw_resource_permission` only ever
/// dispatches `SyncServers`/`CloudStorage`/`SyncRules`/`Bookmarks`.
fn make_action(resource_type: ResourceType, action: RwAction) -> Action {
    match resource_type {
        ResourceType::SyncServers => Action::SyncServers(action),
        ResourceType::CloudStorage => Action::CloudStorage(action),
        ResourceType::SyncRules => Action::SyncRules(action),
        ResourceType::Bookmarks => Action::Bookmarks(action),
        _ => unreachable!("rw_resource helpers only handle RW resources"),
    }
}

/// `true` iff `perm_action` (carried by a row of `resource_type`) grants
/// `required`. A `ReadWrite` permission implicitly covers a `Read` request.
fn action_allows(perm_action: &Action, resource_type: ResourceType, required: RwAction) -> bool {
    match perm_rw_action(perm_action, resource_type) {
        Some(a) if a == required => true,
        Some(RwAction::ReadWrite) if required == RwAction::Read => true,
        _ => false,
    }
}

/// `true` iff `perm_action` (carried by a row of `resource_type`) denies
/// `required`. Denials are **exact-action**: a `ReadWrite` deny must not
/// block a separately granted `Read`.
fn action_denies(perm_action: &Action, resource_type: ResourceType, required: RwAction) -> bool {
    perm_rw_action(perm_action, resource_type) == Some(required)
}

/// Extracts the [`RwAction`] from `perm_action` when it belongs to
/// `resource_type`. Returns `None` for unrelated `Action` variants.
fn perm_rw_action(perm_action: &Action, resource_type: ResourceType) -> Option<RwAction> {
    match (resource_type, perm_action) {
        (ResourceType::SyncServers, Action::SyncServers(a)) => Some(*a),
        (ResourceType::CloudStorage, Action::CloudStorage(a)) => Some(*a),
        (ResourceType::SyncRules, Action::SyncRules(a)) => Some(*a),
        (ResourceType::Bookmarks, Action::Bookmarks(a)) => Some(*a),
        _ => None,
    }
}
