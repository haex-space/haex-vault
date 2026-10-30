//! The history of a tool call renders through the real chat templates of the supported models without
//! error and shows the call and its result in the shape each model was trained on (spec 070 T015).

use serde_json::json;

use super::formats::{detect, template_tools, ChatFormat};
use super::template::{render, RenderInput};
use crate::adapters::types::{ChatMessage, ChatRole, ToolSpec};

const QWEN3: &str = include_str!("fixtures/qwen3.jinja");
const QWEN3_5: &str = include_str!("fixtures/qwen3_5.jinja");
const GEMMA4: &str = include_str!("fixtures/gemma4.jinja");

fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: vec![],
    }
}

fn tool() -> ToolSpec {
    ToolSpec {
        name: "open_settings".to_owned(),
        description: "Opens a settings page".to_owned(),
        input_schema: json!({
            "type": "object",
            "properties": {"page": {"type": "string"}},
            "required": ["page"],
        }),
    }
}

fn tool_history() -> Vec<ChatMessage> {
    vec![
        message(ChatRole::User, "Open the sync settings."),
        message(
            ChatRole::ToolCall {
                id: "call_0".to_owned(),
                name: "open_settings".to_owned(),
                input: json!({"page": "sync"}),
            },
            "",
        ),
        message(
            ChatRole::ToolResult {
                call_id: "call_0".to_owned(),
                content: "opened the sync page".to_owned(),
                is_error: false,
            },
            "",
        ),
    ]
}

fn render_with(
    template: &str,
    format: &dyn ChatFormat,
    history: &[ChatMessage],
    tools: &[ToolSpec],
    enable_thinking: bool,
) -> String {
    render(
        template,
        &RenderInput {
            messages: format.shape_messages(Some("You are a helpful assistant."), history),
            tools: template_tools(tools),
            enable_thinking,
            bos_token: "<|bos|>",
            eos_token: "<|eos|>",
        },
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn qwen3_shows_the_call_as_json_and_the_result_as_a_response() {
    let format = detect(QWEN3).unwrap();
    let out = render_with(QWEN3, format, &tool_history(), &[tool()], false);
    assert!(out.contains("<tools>"), "{out}");
    assert!(out.contains(r#""name":"open_settings""#), "{out}");
    assert!(out.contains("<tool_call>"), "{out}");
    assert!(
        out.contains(r#"{"name": "open_settings", "arguments": {"page":"sync"}}"#),
        "{out}"
    );
    assert!(
        out.contains("<tool_response>\nopened the sync page\n</tool_response>"),
        "{out}"
    );
    assert!(
        out.ends_with("<|im_start|>assistant\n<think>\n\n</think>\n\n"),
        "{out}"
    );
}

#[test]
fn qwen3_5_shows_the_call_as_xml_and_the_result_as_a_response() {
    let format = detect(QWEN3_5).unwrap();
    let out = render_with(QWEN3_5, format, &tool_history(), &[tool()], false);
    assert!(out.contains("<function=open_settings>"), "{out}");
    assert!(
        out.contains("<parameter=page>\nsync\n</parameter>"),
        "{out}"
    );
    assert!(
        out.contains("<tool_response>\nopened the sync page\n</tool_response>"),
        "{out}"
    );
    assert!(
        out.ends_with("<|im_start|>assistant\n<think>\n\n</think>\n\n"),
        "{out}"
    );
}

#[test]
fn thinking_on_leaves_the_block_open_for_the_model() {
    let format = detect(QWEN3_5).unwrap();
    let history = [message(ChatRole::User, "Hi")];
    let out = render_with(QWEN3_5, format, &history, &[], true);
    assert!(out.ends_with("<|im_start|>assistant\n<think>\n"), "{out}");
}

#[test]
fn a_plain_question_renders_without_tools_through_every_fixture() {
    let history = [message(ChatRole::User, "Name three uses of a paper clip.")];
    // Gemma 4 has no format yet; the default message shape is enough for a plain question.
    let format = detect(QWEN3).unwrap();
    for (name, template) in [("qwen3", QWEN3), ("qwen3.5", QWEN3_5), ("gemma4", GEMMA4)] {
        let out = render_with(template, format, &history, &[], false);
        assert!(
            out.contains("Name three uses of a paper clip."),
            "{name}: {out}"
        );
        assert!(!out.contains("<tools>"), "{name}: {out}");
    }
}

#[test]
fn the_gemma4_template_renders_tools_in_its_own_syntax() {
    let format = detect(QWEN3).unwrap();
    let out = render_with(GEMMA4, format, &tool_history(), &[tool()], false);
    assert!(out.contains("<|tool_call>call:open_settings{"), "{out}");
    assert!(out.contains("response:open_settings{"), "{out}");
}

#[test]
fn the_system_prompt_comes_first() {
    let format = detect(QWEN3).unwrap();
    let history = [message(ChatRole::User, "Hi")];
    let out = render_with(QWEN3, format, &history, &[], false);
    assert!(
        out.starts_with("<|im_start|>system\nYou are a helpful assistant.<|im_end|>"),
        "{out}"
    );
}
