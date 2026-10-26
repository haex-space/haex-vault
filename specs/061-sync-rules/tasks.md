---
description: 'Task list for spec 061-sync-rules'
---

# Tasks: Sync-Regeln

**Input**: Design documents from `/specs/061-sync-rules/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md),
[contracts/ignore-file.md](./contracts/ignore-file.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic; SC-003 (no data
loss), SC-004 (empty place never deletes), SC-005 (no half file) and SC-009 (excluded files untouched)
need dedicated test catalogs. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. The plan logic is pure and tested without
backends; the engine is tested against an in-memory `Place` and a real temp folder. No network services and no
`sleep` in `cargo test`. Build only the affected targets with `-j 4`; ask the operator before a full
workspace build (OOM risk on the 31 GB machine). Timing tests in `contacts::phone::acceptance_tests` are
flaky under load; not related.

**Organization**: Grouped by user story in priority order (P1: US1, US2; P2: US3, US4; P3: US5).

**Shipping note**: PR A = Phase 1 + Phase 2 (foundation, no visible change); PR B = US1; PR C = US2;
PR D = US3; PR E = US4; PR F = US5 + Polish. Each PR is reviewed and merged before the next one starts on top
of `main`. Work in `.worktrees/061-sync-rules` (other sessions switch branches in the main checkout). The
documentation PR #580 (design, spec, plan, tasks) lands first.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup (PR A)

- [ ] T001 Spike: behavior of the conflict choice `Replace` of `files::transfer` (research R2, risk 1). In a new `src-tauri/src/files/transfer/replace_tests.rs` pin what `local::run` and `remote::run_remote` do for an existing target (moved aside, left over, atomic?), for an aborted copy, and for a changed peer version. Write the result as a short note in research.md R2. If `Replace` leaves leftovers, T014 gets a "replace without moving aside" mode.
- [ ] T002 [P] Check the highest used migration number on `main` (`src-tauri/src/identity/migrations.rs`, specs 059/060 and open PRs); settle the number of `0044_sync_rules` (research R7, risk 3) and write it into data-model.md and plan.md if it differs.
- [ ] T003 [P] Settle how an encrypted folder is addressed as a place (research R1, risk 0): read `src-tauri/src/files/encrypted/route.rs`, `source.rs` and `browser_commands/mod.rs` (`storage_files`, `window_storage`); write the place form (storage id plus path, or `device:<root>`) into data-model.md (`place_*_source`) and research.md.
- [ ] T004 [P] Before the first new name, run bounded `graphify query … --budget 1200` for "device-local table no_sync", "gitignore matcher", "folder watcher debounce", "storage place list recursive"; note candidates to reuse in research.md.
- [ ] T005 [P] ADR `docs/adr/0018-sync-rules-state-stays-on-the-device.md`: rules, per-file state and run log live in `*_no_sync` tables keyed by `vault_device_uuid`; why this deviates from ADR 0001 (state per file would sync to every device; spec FR-003, FR-045); copy-safety through the device column. Status proposed, date 2026-10-26.
- [ ] T006 [P] Glossary: add **Sync-Regel / Ort / Abgleich / Stand / Plan / Konfliktkopie / Löschmodus / Ausnahmedatei / Lauf-Protokoll** to `CONTEXT.md` (new section "Sync-Regeln"); remove the "Sync-Regeln (ersetzen 025)" wording where it only points to the draft.
- [ ] T007 Add the crate: `cargo add ignore@0.4 --no-default-features` in `src-tauri/Cargo.toml` (only `gitignore` is used; verify the resolved version and that `aarch64-linux-android` still builds); commit `Cargo.lock`.
- [ ] T008 Prepare the worktree for code: `pnpm install --frozen-lockfile`; check `df -h`; reflink `src-tauri/target` from a merged worktree with the same `Cargo.lock` if one exists (memory: old worktree targets fill the disk).

---

## Phase 2: Foundational (PR A, no visible change)

**Purpose**: Tables, model types, the `Place` layer, the exclusion matcher and the pure plan types. Nothing is
offered in the UI yet; every later story builds on these.

### Data

- [ ] T009 Migration `0044_sync_rules` (number per T002) in a new `src-tauri/src/identity/migrations_sync_rules.rs` with exactly the five tables of data-model.md, all `_no_sync`, each with `vault_device_uuid` FK to `known_devices(vault_device_uuid) ON DELETE CASCADE`; register in `src-tauri/src/identity/migrations.rs` (`holzi_migration_source`). No `HOLZI_TRIGGER_VERSION` change; check whether the legacy-name helpers (`holzi_migration_source_with_legacy_*`) need an entry.
- [ ] T010 [P] Migration test in `src-tauri/src/identity/migrations_sync_rules_tests.rs` following `migrations_file_shares_tests.rs`: tables exist, no CRDT triggers are created for them, deleting a rule cascades to state, pending, runs and run items, deleting the device cascades to all.
- [ ] T011 Model in a new `src-tauri/src/file_sync/model.rs` (+ `mod.rs` with the module and `#[cfg(test)]` test modules): `Rule`, `RuleDraft`, `Place { source, path }`, `Direction`, `DeleteMode`, `RuleStatus`, ts-rs types with the prefix `SyncRule` and `#[ts(export, export_to = "../../src/types/bindings/")]`, `u64`/`i64` as `number`; check `ls src/types/bindings/SyncRule*.ts` first (name collision rule).
- [ ] T012 Rule store in a new `src-tauri/src/file_sync/store.rs`: create, update, delete, list, set enabled, last result; state rows (`get_all(rule_id)`, `put(rule_id, path, row)`, `rename`, `clear`), pending rows, all with the device uuid. Tests in `src-tauri/src/file_sync/store_tests.rs`: round trip, one row per `(rule_id, path)`, `clear` on changed places, rows of another device are invisible.
- [ ] T013 [P] Rule validation in a new `src-tauri/src/file_sync/rules.rs` (`validate`): places differ, no overlap with another rule of this device (same source and one path inside the other, also crosswise A/B), no `share:` source, no holzi-owned area (`OwnPlaces`), interval 60..86 400, name 1..80 chars, `bToA` normalised to `aToB`; error type `SyncRuleError { code, message }` (research R9). Tests in `rules_tests.rs`: every code, overlap crosswise, a rule on another device does not count (FR-004).

