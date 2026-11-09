//! Writing the summary of older messages (spec 086 FR-014, research R11): one call of the conversation's
//! own model, no tools, over the rows as plain text with each tool result clipped. A longer span goes in
//! chunks, the summary of one the "earlier summary" of the next.

use tokio_util::sync::CancellationToken;

use crate::adapters::types::{
    ChatMessage as LlmMessage, ChatRequest, ChatRole, Sampling, StreamChunk,
};
use crate::adapters::ProviderAdapter;
use crate::chat::reply_language::ReplyLanguage;
use crate::storage::chat_messages::{ChatMessage, MessageRole};

/// How long a summary may be, in tokens: it is asked for in this size and cut at a paragraph end above it.
pub const SUMMARY_TOKENS: usize = 1200;
/// Characters one token is taken to be, as in `chat::turn::fit`.
const CHARS_PER_TOKEN: usize = 3;
/// How many calls one summary may take.
const MAX_CALLS: usize = 4;
/// A tool result goes into the input up to this long.
const RESULT_CLIP: usize = 1500;
/// A tool call's arguments go into the input up to this long.
const INPUT_CLIP: usize = 500;
/// The least room for messages a call needs to be worth making, in characters.
const MIN_CHUNK_CHARS: usize = 2000;
/// What the instruction takes, in characters, with a margin for the template's markers.
const INSTRUCTION_ROOM: usize = 1200;

/// Why a summary was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryError {
    /// The turn was cancelled while the model wrote.
    Cancelled,
    /// The model gave nothing, failed, or the context is too small to summarise in.
    Failed(String),
}

/// A summary and how many of the rows it covers (all of them, unless the calls ran out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub text: String,
    pub covered: usize,
}

/// What the model is asked to do.
pub fn instruction(language: Option<ReplyLanguage>) -> String {
    let written_in = match language {
        Some(language) => format!("in {}", language.english_name()),
        None => "in the language the person writes in".to_owned(),
    };
    format!("You summarise the earlier part of a conversation between a person and an AI assistant that works with \
tools, so the assistant can go on without the original messages. Keep what the person wants and asked for, \
their instructions and preferences, decisions made, names, numbers, addresses, URLs and file names that a \
later step needs, what was done and what the tools returned that still matters, what failed, and what is \
still open. Leave out greetings and what no longer matters. Write plain text in short paragraphs or lines, \
{written_in}, in at most about 600 words. Do not answer the person and do not say that you are summarising."
    )
}

fn clipped(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(limit).collect();
    cut.push_str(" […]");
    cut
}

/// One row as a line of the transcript; `None` for a row that says nothing.
pub fn line(row: &ChatMessage) -> Option<String> {
    let text = row.content.trim();
    match row.role {
        MessageRole::User => Some(format!("Person: {text}")),
        MessageRole::Assistant if text.is_empty() => None,
        MessageRole::Assistant => Some(format!("Assistant: {text}")),
        MessageRole::ToolCall => Some(format!(
            "Assistant used the tool {} with {}",
            row.tool_name.as_deref().unwrap_or("?"),
            clipped(row.tool_input.as_deref().unwrap_or("{}"), INPUT_CLIP)
        )),
        MessageRole::ToolResult => Some(format!(
            "Tool result{}: {}",
            if row.tool_is_error == Some(true) {
                " (an error)"
            } else {
                ""
            },
            clipped(text, RESULT_CLIP)
        )),
        MessageRole::System | MessageRole::Summary => None,
    }
}

/// Where the chunks of `lines` end: each is at most `budget` characters, a line longer than that is clipped
/// by the caller first. Returns the exclusive end index of every chunk.
pub fn chunk_ends(lines: &[String], budget: usize) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut size = 0;
    for (index, line) in lines.iter().enumerate() {
        let len = line.chars().count() + 1;
        if size > 0 && size + len > budget {
            ends.push(index);
            size = 0;
        }
        size += len;
    }
    if !lines.is_empty() {
        ends.push(lines.len());
    }
    ends
}

/// `text` cut at the end of a paragraph or a line within the size of a summary; as it is when it fits.
pub fn clip_summary(text: &str) -> String {
    let text = text.trim();
    let limit = SUMMARY_TOKENS * CHARS_PER_TOKEN;
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let head: String = text.chars().take(limit).collect();
    let end = head
        .rfind("\n\n")
        .or_else(|| head.rfind('\n'))
        .or_else(|| head.rfind(". ").map(|at| at + 1))
        .unwrap_or(head.len());
    head[..end].trim_end().to_owned()
}

