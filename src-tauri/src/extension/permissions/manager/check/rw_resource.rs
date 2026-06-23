use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PermissionStatus, Principal, ResourceType, RwAction,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Gemeinsame Logik für die action-level Read/ReadWrite-Ressourcen
    /// (`SyncServers`, `CloudStorage`, `SyncRules`). `target` ist immer "*";
    /// es wird kein Sub-Target-Matching mehr durchgeführt.
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

        // The matching RwAction variant for this resource, by value (`None` for
        // any other resource/action shape).
        let perm_rw_action = |perm_action: &Action| -> Option<RwAction> {
            match (resource_type, perm_action) {
                (ResourceType::SyncServers, Action::SyncServers(a)) => Some(*a),
                (ResourceType::CloudStorage, Action::CloudStorage(a)) => Some(*a),
                (ResourceType::SyncRules, Action::SyncRules(a)) => Some(*a),
                _ => None,
            }
        };
        // Grants escalate: a `ReadWrite` grant covers a `Read` request.
        let action_allows = |perm_action: &Action| -> bool {
            match perm_rw_action(perm_action) {
                Some(a) if a == action => true,
                Some(RwAction::ReadWrite) if action == RwAction::Read => true,
                _ => false,
            }
        };
        // Denies are exact: a `ReadWrite` deny must not block a separately
        // granted `Read`.
        let action_denies =
            |perm_action: &Action| -> bool { perm_rw_action(perm_action) == Some(action) };
        // These resources are wildcard-only — a sub-target row never matches.
        let is_wildcard =
            |perm: &&ExtensionPermission| perm.resource_type == resource_type && perm.target == "*";

        let action_str = action.as_str();

        // DB permissions, deny-first: an explicit deny wins over any grant.
        if permissions
            .iter()
            .filter(is_wildcard)
            .any(|perm| perm.status == PermissionStatus::Denied && action_denies(&perm.action))
        {
            return Err(ExtensionError::permission_denied(
                extension_id,
                action_str,
                &format!("{resource_label}:*"),
            ));
        }
        if permissions
            .iter()
            .filter(is_wildcard)
            .any(|perm| perm.status == PermissionStatus::Granted && action_allows(&perm.action))
        {
            return Ok(());
        }
        if permissions
            .iter()
            .filter(is_wildcard)
            .any(|perm| perm.status == PermissionStatus::Ask && action_allows(&perm.action))
        {
            return Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                resource_label,
                action_str,
                "*",
            ));
        }

        // No matching DB permission — check session permissions. Map the RW
        // action onto its `Action` variant for the session key (target "*").
        let session_action = match resource_type {
            ResourceType::SyncServers => Action::SyncServers(action),
            ResourceType::CloudStorage => Action::CloudStorage(action),
            ResourceType::SyncRules => Action::SyncRules(action),
            _ => unreachable!("check_rw_resource_permission only handles RW resources"),
        };
        if app_state.session_permissions.is_granted(
            extension_id,
            &session_action,
            resource_type,
            "*",
        ) {
            return Ok(());
        }
        if app_state.session_permissions.is_denied(
            extension_id,
            &session_action,
            resource_type,
            "*",
        ) {
            return Err(ExtensionError::permission_denied(
                extension_id,
                action_str,
                &format!("{resource_label}:*"),
            ));
        }

        Err(ExtensionError::permission_prompt_required(
            extension_id,
            &extension.manifest.name,
            resource_label,
            action_str,
            "*",
        ))
    }
}
