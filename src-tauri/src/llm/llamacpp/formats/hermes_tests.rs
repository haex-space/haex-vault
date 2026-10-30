use serde_json::json;

use super::Hermes;
use crate::llm::llamacpp::formats::ChatFormat;

#[test]
fn reads_a_call_and_keeps_the_text_around_it() {
    let text = "Sure.\n<tool_call>\n{\"name\": \"open\", \"arguments\": {\"page\": \"sync\"}}\n</tool_call>";
    let (content, calls) = Hermes.parse_calls(text, &[]);
    assert_eq!(content, "Sure.");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "open");
    assert_eq!(calls[0].input, json!({"page": "sync"}));
    assert_eq!(calls[0].id, "call_0");
}

#[test]
fn reads_two_calls_in_order() {
    let text = "<tool_call>{\"name\":\"a\",\"arguments\":{}}</tool_call>\n<tool_call>{\"name\":\"b\",\"arguments\":{\"n\":2}}</tool_call>";
    let (content, calls) = Hermes.parse_calls(text, &[]);
    assert_eq!(content, "");
    let names: Vec<_> = calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
    assert_eq!(calls[1].id, "call_1");
    assert_eq!(calls[1].input, json!({"n": 2}));
}

#[test]
fn arguments_given_as_a_json_string_are_parsed() {
    let text = r#"<tool_call>{"name": "a", "arguments": "{\"x\": 1}"}</tool_call>"#;
    assert_eq!(Hermes.parse_calls(text, &[]).1[0].input, json!({"x": 1}));
}

#[test]
fn arguments_that_are_not_json_become_null_and_the_call_is_kept() {
    let text = r#"<tool_call>{"name": "a", "arguments": "not json"}</tool_call>"#;
    let (_, calls) = Hermes.parse_calls(text, &[]);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].input, json!(null));
}

#[test]
fn missing_arguments_are_an_empty_object() {
    let text = r#"<tool_call>{"name": "a"}</tool_call>"#;
    assert_eq!(Hermes.parse_calls(text, &[]).1[0].input, json!({}));
}

#[test]
fn a_block_that_is_not_valid_json_stays_in_the_text() {
    let text =
        "<tool_call>{\"name\":\"a\",\"arguments\":{}}</tool_call><tool_call>{broken</tool_call>";
    let (content, calls) = Hermes.parse_calls(text, &[]);
    assert_eq!(calls.len(), 1);
    assert!(content.contains("{broken"));
}

#[test]
fn a_block_cut_off_before_its_closing_tag_is_read_when_it_is_complete_json() {
    let text = "<tool_call>\n{\"name\": \"a\", \"arguments\": {\"x\": 1}}";
    let (content, calls) = Hermes.parse_calls(text, &[]);
    assert_eq!(content, "");
    assert_eq!(calls[0].input, json!({"x": 1}));
}

#[test]
fn thinking_is_split_off_before_the_calls_are_read() {
    let text = "I should open it.\n</think>\n\n<tool_call>\n{\"name\": \"a\", \"arguments\": {}}\n</tool_call>";
    let parsed = Hermes.parse(text, &[]);
    assert_eq!(parsed.reasoning.as_deref(), Some("I should open it."));
    assert_eq!(parsed.content, "");
    assert_eq!(parsed.calls.len(), 1);
}
