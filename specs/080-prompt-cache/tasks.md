---
description: 'Task list for spec 080, reusing the prompt a local model has already read'
---

# Tasks: Reuse the prompt a local model has already read

**Input**: Design documents from `specs/080-prompt-cache/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/stream-and-report.md, quickstart.md

**Tests**: included. The spec's acceptance scenarios name what to check, and the spaex constitution asks for one
runnable check per non-trivial piece of logic. Unit tests go in `*_tests.rs` next to their module. Tests with the
real model files are `#[ignore]` in `src-tauri/tests/prompt_cache_eval.rs`.

**Organization**: by user story of spec.md. US1 (tool chain), US2 (follow-up), US3 (nothing worse) and US4
(Qwen3.5) are P1; US5 (memory) is P2.

**Working rules for every task**:

- **graphify first**: before a new named function, run `graphify query "<concept>"` and extend an existing
  candidate where one matches (spaex).
- **Builds**: in the memory scope `systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0`, with
  `cargo … -j 1` and `env -u CC -u CXX -u AR`. Lib tests with `--config 'profile.dev.package.holzi.debug=0'`. The
  CUDA build per spec 078's quickstart (`CUDAARCHS=86`, `CUDAFLAGS=-Xcompiler=-fPIC`,
  `CMAKE_BUILD_PARALLEL_LEVEL=2`, `CUDA_LIBRARY_PATH`). Ask the operator before going above 10 GB.
- **Real files**: Gemma 4 E2B and E4B (`unsloth/gemma-4-E2B-it-GGUF`, `unsloth/gemma-4-E4B-it-GGUF`) and Qwen3.5 4B
  (`bartowski/Qwen_Qwen3.5-4B-GGUF`, `Qwen_Qwen3.5-4B-Q4_K_M.gguf`), all Q4_K_M, under their Hugging Face file
  names in one folder outside the repository. The spike reads that folder from `HOLZI_TEST_GGUF_DIR`; every other
  test runs one model and reads its path from `HOLZI_TEST_GGUF` (quickstart.md).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: the user story of spec.md

---

## Phase 1: Setup

**Purpose**: a knowledge graph, the new dependency, and the spike that settles research R1 before the engine
changes.

- [ ] T001 Run `graphify update .` in the worktree root, so later `graphify query` calls see the current tree.
- [ ] T002 Add `self_cell = "1.3"` to `[dependencies]` in `src-tauri/Cargo.toml` (research R2) and check that
      `Cargo.lock` gains no new package.
- [ ] T003 Spike: create `src-tauri/tests/prompt_cache_eval.rs` (`#![cfg(feature = "llm-llamacpp")]`, module doc with
      the run command of quickstart.md) with the ignored test `prompt_cache_spike`, using `llama_cpp_2` directly:
  - Download the three model files of the working rules into `HOLZI_TEST_GGUF_DIR` if they are missing there (curl
    from `https://huggingface.co/<repo>/resolve/main/<file>`); fail with a message naming the variable when it is
    unset.
  - Build a 4000-token prompt: the model's chat template rendered for a system prompt of repeated tool-like text, a
    user question, and a stand-in tool result appended in a second step.
  - Context: `with_n_ctx(8192)`, `with_n_batch(512)`, `with_swa_full(true)`.
  - Case A, attention cut (Gemma 4 E2B): read the full prompt, answer greedily (60 tokens); then
    `clear_kv_cache_seq(Some(0), Some(n), None)` back to the end of the system part, read the rest of a prompt with
    a different user question, answer greedily. Compare with the same second prompt read into a fresh context.
  - Case B, checkpoint (Qwen3.5 4B, `is_hybrid()` true): read the system part, save
    `state_seq_get(0, LlamaStateSeqFlags::PARTIAL_ONLY)`, read the rest and answer; restore with `state_seq_set`,
    clear the attention cache from the checkpoint's position, read the second prompt's rest and answer. Compare with
    a fresh context. Print the checkpoint size in MiB.
  - Print the KV buffer sizes llama.cpp logs with `swa_full` for Gemma 4 E2B and E4B at 8192 and 32768 tokens, and
    the planner's KV estimate for the same (`ModelShape::from_header`). Print the context's compute buffer sizes at
    the full context length too, and compare them with the planner's `DEVICE_MARGIN`.
  - Write the outcome into research R1, R5 and R6 of `specs/080-prompt-cache/research.md`: equal or not, where
    texts part, checkpoint size, and whether the whole sliding-window cache stays within the plan. If case B is not
    equal, stop and ask the operator before T019.

