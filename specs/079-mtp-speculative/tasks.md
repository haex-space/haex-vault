---
description: 'Task list for spec 079, faster local answers with multi-token prediction'
---

# Tasks: Faster local answers with multi-token prediction

**Parked on 2026-11-04** (operator) after T016: the prompt cache comes first (research R11). Done: T001 to T016,
and of T026 the shared token writer (`generate::Writer`). Deviations from the text below:

- The MTP tests live in `src-tauri/tests/mtp_eval.rs`, not in `model_tool_eval.rs`, which keeps only
  `HOLZI_LLAMACPP_DRAFTER` (T015); the file would otherwise pass 500 lines.
- `speculative.rs` and the binding's `common` feature exist on desktops only (plan, Primary Dependencies).
- The drafter check of T012 also compares `embedding_length_out` with the model's embedding width (research R2).
- `mtp_speed` asserts 1.3× (spec SC-001); E2B measured 1.20×, E4B 1.39× (research R11).

**Input**: Design documents from `specs/079-mtp-speculative/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/models-and-catalog.md, quickstart.md

**Tests**: included. The spec's acceptance scenarios and success criteria name what to check, and the spaex
constitution asks for one runnable check per non-trivial piece of logic. Unit tests go in `*_tests.rs` next to
their module (repo convention). Tests with the real model files are `#[ignore]` in
`src-tauri/tests/model_tool_eval.rs`.

**Organization**: by user story of spec.md. US1 (engine), US2 (files) and US3 (fallback) are P1, US4 (placement for a
partly offloaded model) is P2, and US5 (drafter for models installed earlier) is P3.

**Working rules for every task**:

- **graphify first**: before a new named function, run `graphify query "<concept>"` and extend an existing
  candidate where one matches (spaex).
- **Builds**: in the memory scope `systemd-run --user --scope -p MemoryMax=10G -p MemorySwapMax=0`, with
  `cargo … -j 1`, `env -u CC -u CXX -u AR` and `CMAKE_BUILD_PARALLEL_LEVEL=2`. The CUDA lib-test build needs 12 GB;
  ask the operator before going above 10 GB.
- **Real files**: the drafters and models of research R2 sit outside the repository; tests read their paths from
  `HOLZI_TEST_GGUF` and `HOLZI_LLAMACPP_DRAFTER`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: the user story of spec.md

---

## Phase 1: Setup

**Purpose**: a branch that already has the Gemma 4 catalog entries, a knowledge graph, and the spike that settles the
open point of research R6 before any engine code is written.

- [x] T001 Bring the Gemma 4 catalog entries into `079-mtp-speculative`: if #806 is merged, merge `origin/main`;
      otherwise merge `origin/gemma-4-catalog` and note in the PR that it depends on #806. Never rebase: the branch is
      pushed. Check
      `src-tauri/src/catalog/model_catalog.json` has the three `gemma-4-*` entries with `"trial": true`.
- [x] T002 Run `graphify update .` in the worktree root (no `graphify-out/` exists yet), so the later
      `graphify query` calls see the current tree.
- [x] T003 Spike: add the ignored test `mtp_spike` to `src-tauri/tests/model_tool_eval.rs`. Steps:
  - Load `HOLZI_TEST_GGUF` (Gemma 4 E2B) through `llama_cpp_2`. Create a target context with flash attention
    disabled (`with_flash_attention_policy`), the drafter from `HOLZI_LLAMACPP_DRAFTER` with
    `LlamaContextType::Mtp`, and `new_context_with_ctx_other`.
  - Run `MtpSpeculative` for one greedy 200-token answer to the R1 mail-summary prompt, then generate the same
    prompt without the drafter. Assert both answers are non-empty, and log whether the texts are equal and where
    they first differ (spec Assumptions: rare wording differences are allowed).
  - Log whether `clear_kv_cache_seq(Some(0), Some(n_past), None)` returns true after a partial acceptance.
  - If it returns false, repeat with `with_swa_full(true)` and record which setting works.
  - Write the outcome (removal works with or without `swa_full`, the drafter context is created, the answers are
    equal) into research R6 in `specs/079-mtp-speculative/research.md`, replacing "Open point".

