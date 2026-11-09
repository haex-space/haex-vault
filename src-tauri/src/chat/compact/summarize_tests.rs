use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{chunk_ends, clip_summary, instruction, line, summarize, SummaryError, SUMMARY_TOKENS};
use crate::adapters::types::{ChatRequest, ChatRole, StreamChunk};
use crate::chat::eval::testing::Scripted;
use crate::chat::reply_language::ReplyLanguage;
use crate::storage::chat_messages::{ChatMessage, MessageRole};

fn row(role: MessageRole, content: &str) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        thread_id: Uuid::nil(),
        parent_id: None,
        role,
        content: content.to_owned(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: None,
        created_at: 0,
        idempotency_key: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_is_error: None,
        tool_source: None,
        autonomy_mode: None,
        tool_origin: None,
    }
}

fn say(text: &str) -> Vec<StreamChunk> {
    vec![StreamChunk::Delta {
        content: text.to_owned(),
        reasoning: None,
    }]
}

fn refs(rows: &[ChatMessage]) -> Vec<&ChatMessage> {
    rows.iter().collect()
}

fn text_of(request: &ChatRequest) -> &str {
    &request.messages[0].content
}

#[test]
fn every_kind_of_row_becomes_a_line_and_a_tool_result_is_clipped() {
    assert_eq!(
        line(&row(MessageRole::User, "open the page")).unwrap(),
        "Person: open the page"
    );
    assert_eq!(
        line(&row(MessageRole::Assistant, " done ")).unwrap(),
        "Assistant: done"
    );
    assert_eq!(line(&row(MessageRole::Assistant, "")), None);
    assert_eq!(line(&row(MessageRole::System, "note")), None);
    let call = ChatMessage {
        tool_name: Some("browser_open".to_owned()),
        tool_input: Some(r#"{"url":"https://example.org"}"#.to_owned()),
        ..row(MessageRole::ToolCall, "")
    };
    assert_eq!(
        line(&call).unwrap(),
        r#"Assistant used the tool browser_open with {"url":"https://example.org"}"#
    );
    let result = line(&row(MessageRole::ToolResult, &"x".repeat(5000))).unwrap();
    assert!(result.chars().count() < 1700, "{}", result.len());
    assert!(result.ends_with("[…]"));
}

#[test]
fn chunks_end_where_the_next_line_would_not_fit() {
    let lines: Vec<String> = (0..5).map(|_| "x".repeat(99)).collect();
    // 100 characters a line with its break; a budget of 250 takes two.
    assert_eq!(chunk_ends(&lines, 250), vec![2, 4, 5]);
    assert_eq!(chunk_ends(&lines, 10_000), vec![5]);
    assert!(chunk_ends(&[], 100).is_empty());
}

#[test]
fn a_summary_that_is_too_long_is_cut_at_a_paragraph_end() {
    let paragraph = format!("{}.", "word ".repeat(300));
    let long = std::iter::repeat_n(paragraph.as_str(), 6)
        .collect::<Vec<_>>()
        .join("\n\n");
    let clipped = clip_summary(&long);
    assert!(clipped.chars().count() <= SUMMARY_TOKENS * 3);
    assert!(clipped.ends_with('.'));
    assert_eq!(clip_summary("  short  "), "short");
}

#[tokio::test]
async fn one_call_writes_the_summary_of_the_rows_without_tools() {
    let model = Scripted::new(|_| say("The person wants the weather in Oslo."));
    let rows = vec![
        row(MessageRole::User, "what is the weather in Oslo"),
        row(MessageRole::Assistant, "Let me look."),
    ];
    let written = summarize(
        &model,
        "m",
        None,
        &refs(&rows),
        8192,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(written.text, "The person wants the weather in Oslo.");
    assert_eq!(written.covered, 2);
    let seen = model.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].tools.is_empty());
    assert!(!seen[0].reasoning_requested);
    assert_eq!(
        seen[0].system_prompt.as_deref(),
        Some(instruction(None).as_str())
    );
    assert!(matches!(seen[0].messages[0].role, ChatRole::User));
    assert!(text_of(&seen[0]).contains("Person: what is the weather in Oslo"));
    assert!(text_of(&seen[0]).contains("Assistant: Let me look."));
}

