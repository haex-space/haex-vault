# Implementation Plan: Reuse the prompt a local model has already read

**Branch**: `080-prompt-cache` | **Date**: 2026-11-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/080-prompt-cache/spec.md`; the measurement of spec 079 research R11.

## Summary

The llama.cpp engine keeps one context per loaded model instead of one per answer.

- **Matching**: before an answer, the new prompt's tokens are compared with the tokens in the kept cache; the
  longest common beginning is reused and only the rest is read.
- **Rewinding**: attention caches (Gemma 4 with its sliding-window cache kept whole) are cut back to the reused
  point. Hybrid models (Qwen3.5 4B) restore a saved checkpoint of their recurrent state.
- **The prompt**: the date and time leave the system prompt; every user message carries the time it was sent, so
  a turn's prompt only grows at its end.
- **Safety**: a failure clears the kept context; nothing that does not match exactly is reused; a model job of an
  extension never reuses what another request left.
- **Privacy**: the kept context holds conversation and vault content in process memory only, never in a file, and
  ends with the model or the vault session (spec 013).

## Technical Context

**Language/Version**: Rust (edition and MSRV of `src-tauri/Cargo.toml`)

**Primary Dependencies**:

- `llama-cpp-2` =0.1.160: `clear_kv_cache_seq`, `with_swa_full`, `state_seq_get`/`state_seq_set` with
  `LlamaStateSeqFlags::PARTIAL_ONLY`, `LlamaModel::is_hybrid`/`is_recurrent`.
- `self_cell` 1.3: declared as a direct dependency; already in `Cargo.lock` through another crate (research R2).

**Storage**: none; the kept context lives in memory with the loaded model and is never written to a file.

**Testing**:

- `cargo test --lib`: the prefix match, the checkpoint choice, the stamp of the user messages, a model job that
  never reuses, the report fields.
- Ignored tests with the real files in a new `src-tauri/tests/prompt_cache_eval.rs` (spike, equality, speed).
- `measure_everyday_voice` with and without `HOLZI_LLAMACPP_PROMPT_CACHE=off` (SC-003, SC-004).

**Target Platform**: desktop (Linux, Windows, macOS) and Android, wherever the llama.cpp engine runs.

**Project Type**: desktop and mobile app (Tauri)

**Performance Goals**: SC-001 (second step of a chain 3× sooner), SC-002 (follow-up 2× sooner), SC-003 (everyday
set 30 % less time).

**Constraints**:

- No load or answer may fail for memory that succeeds today (FR-008).
- Builds in the 10 GB memory scope with `-j 1`; the CUDA build per spec 078's quickstart.

**Scale/Scope**: one new engine module, changes to the load, the token loop, the prompt assembly, one stream field
and the everyday report.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                                                 | Result |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | No secrets involved                                                                                                                   | Pass   |
| II No local absolute paths                              | Model paths come from the environment in the ignored tests; docs use placeholders                                                     | Pass   |
| III Device-independent identity                         | Not touched                                                                                                                           | Pass   |
| IV Pinned cross-repo references                         | llama.cpp is cited by commit; no other repository                                                                                     | Pass   |
| V External sources opt-in                               | No harness content                                                                                                                    | Pass   |
| VI Self-modifying instructions review-gated             | No agent instruction file changes (the repository has no `CLAUDE.md`)                                                                 | Pass   |
| VII Relay never blocks local work                       | Not touched                                                                                                                           | Pass   |
| VIII No concealment instructions                        | Not touched                                                                                                                           | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch, one PR                                                                                                              | Pass   |
| spaex: 500 LoC per file                                 | New logic in a new `llm/llamacpp/kept.rs`. `chat/send.rs` (538) only changes one call; its split is planned under Complexity Tracking | Pass   |
| spaex: graphify first                                   | `graphify query` before each new function; `graphify update` first in this worktree                                                   | Pass   |
| spaex: tests in separate files                          | `*_tests.rs` next to each module; real-model tests ignored in `tests/prompt_cache_eval.rs`                                            | Pass   |
| spaex: ponytail comments                                | The checkpoint spacing and count (research R6) are fixed numbers from one measurement; marked with `ponytail:`                        | Pass   |

Checked against `.specify/memory/constitution.md` and the spaex harness `.spaex/constitution.md`. No ADR: the
engine (ADR 0023) is unchanged; spec 070 plan D5 named prefix reuse as the later optimisation this is. Re-check
after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/080-prompt-cache/
├── plan.md
├── research.md          # R1–R12
├── data-model.md
├── quickstart.md
├── contracts/
│   └── stream-and-report.md
└── tasks.md             # $speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/Cargo.toml                      # self_cell
src-tauri/src/llm/llamacpp/
├── kept.rs                # new: KeptContext, prefix match, rewind, checkpoints
├── model.rs               # LoadedModel owns model + kept context (self_cell); LoadOptions.prompt_cache
├── memory.rs              # host budget counts the checkpoints of a hybrid model
├── generate.rs            # reads through the kept context; reused tokens in Outcome
├── adapter.rs             # sends StreamChunk::PromptCache
└── *_tests.rs
src-tauri/src/adapters/types.rs           # StreamChunk::PromptCache, ChatRequest.reuse_prompt
src-tauri/src/chat/turn/step.rs, model_jobs/runner.rs   # ignore the new chunk; a job sets reuse_prompt false
src-tauri/src/chat/tools/prompt.rs        # system_prompt without the clock; the stamp of every user message
src-tauri/src/chat/send.rs                # one call in history_to_messages
src-tauri/src/chat/eval/everyday/{runner,scoring,report}.rs   # per-step first chunk, reused tokens
src-tauri/tests/model_tool_eval.rs        # HOLZI_LLAMACPP_PROMPT_CACHE
src-tauri/tests/prompt_cache_eval.rs      # new: spike, equality, US1/US2 timing
```

**Structure Decision**: the kept context is a part of the loaded model inside the existing llama.cpp engine. No
new engine, provider, command or table.

## Delivery order

1. **Spike** (`prompt_cache_eval.rs`, ignored): a kept context with `self_cell`; Gemma 4 E2B with the whole
   sliding-window cache, rewound to the end of the system prompt, gives the same greedy answer as a fresh context;
   Qwen3.5 4B the same after restoring a `PARTIAL_ONLY` checkpoint. Records the checkpoint size and the cache size
   against the plan (research R1).
2. **Kept context for attention models** (`kept.rs`, `model.rs`, `generate.rs`), with the failure rules of R7.
3. **Checkpoints for hybrid models** (R6) and the planner's host budget.
4. **The time stamps** of the user messages (R8).
5. **Measurement**: the stream chunk, the everyday report, the switch, and the runs for SC-001 to SC-004
   (`specs/080-prompt-cache/results.md`).

## Complexity Tracking

No violations. `chat/send.rs` is over the spaex 500-LoC boundary (538 lines) before this feature and only changes
one call here. Split, as a follow-up PR:

| File           | Lines | Split                                                                                                                               | Remains                                    |
| -------------- | ----- | ----------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `chat/send.rs` | 538   | `chat/send_request.rs`: building the `ChatRequest` of a turn (tool availability and offer, reply language, system prompt, messages) | the command, the turn start and its events |
