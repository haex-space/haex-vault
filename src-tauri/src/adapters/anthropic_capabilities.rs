//! Maps the `capabilities` object of Anthropic's `GET /v1/models` response to
//! the provider-agnostic [`ModelCapabilities`] record (spec
//! 012-unified-model-capabilities, research.md R3).
//!
//! Kept beside `anthropic.rs` rather than inside it so the wire shape and its
//! mapping can be tested without an HTTP double. Every wire field is
//! optional: a response that omits a leaf yields "not determined" for what
//! depends on it, never an error and never "unsupported" (FR-002). The same
//! mapping serves the direct API-key path and the Claude Code delegate,
//! which lists models through the same endpoint.

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

use crate::adapters::AttachmentKind;
use crate::model_capabilities::{
    ModelCapabilities, ReasoningControl, ReasoningOption, ThinkingStyle, ToolSupport, ToolUse,
    ToolUseBasis,
};

/// `{"supported": <bool>}` — the shape of every capability leaf.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct Leaf {
    supported: Option<bool>,
}

impl Leaf {
    fn supported(leaf: &Option<Leaf>) -> Option<bool> {
        leaf.as_ref().and_then(|l| l.supported)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ThinkingTypes {
    enabled: Option<Leaf>,
    adaptive: Option<Leaf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Thinking {
    supported: Option<bool>,
    types: Option<ThinkingTypes>,
}

#[derive(Debug, Default)]
struct Effort {
    supported: Option<bool>,
    /// Provider-native option names in the order reported by the provider.
    /// Anthropic currently reports names such as `low` and `max`, but the
    /// adapter must not turn that current vocabulary into a global enum.
    levels: Vec<(String, Leaf)>,
}

impl<'de> Deserialize<'de> for Effort {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EffortVisitor;

        impl<'de> Visitor<'de> for EffortVisitor {
            type Value = Effort;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an effort capability object")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut supported = None;
                let mut levels = Vec::new();

                while let Some(key) = map.next_key::<String>()? {
                    if key == "supported" {
                        supported = map.next_value()?;
                    } else {
                        levels.push((key, map.next_value()?));
                    }
                }

                Ok(Effort { supported, levels })
            }
        }

        deserializer.deserialize_map(EffortVisitor)
    }
}

/// The wire `capabilities` object. Unknown top-level leaves are ignored;
/// effort keys are provider-native and are preserved as selectable options.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct WireCapabilities {
    image_input: Option<Leaf>,
    pdf_input: Option<Leaf>,
    thinking: Option<Thinking>,
    effort: Option<Effort>,
}

fn reasoning(wire: &WireCapabilities) -> Option<ReasoningControl> {
    let effort_supported = wire.effort.as_ref().and_then(|e| e.supported);
    let thinking_supported = wire.thinking.as_ref().and_then(|t| t.supported);

    if effort_supported == Some(true) {
        let options: Vec<ReasoningOption> = wire
            .effort
            .as_ref()
            .map(|effort| &effort.levels)
            .into_iter()
            .flatten()
            .filter(|(_, leaf)| leaf.supported == Some(true))
            .map(|(id, _)| ReasoningOption {
                id: id.clone(),
                label: id.clone(),
            })
            .collect();
        if !options.is_empty() {
            return Some(ReasoningControl::presets(options));
        }

        // `effort.supported: true` without any provider-native level leaves
        // is partial data, not an authoritative statement that the model
        // manages effort.
        if wire
            .effort
            .as_ref()
            .is_none_or(|effort| effort.levels.is_empty())
        {
            return None;
        }
    }
    // Both answers must be present before claiming the user has no say; a
    // missing subtree stays "not determined".
    match (effort_supported, thinking_supported) {
        (Some(_), Some(true)) => Some(ReasoningControl::ModelManaged),
        (Some(_), Some(false)) => Some(ReasoningControl::Unavailable),
        _ => None,
    }
}

fn thinking_style(wire: &WireCapabilities) -> Option<ThinkingStyle> {
    let types = wire.thinking.as_ref()?.types.as_ref()?;
    if Leaf::supported(&types.adaptive) == Some(true) {
        Some(ThinkingStyle::Adaptive)
    } else if Leaf::supported(&types.enabled) == Some(true) {
        Some(ThinkingStyle::Manual)
    } else {
        None
    }
}

/// ponytail: attachments are determined only when *both* `image_input` and
/// `pdf_input` are present, so one missing key undetermines every kind.
/// Ceiling: a provider that stops reporting `pdf_input` makes all its
/// attachments "not yet known". Upgrade path: per-kind granularity
/// (`Document` from `pdf_input` alone), see research.md R3.
fn attachment_kinds(wire: &WireCapabilities) -> Option<Vec<AttachmentKind>> {
    let image = Leaf::supported(&wire.image_input)?;
    let pdf = Leaf::supported(&wire.pdf_input)?;
    // Text files are inlined as a plain text block, which every
    // message-capable model accepts once support is determined at all.
    let mut kinds = vec![AttachmentKind::Text];
    if image {
        kinds.push(AttachmentKind::Image);
    }
    if pdf {
        kinds.push(AttachmentKind::Document);
    }
    Some(kinds)
}

/// Maps one model's wire capabilities to the persisted record.
pub(super) fn map_capabilities(wire: &WireCapabilities) -> ModelCapabilities {
    ModelCapabilities {
        reasoning: reasoning(wire),
        accepted_attachment_kinds: attachment_kinds(wire),
        thinking_style: thinking_style(wire),
        // The wire API has no tool field; every current Claude model calls tools. A provider that
        // reports the opposite takes precedence (spec 032 FR-015), there is no such signal yet.
        tool_use: Some(ToolUse::new(ToolSupport::Supported, ToolUseBasis::Provider)),
        user: None,
    }
}

#[cfg(test)]
#[path = "anthropic_capabilities_tests.rs"]
mod anthropic_capabilities_tests;
