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

**Shipping note**: PR A = Phase 1 + Phase 2 (foundation, no visible change); PR B = US1; PR C = US2 plus the
mirror-delete prerequisite T044/T047; PR D = the remaining US3 work (T048–T051); PR E = US4; PR F = US5 + Polish.
Each PR is reviewed and merged before the next one starts on top of `main`. Work in `.worktrees/061-sync-rules`
(other sessions switch branches in the main checkout). The
documentation PR #580 (design, spec, plan, tasks) lands first.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup (PR A)

- [x] T001 Spike: behavior of the conflict choice `Replace` of `files::transfer` (research R2, risk 1). In a new `src-tauri/src/files/transfer/replace_tests.rs` pin what `local::run` and `remote::run_remote` do for an existing target (moved aside, left over, atomic?), for an aborted copy, and for a changed peer version. Write the result as a short note in research.md R2. If `Replace` leaves leftovers, T014 gets a "replace without moving aside" mode.
- [x] T002 [P] Check the highest used migration number on `main` (`src-tauri/src/identity/migrations.rs`, specs 059/060 and open PRs); settle the number of `0044_sync_rules` (research R7, risk 3) and write it into data-model.md and plan.md if it differs.
- [x] T003 [P] Settle how an encrypted folder is addressed as a place (research R1, risk 0): read `src-tauri/src/files/encrypted/route.rs`, `source.rs` and `browser_commands/mod.rs` (`storage_files`, `window_storage`); write the place form (storage id plus path, or `device:<root>`) into data-model.md (`place_*_source`) and research.md.
- [x] T004 [P] Before the first new name, run bounded `graphify query … --budget 1200` for "device-local table no_sync", "gitignore matcher", "folder watcher debounce", "storage place list recursive"; note candidates to reuse in research.md. — Done 2026-10-26: no reusable gitignore matcher; `files::search::Matcher` exists, so the new type is `Exclusions`; `files::search::search_storage` shows the folder-by-folder walk of a storage (reused as a pattern, but a folder that cannot be listed fails the listing instead of being skipped).
- [x] T005 [P] ADR `docs/adr/0018-sync-rules-state-stays-on-the-device.md`: rules, per-file state and run log live in `*_no_sync` tables keyed by `vault_device_uuid`; why this deviates from ADR 0001 (state per file would sync to every device; spec FR-003, FR-045); copy-safety through the device column. Status proposed, date 2026-10-26.
- [x] T006 [P] Glossary: add **Sync-Regel / Ort / Abgleich / Stand / Plan / Konfliktkopie / Löschmodus / Ausnahmedatei / Lauf-Protokoll** to `CONTEXT.md` (new section "Sync-Regeln"); remove the "Sync-Regeln (ersetzen 025)" wording where it only points to the draft.
- [x] T007 Add the crate: `cargo add ignore@0.4 --no-default-features` in `src-tauri/Cargo.toml` (only `gitignore` is used; verify the resolved version and that `aarch64-linux-android` still builds); commit `Cargo.lock`.
- [x] T008 Prepare the worktree for code: `pnpm install --frozen-lockfile`; check `df -h`; reflink `src-tauri/target` from a merged worktree with the same `Cargo.lock` if one exists (memory: old worktree targets fill the disk).

---

## Phase 2: Foundational (PR A, no visible change)

**Purpose**: Tables, model types, the `Place` layer, the exclusion matcher and the pure plan types. Nothing is
offered in the UI yet; every later story builds on these.

### Data

