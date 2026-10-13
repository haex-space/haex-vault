use serde::{Deserialize, Serialize};

use super::TASK_SCHEMA_VERSION;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub task: String,
    pub schema_version: u32,
    pub profile_id: Option<String>,
    pub input: PaperNoteInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperNoteInput {
    pub image: ImageInput,
    pub locale: String,
    #[serde(default)]
    pub options: PaperNoteOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PaperNoteOptions {
    pub detect_drawings: bool,
    pub suggest_spelling: bool,
}

impl Default for PaperNoteOptions {
    fn default() -> Self {
        Self {
            detect_drawings: true,
            suggest_spelling: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingMode {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProvenance {
    pub task_version: u32,
    pub profile_id: String,
    pub model_id: String,
    pub harness_id: String,
    pub mode: ProcessingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperNoteResult {
    pub schema_version: u32,
    pub regions: Vec<PaperNoteRegion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperNoteRegion {
    pub kind: PaperNoteRegionKind,
    pub source_box: NormalizedBox,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<SpellingSuggestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperNoteRegionKind {
    Text,
    Drawing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellingSuggestion {
    pub original: String,
    pub replacement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

impl PaperNoteResult {
    pub fn new(regions: Vec<PaperNoteRegion>) -> Self {
        Self {
            schema_version: TASK_SCHEMA_VERSION,
            regions,
        }
    }
}