### Places

- [ ] T014 `Place` trait and facts in a new `src-tauri/src/file_sync/place.rs`: `list_tree`, `stat`, `hash`, `rename`, `delete_permanent`, `trash`, `read_text`, `write_text`, `create_folder`, plus the `Facts` type (path relative, size, mtime, kind, symlink, no_access). Copy is not part of it; add `copy_file(from: Side, path, to: Side, target, hooks)` as the narrow entry in `src-tauri/src/files/transfer/mod.rs` that wraps `local::run` / `run_remote`, returns the measured target size and time, and answers conflicts with `Replace` (mode per T001).
- [ ] T015 [P] `DevicePlace` in a new `src-tauri/src/file_sync/place_device.rs`: `walkdir` without following links, symlinks listed as skipped entries, `trash::delete` on the desktop (`#[cfg(not(any(target_os = "android", target_os = "ios")))]`), `.holzi-trash` otherwise, streaming SHA-256 with `sha2`, `write_text` through a temp file and rename. Tests in `place_device_tests.rs` with a temp dir: tree with links and unreadable file, trash fallback, hash equals known vector, `write_text` leaves no temp file.
- [ ] T016 [P] `StoragePlace` in a new `src-tauri/src/file_sync/place_storage.rs` over `StorageFiles` (storages, peer devices, encrypted folders per T003): recursive listing from `list` levels, `hash` streamed through `read_start` or the sidecar `sha256` for encrypted folders, `rename` through `rename`/`move_object`, `delete` (always permanent) and `trash` through a move into `.holzi-trash`. Tests in `place_storage_tests.rs` against the local-store test support in `src-tauri/src/remote_storage/test_support.rs` and `files/test_support.rs`.
- [ ] T017 [P] In-memory `Place` for tests in `src-tauri/src/file_sync/test_support.rs` (`MemPlace`: files with size, mtime, content; counters for calls; injectable failures; a switch for "lists empty"); used by plan, execute and run tests.

### Exclusions and plan types