- [x] T009 Migration `0044_sync_rules` (number per T002) in a new `src-tauri/src/identity/migrations_sync_rules.rs` with exactly the five tables of data-model.md, all `_no_sync`, each with `vault_device_uuid` FK to `known_devices(vault_device_uuid) ON DELETE CASCADE`; register in `src-tauri/src/identity/migrations.rs` (`holzi_migration_source`). No `HOLZI_TRIGGER_VERSION` change; check whether the legacy-name helpers (`holzi_migration_source_with_legacy_*`) need an entry.
- [x] T010 [P] Migration test in `src-tauri/src/identity/migrations_sync_rules_tests.rs` following `migrations_file_shares_tests.rs`: tables exist, no CRDT triggers are created for them, deleting a rule cascades to state, pending, runs and run items, deleting the device cascades to all.
- [x] T011 Model in a new `src-tauri/src/file_sync/model.rs` (+ `mod.rs` with the module and `#[cfg(test)]` test modules): `Rule`, `RuleDraft`, `Place { source, path }`, `Direction`, `DeleteMode`, `RuleStatus`, ts-rs types with the prefix `SyncRule` and `#[ts(export, export_to = "../../src/types/bindings/")]`, `u64`/`i64` as `number`; check `ls src/types/bindings/SyncRule*.ts` first (name collision rule).
- [x] T012 Rule store in a new `src-tauri/src/file_sync/store.rs`: create, update, delete, list, set enabled, last result; state rows (`get_all(rule_id)`, `put(rule_id, path, row)`, `rename`, `clear`), pending rows, all with the device uuid. Tests in `src-tauri/src/file_sync/store_tests.rs`: round trip, one row per `(rule_id, path)`, `clear` on changed places, rows of another device are invisible.
- [x] T013 [P] Rule validation in a new `src-tauri/src/file_sync/rules.rs` (`validate`): places differ, no overlap with another rule of this device (same source and one path inside the other, also crosswise A/B), no `share:` source, no holzi-owned area (`OwnPlaces`), interval 60..86 400, name 1..80 chars, `bToA` normalised to `aToB`; error type `SyncRuleError { code, message }` (research R9). Tests in `rules_tests.rs`: every code, overlap crosswise, a rule on another device does not count (FR-004).

### Places

- [x] T014 `Place` trait and facts in a new `src-tauri/src/file_sync/place.rs`: `list_tree`, `stat`, `hash`, `rename`, `delete_permanent`, `trash`, `read_text`, `write_text`, `create_folder`, plus the `Facts` type (path relative, size, mtime, kind, symlink, no_access). Copy is not part of it; add `copy_file(from: Side, path, to: Side, target, hooks)` as the narrow entry in `src-tauri/src/files/transfer/mod.rs` that wraps `local::run` / `run_remote`, returns the measured target size and time, and answers conflicts with `Replace` (mode per T001). — Done 2026-10-26: the entry is `files::transfer::sync_copy::transfer_one(&OneTransfer { op, from, path, to, folder, replaces }, …)` in its own file (open PR #579 changes `transfer/mod.rs`); `replaces` says whether the plan expects a file there, every other question answers Skip and ends as `Conflict` (research R2). Files leaving an encrypted folder for an ordinary folder are refused while a rule runs.
- [x] T015 [P] `DevicePlace` in a new `src-tauri/src/file_sync/place_device.rs`: `walkdir` without following links, symlinks listed as skipped entries, `trash::delete` on the desktop (`#[cfg(not(any(target_os = "android", target_os = "ios")))]`), `.holzi-trash` otherwise, streaming SHA-256 with `sha2`, `write_text` through a temp file and rename. Tests in `place_device_tests.rs` with a temp dir: tree with links and unreadable file, trash fallback, hash equals known vector, `write_text` leaves no temp file. — Done 2026-10-26: `DevicePlace::with_rule_trash` keeps tests out of the user's trash; a folder that cannot be read fails the listing.
- [x] T016 [P] `StoragePlace` in a new `src-tauri/src/file_sync/place_storage.rs` over `StorageFiles` (storages, peer devices, encrypted folders per T003): recursive listing from `list` levels, `hash` streamed through `read_start` or the sidecar `sha256` for encrypted folders, `rename` through `rename`/`move_object`, `delete` (always permanent) and `trash` through a move into `.holzi-trash`. Tests in `place_storage_tests.rs` against the local-store test support in `src-tauri/src/remote_storage/test_support.rs` and `files/test_support.rs`. — Done 2026-10-26: storage paths are window paths (`/Fotos`); hashes of encrypted files come from the sidecar.
- [x] T017 [P] In-memory `Place` for tests in `src-tauri/src/file_sync/test_support.rs` (`MemPlace`: files with size, mtime, content; counters for calls; injectable failures; a switch for "lists empty"); used by plan, execute and run tests.

### Exclusions and plan types

- [x] T018 Exclusion matcher in a new `src-tauri/src/file_sync/ignore.rs`: `Matcher::new(a_text, b_text)` builds two `GitignoreBuilder`s, adds the fixed exclusions, evaluates `matched_path_or_any_parents`, never excludes `.haex-ignore`, collects line errors with line numbers, rejects files over 64 KiB / 2 000 lines and fails the run closed for an existing oversized file (contracts/ignore-file.md). Tests in `ignore_tests.rs`: negation, directory-only, anchored, nested parent, union of both sides, fixed exclusions (`.holzi-trash/`, `.*.holzi-part-*`, `.DS_Store`), bad line skipped and reported, rejected editor write, oversized file abort. — Done 2026-10-26: named `Exclusions` (`files::search::Matcher` exists); `.*.holzi-old-*` is a fixed exclusion too (research R2).
- [x] T019 Plan types in a new `src-tauri/src/file_sync/plan.rs`: `Step` (`Copy`, `Move`, `Delete`, `Conflict`, `Adopt`, `Skip`, `Report`), `Plan { steps, counts, hash_reads, ignored }`, `build` signature `build(rule, left: &Facts, right: &Facts, state: &State, ignore: &Matcher, hashes: &dyn Fn(Side, &str) -> Option<Hash>) -> Plan` with the step order of research R3 (folders, moves, copies and conflict copies, deletions last); `todo!()`-free skeleton that returns an empty plan, so US1 to US3 fill the rows.

**Checkpoint**: `cargo test -j 4 --lib file_sync::` and `identity::migrations_sync_rules` green; nothing visible yet.

---

## Phase 3: User Story 1 — One-way backup (Priority: P1) 🎯 MVP (PR B)

**Goal**: A one-way rule between two places copies new and changed files, keeps deleted ones (mode keep), reports
changes at the target, and survives an abort. Backend runs on a schedule while holzi is active; a minimal
rules list in the file browser can create, run and delete a rule.

**Independent Test**: Quickstart §1 with two local folders.

### Tests (write first)

- [x] T020 [P] [US1] Plan tests (one-way rows) in `src-tauri/src/file_sync/plan_tests.rs`: new → copy, changed → copy, unchanged → nothing, target changed → `Report` and no overwrite (FR-010), deleted at source with `keep` → nothing and `kept` stays, move on the source side found by size and time only after identity/content verification (FR-009), ignored paths absent from the plan.
- [x] T021 [P] [US1] Engine tests in `src-tauri/src/file_sync/execute_tests.rs` and `run_tests.rs` with `MemPlace` and a temp folder: first run copies everything, second run copies nothing, abort after the third of five copies leaves state only for the finished ones and no half file (SC-005, FR-014, FR-015), the target's measured time is stored (not the source's), a failing file does not stop the run and is retried next run (FR-023), symlinks are skipped and listed (FR-018).

