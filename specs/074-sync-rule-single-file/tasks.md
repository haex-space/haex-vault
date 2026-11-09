---
description: 'Task list for spec 074-sync-rule-single-file'
---

# Tasks: Sync rules for single files

**Input**: Design documents from `/specs/074-sync-rule-single-file/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/storage-migration.md](./contracts/storage-migration.md),
[contracts/wire.md](./contracts/wire.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The spec's table is the test table: each user story starts with its tests, written to fail
first. Rust tests live in `*_tests.rs` next to the module (`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never
inline. Run tests use real folders (`tempfile`, `DevicePlace::with_rule_trash`) as the existing `run_tests.rs` does.

**Organization**: Grouped by user story (P1: US1, US2; P2: US3). **US1 and US2 ship together**: with only US1 built,
a one-way file rule whose source is gone would reach the folder planner, which reads "source deleted" as a
deletion to mirror. The guard that prevents it (T014) therefore sits in Phase 3 and comes before T015, which wires
file rules into a run. Its test and the rest of the invalid-source handling follow in Phase 4.

**Working notes**:

- Work in `.worktrees/074-sync-rule-single-file` (other sessions switch branches in the main checkout).
- `pnpm` runs through the devShell: `nix develop --command pnpm …`. Cargo runs like the CI, without the NDK compiler
  variables and inside the memory scope (memory notes `project-host-cargo-env-ndk-vars`, `project-cargo-oom-jobs`):
  `env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 cargo <cmd> --manifest-path src-tauri/Cargo.toml -j 2`.
  Clippy fits `-j 2`. The library test build needs `-j 1` and has been killed even then since the voice changes; if
  it is, the CI runs the Rust tests (say so in the pull request). A bigger scope needs the operator's agreement.
  The first build in a worktree needs `pnpm fetch:sherpa` (the archive of the `voice` feature).
- After every `cargo test`, run `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` (memory note
  `project-ts-rs-binding-drift`).
- **Do not change**: the folder planner's deletion branches (`Planner::file`, `both_ways`, `deleted_at_source`), the
  engine's `Step` list, `sync_copy`, the per-file state, the exclusion code in `ignore.rs`, and the target picker
  (folders only). A file rule is added around them, not into them.
- **Names**: the new exported type carries the domain prefix (`SyncRuleSourceKind`); the new i18n keys sit under
  `files.syncRules.file.*`, `files.syncRules.state.invalidSource` and `files.syncRules.error.{fileRule,invalidSource}`.
- Commits: Conventional Commits in English, no Claude trailer; never force-push; fetch before push.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [x] T001 Recheck the migration number: `git fetch origin`, then
      `git grep -n 'MigrationName::from("00[5-9]' origin/main -- src-tauri/src/identity/migrations.rs | tail -3` and
      `git grep -l '0060_' $(git branch -r | tr -d ' ') -- src-tauri/src/identity/migrations.rs`. If `0060` is taken,
      use the next free number and change it in plan.md, research.md R10 and contracts/storage-migration.md first.
- [x] T002 Merge `origin/main` into the branch (no rebase, no force-push) and get the baseline green on the merged
      tree, so a later failure is ours: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm check:files`,
      `pnpm check:agent-actions`, and `cargo clippy --all-targets -- -D warnings` in the 10 GB scope.

---

## Phase 2: Foundational (blocks every story)

**Purpose**: the stored kind of the source and the types that carry it. Commit T003 to T008 together: the Rust code
reads a column the migration adds, and every struct literal needs the new field.

- [x] T003 [P] Write the migration test `src-tauri/src/identity/migrations_sync_rules_source_kind_tests.rs` as
      [contracts/storage-migration.md](./contracts/storage-migration.md) describes, built like
      `migrations_sync_rules_source_target_tests.rs` (`migration_source_before`, `open`, the helpers of
      `migrations_sync_rules_tests.rs`): an old vault keeps every row and reads as folder rules, a second open changes
      nothing, a fresh vault has the column with default `folder`, a file rule written through `store` reads back as
      `file`. Add `#[cfg(test)] mod migrations_sync_rules_source_kind_tests;` to `identity/mod.rs`.