async fn ask(
    adapter: &dyn ProviderAdapter,
    model_id: &str,
    prior: Option<&str>,
    chunk: &str,
    language: Option<ReplyLanguage>,
    cancel: &CancellationToken,
) -> Result<String, SummaryError> {
    let body = match prior {
        Some(prior) => format!("Summary so far:\n{prior}\n\nMessages since then:\n{chunk}"),
        None => format!("Messages:\n{chunk}"),
    };
    let request = ChatRequest {
        model_id: model_id.to_owned(),
        thread_id: None,
        system_prompt: Some(instruction(language)),
        messages: vec![LlmMessage {
            role: ChatRole::User,
            content: format!("{body}\n\nWrite the summary now."),
            attachments: Vec::new(),
        }],
        reasoning_requested: false,
        max_new_tokens: Some(SUMMARY_TOKENS),
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
        reasoning_option: None,
        capabilities: None,
        sampling: Sampling::Default,
    };
    let collect = async {
        let mut stream = adapter
            .stream_chat(request)
            .await
            .map_err(|e| SummaryError::Failed(e.to_string()))?;
        let mut text = String::new();
        while let Some(item) = stream.next().await {
            match item.map_err(|e| SummaryError::Failed(e.to_string()))? {
                StreamChunk::Delta { content, .. } => text.push_str(&content),
                StreamChunk::Done { .. } => break,
                _ => {}
            }
        }
        Ok(text)
    };
    let text = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(SummaryError::Cancelled),
        text = collect => text?,
    };
    let text = clip_summary(&text);
    if text.is_empty() {
        return Err(SummaryError::Failed(
            "the model wrote no summary".to_owned(),
        ));
    }
    Ok(text)
}

/// Summarises `rows` (in order, messages only) after `prior`, for a model whose context is `limit_tokens`.
/// A span too long for one call goes in chunks, at most [`MAX_CALLS`]; the result covers what the calls
/// covered. A failure after the first call keeps what was written so far.
pub async fn summarize(
    adapter: &dyn ProviderAdapter,
    model_id: &str,
    prior: Option<&str>,
    rows: &[&ChatMessage],
    limit_tokens: usize,
    language: Option<ReplyLanguage>,
    cancel: &CancellationToken,
) -> Result<Written, SummaryError> {
    let answer = SUMMARY_TOKENS * CHARS_PER_TOKEN;
    let prior_room = if prior.is_some() { answer } else { 0 };
    let budget =
        (limit_tokens * CHARS_PER_TOKEN).saturating_sub(INSTRUCTION_ROOM + answer + prior_room) * 9
            / 10;
    if budget < MIN_CHUNK_CHARS {
        return Err(SummaryError::Failed(
            "the context is too small to summarise in".to_owned(),
        ));
    }
    // Rows that say nothing are skipped; `owners[i]` is the index in `rows` that line `i` came from.
    let (lines, owners): (Vec<String>, Vec<usize>) = rows
        .iter()
        .enumerate()
        .filter_map(|(at, row)| line(row).map(|line| (clipped(&line, budget), at)))
        .unzip();
    let mut written: Option<Written> = None;
    let mut start = 0;
    for end in chunk_ends(&lines, budget).into_iter().take(MAX_CALLS) {
        let chunk = lines[start..end].join("\n");
        let before = written.as_ref().map(|w| w.text.as_str()).or(prior);
        match ask(adapter, model_id, before, &chunk, language, cancel).await {
            Ok(text) => {
                // Everything up to the last line of this chunk, and the silent rows after it.
                let covered = owners.get(end).copied().unwrap_or(rows.len());
                written = Some(Written { text, covered });
                start = end;
            }
            Err(error) if written.is_none() => return Err(error),
            Err(SummaryError::Cancelled) => return Err(SummaryError::Cancelled),
            Err(_) => break,
        }
    }
    written.ok_or_else(|| SummaryError::Failed("there was nothing to summarise".to_owned()))
}

#[cfg(test)]
#[path = "summarize_tests.rs"]
mod tests;
