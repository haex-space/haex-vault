// src-tauri/src/database/constants.rs
//
// Database constants for vault settings
// IMPORTANT: These values must match the TypeScript constants in src/stores/vault/settings.ts
// Run `pnpm test:constants` to verify synchronization

/// Vault settings key values (all snake_case for consistency)
#[allow(dead_code)]
pub mod vault_settings_key {
    pub const LOCALE: &str = "locale";
    pub const THEME: &str = "theme";
    pub const VAULT_NAME: &str = "vault_name";
    pub const SPACE_ID: &str = "space_id";
    pub const DESKTOP_ICON_SIZE: &str = "desktop_icon_size";
    pub const TOMBSTONE_RETENTION_DAYS: &str = "tombstone_retention_days";
    pub const EXTERNAL_BRIDGE_PORT: &str = "external_bridge_port";
    pub const INITIAL_SYNC_COMPLETE: &str = "initial_sync_complete";
    pub const TRIGGERS_INITIALIZED: &str = "triggers_initialized";
    pub const TRIGGER_VERSION: &str = "trigger_version";
    pub const GRADIENT_VARIANT: &str = "gradient_variant";
    pub const GRADIENT_ENABLED: &str = "gradient_enabled";
    /// 32-byte secret (hex) used to encrypt the Ed25519 device key file in the app data directory.
    /// Generated once at vault creation, shared across devices via CRDT sync.
    pub const DEVICE_KEY_SECRET: &str = "device_key_secret";
    pub const PEER_STORAGE_RELAY_URL: &str = "peer_storage_relay_url";

    /// Prefix for the per-space, per-device CRDT push cursor used by local
    /// space delivery (`space_delivery::local::sync_loop`). The full key is
    /// `local_sync_push_hlc:<space_id>` and the row is scoped to the local
    /// `device_id` via the `(key, device_id)` unique index. The value is the
    /// max HLC string of the last successfully pushed chunk; the next sync
    /// loop session resumes from there instead of re-scanning from t=0.
    pub const LOCAL_SYNC_PUSH_HLC_PREFIX: &str = "local_sync_push_hlc:";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that exports the constants as JSON for cross-language verification
    /// This test outputs JSON that can be compared with TypeScript constants
    #[test]
    fn export_constants_as_json() {
        let keys = serde_json::json!({
            "locale": vault_settings_key::LOCALE,
            "theme": vault_settings_key::THEME,
            "vaultName": vault_settings_key::VAULT_NAME,
            "spaceId": vault_settings_key::SPACE_ID,
            "desktopIconSize": vault_settings_key::DESKTOP_ICON_SIZE,
            "tombstoneRetentionDays": vault_settings_key::TOMBSTONE_RETENTION_DAYS,
            "externalBridgePort": vault_settings_key::EXTERNAL_BRIDGE_PORT,
            "initialSyncComplete": vault_settings_key::INITIAL_SYNC_COMPLETE,
            "triggersInitialized": vault_settings_key::TRIGGERS_INITIALIZED,
            "triggerVersion": vault_settings_key::TRIGGER_VERSION,
            "gradientVariant": vault_settings_key::GRADIENT_VARIANT,
            "gradientEnabled": vault_settings_key::GRADIENT_ENABLED,
            "deviceKeySecret": vault_settings_key::DEVICE_KEY_SECRET,
        });

        let output = serde_json::json!({
            "vaultSettingsKey": keys,
        });

        // Write to file for TypeScript test to read
        let out_path = std::env::var("CARGO_TARGET_DIR")
            .unwrap_or_else(|_| "target".to_string());
        let file_path = format!("{}/rust_constants.json", out_path);
        std::fs::write(&file_path, serde_json::to_string_pretty(&output).unwrap())
            .expect("Failed to write constants file");

        println!("Constants exported to: {}", file_path);
    }
}