- [x] T004 [P] Create `src-tauri/src/identity/migrations_sync_rules_source_kind.rs` with the constant
      `SYNC_RULES_SOURCE_KIND_0060` (the one `ALTER TABLE … ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'folder'`),
      with a module comment like its sibling `migrations_sync_rules_source_target.rs`. Register it: `mod` in
      `identity/mod.rs`, `use` and `m.insert(MigrationName::from("0060_sync_rules_source_kind"), …)` after the last
      existing insert in `identity/migrations.rs`, with a comment naming spec 074.
- [x] T005 In `src-tauri/src/file_sync/model.rs` add the enum `SyncRuleSourceKind { Folder, File }` (derives and
      `#[ts(export, export_to = "../../src/types/bindings/")]` like its neighbours, `Default` is `Folder`), the field
      `source_kind: SyncRuleSourceKind` on `SyncRule`, the field `source_kind: Option<SyncRuleSourceKind>` on
      `SyncRuleDraft` (research R2: `None` is "not said"; add `#[ts(optional)]` if the generated `SyncRuleDraft.ts`
      does not read `sourceKind?: SyncRuleSourceKind`), `SyncRule::draft()` returns `Some(kind)`, and the value
      `SyncRuleOutcome::InvalidSource`. Add to `model_tests.rs`: the serde texts are `folder` and `file`; a draft
      without the field deserializes with `None`.
- [x] T006 In `src-tauri/src/file_sync/store.rs` read and write the column: `RULE_COLUMNS`, `insert_rule`,
      `update_rule` (the `UPDATE` lists the columns), and the row reader (`parse::<SyncRuleSourceKind>`; an unknown
      stored text skips the row, as for an unknown direction). The writers take the resolved kind
      (`draft.source_kind.unwrap_or_default()`; the caller resolves it, T016). `update_rule` counts a changed kind as a moved place (`moved` also compares `before.source_kind` with the new
      kind), so the per-file state starts afresh (spec FR-007). Extend `store_tests.rs`: a file rule round-trips; changing a file rule to a folder rule (and back) clears its state
      and pending rows, changing only the name does not.
- [x] T007 Add `source_kind: SyncRuleSourceKind::Folder` (or the value the test needs) to every struct literal of
      `SyncRule`, and `source_kind: None` (or `Some(…)`) to every literal of `SyncRuleDraft`. The compiler finds them;
      the files that build one are: `agent.rs` (`draft_of`), `commands.rs`, and the tests `agent_tests`,
      `commands_tests`, `execute_tests`, `manage_tests`, `pending_tests`, `plan_tests`, `rules_tests`, `run_log_tests`,
      `run_tests`, `schedule_tests`, `status_tests`, `store_tests` under `src-tauri/src/file_sync/`. Change no
      assertion.
- [x] T008 Regenerate the bindings. `cargo clippy` does not write them, `cargo test` does: build the tests where the
      memory allows. If the test build is killed, write the four bindings by hand from the Rust types (they are short)
      and let the CI's binding check confirm them. Run the trailing-space `sed`, and confirm these files changed or
      appeared in `src/types/bindings/`: `SyncRuleSourceKind` (new), `SyncRule`, `SyncRuleDraft`, `SyncRuleOutcome`.
      Commit T003 to T008: `feat(file-sync): store whether a rule's source is a file`.

**Checkpoint**: clippy is clean; the migration test and `store_tests` pass where the test build runs (else CI).

---

## Phase 3: User Story 1 - Keep one file in sync with another place (Priority: P1) 🎯

**Goal**: a rule with one file as the source and a folder as the target copies the file when it changes, and nothing
else.

**Independent Test**: create the rule, change the file, run it, see the new version in the folder under the same
name; run again, nothing is copied.

### Tests for User Story 1 (write first, they fail)

