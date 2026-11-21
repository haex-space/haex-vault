---
description: 'Actionable, dependency-ordered task list for the onboarding-model-prefs feature'
---

# Tasks: Onboarding-Härtung und Modellwahl-Persistenz

**Input**: Design documents from `/specs/002-onboarding-model-prefs/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/tauri-commands.md
**Tests**: Backend tests are REQUIRED per research.md Entscheidung 10 (Unit + Integration in Rust). Frontend tests are manuell per quickstart.md (kein Playwright).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1..US5)

## Path Conventions

- Backend: `src-tauri/src/` and `src-tauri/tests/` (Rust, Tauri command handlers, storage wrappers, migrations)
- Frontend: `src/` (Nuxt 4 SPA — pages, components, composables, middleware)
- Docs: `specs/002-onboarding-model-prefs/` (this feature's design docs)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: New module scaffolding needed by later phases.

- [ ] T001 [P] Add `pub mod device;` to `src-tauri/src/lib.rs` and create empty `src-tauri/src/device/mod.rs` with `pub mod commands;` (module skeleton for `current_device_info`)
- [ ] T002 [P] Add `pub const VAULT_SCOPE_UUID: uuid::Uuid = uuid::Uuid::nil();` to `src-tauri/src/identity/mod.rs` (referenced by all subsequent tasks)

**Checkpoint**: Module skeletons and shared constant available. No behavior changes yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Database schema, storage wrappers, and bootstrap changes that every user story depends on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T003 Add Migration 0011 `preferences` (schema per [data-model.md](data-model.md)) in `src-tauri/src/identity/migrations.rs` — includes `CREATE INDEX idx_preferences_key`
- [ ] T004 Add Migration 0012 `DROP TABLE device_downloaded_models_no_sync` in `src-tauri/src/identity/migrations.rs`
- [ ] T005 Extend `HaexBootstrap::bootstrap` in `src-tauri/src/identity/bootstrap.rs` to `INSERT OR IGNORE` the sentinel row `(vault_device_uuid=nil, installation_uuid=nil, alias=NULL, first_seen=0)` BEFORE the existing installation-uuid lookup
- [ ] T006 [P] Create `src-tauri/src/storage/preferences.rs` with `PrefScope` enum, `PrefRow` struct, and typed `insert_or_update`, `get`, `delete`, `list_by_scope`, `list_by_key` functions; each write injects `haex_hlc_no_sync = current_hlc()`
- [ ] T007 [P] Create `src-tauri/src/storage/preferences_tests.rs` unit tests: roundtrip set→get→delete; scope isolation (device rows don't leak to another device's queries); NULL value = absent from `get`; key-format validation (must contain `.`)
- [ ] T008 Register `pub mod preferences;` and `#[cfg(test)] mod preferences_tests;` in `src-tauri/src/storage/mod.rs`
- [ ] T009 Delete `src-tauri/src/storage/device_downloaded_models.rs` (no longer needed once T004 is in effect; Filesystem-Scan replaces DB registry). **Depends on: T011, T012, T013, T014 completed — otherwise the crate does not compile because callers still reference the module.**
- [ ] T010 Remove `pub mod device_downloaded_models;` from `src-tauri/src/storage/mod.rs`. **Depends on: T011, T012, T013, T014 completed.**
- [ ] T011 Add the shared `models::paths::canonical_model_file(app, slug)` selector: enumerate the slug directory, ignore temporary/non-regular files, and return the lexicographically smallest final UTF-8 `.gguf`. Update `src-tauri/src/chat/commands.rs::load_local_model_by_id` to use this selector and stop reading a persisted `device_downloaded_models_no_sync.relative_path`.
- [ ] T012 Update `src-tauri/src/models/commands.rs::list_installed_models` to call the shared selector, join the slug with `models::get_model(&slug)`, and drop `sha256`, `verified_at` from the returned payload (keeps `id`, `name`, `providerId`, `contextWindow`, `relativePath`, `sizeBytes`). Add coverage with two complete `.gguf` files plus one temporary file and assert that discovery and loading select the same lexicographically smallest final file.
- [ ] T013 Update `src-tauri/src/models/commands.rs::register_downloaded` (called from `download_model_from_hf` and `import_model_from_file`) to only upsert into `models` — remove the `device_downloaded_models` write path
- [ ] T014 Update `src-tauri/src/models/commands.rs::delete_installed_model` to delete the canonical on-disk GGUF selected by the shared selector only; no DB row to delete (the `models`-row stays as catalog metadata)
- [ ] T015 Create `src-tauri/tests/preferences_roundtrip.rs` integration test: bootstrap-idempotent-sentinel (running bootstrap twice yields exactly one sentinel row); FK-cascade (deleting a `known_devices` row also deletes its preferences); sync-apply ordering (both child-before-parent and parent-before-child payloads are reordered and committed without orphan rows, while a missing parent is rejected)
- [ ] T016 Run `cargo test --lib storage::preferences_tests` and `cargo test --test preferences_roundtrip` — all green before proceeding

