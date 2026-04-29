// Auto-generated from Drizzle schema
// DO NOT EDIT MANUALLY
// Run 'pnpm generate:rust-types' to regenerate

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexVaultSettings {
    pub id: String,
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

impl HaexVaultSettings {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            key: row.get(1)?,
            value: row.get(2)?,
            device_id: row.get(3)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexExtensions {
    pub id: String,
    pub public_key: String,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_instance: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i18n: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl HaexExtensions {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            public_key: row.get(1)?,
            name: row.get(2)?,
            version: row.get(3)?,
            author: row.get(4)?,
            description: row.get(5)?,
            entry: row.get(6)?,
            homepage: row.get(7)?,
            enabled: row.get(8)?,
            icon: row.get(9)?,
            signature: row.get(10)?,
            single_instance: row.get(11)?,
            display_mode: row.get(12)?,
            i18n: row.get(13)?,
            dev_path: row.get(14)?,
            created_at: row.get(15)?,
            updated_at: row.get(16)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexExtensionPermissions {
    pub id: String,
    pub extension_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl HaexExtensionPermissions {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            extension_id: row.get(1)?,
            resource_type: row.get(2)?,
            action: row.get(3)?,
            target: row.get(4)?,
            constraints: row.get(5)?,
            status: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexCrdtConfigsNoSync {
    pub key: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub value: String,
}

impl HaexCrdtConfigsNoSync {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            key: row.get(0)?,
            r#type: row.get(1)?,
            value: row.get(2)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexDesktopItemsNoSync {
    pub id: String,
    pub workspace_id: String,
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_window_id: Option<String>,
    pub position_x: i64,
    pub position_y: i64,
}

impl HaexDesktopItemsNoSync {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            item_type: row.get(2)?,
            extension_id: row.get(3)?,
            system_window_id: row.get(4)?,
            position_x: row.get(5)?,
            position_y: row.get(6)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexWorkspacesNoSync {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub position: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

impl HaexWorkspacesNoSync {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            device_id: row.get(1)?,
            name: row.get(2)?,
            position: row.get(3)?,
            background: row.get(4)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexCrdtMigrationsNoSync {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_id: Option<String>,
    pub migration_name: String,
    pub migration_content: String,
    pub applied_at: String,
}

impl HaexCrdtMigrationsNoSync {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            extension_id: row.get(1)?,
            migration_name: row.get(2)?,
            migration_content: row.get(3)?,
            applied_at: row.get(4)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexExtensionLimits {
    pub id: String,
    pub extension_id: String,
    pub query_timeout_ms: i64,
    pub max_result_rows: i64,
    pub max_concurrent_queries: i64,
    pub max_query_size_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl HaexExtensionLimits {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            extension_id: row.get(1)?,
            query_timeout_ms: row.get(2)?,
            max_result_rows: row.get(3)?,
            max_concurrent_queries: row.get(4)?,
            max_query_size_bytes: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsItemDetails {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_digits: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_period: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_algorithm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autofill_aliases: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl HaexPasswordsItemDetails {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            title: row.get(1)?,
            username: row.get(2)?,
            password: row.get(3)?,
            note: row.get(4)?,
            icon: row.get(5)?,
            color: row.get(6)?,
            url: row.get(7)?,
            otp_secret: row.get(8)?,
            otp_digits: row.get(9)?,
            otp_period: row.get(10)?,
            otp_algorithm: row.get(11)?,
            expires_at: row.get(12)?,
            autofill_aliases: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsItemKeyValues {
    pub id: String,
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl HaexPasswordsItemKeyValues {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            item_id: row.get(1)?,
            key: row.get(2)?,
            value: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsGroups {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl HaexPasswordsGroups {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            icon: row.get(3)?,
            sort_order: row.get(4)?,
            color: row.get(5)?,
            parent_id: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsGroupItems {
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

impl HaexPasswordsGroupItems {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            item_id: row.get(0)?,
            group_id: row.get(1)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsBinaries {
    pub hash: String,
    pub data: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl HaexPasswordsBinaries {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            hash: row.get(0)?,
            data: row.get(1)?,
            size: row.get(2)?,
            r#type: row.get(3)?,
            created_at: row.get(4)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsItemBinaries {
    pub id: String,
    pub item_id: String,
    pub binary_hash: String,
    pub file_name: String,
}

impl HaexPasswordsItemBinaries {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            item_id: row.get(1)?,
            binary_hash: row.get(2)?,
            file_name: row.get(3)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsItemSnapshots {
    pub id: String,
    pub item_id: String,
    pub snapshot_data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl HaexPasswordsItemSnapshots {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            item_id: row.get(1)?,
            snapshot_data: row.get(2)?,
            created_at: row.get(3)?,
            modified_at: row.get(4)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsSnapshotBinaries {
    pub id: String,
    pub snapshot_id: String,
    pub binary_hash: String,
    pub file_name: String,
}

impl HaexPasswordsSnapshotBinaries {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            snapshot_id: row.get(1)?,
            binary_hash: row.get(2)?,
            file_name: row.get(3)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsGeneratorPresets {
    pub id: String,
    pub name: String,
    pub length: i64,
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub symbols: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_chars: Option<String>,
    pub use_pattern: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    pub is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl HaexPasswordsGeneratorPresets {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            length: row.get(2)?,
            uppercase: row.get(3)?,
            lowercase: row.get(4)?,
            numbers: row.get(5)?,
            symbols: row.get(6)?,
            exclude_chars: row.get(7)?,
            use_pattern: row.get(8)?,
            pattern: row.get(9)?,
            is_default: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsTags {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl HaexPasswordsTags {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            created_at: row.get(3)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsItemTags {
    pub id: String,
    pub item_id: String,
    pub tag_id: String,
}

impl HaexPasswordsItemTags {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            item_id: row.get(1)?,
            tag_id: row.get(2)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexPasswordsPasskeys {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub credential_id: String,
    pub relying_party_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relying_party_name: Option<String>,
    pub user_handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_display_name: Option<String>,
    pub private_key: String,
    pub public_key: String,
    pub algorithm: i64,
    pub sign_count: i64,
    pub is_discoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
}

impl HaexPasswordsPasskeys {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            item_id: row.get(1)?,
            credential_id: row.get(2)?,
            relying_party_id: row.get(3)?,
            relying_party_name: row.get(4)?,
            user_handle: row.get(5)?,
            user_name: row.get(6)?,
            user_display_name: row.get(7)?,
            private_key: row.get(8)?,
            public_key: row.get(9)?,
            algorithm: row.get(10)?,
            sign_count: row.get(11)?,
            is_discoverable: row.get(12)?,
            icon: row.get(13)?,
            color: row.get(14)?,
            nickname: row.get(15)?,
            created_at: row.get(16)?,
            last_used_at: row.get(17)?,
        })
    }
}