- [x] T009 [P] [US1] Write `src-tauri/src/file_sync/place_file_tests.rs` for `OneFilePlace` (added in T013): its
      listing holds the one file with the inner place's facts, or nothing when the file is missing or a folder lies
      at that name; `hash`, `rename`, `side` and `full_path` pass through with the name as the relative path;
      `read_text(".haex-ignore")` is `None` even when the folder has one; `check_root` follows the inner place.
      Use `DevicePlace::with_rule_trash` on a `tempfile` folder, and `MemPlace` from `test_support.rs` for a second
      inner place. Add the `#[cfg(test)] #[path = "place_file_tests.rs"] mod tests;` line to the new module in T013.
- [x] T010 [P] [US1] In `src-tauri/src/file_sync/run_tests.rs` add a `file_world` helper (a source folder with one
      file and other files, an empty target folder, a one-way file rule stored with `source_kind: File` and
      `delete_mode: Keep`) and the tests: the first run copies the file into the target folder under its name and
      copies nothing else; a second run with an unchanged source copies nothing (counts zero); a changed source is
      copied again and the log has the copy item; a different file already at the target on the first run becomes a
      conflict copy and nothing is lost (spec 061 FR-012a); other files of the source's folder stay out of the target;
      a paused file rule does not run (FR-002).
- [x] T011 [P] [US1] In `src-tauri/src/file_sync/rules_tests.rs` add: a device file rule validates when the source is
      a regular file; a folder, a missing path and a symbolic link as the source are `InvalidPlace`; a file under one
      of holzi's own places is `InvalidPlace` (spec 061 FR-002); the stored delete mode of a file rule is `Keep`; the
      overlap table of research R6 (same source file twice, file inside another rule's folder place, same copy twice,
      target folder equal to the file's folder, a copy inside another rule's folder place → rejected; two files from
      different folders into one target folder → accepted); folder rules overlap exactly as before.
- [x] T012 [P] [US1] In `src-tauri/src/file_sync/commands_tests.rs` add the resolution of a missing kind (research
      R2): `create` with `source_kind: None` stores a folder rule; `update` with `None` keeps a file rule a file rule;
      `update` with `Some(Folder)` on a file rule changes it; `create` with `Some(File)` stores a file rule.

### Implementation for User Story 1