**Checkpoint**: research R1 has no open point.

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: the loaded model owns one kept context, the pure matching logic exists, and the stream can report
reused tokens.

- [ ] T004 [P] In `src-tauri/src/llm/llamacpp/kept_tests.rs`, test the pure functions of `kept.rs` (data-model
      "Reuse"), independent of llama.cpp:
  - `common_prefix(kept, prompt)`: empty kept; identical; prompt longer; prompt shorter; first token differs; the
    result is at most `prompt.len() - 1`.
  - `checkpoint_for(checkpoints, common)`: none; one below; one above; several, the latest at or below wins.
  - `keep_checkpoint(list, new)`: at most 4, oldest dropped first, a duplicate `n_tokens` replaced, ordered; the
    head checkpoint is not in the list and never dropped by it.
  - `head_end(prompt, head)`: the longest common beginning of the prompt's tokens and the tokens of the rendered
    prompt up to the first user message's text; 0 without a user message.
- [ ] T005 Create `src-tauri/src/llm/llamacpp/kept.rs`, declared in `src-tauri/src/llm/llamacpp/mod.rs`, with:
  - `KeptContext<'m>` with the fields `context: LlamaContext<'m>`, `tokens: Vec<LlamaToken>`,
    `checkpoints: Vec<Checkpoint>`, `head: Option<Checkpoint>`, `hybrid: bool` and `enabled: bool`, and
    `Checkpoint { n_tokens: usize, state: SeqState }`;
  - the pure functions of T004;
  - `fn clear(&mut self)`: empties the cache (`clear_kv_cache`), `tokens`, `checkpoints` and `head`.
  - Nothing of it is ever written to a file (spec FR-012); no `state_save_file` or session file.
  - A `ponytail:` comment on the checkpoint constants (`CHECKPOINT_EVERY = 1024`, `MAX_CHECKPOINTS = 4`): fixed
    numbers from one measurement (research R6).
- [ ] T006 In `src-tauri/src/llm/llamacpp/model.rs`:
  - wrap model and kept context with `self_cell!` (owner `LlamaModel`, dependent `Mutex<KeptContext<'_>>`,
    `#[not_covariant]`), stored in `LoadedModel` in place of the bare `model` field; give the module's users
    `fn model(&self) -> &LlamaModel` and
    `fn with_kept<R>(&self, f: impl FnOnce(&LlamaModel, &mut KeptContext) -> R) -> R`;
  - create the kept context at load with `Placement.context`, batch 512, the load's threads and
    `with_swa_full(true)`; `hybrid = model.is_hybrid() || model.is_recurrent()`;
  - add `LoadOptions.prompt_cache: bool`, default `true` (implement `Default` by hand), stored as `enabled`;
  - adjust every reader of `loaded.model` in `src-tauri/src/llm/llamacpp/` (`adapter.rs`, `generate.rs`, tests).
- [ ] T007 [P] In `src-tauri/src/adapters/types.rs`, add the variant
      `StreamChunk::PromptCache { reused_tokens: usize }` (contract). Ignore it in every exhaustive match the
      compiler names (at least `chat/turn/step.rs` and `model_jobs/runner.rs`); no other adapter changes (research
      R10).
- [ ] T007a In `src-tauri/src/adapters/types.rs`, add `ChatRequest.reuse_prompt: bool` (contract, research R7):
      `true` at every construction except `model_jobs/runner.rs::request`, which sets `false`; a test in
      `src-tauri/src/model_jobs/runner_tests.rs` checks that a job's request never reuses.

**Checkpoint**: the app builds and behaves as today; every answer still reads its whole prompt.

---

## Phase 3: User Story 1 - The next step of a tool chain starts at once (P1) 🎯 MVP

**Goal**: with an attention model (Gemma 4), the second step of a tool chain reads only what was appended.

**Independent Test**: `prompt_cache_chain` (T011) with Gemma 4 E2B and E4B: the second step's first token comes at
least three times sooner than with `prompt_cache: false`, and the answers are equal.