- [ ] T018 Exclusion matcher in a new `src-tauri/src/file_sync/ignore.rs`: `Matcher::new(a_text, b_text)` builds two `GitignoreBuilder`s, adds the fixed exclusions, evaluates `matched_path_or_any_parents`, never excludes `.haex-ignore`, collects line errors with line numbers, caps 64 KiB / 2 000 lines (contracts/ignore-file.md). Tests in `ignore_tests.rs`: negation, directory-only, anchored, nested parent, union of both sides, fixed exclusions (`.holzi-trash/`, `.*.holzi-part-*`, `.DS_Store`), bad line skipped and reported, size cap.
- [ ] T019 Plan types in a new `src-tauri/src/file_sync/plan.rs`: `Step` (`Copy`, `Move`, `Delete`, `Conflict`, `Adopt`, `Skip`, `Report`), `Plan { steps, counts, hash_reads, ignored }`, `build` signature `build(rule, left: &Facts, right: &Facts, state: &State, ignore: &Matcher, hashes: &dyn Fn(Side, &str) -> Option<Hash>) -> Plan` with the step order of research R3 (folders, moves, copies and conflict copies, deletions last); `todo!()`-free skeleton that returns an empty plan, so US1 to US3 fill the rows.

**Checkpoint**: `cargo test -j 4 --lib file_sync::` and `identity::migrations_sync_rules` green; nothing visible yet.

---

## Phase 3: User Story 1 — One-way backup (Priority: P1) 🎯 MVP (PR B)

**Goal**: A one-way rule between two places copies new and changed files, keeps deleted ones (mode keep), reports
changes at the target, and survives an abort. Backend runs on a schedule while holzi is active; a minimal
rules list in the file browser can create, run and delete a rule.

**Independent Test**: Quickstart §1 with two local folders.

### Tests (write first)

- [ ] T020 [P] [US1] Plan tests (one-way rows) in `src-tauri/src/file_sync/plan_tests.rs`: new → copy, changed → copy, unchanged → nothing, target changed → `Report` and no overwrite (FR-010), deleted at source with `keep` → nothing and `kept` stays, move on the source side found by size and time (FR-009), ignored paths absent from the plan.
- [ ] T021 [P] [US1] Engine tests in `src-tauri/src/file_sync/execute_tests.rs` and `run_tests.rs` with `MemPlace` and a temp folder: first run copies everything, second run copies nothing, abort after the third of five copies leaves state only for the finished ones and no half file (SC-005, FR-014, FR-015), the target's measured time is stored (not the source's), a failing file does not stop the run and is retried next run (FR-023), symlinks are skipped and listed (FR-018).

### Implementation

