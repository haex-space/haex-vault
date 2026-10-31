//! Turns the text a model generates, piece by piece, into the chunks of an `AdapterStream` (spec 070, plan
//! D4): thinking goes to `reasoning`, answer text goes out at once, and from the first character of a
//! tool-call marker on nothing is shown until the answer ends and the call block can be read whole.

use super::formats::{ChatFormat, Holdback};
use crate::adapters::types::{StreamChunk, ToolSpec};
use crate::llm::holdback::longest_prefix_overlap;

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

enum Phase {
    /// The first characters decide whether the model starts by thinking (`<think>` in its own output).
    Probe(String),
    /// Reasoning up to `</think>`. `pending` is the held-back tail; `started` is false until the first
    /// character that is not whitespace.
    Thinking { pending: String, started: bool },
    Answer {
        /// Present when the model's format has a call marker to hold text back at.
        hold: Option<Holdback>,
        /// Whitespace at the start of the answer is dropped, as after `</think>` the template's own
        /// newlines are not part of the answer.
        started: bool,
    },
}

/// What the end of the generation adds to the stream.
pub struct Finished {
    pub chunks: Vec<StreamChunk>,
    /// True when the answer ended in one or more tool calls.
    pub tool_calls: bool,
}

pub struct Streamer {
    format: Option<&'static dyn ChatFormat>,
    tools: Vec<ToolSpec>,
    /// Everything generated, for reading the call blocks at the end.
    raw: String,
    /// Everything shown as content so far.
    shown: String,
    phase: Phase,
}

impl Streamer {
    /// `thinking_open` is true when the prompt itself ends inside a `<think>` block (Qwen3.5 with thinking
    /// on), so the output starts with reasoning and no opening tag.
    pub fn new(
        format: Option<&'static dyn ChatFormat>,
        tools: Vec<ToolSpec>,
        thinking_open: bool,
    ) -> Self {
        let phase = if thinking_open {
            Phase::Thinking {
                pending: String::new(),
                started: false,
            }
        } else {
            Phase::Probe(String::new())
        };
        Self {
            format,
            tools,
            raw: String::new(),
            shown: String::new(),
            phase,
        }
    }

    /// True when `prompt` ends inside an open `<think>` block.
    pub fn prompt_opens_thinking(prompt: &str) -> bool {
        prompt.trim_end().ends_with(THINK_OPEN)
    }

    /// Takes the next piece of generated text; returns what can be shown of it.
    pub fn push(&mut self, piece: &str) -> Vec<StreamChunk> {
        self.raw.push_str(piece);
        let mut out = Vec::new();
        self.feed(piece, &mut out);
        out
    }

    /// The generation ended: shows what is still held back and reads the tool calls.
    pub fn finish(mut self) -> Finished {
        let mut chunks = Vec::new();
        let phase = std::mem::replace(
            &mut self.phase,
            Phase::Probe(String::new()), // never read again
        );
        match phase {
            Phase::Probe(buffer) => {
                // Too short to have told: it is an answer.
                let text = buffer.trim_start();
                self.show(text, &mut chunks);
                self.finish_answer(chunks, None)
            }
            Phase::Thinking { pending, started } => {
                let text = if started {
                    pending.as_str()
                } else {
                    pending.trim_start()
                };
                reasoning(text.trim_end(), &mut chunks);
                Finished {
                    chunks,
                    tool_calls: false,
                }
            }
            Phase::Answer { hold, .. } => self.finish_answer(chunks, hold),
        }
    }

    fn finish_answer(mut self, mut chunks: Vec<StreamChunk>, hold: Option<Holdback>) -> Finished {
        let Some(hold) = hold else {
            return Finished {
                chunks,
                tool_calls: false,
            };
        };
        let in_call = hold.in_call();
        let rest = hold.finish();
        if !in_call {
            // A marker-like tail that never became a marker is plain text.
            self.show(rest.trim_end(), &mut chunks);
            return Finished {
                chunks,
                tool_calls: false,
            };
        }
        let format = self.format.expect("a holdback exists only with a format");
        let parsed = format.parse(&self.raw, &self.tools);
        if parsed.calls.is_empty() {
            // The call block could not be read: it stays in the text, as the whole-text parse would leave it.
            self.show(rest.trim_end(), &mut chunks);
            return Finished {
                chunks,
                tool_calls: false,
            };
        }
        // Text after the first call block was held back with it; the parse has it without the blocks.
        let tail = parsed
            .content
            .strip_prefix(self.shown.trim())
            .unwrap_or_default()
            .trim_start()
            .to_owned();
        self.show(&tail, &mut chunks);
        chunks.push(StreamChunk::ToolCalls(parsed.calls));
        Finished {
            chunks,
            tool_calls: true,
        }
    }

    fn feed(&mut self, text: &str, out: &mut Vec<StreamChunk>) {
        match &mut self.phase {
            Phase::Probe(buffer) => {
                buffer.push_str(text);
                let seen = buffer.trim_start();
                if seen.is_empty()
                    || (THINK_OPEN.starts_with(seen) && seen.len() < THINK_OPEN.len())
                {
                    return;
                }
                let seen = seen.to_owned();
                match seen.strip_prefix(THINK_OPEN) {
                    Some(after) => {
                        self.phase = Phase::Thinking {
                            pending: String::new(),
                            started: false,
                        };
                        self.feed(after, out);
                    }
                    None => {
                        self.phase = Phase::Answer {
                            hold: self.format.map(|f| Holdback::new(f.marker())),
                            started: false,
                        };
                        self.feed(&seen, out);
                    }
                }
            }
            Phase::Thinking { pending, started } => {
                pending.push_str(text);
                if !*started {
                    let trimmed = pending.trim_start().to_owned();
                    *pending = trimmed;
                    *started = !pending.is_empty();
                }
                if let Some(end) = pending.find(THINK_CLOSE) {
                    let thought = pending[..end].trim_end().to_owned();
                    let after = pending[end + THINK_CLOSE.len()..].to_owned();
                    reasoning(&thought, out);
                    self.phase = Phase::Answer {
                        hold: self.format.map(|f| Holdback::new(f.marker())),
                        started: false,
                    };
                    self.feed(&after, out);
                    return;
                }
                // Keep back what may still become `</think>`, and trailing whitespace, which is dropped
                // when the block ends.
                let keep = longest_prefix_overlap(pending, THINK_CLOSE);
                let safe = pending[..pending.len() - keep].trim_end().len();
                let shown: String = pending.drain(..safe).collect();
                reasoning(&shown, out);
            }
            Phase::Answer { hold, started } => {
                let text = if *started { text } else { text.trim_start() };
                if text.is_empty() {
                    return;
                }
                *started = true;
                let shown = match hold {
                    Some(hold) => hold.push(text),
                    None => text.to_owned(),
                };
                self.show(&shown, out);
            }
        }
    }

    fn show(&mut self, text: &str, out: &mut Vec<StreamChunk>) {
        if text.is_empty() {
            return;
        }
        self.shown.push_str(text);
        out.push(StreamChunk::Delta {
            content: text.to_owned(),
            reasoning: None,
        });
    }
}

fn reasoning(text: &str, out: &mut Vec<StreamChunk>) {
    if !text.is_empty() {
        out.push(StreamChunk::Delta {
            content: String::new(),
            reasoning: Some(text.to_owned()),
        });
    }
}

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
