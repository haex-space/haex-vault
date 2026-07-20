use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{Principal, ResourceType, RwAction};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft Bookmarks-Berechtigungen. Anders als `check_passwords_permission`
    /// gibt es keinen Tag-/Sammlungs-Scope: `bookmarks:read`/`readWrite` gilt
    /// für alle Sammlungen gleichermaßen — Sammlungstrennung ist Datenmodell-,
    /// kein Berechtigungskonzept. `target` ist daher immer `"*"`, wie bei den
    /// übrigen action-level RW-Ressourcen (siehe `check_rw_resource_permission`).
    pub async fn check_bookmarks_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: RwAction,
    ) -> Result<(), ExtensionError> {
        Self::check_rw_resource_permission(
            app_state,
            principal,
            action,
            ResourceType::Bookmarks,
            "bookmarks",
        )
        .await
    }
}