- [ ] T022 [US1] Plan rows for one-way in `plan.rs` (`build`): scan against state, moves by size and time with a hash only for ambiguous pairs (FR-009), `Report` for target deviations, `kept` handling. Pure; keep every decision in this function (one implementation per rule).
- [ ] T023 [US1] Scan in a new `src-tauri/src/file_sync/scan.rs`: list both sides in parallel, read both `.haex-ignore` through `read_text`, build `Matcher`, return `Facts` and ignore errors; an empty listing or a missing root returns `PlaceError::Unreachable` (FR-022, used fully in US3).
- [ ] T024 [US1] Executor in a new `src-tauri/src/file_sync/execute.rs`: run steps in plan order, `Copy` through `copy_file` into a temp name and rename (FR-014), measure the target afterwards (`stat`), write the state row in one transaction after each step (FR-015), `Move` through `rename`, collect failures per file without stopping (FR-023); honour the cancel token between steps and pass it into `copy_file`.
- [ ] T025 [US1] One run in a new `src-tauri/src/file_sync/run.rs`: scan → plan → execute → write result to the rule row; return `Outcome { done, partial, waiting, error, cancelled }` with counters. Tests in `run_tests.rs` (T021).
- [ ] T026 [US1] Service and scheduling in a new `src-tauri/src/file_sync/schedule.rs` and `mod.rs` (`FileSyncState`, `start_for_active_instance`): one control task per vault session with a timer per active rule (interval, 60 min safety net for local places), "due" marker when a run is active, at most 2 runs at once, every active rule due once at start (catch up), cancel on `gate.token()` (research R8). Register `crate::file_sync::start_for_active_instance(&app, &state)` in `src-tauri/src/instances/open.rs` **and** `src-tauri/src/instances/import.rs` (memory: both starts), and manage `FileSyncState` in `lib.rs`.
- [ ] T027 [P] [US1] Folder watcher in a new `src-tauri/src/file_sync/watch.rs` over `files::local::watch::watch_folder` (recursive, own 2 s quiet period, drops with the task); wired into `schedule.rs` for rules with a local place; not compiled on iOS. Tests in `watch_tests.rs` with a temp dir and a manual trigger (no sleep: use the debouncer callback directly).
- [ ] T028 [US1] Status in a new `src-tauri/src/file_sync/status.rs` (derive `RuleStatus` from last outcome and control state, research R9) and the event `sync-rules-changed` / `sync-rules-progress` (contracts/tauri-commands.md); tests in `status_tests.rs` for each state.
- [ ] T029 [US1] Commands in a new `src-tauri/src/file_sync/commands.rs`: `sync_rules_list`, `sync_rules_create`, `sync_rules_update`, `sync_rules_delete`, `sync_rules_set_enabled`, `sync_rules_run_now`, `sync_rules_cancel`; register in the single `generate_handler![…]` in `src-tauri/src/lib.rs` behind `gate.wrap(...)`. Update drops state when places or direction change; delete leaves files untouched (FR-006). Tests in `commands_tests.rs` for create/validate/update/delete.
- [ ] T030 [P] [US1] Composable and store: `src/composables/useSyncRules.ts` (all `invoke('sync_rules_*')`) and `src/stores/syncRules.ts` after `src/stores/syncDevices.ts` (listen/stop, generation counter, coalescing load; classify each ref on remount: dialog and error state reset, running state survives); pure helpers in `src/lib/files/syncRules.ts` without nested binding imports (`typecheck:scripts`).
- [ ] T031 [US1] `FilesFolderPicker` in a new `src/components/files/FolderPicker.vue`: choose a source (`files_sources`: Meine Geräte, Speicher, eigener Ordner) and navigate folders with `files_list`; emits `{ source, path }`; no contact shares, no holzi-owned area; `data-testid` for each step. Pure part (path helpers) in `src/lib/files/syncRules.ts`.
- [ ] T032 [US1] Rules page in a new `src/components/files/SyncRules.vue` (list: name, places, direction, status badge `ShadcnBadge`, last run; actions: run now, pause, delete) and create dialog `src/components/files/SyncRuleDialog.vue` (`UiDrawerModal`; two pickers, direction, delete mode with defaults mirror/keep, interval); route `sync-rules` in `src/lib/files/registry.ts` and `src/components/wm/appRoutes.ts`, sidebar button `data-testid="files-sync-rules"` in `src/components/files/Sidebar.vue` before the storages section (`lucide:refresh-cw`); i18n `files.sidebar.syncRules`, `files.syncRules.*` in `src/i18n/locales/de.json` and `en.json`.
- [ ] T033 [P] [US1] Check script `scripts/check-files-sync-rules.ts` (picked up by the `check-files-*` glob): status labels, picker path helpers, defaults per direction; run `pnpm check:files`.
- [ ] T034 [US1] E2E `scripts/e2e/scenarios/files-sync-rules.test.ts` (one-way part): two folders through `deviceFiles(prefix)` after `createAndUnlock`, create the rule in the dialog, run now, assert files arrived and that a deletion at the source stays at the target; add to `scripts/e2e/platform-exclusions.ts` if not stable on Android (`check:e2e-exclusions`).

**Checkpoint**: quickstart §1 passes by hand; `pnpm check:files`, `pnpm typecheck`, `cargo test -j 4 --lib file_sync::` green.

---

## Phase 4: User Story 2 — Two-way sync and conflicts (Priority: P1) (PR C)

**Goal**: Two-way rules between local places and other own devices; conflict copies; first sync adopts equal files; moves and renames propagate.

**Independent Test**: Quickstart §2.

### Tests (write first)

