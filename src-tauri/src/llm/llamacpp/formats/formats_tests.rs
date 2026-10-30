use serde_json::json;

use super::{default_messages, detect, scan_tool_calls, split_thinking, template_tools};
use crate::adapters::types::{ChatMessage, ChatRole, ToolSpec};

const QWEN3: &str = include_str!("../fixtures/qwen3.jinja");
const QWEN3_5: &str = include_str!("../fixtures/qwen3_5.jinja");
const GEMMA4: &str = include_str!("../fixtures/gemma4.jinja");

fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: vec![],
    }
}

fn call(id: &str, name: &str, input: serde_json::Value) -> ChatMessage {
    message(
        ChatRole::ToolCall {
            id: id.to_owned(),
            name: name.to_owned(),
            input,
        },
        "",
    )
}

#[test]
fn each_template_picks_its_format() {
    assert_eq!(detect(QWEN3).map(|f| f.name()), Some("hermes-json"));
    assert_eq!(detect(QWEN3_5).map(|f| f.name()), Some("qwen-xml"));
}

#[test]
fn a_template_in_a_syntax_the_engine_does_not_know_matches_no_format() {
    // Gemma 4 gets its format in a later stage (spec 070 PR E); until then it is refused, not guessed at.
    assert!(detect(GEMMA4).is_none());
    assert!(detect("{{ messages }}").is_none());
    assert!(detect("").is_none());
}

#[test]
fn the_xml_mark_wins_over_the_json_mark() {
    // Qwen3.5's template mentions `"arguments"` too; the `<function=` mark decides.
    assert!(QWEN3_5.contains("\"arguments\"") || QWEN3_5.contains("arguments"));
    assert_eq!(detect(QWEN3_5).map(|f| f.name()), Some("qwen-xml"));
}

#[test]
fn thinking_the_prompt_opened_is_split_off() {
    let (reasoning, text) = split_thinking("Let me see.\n</think>\n\nAnswer.");
    assert_eq!(reasoning.as_deref(), Some("Let me see."));
    assert_eq!(text, "Answer.");
}

#[test]
fn an_empty_thinking_block_gives_no_reasoning() {
    assert_eq!(
        split_thinking("<think>\n\n</think>\n\nHi"),
        (None, "Hi".to_owned())
    );
}

#[test]
fn an_answer_without_a_thinking_end_is_all_text() {
    assert_eq!(split_thinking("plain"), (None, "plain".to_owned()));
}

#[test]
fn a_thinking_block_that_opens_in_the_answer_is_split_too() {
    let (reasoning, text) = split_thinking("<think>\nhmm\n</think>\nDone.");
    assert_eq!(reasoning.as_deref(), Some("hmm"));
    assert_eq!(text, "Done.");
}

#[test]
fn history_becomes_messages_with_grouped_tool_calls() {
    let history = [
        message(ChatRole::User, "open sync and list"),
        call("1", "open", json!({"page": "sync"})),
        call("2", "list", json!({})),
        message(
            ChatRole::ToolResult {
                call_id: "1".to_owned(),
                content: "opened".to_owned(),
                is_error: false,
            },
            "",
        ),
        message(ChatRole::Assistant, "Done."),
    ];
    let messages = default_messages(Some("Be brief."), &history);
    assert_eq!(
        messages,
        vec![
            json!({"role": "system", "content": "Be brief."}),
            json!({"role": "user", "content": "open sync and list"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"type": "function", "function": {"name": "open", "arguments": {"page": "sync"}}},
                {"type": "function", "function": {"name": "list", "arguments": {}}},
            ]}),
            json!({"role": "tool", "content": "opened", "tool_call_id": "1"}),
            json!({"role": "assistant", "content": "Done."}),
        ]
    );
}

#[test]
fn tools_take_the_function_shape_templates_read() {
    let tools = [ToolSpec {
        name: "open".to_owned(),
        description: "Opens a page".to_owned(),
        input_schema: json!({"type": "object"}),
    }];
    assert_eq!(
        template_tools(&tools),
        vec![json!({"type": "function", "function": {
            "name": "open", "description": "Opens a page", "parameters": {"type": "object"},
        }})]
    );
}

#[test]
fn scanning_hands_each_block_to_the_reader_and_keeps_what_it_refuses() {
    let (content, calls) = scan_tool_calls(
        "a<tool_call>ok</tool_call>b<tool_call>no</tool_call>c",
        |inner| (inner == "ok").then(|| ("f".to_owned(), json!({}))),
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(content, "ab<tool_call>no</tool_call>c");
}
