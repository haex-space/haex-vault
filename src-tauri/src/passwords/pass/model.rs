//! The types a pass travels in (spec 092, `contracts/tauri-commands.md`): what the window sends to
//! create or change a pass, and what it gets back for the codes. A code's value is a secret
//! (FR-025): none of these prints one.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use crate::passwords::model::{KeyValueInput, KeyValuePatch, Patch};

/// The kind of a pass. A text this build does not know (written by a newer holzi, or imported) is
/// kept as it is, shown as "other" and written back unchanged (FR-004).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PassKind {
    Ticket,
    LoyaltyCard,
    Voucher,
    Membership,
    BoardingPass,
    #[default]
    Other,
    Unknown(String),
}

impl PassKind {
    pub fn parse(text: &str) -> Self {
        match text {
            "ticket" => Self::Ticket,
            "loyalty_card" => Self::LoyaltyCard,
            "voucher" => Self::Voucher,
            "membership" => Self::Membership,
            "boarding_pass" => Self::BoardingPass,
            "" | "other" => Self::Other,
            other => Self::Unknown(other.to_owned()),
        }
    }

    pub fn as_text(&self) -> &str {
        match self {
            Self::Ticket => "ticket",
            Self::LoyaltyCard => "loyalty_card",
            Self::Voucher => "voucher",
            Self::Membership => "membership",
            Self::BoardingPass => "boarding_pass",
            Self::Other => "other",
            Self::Unknown(text) => text,
        }
    }
}

impl Serialize for PassKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_text())
    }
}

impl<'de> Deserialize<'de> for PassKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// When a pass starts: a date alone, or a moment (UTC, RFC 3339) with the zone of the place where
/// it happens, an IANA name (`Europe/London`) or a fixed offset (`+02:00`) (FR-002).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(untagged)]
pub enum Start {
    Date { date: String },
    At { at: String, zone: String },
}

/// One code in an input. A code that carries an `id` keeps its row; one without is a new row.
/// `flag` is set only by the Rust side (an import or a sync keeps a code that does not fit); the
/// window cannot send one.
#[derive(Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct CodeInput {
    pub id: Option<String>,
    pub format: String,
    pub value: String,
    pub label: Option<String>,
    #[ts(type = "number")]
    pub position: i64,
    #[serde(skip)]
    #[ts(skip)]
    pub flag: Option<String>,
}

impl fmt::Debug for CodeInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeInput")
            .field("id", &self.id)
            .field("format", &self.format)
            .field("value", &"<redacted>")
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

/// A new pass.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase", default)]
pub struct PassInput {
    pub title: Option<String>,
    pub note: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    /// `YYYY-MM-DD`; the end of the pass.
    pub expires_at: Option<String>,
    #[ts(type = "string")]
    pub pass_kind: PassKind,
    pub start: Option<Start>,
    pub parent_id: Option<String>,
    pub codes: Vec<CodeInput>,
    pub key_values: Vec<KeyValueInput>,
    pub tag_names: Vec<String>,
}

/// A partial update of a pass: a field that is absent stays as it is. `codes`, when present, is
/// the whole list by code id: a code with an id keeps its row, one without is new, a stored row
/// whose id is not listed is deleted.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase", default)]
pub struct PassPatch {
    #[ts(type = "string | null", optional)]
    pub title: Patch<String>,
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
    #[ts(type = "string", optional)]
    pub pass_kind: Option<PassKind>,
    #[ts(type = "import(\"./Start\").Start | null", optional)]
    pub start: Patch<Start>,
    #[ts(type = "string | null", optional)]
    pub parent_id: Patch<String>,
    #[ts(optional)]
    pub codes: Option<Vec<CodeInput>>,
    #[ts(optional)]
    pub key_values: Option<Vec<KeyValuePatch>>,
    #[ts(optional)]
    pub tag_names: Option<Vec<String>>,
}

/// One code with its value, for the user's own screen only (`passwords_pass_codes`).
#[derive(Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CodeView {
    pub id: String,
    pub format: String,
    pub label: Option<String>,
    #[ts(type = "number")]
    pub position: i64,
    /// `invalid_value` or `unknown_format`: shown as text and repairable (FR-003).
    pub flag: Option<String>,
    pub value: String,
}

impl fmt::Debug for CodeView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeView")
            .field("id", &self.id)
            .field("format", &self.format)
            .field("value", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// The result of drawing one code: the drawing, or the text when the code cannot be drawn (it was
/// flagged on arrival).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CodeSvg {
    pub svg: Option<String>,
    #[ts(type = "number")]
    pub width: u32,
    #[ts(type = "number")]
    pub height: u32,
    #[ts(type = "number")]
    pub quiet_zone: u32,
    /// The value as text, set only when there is no drawing.
    pub text: Option<String>,
}

/// The answer of the check the editor runs while typing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct CodeCheck {
    pub ok: bool,
    pub reason: Option<String>,
}
