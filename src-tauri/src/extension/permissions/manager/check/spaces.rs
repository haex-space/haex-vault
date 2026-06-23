use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, PermissionStatus, Principal, ResourceType, SpaceAction,
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

        let action_allows = |perm_action: &Action, required: &SpaceAction| -> bool {
            match perm_action {
                Action::Spaces(space_action) => match (space_action, required) {
                    (a, b) if a == b => true,
                    (SpaceAction::ReadWrite, SpaceAction::Read) => true,
                    _ => false,
                },
                _ => false,
            }
        };

        let matching_permission = permissions.iter().find(|perm| {
            perm.resource_type == ResourceType::Spaces && action_allows(&perm.action, &action)
        });

        let action_str = match action {
            SpaceAction::Read => "read",
            SpaceAction::ReadWrite => "readWrite",
        };

        match matching_permission {
            Some(perm) => match perm.status {
                PermissionStatus::Granted => Ok(()),
                PermissionStatus::Denied => Err(ExtensionError::permission_denied(
                    extension_id,
                    action_str,
                    "spaces:*",
                )),
                PermissionStatus::Ask => Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "spaces",
                    action_str,
                    "*",
                )),
            },
            None => {
                if app_state.session_permissions.is_granted(
                    extension_id,
                    &Action::Spaces(action.clone()),
                    ResourceType::Spaces,
                    "*",
                ) {
                    return Ok(());
                }
                if app_state.session_permissions.is_denied(
                    extension_id,
                    &Action::Spaces(action.clone()),
                    ResourceType::Spaces,
                    "*",
                ) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        action_str,
                        "spaces:*",
                    ));
                }

                Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "spaces",
                    action_str,
                    "*",
                ))
            }
        }
    }
}