**Checkpoint**: Preferences infrastructure ready, sentinel bootstrap runs, `device_downloaded_models_no_sync` is gone, filesystem-scan works. User-story phases can begin.

---

## Phase 3: User Story 1 - Onboarding Wizard beim ersten Öffnen (Priority: P1) 🎯 MVP-Kernstory

**Goal**: Nutzer bekommt beim ersten Vault-Open auf einem Gerät einen geführten Wizard (Alias + optional Modell), landet danach auf einer Workspace-Landing mit FAB. Basis für alle nachfolgenden Stories.

**Independent Test**: Genesis- und Adoption-Szenarien aus [quickstart.md](quickstart.md) §1-3 durchlaufen. Kein Zugriff auf Chat-Ansicht ohne gesetzten `alias`.

### Backend for US1

- [ ] T017 [P] [US1] Create `src-tauri/src/hardware/hostname.rs` with `pub fn suggested_alias() -> Option<String>` returning `sysinfo::System::host_name()` (raw Option — Frontend übernimmt Fallback via i18n key `onboarding.alias.defaultPlaceholder`)
- [ ] T018 [US1] Register `pub mod hostname;` in `src-tauri/src/hardware/mod.rs`
- [ ] T019 [US1] Implement `current_device_info` Tauri command in `src-tauri/src/device/commands.rs` per [contracts/tauri-commands.md](contracts/tauri-commands.md); joins active-vault `known_devices` row (via installation-id file lookup) with `hostname::suggested_alias()`
- [ ] T020 [US1] Implement `update_device_alias` Tauri command in `src-tauri/src/device/commands.rs`; validates non-empty; calls existing `storage::known_devices::update_alias`
- [ ] T021 [US1] Register both commands in `src-tauri/src/lib.rs` `invoke_handler!`
- [ ] T022 [P] [US1] Add `TierRecommendation` and `Tier` types to `src-tauri/src/catalog/mod.rs`
- [ ] T023 [US1] Implement `pub fn recommend_tiers(&HardwareInfo) -> [TierRecommendation; 3]` in `src-tauri/src/catalog/mod.rs` per research.md Entscheidung 10 (Easy = kleinster Fits, Sweet = größter Fits, Max = größter Fits/Tight) with the documented fallbacks
- [ ] T024 [US1] Add `catalog_recommend_tiers` Tauri command in `src-tauri/src/catalog/commands.rs` (create file if needed) that calls `hardware::detect()` + `recommend_tiers`; register in `lib.rs`
- [ ] T025 [P] [US1] Add unit test in `src-tauri/src/catalog/catalog_tests.rs` (create file if needed) covering the tier-selection algorithm against synthetic `HardwareInfo` inputs (small VRAM, medium, large, none-detected)

### Frontend composables for US1

- [ ] T026 [P] [US1] Create `src/composables/useDevice.ts` with typed `currentDeviceInfoAsync()` and `updateDeviceAliasAsync(alias)` wrappers
- [ ] T027 [P] [US1] Extend `src/composables/useCatalog.ts` with `recommendTiersAsync(): Promise<TierRecommendation[]>`

### Onboarding route + wizard components for US1

