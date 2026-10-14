//! Row and answer types of the password manager (spec 034, data-model.md §Rust-Typen). They are
//! exported to the frontend with ts-rs (`src/types/bindings/`, `pnpm generate:ts-types`).
//!
//! Types that carry a secret ([`ItemInput`], [`ItemPatch`], [`KeyValueInput`], [`KeyValuePatch`],
//! [`RevealedSecret`]) implement `Debug` by hand and print `<redacted>` for the secret fields, so a
//! log line or an error built with `{:?}` can never carry a value (FR-040, rule Z10).
//!
//! Maintainability exception (spaex 500-LoC rule): the file mirrors data-model.md §Rust-Typen one
//! type at a time and keeps its small deserialization/default implementations alongside the
//! redacting `Debug` impls. Concrete split plan, if this grows further: move the import types
//! (`AttentionKind`, `AttentionRow`, `ImportReport`, `ImportPreview`) into `model_import.rs` and
//! the history types (`SnapshotHeader`, `SnapshotView`, `SnapshotKeyValueView`,
//! `SnapshotAttachmentView`, `HistorySecret`, `RestoreOutcome`) into `model_history.rs`, as
//! `model_references.rs` already does.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zeroize::Zeroizing;

/// A tag as the lists show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct TagRef {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

/// The view of an entry without secrets that lists deliver (FR-026). No password, TOTP secret,
/// passkey key, custom field, note or attachment ever appears here; only whether they exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ItemHeader {
    pub id: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    /// `trash`, a folder id or `None` for the root.
    pub group_id: Option<String>,
    /// For an entry deleted directly: the folder it came from (the trash shows its path); `None`
    /// at the top level and for everything not deleted directly.
    pub trashed_from_group_id: Option<String>,
    pub tags: Vec<TagRef>,
    /// `YYYY-MM-DD`.
    pub expires_at: Option<String>,
    pub has_password: bool,
    pub has_totp: bool,
    pub passkey_count: u32,
    pub attachment_count: u32,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// The holzi function the entry belongs to (spec 038, rule Z14), `None` for the user's own; the
    /// window marks such entries.
    pub owner: Option<String>,
}

/// Whether the stored TOTP secret can produce a code. A value that arrived through sync or an
/// import can be invalid (FR-003); the entry then shows a message instead of a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum OtpState {
    None,
    Valid,
    Invalid,
}

/// A custom field in the detail view: its name and its stored value with the placeholders as they
/// are. Custom values show unmasked in the window (amends spec 034 FR-005); a placeholder's
/// resolved value still comes only through `passwords_reveal`.
#[derive(Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct KeyValueView {
    pub id: String,
    pub key: Option<String>,
    pub value: Option<String>,
    pub has_value: bool,
}

/// Prints no value: a custom field may hold a PIN (R7).
impl fmt::Debug for KeyValueView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyValueView")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("value", &redacted(&self.value))
            .field("has_value", &self.has_value)
            .finish()
    }
}

/// An attachment of an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AttachmentView {
    pub id: String,
    pub file_name: String,
    #[ts(type = "number")]
    pub size: u64,
    pub binary_hash: String,
}

/// A passkey of an entry without its keys (FR-004); spec 036 moved it to `model_passkeys.rs`.
pub use super::model_passkeys::PasskeyView;

/// The detail view of one entry, without secrets: `has_*` flags tell what exists, the values come
/// only through `reveal`, `copy_field` and `totp_code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    #[serde(flatten)]
    #[ts(flatten)]
    pub header: ItemHeader,
    pub note: Option<String>,
    /// JSON `{ "username": [..], … }`; the password manager only keeps it for the later bridge.
    pub autofill_aliases: Option<String>,
    pub otp_digits: Option<u32>,
    pub otp_period: Option<u32>,
    pub otp_algorithm: Option<String>,
    pub has_otp_secret: bool,
    pub otp_state: OtpState,
    pub key_values: Vec<KeyValueView>,
    pub attachments: Vec<AttachmentView>,
    pub passkeys: Vec<PasskeyView>,
    /// The placeholders in the reference fields (spec 036); filled by the service.
    pub references: super::model_references::ItemReferences,
}

/// What the built-in agent may see of an entry (FR-027): title, tag names, folder name and whether
/// a TOTP exists. No username, no URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AgentHeader {
    pub id: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub folder: Option<String>,
    pub has_totp: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
    pub parent_id: Option<String>,
    pub trashed_from_parent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct TagRow {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub item_count: u32,
}

/// One state in the history of an entry, with the names of the fields that changed against the
/// state before it (values of secret fields only through `history_reveal`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHeader {
    pub id: String,
    pub item_id: String,
    pub modified_at: Option<String>,
    pub changed_fields: Vec<String>,
    pub attachment_count: u32,
}

/// What an import could not bring over unchanged (contracts/import-mapping.md §Bericht).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    AttachmentTooLarge,
    AttachmentUnreadable,
    PasskeyKeyUnreadable,
    PasskeyPublicKeyMissing,
    PasskeyDuplicate,
    TotpInvalid,
    IconNotMapped,
    ValueNotStorable,
    SourceSetting,
    HistoryUnreadable,
    GroupReparented,
    TagMerged,
    UnknownSourceData,
}

