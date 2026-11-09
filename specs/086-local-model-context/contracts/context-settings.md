# Contracts: Context for local models

## Tauri commands (Rust, `src-tauri/src/models/context_commands.rs`)

All three need an unlocked vault. They are not reachable by MCP or the agent tools: they exist only as Tauri commands behind the
settings actions below, which are guardrails.

| Command                | Arguments                            | Result                                        |
| ---------------------- | ------------------------------------ | --------------------------------------------- |
| `get_model_context`    | `modelId`                            | `ContextSetting`                              |
| `set_model_context`    | `modelId`, `setting: ContextSetting` | `ContextSetting` as kept; error with a reason |
| `model_context_report` | `modelId`, `setting: ContextSetting` | `ContextReport`                               |

```text
ContextSetting { context: "auto" | number, cache: "standard" | "q8" | "q4" }

ContextReport {
  trained: number | null,
  maxOnGpu: number | null,        // null: not even 2048 fit with every layer on the GPU
  maxAny: number | null,          // null: refused at the floor
  maxStretched: number | null,
  sized: boolean,
  effective: { context, gpuLayers, cache, yarn: number | null, reason } | null   // only for the loaded model
  verdict: "fits" | "fits-on-host" | "too-large" | "beyond-training" | "not-stretchable" | "below-floor" | "unsized",
  need_mib: number | null, free_mib: number | null                               // with "too-large"
}
```

`verdict` is for the setting in the request: how the fixed choice (or the Auto target) would be placed. The page turns it into a
sentence (spec FR-006/007/008/009); the sentences are in the i18n files, not in the command.

`set_model_context` refuses with `InvalidInput { reason }` for `below-floor`, `not-stretchable` and a context above
`4 × trained`; it keeps a choice that is `too-large` for the current free memory (memory changes) but the load then refuses it
(spec FR-007: "the model MUST NOT be loaded with it") and says why.

## Actions (`src/lib/actions/settingsActions.ts`, scope `guardrails`)

| Id                               | Input                  | Output           |
| -------------------------------- | ---------------------- | ---------------- |
| `settings.models.context.get`    | `{ modelId }`          | `ContextSetting` |
| `settings.models.context.set`    | `{ modelId, setting }` | `ContextSetting` |
| `settings.models.context.report` | `{ modelId, setting }` | `ContextReport`  |

Not callable by agents (FR-011); `pnpm check:agent-actions` fails if one is. The eval snapshot `chat/eval/tools.json` does not
change.

## Events

No new event for settings. `LoadedModelInfo` (payload of `load_model` and `active_model_info`) gets `placement:
{ context, gpuLayers, cache, yarn }` so the chat header and the settings page read the same numbers.

For compaction one event, `chat-compaction`, `{ threadId, state }` with state `summarising` (the turn is busy writing a summary),
`done` or `failed`. The chat shows the first as a line like the tool-use notices, and the last as one dismissible notice that
says the conversation was shortened another way and nothing was deleted. A turn that needs no summary emits nothing.

## Engine (`src-tauri/src/llm/llamacpp`)

```text
LoadOptions { context_window, gpu_layers, threads, drafter,
              + choice: ContextChoice, + cache: CacheType }

ContextChoice { Auto, Fixed(u32) }
CacheType     { Standard, Q8, Q4 }          // F16, Q8_0, Q4_0 in the binding
Placement     { gpu_layers, context, drafter, + cache: CacheType, + yarn: Option<f32> }

memory::plan(shape, free, wanted, choice, cache) -> Result<Placement, MemoryRefusal>
memory::limits(shape, free, cache)               -> Limits { trained, max_on_gpu, max_any, max_stretched }
```

`generate` and `speculative` read `loaded.placement()` for `type_k`, `type_v`, flash attention and, when `yarn` is set and the
generation's own context exceeds the trained length, rope scaling (R4).

## Preference key

`models.context.<model id>` in `preferences`, device scope (data-model.md). Read in
`chat::model_loading::resolve_local_model_metadata`, written only by `set_model_context`.

## Database

None. A summary is a row of `chat_messages` with the role `summary` whose `parent_id` is the last row it covers (data-model.md).