- [ ] T028 [US1] Create `src/pages/onboarding/[instance].vue` — Wizard root that loads `currentDeviceInfoAsync()` on mount and renders sequential Alias step + Model step
- [ ] T029 [P] [US1] Create `src/components/onboarding/AliasStep.vue` — Alias input prefilled mit `deviceInfo.hostname` (wenn null: `$t('onboarding.alias.defaultPlaceholder')`), required, validates non-whitespace on submit. Alle sichtbaren Labels/Buttons via `$t()`.
- [ ] T030 [P] [US1] Create `src/components/onboarding/ModelChoiceStep.vue` — Renders three tier-chips (Easy/Sweet/Max) mit fit-badge; alle Labels (`$t('onboarding.model.tier.easy')`, `.sweet`, `.max`, `.laterViaProvider`) via i18n; Klick auf Chip triggert `useModels().downloadFromCatalogAsync`; "später via Anbieter"-Button überspringt Modell-Download ohne Preference-Write
- [ ] T031 [US1] Wire wizard-completion: keep the alias local through the first step; after the model is downloaded and its device default is set, or after the explicit "später via Anbieter" choice, call `updateDeviceAliasAsync(alias)` as the final completion write and then `navigateTo('/workspace/[instance]')`. An exit between steps leaves `alias` NULL so the route guard reopens the wizard. Alle nutzer-sichtbaren Bestätigungs-/Fehlermeldungen via `$t()`.

### Workspace landing + FAB for US1

- [ ] T032 [US1] Create `src/pages/workspace/[instance].vue` — minimal landing: shows active instance name as heading, header mit settings-icon (i18n-`title`-Attribut) linking to `/settings/[instance]`, mounts the FAB component; alle sichtbaren Labels via `$t()`
- [ ] T033 [P] [US1] Create `src/components/workspace/ChatFab.vue` — fixed-position bottom-right rounded button linking to `/chat/[instance]`
- [ ] T034 [US1] Update `src/pages/index.vue` unlock-flow: after successful `open_instance`, `navigateTo('/workspace/[name]')` instead of `/chat/[name]`

### Route guarding for US1

- [ ] T035 [US1] Create `src/middleware/onboarded.ts` — Nuxt named middleware: calls `currentDeviceInfoAsync()`; if `alias === null`, `return navigateTo('/onboarding/${instance}')`; skipped on onboarding route itself. Because the alias is persisted only at final wizard completion, this remains a complete onboarding marker.
- [ ] T036 [US1] Attach the `onboarded` middleware to `/workspace/[instance].vue`, `/settings/[instance].vue`, and `/chat/[instance].vue`

### Verification for US1

- [ ] T037 [US1] Manually verify quickstart.md §1 (Genesis without model), §2 (Genesis with model), §3 (Adoption on new device). Confirm alias placeholder shows OS-hostname; wizard cannot be exited without alias; workspace-landing shows FAB.

**Checkpoint**: User Story 1 works end-to-end. First-open on any device shows the wizard; wizard-completion lands on workspace with FAB reachable to chat. `preferences.chat.default_model_id` optionally written.

---

## Phase 4: User Story 2 - Modellwahl bleibt zwischen Sessions (Priority: P1)

**Goal**: Nach App-Neustart lädt automatisch das zuletzt tatsächlich genutzte Modell.

**Independent Test**: Quickstart §4 (Session-Persistenz), §5 (Ausprobier-Wechsel).

### Preference commands for US2

- [ ] T038 [P] [US2] Implement `get_pref(scope, key) -> Option<String>` Tauri command in `src-tauri/src/storage/preferences_commands.rs` (create file); validates key format, calls `storage::preferences::get`; register in `lib.rs`
- [ ] T039 [P] [US2] Implement `set_pref(scope, key, value) -> ()` Tauri command in same file; validates, calls `storage::preferences::insert_or_update`
- [ ] T040 [P] [US2] Implement `clear_pref(scope, key) -> ()` Tauri command in same file; idempotent
- [ ] T041 [P] [US2] Add contract-shape tests in `src-tauri/src/storage/preferences_commands_tests.rs` covering key-validation errors and scope-parsing

### Session resolver for US2

