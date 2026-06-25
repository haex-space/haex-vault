use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{Principal, ResourceType, RwAction};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft Sync-Home-Server-Berechtigungen (`haex_sync_backends`).
    ///
    /// Action-level wie `check_spaces_permission`; `target` ist immer "*".
    /// Read = lesen/auflisten, ReadWrite = zusätzlich anlegen/ändern/löschen.
    #[allow(dead_code)]
    pub async fn check_sync_servers_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: RwAction,
    ) -> Result<(), ExtensionError> {
        Self::check_rw_resource_permission(
            app_state,
            principal,
            action,
            ResourceType::SyncServers,
            "syncServers",
        )
        .await
    }

    /// Prüft Cloud-Storage-Berechtigungen (`haex_storage_backends`, S3/WebDAV).
    ///
    /// Action-level wie `check_spaces_permission`; `target` ist immer "*".
    /// Read = lesen/auflisten, ReadWrite = zusätzlich anlegen/ändern/löschen.
    pub async fn check_cloud_storage_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: RwAction,
    ) -> Result<(), ExtensionError> {
        Self::check_rw_resource_permission(
            app_state,
            principal,
            action,
            ResourceType::CloudStorage,
            "cloudStorage",
        )
        .await
    }

    /// Prüft Sync-Rules-Berechtigungen (`haex_sync_rules`).
    ///
    /// Action-level wie `check_spaces_permission`; `target` ist immer "*".
    /// Read = lesen/auflisten, ReadWrite = zusätzlich anlegen/ändern/löschen.
    #[allow(dead_code)]
    pub async fn check_sync_rules_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: RwAction,
    ) -> Result<(), ExtensionError> {
        Self::check_rw_resource_permission(
            app_state,
            principal,
            action,
            ResourceType::SyncRules,
            "syncRules",
        )
        .await
    }
}
