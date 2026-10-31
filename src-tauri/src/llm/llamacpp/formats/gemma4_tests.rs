use serde_json::{json, Value};

use super::Gemma4;
use crate::adapters::types::{ChatMessage, ChatRole, ToolSpec};
use crate::llm::llamacpp::formats::ChatFormat;

const GEMMA4: &str = include_str!("../fixtures/gemma4.jinja");
const QWEN3: &str = include_str!("../fixtures/qwen3.jinja");
const QWEN3_5: &str = include_str!("../fixtures/qwen3_5.jinja");

fn tools() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "open_settings".to_owned(),
        description: "Opens a settings page".to_owned(),
        input_schema: json!({"type": "object", "properties": {"page": {"type": "string"}}}),
    }]
}

/// Reads `text` and returns (content, [(name, input)]).
fn parse(text: &str) -> (String, Vec<(String, Value)>) {
    let parsed = Gemma4.parse(text, &tools());
    let calls = parsed
        .calls
        .into_iter()
        .map(|c| (c.name, c.input))
        .collect();
    (parsed.content, calls)
}

fn one_call(text: &str) -> Value {
    let (_, calls) = parse(text);
    assert_eq!(calls.len(), 1, "{text}");
    calls.into_iter().next().unwrap().1
}

#[test]
fn the_fingerprint_matches_gemma4_only() {
    assert!(Gemma4.fingerprint(GEMMA4));
    assert!(!Gemma4.fingerprint(QWEN3));
    assert!(!Gemma4.fingerprint(QWEN3_5));
}

#[test]
fn a_string_argument() {
    let (content, calls) =
        parse("<|tool_call>call:open_settings{page:<|\"|>sync<|\"|>}<tool_call|>");
    assert_eq!(content, "");
    assert_eq!(
        calls,
        vec![("open_settings".to_owned(), json!({"page": "sync"}))]
    );
}

#[test]
fn numbers_booleans_and_null_are_bare() {
    assert_eq!(
        one_call("<|tool_call>call:f{a:5,b:-1.5,c:true,d:false,e:null}<tool_call|>"),
        json!({"a": 5, "b": -1.5, "c": true, "d": false, "e": null})
    );
}

#[test]
fn objects_and_arrays_nest() {
    assert_eq!(
        one_call(
            "<|tool_call>call:f{filter:{tags:[<|\"|>a<|\"|>,<|\"|>b<|\"|>],limit:3},ids:[1,2],empty:{},none:[]}<tool_call|>"
        ),
        json!({"filter": {"tags": ["a", "b"], "limit": 3}, "ids": [1, 2], "empty": {}, "none": []})
    );
}

#[test]
fn a_string_keeps_commas_braces_colons_and_newlines() {
    assert_eq!(
        one_call("<|tool_call>call:f{text:<|\"|>a, {b}: c\nd]<|\"|>}<tool_call|>"),
        json!({"text": "a, {b}: c\nd]"})
    );
}

#[test]
fn an_empty_string() {
    assert_eq!(
        one_call("<|tool_call>call:f{text:<|\"|><|\"|>}<tool_call|>"),
        json!({"text": ""})
    );
}

#[test]
fn spaces_between_the_parts_are_allowed() {
    assert_eq!(
        one_call("<|tool_call>call:f{ page : <|\"|>sync<|\"|> , n : 5 }<tool_call|>"),
        json!({"page": "sync", "n": 5})
    );
}

#[test]
fn a_key_in_string_quotes_is_read_too() {
    assert_eq!(
        one_call("<|tool_call>call:f{<|\"|>page<|\"|>:<|\"|>sync<|\"|>}<tool_call|>"),
        json!({"page": "sync"})
    );
}

#[test]
fn a_bare_word_the_model_left_unquoted_is_a_string() {
    assert_eq!(
        one_call("<|tool_call>call:f{page:sync,n:2}<tool_call|>"),
        json!({"page": "sync", "n": 2})
    );
}

#[test]
fn a_call_without_arguments() {
    assert_eq!(
        one_call("<|tool_call>call:list_tabs{}<tool_call|>"),
        json!({})
    );
    assert_eq!(
        one_call("<|tool_call>call:list_tabs<tool_call|>"),
        json!({})
    );
}

#[test]
fn several_calls_in_one_turn_keep_their_order() {
    let (_, calls) =
        parse("<|tool_call>call:a{n:1}<tool_call|><|tool_call>call:b{n:2}<tool_call|>");
    let names: Vec<_> = calls.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
}

