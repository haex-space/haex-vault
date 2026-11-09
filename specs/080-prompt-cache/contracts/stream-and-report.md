# Contract: stream field, prompt shape and everyday report

Changes visible beyond the llama.cpp engine. Everything not named here stays as it is.

## `StreamChunk::PromptCache` (new variant)

```rust
/// How many prompt tokens were taken from the kept reading instead of being read again. Sent once per
/// answer, before `Done`, by the llama.cpp adapter only.
PromptCache { reused_tokens: usize },
```

- Only the llama.cpp adapter sends it (0 when nothing was reused). No other adapter changes.
- Consumers that match `StreamChunk` exhaustively ignore it (`chat/turn/step.rs`, `model_jobs/runner.rs`, and any
  other the compiler names); the everyday runner records it.
- `StreamChunk` is not serialized to the frontend, so no binding changes.

## `ChatRequest.reuse_prompt` (new field)

```rust
/// Whether the llama.cpp engine may reuse what an earlier request left in the kept reading. `false` reads the
/// prompt from the start (spec 080 FR-013).
pub reuse_prompt: bool,
```

- `true` for the chat (`chat/send.rs`), the voice assistant (through the chat) and the evaluations.
- `false` for a model job (`model_jobs/runner.rs`).
- Only the llama.cpp adapter reads it; every other adapter ignores it.

## The prompt of a turn

- **System prompt**: the given prompt, the tool instruction and the reply language, as today, without the line
  with the date and time, and with one fixed sentence: every user message starts with the date and time it was
  sent, the last one is now.
- **Every user message** (history and current): starts with the stamp of its `created_at` in the turn's UTC offset
  (`chat::tools::prompt::sent_line`, the format of today's `now_line`), then a blank line, then the user's text.
  The stamp of a message never changes, so a turn's prompt only grows at its end.
- **Who**: every provider, from `chat/send.rs` and the everyday runner (research R8).

## Everyday report (spec 078, extended)

Per request:

| Field              | Type        | Meaning                                                                    |
| ------------------ | ----------- | -------------------------------------------------------------------------- |
| Field              | Type        | Meaning                                                                    |
| ------------------ | ----------- | -------------------------------------------------------------------------- |
| `first_chunk_ms`   | number      | Unchanged: first step                                                      |
| `later_chunk_ms`   | number list | New: time to the first chunk of every later step                           |
| `later_chunk_text` | bool list   | New, per later step: the first chunk was text (`true`) or a tool call      |
| `reused_tokens`    | number list | New: `PromptCache.reused_tokens` of every step, `null` when none came      |

A first text chunk is the first word of FR-010. A step that starts with a tool call sends its first chunk only when
the call is parsed, so its time includes writing the call.

Per run: the median of `later_chunk_ms` over the steps that start with text (SC-001, SC-002) and, separately, over
those that start with a tool call; the share of prompt tokens reused; and whether the cache was on
(`HOLZI_LLAMACPP_PROMPT_CACHE`).
