//! Unit tests for the reasoning-mode `<tool_call>` leak buffer. Pure
//! string manipulation, no model/stream involved — the end-to-end
//! streaming behavior is covered by `tests/local_inference.rs` (gated on
//! `HOLZI_TEST_GGUF`) and `tests/chat_tool_loop.rs`'s scripted-adapter
//! coverage.

use super::*;

#[test]
fn emits_plain_content_immediately() {
    let mut buffer = String::new();
    let emitted = drain_reasoning_leak_buffer(&mut buffer, "hello there");
    assert_eq!(emitted, "hello there");
    assert_eq!(buffer, "");
}

#[test]
fn drops_a_tag_that_arrives_in_a_single_chunk() {
    let mut buffer = String::new();
    let emitted =
        drain_reasoning_leak_buffer(&mut buffer, "<tool_call>{\"name\": \"echo\"}</tool_call>");
    assert_eq!(emitted, "");
    assert_eq!(buffer, "");
}

#[test]
fn keeps_prose_before_the_tag_and_drops_the_tag() {
    let mut buffer = String::new();
    let emitted = drain_reasoning_leak_buffer(
        &mut buffer,
        "Let me check.\n<tool_call>{\"name\": \"echo\"}</tool_call>",
    );
    assert_eq!(emitted, "Let me check.\n");
    assert_eq!(buffer, "");
}

#[test]
fn keeps_prose_after_the_tag_too() {
    let mut buffer = String::new();
    let emitted = drain_reasoning_leak_buffer(
        &mut buffer,
        "<tool_call>{\"name\": \"echo\"}</tool_call>done",
    );
    assert_eq!(emitted, "done");
    assert_eq!(buffer, "");
}

/// The realistic case: mistralrs streams a handful of bytes per chunk, so
/// the open tag, the JSON body, and the close tag each arrive split across
/// several calls — and the split points don't line up with the tag
/// boundaries.
#[test]
fn drops_a_tag_split_across_many_chunks() {
    let mut buffer = String::new();
    let mut emitted = String::new();
    for piece in [
        "Sure, ",
        "<tool",
        "_call>",
        "{\"name\":",
        " \"echo\"}",
        "</tool_",
        "call>",
        " done",
    ] {
        emitted.push_str(&drain_reasoning_leak_buffer(&mut buffer, piece));
    }
    assert_eq!(emitted, "Sure,  done");
    assert_eq!(buffer, "");
}

#[test]
fn holds_back_an_open_tag_with_no_close_yet() {
    let mut buffer = String::new();
    let emitted = drain_reasoning_leak_buffer(&mut buffer, "Sure.\n<tool_call>{\"name\": \"ec");
    assert_eq!(emitted, "Sure.\n");
    assert_eq!(buffer, "<tool_call>{\"name\": \"ec");
}

#[test]
fn holds_back_a_partial_open_tag_prefix() {
    let mut buffer = String::new();
    let emitted = drain_reasoning_leak_buffer(&mut buffer, "one moment <tool_c");
    assert_eq!(emitted, "one moment ");
    assert_eq!(buffer, "<tool_c");
}

#[test]
fn drops_a_confirmed_open_tag_at_stream_end() {
    let mut buffer = String::from("<tool_call>{\"name\": \"echo\"");
    assert_eq!(take_terminal_reasoning_leak_buffer(&mut buffer), None);
    assert_eq!(buffer, "");
}

#[test]
fn flushes_a_partial_open_tag_prefix_at_stream_end() {
    let mut buffer = String::from("<tool_c");
    assert_eq!(
        take_terminal_reasoning_leak_buffer(&mut buffer),
        Some(String::from("<tool_c"))
    );
    assert_eq!(buffer, "");
}

#[test]
fn text_that_only_resembles_the_prefix_is_eventually_emitted() {
    // "<tool_c" looked like the start of a tag, but the next chunk takes a
    // different turn — never becomes `<tool_call>`, so it must not be
    // swallowed.
    let mut buffer = String::new();
    let mut emitted = String::new();
    emitted.push_str(&drain_reasoning_leak_buffer(
        &mut buffer,
        "one moment <tool_c",
    ));
    emitted.push_str(&drain_reasoning_leak_buffer(&mut buffer, "hain of thought"));
    assert_eq!(emitted, "one moment <tool_chain of thought");
    assert_eq!(buffer, "");
}

#[test]
fn drops_two_consecutive_tags() {
    let mut buffer = String::new();
    let emitted = drain_reasoning_leak_buffer(
        &mut buffer,
        "<tool_call>{\"name\": \"a\"}</tool_call><tool_call>{\"name\": \"b\"}</tool_call>",
    );
    assert_eq!(emitted, "");
    assert_eq!(buffer, "");
}

#[test]
fn a_tool_found_by_the_search_is_forwarded_as_a_mistralrs_function() {
    let spec = ToolSpec {
        name: "settings_sync_setEnabled".to_string(),
        description: "Turn sync on or off".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": { "enabled": { "type": "boolean" } },
        }),
    };
    let tool = to_mistralrs_tool(&spec);
    assert_eq!(tool.function.name, "settings_sync_setEnabled");
    assert_eq!(
        tool.function.description.as_deref(),
        Some("Turn sync on or off")
    );
    let parameters = tool
        .function
        .parameters
        .expect("object schema becomes parameters");
    assert!(parameters.contains_key("properties"));
}
