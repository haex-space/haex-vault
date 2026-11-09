//! Which rows a summary covers and which request it makes (spec 086, research R10 to R12). Pure: rows in,
//! indexes out, so the cases are tested without a database or a model.
//!
//! A summary covers the rows up to and including its `parent_id`. A request that uses it sends the rows
//! after that one and the summary text as a paragraph of the system prompt.

use crate::chat::tools::context_budget::SHARE;
use crate::storage::chat_messages::{ChatMessage, MessageRole};

/// How many characters one token is taken to be, as in `chat::turn::fit`.
const CHARS_PER_TOKEN: usize = 3;
/// A tool result counts this much in the verbatim tail of a conversation: the results of earlier turns are
/// left out of a request that is too large, so only a short note of them stays.
const TAIL_RESULT_CHARS: usize = 200;

/// The summary a request would use: its row and the index of the last row it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub summary: usize,
    pub covers: usize,
}

/// Whether a row becomes a message of a request (`history_to_messages`).
pub fn is_message(row: &ChatMessage) -> bool {
    !matches!(row.role, MessageRole::System | MessageRole::Summary)
}

/// The summary of the thread a request uses: of the rows with role `summary` whose covered row is in the
/// thread, the one whose covered row sorts last, then the newest, then the greater id (two devices that
/// summarised the same messages keep both rows, spec Edge Cases). `rows` are in the thread's order.
pub fn latest_summary(rows: &[ChatMessage]) -> Option<Applied> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.role == MessageRole::Summary)
        .filter_map(|(summary, row)| {
            let parent = row.parent_id?;
            let covers = rows.iter().position(|candidate| {
                candidate.id == parent && candidate.role != MessageRole::Summary
            })?;
            Some(Applied { summary, covers })
        })
        .max_by_key(|applied| {
            let row = &rows[applied.summary];
            (applied.covers, row.created_at, row.id)
        })
}

/// How many messages of a request the rows before `index` make.
pub fn messages_before(rows: &[ChatMessage], index: usize) -> usize {
    rows[..index.min(rows.len())]
        .iter()
        .filter(|row| is_message(row))
        .count()
}

/// What a row takes in the verbatim tail, in characters.
fn tail_chars(row: &ChatMessage) -> usize {
    match row.role {
        MessageRole::ToolResult => row.content.chars().count().min(TAIL_RESULT_CHARS),
        _ => {
            row.content.chars().count()
                + row
                    .tool_input
                    .as_ref()
                    .map_or(0, |input| input.chars().count())
        }
    }
}

/// Where the verbatim tail of the conversation begins: the index of a user row. Everything before it may
/// be summarised, everything from it stays as it is. The tail is the latest user message with its round and
/// as many earlier turns as fit a quarter of the context (`limit_tokens`), so the model keeps its recent
/// conversation word for word. `None` when no user row exists. Only rows after `after` (the last row an
/// earlier summary covers) are looked at.
pub fn tail_start(
    rows: &[ChatMessage],
    limit_tokens: usize,
    after: Option<usize>,
) -> Option<usize> {
    let first = after.map_or(0, |covered| covered + 1);
    let users: Vec<usize> = rows
        .iter()
        .enumerate()
        .skip(first)
        .filter(|(_, row)| row.role == MessageRole::User)
        .map(|(index, _)| index)
        .collect();
    let latest = *users.last()?;
    let budget = limit_tokens / SHARE * CHARS_PER_TOKEN;
    let mut start = latest;
    for &candidate in users.iter().rev().skip(1) {
        let tail: usize = rows[candidate..].iter().map(tail_chars).sum();
        if tail > budget {
            break;
        }
        start = candidate;
    }
    Some(start)
}

/// The rows a new summary covers: those after the last row an earlier summary covers, up to the start of
/// the tail, without summary and system rows. Empty when there is nothing older than the tail.
pub fn span(rows: &[ChatMessage], after: Option<usize>, tail_start: usize) -> Vec<usize> {
    let first = after.map_or(0, |covered| covered + 1);
    (first..tail_start.min(rows.len()))
        .filter(|index| is_message(&rows[*index]))
        .collect()
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