- [ ] T042 [US2] Add `resolve_default_model` helper function in `src-tauri/src/chat/commands.rs` implementing the FR-014 chain per [contracts/tauri-commands.md](contracts/tauri-commands.md); returns `{ modelId: Option<String>, source: ResolveSource }`; pure read
- [ ] T043 [US2] Add `resolve_default_model` Tauri command wrapper (thin); register in `lib.rs`
- [ ] T044 [US2] `load_model` in `src-tauri/src/chat/commands.rs` schreibt WEDER bei `auto` NOCH bei `manual` in `chat.last_active_model_id`. Der `mode`-Parameter wird gestrichen — Frontend braucht keine Unterscheidung mehr für diesen Zweck. (Post-Analyze-Korrektur FR-009: nur `send_message` schreibt last_active.)
- [x] T045 [US2] Extend `send_message` in `src-tauri/src/chat/commands.rs`: require a stable non-empty `idempotencyKey`, persist it uniquely with the user message, and derive the user/assistant IDs deterministically from the key (UUID v5) so a retry of the frontend's own `invoke()` call returns the same `SendMessageResult` without a second insert or a second `stream_chat`; a retry with a different thread or content for the same key is `HaexError::InvalidInput`. Deliberately does NOT resume/restart a generation that failed after acceptance — that remains "send a new message with a new key" (simplified from an earlier contract draft, see contracts/tauri-commands.md §send_message). `preferences[('<my_device>', 'chat.last_active_model_id')] = session.model_id` still writes only after a fresh (non-duplicate) `stream_chat` start succeeds, unchanged from before this task.
- [ ] T046 [US2] Add integration test in `src-tauri/tests/preferences_roundtrip.rs` covering the resolver chain: seed a loadable `chat.last_active_model_id` together with device and vault `chat.default_model_id` values, call resolve, verify `last_active` wins before device default, vault default, and first-available; retain separate cases for each lower-priority fallback

### Loading UX for US2

- [ ] T047 [P] [US2] Add `model-load-progress` Tauri event emission in `src-tauri/src/chat/commands.rs::load_model`: emit structured payload `{ modelId, modelName, phase, providerName? }` per [contracts/tauri-commands.md](contracts/tauri-commands.md) — NO localized label from backend; frontend übersetzt via i18n; verwende `~/.nv/ComputeCache/`-heuristic für cold-vs-warm-Erkennung (fall back to time-based if not readable). Auf non-CUDA-Builds immer `loading` statt `cuda-jit-warmup`.
- [ ] T048 [P] [US2] Create `src/composables/usePreferences.ts` with `getPrefAsync`, `setPrefAsync`, `clearPrefAsync`, `resolveDefaultModelAsync` wrappers
- [ ] T049 [P] [US2] Extend `src/composables/useChat.ts` with `onModelLoadProgress(handler)` listener; add `ModelLoadProgressEvent` type
- [ ] T050 [US2] Update `src/pages/chat/[instance].vue` on-mount: `resolveDefaultModelAsync()` aufrufen; wenn `modelId !== null`, `loadModelAsync(modelId)` und Ladepanel anzeigen — Panel-Label kommt aus `$t('chat.loading.<phase>', { modelName, providerName? })` basierend auf empfangenen `model-load-progress`-Events; Chat-Input deaktiviert bis `phase === 'ready'`
- [ ] T051 [US2] `src/pages/chat/[instance].vue` sidebar picker: manueller Modellwechsel ruft weiterhin `loadModelAsync(modelId)` (kein `mode`-Parameter mehr, siehe T044) — schreibt NICHT last_active; erst der nächste erfolgreiche `send_message` schreibt es. Ausprobier-Klicks im Dropdown haben keinen Persistenz-Effekt.

### Verification for US2

- [ ] T052 [US2] Manually verify quickstart.md §4 (Session-Persistenz) and §5 (Ausprobier-Wechsel). Confirm: closing app after `send_message` restores that model; closing app after passive auto-select does NOT overwrite existing `last_active`.

**Checkpoint**: User Story 2 works. Preferences persist; resolver returns the right model at start; loading UX shows contextual labels.

---