- [ ] T035 [P] [US2] Plan tests (two-way rows) in `plan_tests.rs`: new on either side → copy, one side changed → that side wins, both changed → conflict (newer keeps the name, `Conflict { keep, aside }`), deleted vs. changed → changed stays and is copied back (FR-013), move on one side → move on the other, equal-size equal-time moves with two candidates → hash decides, no clock comparison across sides except to choose the newer in a conflict (FR-008).
- [ ] T036 [P] [US2] First-sync tests in `plan_tests.rs` and `run_tests.rs`: no state, same name and size and equal hash → `Adopt` without copying; equal size but different hash → conflict copy; different size → conflict copy; hash reads counted in `Plan.hash_reads` (FR-012a); changing places discards state and repeats the first-sync rule.
- [ ] T037 [P] [US2] Engine tests in `execute_tests.rs`: conflict copy name `Name (Konflikt Gerätename Datum).Endung` with device name from `known_devices` alias, both sides get both files, a name collision of the conflict name gets a counter, the ignore file itself syncs with a conflict copy when both sides differ (FR-039).

### Implementation

- [ ] T038 [US2] Two-way rows in `plan::build` (`plan.rs`): both-changed, deleted vs. changed, first-sync adoption, move propagation both ways; keep the one-way rows untouched.
- [ ] T039 [US2] Conflict and adopt steps in `execute.rs`: `Conflict` renames the older version aside on its own side (`Place::rename`) then copies both ways; `Adopt` writes state without data transfer; conflict name builder in a new `src-tauri/src/file_sync/conflict.rs` with tests in `conflict_tests.rs` (date format, extension split, dotfiles, counter).
- [ ] T040 [US2] Hash support in `scan.rs`/`run.rs`: compute hashes only for the candidates the plan asked for (`Plan.hash_reads`), cached per run; for encrypted places use the sidecar value; streaming with the cancel token.
- [ ] T041 [US2] Own-device places: check `PeerStore` behavior in `place_storage.rs` (listing, `Expected::Version` on copy, `StorageError::Conflict` → treat as changed meanwhile and retry next run); the rule goes to `waiting` when the device is offline or its switch is off (FR-024); test with the loopback pair in `src-tauri/src/files/peer/own_device_tests.rs` helpers.
- [ ] T042 [P] [US2] Status `waiting` and catch-up: `status.rs` and `schedule.rs` set `waiting` on `StorageError::Network|Unreachable`/`Unsupported` from a place, retry at the next interval and when `known_devices` presence changes (listen on `sync-devices-changed` equivalent in Rust); tests in `status_tests.rs`/`schedule_tests.rs` with a manual clock (no sleep).
- [ ] T043 [US2] E2E (two-way part) in `files-sync-rules.test.ts`: two folders with overlapping equal files, one differing; both changed → conflict copies on both sides; delete vs. change; rename propagates. Own-device case stays a hand check (quickstart §2.6), like spec 056.

**Checkpoint**: quickstart §2 passes by hand; SC-003 and SC-006 tests green.

---

## Phase 5: User Story 3 — Deletions and protection (Priority: P2) (PR D)

**Goal**: Delete modes mirror, keep, ask; trash behavior; empty-place guard; per-file failures; no run on a locked vault.

**Independent Test**: Quickstart §3.

### Tests (write first)

- [ ] T044 [P] [US3] Plan tests for delete modes in `plan_tests.rs`: `mirror` → `Delete` with trash, `keep` → no step and `kept = 1` so the file is not copied back, `ask` → `Delete` marked pending, delete vs. change (FR-013), restored file leaves the pending list.
- [ ] T045 [P] [US3] Guard tests in `run_tests.rs` (SC-004): a place that lists empty, a missing root, an unreadable root and an `Unreachable` error all abort with "Ort nicht erreichbar" and call no delete or overwrite on `MemPlace`; a place that is empty because the user emptied it (state shows files, root exists, listing succeeds with zero entries) is also treated as unreachable and reported, never mirrored.
- [ ] T046 [P] [US3] Trash tests in `place_device_tests.rs`/`place_storage_tests.rs`: desktop local → system trash (behind `cfg`), Android and storages → `.holzi-trash/<run>/<path>`, `.holzi-trash` never listed or synced, S3 without rename falls back to copy then delete, a failed move leaves the original.

### Implementation