**Checkpoint**: research R6 has no open point; the loop shape of T009 is confirmed on the real file.

---

## Phase 2: Foundational (blocks US1, US2, US5)

**Purpose**: the catalog knows each drafter, and one module knows where a drafter lives and whether it is there.

- [x] T004 [P] In `src-tauri/src/catalog/mod.rs`:
  - add `CatalogDrafter { hf_repo, hf_revision, hf_filename, size_bytes, sha256, architecture,
measured_partly_on_gpu }` (serde, `Clone`, `Serialize`; the bool with `#[serde(default)]`);
  - add `CatalogEntry.drafter: Option<CatalogDrafter>` with
    `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- [x] T005 Put the three drafters of research R2 into `src-tauri/src/catalog/model_catalog.json`: E2B, E4B and 12B
      with repo, commit, file, bytes, SHA-256, and `"architecture": "gemma4-assistant"`. None of them gets
      `measured_partly_on_gpu` yet; T031 decides it for the 12B.
- [x] T006 [P] In `src-tauri/src/catalog/catalog_tests.rs`, test that every drafter is well formed:
  - `hf_revision` is 40 hex characters and `sha256` is 64;
  - `hf_filename` ends in `.gguf` and contains no `/` and no `..`;
  - `size_bytes > 0`;
  - exactly the three Gemma 4 entries have a drafter.
- [x] T007 Create `src-tauri/src/models/drafter.rs`, declared in `src-tauri/src/models/mod.rs`. It holds:
  - `drafter_dir(models_root, slug) -> PathBuf` for `models/<slug>/drafter/`;
  - `drafter_file(…)` for `drafter/<hf_filename>`;
  - `state(models_root, slug, &CatalogDrafter) -> DrafterState { Installed, Missing }`: installed means the file
    exists with exactly `size_bytes`;
  - `path_for_load(models_root, slug, &CatalogDrafter) -> Option<PathBuf>`: an installed file whose SHA-256, computed
    with `models::hash::sha256_file` on the blocking pool, equals the pin. A mismatch is logged, removes the
    `drafter/` folder, and returns `None` (spec FR-008, data-model InstalledDrafter).
  - A slug that does not pass `models::paths`' slug checks is refused.
- [x] T008 [P] Tests in `src-tauri/src/models/drafter_tests.rs`, on a temporary directory:
  - the path layout;
  - `Missing` for no file and for a file of the wrong size, `Installed` for the right size;
  - `path_for_load` is `None` for a wrong hash, and the `drafter/` folder is gone afterwards; `Some` for the right one;
  - a hostile slug (`../x`) is refused;
  - `paths::canonical_model_file` still finds the model with a `drafter/` folder next to it.

**Checkpoint**: the catalog and the file layout of the drafter exist and are tested; nothing loads or downloads yet.

---

## Phase 3: User Story 1 - A local model with a drafter answers faster (P1) 🎯 MVP

**Goal**: with a drafter on disk, the llama.cpp engine writes answers 1.4× faster or more, with the same tool calls.

**Independent Test**: `mtp_speed` (T015) on the reference desktop with E4B and its drafter: ≥ 1.4× tokens per second,
and the same tool calls with greedy decoding.

### Tests for User Story 1

- [x] T009 [P] [US1] In `src-tauri/src/llm/llamacpp/memory_tests.rs`, test the drafter rules of research R7:
  - the model plus drafter fit the GPU whole → `drafter: true`, all layers on the GPU;
  - only the model fits whole → `drafter: false` and the same placement as without a drafter;
  - no GPU → `drafter: false`;
  - no drafter in the shape → unchanged results for all existing cases.

  Rule 3 (partly offloaded) belongs to US4; until then the planner returns `drafter: false` for that case.

- [x] T010 [P] [US1] In `src-tauri/src/llm/llamacpp/speculative_tests.rs`, test the acceptance step as a pure
      function, `accepted(draft, sampled) -> (n_accepted, next_token)`, independent of llama.cpp:
  - all proposals agree;
  - the first one disagrees;
  - the middle one disagrees;
  - the draft is empty;
  - an end-of-generation token inside the accepted part ends there.

### Implementation for User Story 1

- [x] T011 [US1] In `src-tauri/src/llm/llamacpp/memory.rs`:
  - add `DrafterShape { file_bytes, kv_bytes_per_token }`, `ModelShape.drafter: Option<DrafterShape>` and
    `Placement.drafter: bool`;
  - implement rules 1, 2 and 4 of research R7 in `plan`, with a `ponytail:` comment naming the ceiling: no
    per-device speed probe, rules from one measurement;
  - `DrafterShape` is read from the drafter's GGUF header with the same `ModelShape::from_header` arithmetic.
- [x] T012 [US1] Extend the load options:
  - in `src-tauri/src/llm/llamacpp/model.rs`: `LoadOptions.drafter: Option<PathBuf>` (the struct loses `Copy`;
    adjust callers), and `LoadedModel.drafter: Option<LlamaModel>`;
  - `placement_for` reads the drafter's header too;
  - when `Placement.drafter` is true, load the drafter with all its layers on the GPU, and check that its
    `general.architecture` is `gemma4-assistant`. A failed drafter load is logged, and the model loads without it
    (spec FR-008).
- [x] T013 [US1] Write `src-tauri/src/llm/llamacpp/speculative.rs`, the loop of research R6 (with the outcome of
      T003):
  - contexts: target with flash attention off, plus `swa_full` if T003 needs it; drafter context `Mtp` with
    `ctx_other`;
  - prompt in 512-token batches through `process`, then `begin`;
  - each round: `draft`, decode `[id_last, draft…]` with all logits, `process`, the `accepted` step of T010,
    `accept`, cache trim of target and drafter;
  - text through `generate::take_complete_utf8`;
  - the same `Outcome`, `Ended`, cancel and `on_text` handling as `generate.rs`;
  - `pub(super) fn generate_speculative(…) -> Result<Outcome, GenerateError>`.

  Keep the file under 500 lines. Register it in `src-tauri/src/llm/llamacpp/mod.rs`.

- [x] T014 [US1] In `src-tauri/src/llm/llamacpp/generate.rs`, call `speculative::generate_speculative` when
      `loaded.placement.drafter` and `loaded.drafter` are set and the drafter is not marked failed (T026); otherwise the plain loop, which stays byte-for-byte as it
      is (spec FR-012).
- [x] T015 [US1] In `src-tauri/tests/model_tool_eval.rs`:
  - read `HOLZI_LLAMACPP_DRAFTER` into `LoadOptions.drafter`;
  - add the ignored test `mtp_speed`. It runs the four requests of research R1 (200 tokens, greedy) with and without
    the drafter, prints tokens per second for both, and asserts:
    - a gain of at least 1.4× (SC-001);
    - equal tool calls for the JSON request (SC-003).
- [x] T016 [US1] Wire the app's load path:
  - `src-tauri/src/llm/engine.rs`: `LoadRequest.drafter: Option<&Path>` passed into `LoadOptions.drafter`;
  - `src-tauri/src/chat/model_loading.rs` `resolve_local_model_metadata`: call `models::drafter::path_for_load` for a
    catalog model whose entry names a drafter. One call site; the logic stays in `drafter.rs`, because the file is
    over 500 lines.

**Checkpoint**: with a drafter placed by hand in `models/<slug>/drafter/`, the app answers faster with Gemma 4.
`mtp_speed` passes for E4B.

---

## Phase 4: User Story 2 - The drafter comes with the model and goes with it (P1)

**Goal**: one download for model and drafter; cancel, delete, cleanup and the installed size treat the drafter as
part of the model.

**Independent Test**: download E2B in the app, cancel once, download again, check the size and the details, delete;
no file remains (quickstart "In the app").

### Tests for User Story 2

- [ ] T017 [P] [US2] In `src-tauri/src/models/drafter_tests.rs`, test the drafter's own steps against a local HTTP
      stand-in. Reuse the stand-in of the model download tests in `src-tauri/src/models/download_tests.rs`; check with
      graphify first. Cases:
  - `stage` writes `drafter/.<file>.<uuid>.staging` and checks the size and SHA-256 against the pin;
  - a wrong hash discards the staging file and returns a typed reason;
  - `publish` renames into `drafter/<file>`;
  - `remove(models_root, slug)` deletes `drafter/` and succeeds when it is missing.
- [ ] T018 [P] [US2] In `src-tauri/src/instances/startup_tests.rs`, next to the existing cleanup tests, check that
      `cleanup_staging_in_dir` also removes `*.staging` and `*.staging.part` inside `drafter/` and leaves an installed
      drafter alone.

### Implementation for User Story 2

- [ ] T019 [US2] In `src-tauri/src/models/drafter.rs`, implement `stage`, `publish` and `remove` (T017):
  - downloads through `models::download::download_to_file` with the pinned revision
    (`huggingface::resolve_download_url`);
  - progress offset by the bytes already counted, so one stream covers both files (contract
    `model-download-progress`).
- [ ] T020 [US2] In `src-tauri/src/models/commands.rs` `download_from_hf_inner`, add call sites only:
  - for a catalog entry with a drafter, `drafter::stage` before the model, under the same cancel token and
    publication lock;
  - `bytesTotal` = the model's announced size + `drafter.size_bytes`;
  - after the model is published, `drafter::publish`;
  - a drafter failure is logged, and the model is installed without it (spec FR-006);
  - on cancel, the drafter's staging file is discarded too (spec US2 scenario 2).
- [ ] T021 [US2] In `src-tauri/src/models/commands.rs`:
  - `delete_installed_model` calls `drafter::remove` after removing the model file (spec FR-005, SC-005);
  - `list_installed_models` and the complete event's `InstalledModelPayload` fill `drafter: { installed, sizeBytes }`
    for catalog models with a drafter, and add an installed drafter to `sizeBytes` (contract).
- [ ] T022 [P] [US2] In `src-tauri/src/models/import.rs`, extend `cleanup_staging_in_dir` to the `drafter/` folder
      of each slug (T018).
- [ ] T023 [P] [US2] In `src-tauri/src/models/download_check.rs`, add the drafter's size for a catalog target, and
      test it in its `_tests.rs`.
- [ ] T024 [P] [US2] No code change in `src-tauri/src/catalog/mod.rs`: `list_catalog` and `recommend_tiers` keep
      classifying the fit on `approx_size_bytes` alone, because a drafter that does not fit is left out at load
      (contract `list_catalog`, research R7 rule 2). Only the shown size adds the drafter (T025).
- [ ] T025 [P] [US2] In the frontend, show model sizes with the drafter:
  - `src/composables/useCatalog.ts`: `drafter?: { size_bytes: number }` on `CatalogEntry`;
  - `src/composables/useModels.ts`: `drafter?: { installed: boolean; sizeBytes: number }` on `InstalledModel`;
  - `src/components/chat/ModelSelection.vue` and `src/stores/models.ts` (`downloadTotalBytes` seed) show
    `approx_size_bytes + (drafter?.size_bytes ?? 0)`.

**Checkpoint**: a fresh Gemma 4 download brings its drafter; cancel and delete leave nothing behind.

---

## Phase 5: User Story 3 - Without a drafter everything works as today (P1)

**Goal**: a missing, wrong, unloadable or failing drafter never shows an error in the chat.

**Independent Test**: the four failure cases of SC-004 each still get an answer.

- [ ] T026 [US3] In `src-tauri/src/llm/llamacpp/speculative.rs`: on any `MtpSpeculativeError` or failed draft
      decode, trim the target cache to `n_past`, log the reason once, and finish the answer with the plain token loop on
      the same target context. Extract the plain per-token step of `generate.rs` into a function both loops call, rather
      than copying it.
  - The rest of that answer keeps flash attention off (research R6 step 5).
  - Mark the loaded model's drafter as failed (an `AtomicBool` on `LoadedModel`). `generate.rs` (T014) then takes the
    plain path with today's context settings until the model is loaded again (spec FR-012).
- [ ] T027 [P] [US3] In `src-tauri/tests/model_tool_eval.rs`, add the ignored test `mtp_fallbacks`, with the real E2B
      file. Every case must answer, and each model load must succeed. In the forced-failure case, the text emitted
      before the failure must appear exactly once, followed by the rest of the answer:
  - no drafter;
  - a drafter path that does not exist;
  - the E4B drafter given to E2B (wrong model). Creating its context or the first draft fails and the answer falls
    back, or it only proposes tokens the model rejects. Either way the answer comes (research R2, pairing);
  - a drafter whose `MtpSpeculative` is forced to fail after the first round, through a test-only hook of
    `speculative.rs` behind `#[cfg(test)]` or an env switch read only in tests.
