//! Unit tests for the `capabilities_json` column codec (spec
//! 012-unified-model-capabilities). The database-backed round trip
//! (`upsert_model` → `get_model`, overwrite, backfill) needs `current_hlc()`
//! from an open vault and lives in `tests/model_capabilities_storage.rs`.

use crate::adapters::AttachmentKind;
use crate::model_capabilities::{
    ModelCapabilities, ReasoningControl, ReasoningOption, ThinkingStyle, ToolSupport, ToolUse,
    ToolUseBasis,
};

use super::{capabilities_from_column, capabilities_to_column};

fn determined() -> ModelCapabilities {
    ModelCapabilities {
        reasoning: Some(ReasoningControl::presets(vec![ReasoningOption {
            id: "high".to_string(),
            label: "high".to_string(),
        }])),
        accepted_attachment_kinds: Some(vec![AttachmentKind::Text]),
        thinking_style: Some(ThinkingStyle::Manual),
        tool_use: Some(ToolUse::new(ToolSupport::Supported, ToolUseBasis::Provider)),
        user: None,
    }
}

#[test]
fn a_determined_record_survives_the_column_round_trip() {
    let column = capabilities_to_column(Some(&determined())).expect("serialize");

    assert_eq!(capabilities_from_column("m", column), Some(determined()));
}

#[test]
fn an_undetermined_or_absent_record_is_stored_as_sql_null() {
    assert_eq!(capabilities_to_column(None).expect("serialize"), None);
    assert_eq!(
        capabilities_to_column(Some(&ModelCapabilities::default())).expect("serialize"),
        None
    );
}

#[test]
fn a_null_column_reads_as_not_determined() {
    assert_eq!(capabilities_from_column("m", None), None);
}

#[test]
fn unparseable_json_reads_as_not_determined_instead_of_failing() {
    for raw in [
        "not json",
        "{\"reasoning\":",
        "[1, 2]",
        "{\"reasoning\":{\"kind\":\"nonsense\"}}",
    ] {
        assert_eq!(
            capabilities_from_column("m", Some(raw.to_string())),
            None,
            "{raw}"
        );
    }
}

#[test]
fn a_stored_empty_presets_list_reads_back_as_unavailable() {
    let raw = r#"{"reasoning":{"kind":"presets","options":[]},"thinkingStyle":"adaptive"}"#;

    let caps = capabilities_from_column("m", Some(raw.to_string())).expect("parsed");

    assert_eq!(caps.reasoning, Some(ReasoningControl::Unavailable));
    assert_eq!(caps.thinking_style, Some(ThinkingStyle::Adaptive));
}
