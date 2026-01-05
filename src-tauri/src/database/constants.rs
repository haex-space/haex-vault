// src-tauri/src/database/constants.rs
//
// Database constants for vault settings
// IMPORTANT: These values must match the TypeScript constants in src/stores/vault/settings.ts
// Run `pnpm test:constants` to verify synchronization

/// Vault settings type values
pub mod vault_settings_type {
    pub const SETTINGS: &str = "settings";
    pub const SYSTEM: &str = "system";
}

/// Vault settings key values (all snake_case for consistency)
pub mod vault_settings_key {
    pub const LOCALE: &str = "locale";
    pub const THEME: &str = "theme";
    pub const VAULT_NAME: &str = "vault_name";
    pub const VAULT_ID: &str = "vault_id";
    pub const DESKTOP_ICON_SIZE: &str = "desktop_icon_size";
    pub const TOMBSTONE_RETENTION_DAYS: &str = "tombstone_retention_days";
    pub const EXTERNAL_BRIDGE_PORT: &str = "external_bridge_port";
    pub const INITIAL_SYNC_COMPLETE: &str = "initial_sync_complete";
    pub const TRIGGERS_INITIALIZED: &str = "triggers_initialized";
    pub const TRIGGER_VERSION: &str = "trigger_version";
    pub const GRADIENT_VARIANT: &str = "gradient_variant";
    pub const GRADIENT_ENABLED: &str = "gradient_enabled";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that exports the constants as JSON for cross-language verification
    /// This test outputs JSON that can be compared with TypeScript constants
    #[test]
    fn export_constants_as_json() {
        let types = serde_json::json!({
            "settings": vault_settings_type::SETTINGS,
            "system": vault_settings_type::SYSTEM,
        });

        let keys = serde_json::json!({
            "locale": vault_settings_key::LOCALE,
            "theme": vault_settings_key::THEME,
            "vaultName": vault_settings_key::VAULT_NAME,
            "vaultId": vault_settings_key::VAULT_ID,
            "desktopIconSize": vault_settings_key::DESKTOP_ICON_SIZE,
            "tombstoneRetentionDays": vault_settings_key::TOMBSTONE_RETENTION_DAYS,
            "externalBridgePort": vault_settings_key::EXTERNAL_BRIDGE_PORT,
            "initialSyncComplete": vault_settings_key::INITIAL_SYNC_COMPLETE,
            "triggersInitialized": vault_settings_key::TRIGGERS_INITIALIZED,
            "triggerVersion": vault_settings_key::TRIGGER_VERSION,
            "gradientVariant": vault_settings_key::GRADIENT_VARIANT,
            "gradientEnabled": vault_settings_key::GRADIENT_ENABLED,
        });

        let output = serde_json::json!({
            "vaultSettingsType": types,
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