- [ ] T047 [US3] Delete rows and modes in `plan::build` and `execute::delete_step` (the only place that applies a delete mode): mirror through `Place::trash`, keep marks the state, ask writes `haex_sync_rule_pending_no_sync`; refine the guard in `scan.rs` (FR-022) and make it a distinct `Outcome::waiting` with reason.
- [ ] T048 [US3] Pending flow: `sync_rules_pending_list` and `sync_rules_pending_resolve` (`delete`, `keep`, `restore`) in `commands.rs` with `choice = delete` executing the delete in the next run (or immediately when idle); a confirmed delete is checked against the current state again before it runs (the file may have changed meanwhile). Tests in `commands_tests.rs`.
- [ ] T049 [US3] Failures per file: executor records `error` items with the reason and keeps going, the rule ends `partial`, the next run retries (FR-023); locked vault mid-run → `cancelled` and no half file (FR-026), covered with a cancel token test in `run_tests.rs`.
- [ ] T050 [P] [US3] UI in a new `src/components/files/SyncRulePending.vue` (list of pending deletions with count, confirm or discard per item and for all) and the error/conflict lists in `SyncRules.vue`; i18n; `data-testid`s.
- [ ] T051 [P] [US3] E2E (protection part): delete mode ask → list appears, confirm deletes; make a place unreachable (rename the folder) → rule `waiting`/error, nothing deleted.

**Checkpoint**: quickstart §3 passes; SC-004 and SC-005 tests green.

---

## Phase 6: User Story 4 — Manage, preview, exclusions, log (Priority: P2) (PR E)

**Goal**: Preview, edit, pause, ignore-file editor, run log and the settings item "Sync-Protokoll".

**Independent Test**: Quickstart §4.

### Tests (write first)

- [ ] T052 [P] [US4] Log tests in a new `src-tauri/src/file_sync/log_tests.rs`: depth 10 keeps the latest ten, depth 0 writes nothing, shrinking trims immediately for every rule, 5 000-item cap with "not listed" flag, `trash_kind`/`trash_path` stored, log removed with the rule, no file content stored (FR-045).
- [ ] T053 [P] [US4] Ignore tests in `run_tests.rs` (SC-009): a file excluded after the first run stays on both sides and drops from state, an exclusion lifted later makes the file new (FR-040), the union of both sides excludes, a line error shows in the status but does not stop the run, an unwritable place returns `readOnly` from `ignore_set`.
- [ ] T054 [P] [US4] Preview tests in `commands_tests.rs`: `sync_rules_preview` changes nothing (no `put`, no state write), works for a draft without saving, reports `hashReads` and `ignored`.

### Implementation

- [ ] T055 [US4] Log in a new `src-tauri/src/file_sync/log.rs` (`record_run`, `trim`, `runs`, `items`) used by `run.rs`; commands `sync_rules_runs` and `sync_rules_run_items`; log depth read through `storage::preferences` (`PrefScope::Device`, key `syncRules.logDepth`, default 10) with a validation clause in `validate_value` (integer 0..100); commands `sync_rules_log_depth_get/set` (set trims at once).
- [ ] T056 [US4] Preview and ignore commands in `commands.rs`: `sync_rules_preview` (scan + plan, no execute), `sync_rules_ignore_get` (file at the source place, fixed exclusions, `ignoredNow`, `writable`), `sync_rules_ignore_set` (`Place::write_text`, `readOnly` when not writable).
- [ ] T057 [P] [US4] Ignore editor in a new `src/components/files/SyncRuleIgnoreEditor.vue` (`UiTextarea`, fixed exclusions listed read-only, count of ignored files, line errors, read-only notice with reason); opened from the rule row ("Ausnahmen").
- [ ] T058 [P] [US4] Preview and log views: `src/components/files/SyncRulePlan.vue` (steps grouped by action, counts, hash read count) and `src/components/files/SyncRuleLog.vue` (runs with counters, expandable per-file list, link to `.holzi-trash` entry or "Papierkorb des Systems" text); the create dialog gets the cloud hint (FR-031: an encrypted folder protects content) and a preview button; edit and pause actions in the rule row (Story 4, scenarios 5 and 6).
- [ ] T059 [US4] Settings item "Sync-Protokoll": `subView('general.syncLog', 'general/sync-log', 'general', { icon: 'lucide:history', overviewRow: true, settingKeys: ['syncRules.logDepth'] })` in `src/lib/settings/registry.ts`, `src/components/settings/general/SyncLogView.vue` after `ExtensionLimitsView.vue` (number input, save on blur or Enter, invalid input falls back), route in `src/components/wm/appRoutes.ts`, path in `scripts/check-settings.ts` `LOCATION_PATHS`, i18n `settings.locations.general.syncLog.*`.
- [ ] T060 [P] [US4] E2E (manage part): create a rule, preview, edit `.haex-ignore` through the editor, run, assert excluded files untouched; open the log and expand a run; set the depth to 2 in the settings and see older runs gone.

