use uuid::Uuid;

use super::{latest_summary, messages_before, span, tail_start, Applied};
use crate::storage::chat_messages::{ChatMessage, MessageRole};

fn row(role: MessageRole, content: &str, at: i64) -> ChatMessage {
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
        created_at: at,
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

fn user(text: &str, at: i64) -> ChatMessage {
    row(MessageRole::User, text, at)
}

fn assistant(text: &str, at: i64) -> ChatMessage {
    row(MessageRole::Assistant, text, at)
}

fn summary_of(covered: &ChatMessage, text: &str, at: i64) -> ChatMessage {
    ChatMessage {
        parent_id: Some(covered.id),
        ..row(MessageRole::Summary, text, at)
    }
}

/// Three turns: user, assistant each.
fn three_turns() -> Vec<ChatMessage> {
    vec![
        user("first task", 1),
        assistant("first answer", 2),
        user("second task", 3),
        assistant("second answer", 4),
        user("third task", 5),
    ]
}

#[test]
fn a_thread_without_a_summary_has_none() {
    assert_eq!(latest_summary(&three_turns()), None);
}

#[test]
fn a_summary_covers_up_to_its_parent() {
    let mut rows = three_turns();
    let summary = summary_of(&rows[1], "summary", 2);
    rows.insert(2, summary);
    assert_eq!(
        latest_summary(&rows),
        Some(Applied {
            summary: 2,
            covers: 1
        })
    );
}

#[test]
fn a_summary_whose_covered_row_is_gone_is_ignored() {
    let mut rows = three_turns();
    let gone = assistant("gone", 2);
    rows.push(summary_of(&gone, "orphan", 9));
    assert_eq!(latest_summary(&rows), None);
}

#[test]
fn of_two_summaries_the_one_covering_more_wins_and_a_tie_goes_to_the_newer() {
    let mut rows = three_turns();
    let short = summary_of(&rows[1], "short", 2);
    let long = summary_of(&rows[3], "long", 4);
    let same_end_older = summary_of(&rows[3], "older twin", 4);
    let same_end_newer = ChatMessage {
        created_at: 5,
        ..summary_of(&rows[3], "newer twin", 5)
    };
    rows.extend([short, long.clone()]);
    assert_eq!(latest_summary(&rows).unwrap().covers, 3);
    assert_eq!(rows[latest_summary(&rows).unwrap().summary].id, long.id);
    rows.extend([same_end_older, same_end_newer.clone()]);
    assert_eq!(
        rows[latest_summary(&rows).unwrap().summary].id,
        same_end_newer.id
    );
}

#[test]
fn only_message_rows_count_for_the_messages_of_a_request() {
    let mut rows = three_turns();
    let summary = summary_of(&rows[1], "summary", 2);
    rows.insert(2, summary);
    rows.insert(0, row(MessageRole::System, "note", 0));
    assert_eq!(messages_before(&rows, rows.len()), 5);
    assert_eq!(messages_before(&rows, 4), 2);
}

#[test]
fn the_tail_keeps_the_latest_turn_and_as_many_earlier_ones_as_fit_a_quarter_of_the_context() {
    let rows = three_turns();
    // A context of 400 tokens: a quarter is 100 tokens, 300 characters. Everything is short: all turns fit.
    assert_eq!(tail_start(&rows, 400, None), Some(0));
    // A context of 40 tokens: 10 tokens, 30 characters. Only the latest turn fits (and it stays anyway).
    assert_eq!(tail_start(&rows, 40, None), Some(4));
    // 48 tokens: 36 characters. "second task" + "second answer" + "third task" are 34, the first turn adds 22.
    assert_eq!(tail_start(&rows, 48, None), Some(2));
    assert_eq!(tail_start(&rows, 60, None), Some(2));
    assert_eq!(tail_start(&rows, 76, None), Some(0));
}

#[test]
fn a_big_tool_result_in_the_tail_counts_only_as_its_note() {
    let mut rows = three_turns();
    rows.insert(4, row(MessageRole::ToolResult, &"x".repeat(50_000), 4));
    assert_eq!(tail_start(&rows, 400, None), Some(0));
}

#[test]
fn without_a_user_row_there_is_no_tail() {
    let rows = vec![assistant("hello", 1)];
    assert_eq!(tail_start(&rows, 400, None), None);
}

#[test]
fn the_span_is_what_lies_between_the_earlier_summary_and_the_tail() {
    let mut rows = three_turns();
    let summary = summary_of(&rows[1], "summary", 2);
    rows.insert(2, summary);
    // rows: 0 user, 1 assistant, 2 summary, 3 user, 4 assistant, 5 user. Covered through 1, tail from 5.
    let applied = latest_summary(&rows).unwrap();
    assert_eq!(span(&rows, Some(applied.covers), 5), vec![3, 4]);
    assert_eq!(span(&rows, None, 3), vec![0, 1]);
    assert!(span(&rows, Some(applied.covers), 3).is_empty());
}
