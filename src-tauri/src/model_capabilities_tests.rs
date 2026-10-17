//! Unit tests for the capability record (spec 012-unified-model-capabilities,
//! data-model.md and contracts/capabilities-json.md). Pure serde and
//! invariant checks — no I/O.

use serde_json::json;

use super::{
    local_model_reasoning, ModelCapabilities, ReasoningControl, ReasoningOption, ThinkingStyle,
    ToolSupport, ToolUse, ToolUseBasis,
};
use crate::adapters::AttachmentKind;

fn option(id: &str) -> ReasoningOption {
    ReasoningOption {
        id: id.to_string(),
        label: id.to_string(),
    }
}

#[test]
fn not_determined_stays_distinct_from_unavailable_after_a_json_round_trip() {
    let unknown = ModelCapabilities::default();
    let unavailable = ModelCapabilities {
        reasoning: Some(ReasoningControl::Unavailable),
        ..ModelCapabilities::default()
    };

    let unknown_back: ModelCapabilities =
        serde_json::from_str(&serde_json::to_string(&unknown).unwrap()).unwrap();
    let unavailable_back: ModelCapabilities =
        serde_json::from_str(&serde_json::to_string(&unavailable).unwrap()).unwrap();

    assert_eq!(unknown_back.reasoning, None);
    assert_eq!(
        unavailable_back.reasoning,
        Some(ReasoningControl::Unavailable)
    );
    assert_ne!(unknown_back, unavailable_back);
}

#[test]
fn serialized_shape_matches_the_documented_examples() {
    let caps = ModelCapabilities {
        reasoning: Some(ReasoningControl::presets(vec![
            option("low"),
            option("high"),
        ])),
        accepted_attachment_kinds: Some(vec![AttachmentKind::Text, AttachmentKind::Image]),
        thinking_style: Some(ThinkingStyle::Adaptive),
        tool_use: Some(ToolUse::new(ToolSupport::Supported, ToolUseBasis::Provider)),
        user: None,
    };

    assert_eq!(
        serde_json::to_value(&caps).unwrap(),
        json!({
            "reasoning": {
                "kind": "presets",
                "options": [
                    { "id": "low", "label": "low" },
                    { "id": "high", "label": "high" },
                ],
            },
            "acceptedAttachmentKinds": ["text", "image"],
            "thinkingStyle": "adaptive",
            "toolUse": { "support": "supported", "basis": "provider" },
        })
    );
    assert_eq!(
        serde_json::to_value(ModelCapabilities::local("qwen3-4b")).unwrap(),
        json!({
            "reasoning": { "kind": "model_managed" },
            "acceptedAttachmentKinds": [],
            "thinkingStyle": null,
            "toolUse": null,
        })
    );
}

#[test]
fn unknown_fields_are_ignored_and_missing_fields_default_to_not_determined() {
    let newer_build = json!({
        "reasoning": { "kind": "unavailable" },
        "somethingAddedLater": { "nested": true },
    });
    let caps: ModelCapabilities = serde_json::from_value(newer_build).unwrap();

    assert_eq!(caps.reasoning, Some(ReasoningControl::Unavailable));
    assert_eq!(caps.accepted_attachment_kinds, None);
    assert_eq!(caps.thinking_style, None);

    let empty: ModelCapabilities = serde_json::from_value(json!({})).unwrap();
    assert!(empty.is_undetermined());
}

#[test]
fn partial_nested_reasoning_data_keeps_sibling_capabilities() {
    let partial: ModelCapabilities = serde_json::from_value(json!({
        "reasoning": {
            "kind": "presets",
            "options": [{}],
        },
        "acceptedAttachmentKinds": ["image"],
        "thinkingStyle": "manual",
    }))
    .unwrap();

    assert_eq!(
        partial.reasoning,
        Some(ReasoningControl::Presets {
            options: vec![ReasoningOption {
                id: String::new(),
                label: String::new(),
            }],
        })
    );
    assert_eq!(
        partial.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Image])
    );
    assert_eq!(partial.thinking_style, Some(ThinkingStyle::Manual));

    let missing_options: ModelCapabilities = serde_json::from_value(json!({
        "reasoning": { "kind": "presets" },
        "acceptedAttachmentKinds": ["text"],
    }))
    .unwrap();
    let normalized = missing_options.clone().normalized();
    assert_eq!(normalized.reasoning, Some(ReasoningControl::Unavailable));
    assert_eq!(
        missing_options.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Text])
    );
}

#[test]
fn an_empty_option_list_never_becomes_a_selectable_control() {
    assert_eq!(
        ReasoningControl::presets(Vec::new()),
        ReasoningControl::Unavailable
    );
    assert!(matches!(
        ReasoningControl::presets(vec![option("low")]),
        ReasoningControl::Presets { .. }
    ));
}