/// One place where the user has to rework by hand: entry, folder path and what is missing; never a
/// value of a secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AttentionRow {
    /// The entry the place belongs to, so the window can open it; `None` for the source as a whole.
    pub item_id: Option<String>,
    pub title: String,
    pub folder_path: String,
    pub kind: AttentionKind,
    pub field: Option<String>,
    pub file_name: Option<String>,
    pub size_mib: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: u32,
    pub trashed: u32,
    pub history_states: u32,
    pub skipped_duplicates: u32,
    /// KeePass references that became holzi placeholders (spec 036, FR-050).
    pub references_converted: u32,
    /// KeePass references that stayed text (no unique source, or a field holzi cannot point at).
    pub references_left_as_text: u32,
    pub needs_attention: Vec<AttentionRow>,
}

/// What an import would bring over, counted before anything is written (FR-023). `warnings` are
/// the kinds of [`AttentionKind`] that occur, as their snake_case names; the window gives each a
/// text. Never a value of a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub entries: u32,
    pub groups: u32,
    pub trashed_entries: u32,
    pub history_states: u32,
    pub attachments: u32,
    pub passkeys: u32,
    pub duplicates: u32,
    pub tags: u32,
    pub presets: u32,
    pub warnings: Vec<String>,
}

/// A custom field to create.
#[derive(Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueInput {
    pub key: String,
    pub value: Option<String>,
}

impl fmt::Debug for KeyValueInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyValueInput")
            .field("key", &self.key)
            .field("value", &redacted(&self.value))
            .finish()
    }
}

/// The data to create an entry with. Everything may be empty, also the title (FR-001).
#[derive(Clone, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemInput {
    pub title: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub note: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub expires_at: Option<String>,
    /// A secret or an `otpauth://` address; normalised and checked on create (FR-003).
    pub otp_secret: Option<String>,
    pub otp_digits: Option<u32>,
    pub otp_period: Option<u32>,
    pub otp_algorithm: Option<String>,
    pub autofill_aliases: Option<String>,
    pub tags: Vec<String>,
    pub key_values: Vec<KeyValueInput>,
}

impl fmt::Debug for ItemInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemInput")
            .field("title", &self.title)
            .field("username", &self.username)
            .field("password", &redacted(&self.password))
            .field("url", &self.url)
            .field("otp_secret", &redacted(&self.otp_secret))
            .field("tags", &self.tags)
            .field("key_values", &self.key_values)
            .finish_non_exhaustive()
    }
}

pub use crate::patch::Patch;

/// A custom field in a partial update: `id` names an existing field, a missing `value` keeps its
/// stored value, a row with an empty key is dropped.
#[derive(Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct KeyValuePatch {
    pub id: Option<String>,
    pub key: String,
    pub value: Option<String>,
}

impl fmt::Debug for KeyValuePatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyValuePatch")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("value", &redacted(&self.value))
            .finish()
    }
}

/// A partial update of an entry. A field that is absent stays as it is.
#[derive(Clone, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase", default)]
pub struct ItemPatch {
    #[ts(type = "string | null", optional)]
    pub title: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub username: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub password: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub note: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub url: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub icon: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub color: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub expires_at: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub otp_secret: Patch<String>,
    #[ts(type = "number | null", optional)]
    pub otp_digits: Patch<u32>,
    #[ts(type = "number | null", optional)]
    pub otp_period: Patch<u32>,
    #[ts(type = "string | null", optional)]
    pub otp_algorithm: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub autofill_aliases: Patch<String>,
    /// Tag names; replaces the set when present.
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
    /// Replaces the custom fields when present.
    #[ts(optional)]
    pub key_values: Option<Vec<KeyValuePatch>>,
}

impl fmt::Debug for ItemPatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemPatch")
            .field("title", &self.title)
            .field("username", &self.username)
            .field("password", &self.password)
            .field("otp_secret", &self.otp_secret)
            .field("tags", &self.tags)
            .field("key_values", &self.key_values)
            .finish_non_exhaustive()
    }
}

/// Which secret of an entry to reveal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SecretField {
    Password,
    OtpSecret,
    KeyValue {
        id: String,
    },
    /// User name, address and note are no secrets, but with a placeholder their resolved value can
    /// hold one (spec 036, FR-045, FR-047), so it is revealed like a secret.
    Username,
    Url,
    Note,
    /// The stored password with its placeholders unresolved, for the editor, which shows the
    /// password in a plain field and must write placeholders back as they are.
    StoredPassword,
}

/// A secret on its way to the user's eyes: the one type that serialises a value to the webview.
/// Zeroed when dropped; `Debug` prints no value.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct RevealedSecret {
    #[ts(type = "string")]
    pub value: Zeroizing<String>,
}

impl fmt::Debug for RevealedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RevealedSecret(<redacted>)")
    }
}