### Tests for User Story 1

- [ ] T008 [P] [US1] In `src-tauri/src/llm/llamacpp/kept_tests.rs`, test
      `KeptContext::plan_read(&prompt, reuse_prompt) -> Read { reuse: usize, rewind: Rewind }` as a pure decision
      (no context): nothing kept → reuse 0; matching beginning on an attention model → `Rewind::Cut(n)`;
      `enabled == false` or `reuse_prompt == false` → reuse 0 and `Rewind::Clear`; kept tokens that fill the whole
      context and a new prompt sharing only a beginning → `Cut` to that beginning, so the new prompt and its answer
      fit (spec edge case "A prompt that fills the context").

### Implementation for User Story 1

- [ ] T009 [US1] In `src-tauri/src/llm/llamacpp/kept.rs`, implement for attention models:
  - `plan_read` (T008), which takes the request's `reuse_prompt`;
  - `fn prepare(&mut self, prompt: &[LlamaToken], reuse_prompt: bool) -> usize`: applies the plan; a `Cut(n)` calls
    `clear_kv_cache_seq(Some(0), Some(n), None)` and truncates `tokens` to `n`; a refused or failing cut clears
    everything and returns 0 (research R4); returns the reused count;
  - `fn record(&mut self, decoded: &[LlamaToken])`: appends decoded tokens to `tokens`.
- [ ] T010 [US1] In `src-tauri/src/llm/llamacpp/generate.rs`, read and write through the kept context:
  - inside `loaded.with_kept`, call `prepare`, decode only `tokens[reused..]` in batches of `BATCH` (cancel checked
    between batches as today), `record` every decoded batch and every decoded answer token;
  - the context is no longer created per generation; the length check against `placement.context` stays;
  - add `Outcome.reused_tokens`; in `src-tauri/src/llm/llamacpp/adapter.rs` send
    `StreamChunk::PromptCache { reused_tokens }` before `Done`.
