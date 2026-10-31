use serde_json::json;

use super::Streamer;
use crate::adapters::types::{StreamChunk, ToolCall, ToolSpec};
use crate::llm::llamacpp::formats::{Gemma4, Hermes, QwenXml};

#[derive(Debug, Default, PartialEq)]
struct Collected {
    content: String,
    reasoning: String,
    calls: Vec<ToolCall>,
    saw_tool_calls: bool,
}

impl Collected {
    fn take(&mut self, chunks: Vec<StreamChunk>) {
        for chunk in chunks {
            match chunk {
                StreamChunk::Delta { content, reasoning } => {
                    self.content.push_str(&content);
                    self.reasoning.push_str(&reasoning.unwrap_or_default());
                }
                StreamChunk::ToolCalls(calls) => self.calls.extend(calls),
                other => panic!("unexpected chunk {other:?}"),
            }
        }
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

/// Feeds `pieces`, then ends the generation.
fn run(mut streamer: Streamer, pieces: &[&str]) -> Collected {
    let mut out = Collected::default();
    for piece in pieces {
        out.take(streamer.push(piece));
    }
    let finished = streamer.finish();
    out.saw_tool_calls = finished.tool_calls;
    out.take(finished.chunks);
    out
}

fn hermes() -> Streamer {
    Streamer::new(Some(&Hermes), vec![tool()], false)
}

#[test]
fn plain_text_streams_through_without_a_format() {
    let out = run(
        Streamer::new(None, vec![], false),
        &["Hel", "lo ", "there."],
    );
    assert_eq!(out.content, "Hello there.");
    assert_eq!(out.reasoning, "");
    assert!(!out.saw_tool_calls);
}

#[test]
fn every_piece_is_shown_as_it_comes() {
    let mut streamer = Streamer::new(None, vec![], false);
    let first = streamer.push("Hello");
    assert!(matches!(&first[..], [StreamChunk::Delta { content, .. }] if content == "Hello"));
    let second = streamer.push(" world");
    assert!(matches!(&second[..], [StreamChunk::Delta { content, .. }] if content == " world"));
}

#[test]
fn a_prompt_that_opened_thinking_starts_with_reasoning() {
    let out = run(
        Streamer::new(None, vec![], true),
        &["\nLet me ", "think.\n", "</thi", "nk>\n\nThe answer."],
    );
    assert_eq!(out.reasoning, "Let me think.");
    assert_eq!(out.content, "The answer.");
}

#[test]
fn a_think_block_the_model_opens_itself_is_reasoning_too() {
    let out = run(
        Streamer::new(None, vec![], false),
        &["<thi", "nk>\nHmm", ".\n</think>", "\n\nDone."],
    );
    assert_eq!(out.reasoning, "Hmm.");
    assert_eq!(out.content, "Done.");
}

#[test]
fn text_starting_like_a_think_tag_but_not_one_is_content() {
    let out = run(
        Streamer::new(None, vec![], false),
        &["<th", "ing> is a tag"],
    );
    assert_eq!(out.content, "<thing> is a tag");
    assert_eq!(out.reasoning, "");
}

#[test]
fn a_cut_off_thought_is_reasoning_and_leaves_no_content() {
    let out = run(
        Streamer::new(None, vec![], true),
        &["Still thinking", " about it\n"],
    );
    assert_eq!(out.reasoning, "Still thinking about it");
    assert_eq!(out.content, "");
}

#[test]
fn a_tool_call_is_held_back_and_arrives_as_a_call() {
    let out = run(
        hermes(),
        &[
            "Let me check. <tool_",
            "call>\n{\"name\": \"open_settings\", ",
            "\"arguments\": {\"page\": \"sync\"}}\n</tool_call>",
        ],
    );
    assert_eq!(out.content, "Let me check. ");
    assert!(out.saw_tool_calls);
    assert_eq!(
        out.calls,
        vec![ToolCall {
            id: "call_0".to_owned(),
            name: "open_settings".to_owned(),
            input: json!({"page": "sync"}),
        }]
    );
}

#[test]
fn a_qwen_xml_call_after_thinking_is_read() {
    let out = run(
        Streamer::new(Some(&QwenXml), vec![tool()], true),
        &[
            "The user wants sync.\n</think>\n\n",
            "<tool_call>\n<function=open_settings>\n<parameter=page>\nsync\n</parameter>\n</function>\n</tool_call>",
        ],
    );
    assert_eq!(out.reasoning, "The user wants sync.");
    assert_eq!(out.content, "");
    assert_eq!(out.calls.len(), 1);
    assert_eq!(out.calls[0].input, json!({"page": "sync"}));
}

#[test]
fn a_marker_that_never_completes_is_ordinary_text() {
    let out = run(hermes(), &["Use the <tool"]);
    assert_eq!(out.content, "Use the <tool");
    assert!(!out.saw_tool_calls);
}

#[test]
fn a_call_block_that_cannot_be_read_stays_in_the_text() {
    let out = run(hermes(), &["<tool_call>\nnot json\n</tool_call>"]);
    assert_eq!(out.content, "<tool_call>\nnot json\n</tool_call>");
    assert!(out.calls.is_empty());
    assert!(!out.saw_tool_calls);
}

#[test]
fn text_after_the_call_block_is_shown_at_the_end() {
    let out = run(
        hermes(),
        &[
            "Checking.\n<tool_call>{\"name\": \"open_settings\", \"arguments\": {\"page\": \"a\"}}</tool_call>\nBye.",
        ],
    );
    assert_eq!(out.content, "Checking.\nBye.");
    assert_eq!(out.calls.len(), 1);
}

#[test]
fn a_call_cut_off_by_the_token_limit_is_read_up_to_its_end() {
    let out = run(
        hermes(),
        &["<tool_call>\n{\"name\": \"open_settings\", \"arguments\": {\"page\": \"sync\"}}"],
    );
    assert!(out.saw_tool_calls);
    assert_eq!(out.calls[0].name, "open_settings");
}

#[test]
fn the_prompt_ending_in_an_open_think_tag_is_recognised() {
    assert!(Streamer::prompt_opens_thinking(
        "…<|im_start|>assistant\n<think>\n",
        None
    ));
    assert!(!Streamer::prompt_opens_thinking(
        "…assistant\n<think>\n\n</think>\n\n",
        None
    ));
    assert!(!Streamer::prompt_opens_thinking("…assistant\n", None));
    // Gemma 4 opens its thought channel after a tool result when thinking is on.
    assert!(Streamer::prompt_opens_thinking(
        "…<tool_response|><|channel>thought\n",
        Some(&Gemma4)
    ));
    assert!(!Streamer::prompt_opens_thinking(
        "…<|turn>model\n",
        Some(&Gemma4)
    ));
}

fn gemma4(thinking_open: bool) -> Streamer {
    Streamer::new(Some(&Gemma4), vec![tool()], thinking_open)
}

#[test]
fn a_gemma4_thought_is_reasoning_and_its_call_arrives_as_a_call() {
    let out = run(
        gemma4(false),
        &[
            "<|channel>",
            "thought",
            "\nThe user wants sync.",
            "<channel|>",
            "<|tool_call>",
            "call:open_settings{page:",
            "<|\"|>",
            "sync",
            "<|\"|>",
            "}",
            "<tool_call|>",
        ],
    );
    assert_eq!(out.reasoning, "The user wants sync.");
    assert_eq!(out.content, "");
    assert!(out.saw_tool_calls);
    assert_eq!(out.calls[0].name, "open_settings");
    assert_eq!(out.calls[0].input, json!({"page": "sync"}));
}

#[test]
fn an_empty_gemma4_thought_after_the_answer_is_not_shown() {
    let out = run(
        gemma4(false),
        &["Done", ".", "<|channel>", "thought", "\n", "<channel|>"],
    );
    assert_eq!(out.content, "Done.");
    assert_eq!(out.reasoning, "");
    assert!(!out.saw_tool_calls);
}

#[test]
fn a_gemma4_thought_the_prompt_opened_ends_at_the_closing_tag() {
    let out = run(
        gemma4(true),
        &[
            "Checking",
            " the result.",
            "<channel|>",
            "It is open.",
            "<channel|>",
        ],
    );
    assert_eq!(out.reasoning, "Checking the result.");
    assert_eq!(out.content, "It is open.");
}
