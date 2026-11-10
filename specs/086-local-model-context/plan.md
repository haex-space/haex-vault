# Implementation Plan: Context for local models

**Branch**: `086-local-model-context` | **Date**: 2026-11-09 | **Spec**: [spec.md](./spec.md)

## Summary

The planner already aims every local model at 131072 tokens and counts the cache from the file (PRs #903, #904). This plan adds
what the spec still asks for, in three stages that are each their own PR from `main`:

1. **A person's choice reaches the engine.** A per-model, per-device preference (context length Auto or fixed, cache type
   standard, 8-bit or 4-bit) is read at the load. The planner counts the chosen cache, honours a fixed context without shrinking
   it, and allows up to four times the trained length through YaRN; a generation applies the cache type and the rope scaling to
   its context. A read command answers what the model was trained for, what the device allows and what is in effect.
2. **The settings page.** Three guardrail actions and one block in the local-model settings.
3. **Compaction.** When the request would not leave the headroom after the cheap steps, the rows before the latest user
   message are summarised by the model itself, the summary is stored as a row of its own (new role, one new column), shown in the
   history, and used in a request only where needed.

Nothing is decided by guess that a measurement can decide: a probe the operator runs on the real card (R9) comes first and its
table decides what the page recommends, not what Auto does.

## Technical Context

- **Language/Stack**: Rust (Tauri 2) with `llama-cpp-2` 0.1.160; Vue/Nuxt for one settings block and one history row.
- **Storage**: `preferences` (device scope) for the setting; no migration: a summary is a `chat_messages` row of a new role (R6, R12).
- **Testing**: `cargo test` unit tests in `*_tests.rs` next to the modules (planner fixtures from real headers, a fake adapter
  with a small context for compaction), an `--ignored` test for a real file, `vue-tsc`/lint, `pnpm check:agent-actions`; the probe
  example and the manual quickstart for what only the real card can say.
- **Platforms**: Linux, Windows, macOS, Android. The planner and the compaction are plain Rust that the tests run on Linux; the
  engine calls (cache type, rope scaling) are the same on every backend. Android keeps the same rules and shrinks to what fits.
- **Performance goals**: none new for Auto. A load adds one 512-token context (milliseconds); compaction adds one to four model
  calls per turn and only when the cheap steps do not suffice.
- **Constraints**: no stacked PRs, files ≤ 500 lines, tests in separate files, one implementation per rule, no apply buttons,
  errors as toasts, texts in de and en.

## Constitution Check

- _One implementation per rule_: the planner is the only place that turns a setting into a placement; the report command uses the
  same `plan` and `limits`, not a copy (R5, R7). The fitting stages stay in `fit.rs`; compaction calls them, it does not
  reimplement them (R10).
- _Settings are guardrails_: the three actions are `guardrails`, so no agent or MCP can set a 4-bit cache (R14, FR-011).
- _Every OS_: no `cfg` gate in the planner or the compaction; the one target-specific part (the drafter) is unchanged.
- _No concealment_: a failed summary tells the person (FR-017); the covered messages stay readable.
- _No secrets, no local paths_: nothing here touches either.
- _Spec files in English_: yes.

Re-checked after the design: no violation.

## Stages and pull requests

| PR  | Stage | What                                                                                                                                                                                       | Behaviour change                 |
| --- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------- |
| A   | 1     | `CacheType` in the planner, `ContextChoice`, `limits`, YaRN and cache type in the generation, load-time check, the preference store, the three Tauri commands, `LoadedModelInfo.placement` | none until a setting exists      |
| B   | 2     | Actions, the settings block with reload and the sentences, i18n de/en                                                                                                                      | a setting can be chosen          |
| C   | 3     | Summary role and row, split of `fit`, selection, summariser, turn integration, history row, notice                                                                                         | long conversations are compacted |
| D   | 4     | Quickstart M1–M5 by hand, measurement table in R9, memory note, spec Done                                                                                                                  | none                             |

