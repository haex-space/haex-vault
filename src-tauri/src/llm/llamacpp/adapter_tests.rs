use super::{finish_reason, probe, prompt_for, stream_error, PromptParts};
use crate::adapters::types::{
    ChatMessage, ChatRequest, ChatRole, Sampling, StreamError, ToolSpec, ToolTemplateProbe,
};
use crate::llm::llamacpp::formats::detect;
use crate::llm::llamacpp::generate::{Ended, GenerateError};
use crate::llm::llamacpp::model::LoadError;

const QWEN3: &str = include_str!("fixtures/qwen3.jinja");
const QWEN3_5: &str = include_str!("fixtures/qwen3_5.jinja");
const GEMMA4: &str = include_str!("fixtures/gemma4.jinja");

fn parts(template: &str) -> PromptParts<'_> {
    PromptParts {
        template,
        bos: "<s>",
        eos: "</s>",
        format: detect(template),
    }
}

fn request(tools: Vec<ToolSpec>, reasoning: bool) -> ChatRequest {
    ChatRequest {
        model_id: String::new(),
        thread_id: None,
        system_prompt: Some("Be brief.".to_owned()),
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "Open the sync settings.".to_owned(),
            attachments: Vec::new(),
        }],
        reasoning_requested: reasoning,
        max_new_tokens: None,
        tools,
        autonomy_mode: Default::default(),
        reasoning_option: None,
        capabilities: None,
        sampling: Sampling::Default,
    }
}

fn tool() -> ToolSpec {
    ToolSpec {
        name: "open_settings".to_owned(),
        description: "Opens a settings page".to_owned(),
        input_schema: serde_json::json!({"type": "object", "properties": {"page": {"type": "string"}}}),
    }
}

#[test]
fn the_prompt_carries_system_user_and_tools_through_the_files_own_template() {
    let prompt = prompt_for(&parts(QWEN3), &request(vec![tool()], false)).unwrap();
    assert!(prompt.contains("Be brief."), "{prompt}");
    assert!(prompt.contains("Open the sync settings."), "{prompt}");
    assert!(prompt.contains("open_settings"), "{prompt}");
}

#[test]
fn qwen3_5_with_thinking_ends_inside_an_open_think_block() {
    use crate::llm::llamacpp::streaming::Streamer;
    let on = prompt_for(&parts(QWEN3_5), &request(vec![], true)).unwrap();
    let format = detect(QWEN3_5);
    assert!(Streamer::prompt_opens_thinking(&on, format), "{on}");
    let off = prompt_for(&parts(QWEN3_5), &request(vec![], false)).unwrap();
    assert!(!Streamer::prompt_opens_thinking(&off, format), "{off}");
}

#[test]
fn gemma4_gets_its_own_format_and_tools_in_its_own_syntax() {
    let gemma = parts(GEMMA4);
    assert_eq!(gemma.format.map(|f| f.name()), Some("gemma4"));
    let prompt = prompt_for(&gemma, &request(vec![tool()], false)).unwrap();
    assert!(
        prompt.contains("<|tool>declaration:open_settings{"),
        "{prompt}"
    );
    assert!(prompt.ends_with("<|turn>model\n"), "{prompt}");
}

#[test]
fn a_model_without_a_known_call_format_is_refused_tools_but_not_chat() {
    let unknown = parts(
        "{% for m in messages %}{{ m.content }}{% endfor %}{% if tools %}{{ tools|tojson }}{% endif %}",
    );
    assert!(unknown.format.is_none());
    let error = prompt_for(&unknown, &request(vec![tool()], false)).unwrap_err();
    assert!(matches!(error, StreamError::StartFailed(_)), "{error:?}");
    assert!(prompt_for(&unknown, &request(vec![], false)).is_ok());
}

#[test]
fn a_template_that_does_not_render_is_a_start_failure() {
    let broken = PromptParts {
        template: "{{ nonexistent_function() }}",
        bos: "",
        eos: "",
        format: None,
    };
    let error = prompt_for(&broken, &request(vec![], false)).unwrap_err();
    assert!(matches!(error, StreamError::StartFailed(_)), "{error:?}");
    assert!(!error.is_transient());
}

#[test]
fn the_probe_sees_that_the_real_templates_show_tools() {
    for template in [QWEN3, QWEN3_5, GEMMA4] {
        assert_eq!(probe(&parts(template)), ToolTemplateProbe::Inconclusive);
    }
}

#[test]
fn the_probe_sees_a_template_that_ignores_tools() {
    let plain = "{% for m in messages %}{{ m.role }}: {{ m.content }}\n{% endfor %}";
    assert_eq!(probe(&parts(plain)), ToolTemplateProbe::IgnoresTools);
}

#[test]
fn the_probe_cannot_tell_when_the_template_does_not_render() {
    let broken = "{{ nonexistent_function() }}";
    assert_eq!(probe(&parts(broken)), ToolTemplateProbe::Inconclusive);
}

#[test]
fn errors_map_onto_stream_errors_that_are_never_retried() {
    let long = stream_error(GenerateError::PromptTooLong {
        tokens: 9000,
        context: 8192,
    });
    assert!(matches!(long, StreamError::Validation(_)));
    let ggml = stream_error(GenerateError::Ggml("decode failed".to_owned()));
    assert!(matches!(ggml, StreamError::Model(_)));
    let backend = stream_error(GenerateError::Backend(LoadError::Backend("x".to_owned())));
    assert!(matches!(backend, StreamError::StartFailed(_)));
    for error in [long, ggml, backend] {
        assert!(!error.is_transient(), "{error:?}");
    }
}

#[test]
fn the_finish_reason_follows_the_contract() {
    assert_eq!(finish_reason(Ended::Stop, false), "stop");
    assert_eq!(finish_reason(Ended::Length, false), "length");
    assert_eq!(finish_reason(Ended::Stop, true), "tool_calls");
    assert_eq!(finish_reason(Ended::Length, true), "tool_calls");
}

#[test]
fn one_adapter_can_be_shared_between_tasks() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<super::LlamaCppAdapter>();
}
