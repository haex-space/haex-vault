//! The types of importing a pass from a file (spec 092, `contracts/tauri-commands.md`): what the
//! dialog is shown before saving, what the user may change, and the report afterwards. None of
//! them holds the value of a code (FR-025, FR-032): the dialog sees format and label.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{PassKind, Start};
use super::pkpass::NotKept;
use super::place::Proposal;

/// One code as the dialog shows it: its format and label, never its value.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct CodePreview {
    pub format: String,
    pub label: Option<String>,
    /// `invalid_value` or `unknown_format`: the code is kept as text and can be repaired.
    pub flag: Option<String>,
}

/// One free field of the pass.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct FieldPreview {
    pub name: String,
    pub value: String,
}

/// The pass of a file as the dialog shows it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PassPreview {
    #[ts(type = "string")]
    pub pass_kind: PassKind,
    pub title: String,
    pub provider: Option<String>,
    pub start: Option<Start>,
    pub expires_at: Option<String>,
    pub codes: Vec<CodePreview>,
    pub fields: Vec<FieldPreview>,
    pub has_icon: bool,
}

/// A pass of the vault that comes from the same file (by its identity), not in the trash.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateOf {
    pub id: String,
    pub title: Option<String>,
}

/// What `passwords_pass_import_preview` returns.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PassImportPreview {
    pub preview_id: String,
    pub pass: PassPreview,
    pub proposal: Proposal,
    pub duplicate_of: Option<DuplicateOf>,
    pub not_kept: Vec<NotKept>,
}

/// What to do when the vault holds the pass already.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum PassOnDuplicate {
    /// Refuse and let the dialog ask.
    #[default]
    Ask,
    /// Save nothing.
    Skip,
    /// Save it as a new entry anyway.
    Add,
}

/// What the user chose in the dialog; a field that is absent stays as the file gave it.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase", default)]
pub struct PassImportChoices {
    pub title: Option<String>,
    #[ts(type = "string | undefined")]
    pub pass_kind: Option<PassKind>,
    pub start: Option<Start>,
    /// The folders below the root of Wallet, outermost first; empty is the top of Wallet.
    pub folder_path: Vec<String>,
    pub tag_name: Option<String>,
    pub parent_id: Option<String>,
    pub on_duplicate: PassOnDuplicate,
}

/// What an import reports for one pass: the entry, the place and what was not kept. It holds no
/// code and no secret (FR-032) and can be saved as text.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PassImportReport {
    pub title: String,
    pub folder_path: Vec<String>,
    pub tag_name: Option<String>,
    pub not_kept: Vec<NotKept>,
    /// How many codes are kept as text because they did not fit their format.
    pub flagged_codes: u32,
}

/// What `passwords_pass_import_save` returns; `id` is absent when the user chose to skip a
/// duplicate.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PassImportSaved {
    pub id: Option<String>,
    pub report: PassImportReport,
}
