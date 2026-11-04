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

## The prompt of a turn

- **System prompt**: the given prompt, the tool instruction and the reply language, as today, without the line
  with the date and time.
- **The current user message**: starts with that line (`chat::tools::prompt::now_line`), then a blank line, then
  the user's text. Earlier user messages of the history are unchanged.
- **Who**: every provider, from `chat/send.rs` and the everyday runner (research R8).

## Everyday report (spec 078, extended)

Per request:

| Field            | Type        | Meaning                                                               |
| ---------------- | ----------- | --------------------------------------------------------------------- |
| `first_chunk_ms` | number      | Unchanged: first step                                                 |
| `later_chunk_ms` | number list | New: time to the first chunk of every later step                      |
| `reused_tokens`  | number list | New: `PromptCache.reused_tokens` of every step, `null` when none came |

Per run: the median of `later_chunk_ms`, the share of prompt tokens reused, and whether the cache was on
(`HOLZI_LLAMACPP_PROMPT_CACHE`).
