//! Import into the password manager (spec 034, US7, FR-023, research R12): a parser per source
//! format turns the bytes of a file into an [`ImportModel`] (pure, no database), `apply` writes the
//! model in steps with a ledger for the rollback, and `report` tells what could not arrive as it
//! was. **Nothing is dropped silently**: a source property without a field of its own becomes a
//! custom field or a tag, and what cannot arrive at all is listed in the report, never with a value.
//! The mapping of every format is in `specs/034-password-manager/contracts/import-mapping.md`.

pub mod apply;
pub mod apply_extras;
pub mod apply_references;
pub mod bitwarden;
pub mod csv;
pub mod icons;
pub mod keepass;
pub mod lastpass;
pub mod legacy_haex_vault;
pub mod passkey;
pub mod references;
#[cfg(test)]
mod references_tests;
pub mod registry;
pub mod report;

use std::collections::HashSet;
use std::fmt;

use serde::Deserialize;
use ts_rs::TS;
use zeroize::Zeroizing;

use super::model::{AttentionKind, ImportPreview, KeyValueInput, PresetInput};
use super::passkeys::PasskeyInput;
use super::snapshots::SnapshotData;
use super::ATTACHMENT_LIMIT_BYTES;
use crate::error::{HolziError, Result};

/// The formats the import reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "lowercase")]
pub enum ImportSource {
    Keepass,
    Bitwarden,
    Lastpass,
    /// The vault file of haex-vault 2.x (Legacy) (spec 037); read from disk, not from bytes.
    #[serde(rename = "legacyhaexvault")]
    LegacyHaexVault,
}

/// The password and key file of a KeePass database. Both are zeroed when dropped and `Debug` prints
/// neither (R12 point 11).
#[derive(Default)]
pub struct Credentials {
    pub password: Option<Zeroizing<String>>,
    pub key_file: Option<Zeroizing<Vec<u8>>>,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("key_file", &self.key_file.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// How the source names an icon: one of the pictures of holzi, or the bytes of a picture of its own.
#[derive(Clone, PartialEq, Eq)]
pub enum IconRef {
    Standard(String),
    Custom(Vec<u8>),
}

impl fmt::Debug for IconRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IconRef::Standard(name) => f.debug_tuple("Standard").field(name).finish(),
            IconRef::Custom(bytes) => write!(f, "Custom({} bytes)", bytes.len()),
        }
    }
}

/// A place in the source that could not arrive as it was. It names the field or file, never a value.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub kind: AttentionKind,
    pub field: Option<String>,
    pub file_name: Option<String>,
    pub size_mib: Option<f64>,
}

impl Problem {
    pub fn new(kind: AttentionKind) -> Self {
        Self {
            kind,
            field: None,
            file_name: None,
            size_mib: None,
        }
    }

    pub fn field(kind: AttentionKind, field: &str) -> Self {
        Self {
            field: Some(field.to_string()),
            ..Self::new(kind)
        }
    }

    /// An attachment above the limit, named with its size in MiB.
    pub fn too_large(file_name: &str, size: u64) -> Self {
        Self {
            file_name: Some(file_name.to_string()),
            size_mib: Some(size as f64 / (1024.0 * 1024.0)),
            ..Self::new(AttentionKind::AttachmentTooLarge)
        }
    }
}

/// A folder of the source. `reference` is the source's own key; parents come before children.
#[derive(Debug, Clone)]
pub struct ImportGroup {
    pub reference: String,
    pub parent_ref: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<IconRef>,
    /// The recycle bin of the source: it becomes the trash row, not a folder.
    pub is_recycle_bin: bool,
    /// The folder a trashed folder came from, if the source names it.
    pub previous_parent_ref: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i64>,
}

/// An attachment with its bytes (never above the limit; a larger one is a [`Problem`]).
#[derive(Clone)]
pub struct ImportAttachment {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for ImportAttachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportAttachment")
            .field("file_name", &self.file_name)
            .field("size", &self.bytes.len())
            .finish()
    }
}

impl ImportAttachment {
    /// `None` when the bytes are above the limit; the caller then records the [`Problem`].
    pub fn within_limit(file_name: &str, bytes: Vec<u8>) -> std::result::Result<Self, Problem> {
        if bytes.len() as u64 > ATTACHMENT_LIMIT_BYTES {
            return Err(Problem::too_large(file_name, bytes.len() as u64));
        }
        Ok(Self {
            file_name: file_name.to_string(),
            bytes,
        })
    }
}

