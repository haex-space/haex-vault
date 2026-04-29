//! Type definitions for the Core Passwords API.
//!
//! Design decisions (2026-04-19):
//!
//! 1. **No API-level type variant.** A "password" record has optional fields
//!    (password, OTP, attachments, custom fields). Extensions and UI read
//!    whichever fields are populated — no switch on a `kind` enum.
//!
//! 2. **Normalized DB schema preserved** from the haex-pass extension.
//!    Relations that are 1:N (passkeys, key-values, snapshots, binaries,
//!    item-tags) live in their own tables. This keeps CRDT merge semantics
//!    per-row and honors uniqueness (tag.name, passkey.credential_id,
//!    binary.hash).
//!
//! 3. **Plaintext columns.** The vault DB is encrypted at rest; no second
//!    envelope inside.
//!
//! 4. **Permission model (to be added in `extension/permissions/types.rs`
//!    when the bridge is wired):**
//!      - `PasswordsAction::{Read, ReadWrite}` mirrors `SpaceAction`.
//!      - Extensions scope access via TAG FILTER in the permission `target`
//!        field (e.g. `target="calendar"`). `target="*"` = all.
//!      - Write authorization (variant B): on create/update, the item must
//!        carry at least one tag inside the extension's permission scope.
//!        Core rejects writes that would place the item outside scope.
//!
//! 5. **Sharing only via Spaces.** Items are assigned to spaces via the
//!    existing `haex_shared_space_sync` mechanism. No per-secret UCAN.
//!
//! 6. **Groups vs. Tags are intentionally distinct:**
//!      - Groups = UI-only folders (1 item → 1 group, hierarchical).
//!      - Tags = logical classification (1 item → n tags), drives permission
//!        scope for extensions and user-facing filtering.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

// ============================================================================
// haex_passwords_items
// ============================================================================

/// Core password record. Row type for `haex_passwords_items`.
/// OTP fields are inline (1:1 relation with the item). All other relations
/// are separate tables (see below).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordItem {
    pub id: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub note: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub url: Option<String>,

    pub otp_secret: Option<String>,
    pub otp_digits: Option<u32>,
    pub otp_period: Option<u32>,
    pub otp_algorithm: Option<String>,

    /// Maps canonical field names to autofill aliases for browser matching.
    /// Stored as JSON text in the DB.
    /// e.g. `{ "username": ["email", "login"], "password": ["pass"] }`
    pub autofill_aliases: Option<HashMap<String, Vec<String>>>,

    pub expires_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

// ============================================================================
// haex_passwords_item_key_values  (1:N — one item, many user-defined fields)
// ============================================================================

/// User-defined extra field on an item (e.g. "Recovery Code", "PIN").
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordItemKeyValue {
    pub id: String,
    pub item_id: String,
    pub key: Option<String>,
    pub value: Option<String>,
    pub updated_at: Option<String>,
}

// ============================================================================
// haex_passwords_groups  (hierarchical UI-only folders)
// ============================================================================

/// **UI-only organization.** Folders for visual grouping.
/// Not used for extension permission scoping — that is the Tag's job.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordGroup {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
    pub parent_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

// ============================================================================
// haex_passwords_group_items  (1:1 — an item can only be in one group)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordGroupItem {
    pub item_id: String,
    pub group_id: Option<String>,
}

// ============================================================================
// haex_passwords_tags  (first-class tag entities, name UNIQUE)
// ============================================================================

/// Tags drive **logical classification**, including the permission scope
/// that extensions receive. First-class entities with color and unique name.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordTag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: Option<String>,
}

// ============================================================================
// haex_passwords_item_tags  (n:m junction)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordItemTag {
    pub id: String,
    pub item_id: String,
    pub tag_id: String,
}

// ============================================================================
// haex_passwords_binaries  (SHA-256-deduplicated blob storage)
// ============================================================================

/// Attachments and icons. Primary key is the SHA-256 hash, so identical
/// binaries across items/snapshots deduplicate automatically.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordBinary {
    pub hash: String,
    /// Base64-encoded binary data.
    pub data: String,
    pub size: i64,
    /// "attachment" or "icon".
    pub kind: Option<String>,
    pub created_at: Option<String>,
}

// ============================================================================
// haex_passwords_item_binaries  (n:m junction; per-item file name)
// ============================================================================

/// Links an item to a stored binary. File name may differ per-item even when
/// the underlying binary (hash) is shared.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordItemBinary {
    pub id: String,
    pub item_id: String,
    pub binary_hash: String,
    pub file_name: String,
}

// ============================================================================
// haex_passwords_item_snapshots  (KeePass-style history)
// ============================================================================

/// Point-in-time snapshot of an item. Contents serialized as JSON in
/// `snapshot_data` so the history format stays stable if `PasswordItem`
/// evolves later.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordItemSnapshot {
    pub id: String,
    pub item_id: String,
    /// JSON: serialized item state (excluding binaries — those are linked
    /// via `haex_passwords_snapshot_binaries`).
    pub snapshot_data: String,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
}

// ============================================================================
// haex_passwords_snapshot_binaries  (history-attachment links)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordSnapshotBinary {
    pub id: String,
    pub snapshot_id: String,
    pub binary_hash: String,
    pub file_name: String,
}

// ============================================================================
// haex_passwords_passkeys  (WebAuthn credentials — own lifecycle)
// ============================================================================

/// WebAuthn credential. Either linked to a `PasswordItem` (displayed inside
/// the corresponding login entry) or standalone (discoverable resident-key
/// passkey not tied to a classical login).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordPasskey {
    pub id: String,
    pub item_id: Option<String>,

    /// Unique per credential (WebAuthn requirement).
    pub credential_id: String,
    pub relying_party_id: String,
    pub relying_party_name: Option<String>,

    pub user_handle: String,
    pub user_name: Option<String>,
    pub user_display_name: Option<String>,

    /// PKCS8 format, Base64-encoded.
    pub private_key: String,
    /// SPKI format, Base64-encoded.
    pub public_key: String,

    /// COSE algorithm: -7 = ES256, -8 = EdDSA, -257 = RS256.
    pub algorithm: i32,

    /// Replay-protection counter. Incremented on every authentication use.
    pub sign_count: i64,

    pub is_discoverable: bool,

    pub icon: Option<String>,
    pub color: Option<String>,
    pub nickname: Option<String>,

    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
}

// ============================================================================
// haex_passwords_generator_presets  (user password-generator configs)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordGeneratorPreset {
    pub id: String,
    pub name: String,
    pub length: i32,
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub symbols: bool,
    pub exclude_chars: Option<String>,
    pub use_pattern: bool,
    pub pattern: Option<String>,
    pub is_default: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}
