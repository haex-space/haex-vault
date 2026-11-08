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

use crate::adapters::types::{ChatRequest, ChatRole};

/// How many characters one token is taken to be: a prudent figure for JSON and German text.
const CHARS_PER_TOKEN: usize = 3;
/// What the answer may use of the context, at most a quarter of it.
const ANSWER_TOKENS: usize = 1024;
/// A result in the latest round is cut to this much at the least.
const MIN_RESULT_CHARS: usize = 1500;

const LEFT_OUT: &str = "[An earlier tool result was left out to fit the context.]";
const CUT_NOTE: &str = "\n[…the rest of this tool result did not fit the context and was cut. \
Ask for less, or read on in smaller parts.]";

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

fn result_len(role: &ChatRole) -> usize {
    match role {
        ChatRole::ToolResult { content, .. } => content.chars().count(),
        _ => 0,
    }
}

/// Shrinks the tool results of `request` until it fits `limit_tokens` or nothing more can go.
/// Returns whether anything was changed.
pub fn fit(request: &mut ChatRequest, limit_tokens: usize) -> bool {
    let budget = budget_chars(limit_tokens);
    let mut size = size_in_chars(request);
    if size <= budget {
        return false;
    }
    // The results of the round the model has not seen yet are the last messages.
    let latest_from = request
        .messages
        .iter()
        .rposition(|message| !matches!(message.role, ChatRole::ToolResult { .. }))
        .map_or(0, |at| at + 1);
    let mut changed = false;

    // Earlier rounds, oldest first: left out whole.
    for message in &mut request.messages[..latest_from] {
        if size <= budget {
            return changed;
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
        // The note that tells the model about the cut takes room too.
        let keep = before
            .saturating_sub(size - budget + CUT_NOTE.chars().count())
            .max(MIN_RESULT_CHARS);
        let mut cut: String = content.chars().take(keep).collect();
        cut.push_str(CUT_NOTE);
        let after = cut.chars().count();
        *content = cut;
        size = size.saturating_sub(before.saturating_sub(after));
        changed = true;
        if after >= before {
            break;
        }
    }
    changed
}
