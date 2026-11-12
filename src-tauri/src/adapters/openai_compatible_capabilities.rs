//! What an OpenAI-compatible server's `/models` answer says about a model (spec 012 FR-024).
//!
//! The OpenAI list itself carries only ids, but aggregators add fields: Abacus lists
//! `input_modalities`, `model_type`, `tools` and `thinking` per model, OpenRouter nests the
//! modalities in `architecture` and names the accepted request parameters in
//! `supported_parameters`. Whatever is present is read; a field that is absent, empty or of an
//! unexpected type states nothing, so a plain Ollama or vLLM list still yields "not determined".
//! Effort levels are never in these answers: the user states them (spec 055 FR-019).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::AttachmentKind;
use crate::model_capabilities::{
    ModelCapabilities, ModelKind, ReasoningControl, ToolSupport, ToolUse, ToolUseBasis,
};

/// A field that reads as `None` when it is of another type than expected, so one odd server field
/// can never make the whole model list fail.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(Option::<Value>::deserialize(deserializer)?.and_then(|v| serde_json::from_value(v).ok()))
}

/// The capability fields a server may add to a model entry.
#[derive(Deserialize, Default)]
pub(super) struct Described {
    #[serde(default, deserialize_with = "lenient")]
    input_modalities: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient")]
    output_modalities: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient")]
    architecture: Option<Architecture>,
    #[serde(default, deserialize_with = "lenient")]
    model_type: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    tools: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    thinking: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    supported_parameters: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct Architecture {
    #[serde(default, deserialize_with = "lenient")]
    input_modalities: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient")]
    output_modalities: Option<Vec<String>>,
}

/// A non-empty list; an empty one states nothing.
fn listed(list: &Option<Vec<String>>) -> Option<&[String]> {
    list.as_deref().filter(|l| !l.is_empty())
}

fn names(list: &[String], wanted: &str) -> bool {
    list.iter().any(|m| m.eq_ignore_ascii_case(wanted))
}

impl Described {
    fn inputs(&self) -> Option<&[String]> {
        listed(&self.input_modalities).or_else(|| {
            self.architecture
                .as_ref()
                .and_then(|a| listed(&a.input_modalities))
        })
    }

    fn outputs(&self) -> Option<&[String]> {
        listed(&self.output_modalities).or_else(|| {
            self.architecture
                .as_ref()
                .and_then(|a| listed(&a.output_modalities))
        })
    }

    fn parameters(&self) -> Option<&[String]> {
        listed(&self.supported_parameters)
    }

    fn attachment_kinds(&self) -> Option<Vec<AttachmentKind>> {
        let inputs = self.inputs()?;
        let mut kinds = Vec::new();
        if names(inputs, "text") {
            kinds.push(AttachmentKind::Text);
        }
        if names(inputs, "image") {
            kinds.push(AttachmentKind::Image);
        }
        Some(kinds)
    }

    fn kind(&self) -> Option<ModelKind> {
        if let Some(model_type) = self.model_type.as_deref().filter(|t| !t.is_empty()) {
            return Some(match model_type {
                "text_generation" | "chat" => ModelKind::Text,
                "image_generation" => ModelKind::Image,
                "audio_generation" => ModelKind::Audio,
                _ => ModelKind::Other,
            });
        }
        let outputs = self.outputs()?;
        Some(if names(outputs, "text") {
            ModelKind::Text
        } else if names(outputs, "image") {
            ModelKind::Image
        } else if names(outputs, "audio") {
            ModelKind::Audio
        } else {
            ModelKind::Other
        })
    }

    fn tool_use(&self) -> Option<ToolUse> {
        let supported = match (self.tools, self.parameters()) {
            (Some(tools), _) => tools,
            (None, Some(parameters)) => names(parameters, "tools"),
            (None, None) => return None,
        };
        let support = if supported {
            ToolSupport::Supported
        } else {
            ToolSupport::Unsupported
        };
        Some(ToolUse::new(support, ToolUseBasis::Provider))
    }

    /// A model that reasons is known to do so; which levels it takes is not in the answer.
    fn reasoning(&self) -> Option<ReasoningControl> {
        let says_reasoning = self
            .parameters()
            .is_some_and(|p| names(p, "reasoning") || names(p, "include_reasoning"));
        match self.thinking {
            Some(true) => Some(ReasoningControl::ModelManaged),
            Some(false) => Some(ReasoningControl::Unavailable),
            None if says_reasoning => Some(ReasoningControl::ModelManaged),
            None => None,
        }
    }

    pub(super) fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            reasoning: self.reasoning(),
            accepted_attachment_kinds: self.attachment_kinds(),
            tool_use: self.tool_use(),
            kind: self.kind(),
            ..ModelCapabilities::default()
        }
    }
}