**Checkpoint**: quickstart §4 passes; `pnpm check:settings`, `pnpm check:files` green.

---

## Phase 7: User Story 5 — Agents (Priority: P3) (PR F)

**Goal**: An agent with the permission "manage sync rules" reads rules and, after a per-call confirmation, creates, changes and deletes them.

**Independent Test**: Quickstart §5.

### Tests (write first)

- [ ] T061 [P] [US5] Permission tests: extend `src-tauri/src/files/permissions_tests.rs` and `access_tests.rs` for `AgentFileKind::SyncRules` (empty target, parse/as_str, `AgentGrants.sync_rules`); `scripts/check-files-agent-permissions.ts` for the new kind; confirm no CHECK constraint on `agent_file_permissions.kind` (migration `0029`).
- [ ] T062 [P] [US5] Agent tests in `src-tauri/src/file_sync/agent_tests.rs` (after `src-tauri/src/files/agent/shares_tests.rs`): no permission → refused, read actions without a question, `create`/`update`/`delete` ask every time regardless of approval mode, decline and timeout (`PromptEnd`) change nothing, an agent cannot pick holzi-owned areas or `share:` places, `confirms_itself` avoids a double question.

### Implementation

- [ ] T063 [US5] `AgentFileKind::SyncRules` and `AgentGrants.sync_rules` in `src-tauri/src/files/permissions.rs` and `access.rs` (user-set only, mirror `FileShares`); regenerate `AgentFileKind.ts`.
- [ ] T064 [US5] Actions in a new `src/lib/actions/syncRuleActions.ts` (`syncRules.list`, `.status` read; `.create`, `.update`, `.delete` destructive; `runner: 'native'`, `scope: 'files.syncRules'`, `agentCallable: true`), registered where `fileShareActions` is; `check:agent-actions` updated.
- [ ] T065 [US5] Executor in a new `src-tauri/src/file_sync/agent.rs` (`SyncRulesAgent: NativeExecutor`, grant check) and the question in `agent_confirm.rs` (`SyncRuleConfirm` via `PromptPark`, event + answer command, shows places, direction, delete mode, interval; old values for `update`); register in `src-tauri/src/lib.rs` (`native_actions.add`) and the command in `generate_handler!`.
- [ ] T066 [P] [US5] UI: switch "Sync-Regeln verwalten" in `src/components/settings/AgentFilesSetting.vue` (kind `syncRules`, `src/lib/files/agentPermissions.ts`), question dialog `src/components/files/SyncRuleConfirmDialog.vue` after `files/AgentPermissionDialog.vue`; i18n `settings.agentFiles.syncRules*`.
- [ ] T067 [P] [US5] E2E in `scripts/e2e/scenarios/settings-agent-files.test.ts` or a new scenario: permission off → refused, on → confirmation dialog, accept creates the rule, decline does not.

**Checkpoint**: quickstart §5 passes.

---

## Phase 8: Polish & cross-cutting (PR F)

