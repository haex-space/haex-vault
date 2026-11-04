# Data model: Reuse the prompt a local model has already read

No database or file changes. Everything here lives in memory with a loaded model and ends with it.

## KeptContext (new, `llm/llamacpp/kept.rs`)

The spec's "kept reading".

One per `LoadedModel`, behind a mutex, created at load (research R2).

| Field         | Type                 | Rule                                                                                            |
| ------------- | -------------------- | ----------------------------------------------------------------------------------------------- |
| `context`     | llama.cpp context    | Context length `Placement.context`, batch 512, sliding-window cache kept whole (research R5)    |
| `tokens`      | token list           | Exactly the tokens in the cache, in order: prompt tokens read and generated tokens decoded      |
| `checkpoints` | list of `Checkpoint` | Only for hybrid or recurrent models; at most 4, ordered by `n_tokens` (research R6)             |
| `head`        | `Checkpoint` or none | Only for hybrid or recurrent models; the end of the head, outside the four (research R6)        |
| `enabled`     | bool                 | `LoadOptions.prompt_cache`; when false, every answer clears the context first (today's reading) |

A request with `reuse_prompt == false` (a model job) clears the context first as well (research R7). Everything
here is process memory and is never written to a file (spec FR-012).

## Checkpoint (new)

| Field      | Type                | Rule                                                                          |
| ---------- | ------------------- | ----------------------------------------------------------------------------- |
| `n_tokens` | count               | How many of `tokens` the saved state covers                                   |
| `state`    | saved partial state | `state_seq_get(0, PARTIAL_ONLY)` at that point; restored with `state_seq_set` |

## Reuse (derived, per answer)

| Value    | Rule                                                                                                                   |
| -------- | ---------------------------------------------------------------------------------------------------------------------- |
| `common` | Longest common beginning of `tokens` and the new prompt's tokens, at most the prompt's length minus one                |
| `reused` | Attention model: `common`. Hybrid model: `n_tokens` of the latest checkpoint or head with `n_tokens <= common`, else 0 |

## State changes of the kept context

```text
answer starts ─► compare ─► rewind to `reused` ─► read the rest ─► write ─► end: tokens = all decoded
                     │                                   │              │
                     │                                   └─ error ──────┴─► clear (tokens, cache, checkpoints)
                     └─ rewind refused ─► clear ─► read all
cancel ─► tokens = what was decoded until the cancel (kept)
model dropped ─► kept context dropped with it
```

## Outcome (extended, `llm/llamacpp/generate.rs`)

| Field           | Change                                      |
| --------------- | ------------------------------------------- |
| `reused_tokens` | New: the prompt tokens taken from the cache |

## Placement and memory (unchanged fields)

The planner already counts the full context length for every layer (spec 070), which covers the kept context with
its whole sliding-window cache. For a hybrid model, the host budget additionally counts five checkpoints (four and
the head) of the size the spike measures (research R6).