#[test]
fn text_before_the_call_is_content() {
    let (content, calls) =
        parse("Let me open it.<|tool_call>call:open_settings{page:<|\"|>sync<|\"|>}<tool_call|>");
    assert_eq!(content, "Let me open it.");
    assert_eq!(calls.len(), 1);
}

#[test]
fn a_call_cut_off_after_its_object_is_read() {
    assert_eq!(
        one_call("<|tool_call>call:open_settings{page:<|\"|>sync<|\"|>}"),
        json!({"page": "sync"})
    );
}

#[test]
fn a_call_cut_off_inside_its_object_stays_text() {
    for text in [
        "<|tool_call>call:open_settings{page:<|\"|>sy",
        "<|tool_call>call:open_settings{page:",
        "<|tool_call>call:open_settings{n:5",
    ] {
        let (content, calls) = parse(text);
        assert!(calls.is_empty(), "{text}");
        assert!(content.starts_with("<|tool_call>"), "{text}: {content}");
    }
}

#[test]
fn a_block_without_call_prefix_or_name_stays_text() {
    for text in [
        "<|tool_call>open_settings{}<tool_call|>",
        "<|tool_call>call:{}<tool_call|>",
        "<|tool_call>call:open settings{}<tool_call|>",
    ] {
        let (content, calls) = parse(text);
        assert!(calls.is_empty(), "{text}");
        assert_eq!(content, text);
    }
}

#[test]
fn a_thought_is_reasoning() {
    let parsed = Gemma4.parse(
        "<|channel>thought\nThe user wants sync.<channel|>Opening it.",
        &tools(),
    );
    assert_eq!(parsed.reasoning.as_deref(), Some("The user wants sync."));
    assert_eq!(parsed.content, "Opening it.");
}

#[test]
fn empty_thoughts_stray_channels_and_lone_closing_tags_are_dropped() {
    let parsed = Gemma4.parse(
        "<|channel>Hello.<|channel>thought\n<channel|><channel|>",
        &tools(),
    );
    assert_eq!(parsed.reasoning, None);
    assert_eq!(parsed.content, "Hello.");
}

#[test]
fn a_thought_cut_off_by_the_token_limit_is_reasoning() {
    let parsed = Gemma4.parse("<|channel>thought\nStill thin", &tools());
    assert_eq!(parsed.reasoning.as_deref(), Some("Still thin"));
    assert_eq!(parsed.content, "");
}

fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: vec![],
    }
}

fn call(id: &str, name: &str, input: Value) -> ChatMessage {
    message(
        ChatRole::ToolCall {
            id: id.to_owned(),
            name: name.to_owned(),
            input,
        },
        "",
    )
}

fn result(call_id: &str, content: &str) -> ChatMessage {
    message(
        ChatRole::ToolResult {
            call_id: call_id.to_owned(),
            content: content.to_owned(),
            is_error: false,
        },
        "",
    )
}

#[test]
fn tool_results_and_the_answer_after_them_join_the_turn_that_called() {
    let history = [
        message(ChatRole::User, "Open sync and list the tabs."),
        call("call_0", "open_settings", json!({"page": "sync"})),
        call("call_1", "list_tabs", json!({})),
        // Results may come back in another order than the calls went out.
        result("call_1", r#"{"tabs": 2}"#),
        result("call_0", "opened"),
        message(ChatRole::Assistant, "Done: two tabs."),
        message(ChatRole::User, "Thanks."),
    ];
    let shaped = Gemma4.shape_messages(None, &history);
    assert_eq!(shaped.len(), 3, "{shaped:#?}");
    let turn = &shaped[1];
    assert_eq!(turn["role"], "assistant");
    assert_eq!(turn["tool_calls"].as_array().unwrap().len(), 2);
    assert_eq!(
        turn["tool_responses"],
        json!([
            {"name": "list_tabs", "response": {"tabs": 2}},
            {"name": "open_settings", "response": "opened"},
        ])
    );
    assert_eq!(turn["content"], "Done: two tabs.");
    assert_eq!(shaped[2]["content"], "Thanks.");
}

#[test]
fn calls_without_results_yet_stay_as_they_are() {
    let history = [
        message(ChatRole::User, "Open sync."),
        call("call_0", "open_settings", json!({"page": "sync"})),
    ];
    let shaped = Gemma4.shape_messages(Some("Be brief."), &history);
    assert_eq!(shaped.len(), 3);
    assert!(shaped[2].get("tool_responses").is_none());
}
