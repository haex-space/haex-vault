//! Keeps a request within the context of a model that has a small one. A local model has the
//! context its load was planned for (8192 tokens by default); a browser page, a file listing or
//! the results of a few rounds before it fill that quickly, and the engine then refuses the whole
//! prompt ("the prompt has 10522 tokens and does not fit the context of 8192 tokens"), which ends
//! the conversation, since the same history is sent again with the next message.
//!
//! Before a request goes out, the results of tools are shrunk until it fits, oldest first: an
//! earlier result is left out altogether, and when the latest round alone does not fit, its
//! largest result is cut at the end. The rows of the conversation are not touched, only what the
//! model is sent. What is not enough to fit (the system prompt, the tools and the messages of the
//! person) is left as it is: the engine says so.

use serde_json::Value;

use crate::adapters::types::{ChatRequest, ChatRole};

/// How many characters one token is taken to be: a prudent figure for JSON and German text.
const CHARS_PER_TOKEN: usize = 3;
/// What the answer may use of the context, at most a quarter of it.
const ANSWER_TOKENS: usize = 1024;
/// A result in the latest round is cut to this much at the least.
const MIN_RESULT_CHARS: usize = 1500;

/// The text of a page result is shrunk to this much at the least, and this many of its elements stay.
const MIN_PAGE_TEXT_CHARS: usize = 500;
const MIN_PAGE_ELEMENTS: usize = 10;
/// What one element of a page result takes in characters, as a figure for how many to drop at once.
const ELEMENT_CHARS: usize = 90;

const LEFT_OUT: &str = "[An earlier tool result was left out to fit the context.]";
const CUT_NOTE: &str = "\n[…the rest of this tool result did not fit the context and was cut. \
Ask for less, or read on in smaller parts.]";
const PAGE_CUT_NOTE: &str = "The text and the list of elements were shortened to fit the context; \
read on with next_from where it is given, or open less at once.";

/// The size of `request` in characters: system prompt, messages, tool calls and the tools offered.
pub fn size_in_chars(request: &ChatRequest) -> usize {
    let tools: usize = request
        .tools
        .iter()
        .map(|tool| {
            tool.name.chars().count()
                + tool.description.chars().count()
                + tool.input_schema.to_string().chars().count()
                + 24
        })
        .sum();
    let system = request
        .system_prompt
        .as_deref()
        .map_or(0, |prompt| prompt.chars().count());
    let messages: usize = request
        .messages
        .iter()
        .map(|message| {
            message.content.chars().count()
                + match &message.role {
                    ChatRole::ToolResult { content, .. } => content.chars().count(),
                    ChatRole::ToolCall { name, input, .. } => {
                        name.chars().count() + input.to_string().chars().count()
                    }
                    _ => 0,
                }
                + 16
        })
        .sum();
    tools + system + messages
}

/// The size of `request` in tokens, as far as it can be told without the model's tokenizer.
pub fn estimate_tokens(request: &ChatRequest) -> usize {
    size_in_chars(request).div_ceil(CHARS_PER_TOKEN)
}

/// How many characters of the context the request may fill: all of it but the room for the answer.
fn budget_chars(limit_tokens: usize) -> usize {
    let answer = ANSWER_TOKENS.min(limit_tokens / 4);
    limit_tokens.saturating_sub(answer) * CHARS_PER_TOKEN
}

/// A page result of the browser tools (`untrusted_page_content` with `text` and `elements`) shrunk to
/// about `keep` characters by its structure: the text is shortened first, then elements are dropped from
/// the end, and the result stays valid JSON with the page's site, url, title and the numbers of the
/// elements that remain, so the model can still use the page. `None` when `content` is no page result;
/// a page that cannot go below its floors comes back as it is.
fn shrink_page(content: &str, keep: usize) -> Option<String> {
    let mut value: Value = serde_json::from_str(content).ok()?;
    value.get("untrusted_page_content")?.as_object()?;
    let mut shrunk = false;
    loop {
        let size = value.to_string().chars().count();
        if size <= keep {
            break;
        }
        let excess = size - keep;
        let page = value.get_mut("untrusted_page_content")?.as_object_mut()?;
        let text = page.get("text").and_then(Value::as_str).unwrap_or_default();
        let text_chars = text.chars().count();
        if text_chars > MIN_PAGE_TEXT_CHARS {
            let removed = excess.min(text_chars - MIN_PAGE_TEXT_CHARS);
            let kept: String = text.chars().take(text_chars - removed).collect();
            page.insert("text".to_owned(), Value::String(kept));
            // The offset to read on from moves back with the text that was taken away.
            if let Some(next) = page.get("next_from").and_then(Value::as_u64) {
                page.insert(
                    "next_from".to_owned(),
                    Value::from(next.saturating_sub(removed as u64)),
                );
            }
            shrunk = true;
            continue;
        }
        let elements = page.get_mut("elements").and_then(Value::as_array_mut);
        match elements {
            Some(elements) if elements.len() > MIN_PAGE_ELEMENTS => {
                let drop = (excess / ELEMENT_CHARS)
                    .max(1)
                    .min(elements.len() - MIN_PAGE_ELEMENTS);
                elements.truncate(elements.len() - drop);
                shrunk = true;
            }
            _ => break,
        }
    }
    if !shrunk {
        // Already as small as a page result goes; it is never cut through the middle of its JSON.
        return Some(content.to_owned());
    }
    let page = value.get_mut("untrusted_page_content")?.as_object_mut()?;
    page.insert("truncated".to_owned(), Value::Bool(true));
    page.insert("cut".to_owned(), Value::String(PAGE_CUT_NOTE.to_owned()));
    Some(value.to_string())
}