#[test]
fn normalized_maps_a_deserialized_empty_presets_to_unavailable_and_changes_nothing_else() {
    let empty_presets: ModelCapabilities = serde_json::from_value(json!({
        "reasoning": { "kind": "presets", "options": [] },
        "thinkingStyle": "manual",
    }))
    .unwrap();
    let normalized = empty_presets.normalized();
    assert_eq!(normalized.reasoning, Some(ReasoningControl::Unavailable));
    assert_eq!(normalized.thinking_style, Some(ThinkingStyle::Manual));

    let untouched = ModelCapabilities {
        reasoning: Some(ReasoningControl::presets(vec![option("high")])),
        ..ModelCapabilities::default()
    };
    assert_eq!(untouched.clone().normalized(), untouched);
    assert_eq!(
        ModelCapabilities::default().normalized(),
        ModelCapabilities::default()
    );
}

#[test]
fn offers_is_true_only_for_ids_in_presets() {
    let presets = ReasoningControl::presets(vec![option("low"), option("max")]);
    assert!(presets.offers("low"));
    assert!(presets.offers("max"));
    assert!(!presets.offers("xhigh"));
    assert!(!ReasoningControl::ModelManaged.offers("low"));
    assert!(!ReasoningControl::Unavailable.offers("low"));
}

#[test]
fn only_a_record_with_no_answers_at_all_is_undetermined() {
    assert!(ModelCapabilities::default().is_undetermined());
    assert!(!ModelCapabilities {
        accepted_attachment_kinds: Some(Vec::new()),
        ..ModelCapabilities::default()
    }
    .is_undetermined());
    assert!(!ModelCapabilities {
        reasoning: Some(ReasoningControl::Unavailable),
        ..ModelCapabilities::default()
    }
    .is_undetermined());
}

#[test]
fn local_reasoning_is_derived_conservatively_from_the_model_id() {
    let reasons = |id: &str| local_model_reasoning(id) == ReasoningControl::ModelManaged;

    assert!(reasons("Qwen/Qwen3-4B-Instruct"));
    assert!(reasons("deepseek-r1-distill-qwen-7b"));
    assert!(reasons("gpt-oss-20b"));
    assert!(!reasons("Qwen/Qwen2.5-0.5B-Instruct"));
    assert!(!reasons("Qwen/Qwen3-4B-Instruct-2507"));
    assert!(!reasons("qwen3-30b-a3b-instruct-2507"));
    assert_eq!(
        local_model_reasoning("Qwen/Qwen2.5-0.5B-Instruct"),
        ReasoningControl::Unavailable
    );
}

#[test]
fn provider_model_ids_are_not_matched_by_the_local_heuristic() {
    // Anthropic models get their record from the provider's own answer; the
    // local-only rule must not carry provider-specific knowledge (FR-019).
    for id in [
        "claude-sonnet-4-20250514",
        "claude-haiku-4-5",
        "claude-opus-5",
    ] {
        assert_eq!(
            local_model_reasoning(id),
            ReasoningControl::Unavailable,
            "{id}"
        );
    }
}

#[test]
fn local_records_are_determined_and_accept_no_attachments() {
    let caps = ModelCapabilities::local("Qwen/Qwen3-4B-Instruct");

    assert!(!caps.is_undetermined());
    assert_eq!(caps.accepted_attachment_kinds, Some(Vec::new()));
    assert_eq!(caps.thinking_style, None);
}

#[test]
fn only_selectable_and_model_managed_reasoning_counts_as_reasoning() {
    assert!(ReasoningControl::ModelManaged.reasons());
    assert!(ReasoningControl::presets(vec![option("low")]).reasons());
    assert!(!ReasoningControl::Unavailable.reasons());
}

#[test]
fn a_record_without_tool_use_reads_as_not_determined() {
    let caps: ModelCapabilities =
        serde_json::from_value(json!({ "reasoning": { "kind": "unavailable" } })).unwrap();
    assert_eq!(caps.tool_use, None);
    let known: ModelCapabilities = serde_json::from_value(
        json!({ "toolUse": { "support": "unsupported", "basis": "template" } }),
    )
    .unwrap();
    assert_eq!(
        known.tool_use,
        Some(ToolUse::new(
            ToolSupport::Unsupported,
            ToolUseBasis::Template
        ))
    );
}

#[test]
fn tool_use_alone_makes_a_record_determined() {
    assert!(ModelCapabilities::default().is_undetermined());
    assert!(!ModelCapabilities {
        tool_use: Some(ToolUse::new(ToolSupport::Supported, ToolUseBasis::SelfTest)),
        ..ModelCapabilities::default()
    }
    .is_undetermined());
}

#[test]
fn a_finding_never_replaces_one_of_higher_rank() {
    use ToolSupport::{Supported, Unsupported};
    use ToolUseBasis::{Curated, Provider, SelfTest, Template};
    let self_test = ToolUse::new(Unsupported, SelfTest);
    let template = ToolUse::new(Unsupported, Template);
    let curated = ToolUse::new(Supported, Curated);
    let provider = ToolUse::new(Supported, Provider);
    assert!(template.may_replace(&self_test));
    assert!(curated.may_replace(&template));
    assert!(provider.may_replace(&curated) && curated.may_replace(&provider));
    assert!(self_test.may_replace(&self_test));
    assert!(!self_test.may_replace(&template));
    assert!(!template.may_replace(&curated));
    assert!(!self_test.may_replace(&provider));
}
