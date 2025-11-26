// Auto-generated from Drizzle schema
// DO NOT EDIT MANUALLY
// Run 'pnpm generate:rust-types' to regenerate

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexVaultSettings {
    pub id: String,
    pub key: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub haex_timestamp: Option<String>,
    pub haex_column_hlcs: String,
    pub haex_tombstone: bool,
}

impl HaexVaultSettings {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            key: row.get(1)?,
            r#type: row.get(2)?,
            value: row.get(3)?,
            haex_timestamp: row.get(4)?,
            haex_column_hlcs: row.get(5)?,
            haex_tombstone: row.get(6)?,
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
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub haex_timestamp: Option<String>,
    pub haex_column_hlcs: String,
    pub haex_tombstone: bool,
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
            created_at: row.get(13)?,
            updated_at: row.get(14)?,
            haex_timestamp: row.get(15)?,
            haex_column_hlcs: row.get(16)?,
            haex_tombstone: row.get(17)?,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub haex_timestamp: Option<String>,
    pub haex_column_hlcs: String,
    pub haex_tombstone: bool,
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
            haex_timestamp: row.get(9)?,
            haex_column_hlcs: row.get(10)?,
            haex_tombstone: row.get(11)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexCrdtConfigs {
    pub key: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub value: String,
}

impl HaexCrdtConfigs {
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
pub struct HaexDesktopItems {
    pub id: String,
    pub workspace_id: String,
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_window_id: Option<String>,
    pub position_x: i64,
    pub position_y: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub haex_timestamp: Option<String>,
    pub haex_column_hlcs: String,
    pub haex_tombstone: bool,
}

impl HaexDesktopItems {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            item_type: row.get(2)?,
            extension_id: row.get(3)?,
            system_window_id: row.get(4)?,
            position_x: row.get(5)?,
            position_y: row.get(6)?,
            haex_timestamp: row.get(7)?,
            haex_column_hlcs: row.get(8)?,
            haex_tombstone: row.get(9)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexWorkspaces {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub position: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub haex_timestamp: Option<String>,
    pub haex_column_hlcs: String,
    pub haex_tombstone: bool,
}

impl HaexWorkspaces {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            device_id: row.get(1)?,
            name: row.get(2)?,
            position: row.get(3)?,
            background: row.get(4)?,
            haex_timestamp: row.get(5)?,
            haex_column_hlcs: row.get(6)?,
            haex_tombstone: row.get(7)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaexCrdtMigrations {
    pub id: String,
    pub migration_name: String,
    pub migration_content: String,
    pub applied_at: String,
}

impl HaexCrdtMigrations {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            migration_name: row.get(1)?,
            migration_content: row.get(2)?,
            applied_at: row.get(3)?,
        })
    }
}