fn result_len(role: &ChatRole) -> usize {
    match role {
        ChatRole::ToolResult { content, .. } => content.chars().count(),
        _ => 0,
    }
}

/// Where the results of the round the model has not seen yet begin: they are the last messages.
fn latest_from(request: &ChatRequest) -> usize {
    request
        .messages
        .iter()
        .rposition(|message| !matches!(message.role, ChatRole::ToolResult { .. }))
        .map_or(0, |at| at + 1)
}

/// Shrinks the tool results of `request` until it fits `limit_tokens` or nothing more can go.
/// Returns whether anything was changed. The two stages are [`leave_out_old_results`] and [`cut_latest`];
/// compaction (spec 086) runs between them.
pub fn fit(request: &mut ChatRequest, limit_tokens: usize) -> bool {
    let left_out = leave_out_old_results(request, limit_tokens);
    let cut = cut_latest(request, limit_tokens);
    left_out || cut
}

/// The first stage: the results of earlier rounds are left out whole, oldest first, until the request
/// fits `limit_tokens`. Returns whether anything was changed.
pub fn leave_out_old_results(request: &mut ChatRequest, limit_tokens: usize) -> bool {
    let budget = budget_chars(limit_tokens);
    let mut size = size_in_chars(request);
    let latest_from = latest_from(request);
    let mut changed = false;
    for message in &mut request.messages[..latest_from] {
        if size <= budget {
            break;
        }
        if let ChatRole::ToolResult { content, .. } = &mut message.role {
            let before = content.chars().count();
            if content != LEFT_OUT && before > LEFT_OUT.chars().count() {
                *content = LEFT_OUT.to_owned();
                size = size.saturating_sub(before - LEFT_OUT.chars().count());
                changed = true;
            }
        }
    }
    changed
}

/// The last stage: when the latest round alone does not fit `limit_tokens`, its largest result is cut at
/// the end (a page by its structure). Returns whether anything was changed.
pub fn cut_latest(request: &mut ChatRequest, limit_tokens: usize) -> bool {
    let budget = budget_chars(limit_tokens);
    let mut size = size_in_chars(request);
    let latest_from = latest_from(request);
    let mut changed = false;
    // The latest round: the largest result first, cut at the end.
    while size > budget {
        let Some(at) = (latest_from..request.messages.len())
            .filter(|at| {
                result_len(&request.messages[*at].role) > MIN_RESULT_CHARS + CUT_NOTE.len()
            })
            .max_by_key(|at| result_len(&request.messages[*at].role))
        else {
            break;
        };
        let ChatRole::ToolResult { content, .. } = &mut request.messages[at].role else {
            break;
        };
        let before = content.chars().count();
        // A page result is shrunk by its structure, so it stays valid JSON with usable elements; any other
        // result is cut at the end, with a note.
        let target = before.saturating_sub(size - budget);
        let cut = match shrink_page(content, target.max(MIN_RESULT_CHARS)) {
            Some(shrunk) => shrunk,
            None => {
                // The note that tells the model about the cut takes room too.
                let keep = before
                    .saturating_sub(size - budget + CUT_NOTE.chars().count())
                    .max(MIN_RESULT_CHARS);
                let mut cut: String = content.chars().take(keep).collect();
                cut.push_str(CUT_NOTE);
                cut
            }
        };
        let after = cut.chars().count();
        if after >= before {
            break;
        }
        *content = cut;
        size = size.saturating_sub(before - after);
        changed = true;
    }
    changed
}