- [ ] T028 [P] [US3] In `src-tauri/src/models/drafter_tests.rs`, test that `path_for_load` returns `None` for a missing
      file, a wrong size and a wrong hash, never panics, and removes the `drafter/` folder only for the wrong hash.

**Checkpoint**: SC-004 holds; the plain path is unchanged for models without a drafter.

---

## Phase 6: User Story 4 - The memory planner counts the drafter (P2)

**Goal**: a model that fits the GPU only partly gets the drafter only where its catalog drafter says
`measured_partly_on_gpu`. It is placed so the load cannot fail. Research R1 measured the 12B at a fixed 24 layers;
T031 measures it in the app's own placement.

**Independent Test**: load Gemma 4 12B with its drafter on the reference laptop ten times (SC-006).

- [ ] T029 [US4] In `src-tauri/src/llm/llamacpp/memory_tests.rs`, test rule 3 of research R7:
  - without `measured_partly_on_gpu`, a model that fits only partly gets `drafter: false` and the old placement;
  - with it, the drafter's bytes and cache come off the device budget first;
  - the model's layers are planned in the rest, and `drafter: true`;
  - if the drafter alone does not fit the device, `drafter: false` and the old placement.
- [ ] T030 [US4] Implement rule 3 in `src-tauri/src/llm/llamacpp/memory.rs` `plan`.
- [ ] T031 [US4] In `src-tauri/tests/model_tool_eval.rs`, add the ignored test `mtp_load_12b`: ten loads of
      `HOLZI_TEST_GGUF` (12B) with its drafter through the app's own placement, each followed by one short answer
      (SC-006).
  - Run `mtp_speed` for the 12B with the app's placement with the drafter against its placement without.
  - Only if the drafter is faster, set `measured_partly_on_gpu: true` on the 12B drafter in
    `src-tauri/src/catalog/model_catalog.json`.
  - Record both figures in `results.md` (T036).