### Implementation

- [x] T022 [US1] Plan rows for one-way in `plan.rs` (`build`): scan against state, moves by size and time only after reliable identity/content verification (hash when needed), `Report` for target deviations, `kept` handling. Pure; keep every decision in this function (one implementation per rule). — Done 2026-10-27: the plan has two more steps, `Aside` (a conflict version goes aside) and `Forget` (gone on both sides). A move needs the hash the state holds (FR-009); a copy notes the hash of a device source up to 64 MiB, or the sidecar's. Also done for one-way rules: the delete modes `mirror`/`keep`/`ask` (rows of T047); the pending flow of T048 stays open.
- [x] T023 [US1] Scan in a new `src-tauri/src/file_sync/scan.rs`: list both sides in parallel, read both `.haex-ignore` through `read_text`, build `Matcher`, return `Facts` and ignore errors; a missing/unreadable root or `Unreachable` returns `PlaceError::Unreachable`, while a verified empty target is allowed on the first run without state; an empty known place remains protected (FR-022, used fully in US3). — Done 2026-10-27: an exclusion file over its limits or one whose matcher cannot be built fails the run closed.
- [x] T024 [US1] Executor in a new `src-tauri/src/file_sync/execute.rs`: run steps in plan order, `Copy` through `copy_file` into a temp name and rename (FR-014), measure the target afterwards (`stat`), write the state row in one transaction after each step (FR-015), `Move` through `rename`, collect failures per file without stopping (FR-023); honour the cancel token between steps and pass it into `copy_file`.
- [x] T025 [US1] One run in a new `src-tauri/src/file_sync/run.rs`: scan → plan → execute → write result to the rule row; return `Outcome { done, partial, waiting, error, cancelled }` with counters. Tests in `run_tests.rs` (T021).
- [x] T026 [US1] Service and scheduling in a new `src-tauri/src/file_sync/schedule.rs` and `mod.rs` (`FileSyncState`, `start_for_active_instance`): one control task per vault session with a timer per active rule (interval, 60 min safety net for local places), "due" marker when a run is active, at most 2 runs at once, every active rule due once at start (catch up), cancel on `gate.token()` (research R8). Register `crate::file_sync::start_for_active_instance(&app, &state)` in `src-tauri/src/instances/open.rs` **and** `src-tauri/src/instances/import.rs` (memory: both starts), and manage `FileSyncState` in `lib.rs`. — Done 2026-10-27: loop in `schedule.rs` (`Runner` as seam, tested with a paused clock), session in `service.rs` (`FileSyncState`, `TauriRunner`); places open through `files::browser_commands::place_of`/`window_storage` (now `pub(crate)`). One-way rules watch their source only.
- [x] T027 [P] [US1] Folder watcher in a new `src-tauri/src/file_sync/watch.rs` over `files::local::watch::watch_folder` (recursive, own 2 s quiet period, drops with the task); wired into `schedule.rs` for rules with a local place; not compiled on iOS. Tests in `watch_tests.rs` with a temp dir and a manual trigger (no sleep: use the debouncer callback directly).
- [x] T028 [US1] Status in a new `src-tauri/src/file_sync/status.rs` (derive `RuleStatus` from last outcome and control state, research R9) and the event `sync-rules-changed` / `sync-rules-progress` (contracts/tauri-commands.md); tests in `status_tests.rs` for each state.
- [x] T029 [US1] Commands in a new `src-tauri/src/file_sync/commands.rs`: `sync_rules_list`, `sync_rules_create`, `sync_rules_update`, `sync_rules_delete`, `sync_rules_set_enabled`, `sync_rules_run_now`, `sync_rules_cancel`; register in the single `generate_handler![…]` in `src-tauri/src/lib.rs` behind `gate.wrap(...)`. Update drops state when places or direction change; delete leaves files untouched (FR-006). Tests in `commands_tests.rs` for create/validate/update/delete. — Done 2026-10-27: `create`/`update` refuse `both` with `unsupported` until PR C; new codes `unsupported`, `unavailable`. The refusal and the code `unsupported` went with the two-way work (2026-10-28).
- [x] T030 [P] [US1] Composable and store: `src/composables/useSyncRules.ts` (all `invoke('sync_rules_*')`) and `src/stores/syncRules.ts` after `src/stores/syncDevices.ts` (listen/stop, generation counter, coalescing load; classify each ref on remount: dialog and error state reset, running state survives); pure helpers in `src/lib/files/syncRules.ts` without nested binding imports (`typecheck:scripts`).
- [x] T031 [US1] `FilesFolderPicker` in a new `src/components/files/FolderPicker.vue`: choose a source (`files_sources`: Meine Geräte, Speicher, eigener Ordner) and navigate folders with `files_list`; emits `{ source, path }`; no contact shares, no holzi-owned area; `data-testid` for each step. Pure part (path helpers) in `src/lib/files/syncRules.ts`.
- [x] T032 [US1] Rules page in a new `src/components/files/SyncRules.vue` (list: name, places, direction, status badge `ShadcnBadge`, last run; actions: run now, pause, delete) and create dialog `src/components/files/SyncRuleDialog.vue` (`UiDrawerModal`; two pickers, one-way direction and keep mode in the MVP; expose beidseitig and its mirror default only after the mirror-delete path in T047 is available, interval); route `sync-rules` in `src/lib/files/registry.ts` and `src/components/wm/appRoutes.ts`, sidebar button `data-testid="files-sync-rules"` in `src/components/files/Sidebar.vue` before the storages section (`lucide:refresh-cw`); i18n `files.sidebar.syncRules`, `files.syncRules.*` in `src/i18n/locales/de.json` and `en.json`. — Done 2026-10-27, as a dialog like „Meine Dateifreigaben“ instead of a route (`files-sync-rules-open` in the sidebar); no change to the tab location.
- [x] T033 [P] [US1] Check script `scripts/check-files-sync-rules.ts` (picked up by the `check-files-*` glob): status labels, picker path helpers, defaults per direction; run `pnpm check:files`.
- [x] T034 [US1] E2E `scripts/e2e/scenarios/files-sync-rules.test.ts` (one-way part): two folders through `deviceFiles(prefix)` after `createAndUnlock`, create the rule in the dialog, run now, assert files arrived and that a deletion at the source stays at the target; add to `scripts/e2e/platform-exclusions.ts` if not stable on Android (`check:e2e-exclusions`). — Done 2026-10-27: the rule is created through `sync_rules_create`, since the picker's roots differ per platform; deletions are covered by the Rust runs.