## Phase 5: User Story 3 - Expliziter Standard mit Scope (Priority: P2)

**Goal**: Nutzer kann via Settings-Screen ein Modell als Standard für "dieses Gerät" oder "vault-weit" setzen.

**Independent Test**: Quickstart §6 (explizit "Als Standard setzen").

### Settings route for US3

- [ ] T053 [US3] Create `src/pages/settings/[instance].vue` — Settings-Screen root; loads `currentDeviceInfoAsync()`, mounts `AliasSetting` and `DefaultModelSetting` components; header zeigt `$t('settings.header.forDevice', { alias })`; onboarded middleware attached
- [ ] T054 [P] [US3] Create `src/components/settings/DefaultModelSetting.vue` — model selector (all installed + api_key models via existing composables) + scope radio (`$t('settings.default.scope.device')` / `.vault`) + Save button; on save calls `usePreferences().setPrefAsync({ scope: { kind: 'device'|'vault', uuid: ... }, key: 'chat.default_model_id', value: modelId })`; alle Labels via `$t()`
- [ ] T055 [P] [US3] Add "settings"-Icon link in the workspace-landing header (from T032) that navigates to `/settings/[instance]`, mit i18n-`title` (`$t('workspace.settings.iconTitle')`)

### Verification for US3

- [ ] T056 [US3] Manually verify quickstart.md §6. Confirm: setting a device-scoped default persists; on app-restart WITHOUT a last_active, the device-default is loaded; setting a vault-scoped default is visible on other devices (via sync) as fallback.

**Checkpoint**: User Story 3 works. Settings-Screen exists with two functional controls (alias-rename via US4 + default-model-set via US3).

---

## Phase 6: User Story 4 - Gerätename im Settings-Kontext (Priority: P3)

**Goal**: Nutzer sieht im Settings-Screen unmittelbar welches Gerät er konfiguriert; kann den Alias rename-en.

**Independent Test**: Öffnet Settings, prüft Titel enthält Gerätename; ändert Alias, sieht sofortigen Effekt.

### Alias rename UI for US4

- [ ] T057 [US4] Create `src/components/settings/AliasSetting.vue` — Input mit aktuellem Alias vorbefüllt, Save-Button; ruft `useDevice().updateDeviceAliasAsync(newAlias)`; validiert non-empty; alle Labels und Fehlermeldungen via `$t()`
- [ ] T058 [US4] Mount `AliasSetting` in `src/pages/settings/[instance].vue` (from T053); wire re-fetch of `currentDeviceInfoAsync()` after save so header updates immediately

### Verification for US4

- [ ] T059 [US4] Manually verify: Settings-Titel zeigt Alias; Rename in AliasSetting aktualisiert Titel ohne Neuladen; nach Reload persistent.

**Checkpoint**: US4 komplett. Settings-Screen ist self-descriptive: Nutzer weiß welches Gerät er bearbeitet.

---

## Phase 7: User Story 5 - Bereitschaft für spätere Gerätehygiene (Priority: P3)

**Goal**: Nachweis dass die FK-Cascade auf `preferences.vault_device_uuid → known_devices` funktioniert, ohne dass ein Retire-UI existiert.

**Independent Test**: Schema-/Storage-Test in `preferences_roundtrip.rs` (bereits in Phase 2 T015 angelegt für Bootstrap-Idempotenz + FK-Cascade). Kein zusätzlicher Code nötig.

### Verification for US5

- [ ] T060 [US5] Confirm the FK-Cascade test from T015 covers: insert 2 preferences under device A, delete `known_devices` row for A, verify 0 preferences left with `vault_device_uuid == A`. Add explicit test case if T015 doesn't already cover it.

**Checkpoint**: US5 as a forward-looking contract is verified. When a Retire-UI later exists, it will just DELETE from `known_devices` and everything cascades correctly without touching preferences code.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, cleanup, verification.

