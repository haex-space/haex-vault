use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, NotificationsAction, PermissionStatus, Principal, ResourceType,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft die generische Notifications-Berechtigung (`notifications.show`).
    ///
    /// Notifications sind nicht ressourcen-gescoped, daher ist `target` immer
    /// "*". Analog zu `check_spaces_permission`: Granted → Ok, Denied → Fehler,
    /// Ask/keine Permission → Prompt (Session-Grants haben Vorrang).
    pub async fn check_notifications_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: NotificationsAction,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        let action_matches = |perm_action: &Action| -> bool {
            matches!(perm_action, Action::Notifications(a) if *a == action)
        };

        let matching_permission = permissions.iter().find(|perm| {
            perm.resource_type == ResourceType::Notifications && action_matches(&perm.action)
        });

        match matching_permission {
            Some(perm) => match perm.status {
                PermissionStatus::Granted => Ok(()),
                PermissionStatus::Denied => Err(ExtensionError::permission_denied(
                    extension_id,
                    action.as_str(),
                    "notifications:*",
                )),
                PermissionStatus::Ask => Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "notifications",
                    action.as_str(),
                    "*",
                )),
            },
            None => {
                if app_state.session_permissions.is_granted(
                    extension_id,
                    &Action::Notifications(action),
                    ResourceType::Notifications,
                    "*",
                ) {
                    return Ok(());
                }
                if app_state.session_permissions.is_denied(
                    extension_id,
                    &Action::Notifications(action),
                    ResourceType::Notifications,
                    "*",
                ) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        action.as_str(),
                        "notifications:*",
                    ));
                }

                Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "notifications",
                    action.as_str(),
                    "*",
                ))
            }
        }
    }
}
