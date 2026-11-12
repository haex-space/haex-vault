# Tasks: Context for local models

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md),
[contracts/context-settings.md](./contracts/context-settings.md)

**Delivery**: stages A to D of the plan, each its own PR from `main`, none stacked. `[P]` marks tasks in different files that can
run together. `[US1]`–`[US3]` map to the spec's user stories. Tests sit in `*_tests.rs` next to the module, never in the module.
Before every PR: `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, the targeted `cargo test`, and for the UI
stage `pnpm typecheck`, `pnpm lint`, `pnpm check:agent-actions`, Prettier.

## Phase 1: Probe (PR A)

- [x] T001 Write `src-tauri/examples/context_probe.rs`: takes a GGUF path, `--cache standard|q8|q4`, `--context N`, `--yarn`;
      loads through `LoadedModel::load`, prints one JSON line with free device memory before and after the load, the time to
      fill the context with a synthetic prompt, tokens per second at 4k and at the end, and the needle test of R9 (a code word
      at 10 %, 50 %, 90 % of the prompt, three greedy runs each). It uses only the public engine API; if something is private,
      export it under the `llm-llamacpp` feature, do not copy it. It needs the settings of T006–T008, so it is finished after them.
- [x] T002 Operator: run the probe for Gemma 4 12B, Gemma 4 E4B, Qwen3.5 4B and Qwen3 4B as in quickstart M1 and add the table to
      `research.md` R9. Not blocking for A or C; blocks the recommendation text of B (T016). Done 2026-11-11 for Qwen3.5 4B and Gemma 4 E4B
      (R9 result); Gemma 4 12B left out for now (does not fit beside a running app), Qwen3 dropped (operator: Qwen3.5 is the minimum).

## Phase 2: Foundational — the engine takes a setting (PR A)

- [x] T003 [P] [US2] In `src-tauri/src/llm/llamacpp/memory.rs` add `CacheType {Standard, Q8, Q4}` and make
      `Kv::bytes(context, cache)` count 2, `34/32` and `18/32` bytes per stored value (R3), the fixed state of hybrid layers unchanged. Replace
      `KV_BYTES_PER_VALUE`. Tests in `memory_tests.rs`: Qwen3 4B at 131072 is 18 GiB / 9.6 GiB / 5.1 GiB, Gemma 4 12B 2.6 / 1.4 GiB,
      Qwen3.5 4B hybrid state not shrunk.
- [x] T004 [US1][US2] In the same file add `ContextChoice {Auto, Fixed(u32)}` and extend `plan(shape, free, wanted, choice, cache)`
      and `Placement` with `cache` and `yarn: Option<f32>` (R5): Auto as today; Fixed whole-GPU, then partly host, then
      `MemoryRefusal`, never shrinking; above `4 × trained` and below 2048 refused before memory is looked at; above trained sets
      `yarn = context / trained`. `drafter_fits` counts the cache type. Tests: the table of the spec's US1 (Gemma 4 12B 131072
      all layers on 12 GiB, Qwen3.5 4B 131072, Qwen3 4B 32768, no GPU, unsized header), fixed fits / fits on host / too large /
      beyond training / not stretchable, drafter with q8.
- [x] T005 [P] [US2] Add `limits(shape, free, cache) -> Limits {trained, max_on_gpu, max_any, max_stretched}` next to `plan`,
      sharing its helpers (one implementation, plan Design notes). Tests with the same fixtures, including "no GPU" and "not even 2048".
- [x] T006 [P] [US2] In `src-tauri/src/llm/gguf_header.rs` read `<arch>.rope.scaling.type` (string) and
      `<arch>.rope.scaling.original_context_length` into `GgufSummary`; `ModelShape::from_header` carries `stretchable` (false
      when the file declares a rope scaling the engine must not override). Tests in `gguf_header_tests.rs` with a header fixture.
- [x] T007 [US2] Create `src-tauri/src/llm/llamacpp/context_params.rs`: `fn context_params(placement, n_ctx, trained, threads) -> LlamaContextParams`
      setting `type_k`/`type_v`, flash attention (enabled for a non-standard cache), `swa_full`, and, when
      `placement.yarn` is set and `n_ctx > trained`, `RopeScalingType::Yarn`, `rope_freq_scale = trained / yarn_context`,
      `yarn_orig_ctx = trained` (R4). Use it in `generate.rs` and in `speculative.rs` (model and drafter). Tests in
      `context_params_tests.rs` read the params back through the binding's getters, including "short request, no scaling".
- [x] T008 [US2] In `model.rs` add `choice` and `cache` to `LoadOptions`, pass them to `plan`, and after the weights are loaded
      make one context of 512 tokens with `context_params`; a failure becomes `LoadError::Load` with the engine's text and the
      model is dropped (R2). Test: a placement the fake params builder rejects returns the error (build the check as a function
      of the builder so it runs without a model); the existing `--ignored` real-file test also loads with q8.

## Phase 3: The setting and its commands (PR A, continued)

- [x] T009 [P] [US2] Create `src-tauri/src/models/context_setting.rs`: `ContextSetting`, `read(q, device, model_id)`,
      `write(tx, device, model_id, setting)` (deletes the row for Auto + standard), `validate(shape, setting)`, the rules of
      data-model.md, key `models.context.<model id>` in `PrefScope::Device`. Tests in `context_setting_tests.rs` in the style of
      `agent_browser/choices_tests.rs`: default on no row, on garbage, round trip, deletion.
- [x] T010 ~~Call `delete_for_model` from `delete_installed_model`~~ Dropped: deleting an installed model removes only its
      file on this device and keeps the synced `models` row, so its context setting stays too and a reinstall finds it again
      (spec 086 FR-005: a setting is per model and device). Nothing deletes a setting row but choosing Auto and the standard cache.
- [x] T011 [US2] Extend `LoadRequest` in `src-tauri/src/llm/engine.rs` with `choice` and `cache`; in
      `chat/model_loading.rs::resolve_local_model_metadata` read the setting of this device (find the device uuid with
      `current_device_uuid`) and pass it through `load_local_model_from_metadata`; add `placement {context, gpuLayers, cache, yarn}`
      to `LoadedModelInfo` from `LoadedModel::placement()`. Tests: a setting in the vault reaches `LoadOptions` (test on the
      function that builds the request, no model).
- [x] T012 [US2] Create `src-tauri/src/models/context_commands.rs` with `get_model_context`, `set_model_context` (validates,
      refuses with the reason, keeps a `too-large` choice) and `model_context_report` (header + free memory →
      `limits` + `plan` for the given setting → `ContextReport`, `effective` from the loaded model when it is the same id, R7);
      register all three in the invoke handler and refresh the bindings. Tests in `context_commands_tests.rs` for every verdict.

**Checkpoint A**: with a preference row written by hand, a model loads with that context and cache; the probe runs; everything
else is as before. Open PR A.

## Phase 4: User Story 2 — the settings page (PR B)

- [x] T013 [US2] Add `settings.models.context.get`, `.set`, `.report` to `src/lib/actions/settingsActions.ts` with scope
      `guardrails`, the handlers in `src/stores/settingsActionHandlers.ts` calling the three commands, and run
      `pnpm export:eval-tools` and `pnpm check:agent-actions` (the snapshot must not change, FR-011).
- [x] T014 [P] [US2] Create `src/components/settings/ModelContextSetting.vue` in the Group/Row/OptionRow style: a context row
      (Auto or a number, with the limits as hints), a cache row (standard, 8-bit, 4-bit), three read-only lines (trained, allowed,
      in effect), and the sentences for every `verdict` and for the cache and YaRN notes (spec FR-006–FR-009). Saves on selection,
      errors as toast, a wrong number under its field, reset to Auto. No apply or "saved" notice.
- [x] T015 [US2] Mount it below the selected-model group of `src/components/settings/InstalledModels.vue` (done as `SettingsModelContextSetting`, with the typed length as an "Other…" row of the select and the pure helpers in `src/lib/models/contextSetting.ts`, tested by `scripts/check-model-context.ts` under `pnpm check:settings`); after a change on the
      active model call `settings.models.activate` (R8) and show the existing busy state. Keys in `src/i18n/locales/de.json` and
      `en.json` under `models.context.*` (no `<…>` in messages).
- [x] T016 [US2] After T002: write the recommendation sentence for 8-bit and the Qwen3 hint from the measured table; if the
      table says 8-bit is not worth recommending, say only what it costs. Component test or story for the sentences by verdict.
      Done 2026-11-11: the 8-bit sentence (`models.context.note.cacheQ8`); no Qwen3 hint, since Qwen3 was dropped (T002).

**Checkpoint B**: quickstart M2 and M3. Open PR B.

## Phase 5: User Story 3 — compaction (PR C)

- [x] T017 [US3] `MessageRole::Summary`, the validator rule (a summary row names the last row it covers as its parent and
      carries no tool column) and the order of the row in the thread in `src-tauri/src/storage/chat_messages.rs`. No migration
      (research R12: `parent_id` is the covered row). Tests in `chat_messages_tests.rs`: validation, and the order in a real vault.
- [x] T018 [US3] Every reader of `chat_messages` (the "every call site" rule): `chat/send.rs::history_to_messages` (summary rows are
      not messages), `chat/thread_commands.rs` (role to string), `chat/search.rs` (selects only `user` and `assistant`, so
      summaries are not hits), `send_admission::last_message_id` (a summary is never the last row), the frontend role union in
      `src/composables/useChat.ts` and `MessageList.vue`; the peer chat has its own tables. Nothing else reads the table.
- [x] T019 [P] [US3] Split `src-tauri/src/chat/turn/fit.rs` into `leave_out_old_results(request, limit)` and
      `cut_latest(request, limit)`, both returning whether the request now leaves the headroom; `fit` calls both so nothing else
      changes. Export the headroom constant shared with `chat/tools/context_budget.rs`. Existing `fit_tests.rs` stay green; add
      tests for each stage alone.
- [x] T020 [P] [US3] Create `src-tauri/src/chat/compact/select.rs`: from the rows, the span to cover (everything before the latest
      user message, minus what the latest covering summary already covers), the effective history with a summary applied (the
      summary goes to a trailing paragraph of the system prompt, R11), and the "would not leave the headroom" test using
      `estimate_tokens`. Tests in `select_tests.rs`: first turn, a turn after a summary, a thread without a user message, a summary
      whose covered row is gone, two summaries, headroom boundary.
- [x] T021 [P] [US3] Create `src-tauri/src/chat/compact/summarize.rs`: the instruction in the interface language (reuse
      `chat/reply_language.rs`), input as plain text with each tool result clipped to 1500 characters and attachments named,
      chunks that fit the context less 1024 tokens, the adapter call without tools honouring the turn's cancel token, at most four
      calls, the result cut at a paragraph end to about 1200 tokens, an empty answer treated as failure. Tests in
      `summarize_tests.rs` with a fake adapter: one chunk, two chunks, an error, a cancel, an empty answer, an attachment.
- [x] T022 [US3] Create `src-tauri/src/chat/compact/mod.rs` with `prepare(...)`: stages in the order of R10 (leave out → stored
      summary → new summary → cut), only when `adapter.context_limit()` is `Some` (FR-018); stores the new row, returns the
      request and the turn's `Compaction` state. On failure: no row, `cut_latest`, `failed = true`. Call it from `chat/send.rs`
      before the first model call; keep `tool_round.rs` on plain `fit`. Emit the new event `chat-compaction`
      (`summarising`, `done`, `failed`, in `chat/events.rs`), shown as a notice by `useToolNotice`.
- [x] T023 [US3] Integration tests in `src-tauri/src/chat/compact/compact_tests.rs` (fake adapter, context of 8192): ten browser-page
      results in a history → no stage fails and the first task survives in the summary text sent (scenario 1, SC-002/006); a stored
      summary is not used when the request fits without it, a larger context uses the originals (scenario 3, FR-019); a second
      overflow summarises the first summary and keeps the newest rows verbatim (scenario 4); the latest user message and the open
      round are never in the input (scenario 6); an unknown limit is never summarised (FR-018).
- [x] T024 [P] [US3] Frontend: `src/components/chat/MessageList.vue` shows a summary row as a centred line "Older messages
      summarised" with a disclosure for the text, never as a bubble; the covered rows render as before. Keys `chat.summary.*` in
      both locale files. The texts of the two notices (`chat.compaction.*`). Lint, typecheck.
- [x] T025 [US3] Voice (checked: `assistant/turn.rs` starts its turn through `chat::send::start_turn`, so it runs through `prepare`; the summary takes a model call before the first word, like a slow model, and the assistant's own cue is unchanged): check that a voice turn (spec 066) goes through `send.rs` and so through `prepare`, and that the spoken
      "thinking" cue covers the summarising state; add the one test or the note to the PR that settles it.

**Checkpoint C**: quickstart M4 and M5. Open PR C.

## Phase 6: Closing (PR D)

- [ ] T026 Operator: quickstart M1–M5 by hand; the measurement table in `research.md` R9 (from T002); mark spec 086 Done in
      `spec.md`; set the checkboxes here.
- [x] T027 Update `CONTEXT.md` with the terms "context in effect" and "summary row"; add a memory note for the work (decisions of
      2026-11-09: 128k minimum, settings per model, summary stored and visible) and the index line.
- [x] T028 Revisit the defaults only if T002 shows a model that Auto handles badly (a number from the table, a PR of its own).

## Dependencies and order

- A: T003 → T004 → T005; T006 beside T003; T007 needs T004; T008 needs T007; T009 beside; T010–T012 need T004, T005, T009; T001
  needs T007, T008. B needs A. C needs only the context limit that is already on `main`; its tasks T017–T025 are independent of A
  and B and may be built first.
- Within C: T017, T019, T020, T021 are parallel; T018 after T017; T022 needs T017, T019–T021; T023 needs T022; T024 needs T017.

## Parallel examples

- A: T003, T006 and T009 together (three files). Later T005 next to T007.
- C: T019, T020 and T021 together (three files) once T017 has landed.

## Implementation strategy

- **MVP**: stage C alone ends the "session unusable" failure for every local model, whatever the settings; stage A+B give the
  choice. The operator asked for both; the order above follows the agreed one (settings first) and C can move up if the
  measurement for A or B is not in yet.
- Each stage ends with the checkpoint and its own PR. Nothing is stacked; the next stage starts from `main` after the merge.