- [ ] T060a [P] i18n-Locale-Einträge in `src/i18n/de/onboarding.json`, `src/i18n/de/settings.json`, `src/i18n/de/workspace.json`, `src/i18n/de/chat.json` (bzw. bestehende Locale-Struktur) hinzufügen und in `src/i18n/en/` denselben Satz auf Englisch spiegeln. Keys mindestens: `onboarding.alias.label`, `onboarding.alias.defaultPlaceholder`, `onboarding.model.tier.{easy,sweet,max,laterViaProvider}`, `onboarding.wizard.{next,finish,cancel}`, `settings.header.forDevice`, `settings.default.scope.{device,vault}`, `settings.default.save`, `settings.alias.{label,save}`, `workspace.settings.iconTitle`, `chat.loading.{connecting,loading,cudaJitWarmup,ready}`. Grep über alle neuen `.vue`-Dateien: KEINE hardcoded deutschen/englischen Strings.
- [ ] T061 [P] Update the repository-owned `plans/001-desktop-mvp.md` "Onboarding-Härtung"-Abschnitt to mark Feature 002 as landed (Feature 002 abgeschlossen am YYYY-MM-DD) and summarize the migration numbers, DROP of `device_downloaded_models_no_sync`, sentinel-row convention, and new command surface
- [ ] T063 Run `cargo fmt --check` and fix any style violations
- [ ] T064 Run `cargo clippy --lib --tests -- -D warnings` — no warnings
- [ ] T065 Run `cargo test` (full suite: lib + all integration tests) — all green
- [ ] T066 Run `pnpm typecheck` — exit 0
- [ ] T067 Run `cargo test --no-default-features --lib` — verify feature still builds without `llm-cpu`
- [ ] T068 Manual smoke test: `pnpm tauri:dev:cuda`, walk through Genesis + Adoption + Session-Persistenz per quickstart.md
- [ ] T069 Commit + open PR per conventional commits + workflow

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 — **BLOCKS all user stories**
- **Phase 3 (US1)**: Depends on Phase 2 complete
- **Phase 4 (US2)**: Depends on Phase 2 complete; can run in parallel with Phase 3 if two implementers work concurrently
- **Phase 5 (US3)**: Depends on Phase 4 (needs `usePreferences` composable + `set_pref` command); also touches settings-route created in Phase 6 — recommended sequence is Phase 3 → Phase 4 → Phase 6 → Phase 5
- **Phase 6 (US4)**: Depends on Phase 3 (needs `useDevice.updateDeviceAliasAsync` and settings route stub); logically small; can be interleaved right after Phase 3
- **Phase 7 (US5)**: Depends only on Phase 2 (test-only; the FK-cascade test can be written as soon as migrations exist)
- **Phase 8 (Polish)**: Depends on all previous phases

### Within Phase 2

- T003, T004 sequentially (both edit `identity/migrations.rs`)
- T005 depends on T002 (uses `VAULT_SCOPE_UUID`)
- T006 [P] and T007 [P] can proceed in parallel (different files)
- T008 depends on T006, T007
- T011, T012, T013, T014 sequentially (all edit `models/commands.rs` and `chat/commands.rs`) — MUST complete before T009/T010, otherwise crate does not compile
- T010, T009 sequentially AFTER T011-T014 (remove mod-decl, then delete file)
- T015 depends on T003, T005, T006
- T016 is the gate — run before starting any user-story phase

### Within Phase 3 (US1)

- T017, T018 sequentially (module + mod-decl)
- T019 depends on T018 (uses hostname helper)
- T020 depends on `storage::known_devices::update_alias` existence (already in main)
- T021 depends on T019, T020
- T022 [P], T023, T024, T025 sequentially for catalog (all edit `catalog/`)
- T026, T027 [P] parallel — different composable files
- T028 depends on T026, T027, T024
- T029, T030 [P] parallel — different component files
- T031 depends on T028, T029, T030
- T032 depends on T033 (mounts FAB); T033 [P] can be built in parallel with T032 skeleton
- T034 depends on T032 existing as route
- T035 depends on T026 (uses `useDevice`)
- T036 depends on T032, T035 (wires middleware to routes)
- T037 is the story-gate

### Within Phase 4 (US2)