- [ ] T011 [US1] In `src-tauri/tests/prompt_cache_eval.rs`, add the ignored test `prompt_cache_chain`: through
      `LoadedModel::load` and `generate::generate`, render a 4000-token chat prompt with tools
      (`LoadedModel::render_user_message` does not exist on `main`; render with `llamacpp::template::render` and the
      model's template, or add a small public accessor as spec 079 did), answer step 1, append a stand-in tool
      result as step 2, and measure the time to the first token of step 2 with `prompt_cache: true` and `false`.
      Assert ≥ 3× (SC-001) and equal greedy answers.

**Checkpoint**: Gemma 4 tool chains start their later steps at once.

---

## Phase 4: User Story 2 - A follow-up question in the same conversation starts sooner (P1)

**Goal**: the date and time no longer cut the reuse off; a turn's prompt only grows at its end.

**Independent Test**: `prompt_cache_follow_up` (T015): a follow-up two minutes later reuses the system prompt, the
tools and turn 1, and its first token comes at least twice as soon.

### Tests for User Story 2

- [ ] T012 [P] [US2] In `src-tauri/src/chat/tools/prompt_tests.rs`: `system_prompt` has no date line; with
      `stamps: true` (in place of today's clock parameter) it has the fixed sentence about the stamps, also when it
      has nothing else, and with `false` it is as today without the clock;
      the new `sent_line(clock)` has the format of today's `now_line`; the new `stamped(text, created_at, offset)`
      puts that line and a blank line before the text and gives the same result for the same message at any later
      time, so the messages of turn 1 stay a prefix of those of turn 2 (FR-007).

### Implementation for User Story 2

- [ ] T013 [US2] In `src-tauri/src/chat/tools/prompt.rs`: replace the `clock` parameter of `system_prompt` with `stamps: bool`
      and add the fixed sentence for `true`; rename `now_line` to `pub fn sent_line(clock: Clock)` with a wording for any moment, not only
      now; add `pub fn stamped(text: &str, created_at: i64, utc_offset_minutes: i32) -> String` (research R8).
- [ ] T014 [US2] Call sites: `history_to_messages` in `src-tauri/src/chat/send.rs` passes every user message through
      `stamped` with the turn's `utc_offset_minutes` (one call; `send.rs` is over 500 lines, Complexity Tracking); the
      one `system_prompt` call there,
      in `src-tauri/src/chat/eval/everyday/runner.rs` (whose user messages get the stamp of the run's clock) and in
      `src-tauri/src/chat/eval/runner.rs` (`false` for the `None`). Update the tests of `chat/send` and the everyday runner
      that read the system prompt.
- [ ] T015 [US2] In `src-tauri/tests/prompt_cache_eval.rs`, add the ignored test `prompt_cache_follow_up`: render
      turn 1 with a clock, answer; render turn 2 with the history, a clock two minutes later and a new question;
      measure the time to its first token with `prompt_cache: true` and `false`. Assert ≥ 2× (SC-002) and that the
      reused tokens reach the end of turn 1's answer, apart from what the template writes differently for it.

**Checkpoint**: follow-up questions reuse the system prompt and the tools.

---

## Phase 5: User Story 3 - Nothing gets worse (P1)

**Goal**: no stale reading ever reaches an answer.

**Independent Test**: `prompt_cache_safety` (T018): every answer equals the answer without reuse.

- [ ] T016 [P] [US3] In `src-tauri/src/llm/llamacpp/kept_tests.rs`: `plan_read` after a prompt of another
      conversation reuses only the common beginning; after `clear` it reuses nothing.
- [ ] T017 [US3] In `src-tauri/src/llm/llamacpp/generate.rs`: any `GenerateError` while reading or writing calls
      `kept.clear()` before it returns (research R7); a cancel keeps what was decoded.
- [ ] T018 [US3] In `src-tauri/tests/prompt_cache_eval.rs`, add the ignored test `prompt_cache_safety` with Gemma 4
      E2B: conversation A, conversation B, A again, an answer cancelled after 5 tokens, a request with
      `reuse_prompt: false` (reports 0 reused tokens), then A; each answer compared with the same request on a model
      loaded with `prompt_cache: false` (SC-005, FR-013).

**Checkpoint**: SC-005 holds for attention models.

---

## Phase 6: User Story 4 - The desktop default model gains as well (P1)

**Goal**: Qwen3.5 4B rewinds through checkpoints.

**Independent Test**: `prompt_cache_chain` and `prompt_cache_follow_up` with Qwen3.5 4B.

- [ ] T019 [P] [US4] In `src-tauri/src/llm/llamacpp/kept_tests.rs`: `plan_read` on a hybrid model returns
      `Rewind::Restore { checkpoint, from }` for the latest checkpoint or head at or below the common beginning, and
      `Rewind::Clear` without one; checkpoints beyond the reused point are dropped; after a chain that pushed all four
      checkpoints past the head, a prompt of another conversation restores the head.
- [ ] T020 [US4] In `src-tauri/src/llm/llamacpp/kept.rs`: for `hybrid`, `prepare` restores the chosen checkpoint
      with `state_seq_set`, cuts the attention cache from its `n_tokens` and truncates `tokens`; a failed restore
      clears everything. Add `fn checkpoint(&mut self)` (`state_seq_get(0, PARTIAL_ONLY)`, kept by
      `keep_checkpoint`) and `fn checkpoint_head(&mut self)` (the same state, stored as `head`).
- [ ] T021 [US4] In `src-tauri/src/llm/llamacpp/generate.rs`: on a hybrid model, call `kept.checkpoint()` after every
      `CHECKPOINT_EVERY` prompt tokens read and after the whole prompt is read, before the first answer token, and
      `kept.checkpoint_head()` when reading passes `head_end` and the kept head does not already end there. The
      adapter renders the head (`src-tauri/src/llm/llamacpp/adapter.rs`) and hands its text to `generate`.
- [ ] T022 [US4] Run `prompt_cache_chain`, `prompt_cache_follow_up` and `prompt_cache_safety` with Qwen3.5 4B; fix
      what fails.

**Checkpoint**: SC-001, SC-002 and SC-005 hold for Qwen3.5 4B.

---

## Phase 7: User Story 5 - Memory stays planned (P2)

**Goal**: the kept context and the checkpoints are counted before a load.

**Independent Test**: ten answers in a row with long prompts on each model, on the reference laptop and a phone.

- [ ] T023 [P] [US5] In `src-tauri/src/llm/llamacpp/memory_tests.rs`: a hybrid shape's host budget counts
      `MAX_CHECKPOINTS + 1` checkpoints (the head) of the measured size; an attention shape's placement is unchanged.
- [ ] T024 [US5] In `src-tauri/src/llm/llamacpp/memory.rs`: `ModelShape.checkpoint_bytes` (0 unless hybrid), counted
      `MAX_CHECKPOINTS + 1` times on the host side of `plan`. Read the estimate from the header numbers the spike (T003)
      related to the measured size; record the rule in research R6.
- [ ] T025 [US5] In `src-tauri/tests/prompt_cache_eval.rs`, add the ignored test `prompt_cache_ten_answers`: ten
      answers with 4000-token prompts in a row, each followed by a different conversation; no failure (SC-006). Run
      it on the reference laptop for every catalog model that loads there. On an Android phone, run ten chat answers
      with the mobile default model by hand and record the result in `results.md` (SC-006).

**Checkpoint**: SC-006 holds.

---

## Phase 8: Polish & measurement

- [ ] T026 [P] In `src-tauri/src/chat/eval/everyday/runner.rs`, `scoring.rs` and `report.rs`: record per step the
      time to the first chunk, whether it was text, and the `PromptCache` chunk's tokens (`later_chunk_ms`,
      `later_chunk_text`, `reused_tokens`, contract); the report shows the median of `later_chunk_ms` for the steps
      that start with text and for those that start with a tool call, the reused share and the cache switch. Tests in `report_tests.rs` and
      `scoring_tests.rs`.
- [ ] T027 In `src-tauri/tests/model_tool_eval.rs`: read `HOLZI_LLAMACPP_PROMPT_CACHE=off` into
      `LoadOptions.prompt_cache` and note it in the report's placement string.
- [ ] T028 Run `measure_everyday_voice` with Qwen3.5 4B, Qwen3 1.7B, Gemma 4 E2B, E4B and 12B, with and without the
      cache. Write
      `specs/080-prompt-cache/results.md`: date, commit, machine, pass counts, safety errors, total time,
      `later_chunk_ms`, reused share (SC-003, SC-004).
- [ ] T029 [P] Checks:
  - `cargo fmt --check`;
  - `cargo clippy --all-targets -- -D warnings` with default features, and for `aarch64-linux-android`;
  - `cargo test --lib` for `llm::llamacpp`, `chat::tools::prompt`, `chat::eval`, `adapters`, `model_jobs`;
  - `pnpm typecheck` if a binding changed;
  - prettier on the changed Markdown.
- [ ] T030 Walk through `specs/080-prompt-cache/quickstart.md` once as written, including the in-app steps, and fix
      what differs. Note in `specs/079-mtp-speculative/research.md` R12 of this spec that MTP can be measured again.

---

## Dependencies & Execution Order

- **Setup (T001–T003)** first. T003 decides whether US4 is built as planned.
- **Foundational (T004–T007a)** blocks every story. T007 and T007a are independent of T004–T006.
- **US1 (T008–T011)** needs Foundational.
- **US2 (T012–T015)** needs only T007 for building; T015 needs US1.
- **US3 (T016–T018)** needs US1.
- **US4 (T019–T022)** needs US1 and the outcome of T003.
- **US5 (T023–T025)** needs US4 for the checkpoint size.
- **Polish (T026–T030)** last; T026 and T027 can start after T007.

## Parallel Example: Foundational and User Story 2

```text
T004 kept_tests.rs  ║  T007 StreamChunk::PromptCache  ║  T012 prompt_tests.rs
then T005 kept.rs → T006 model.rs → T008/T009 → T010 generate.rs → T011 chain timing
```

## Implementation Strategy

1. **MVP**: Setup + Foundational + US1 (T001–T011). Gemma 4 tool chains reuse their prompt; measure with
   `prompt_cache_chain`.
2. **Shippable**: + US2 + US3 + US4 + the planner part of US5 (T023–T024), since US4's checkpoints take host
   memory. Follow-ups gain, nothing is ever wrong, and the desktop default model gains. This is the smallest
   version worth a PR.
3. **Then**: T025 (ten answers, phone) and the measurement.
