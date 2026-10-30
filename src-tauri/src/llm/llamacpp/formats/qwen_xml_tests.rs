use serde_json::json;

use super::QwenXml;
use crate::adapters::types::ToolSpec;
use crate::llm::llamacpp::formats::ChatFormat;

fn tool(name: &str, properties: serde_json::Value) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        input_schema: json!({"type": "object", "properties": properties}),
    }
}

#[test]
fn reads_a_call_and_types_each_parameter_by_the_schema() {
    let tools = [tool(
        "set",
        json!({"label": {"type": "string"}, "count": {"type": "integer"}, "on": {"type": "boolean"}}),
    )];
    let text = "<tool_call>\n<function=set>\n<parameter=label>\n42\n</parameter>\n<parameter=count>\n3\n</parameter>\n<parameter=on>\ntrue\n</parameter>\n</function>\n</tool_call>";
    let (content, calls) = QwenXml.parse_calls(text, &tools);
    assert_eq!(content, "");
    assert_eq!(calls[0].name, "set");
    assert_eq!(
        calls[0].input,
        json!({"label": "42", "count": 3, "on": true})
    );
}

#[test]
fn a_value_loses_exactly_one_newline_at_each_end() {
    let tools = [tool("note", json!({"text": {"type": "string"}}))];
    let text = "<tool_call><function=note><parameter=text>\n\nline one\nline two\n\n</parameter></function></tool_call>";
    let (_, calls) = QwenXml.parse_calls(text, &tools);
    assert_eq!(calls[0].input, json!({"text": "\nline one\nline two\n"}));
}

#[test]
fn an_object_parameter_is_read_as_json() {
    let tools = [tool("f", json!({"opts": {"type": "object"}}))];
    let text = "<tool_call><function=f><parameter=opts>\n{\"a\": [1, 2]}\n</parameter></function></tool_call>";
    assert_eq!(
        QwenXml.parse_calls(text, &tools).1[0].input,
        json!({"opts": {"a": [1, 2]}})
    );
}

#[test]
fn a_parameter_the_schema_does_not_name_is_json_or_else_a_string() {
    let text = "<tool_call><function=f><parameter=n>\n7\n</parameter><parameter=s>\nhello\n</parameter></function></tool_call>";
    assert_eq!(
        QwenXml.parse_calls(text, &[]).1[0].input,
        json!({"n": 7, "s": "hello"})
    );
}

#[test]
fn a_value_that_is_not_json_for_a_non_string_type_stays_a_string() {
    let tools = [tool("f", json!({"n": {"type": "integer"}}))];
    let text = "<tool_call><function=f><parameter=n>\nseven\n</parameter></function></tool_call>";
    assert_eq!(
        QwenXml.parse_calls(text, &tools).1[0].input,
        json!({"n": "seven"})
    );
}

#[test]
fn a_call_without_parameters_has_empty_arguments() {
    let text = "<tool_call>\n<function=list_things>\n</function>\n</tool_call>";
    let (_, calls) = QwenXml.parse_calls(text, &[]);
    assert_eq!(calls[0].name, "list_things");
    assert_eq!(calls[0].input, json!({}));
}

#[test]
fn a_block_cut_off_by_the_token_limit_keeps_the_finished_parameters_and_drops_the_open_one() {
    let text = "<tool_call>\n<function=f>\n<parameter=a>\n1\n</parameter>\n<parameter=b>\nhal";
    let (content, calls) = QwenXml.parse_calls(text, &[]);
    assert_eq!(content, "");
    assert_eq!(calls[0].input, json!({"a": 1}));
}

#[test]
fn two_calls_and_the_text_between_them() {
    let text = "First.\n<tool_call><function=a></function></tool_call>\nThen.\n<tool_call><function=b></function></tool_call>";
    let (content, calls) = QwenXml.parse_calls(text, &[]);
    assert_eq!(content, "First.\n\nThen.");
    let names: Vec<_> = calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
}

#[test]
fn a_block_without_a_function_stays_in_the_text() {
    let text = "<tool_call>nonsense</tool_call>";
    let (content, calls) = QwenXml.parse_calls(text, &[]);
    assert!(calls.is_empty());
    assert_eq!(content, text);
}