/// An earlier state of an entry, with the time the source gives. `data.attachments` stays empty:
/// the attachments of the state travel in `attachments` with their bytes, and `apply` links them.
#[derive(Debug, Clone)]
pub struct ImportState {
    pub modified_at: Option<String>,
    pub data: SnapshotData,
    pub attachments: Vec<ImportAttachment>,
}

/// An entry of the source. `Debug` prints no value.
#[derive(Clone, Default)]
pub struct ImportItem {
    pub title: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub note: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// The TOTP text as the source has it (a secret or an `otpauth://` address), kept even when it
    /// is not valid.
    pub otp_raw: Option<String>,
    pub otp_digits: Option<i64>,
    pub otp_period: Option<i64>,
    pub otp_algorithm: Option<String>,
    pub color: Option<String>,
    /// The JSON object of the autofill aliases (`{ field: [alias, …] }`).
    pub autofill_aliases: Option<serde_json::Value>,
    pub tags: Vec<String>,
    pub key_values: Vec<KeyValueInput>,
    pub attachments: Vec<ImportAttachment>,
    pub passkeys: Vec<PasskeyInput>,
    pub history: Vec<ImportState>,
    pub icon: Option<IconRef>,
    pub group_ref: Option<String>,
    /// In the trash of the source: under the recycle bin group, or deleted.
    pub trashed: bool,
    /// The folder the source names for a deleted entry.
    pub trashed_from_ref: Option<String>,
    pub problems: Vec<Problem>,
    /// The id of the entry in the source file (KeePass), for the references between its entries
    /// (spec 036, FR-050).
    pub source_ref: Option<String>,
    /// The id the import gives the entry, set before writing so references can point at it.
    pub assigned_id: Option<String>,
}

impl fmt::Debug for ImportItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportItem")
            .field("title", &self.title)
            .field("secrets", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Everything a file holds, ready to be written.
#[derive(Debug, Clone, Default)]
pub struct ImportModel {
    pub groups: Vec<ImportGroup>,
    pub items: Vec<ImportItem>,
    /// Problems of the source as a whole (its application settings).
    pub source_problems: Vec<Problem>,
    /// Colours of tags by tag name; a tag of the vault without a colour takes it.
    pub tag_colors: Vec<(String, String)>,
    /// Passkeys that belong to no entry.
    pub passkeys: Vec<PasskeyInput>,
    /// Presets of the password generator (empty `id`: new ones).
    pub presets: Vec<PresetInput>,
}

/// Reads a file of `source`. Pure: no database, no clock. `Bitwarden` is told apart by its first
/// character (`{` is the JSON export, anything else the CSV). `LegacyHaexVault` is a database file and is
/// read by [`legacy_haex_vault::read`] from its path, not from bytes.
pub fn parse(source: ImportSource, bytes: &[u8], credentials: &Credentials) -> Result<ImportModel> {
    match source {
        ImportSource::Keepass => keepass::parse(bytes, credentials),
        ImportSource::Bitwarden => bitwarden::parse(bytes),
        ImportSource::Lastpass => lastpass::parse(bytes),
        ImportSource::LegacyHaexVault => Err(failed("unsupported_format")),
    }
}

/// The text of an optional value, `None` when it is missing or empty.
pub(crate) fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|v| !v.is_empty()).map(str::to_string)
}

/// Adds a custom field named `key` unless its value is missing or empty.
pub(crate) fn push_kv(fields: &mut Vec<KeyValueInput>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
        fields.push(KeyValueInput {
            key: key.to_string(),
            value: Some(value.to_string()),
        });
    }
}

/// A time of another product in the form the vault stores; `None` if it cannot be read.
pub(crate) fn holzi_time(text: Option<&str>) -> Option<String> {
    text.and_then(super::clock::parse_iso_millis)
        .map(super::clock::format_millis)
}

/// Adds a tag once (names are compared as the vault does, by [`super::ids::fold`]).
pub(crate) fn push_tag(tags: &mut Vec<String>, name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let folded = super::ids::fold(name);
    if !tags.iter().any(|t| super::ids::fold(t) == folded) {
        tags.push(name.to_string());
    }
}

/// The folder of `segments` (created with its parents if needed), as its reference; `None` for no
/// segments. Folders are matched by parent and exact name, so equal paths share one folder.
pub(crate) fn ensure_group_path(
    groups: &mut Vec<ImportGroup>,
    segments: &[String],
) -> Option<String> {
    let mut parent: Option<String> = None;
    for name in segments {
        let found = groups
            .iter()
            .find(|g| g.parent_ref == parent && g.name == *name && !g.is_recycle_bin)
            .map(|g| g.reference.clone());
        let reference = match found {
            Some(reference) => reference,
            None => {
                let reference = format!("group-{}", groups.len());
                groups.push(ImportGroup {
                    reference: reference.clone(),
                    parent_ref: parent.clone(),
                    name: name.clone(),
                    description: None,
                    icon: None,
                    is_recycle_bin: false,
                    previous_parent_ref: None,
                    color: None,
                    sort_order: None,
                });
                reference
            }
        };
        parent = Some(reference);
    }
    parent
}