- [x] T013 [US1] In `src-tauri/src/file_sync/place.rs` add `async fn check_root(&self) -> Result<(), FilesError>` to
      `Place` with the default `Ok(())`. Implement it in `place_device.rs` (the folder's `symlink_metadata` is a
      directory, else `unreachable`) and `place_storage.rs` (asks for the folder the way `list_tree` finds a missing
      root, else `unreachable`), with tests in `place_device_tests.rs` and `place_storage_tests.rs`. Create
      `src-tauri/src/file_sync/place_file.rs`: `OneFilePlace { inner: Box<dyn Place>, name: String }` implementing
      `Place` as research R1 and R5 say, and `split_file_path(location, path) -> (parent, name)` for a device path
      (`Path::parent`/`file_name`, so `\` works on Windows) and a window path (`/Fotos/a.jpg`). Declare the module in
      `file_sync/mod.rs`. T009 passes.
- [x] T014 [US1] **The guard, before T015.** In `src-tauri/src/file_sync/plan.rs`, for a file rule
      (`rule.source_kind == File`) whose source listing holds no file, `build` returns an empty plan (research R4,
      second line of defence). Add the test to `plan_tests.rs`: for one-way and two-way rules, with and without state,
      no step at all.
- [x] T015 [US1] In `src-tauri/src/file_sync/service.rs` add `open_rule_place(app, rule, side, want)`: for a folder
      rule it is `open_place_for` on that side's place; for a file rule it opens the parent folder of the source (or
      the target folder) and wraps it in `OneFilePlace` with the file's name (the target uses the source file's name).
      Use it at every call site that opened a rule's places: the run in `service.rs`, and in `commands.rs` the pending
      resolution, `sync_rules_preview` and `open_source`/`ignore_of`. In `run.rs::run_inner`, for a file rule call
      `target.check_root()` first and end an `Unreachable` as outcome `Waiting`, like an unreachable scan.
- [x] T016 [US1] In `src-tauri/src/file_sync/rules.rs` extend `validate` (research R6, data-model.md): for a file rule
      require a regular, non-link file for a device source and a file name in the path, set `delete_mode` to `Keep`,
      and judge overlap by the claims of each rule (a folder rule: its two folders; a file rule: the source file and
      the target path joined with the file's name). Keep `overlap` itself unchanged. In `commands.rs`, `create` and
      `update` resolve `draft.source_kind` before they call `validate` (`None` on create is `Folder`, `None` on update
      is the stored rule's kind); `validate` and the store then read `draft.source_kind.unwrap_or_default()`. T011
      and T012 pass.
- [x] T017 [US1] Folder picker and dialog. In `src/components/files/FolderPicker.vue` add the prop `allowFiles`: the
      list also shows files (not links, not holzi's own entries; file rows with the icon `lucide:file`, test id
      `files-picker-<name>-file-<name>`), a file row is taken at once as the choice, and `PickedFolder` gets
      `kind: 'folder' | 'file'` (`choose()` sets `folder`); the `encrypted` of a picked file is the flag of the open
      folder, so the hints of spec 061 FR-030a and FR-031 apply to it. In `src/components/files/SyncRuleDialog.vue`
      pass `allow-files` to the source picker only, send `sourceKind` in the draft, take the kind in
      `pickedOf(rule.source)` from `rule.sourceKind` and let an edit pick a folder or a file (FR-007: the kind follows the
      choice, `keepsStored` compares the kind too), show the file's name and icon in the chosen row, hide the delete
      mode field while the source is a file, and keep the rule name following the file's name. Add to
      `src/lib/files/syncRules.ts` the helpers the dialog needs (for example `showsDeleteMode(kind)`), and the
      translation keys of contracts/wire.md for the picker to `src/i18n/locales/en.json` and `de.json`.
- [x] T018 [P] [US1] Extend `scripts/check-files-sync-rules.ts` for the helpers of T017 (the delete mode is shown for
      a folder source only; `canSave` is unchanged for a file source) and add the case of spec US1.5: a picked file
      with `encrypted: true` and a target without it needs consent (`needsDecryptConsent`), and a target storage
      without encryption shows the cloud hint. Run `pnpm check:files`.
- [x] T019 [US1] In `src/components/files/SyncRulesList.vue` show a file rule with the file icon and the file's path as
      its place label (`placeLabel` of the source), unchanged for folder rules.
- [x] T020 [US1] Agents. In `src/lib/actions/syncRuleActions.ts` add the optional `sourceKind` to `DRAFT` as in
      contracts/wire.md and mention it in the `source` description; `src/components/files/SyncRuleConfirmDialog.vue`
      names the kind in the question. Run `nix develop --command pnpm export:eval-tools` and commit the regenerated
      `src-tauri/src/chat/eval/tools.json`; `pnpm check:agent-actions` passes. In `agent_tests.rs` add: a file rule
      created through the tool is stored as a file rule; the same call without `sourceKind` creates a folder rule; an
      `update` of a file rule without `sourceKind` keeps it a file rule (the case of finding A1).
- [x] T021 [US1] Watching (research R7). In `src-tauri/src/file_sync/watch.rs` add `watch_file(parent, name, notify)`:
      `watch_folder(parent, false, …)` that calls `notify` only for a batch with a change whose path is
      `parent/name`. In `service.rs::watch` use it for a file rule's source and, for a two-way rule, for the copy in
      the target folder; a storage place is not watched, as before. In `watch_tests.rs` add: a write to the file wakes
      the rule; a write to a sibling does not; replacing the file by renaming a new file over it wakes the rule.

**Checkpoint**: with T014 in place, a file rule can be created in the window and via the tool, runs one-way and
copies the file. Do not merge before Phase 4.

---

## Phase 4: User Story 2 - A renamed or moved source makes the rule's source invalid (Priority: P1)

**Goal**: with no file at the stored path, a run changes nothing and the rule shows "Quelle ungültig" with the
reason; the rule recovers when a file is there again.

**Independent Test**: rename, move and delete the source file in turn; the target is unchanged each time and the
state is invalid; put a file back and the next run continues.

### Tests for User Story 2 (write first, they fail)

- [x] T022 [P] [US2] In `place_file_tests.rs` add tests for `source_check` and `mount_root` (T027): a regular file is
      valid; a missing file with an existing parent is `noFile`, and so is a folder at the path (the place answers
      only for files); a missing parent is `folderGone`; a path under a missing mount root is an `Unreachable` error,
      while the same missing path under an existing root is `folderGone`. `mount_root` is a pure function tested on
      strings (`/media/anna/STICK/a/b.txt`, `/run/media/anna/STICK/…`, `/mnt/disk/…`, `/Volumes/Disk/…`,
      `D:\x\y.txt`, `/storage/1A2B-3C4D/…`, `/home/anna/x` has none); `source_check` takes the "does this root exist"
      test as a parameter so it can be tested without a drive.
- [x] T023 [P] [US2] In `run_tests.rs` add: renaming, moving and deleting the source file in turn ends each run with
      outcome `InvalidSource`, a reason key in the error, no copy and no deletion at the target, and the run is
      logged; a folder above the file renamed; a folder at the file's path (state `InvalidSource`, as for a missing
      file); the file back at the path → the next run is `Done` with no other step; the file replaced by renaming a
      new file over it → stays valid (US2 scenario 7); the rule edited to another file → valid and runs; deleting the
      rule changes neither the former source nor the copy; a missing target folder ends `Waiting` and deletes nothing
      (FR-022); an unreachable source place (a `MemPlace` that answers `Unreachable`) ends `Waiting`, not
      `InvalidSource` (FR-008). Run the invalid cases for a one-way and a two-way rule.
- [x] T024 [P] [US2] In `status_tests.rs` add: an enabled rule whose last outcome is `InvalidSource` has the state
      `InvalidSource` and the reason; a paused one stays `Paused`; a running one stays `Running`.
- [x] T025 [P] [US2] In `manage_tests.rs` add: the preview of a file rule whose source is gone is the error
      `InvalidSource`, and the preview of a valid file rule shows its one step.

### Implementation for User Story 2

- [x] T026 [US2] In `src-tauri/src/file_sync/status.rs` add `SyncRuleState::InvalidSource` and map
      `last_outcome == InvalidSource` to it in `status_of`; in `rules.rs` add `SyncRuleErrorCode::InvalidSource`.
      Regenerate the bindings and run the trailing-space `sed`; `SyncRuleState` and `SyncRuleErrorCode` are the two
      files that change. T024 passes.
- [x] T027 [US2] In `place_file.rs` add `mount_root(path) -> Option<PathBuf>`, the enum `SourceCheck` with the reason
      keys `noFile` and `folderGone`, and
      `async fn source_check(place, name, device_path, root_exists) -> Result<SourceCheck, FilesError>` as research R3
      defines: a missing file is `folderGone` when the parent's `check_root` fails, else `noFile`. T022 passes.
- [x] T028 [US2] Wire the check. In `scan.rs` add `ScanError::InvalidSource { reason }` and in `run.rs::run_inner`, for
      a file rule, run `source_check` on the source before `scan`: valid continues, invalid ends the run as
      `RunReport::stopped(SyncRuleOutcome::InvalidSource, <reason key>)`, `Unreachable` ends it `Waiting`. Do the same
      in `manage::preview`, and map the error in `commands.rs::scan_error` to `SyncRuleErrorCode::InvalidSource`.
      T023 and T025 pass.
- [x] T029 [US2] Frontend state. Add `'invalidSource'` to `SyncState` in `src/lib/files/syncRules.ts`, the keys
      `files.syncRules.state.invalidSource`, `files.syncRules.file.invalidReason.{noFile,folderGone}` and
      `files.syncRules.error.invalidSource` to both locale files, and in `SyncRulesList.vue` show the reason (the
      translated reason key) in the rule's details where the waiting and error states show theirs. Extend
      `check-files-sync-rules.ts` for `shownState` with the new state and run `pnpm check:files`.

**Checkpoint**: US1 and US2 together are shippable. Commit: `feat(file-sync): sync one file and show an invalid source`.

---

## Phase 5: User Story 3 - A file rule works both ways, and never deletes (Priority: P2)

**Goal**: two-way file rules bring a change of the copy back and handle conflicts; a missing copy is copied again; no
file rule deletes anything; the exclusion file does not apply.

**Independent Test**: change the copy, run, see the change in the source; delete the copy, run, see it back.

### Tests for User Story 3 (write first, they fail)

- [x] T030 [P] [US3] In `plan_tests.rs` add the table of a file rule: the target missing and the source there is one
      `Copy` with `replaces: false`, in both directions and whether or not the state knows the file; only the copy
      changed → two-way copies it back, one-way reports the deviation (as for a folder rule); both changed →
      conflict; equal first meeting → adopt; a loop over every combination of (source present, target present or
      missing, state known or not, direction) asserts the plan contains no `Delete`, `Pending`, `Keep` or `Forget`
      step that touches a file; a source named `.haex-ignore` in a two-way rule is a plain `Copy`, never
      `MergeIgnore`.
- [x] T031 [P] [US3] In `run_tests.rs` add: a one-way rule whose copy was changed at the target (source unchanged) leaves the copy and counts a deviation,
      and when the source changes later the changed copy is kept as a conflict copy (US1 scenario 6); two-way, only the copy edited → the source file has the new content; both
      edited → a conflict copy on both sides and neither version lost; the copy deleted → it is copied again, for a
      one-way and a two-way rule, and the run is `Done` (not `Waiting`); in every scenario of this phase and of
      Phase 4 both the source and the copy still exist at the end unless the scenario removed them itself; a file
      rule stored with delete mode `Mirror` (inserted directly) still deletes nothing.
- [x] T032 [P] [US3] In `commands_tests.rs` add: `ignore_get` and `ignore_set` of a file rule return the error
      `FileRule` and change nothing.

### Implementation for User Story 3

- [x] T033 [US3] In `plan.rs`, in `build`, add the branch of research R4 in front of the folder planner: for a file rule
      with the source file present and the target lacking it, the plan is one `Copy { from: Source, replaces: false }`
      and nothing else; in `Planner::both_ways` skip the `path == IGNORE_FILE` branch for a file rule. T030 passes.
- [x] T034 [US3] In `run.rs` skip the target side of `emptied` for a file rule (a deleted copy is not an emptied
      place). T031 passes.
- [x] T035 [US3] In `rules.rs` add `SyncRuleErrorCode::FileRule`, regenerate the bindings and run the trailing-space
      `sed` (`SyncRuleErrorCode` changes again). In `commands.rs` return it from `sync_rules_ignore_get` and `_set` for
      a file rule, before any place is opened. T032 passes. In the window: `SyncRulesList.vue` does not offer the
      exclusion action for a file rule, and `SyncRuleDialog.vue` has no exclusion hint to hide; add
      `files.syncRules.file.noExclusions` and `files.syncRules.error.fileRule` to both locale files.

**Checkpoint**: all three stories work; the spec's table has a test in every row.

---

## Phase 6: Polish and cross-cutting

- [x] T036 [P] Add the E2E scenario `scripts/e2e/scenarios/files-sync-file-rule.test.ts`, modelled on
      `files-sync-rules.test.ts` (folders from `deviceFiles`, the rule created through `sync_rules_create` with
      `sourceKind: 'file'`): the first run copies the file and not its neighbour; a change is copied; renaming the
      source makes the rule's state `invalidSource` (read through `sync_rules_list`) and leaves the target unchanged;
      putting the file back and running ends it; the copy deleted is copied again. The picker is not part of it: a
      temporary folder is not among the places of its start list, so the picker's file rows are covered by the hand
      check. It cannot run locally; dispatch the branch's CI (T040).
- [x] T037 [P] Docs: add a short "file rule" section to `specs/061-sync-rules/data-model.md` and
      `contracts/tauri-commands.md` (the new field, outcome, state and error codes, and a pointer to spec 074); run
      `pnpm exec prettier --write` on every Markdown and JSON file touched and `python3 scripts/ci/check-docs.py`.
- [x] T038 Run the local gates on the finished change: `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`
      (desktop); the Android clippy with `--no-default-features` (per-target NDK variables, memory note
      `project-host-cargo-env-ndk-vars`); the library tests with `-j 1` if the build fits, else leave them to the CI;
      `pnpm typecheck`, `typecheck:scripts`, `lint`, all `check:*`; `pnpm export:eval-tools` leaves no diff; the
      trailing-space `sed` on the bindings, and `git status` shows exactly six binding files changed or new
      (`SyncRuleSourceKind`, `SyncRule`, `SyncRuleDraft`, `SyncRuleOutcome`, `SyncRuleState`, `SyncRuleErrorCode`) and
      otherwise the files this list names.
- [x] T039 Before the pull request: `git fetch origin`; recheck the migration number (T001) against `origin/main`;
      merge `origin/main` if it moved; resolve the migration number if it was taken (rename the constant, the
      registration, the tests and the spec documents; add the old name to `identity/migrations_legacy_names.rs` only
      if a vault already ran it under that name). Push the branch (as the account with write access, then restore the
      account that was active).
- [ ] T040 Dispatch the CI on the branch (`gh workflow run ci.yml --ref 074-sync-rule-single-file`, E2E runs on `main`
      and on dispatch only) and read the result: the Rust checks, the Android jobs, and the scenarios
      `files-sync-rules`, `agent-sync-rules` and `files-sync-file-rule`. Failures that also fail on `main` are named as
      such in the pull request, with the run that shows it.
- [ ] T041 Pull request, only after the operator agrees (as `haex-space` through `gh auth switch`, back afterwards;
      fallback `gh api -X POST …/pulls` on the GraphQL bug). Body: what changed, the migration number, that US1 and US2
      ship together, the test table of the quickstart, which checks ran locally and which only in the CI, the open hand
      check (T042), the Android default-feature clippy if left to the CI. No Claude trailer. Merge with a merge commit
      or rebase-merge, never squash.
- [ ] T042 **Operator**: the hand check of [quickstart.md](./quickstart.md) with a build of this branch, including a
      vault from before the migration. An agent cannot tick this task.

---

## Dependencies and order

- Phase 1 → Phase 2 → Phase 3 and 4 (as one change) → Phase 5 → Phase 6.
- Inside Phase 2: T003 and T004 can start together, T005 needs nothing; T006 and T007 need T005; T008 needs T004 to
  T007.
- Inside Phase 3: T009 to T012 first; T013 before T014 and T015; **T014 (the guard) before T015**; T016 to T021 are
  independent of each other (different files) and need T005 to T008.
- Inside Phase 4: T022 to T025 first; T026 and T027 are independent; T028 needs T026 and T027; T029 needs T026.
- Inside Phase 5: T030 to T032 first; T033 to T035 are independent of each other (T035 touches `rules.rs`, which T016
  also touched: after T016).
- User stories 1 and 2 are the minimum to merge. Ship all three together: the dialog already offers both directions,
  so a file rule can be two-way from the first release.

## Parallel opportunities

- Phase 2: `T003` (test file) with `T004` (migration module).
- Phase 3: `T009` to `T012` (four test files); then `T016`, `T017`, `T020`, `T021` (rules and commands, the Vue files,
  the action and agent files, `watch.rs`).
- Phase 4: `T022` to `T025` (four test files).
- Phase 5: `T030`, `T031`, `T032`.
- Phase 6: `T036` and `T037`.

## Implementation strategy

1. Phase 1 and 2: the stored kind, committed alone as a green, behaviour-free change (every rule is a folder rule).
2. Phase 3 with the guard T014 first, then Phase 4, as one change: a file rule is only reachable once it is safe.
3. Phase 5 completes the spec; the CI run (T040) is the check that the window and the E2E scenarios agree with the
   Rust.
4. The migration number is the one thing that can change under us: recheck it at T001 and T039.

## Notes

- Each task names its file; each test task names the case in the spec it covers (the quickstart's table lists them).
- A test written for a story must fail before its implementation task and pass after it.
- Stop at a checkpoint to check the story on its own before going on.
