---
description: 'Task list for spec 075-sync-rule-source-target'
---

# Tasks: Name the two places of a sync rule "source" and "target" in code

**Input**: Design documents from `/specs/075-sync-rule-source-target/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/storage-migration.md](./contracts/storage-migration.md),
[contracts/wire-names.md](./contracts/wire-names.md), [quickstart.md](./quickstart.md)

**Tests**: Included where logic is new: the migration (SC-003) and the refusal of old names (SC-004). Every
existing test keeps its assertions and only its names move (SC-002). Rust tests live in `*_tests.rs` next to the
module (`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline.

**Organization**: Grouped by user story (P1: US1, US2; P2: US3). The rename is one atomic change: the stored columns,
the Rust that reads them, the generated types and the frontend must agree, so Phase 2 and Phase 3 are committed in
the order below but merged in one pull request.

**Working notes**:

- Work in `.worktrees/075-sync-rule-source-target` (other sessions switch branches in the main checkout).
- Run everything through the devShell: `nix develop --command pnpm …`, and cargo as
  `nix develop --command scripts/with-nix-host-bridge.sh cargo …`.
- **Building on this machine** (Pop!_OS, found while doing T002): the Nix host bridge fails here (`javascriptcore-rs-sys` needs the
  Arch host's webkit), and the shell exports `CC`, `CXX` and `AR` pointing at the Android NDK, which breaks the host build of
  `llama-cpp-sys-2`. Build like the CI instead, without those three variables and inside the memory scope:
  `env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 cargo <cmd> --manifest-path src-tauri/Cargo.toml -j 4`.
  `pnpm` commands still run through `nix develop`. The first build in a worktree fills an empty `target/` (clippy about 6 GB, the test build more).
- **Cargo and memory**: the machine has 31 GB and the library test build alone has exceeded a 10 GB scope since
  2026-10-31 (clippy still fits). Run clippy in the 10 GB scope. The library test build fits in the 10 GB scope only with
  `-j 1` (`-j 2` was OOM-killed on 2026-11-02, `-j 1` finished in 7 minutes on a warm cache); use `-j 1` for `cargo test`. A larger scope or a full
  workspace build needs the operator's agreement.
- **Do not rename**: `rules.rs::overlap(a, b)` and its locals (a symmetric helper that compares two places of two
  _different_ rules, not a rule's source and target), `files/agent` (`place_at`, `Place`), `placeAsync`, the split
  view's `side: 'left' | 'right'`, the type name `Target` in `execute.rs` (only its fields change), the type
  `SourceRef`, and the historic `specs/061-sync-rules/tasks.md`.
- A new name must never shadow an old one: before renaming a local or parameter to `source` or `target`, check the
  scope for an existing `source`/`target` (a `SourceRef`, a place) and rename that one first.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [x] T001 Record the baseline: run the SC-001 search of [quickstart.md](./quickstart.md) §1 on the untouched tree and
      keep the numbers for the PR description (expected 49 files, 351 hits); run `git fetch origin` and confirm that
      no branch and no `origin/main` uses the migration number `0050`
      (`git grep -l '0050_' $(git branch -r | tr -d ' ') -- src-tauri/src/identity/migrations.rs`); if one does, take
      the next free number and change it in plan.md, research.md R4 and contracts/storage-migration.md first.
- [x] T002 Baseline green on the untouched tree, so a later failure is ours: `pnpm typecheck`, `pnpm typecheck:scripts`,
      `pnpm lint`, `pnpm check:files`, `pnpm check:agent-actions`, and `cargo clippy --manifest-path
src-tauri/Cargo.toml --all-targets` in the 10 GB scope.

---

## Phase 2: Foundational (blocks every story)

**Purpose**: the schema the renamed Rust code will read. Do not commit this phase alone: the app would query columns
that no longer exist. Commit it together with T008 (`store.rs`).

- [x] T003 [P] Write the migration test in `src-tauri/src/identity/migrations_sync_rules_source_target_tests.rs`, built
      like `migrations_sync_rules_tests.rs` (`migration_source_before`, `open`, the `fill()` pattern, the `#![allow(clippy::disallowed_methods)]`
      header it uses for raw SQL): an existing vault with a rule (direction `aToB`), a state row, a pending deletion on
      side `b`, a run and a run item on side `b` → after `0053` read the rows with the **new** column names and assert
      equal counts per table, `sourceToTarget`, the state numbers in `source_*`/`target_*`, the pending deletion and the run
      item on `target`; a pending deletion on `a` ends on `source`; a `side` value `x` stays `x`; a fresh vault has the
      new columns and no `place_a_source`, `a_size`; opening the migrated vault a second time with `holzi_migration_source()` succeeds and
      changes nothing (the migration runs once); the rows keep their order: assert the `seq` of the run items and the primary keys of
      the state and pending rows before and after. (Reader and zero-step tests come in Phase 4.)
- [x] T004 Create `src-tauri/src/identity/migrations_sync_rules_source_target.rs` with the constant
      `SYNC_RULES_SOURCE_TARGET_0053`: the eleven statements of [contracts/storage-migration.md](./contracts/storage-migration.md),
      separated by `--> statement-breakpoint`, with a module doc in the style of `migrations_sync_rules_decrypt.rs`
      (spec 075, tables are `_no_sync`, `HOLZI_TRIGGER_VERSION` stays).
- [x] T005 Register it: in `src-tauri/src/identity/mod.rs` add `mod migrations_sync_rules_source_target;` and
      `#[cfg(test)] mod migrations_sync_rules_source_target_tests;` next to the `migrations_sync_rules` lines (72–75); in
      `src-tauri/src/identity/migrations.rs` add the `use` and an
      `m.insert(MigrationName::from("0053_sync_rules_source_target"), …)` after `0052_tts_voices`, with a comment in
      the style of its neighbours ("Spec 075: …"). Verify that the migration runner accepts `RENAME COLUMN` on these `_no_sync`
      tables (T003 passes).

- [x] T006 Keep the existing 0044 tests working after `0053`: in `src-tauri/src/identity/migrations_sync_rules_tests.rs` the helper `fill()`
      inserts with the old names (`place_a_source`, `place_a_path`, `place_b_*`, `a_size`, `a_mtime_ms`, `b_*`, direction `'aToB'`, sides
      `'b'`). Rename the current helper to `fill_before_0053` and keep it for `migration_0046_adds_the_decrypt_switch_off_for_existing_rules`,
      which fills a vault at the schema before 0046 (old names are right there) and then upgrades it through every later migration. Give
      `fill()` the new names (`source_location`, `source_path`, `target_location`, `target_path`, `source_size`, `source_mtime_ms`,
      `target_size`, `target_mtime_ms`, `'sourceToTarget'`, sides `'target'`) for `deleting_a_rule_takes_its_state_pending_and_log_along` and
      `deleting_the_device_takes_its_rules_along`, which run on the fully migrated vault. No assertion changes (SC-002); the applied
      migration `migrations_sync_rules.rs` stays untouched. This file keeps old names on purpose (`fill_before_0053`), as T035 expects.

**Checkpoint**: T003 and T006 pass against the new migration. They need the library test build, so ask the operator for the scope first (see the working notes); run only the two migration test modules (`migrations_sync_rules_source_target` and `migrations_sync_rules`).

---

## Phase 3: User Story 1 - The code says source and target (Priority: P1) 🎯 MVP

**Goal**: No name in code, generated types, translations, test ids or checks calls a place "A" or "B".

**Independent Test**: the SC-001 search finds nothing outside the migrations of spec 061 and 075 and their tests; clippy, typecheck, lint and all
`check:*` pass; existing tests pass under their new names.

### Rust: model and storage (first, the rest depends on the types)

- [x] T007 [US1] `src-tauri/src/file_sync/model.rs` + `model_tests.rs`: `SyncRuleSide::{A, B}` → `{Source, Target}` (and
      `other()`), `SyncRuleDirection::AToB` → `SourceToTarget`, `SyncRulePlace.source` → `location`,
      `SyncRuleDraft`/`SyncRule` `place_a`/`place_b` → `source`/`target`, `SyncRule::draft()` and the side accessor
      (`place(side)`); update the doc comments (the draft's "The source; its `.haex-ignore`…" stays, it is now literally true).
- [x] T008 [US1] `src-tauri/src/file_sync/store.rs` + `store_tests.rs`: `RULE_COLUMNS`, `insert_rule`, `update_rule`
      (including the `moved` comparison), `rule_of_row` (`place` closure builds `SyncRulePlace { location, path }`),
      the per-file row's `a`/`b` → `source`/`target` and `seen(side)`, `state`/`put_state` columns
      (`source_size`, `source_mtime_ms`, `target_size`, `target_mtime_ms`), `PendingRow`; every SQL string uses the
      new column names of [data-model.md](./data-model.md).

### Rust: engine and callers (parallel once T007–T008 are in)

- [x] T009 [P] [US1] `src-tauri/src/file_sync/scan.rs` + `scan_tests.rs`: `Scanned.a/.b` → `.source/.target`, `scan()` parameters
      and locals (`listed_a`, `listed_b`).
- [x] T010 [P] [US1] `src-tauri/src/file_sync/plan.rs` + `plan_tests.rs` (the largest, 49 test hits): `Inputs.a/.b` →
      `.source/.target`, the side matches, `excluded_count(exclusions, a, b)` parameters, `Planner` accessors,
      `SyncRuleDirection::SourceToTarget`.
- [x] T011 [P] [US1] `src-tauri/src/file_sync/execute.rs` + `execute_tests.rs`: `Target`'s fields `a`, `b`, `listing_a`,
      `listing_b` → `source`, `target`, `source_listing`, `target_listing` (type name stays), the side matches, `row.a`/`row.b`.
- [x] T012 [P] [US1] `src-tauri/src/file_sync/pending.rs` + `pending_tests.rs`: side matches, `listing_a`/`listing_b`,
      parameters `a`, `b`.
- [x] T013 [P] [US1] `src-tauri/src/file_sync/run.rs` + `run_tests.rs`, `run_log.rs` + `run_log_tests.rs`:
      `listing_a`/`listing_b`, `scanned.a`/`scanned.b`, the `[(Side::A, …), (Side::B, …)]` arrays, and the side values
      (de)serialised through `stored`/`parse`. `run_log.rs` has no pattern hit but uses `SyncRuleSide`: read it.
- [x] T014 [P] [US1] `src-tauri/src/file_sync/ignore.rs` + `ignore_tests.rs`, `manage.rs` + `manage_tests.rs`: parameters
      `a`, `b` of the scan and preview calls, `ignored_now`, the editor's side (`the_editor_writes_place_a_within_the_limits`
      → `…_source_…`).
- [x] T015 [P] [US1] `src-tauri/src/file_sync/rules.rs` + `rules_tests.rs`: validation of `draft.source`/`draft.target`
      (the overlap check crosswise); **leave** `overlap(a, b)` and its locals as they are (see the working notes).
- [x] T016 [P] [US1] `src-tauri/src/file_sync/commands.rs` + `commands_tests.rs`: `rule.place_a`/`place_b`, locals `a`, `b`,
      `open_a` → `open_source`, `open_place_for(…, &rule.target, …)`.
- [x] T017 [P] [US1] `src-tauri/src/file_sync/service.rs`, `schedule.rs` + `schedule_tests.rs`, `status_tests.rs`: the watcher's
      `watch_place(&rule.source…)`, the pair `(a, b)`, test fixtures `place_a`/`place_b`. The compiler also found `watch.rs`
      (`place.source` is the storage reference, now `place.location`), `watch_tests.rs` and `perf_tests.rs` (`w.a`), which the text search
      cannot find; the clippy gate T019 is what catches such files.
- [x] T018 [P] [US1] `src-tauri/src/file_sync/agent.rs` + `agent_tests.rs`: `draft_of` and the fixtures; the tool schema itself
      is T030.
- [x] T019 [US1] Compile gate: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets` in the 10 GB scope, then the
      same for `--target aarch64-linux-android --no-default-features -- -D warnings` (`file_sync` has `cfg` branches; done for the
      `no-default` variant only, the `default` variant cross-builds llama.cpp and runs in the CI). Fix every error; then read each changed
      function once for a shadowed `source`/`target` that compiled anyway.
- [x] T020 [US1] Regenerate the bindings: ask the operator for a scope that fits the library test build, run
      `cargo test --manifest-path src-tauri/Cargo.toml file_sync`, then
      `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`. `git diff --stat src/types/bindings` must show exactly the five
      files `SyncRule`, `SyncRuleDraft`, `SyncRulePlace`, `SyncRuleSide`, `SyncRuleDirection` (`SyncRulePending`, `SyncRulePlanStep` and
      `SyncRuleRunItem` only name `SyncRuleSide` and stay unchanged). Never edit them by hand.

### Frontend (parallel after T020)

- [x] T021 [P] [US1] `src/lib/files/syncRules.ts` (`SyncDirection = 'sourceToTarget' | 'both'`, `targetSides` returning
      `'source' | 'target'`, any `aToB` default) and `scripts/check-files-sync-rules.ts` (lines 29, 56, 57 and the rest of the
      file by the search).
- [x] T022 [P] [US1] `src/components/files/SyncRuleDialog.vue`: refs `placeA`/`placeB` → `source`/`target`, `hasA`/`hasB` →
      `hasSource`/`hasTarget`, `labels: { source, target }`, `keepsStored`/`labelOf`/`pickerOf` side arguments, picker
      `name="source"`/`"target"` (test ids `files-picker-source-root`, `files-picker-target-root`), the draft
      `{ source: { location, path }, target: … }`, `'aToB'` defaults, `files.syncRules.source`/`.target` keys.
- [x] T023 [P] [US1] `src/components/files/SyncRulesList.vue` (`labelsOf` → `{ source, target }`, `rule.source`/`rule.target`,
      `ARROWS[...]`, `direction.${…}` key), `SyncRulePlan.vue` and `SyncRuleLog.vue` (`labels: { source: string; target: string }`),
      `SyncRulePending.vue` (`Record<SyncRuleSide, string>`: check it still type-checks and that its callers pass the new keys),
      `SyncRuleConfirmDialog.vue` (fields `source`/`target`, test ids `files-sync-rule-confirm-source`/`-target`).
- [x] T024 [P] [US1] `src/i18n/locales/en.json` and `de.json`: `files.syncRules.placeA`/`placeB` →
      `source`/`target`, `files.syncRules.direction.aToB` → `sourceToTarget`; the texts stay ("Quelle", "Ziel", "Quelle → Ziel").
      The keys `files.syncRules.source`/`.target` are free (checked 2026-11-01); re-check with a search before renaming.
- [x] T025 [P] [US1] `scripts/e2e/scenarios/files-sync-rules.test.ts`: the `draft()` helper (`source`/`target` with
      `{ location: { kind: 'device' }, path }`, `'sourceToTarget'`), the test ids at lines 275–276.
- [x] T026 [US1] Frontend gate: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm check:files`,
      `pnpm check:templates`, then every other `pnpm check:*` (the composables run in a sandbox without DOM globals).

**Checkpoint**: US1 is done when the SC-001 search finds nothing outside the migrations and their tests and T019/T026 are green.

---

## Phase 4: User Story 2 - Existing rules and their state survive (Priority: P1)

**Goal**: A vault from before the change opens with every row, every rule unchanged, and no extra work planned.

**Independent Test**: the migration tests pass, and the by-hand check M1 shows the same rules, the same pending deletion
and a plan of 0 steps for a rule that had nothing to do.

- [x] T027 [US2] Read the vault back through the Rust readers after `0053`: `store::rules`, `store::pending`, `store::state` and
      `run_log::items` and assert the rows come back (a dropped row, `parse` returning `None`, fails here), with the rule's
      `source`/`target` places, `SourceToTarget`, the pending deletion on `SyncRuleSide::Target`, the run item on `Target`. These readers
      take `&mut impl Query`, which only `CrdtTransaction` and `Reader` implement (`storage/query.rs`), so the migration tests' raw
      `with_connection` is not enough: first look at how `file_sync/store_tests.rs` gets its `Query` (`vault()` from
      `remote_storage::test_support`, `VaultDb`) and use the same access. The pre-0053 vault is made by opening once with
      `migration_source_before("0053_sync_rules_source_target")`, filling with `fill_before_0053` (T006), and reopening with
      `holzi_migration_source()`, as the 0046 test does. If `vault()` cannot start from an older migration source, put this test in
      `migrations_sync_rules_source_target_tests.rs` and read through a `Reader` of its `Database`.
- [x] T028 [US2] In the test module of T027 (same access to the vault): a rule whose files are equal on both sides, with the state rows read after `0053`, gives a
      plan with no step (`plan::build(&Inputs)` with matching listings and the migrated state; FR-005, SC-003 "plans 0 steps"). Build the
      listings from the same sizes and times as the state rows.
- [ ] T029 [US2] **By hand, operator** (quickstart §4, M1): open a vault made by the previous build (a one-way rule, a two-way
      rule with delete mode "ask", a pending deletion) with this build; both rules show the same source, target and direction,
      the pending deletion is still listed on the same place, and running the one-way rule shows a plan of 0 steps. Cannot be
      ticked by an agent.

**Checkpoint**: T027–T028 green; M1 reported by the operator before the merge.

---

## Phase 5: User Story 3 - Agents use the new names (Priority: P2)

**Goal**: The tool schema says `source` and `target`; the old names are refused.

**Independent Test**: `pnpm check:agent-actions` passes; a call with `placeA` and `placeB` fails with an invalid-input error and
the rule list is unchanged.

- [x] T030 [US3] `src/lib/actions/syncRuleActions.ts`: `PLACE` → `{ location, path }` with `required: ['location', 'path']`, the `DRAFT`
      properties `source`/`target`, the direction `enum: ['sourceToTarget', 'both']`, the descriptions as in
      [contracts/wire-names.md](./contracts/wire-names.md), the `required` list, and the `VIEW` schema's `rule.source`/`rule.target`.
- [x] T031 [US3] `pnpm export:eval-tools` to regenerate `src-tauri/src/chat/eval/tools.json` (never by hand); `git diff` shows only the
      `placeA`/`placeB`/`aToB` lines changing; `pnpm check:agent-actions` passes.
- [x] T032 [US3] In `src-tauri/src/file_sync/agent_tests.rs`: a create call with the new names works; the same call with `placeA`
      and `placeB` returns an invalid-input error and `store::rules` is unchanged (FR-006, SC-004).
- [x] T033 [US3] `scripts/e2e/scenarios/agent-sync-rules.test.ts` (renamed; the old-names step was **not** added: the scenario cannot run here and an
      unverified step could break it, so the refusal is covered by the Rust test T032 and the PR says so): the draft at lines 95–97 uses `source`/`target` with `location` and
      `'sourceToTarget'`; the test ids at lines 145 and 149 become `files-sync-rule-confirm-source`/`-target`; first read how the
      scenario's call helper surfaces a tool error. If it returns the error, add one step that sends the old names and expects the
      refusal. If it does not, do not extend the helper (out of scope) and say in the PR that the refusal is covered by T032 only.

**Checkpoint**: T031 and T032 green.

---

## Phase 6: Polish and cross-cutting

- [x] T034 [P] Docs, FR-008: `specs/061-sync-rules/data-model.md` (7 hits), `contracts/tauri-commands.md` (2),
      `research.md` (2): rename the code names, keep the prose; leave `specs/061-sync-rules/tasks.md` (history).
- [x] T035 SC-001: run the search of [quickstart.md](./quickstart.md) §1 with `--glob '!specs/061-sync-rules/tasks.md'`: the only hits are
      `file_sync/agent_tests.rs` (the refusal test sends the old names on purpose), `identity/migrations_sync_rules.rs`, `identity/migrations_sync_rules_tests.rs` (the applied `0044` and its test) and the new
      migration with its test; `rg -n "picker-(a|b)-root|confirm-place(A|B)" src scripts` finds nothing.
- [x] T036 `pnpm exec prettier --write` on every Markdown and JSON file touched (`specs/075-sync-rule-source-target`,
      `specs/061-sync-rules/*.md`, `src/i18n/locales/*.json`; `tools.json` comes out formatted from T031); `pnpm format:check` passes.
- [x] T037 Final gates, in order: clippy desktop and `aarch64-linux-android` (10 GB scope); `cargo test` for `file_sync` and
      `identity::migrations_sync_rules` (scope agreed with the operator; the timing tests of `contacts::phone::acceptance_tests` may
      flake under load and are unrelated); `pnpm typecheck`, `typecheck:scripts`, `lint`, every `check:*`, `format:check`.
- [ ] T038 Commit in the order Phase 2 + T008 together first, then the Rust engine, bindings, frontend, agent, docs; Conventional
      Commits in English (`refactor(file-sync): …`, `feat(identity): …` for the migration), **no** Claude trailer or "Generated
      with" line (the user's global rule). Branch has no upstream (`--no-track`); `git log origin/main..HEAD` and
      `git diff --stat origin/main HEAD` before the push; fetch first, never force-push (the operator may push review fixes).
- [ ] T039 Pull request, only after the operator agrees: `gh` as `haex-space` (switch back afterwards; if `gh pr create` hits the GraphQL
      bug use `gh api -X POST …/pulls`), body with the SC-001 numbers from T001 and T035, the migration note, and the open M1 check.
      Then `gh workflow run ci.yml --ref 075-sync-rule-source-target` for the E2E scenarios `files-sync-rules` and `agent-sync-rules`
      (E2E does not run on pull requests), and read `gh pr checks` before reporting "ready". Merge by rebase-merge or merge-commit, never squash (Spec Kit constitution,
      Development Workflow).

---

## Dependencies and order

- Phase 1 → Phase 2 → Phase 3 (T007, T008 first) → Phase 4 and Phase 5 (independent of each other) → Phase 6. Phase 4 needs Phase 3.
  In Phase 5 only T032 and T033 need Phase 3; T030 and T031 are TypeScript-only and need nothing but Phase 1.
- T008 needs T004–T005 (the new column names exist). T009–T018 need T007 and T008, and are parallel with each other.
- T019 needs T009–T018. T020 needs T019 (it needs a compiling test build). T021–T025 need T020. T026 needs T021–T025.
- T027–T028 need T007–T010 and T013 (and T006 for `fill_before_0053`). T030–T031 need nothing from Phase 3; T032 needs T018; T033 needs
  T023 (the confirm dialog's test ids) and T030.
- US1 is the MVP: with US1 and the migration the rename is complete; US2's extra tests and US3's schema are what make it safe
  to merge, so all three ship together.

## Parallel example

After T008, in parallel: T009, T010, T011, T012, T013, T014, T015, T016, T017, T018 (ten files with their tests, no shared
file). After T020: T021, T022, T023, T024, T025.

## Implementation strategy

1. Phase 1–2: the migration with its test (T003 first, then T004–T006).
2. Phase 3: types and storage first (T007–T008), then the engine in parallel, compile and clippy (T019), regenerate (T020),
   then the frontend.
3. Phase 4–5: reader and zero-step tests, the agent schema and its refusal test.
4. Phase 6: docs, search, format, gates, commits, pull request, E2E dispatch.