The probe (T001) goes into A, so the operator can run it while B and C are built; its table lands with D at the latest, and
earlier if it changes what B says. Each PR keeps `main` working: after A nothing differs for anyone who has chosen nothing;
after B the setting exists; C is independent of B (it needs only the context limit of #903) and may be built first if B waits for
the measurement.

## Project Structure

```text
src-tauri/src/llm/llamacpp/
  memory.rs            CacheType, ContextChoice, Kv::bytes(context, cache), plan(.., choice, cache), limits(), Placement.cache/yarn
  memory_tests.rs      real-header fixtures (Gemma 4 12B, Qwen3.5 4B, Qwen3 4B): cache types, fixed, stretched, refusals
  context_params.rs    one place that builds LlamaContextParams from a Placement and the generation's n_ctx (cache, flash, YaRN)
  context_params_tests.rs
  model.rs             LoadOptions.choice/cache; placement_for passes them; the 512-token check at load
  generate.rs, speculative.rs   use context_params
src-tauri/src/llm/gguf_header.rs   rope.scaling type and original length
src-tauri/src/llm/engine.rs        LoadRequest.choice/cache
src-tauri/src/models/
  context_setting.rs   the preference: read, validate, write, delete with the model
  context_commands.rs  get_model_context, set_model_context, model_context_report
src-tauri/src/chat/model_loading.rs  reads the setting; LoadedModelInfo.placement
src-tauri/examples/context_probe.rs  the measurement (R9)
src-tauri/src/chat/compact/
  mod.rs               prepare(): the stages in order, the turn's Compaction state
  select.rs            the span to cover, headroom, the latest covering summary, the effective history
  summarize.rs         instruction, clipping, chunks, the adapter call, size cap
  *_tests.rs
src-tauri/src/chat/turn/fit.rs       split into leave_out_old_results and cut_latest (fit stays as both)
src-tauri/src/chat/send.rs           calls compact::prepare before the first model call; summary rows are not messages
src-tauri/src/storage/chat_messages.rs   MessageRole::Summary and its validator rule
src-tauri/src/chat/events.rs         MessageErrorKind::SummaryFailed
src/lib/actions/settingsActions.ts, src/stores/settingsActionHandlers.ts   the three guardrail actions
src/components/settings/ModelContextSetting.vue, InstalledModels.vue
src/components/chat/MessageList.vue, src/composables/useChat.ts           the summary row
src/i18n/locales/{de,en}.json            models.context.*, chat.summary.*
```

## Design notes

- **One planner, two questions.** `plan(shape, free, wanted, choice, cache)` answers "where does this load go"; `limits(shape, free,
cache)` answers "what would fit" for the page. Both read the same `Kv` and the same margins. The report command builds
  `ModelShape` the way the load does (header + file size), so the page and the load cannot disagree about what is free except by
  time.
- **Fixed never shrinks** (R5). The planner's halving belongs to Auto. A fixed context that cannot be placed is a
  `MemoryRefusal` with the same text the load error already carries, so the settings sentence and the load error say the same.
- **Cache and YaRN live in `context_params.rs`** so the plain generation and the speculative one cannot diverge (they are the two
  places that make a context, R1). A unit test builds the params for a placement and reads them back through the binding's
  getters.
- **The check at load** (R2) uses the same builder with `n_ctx = 512`, then drops the context. It runs after the weights are in,
  so a refusal frees them again; a failing combination never leaves a half-loaded model active.
- **The summary is evidence, not a replacement.** The rows stay; the request builder is the only thing that swaps (R10). A device
  with a larger context never reads the summary (FR-019) because the swap happens only when the headroom is missing.
- **The headroom** (a quarter of the context) is one constant shared with `context_budget` so a tool result and the room kept for
  it are the same number.
- **No mid-turn compaction.** The prefix before the latest user message cannot change during a turn, so one decision at the start
  of the turn is enough; growth inside the turn is #903's job (R10).
- **Reload on change** (R8) reuses `settings.models.activate`; no second load path.

## Risks

| Risk                                                                           | Answer                                                                                                  |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------- |
| Flash attention or a quantised cache is not available for a model's head sizes | The 512-token check at load turns it into a clear refusal (R2); Auto stays standard.                    |
| YaRN makes a model worse at short texts                                        | Applied only to a generation longer than the trained length (R4); the page says it stretches the model. |
| A summary loses what the agent needs                                           | Tool results are clipped, not dropped, in its input; originals stay; SC-003 is measured in M4.          |
| A summary call is slow on a small device                                       | Only when the cheap steps fail; at most four calls; the turn shows `summarising`.                       |
| Memory changes between the report and the load                                 | The page labels "in effect" from the load; the load decides, the report advises (spec Edge Cases).      |