**Checkpoint**: the 12B with its drafter loads every time. It uses the drafter only if T031 measured a gain;
otherwise it loads without, and `results.md` records SC-001 for the 12B as not reached.

---

## Phase 7: User Story 5 - Gemma 4 models installed before this feature get their drafter (P3)

**Goal**: an installed model without its drafter shows "Speed-up available (size)" and fetches it with one action.

**Independent Test**: install E2B, remove its `drafter/` folder, open the settings, download the speed-up, chat; the
next answer uses the drafter.

- [ ] T032 [US5] Add the command `download_model_drafter(model_id)` (contract) in `src-tauri/src/models/commands.rs`:
  - call sites only: the work is `drafter::stage` + `drafter::publish`;
  - the same cancel registry, publication lock and progress events;
  - the errors `ModelNotFound` and `InvalidInput`, and `ModelDownload` for a check failure;
  - register it in `src-tauri/src/lib.rs` next to `download_model_from_catalog`, not on the vault gate's
    allow-list.
- [ ] T033 [P] [US5] Tests for `download_model_drafter` in `src-tauri/src/models/commands_tests.rs`, against the
      local stand-in: success, model not installed, no drafter in the catalog, wrong hash.
- [ ] T034 [US5] The next load uses a newly downloaded drafter without an app restart: if the model is loaded,
      `download_model_drafter` reloads it through the existing deactivate-and-load path
      (`chat::local_model::deactivate`, the in-app side of the `deactivate_local_model` command, which
      `delete_installed_model` already uses; then `activate_local_model`'s load).
- [ ] T035 [US5] In the frontend:
  - `src/composables/useModels.ts`: `downloadDrafterAsync(modelId)`;
  - `src/components/settings/InstalledModels.vue`: in the details of the selected model, "Beschleunigung: aktiv" /
    "Speed-up: on" when installed. When missing, "Beschleunigung verfügbar (98 MB)" / "Speed-up available (98 MB)"
    with a download button that uses the model's existing download bar and cancel.
  - Keys `models.installed.drafter.{on,available,download}` in `src/i18n/locales/de.json` and
    `src/i18n/locales/en.json`.

**Checkpoint**: an older install can fetch its speed-up; nothing downloads without the user.

---

## Phase 8: Polish & measurement

- [ ] T036 Run `mtp_speed` for E2B, E4B and 12B on the reference laptop. Write
      `specs/079-mtp-speculative/results.md` with:
  - date, commit and machine;
  - tokens per second with and without the drafter, and the acceptance rate;
  - the comparison to research R1.
- [ ] T037 Extend its everyday report with generated tokens and generation time per request
      (research R9), and run `measure_everyday_voice` for each Gemma 4 model with and without
      `HOLZI_LLAMACPP_DRAFTER`. Record pass counts, safety errors and the time to the first chunk in `results.md`
      (SC-002, SC-003, SC-007).
- [ ] T038 [P] Checks:
  - `cargo fmt --check`;
  - `cargo clippy --all-targets -D warnings` without features, with `llm-llamacpp`, and for
    `aarch64-linux-android` (in the Android devShell, with the NDK's clang as `CC_aarch64_linux_android`);
  - `cargo test --lib` for `catalog`, `models::` and `llm::`;
  - `pnpm typecheck`, `pnpm typecheck:scripts` and `pnpm check:agent-actions`;
  - prettier on the changed Markdown, JSON and Vue files;
  - after `cargo test`, strip trailing whitespace from `src/types/bindings/*.ts` as the CI does.
- [ ] T039 Walk through `specs/079-mtp-speculative/quickstart.md` once as written, including the in-app steps, and
      fix what differs.

---

## Dependencies & Execution Order

- **Setup (T001–T003)** first. T003 decides the cache settings that T013 uses.
- **Foundational (T004–T008)** blocks US1's app wiring (T016), US2 and US5.
- **US1 (T009–T016)**: the engine tasks T009–T015 need only Setup. The test drafter path comes from the
  environment, so they can run in parallel with Foundational. T016 needs T007.
- **US2 (T017–T025)** needs Foundational. It does not need US1: files can be downloaded before the engine uses them.
- **US3 (T026–T028)** needs T013–T014.
- **US4 (T029–T031)** needs T011.
- **US5 (T032–T035)** needs US2's `stage` and `publish` (T019).
- **Polish (T036–T039)** last.

## Parallel Example: User Story 1

```text
T009 memory_tests.rs (drafter rules)   ║   T010 speculative_tests.rs (acceptance step)
then T011 memory.rs → T012 model.rs → T013 speculative.rs → T014 generate.rs → T015 mtp_speed → T016 app wiring
```

## Implementation Strategy

1. **MVP**: Setup + US1 (T001–T003, T009–T016) with T004–T007. A drafter put in place by hand makes the app faster.
   Measure with `mtp_speed`.
2. **Shippable**: + US2 + US3. The drafter downloads with the model, and nothing can fail visibly. This is the
   smallest version worth a PR.
3. **Then**: US4 (12B gain), US5 (older installs), Polish.