- T038, T039, T040 [P] parallel — same file but different pub fns; if serialised, sequential
- T041 depends on T038, T039, T040
- T042 depends on T038, T039 (uses get_pref internally? actually resolve_default_model is pure Rust — see [contracts/tauri-commands.md](contracts/tauri-commands.md); calls `storage::preferences::get` directly)
- T043 depends on T042
- T044, T045 depend on T042 (both write `last_active_model_id` via storage layer)
- T046 depends on T042
- T047 [P] parallel with T044, T045 (different concern)
- T048 [P], T049 [P] parallel with each other
- T050 depends on T048, T049, T047
- T051 depends on T050
- T052 is the story-gate

### Parallel Opportunities

- **Phase 2**: T006 || T007 (both new files); once done, T008 sequences them into `mod.rs`
- **Phase 3**: within backend, T017/T018 in one thread + T022/T023/T024 in another; within frontend, T026 || T027 || T029 || T030 || T033
- **Phase 4**: T038 || T039 || T040 (if editor supports parallel edits to different fns in same file); T047 || T048 || T049 as separate concerns
- **Phase 5**: T054 || T055 different files

### MVP Scope

**Minimal Marketable Product = Phase 1 + Phase 2 + Phase 3 (US1)**. Delivers: der Wizard-Loop schließt sich für Genesis und Adoption, Workspace-Landing mit FAB steht. Modell-Wahl noch nicht persistiert zwischen Sessions — Nutzer wählt jedes Mal neu im Sidebar. Nicht ideal, aber funktional und testbar. Empfehlung: Phase 3 + Phase 4 zusammen liefern (P1 = beide P1-Stories), damit Session-Persistenz von Anfang an dabei ist.

**Full P1 = Phase 1 + 2 + 3 + 4**. Delivers: Wizard + Persistenz. Diese Grenze ist der natürliche PR-Schnitt.

**Full Feature = Phase 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8**.

---

## Parallel Example: within Phase 2

```bash
# After T001-T005 done, parallel:
Task T006: "Create src-tauri/src/storage/preferences.rs (storage wrapper)"
Task T007: "Create src-tauri/src/storage/preferences_tests.rs (unit tests)"

# Then sequentially:
Task T008: "Register mod in storage/mod.rs"

# Then parallel again:
Task T009+T010 (one file at a time — sequential)
```

## Parallel Example: within Phase 3 backend

```bash
# After Phase 2 checkpoint:
Task T017: "Create src-tauri/src/hardware/hostname.rs"
Task T022: "Add TierRecommendation types to catalog/mod.rs"
Task T026: "Create src/composables/useDevice.ts"
Task T027: "Extend src/composables/useCatalog.ts"
```

---

## Implementation Strategy

### Recommended sequence

1. **Phase 1** (Setup, ~10 min) — module skeletons.
2. **Phase 2** (Foundational, ~2-4 h) — migrations, storage, bootstrap sentinel, filesystem-scan for models. Run T016 gate.
3. **Phase 3** (US1 Wizard, ~4-8 h) — biggest slice, includes new Nuxt routes and components. Manuell verifiziert am Ende (T037).
4. **Phase 4** (US2 Persistenz, ~2-4 h) — preferences commands + resolver + loading UX + write triggers.
5. **Phase 5+6** interleaved (~2-3 h) — settings-Screen mit beiden Controls in einem Schritt (aufgeteilt als US3/US4 in tasks für Priority-Klarheit).
6. **Phase 7** (US5, ~15 min) — nur ein Test-Case-Ausbau von T015.
7. **Phase 8** (Polish, ~1 h) — fmt, clippy, typecheck, full suite, manual smoke, memory update, PR.

Gesamt-Aufwand grob: 12-20 fokussierte Stunden, ≈1-2 Sessions.

### Was diese Tasks NICHT liefern (aus Non-Goals)

- Voller Workspace-Ausbau (Widgets, Panels, Dashboard-Elemente)
- Vollständiger Settings-Screen (weitere Einstellungen über die zwei Controls hinaus)
- Config-Overlay-Kommando-Palette
- OpenAI/Google/Groq-Adapter
- Tool-Calling-Loop / MCP-Client
- Retire-Vorgang (UI + Backend-Command "Gerät aus Vault entfernen")
- Cross-device-Attribution auf Chat-Messages