#[tokio::test]
async fn an_earlier_summary_goes_in_first() {
    let model = Scripted::new(|_| say("new"));
    let rows = vec![row(MessageRole::User, "next")];
    summarize(
        &model,
        "m",
        Some("old summary"),
        &refs(&rows),
        8192,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let seen = model.seen.lock().unwrap();
    assert!(text_of(&seen[0])
        .starts_with("Summary so far:\nold summary\n\nMessages since then:\nPerson: next"));
}

#[tokio::test]
async fn a_long_span_goes_in_chunks_and_each_summary_is_the_earlier_one_of_the_next() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let model = Scripted::new(move |_| {
        say(&format!(
            "summary {}",
            counter.fetch_add(1, Ordering::SeqCst) + 1
        ))
    });
    // A context of 6000 tokens leaves about 11000 characters for messages: two lines of 5000 fit, not three.
    let rows: Vec<ChatMessage> = (0..5)
        .map(|n| row(MessageRole::User, &format!("{n} {}", "a".repeat(5000))))
        .collect();
    let written = summarize(
        &model,
        "m",
        None,
        &refs(&rows),
        6000,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(written.text, "summary 3");
    assert_eq!(written.covered, 5);
    let seen = model.seen.lock().unwrap();
    assert!(text_of(&seen[1]).starts_with("Summary so far:\nsummary 1"));
    assert!(text_of(&seen[2]).starts_with("Summary so far:\nsummary 2"));
}

#[tokio::test]
async fn the_calls_are_limited_and_the_summary_covers_what_they_covered() {
    let model = Scripted::new(|_| say("part"));
    let rows: Vec<ChatMessage> = (0..20)
        .map(|n| row(MessageRole::User, &format!("{n} {}", "a".repeat(8000))))
        .collect();
    let written = summarize(
        &model,
        "m",
        None,
        &refs(&rows),
        6000,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(model.seen.lock().unwrap().len(), 4);
    assert!(written.covered < rows.len(), "{}", written.covered);
    assert!(written.covered > 0);
}

#[tokio::test]
async fn nothing_written_is_a_failure_and_a_later_failure_keeps_what_came_before() {
    let empty = Scripted::new(|_| say("   "));
    let rows = vec![row(MessageRole::User, "hello")];
    assert!(matches!(
        summarize(
            &empty,
            "m",
            None,
            &refs(&rows),
            8192,
            None,
            &CancellationToken::new()
        )
        .await,
        Err(SummaryError::Failed(_))
    ));

    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let second_fails = Scripted::new(move |_| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            say("first part")
        } else {
            say("")
        }
    });
    let rows: Vec<ChatMessage> = (0..4)
        .map(|n| row(MessageRole::User, &format!("{n} {}", "a".repeat(5000))))
        .collect();
    let written = summarize(
        &second_fails,
        "m",
        None,
        &refs(&rows),
        6000,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(written.text, "first part");
    assert!(written.covered < rows.len());
}

#[tokio::test]
async fn a_cancelled_turn_writes_nothing() {
    let model = Scripted::new(|_| say("never"));
    let cancel = CancellationToken::new();
    cancel.cancel();
    let rows = vec![row(MessageRole::User, "hello")];
    assert_eq!(
        summarize(&model, "m", None, &refs(&rows), 8192, None, &cancel).await,
        Err(SummaryError::Cancelled)
    );
}

#[tokio::test]
async fn a_context_too_small_to_hold_the_instruction_and_the_answer_is_a_failure() {
    let model = Scripted::new(|_| say("never"));
    let rows = vec![row(MessageRole::User, "hello")];
    assert!(matches!(
        summarize(
            &model,
            "m",
            None,
            &refs(&rows),
            2048,
            None,
            &CancellationToken::new()
        )
        .await,
        Err(SummaryError::Failed(_))
    ));
    assert!(model.seen.lock().unwrap().is_empty());
}

#[test]
fn the_summary_is_asked_for_in_the_language_of_the_turn() {
    assert!(instruction(Some(ReplyLanguage::De)).contains("in German"));
    assert!(instruction(None).contains("in the language the person writes in"));
}
