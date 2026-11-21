# Tasks: STT Model Choice

**Input**: Design documents from `specs/010-stt-model-choice/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/tauri-commands.md](contracts/tauri-commands.md), [quickstart.md](quickstart.md)

**Tests**: Included — Rust tests live in a dedicated `*_tests.rs` file next to the tested module
(never inline `#[cfg(test)] mod tests`), matching this repo's convention (see spec 008's tasks.md
and CLAUDE.md). Every new/changed command module and every changed existing module with its own
test file gets a paired test task — mirroring `models/commands_tests.rs`,
`storage/preferences_commands_tests.rs`, and `chat/commands_tests.rs`, which already exist for
their respective command modules. The repo has no frontend test runner (no `vitest`/component
harness); frontend verification is `pnpm typecheck` plus the manual `quickstart.md` walkthroughs
per user story.

**Organization**: Tasks are grouped by user story (spec.md: US1 P1, US2 P2) so each story is
independently implementable and testable. Both stories share the same Foundational phase, since
the catalog/storage/adapter refactor is genuinely needed by both — there is no meaningful way to
build the onboarding step or the Settings section without it existing first.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Maps the task to US1 / US2 from spec.md

## Phase 1: Setup

- [x] T001 [P] Create `src-tauri/src/stt/stt_catalog.json` with three entries (`whisper-tiny`,
      `whisper-base`, `whisper-small`) — `id`, `name`, `hf_repo`, `hf_revision`, `approx_size_bytes`
      (sum of all three files, not just weights), `license`. Use the pinned revisions and sizes
      already resolved in [research.md §6](research.md#6-gepinnte-revisionen-für-whisper-basewhisper-small):
      `whisper-tiny` keeps the existing `169d4a4341b33bc18d8881c4b69c2e104e1cc0af`; `whisper-base`
      pins `e37978b90ca9030d5170a5c07aadb050351a65bb`; `whisper-small` pins
      `973afd24965f72e36ca33b3055d56a652f456b4d`.

**Checkpoint**: Catalog data exists; nothing reads it yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared infrastructure both user stories need. No user story can be verified
end-to-end until this phase is done.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T002 [P] Extract the sort/pick algorithm inside `catalog::recommend_tiers`
      (`src-tauri/src/catalog/mod.rs:150`) into a new generic helper in
      `src-tauri/src/hardware/tiers.rs` (e.g. a generic `pick_three` returning three
      `(Tier, T, Fit)` tuples), moving the
      `Tier` enum alongside it if that avoids a `catalog` → `hardware` dependency inversion. Add
      `src-tauri/src/hardware/tiers_tests.rs` covering the same Easy/Sweet/Max fallback cases
      `catalog/catalog_tests.rs` already covers for the LLM catalog. Update
      `catalog::recommend_tiers` to call the new helper — this must be a behavior-preserving
      refactor; the existing `catalog_tests.rs` suite passes unmodified.
- [x] T003 [P] Create `src-tauri/src/stt/catalog.rs` mirroring `catalog/mod.rs`: `SttCatalogEntry`
      struct (per [data-model.md](data-model.md#neue-entität-sttcatalogentry-statischer-katalog-kein-db-table)),
      `entries()`/`get(id)` reading `stt_catalog.json` (T001) via `OnceLock`, `CatalogEntryWithFit`,
      and `recommend_tiers(&HardwareInfo)` built on `hardware::tiers::pick_three` (T002). Add
      `src-tauri/src/stt/catalog_tests.rs`. Register the module in `src-tauri/src/stt/mod.rs`.
      (depends on T001, T002)
- [x] T004 In `src-tauri/src/stt/local.rs`: remove the `WHISPER_REPO`/`WHISPER_REVISION`/
      `WHISPER_DIR` constants and the `model_dir()` function. Change `LocalWhisperAdapter::load`,
      `load_from_dir`, and `ensure_model_files` to take a `&stt::catalog::SttCatalogEntry` instead
      of reading the removed constants, resolving the canonical model directory via
      `models::paths::slug_dir(app, &entry.id)` / `model_file_path` (existing helpers, unchanged) —
      the download URL becomes `https://huggingface.co/{entry.hf_repo}/resolve/{entry.hf_revision}/{filename}`
      as before, just parameterized. Add a shared `resolve_model_dir` resolver plus
      `is_complete_file`/`is_complete_model` helpers: each file must be a regular file with size > 0,
      and `ensure_model_files` must redownload every file failing that predicate. Run the same
      resolver before status listing, loading, and downloading. Update
      `src-tauri/src/stt/local_tests.rs`'s fixtures to pass an entry instead of relying on removed
      constants, and cover zero-byte/partial files. (depends on T003)
- [x] T005 [P] Add `list_installed_stt_models` (checks, for every `stt::catalog::entries()` item,
      whether `config.json`/`tokenizer.json`/`model.safetensors` all satisfy the shared
      `is_complete_model` predicate under its slug dir via `models::paths`) and the Tauri commands `list_stt_catalog`, `stt_recommend_tiers`,
      `list_installed_stt_models`, `download_stt_model` in new `src-tauri/src/stt/commands.rs`,
      per [contracts/tauri-commands.md](contracts/tauri-commands.md). `download_stt_model` resolves
      the catalog entry via `stt::catalog::get` (404s as `CatalogEntryNotFound` if unknown), invokes
      the same directory resolution as `load` and the status listing, and calls
      `ensure_model_files` (T004) so incomplete files are repaired rather than treated as installed.
      (depends on T003, T004)
- [x] T006 [P] Add `src-tauri/src/stt/commands_tests.rs` covering `list_installed_stt_models`
      (no files / zero-byte files / partial files / all files present, per catalog entry) and `download_stt_model`'s
      `CatalogEntryNotFound` path for an unknown id — mirroring how `models/commands_tests.rs`
      covers the analogous chat-model commands. (depends on T005)
- [x] T007 [P] In `src-tauri/src/voice.rs`: add an `invalidate_stt_model_cache` Tauri command and a
      `VoiceState` method that resets the cached `whisper: AsyncMutex<Option<Arc<LocalWhisperAdapter>>>`
      to `None` in the `voice` + `llm-cpu` build. Keep the command registered in all feature sets:
      the `voice`-without-`llm-cpu` implementation and the `voice`-disabled stub return successful
      no-ops without accessing the cfg-disabled cache field. Change `resolve_local_adapter` to read the device-scoped `voice.stt_model_id`
      preference before loading, resolve it via `stt::catalog::get`, and fall back to the
      `"whisper-tiny"` entry when the preference is missing, empty, or names an unknown id — this
      preserves today's behavior exactly for anyone who never touches the new setting (FR-007).
      (depends on T003, T004)
- [x] T008 [P] Extend `src-tauri/src/voice_tests.rs` with cases for T007's new behavior: preference
      missing/empty/unknown-id all resolve to `whisper-tiny`, a valid preference resolves to that
      entry, and `invalidate_stt_model_cache` actually clears a previously-populated cache slot
      (assert the next `resolve_local_adapter` call reloads rather than reusing the old `Arc`). Add
      a no-default-feature compile/test assertion that the command is still registered and is a
      successful no-op. (depends on T007)
- [x] T009 Register the four commands from T005 and `invalidate_stt_model_cache` from T007 in
      `src-tauri/src/lib.rs`'s `generate_handler!`. (depends on T005, T007)
- [x] T010 ~~Regenerate ts-rs bindings~~ — verified this doesn't apply: neither
      `catalog::CatalogEntry`/`TierRecommendation` (the LLM catalog this feature mirrors) nor the
      new `SttCatalogEntry`/`SttTierRecommendation` derive `ts_rs::TS` (`ts-rs` is only used for
      `HaexError` and `instances::`); the frontend hand-writes matching TS interfaces in
      `useCatalog.ts`/`useSttCatalog.ts` instead (T011), consistent with the existing pattern. Ran
      `cargo test --features llm-cpu export_bindings` to confirm: only the known unrelated
      trailing-whitespace diff in `src/types/bindings/InstanceInfo.ts` appeared, no new binding
      files — reverted that diff (`git checkout -- src/types/bindings/InstanceInfo.ts`).
      (depends on T005)
- [x] T011 [P] Refactor `src/composables/useCatalog.ts` and the `listInstalledAsync`/
      `downloadFromCatalogAsync` pair in `src/composables/useModels.ts` into small generic factory
      functions parameterized by Tauri command name (and TS entry type via generics), so a second
      catalog/model-list pair can be created without duplicating the implementation. Add
      `src/composables/useSttCatalog.ts` and `src/composables/useSttModels.ts` as thin exports of
      those factories wired to the T005 commands (`SttCatalogEntry`, `list_stt_catalog`,
      `stt_recommend_tiers`, `list_installed_stt_models`, `download_stt_model`). Existing call sites
      (`ModelChoiceStep.vue`, `DefaultModelSetting.vue`) must keep compiling and behaving unchanged.
      (depends on T010)

**Checkpoint**: Foundation ready — both user stories can now be implemented independently.

---

## Phase 3: User Story 1 - Pick an STT model during first-run onboarding (Priority: P1) 🎯 MVP

**Goal**: The onboarding wizard offers a hardware-fit-recommended STT model tier right after the
chat/agent model step, downloadable and selectable, with a "decide later" skip that preserves
today's default-download behavior.

**Independent Test**: Complete first-run onboarding on a fresh device/vault; confirm the STT tier
step appears after the model step, that choosing a tier downloads it and makes it active, and that
skipping still leaves dictation fully working via the built-in default.

- [x] T012 [P] [US1] Add `onboarding.sttModel.*` keys to `src/i18n/locales/en.json` and `de.json`,
      mirroring the existing `onboarding.model.*` keys one for one (title, description, tier
      labels reused from `onboarding.model.tier.*`/`onboarding.model.fit.*` since they're generic
      enough, downloading/downloadFailed/empty strings).
- [x] T013 [US1] Create `src/components/onboarding/SttModelChoiceStep.vue`, mirroring
      `src/components/onboarding/ModelChoiceStep.vue` structurally (tier chips from
      `useSttCatalog().recommendTiersAsync()`, `choose`/`skip`/`back` emits, downloading/error
      state), using the T012 i18n keys and the T011 composables. (depends on T011, T012)
- [x] T014 [US1] Wire the new step into `src/pages/onboarding/[instance].vue`: extend `step` to
      `'alias' | 'model' | 'sttModel'`; after the existing model step's `completeWithModel`/
      `completeWithoutModel` paths, transition to `'sttModel'` instead of navigating to the
      workspace directly; on STT choose, call `useSttModels().downloadFromCatalogAsync(id)` then
      `setPrefAsync` for `voice.stt_model_id`, then navigate to the workspace; on STT skip, navigate
      to the workspace exactly as today (no new preference written, so `resolve_local_adapter`'s
      T007 fallback applies).
      (depends on T013)
- [ ] T015 [US1] Manual verification: run [quickstart.md §1](quickstart.md#1-erstwahl-beim-onboarding-user-story-1)
      end to end — both the "choose a tier" path and the "decide later" skip path, on a fresh
      device/vault. (depends on T014)

**Checkpoint**: User Story 1 is fully functional and independently testable — STT model choice is
part of first-run onboarding, with no regression for a skipped/pre-feature device.

---

## Phase 4: User Story 2 - Change the active local STT model later (Priority: P2)

**Goal**: Settings shows the active local STT model and lets the user switch to any other tier
(installed or not), taking effect on the next recording without an app restart.

**Independent Test**: From Settings, switch the active model to a different tier (downloading it
if needed) and confirm the next dictation uses it without restarting the app.

- [x] T016 [P] [US2] Add `settings.sttModel.*` keys to `src/i18n/locales/en.json` and `de.json`,
      mirroring `settings.default.*` (device-scoped only — no vault-wide variant, per
      [data-model.md](data-model.md#neue-preference-voicestt_model_id)).
- [x] T017 [US2] Create `src/components/settings/SttModelSetting.vue`, mirroring
      `src/components/settings/DefaultModelSetting.vue`'s structure (current-value display, list of
      options, save button, busy/error/saved-flash states) but simplified to the single
      device-only scope: show the active model via `getPrefAsync` for `voice.stt_model_id` (displaying
      `whisper-tiny` when unset), list catalog entries via
      `useSttCatalog().listAsync()` marked installed/not-installed via
      `useSttModels().listInstalledAsync()`; switching calls `downloadFromCatalogAsync` (no-op if
      already installed) → `setPrefAsync(..., 'voice.stt_model_id', id)` →
      `invoke('invalidate_stt_model_cache')`. (depends on T007, T011, T016)
- [x] T018 [US2] Wire `SettingsSttModelSetting` into `src/pages/settings/[instance].vue`, next to
      the existing `SettingsDefaultModelSetting`. (depends on T017)
- [ ] T019 [US2] Manual verification: run [quickstart.md §2](quickstart.md#2-nachträglich-wechseln-user-story-2)
      end to end — switch to a not-yet-downloaded tier, then to an already-installed one (**time
      this second switch: SC-002 requires under two minutes end-to-end from opening Settings**),
      and confirm dictation picks up the change without an app restart. (depends on T018)

**Checkpoint**: Both user stories now independently functional.

---

## Final Phase: Polish & Cross-Cutting Concerns

- [ ] T020 [P] Run [quickstart.md §3](quickstart.md#3-rand--und-fehlerfälle) (edge cases) in full:
      interrupted/no-network onboarding download, manually-deleted active-model files before a
      dictation, switching the active model mid-transcription. Confirm the T013 download-error/skip
      affordance (mirrored from `ModelChoiceStep.vue`) already covers the onboarding failure case;
      add handling only if it doesn't.
- [x] T021 [P] Run `cargo test --features llm-cpu` (covers T002/T003/T004/T006/T008's new/updated
      test files), `pnpm typecheck`, and `pnpm lint:rust`; fix any fallout.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately.
- **Foundational (Phase 2)**: Depends on Setup (T001) — BLOCKS both user stories.
- **User Story 1 (Phase 3)**: Depends on Foundational completion. No dependency on US2.
- **User Story 2 (Phase 4)**: Depends on Foundational completion. No dependency on US1 (US1's
  onboarding step and US2's Settings section touch disjoint files after Phase 2).
- **Polish (Final Phase)**: Depends on both user stories being complete.

### Parallel Opportunities

- T002 and T003 touch different files but T003 depends on T002's output type — sequence them, but
  both are `[P]`-eligible against unrelated other tasks.
- T005/T006 (STT commands + their tests) and T007/T008 (voice.rs + its tests) can proceed in
  parallel with each other once T004 lands, since they touch disjoint files.
- Once Phase 2 (through T011) is done, **US1 (T012-T015) and US2 (T016-T019) can be implemented
  fully in parallel** — disjoint files (`SttModelChoiceStep.vue`/onboarding page vs.
  `SttModelSetting.vue`/settings page), both only reading from the T011 composables.
- T020 and T021 in the Polish phase are independent of each other.

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 (T001) and Phase 2 (T002-T011).
2. Complete Phase 3 (US1 — onboarding).
3. **STOP and VALIDATE**: run quickstart.md §1.
4. Settings-side switching (US2) can ship later without touching US1's code.

### Incremental Delivery

1. Setup + Foundational → shared catalog/storage/command infrastructure ready, nothing
   user-visible yet.
2. Add US1 → onboarding gets the new step → demoable MVP.
3. Add US2 → Settings gets the new section → full feature complete.
4. Polish → edge cases + full test/lint pass.

---

## Notes

- [P] tasks = different files, no dependency on an incomplete task.
- [Story] label maps a task to US1/US2 for traceability; Setup/Foundational/Polish tasks carry no
  story label by design (per spec-kit convention).
- Spec 008's still-unbuilt US3 (external transcription provider) is untouched by this feature and
  is not represented here — see [spec.md](spec.md#fr-009) / [research.md §1](research.md#1-scope-lokale-größen-tiers-statt-externer-anbieter).