**Checkpoint**: quickstart §1 passes by hand; `pnpm check:files`, `pnpm typecheck`, `cargo test -j 4 --lib file_sync::` green.

---

### Added during PR B (2026-10-27)

- [x] T074 [US1] Decrypt consent (FR-030a): migration `0046_sync_rules_decrypt` (`allow_decrypted`), `SyncRuleDraft.allowDecrypted`, `OneTransfer.decrypts` (the plan's `leaving` is confirmed only with it; moves within a place never decrypt), the consent checkbox in `SyncRuleDialog.vue` when A is encrypted and B is not.

---

## Phase 4: User Story 2 — Two-way sync and conflicts (Priority: P1) (PR C)

**Goal**: Two-way rules between local places and other own devices; conflict copies; first sync adopts equal files; moves and renames propagate.

**Independent Test**: Quickstart §2.

### Tests (write first)

- [x] T035 [P] [US2] Plan tests (two-way rows) in `plan_tests.rs`: new on either side → copy, one side changed → that side wins, both changed → conflict (newer keeps the name, `Conflict { keep, aside }`), deleted vs. changed → changed stays and is copied back (FR-013), move on one side → move on the other, equal-size equal-time moves with two candidates → hash decides, no clock comparison across sides except to choose the newer in a conflict (FR-008). — Done 2026-10-28: all rows in `plan_tests.rs` (`two_way_*`); the side whose version is newer by its time keeps the name, A on a tie.
- [x] T036 [P] [US2] First-sync tests in `plan_tests.rs` and `run_tests.rs`: no state, same name and size and equal hash → `Adopt` without copying; equal size but different hash → conflict copy; different size → conflict copy; hash reads counted in `Plan.hash_reads` (FR-012a); changing places discards state and repeats the first-sync rule. — Done 2026-10-28: adoption and conflict rows in `plan_tests.rs`, a first run against real folders in `run_tests.rs`; the reset of the state on a change of places or direction is the store's (PR A, `update_rule`).
- [x] T037 [P] [US2] Engine tests in `execute_tests.rs`: conflict copy name `Name (Konflikt Gerätename Datum).Endung` with device name from `known_devices` alias, both sides get both files, a name collision of the conflict name gets a counter, and conflicting ignore files retain both conflict copies while the merged pattern union becomes the active file on both sides (FR-039). — Done 2026-10-28: covered by the two-way runs in `run_tests.rs` against real folders instead of `execute_tests.rs` (renames, copies both ways and the merge need real places). The device name is the alias of this device, the counter is in `conflict_tests.rs`. A conflict copy's name is free on both sides; the merged exclusion file keeps the lines of A, then the lines of B that A does not have (`ignore::union_text`).

### Implementation

- [x] T038 [US2] Two-way rows in `plan::build` (`plan.rs`): both-changed, deleted vs. changed, first-sync adoption, move propagation both ways; keep the one-way rows untouched. — Done 2026-10-28: one planner (`Planner`) with a one-way and a two-way path; one-way rows unchanged. A folder is not in the state, so a two-way rule creates only the folders its copies go into: an empty folder deleted on one side would otherwise come back from the other. Empty folders of a one-way source still come.
- [x] T039 [US2] Conflict and adopt steps in `execute.rs`: `Conflict` renames the older version aside on its own side (`Place::rename`) then copies both ways; `Adopt` writes state without data transfer; conflict name builder in a new `src-tauri/src/file_sync/conflict.rs` with tests in `conflict_tests.rs` (date format, extension split, dotfiles, counter). — Done 2026-10-28: steps `Conflict { path, keep, aside }` and `MergeIgnore { aside_a, aside_b }`; `conflict.rs` came with PR B.
- [x] T040 [US2] Hash support in `scan.rs`/`run.rs`: compute hashes only for the candidates the plan asked for (`Plan.hash_reads`), cached per run; for encrypted places use the sidecar value; streaming with the cancel token. — Done 2026-10-28: came with PR B (`run.rs` reads `Plan.hash_reads` once and plans again; the sidecar value counts as the hash); used by two-way moves and first meetings.
- [x] T041 [US2] Own-device places: check `PeerStore` behavior in `place_storage.rs` (listing, `Expected::Version` on copy, `StorageError::Conflict` → treat as changed meanwhile and retry next run); the rule goes to `waiting` when the device is offline or its switch is off (FR-024); test with the loopback pair in `src-tauri/src/files/peer/own_device_tests.rs` helpers. — Done 2026-10-28: no change needed: another own device that is off (`Network`) or whose file access is off (`MissingRight`) lists as `Unreachable`, so the rule waits; test in `place_storage_tests.rs`. A version conflict on copy ends as a failed file and comes again (PR B, `sync_copy`). No loopback test: the peer path is the storage path of 056.
- [x] T042 [P] [US2] Status `waiting` and catch-up: `status.rs` and `schedule.rs` set `waiting` on `StorageError::Network|Unreachable`/`Unsupported` from a place, retry at the next interval and when `known_devices` presence changes (listen on `sync-devices-changed` equivalent in Rust); tests in `status_tests.rs`/`schedule_tests.rs` with a manual clock (no sleep). — Done 2026-10-28: `waiting` came with PR B; new `Command::Wake` makes waiting rules run when `sync-devices-changed` fires (`service.rs` listens), test in `schedule_tests.rs`. A two-way rule watches both local places.
- [x] T043 [US2] E2E (two-way part) in `files-sync-rules.test.ts`: two folders with overlapping equal files, one differing; both changed → conflict copies on both sides; delete vs. change; rename propagates. Own-device case stays a hand check (quickstart §2.6), like spec 056. — Done 2026-10-28: new files both ways and a conflict copy on both sides; delete against change and moves stay in the Rust runs, since the device helpers of the e2e have no delete or rename.

**Checkpoint**: quickstart §2 passes by hand; SC-003 and SC-006 tests green.

---

## Phase 5: User Story 3 — Deletions and protection (Priority: P2) (PR D)

**Goal**: Delete modes mirror, keep, ask; trash behavior; empty-place guard; per-file failures; no run on a locked vault.

**Independent Test**: Quickstart §3.

### Tests (write first)

- [x] T044 [P] [US3] Plan tests for delete modes in `plan_tests.rs`: `mirror` → `Delete` with trash, `keep` → no step and `kept = 1` so the file is not copied back, `ask` → `Delete` marked pending, delete vs. change (FR-013), restored file leaves the pending list. — Done 2026-10-28: one-way rows in PR B, two-way rows here (`two_way_a_deletion_follows_the_delete_mode_on_the_other_side`, `two_way_a_kept_file_is_not_copied_back`); the pending list's part comes with T048.
- [x] T045 [P] [US3] Guard tests in `run_tests.rs` (SC-004): a missing root, an unreadable root and an `Unreachable` error abort with "Ort nicht erreichbar" and call no delete or overwrite on `MemPlace`; a reachable, intentionally empty target on the first run is allowed, while a place that is empty because the user emptied it (state shows files, root exists, listing succeeds with zero entries) is treated as unreachable and reported, never mirrored. — Done 2026-10-29: the empty known place, `Unreachable` and the first run into an empty target came with PR B (`a_known_place_that_lists_empty_waits_and_changes_nothing`, `an_unreachable_place_waits`, `the_first_run_copies_everything_and_the_second_nothing`); new `a_missing_or_unreadable_root_stops_before_any_change` against real folders. An unreadable root ends `error`, a missing one `waiting`; neither changes the other place.
- [x] T046 [P] [US3] Trash tests in `place_device_tests.rs`/`place_storage_tests.rs`: desktop local → system trash (behind `cfg`), Android and storages → `.holzi-trash/<run>/<path>`, `.holzi-trash` never listed or synced, S3 without rename falls back to copy then delete, a failed move leaves the original. — Done 2026-10-29: rule trash and `.holzi-trash` never synced (`run_tests`), a failed move into the trash keeps the original (device and storage), a storage without rename copies then deletes. The system trash has no test: it would fill the trash of whoever runs the tests.

### Implementation

- [x] T047 [US3] Delete rows and modes in `plan::build` and `execute::delete_step` (the only place that applies a delete mode): mirror through `Place::trash`, keep marks the state, ask writes `haex_sync_rule_pending_no_sync`; refine the guard in `scan.rs` (FR-022) and make it a distinct `Outcome::waiting` with reason. The mirror path and its tests must land before two-way rules with the mirror default are exposed; this is a prerequisite for the PR C two-way work. — Done 2026-10-28: rows and modes came with PR B for one-way and apply to both sides here; the guard (FR-022) is `run::emptied` with `Waiting` and a reason.
- [x] T048 [US3] Pending flow: `sync_rules_pending_list` and `sync_rules_pending_resolve` (`delete`, `keep`, `restore`) in `commands.rs` with `choice = delete` executing the delete in the next run (or immediately when idle); a confirmed delete is checked against the current state again before it runs (the file may have changed meanwhile). Tests in `commands_tests.rs`. — Done 2026-10-29: the choice runs at once in a new `pending.rs` (`list`, `resolve`) through the steps of a run, never "in the next run"; a run of the rule stops meanwhile and goes on after. A file that changed or is back on the other side is not deleted or kept, its row goes and the next run decides; a row whose step failed stays. A run drops rows it no longer plans (file back, mode changed). `restore` copies the file back, also against the direction of a one-way rule. Tests in `pending_tests.rs`, the unknown rule in `commands_tests.rs`. The command returns the files that failed; new error code `unreachable`.
- [x] T049 [US3] Failures per file: executor records `error` items with the reason and keeps going, the rule ends `partial`, the next run retries (FR-023); locked vault mid-run → `cancelled` and no half file (FR-026), covered with a cancel token test in `run_tests.rs`. — Done 2026-10-29: came with PR B (`a_failing_file_does_not_stop_the_run_and_comes_again`, `an_abort_keeps_the_state_of_finished_files_and_leaves_no_half_file`); the error items of the log come with T055.
- [x] T050 [P] [US3] UI in a new `src/components/files/SyncRulePending.vue` (list of pending deletions with count, confirm or discard per item and for all) and the error/conflict lists in `SyncRules.vue`; i18n; `data-testid`s. — Done 2026-10-29: `SyncRulePending.vue` in the rule row while deletions wait (count, list, delete/keep/copy back per file and for all); the overview lists the conflict copies of the last run (new `conflictCopies` in the status) and the failed files. Opening a conflict copy from there is not done; the names say where to find it.
- [x] T051 [P] [US3] E2E (protection part): delete mode ask → list appears, confirm deletes; make a place unreachable (rename the folder) → rule `waiting`/error, nothing deleted. — Done 2026-10-29: in `files-sync-rules.test.ts` with a third rule "Nachfrage"; the gone place is a removed folder (no rename helper), the mirroring two-way rule then waits and the other side keeps its files. New `delete` in the e2e device-files helper.

**Checkpoint**: quickstart §3 passes; SC-004 and SC-005 tests green.

---

## Phase 6: User Story 4 — Manage, preview, exclusions, log (Priority: P2) (PR E)

**Goal**: Preview, edit, pause, ignore-file editor, run log and the settings item "Sync-Protokoll".

**Independent Test**: Quickstart §4.

### Tests (write first)

- [x] T052 [P] [US4] Log tests in a new `src-tauri/src/file_sync/log_tests.rs`: depth 10 keeps the latest ten, depth 0 writes nothing, shrinking trims immediately for every rule, every file action is listed without a per-run cap, `trash_kind`/`trash_path` stored, log removed with the rule, no file content stored (FR-045). — Done 2026-10-29: in `run_log_tests.rs` (the module is `run_log.rs`: `log` would hide the `log` crate); also runs of another device stay out. The log holds paths, actions and reasons only.
- [x] T053 [P] [US4] Ignore tests in `run_tests.rs` (SC-009): a file excluded after the first run stays on both sides and drops from state, an exclusion lifted later makes the file new (FR-040), the union of both sides excludes, a line error shows in the status but does not stop the run, an unwritable place returns `readOnly` from `ignore_set`. — Done 2026-10-29: `an_exclusion_added_later_leaves_the_file_alone_until_it_goes` in `run_tests.rs` with a bad line in the file; the plan forgot no excluded state row before, so FR-040 needed `forget_excluded` in `plan.rs` (test in `plan_tests.rs`). Union and `readOnly`: union in `ignore_tests.rs` (PR A), the read-only case through `manage_tests.rs` (`ignore_get` with a reason); the command maps a read-only place A to `readOnly`.
- [x] T054 [P] [US4] Preview tests in `commands_tests.rs`: `sync_rules_preview` changes nothing (no `put`, no state write), works for a draft without saving, reports `hashReads` and `ignored`. — Done 2026-10-29: the logic lives in a new `manage.rs` with tests in `manage_tests.rs` (no write on either place, hash reads and excluded files counted, conflict copies and deviations named); `commands_tests.rs` checks which state a preview starts from (stored rule, renamed draft, moved places, new draft).

### Implementation

- [x] T055 [US4] Log in a new `src-tauri/src/file_sync/log.rs` (`record_run`, `trim`, `runs`, `items`) used by `run.rs`; commands `sync_rules_runs` and `sync_rules_run_items`; log depth read through `storage::preferences` (`PrefScope::Device`, key `syncRules.logDepth`, default 10) with a validation clause in `validate_value` (integer 0..100); commands `sync_rules_log_depth_get/set` (set trims at once). — Done 2026-10-29: `run_log.rs` (`record`, `trim`, `set_depth`, `runs`, `items`); `run::note` writes result and log in one transaction, also for a run whose place could not be opened. Each log entry has its own id. The control loop passes the cause (`startup`, `manual` also for a new or changed rule, `watch`, `interval` also for a waiting rule that wakes). No `itemsTruncated`: every file is kept.
- [x] T056 [US4] Preview and ignore commands in `commands.rs`: `sync_rules_preview` (scan + plan, no execute), `sync_rules_ignore_get` (file at the source place, fixed exclusions, `ignoredNow`, `writable`), `sync_rules_ignore_set` (`Place::write_text`, `readOnly` when not writable). — Done 2026-10-29: the preview takes `id`, `draft` or both (a change of a stored rule keeps its state unless places or direction change). The editor works on place A, opened to read with the reason when it cannot be written; `ignore_set` runs the rule after. New codes `invalidDepth`, `placeError`, `invalidIgnore`.
- [x] T057 [P] [US4] Ignore editor in a new `src/components/files/SyncRuleIgnoreEditor.vue` (`UiTextarea`, fixed exclusions listed read-only, count of ignored files, line errors, read-only notice with reason); opened from the rule row ("Ausnahmen"). — Done 2026-10-29: `SyncRuleIgnoreEditor.vue`, opened from the rule's menu.
- [x] T058 [P] [US4] Preview and log views: `src/components/files/SyncRulePlan.vue` (steps grouped by action, counts, hash read count) and `src/components/files/SyncRuleLog.vue` (runs with counters, expandable per-file list, link to `.holzi-trash` entry or "Papierkorb des Systems" text); the create dialog gets the cloud hint (FR-031: an encrypted folder protects content) and a preview button; edit and pause actions in the rule row (Story 4, scenarios 5 and 6). — Done 2026-10-29: `SyncRulePlan.vue` and `SyncRuleLog.vue`; the rule row has a menu with preview, exclusions, log and edit. The dialog changes a stored rule too and previews what it holds. A rule trash entry shows its path, there is no link into the file browser yet. The picker cannot open at a stored place, so the dialog keeps a stored place until another folder is picked.
- [x] T059 [US4] Settings item "Sync-Protokoll": `subView('general.syncLog', 'general/sync-log', 'general', { icon: 'lucide:history', overviewRow: true, settingKeys: ['syncRules.logDepth'] })` in `src/lib/settings/registry.ts`, `src/components/settings/general/SyncLogView.vue` after `ExtensionLimitsView.vue` (number input, save on blur or Enter, invalid input falls back), route in `src/components/wm/appRoutes.ts`, path in `scripts/check-settings.ts` `LOCATION_PATHS`, i18n `settings.locations.general.syncLog.*`. — Done 2026-10-29: as described; the view is `src/components/settings/SyncLogView.vue` (settings views are not in a `general/` folder) and uses the depth commands so a smaller depth trims at once.
- [x] T060 [P] [US4] E2E (manage part): create a rule, preview, edit `.haex-ignore` through the editor, run, assert excluded files untouched; open the log and expand a run; set the depth to 2 in the settings and see older runs gone. — Done 2026-10-29: in `files-sync-rules.test.ts` with the one-way rule; editing the rule is not covered end to end.

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

## Phase 9: Rules list in the panel (FR-047, FR-048)

- [x] T074 [US4] The rules as a list in the file browser panel: `sr` in the tab location (`lib/files/registry.ts`, `split.ts`), `components/files/SyncRulesList.vue` replaces the dialog overview, the sidebar entry opens it in the active panel; sort and filter in `lib/files/syncRules.ts`, checked in `scripts/check-files-sync-rules.ts` and `check-files-registry.ts`.
- [x] T075 [US4] Progress of a running rule: `file_sync/progress.rs` with `progress_tests.rs` and `run_tests.rs`, bar in the list row.
- [x] T076 [US1] A run gives up when a place refuses (FR-049): `execute.rs` (`stops_the_run`, ten in a row, 200 named), `run.rs` (outcome waiting or error), tests in `run_tests.rs`; changes reach a rule with a network place only once per interval (FR-050), test in `schedule_tests.rs`.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none; T001–T003 are decision spikes that block the tasks naming them (T001 → T014, T002 → T009, T003 → T016); T007 before any code using `ignore`; T008 before any code.
- **Foundational (Phase 2)**: after Setup; blocks all stories. T009 → T010–T012; T011 before T012–T013; T014 before T015–T017; T018 and T019 can follow T011.
- **US1 (Phase 3)**: after Phase 2. T020–T021 before T022–T025; T023 before T025; T024 before T025; T026 after T025; T027 parallel to T026; T028 and T029 after T026; T030–T033 after T029; T034 last.
- **US2 (Phase 4)**: after US1 (executor, run, schedule) and the mirror-delete prerequisite T047. T035–T037 before T038–T039; T040 after T038; T041 after T039; T042 after T041; T043 only after T047.
- **US3 (Phase 5)**: T044–T046 before T047. The mirror-delete subset T044/T047 is included in PR C before two-way rules are exposed; the remaining pending/protection work (T048–T051) follows in PR D.
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