/// Splits a folder name at the given separators into its levels, dropping empty ones.
pub(crate) fn split_path(name: &str, separators: &[char]) -> Vec<String> {
    name.split(|c| separators.contains(&c))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// A TOTP that cannot make a code stays as it is (R12 point 9) and is listed in the report.
pub(crate) fn check_otp(item: &mut ImportItem) {
    let Some(raw) = item.otp_raw.as_deref().filter(|r| !r.trim().is_empty()) else {
        item.otp_raw = None;
        return;
    };
    if super::totp::resolve(
        raw,
        item.otp_digits,
        item.otp_period,
        item.otp_algorithm.as_deref(),
    )
    .is_err()
    {
        item.problems
            .push(Problem::field(AttentionKind::TotpInvalid, "totp"));
    }
}

/// The state of an entry as the history of a source sees it: the entry's own fields, so that a
/// state only needs to say what differs.
pub(crate) fn base_state(item: &ImportItem) -> SnapshotData {
    SnapshotData {
        title: item.title.clone(),
        username: item.username.clone(),
        password: item.password.clone(),
        url: item.url.clone(),
        note: item.note.clone(),
        expires_at: item.expires_at.clone(),
        otp_secret: item.otp_raw.clone(),
        otp_digits: item.otp_digits.and_then(|v| u32::try_from(v).ok()),
        otp_period: item.otp_period.and_then(|v| u32::try_from(v).ok()),
        otp_algorithm: item.otp_algorithm.clone(),
        tag_names: item.tags.clone(),
        key_values: item
            .key_values
            .iter()
            .map(|kv| super::snapshots::SnapshotKeyValue {
                key: Some(kv.key.clone()),
                value: kv.value.clone(),
            })
            .collect(),
        ..SnapshotData::default()
    }
}

pub(crate) fn failed(reason: &str) -> HolziError {
    HolziError::PasswordsImportFailed {
        reason: reason.to_string(),
    }
}

/// How an entry is told apart from another one already in the vault (R12 point 10): title, user
/// name and address; a missing value equals an empty one.
pub fn duplicate_key(
    title: Option<&str>,
    username: Option<&str>,
    url: Option<&str>,
) -> (String, String, String) {
    (
        title.unwrap_or_default().to_string(),
        username.unwrap_or_default().to_string(),
        url.unwrap_or_default().to_string(),
    )
}

pub type ExistingKeys = HashSet<(String, String, String)>;

/// What the file would bring: counts, and the kinds of problems found while reading. Writes nothing.
pub fn preview(model: &ImportModel, existing: &ExistingKeys) -> ImportPreview {
    let mut preview = ImportPreview {
        entries: model.items.len() as u32,
        groups: model.groups.iter().filter(|g| !g.is_recycle_bin).count() as u32,
        trashed_entries: 0,
        history_states: 0,
        attachments: 0,
        passkeys: model.passkeys.len() as u32,
        duplicates: 0,
        tags: 0,
        presets: model.presets.len() as u32,
        warnings: Vec::new(),
    };
    let mut tag_names: HashSet<String> = HashSet::new();
    let mut kinds: Vec<AttentionKind> = model.source_problems.iter().map(|p| p.kind).collect();
    for item in &model.items {
        if item.trashed {
            preview.trashed_entries += 1;
        }
        preview.history_states += item.history.len() as u32;
        preview.attachments += (item.attachments.len()
            + item
                .history
                .iter()
                .map(|s| s.attachments.len())
                .sum::<usize>()) as u32;
        preview.passkeys += item.passkeys.len() as u32;
        tag_names.extend(item.tags.iter().map(|t| super::ids::fold(t)));
        let key = duplicate_key(
            item.title.as_deref(),
            item.username.as_deref(),
            item.url.as_deref(),
        );
        if existing.contains(&key) {
            preview.duplicates += 1;
        }
        kinds.extend(item.problems.iter().map(|p| p.kind));
    }
    preview.tags = tag_names.len() as u32;
    for kind in kinds {
        let name = report::kind_name(kind).to_string();
        if !preview.warnings.contains(&name) {
            preview.warnings.push(name);
        }
    }
    preview
}