- [ ] T068 Performance test in `src-tauri/src/file_sync/perf_tests.rs` (SC-007, after `files/encrypted/perf_tests.rs`): 10 000 unchanged local files, one run under 5 s; `#[ignore]` unless `HOLZI_PERF=1` so normal `cargo test` stays quiet; record the number in research.md.
- [ ] T069 [P] Android and platform check: `cargo clippy -j 4 --lib --target aarch64-linux-android -- -D warnings` for the `cfg` branches (trash, watcher); run the quickstart §1 on the local emulator per memory (JDK from android-studio, AVD API 35, uninstall before install; clean up afterwards).
- [ ] T070 [P] Remove spec 025 and fix every reference (research R14): `specs/056-file-access-sharing/spec.md` (~67, ~583), `specs/044-file-browser/spec.md` (~73), `docs/plans/2026-10-07-file-browser-design.md` (~16), `docs/plans/2026-10-21-file-access-and-sharing-design.md` (~8, ~125), `docs/adr/0013-encrypted-folder-format.md` (~10), `docs/plans/2026-09-28-sync-architecture.md` (table ~637), `docs/plans/2026-10-26-sync-rules-design.md` (line 5), `specs/061-sync-rules/spec.md` (~61); check `specs/026-blind-relay` and `specs/028-data-shares` by hand; delete `specs/025-own-device-file-sync/`; run `python3 scripts/ci/check-docs.py`.
- [ ] T071 [P] Docs: mark ADR 0018 accepted; update `plans/README.md` row; add the new Tauri commands to the contracts index if there is one; tick tasks in this file.
- [ ] T072 Hand check on two real devices (own-device place, Android local place), quickstart §1 to §5; write what was checked in the PR description. Everything else needs only the automatic checks above.
- [ ] T073 Final pass before each PR: `pnpm typecheck && pnpm typecheck:scripts && pnpm lint && pnpm format:check && pnpm check:files && pnpm check:settings && pnpm check:agent-actions`; `cargo clippy -j 4 --lib -- -D warnings`; `cargo test -j 4 --lib file_sync::`; run `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` after `cargo test` (ts-rs drift, memory) and check `git status` for stray binding changes.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none; T001–T003 are decision spikes that block the tasks naming them (T001 → T014, T002 → T009, T003 → T016); T007 before any code using `ignore`; T008 before any code.
- **Foundational (Phase 2)**: after Setup; blocks all stories. T009 → T010–T012; T011 before T012–T013; T014 before T015–T017; T018 and T019 can follow T011.
- **US1 (Phase 3)**: after Phase 2. T020–T021 before T022–T025; T023 before T025; T024 before T025; T026 after T025; T027 parallel to T026; T028 and T029 after T026; T030–T033 after T029; T034 last.
- **US2 (Phase 4)**: after US1 (executor, run, schedule). T035–T037 before T038–T039; T040 after T038; T041 after T039; T042 after T041.
- **US3 (Phase 5)**: after US1; independent of US2 except for the shared `plan.rs` (merge order PR C → PR D). T044–T046 before T047.
- **US4 (Phase 6)**: after US1 (commands, run); T055 before T058–T060; preview and ignore commands (T056) before T057/T058.
- **US5 (Phase 7)**: after US1 (commands); needs `AgentFileKind` (T063) before T064–T066.
- **Polish (Phase 8)**: after all stories; T070 only after the references in other docs are ready, in the last PR.

### Parallel Opportunities

- Phase 1: T002–T006 in parallel after T001 is under way.
- Phase 2: T010 parallel to T011; T013, T015, T016, T017 in parallel after T014; T018 parallel to T019.
- US1: T020 and T021 in parallel; T027, T030, T031, T033 parallel once T029 exists.
- US2: T035, T036, T037 in parallel.
- US3: T044, T045, T046 in parallel; T050 and T051 in parallel.
- US4: T052, T053, T054 in parallel; T057 and T058 in parallel.

## Parallel Example: User Story 1

```text
Task: "T020 [P] [US1] Plan tests (one-way rows) in src-tauri/src/file_sync/plan_tests.rs"
Task: "T021 [P] [US1] Engine tests in src-tauri/src/file_sync/execute_tests.rs and run_tests.rs"
```

## Implementation Strategy

### MVP First (User Story 1)

1. Phase 1 + Phase 2 (PR A): decisions, ADR, migration, places, matcher; no visible change.
2. Phase 3 (PR B): one-way backup, already useful (folder to cloud).
3. **Stop and validate** with quickstart.md §1.

### Incremental Delivery

1. PR C (US2): two-way and conflicts, first-sync adoption.
2. PR D (US3): delete modes, pending list, guards.
3. PR E (US4): preview, exclusions editor, log, settings item.
4. PR F (US5 + Polish): agents, performance, platform check, removal of spec 025.

## Notes

- Never `git stash` bare; never force-push; fetch before push (the operator pushes review fixes).
- Commits follow Conventional Commits without agent trailers; no `Co-Authored-By`.
- New ts-rs types use the prefix `SyncRule`; check `ls src/types/bindings/<Name>.ts` before adding one.
- Clippy also for `aarch64-linux-android` before pushing code with `cfg` branches.
- A Rust change needs an app restart before debugging in the UI (no hot reload for Rust).
- Tasks are ticked only when the named test exists and passes (memory: adapters no silent defaults, #485).
