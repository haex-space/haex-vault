use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::openai_compatible::OpenAiCompatibleAdapter;
use super::openai_compatible_capabilities::Described;
use super::{AttachmentKind, ProviderAdapter};
use crate::model_capabilities::{
    ModelCapabilities, ModelKind, ReasoningControl, ToolSupport, ToolUse, ToolUseBasis,
};

fn read(entry: Value) -> ModelCapabilities {
    serde_json::from_value::<Described>(entry)
        .expect("a model entry always reads")
        .capabilities()
}

fn provider(support: ToolSupport) -> Option<ToolUse> {
    Some(ToolUse::new(support, ToolUseBasis::Provider))
}

#[test]
fn a_plain_list_entry_determines_nothing() {
    let caps = read(json!({"id": "qwen3-vl:8b", "object": "model", "owned_by": "library"}));
    assert!(caps.is_undetermined());
}

#[test]
fn abacus_modalities_say_whether_a_model_takes_images() {
    let with_images = read(json!({
        "model_type": "text_generation",
        "input_modalities": ["text", "image", "video"],
        "output_modalities": ["text"],
        "tools": true,
    }));
    assert_eq!(
        with_images.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Text, AttachmentKind::Image])
    );
    assert_eq!(with_images.kind, Some(ModelKind::Text));
    assert_eq!(with_images.tool_use, provider(ToolSupport::Supported));

    let text_only = read(json!({
        "model_type": "text_generation",
        "input_modalities": ["text"],
    }));
    assert_eq!(
        text_only.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Text])
    );
}

#[test]
fn a_tools_flag_that_is_false_means_unsupported_and_an_absent_one_means_unknown() {
    let without = read(json!({"model_type": "text_generation", "tools": false}));
    assert_eq!(without.tool_use, provider(ToolSupport::Unsupported));

    let silent = read(json!({"model_type": "text_generation"}));
    assert_eq!(silent.tool_use, None);
}

#[test]
fn the_model_type_tells_chat_models_from_the_others() {
    for (model_type, kind) in [
        ("text_generation", ModelKind::Text),
        ("image_generation", ModelKind::Image),
        ("audio_generation", ModelKind::Audio),
        ("classification", ModelKind::Other),
    ] {
        assert_eq!(
            read(json!({"model_type": model_type})).kind,
            Some(kind),
            "{model_type}"
        );
    }
}

#[test]
fn the_model_type_beats_the_output_modalities() {
    // An image model that also answers in text is still one for images.
    let caps = read(json!({
        "model_type": "image_generation",
        "output_modalities": ["text", "image"],
    }));
    assert_eq!(caps.kind, Some(ModelKind::Image));
}

#[test]
fn openrouter_nests_its_modalities_and_names_the_request_parameters() {
    let caps = read(json!({
        "id": "vendor/model",
        "architecture": {
            "input_modalities": ["text", "image"],
            "output_modalities": ["text"],
        },
        "supported_parameters": ["max_tokens", "tools", "reasoning", "include_reasoning"],
    }));
    assert_eq!(
        caps.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Text, AttachmentKind::Image])
    );
    assert_eq!(caps.kind, Some(ModelKind::Text));
    assert_eq!(caps.tool_use, provider(ToolSupport::Supported));
    assert_eq!(caps.reasoning, Some(ReasoningControl::ModelManaged));

    // A complete parameter list without `tools` is a statement: the model takes none.
    let no_tools = read(json!({"supported_parameters": ["max_tokens", "temperature"]}));
    assert_eq!(no_tools.tool_use, provider(ToolSupport::Unsupported));
    assert_eq!(no_tools.reasoning, None);
}

#[test]
fn a_thinking_model_is_known_to_reason_but_its_levels_stay_open() {
    let thinking = read(json!({"model_type": "text_generation", "thinking": true}));
    assert_eq!(thinking.reasoning, Some(ReasoningControl::ModelManaged));

    let not_thinking = read(json!({"model_type": "text_generation", "thinking": false}));
    assert_eq!(not_thinking.reasoning, Some(ReasoningControl::Unavailable));

    // Absent is not "does not reason".
    let silent = read(json!({"model_type": "text_generation"}));
    assert_eq!(silent.reasoning, None);
}

#[test]
fn empty_or_oddly_typed_fields_state_nothing_and_never_fail_the_list() {
    let caps = read(json!({
        "input_modalities": [],
        "output_modalities": "text",
        "architecture": "unknown",
        "model_type": 7,
        "tools": "yes",
        "thinking": null,
        "supported_parameters": {"tools": true},
    }));
    assert!(caps.is_undetermined());
}

#[tokio::test]
async fn list_models_keeps_what_an_aggregator_says_about_each_model() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "id": "Qwen/Qwen3.8-27B",
                    "model_type": "text_generation",
                    "input_modalities": ["text", "image"],
                    "context_length": 262144,
                    "thinking": true,
                    "tools": true,
                },
                {"id": "image-maker", "model_type": "image_generation", "input_modalities": ["text"]},
            ],
        })))
        .mount(&server)
        .await;

    let adapter = OpenAiCompatibleAdapter::new(format!("{}/v1", server.uri()), String::new())
        .expect("adapter");
    let models = adapter.list_models().await.expect("list succeeds");

    // The extra fields sit beside the ones the adapter already read.
    assert_eq!(models[0].context_window, Some(262144));
    assert_eq!(models[0].capabilities.kind, Some(ModelKind::Text));
    assert_eq!(
        models[0].capabilities.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Text, AttachmentKind::Image])
    );
    assert_eq!(models[1].capabilities.kind, Some(ModelKind::Image));
}