/// `Some(<redacted>)` or `None`: whether a secret is present, never what it is.
fn redacted(value: &Option<String>) -> Option<&'static str> {
    value.as_ref().map(|_| "<redacted>")
}

/// Everything the window needs for its store in one answer: every entry (also those in the trash,
/// the window separates them), every folder, every tag with its count. The user's overview only;
/// callers from outside get `list_headers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub headers: Vec<ItemHeader>,
    pub groups: Vec<GroupRow>,
    pub tags: Vec<TagRow>,
}

/// The field a text copied from the window comes from (`passwords_copy_text`): its placeholders
/// are resolved as in that field of the entry (spec 036, FR-045).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TextField {
    Username,
    Password,
    Url,
    Note,
    /// The custom field with this name.
    KeyValue {
        key: String,
    },
}

/// Which value `passwords_copy_field` puts on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CopyField {
    Username,
    Password,
    /// The current TOTP code, not the secret.
    Totp,
    KeyValue {
        id: String,
    },
    Url,
    Note,
}

/// The current TOTP code of an entry, computed in Rust (FR-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct TotpCode {
    pub code: String,
    pub remaining_seconds: u32,
    pub period: u32,
    pub digits: u32,
}

/// A custom field with its value, as the single-entry secret read delivers it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretKeyValue {
    pub id: String,
    pub key: Option<String>,
    pub value: Option<String>,
}

/// The whole entry with its secrets, the answer of the single-entry read for callers from outside
/// (FR-026: only on a request for one entry that a grant covers). `Debug` prints no value.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretItem {
    pub id: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub note: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub expires_at: Option<String>,
    pub otp_secret: Option<String>,
    pub otp_digits: Option<u32>,
    pub otp_period: Option<u32>,
    pub otp_algorithm: Option<String>,
    /// The stored JSON text of the autofill aliases.
    pub autofill_aliases: Option<String>,
    /// All tag names of the entry (rule Z12).
    pub tags: Vec<String>,
    pub key_values: Vec<SecretKeyValue>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl fmt::Debug for SecretItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretItem")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("secrets", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// What a bulk action works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    Item,
    Group,
}

/// An entry or a folder, named by its id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub kind: TargetKind,
    pub id: String,
}

/// A partial update of a folder: a field that is absent stays as it is, `null` clears it.
#[derive(Clone, Debug, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase", default)]
pub struct GroupPatch {
    #[ts(type = "string | null", optional)]
    pub name: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub description: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub icon: Patch<String>,
    #[ts(type = "string | null", optional)]
    pub color: Patch<String>,
    #[ts(type = "number | null", optional)]
    pub sort_order: Patch<i32>,
}

/// A saved generator configuration (FR-014). Exactly one preset is the default; with two flagged
/// ones (concurrent writes on two devices) the list shows only the newest as the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub length: u32,
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub symbols: bool,
    pub exclude_chars: String,
    pub use_pattern: bool,
    pub pattern: String,
    pub is_default: bool,
}

/// What the window sends to save a preset; an empty `id` creates one.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase", default)]
pub struct PresetInput {
    pub id: String,
    pub name: String,
    pub length: u32,
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub symbols: bool,
    pub exclude_chars: String,
    pub use_pattern: bool,
    pub pattern: String,
    pub is_default: bool,
}

impl Default for PresetInput {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            length: 16,
            uppercase: true,
            lowercase: true,
            numbers: true,
            symbols: true,
            exclude_chars: String::new(),
            use_pattern: false,
            pattern: String::new(),
            is_default: false,
        }
    }
}

/// A key-value of a history state in the masked view: the name, whether it has a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotKeyValueView {
    pub key: Option<String>,
    pub has_value: bool,
}

/// An attachment of a history state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotAttachmentView {
    pub file_name: String,
    pub binary_hash: String,
    /// Whether the binary data is still there (a restore skips the attachment if it is not).
    pub available: bool,
}

/// One history state of an entry without its secrets (FR-017): flags instead of the password, the
/// TOTP secret and the values of custom fields; the values come only through `history_reveal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotView {
    pub id: String,
    pub item_id: String,
    pub modified_at: Option<String>,
    pub title: Option<String>,
    pub username: Option<String>,
    pub url: Option<String>,
    pub note: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub expires_at: Option<String>,
    pub has_password: bool,
    pub has_otp_secret: bool,
    pub otp_digits: Option<u32>,
    pub otp_period: Option<u32>,
    pub otp_algorithm: Option<String>,
    pub tags: Vec<String>,
    pub key_values: Vec<SnapshotKeyValueView>,
    pub attachments: Vec<SnapshotAttachmentView>,
}

/// Which secret of a history state to reveal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HistorySecret {
    Password,
    OtpSecret,
    /// The custom field with this name (history states have no ids for them).
    KeyValue {
        key: String,
    },
}

/// What a restore of a history state did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub updated_at: String,
    /// File names of attachments whose binary data is gone and that were therefore not re-linked.
    pub skipped_attachments: Vec<String>,
}
