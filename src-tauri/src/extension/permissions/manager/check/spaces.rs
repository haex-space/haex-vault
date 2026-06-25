use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::check::deny_first_precedence;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::session::SessionPermissionStore;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PermissionStatus, Principal, ResourceType, SpaceAction,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft Shared-Spaces-Berechtigungen.
    /// Read = Spaces lesen/anzeigen, ReadWrite = zusätzlich Spaces anlegen.
    pub async fn check_spaces_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: SpaceAction,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        let action_str = match action {
            SpaceAction::Read => "read",
            SpaceAction::ReadWrite => "readWrite",
        };

        match spaces_matching_status(&permissions, action.clone()) {
            Some(PermissionStatus::Granted) => Ok(()),
            Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                extension_id,
                action_str,
                "spaces:*",
            )),
            Some(PermissionStatus::Ask) => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "spaces",
                action_str,
                "*",
            )),
            // No matching permission in database - check session permissions
            None => match spaces_session_status(
                &app_state.session_permissions,
                extension_id,
                action.clone(),
            ) {
                Some(PermissionStatus::Granted) => Ok(()),
                Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                    extension_id,
                    action_str,
                    "spaces:*",
                )),
                // Session permission is Ask or no session entry — prompt the user.
                Some(PermissionStatus::Ask) | None => {
                    Err(ExtensionError::permission_prompt_required(
                        extension_id,
                        &extension.manifest.name,
                        "spaces",
                        action_str,
                        "*",
                    ))
                }
            },
        }
    }
}

/// Resolves the matching spaces permission status for `action` with
/// **deny-first precedence**. Returns `None` when no spaces row matches.
///
/// A `ReadWrite` row implicitly covers a `Read` request (writer trivially
/// reads), mirroring the pre-refactor `action_allows` predicate.
///
/// Pure helper (no `State<AppState>`) so the deny-wins precedence is
/// unit-testable, mirroring `database_matching_status` /
/// `filesystem_matching_status` / `shell_matching_status` /
/// `web_matching_status`.
pub(crate) fn spaces_matching_status(
    permissions: &[ExtensionPermission],
    action: SpaceAction,
) -> Option<PermissionStatus> {
    deny_first_precedence(
        permissions
            .iter()
            .filter(|perm| {
                perm.resource_type == ResourceType::Spaces && action_allows(&perm.action, &action)
            })
            .map(|perm| perm.status),
    )
}

/// Resolves the spaces session-permission status for `action`.
///
/// Implements **RW→R escalation**: when the request is `Read`, a session
/// `ReadWrite` grant is also probed before falling through to `None`. This
/// matches the DB-side `action_allows` semantics (writer trivially reads),
/// while leaving the generic [`SessionPermissionStore`] exact-match contract
/// untouched for other resources.
///
/// Returns:
/// - `Some(Granted)` if the session has Granted for `action` (or `ReadWrite`
///   when `action == Read`).
/// - `Some(Denied)` if the session has Denied for `action`. The RW→R probe
///   never widens a denial — DB rows are exact-match by design, and a session
///   `Read`-specific Denied must NOT be lifted by a separate `ReadWrite`
///   entry. Denial probing therefore stays on the requested action only.
/// - `None` otherwise.
pub(crate) fn spaces_session_status(
    session: &SessionPermissionStore,
    extension_id: &str,
    action: SpaceAction,
) -> Option<PermissionStatus> {
    if session.is_granted(
        extension_id,
        &Action::Spaces(action.clone()),
        ResourceType::Spaces,
        "*",
    ) {
        return Some(PermissionStatus::Granted);
    }
    // RW->R escalation: a session ReadWrite grant implicitly covers Read.
    if action == SpaceAction::Read
        && session.is_granted(
            extension_id,
            &Action::Spaces(SpaceAction::ReadWrite),
            ResourceType::Spaces,
            "*",
        )
    {
        return Some(PermissionStatus::Granted);
    }
    if session.is_denied(
        extension_id,
        &Action::Spaces(action),
        ResourceType::Spaces,
        "*",
    ) {
        return Some(PermissionStatus::Denied);
    }
    None
}

/// `true` iff `perm_action` grants `required`. A `ReadWrite` permission
/// implicitly covers a `Read` request (writer trivially reads).
fn action_allows(perm_action: &Action, required: &SpaceAction) -> bool {
    match perm_action {
        Action::Spaces(space_action) => match (space_action, required) {
            (a, b) if a == b => true,
            (SpaceAction::ReadWrite, SpaceAction::Read) => true,
            _ => false,
        },
        _ => false,
    }
}
