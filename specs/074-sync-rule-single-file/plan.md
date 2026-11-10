# Implementation Plan: Sync rules for single files

**Branch**: `074-sync-rule-single-file` | **Date**: 2026-11-09 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/074-sync-rule-single-file/spec.md`

## Summary

A rule may take one file as its source. The engine of spec 061 stays as it is: it already syncs two listings of
relative paths. A file rule hands it two one-entry listings, built by a thin wrapper (`OneFilePlace`) around the
folder that holds the file on each side, so planning, copying, conflicts, the per-file state and the run log
work unchanged. What is new is small and sits at the edges:

- the rule stores whether its source is a folder or a file (`source_kind`, one additive migration);
- a run checks the source file first and ends as "Quelle ungültig" (a new outcome and state) when no file is at
  the stored path, before it plans anything, so nothing is copied or deleted;
- the planner copies a missing copy again and never plans a deletion for a file rule;
- saving checks that the source is a file, and overlap is judged by the files a rule touches, so several file
  rules can fill one target folder;
- the watcher watches the parent folder for that one file;
- the folder picker offers files for the source place; the dialog, the list and the agent schema learn the kind.

Folder rules do not change. The research (R1 to R10) leaves no open question; R3 records one heuristic.

## Technical Context

**Language/Version**: Rust (workspace edition of `src-tauri`), TypeScript 5 with Vue 3 / Nuxt 3

**Primary Dependencies**: Tauri 2, `haex_crdt` (SQLite through `rusqlite`), `ts-rs` for the generated bindings,
`serde`, `async-trait`. No new dependency.

**Storage**: SQLite, the `_no_sync` table `haex_sync_rules_no_sync` (spec 061, ADR 0018): one added column, one new
migration

**Testing**: `cargo test` (tests in `*_tests.rs` next to the module), the node-based checks `pnpm check:*` (among
them `check-files-sync-rules.ts`, `check-agent-actions.ts`), `pnpm typecheck`, `pnpm typecheck:scripts`, the
WebDriver E2E scenarios (they run on `main` only, so the branch is dispatched by hand before the merge)

**Target Platform**: Desktop (Linux, macOS, Windows) and Android, one code base

**Project Type**: Desktop and mobile app (Tauri backend, Nuxt frontend)

**Performance Goals**: A file rule run does two `stat` calls and at most one copy; it never lists a folder. The
watcher of a file rule does not wake a run for a sibling file (SC-003 without a busy loop).

**Constraints**: Folder rules unchanged (FR-014, SC-004); a file rule deletes nothing in any state (FR-004, SC-002);
an unreachable place stays distinct from an invalid source (FR-008); the already-applied migrations stay untouched

**Scale/Scope**: `src-tauri/src/file_sync/` (one new module `place_file.rs`, changes in `model`, `store`, `plan`,
`run`, `rules`, `service`, `commands`, `status`, `watch`, `agent`), one migration, 6 generated bindings changed or
added, `FolderPicker.vue`, `SyncRuleDialog.vue`, `SyncRulesList.vue`, `lib/files/syncRules.ts`,
`lib/actions/syncRuleActions.ts`, the translation files, one E2E scenario

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Gate                                                                                         | Source                                      | Result                                                                                                                                                                                |
| -------------------------------------------------------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| No secrets in git                                                                            | Spec Kit constitution I, spaex              | Pass: no secret or key material is touched                                                                                                                                            |
| No local absolute paths in versioned config                                                  | Spec Kit constitution II                    | Pass: only repository-relative paths in the spec and plan; example paths in the quickstart are placeholders, not config                                                               |
| Cross-repo references pin a revision                                                         | Spec Kit constitution IV                    | Pass: no cross-repository reference                                                                                                                                                   |
| Self-modifying instructions are review-gated                                                 | Spec Kit constitution VI                    | Pass: the change goes through a pull request; no skill, constitution or permission changes                                                                                            |
| Work lands through a pull request from a topic branch, no squash-merge, Conventional Commits | Spec Kit constitution, Development Workflow | Pass: branch `074-sync-rule-single-file`; commits `feat(file-sync): …`                                                                                                                |
| Speckit workflow for the primary task                                                        | spaex `speckit-workflow-adherence`          | Pass: specify, plan, tasks, analyze, implement; this plan is the second stage                                                                                                         |
| Smallest correct change that fits the crate's conventions                                    | spaex `coding-guideline-rust`, `ponytail`   | Pass: one decorator, one stored kind, one planner branch; the engine keeps its shape (R1)                                                                                             |
| One runnable check per non-trivial logic                                                     | spaex `ponytail`                            | Pass: every decision R1 to R9 names its test (see tasks); the run tests use real folders                                                                                              |
| Fix the root cause everywhere, grep every caller                                             | spaex `ponytail`                            | Pass: R1 lists the call sites that open a rule's places and routes them through one function                                                                                          |
| Graphify before raw search                                                                   | spaex `graphify-context-map`                | Partly: one `graphify query` returned only spaex rule nodes, no Rust symbols (as for spec 075); the survey then read the `file_sync` module directly. Noted as a failed consultation. |
| Tests in separate files, never inline `#[cfg(test)] mod tests`                               | project convention                          | Pass: every new module has its `*_tests.rs`                                                                                                                                           |
| Prettier on every Markdown and JSON touched                                                  | spaex `holzi-devshell-commands`             | Pass: part of the tasks                                                                                                                                                               |
| New `#[ts(export)]` names with a domain prefix; check i18n groups for collisions             | project convention                          | Pass: the one new exported type is `SyncRuleSourceKind`; the new keys sit under `files.syncRules.file.*`, checked free (R9)                                                           |
| Adding a field or value must not break an old reader                                         | spec 075 R3                                 | Pass: `source_kind` has a column default and the draft's field is an `Option` (R2); a stored value an older build does not know is read as "no row", as for other unknown values (R2) |

**Post-design re-check**: unchanged, all pass. Two tensions, both recorded under Complexity Tracking: the project's
rule "no migration paths for holzi" against the additive migration, and the new `Place` method.

## Project Structure

### Documentation (this feature)

```text
specs/074-sync-rule-single-file/
├── spec.md
├── plan.md              # this file
├── research.md          # Phase 0: R1 to R10
├── data-model.md        # Phase 1: the new type, column, outcome, state, error code
├── quickstart.md        # Phase 1: the manual checks and the automatic ones
├── contracts/
│   ├── storage-migration.md   # the migration
│   └── wire.md                # commands, status, agent schema, translation keys
├── checklists/requirements.md
└── tasks.md             # Phase 2 output ($speckit-tasks command, not created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── file_sync/
│   ├── model.rs                 # + SyncRuleSourceKind; source_kind on SyncRule and SyncRuleDraft; SyncRuleOutcome::InvalidSource
│   ├── place.rs                 # + Place::check_root (default: Ok) (R3)
│   ├── place_device.rs          # check_root for a device folder
│   ├── place_storage.rs         # check_root for a storage folder
│   ├── place_file.rs            # new: OneFilePlace, source_check, mount_root heuristic (R1, R3, R5)
│   ├── place_file_tests.rs      # new
│   ├── plan.rs                  # file rule: copy a missing copy again, no deletion, plain .haex-ignore (R4)
│   ├── manage.rs                # preview: the source check (R3)
│   ├── scan.rs                  # ScanError::InvalidSource
│   ├── run.rs                   # file rule: source check first, no "emptied" guard for the copy (R3, R4)
│   ├── rules.rs                 # validate: the source is a file; overlap by claimed files (R6)
│   ├── store.rs                 # source_kind column
│   ├── status.rs                # SyncRuleState::InvalidSource
│   ├── service.rs               # open_rule_place: the one place that wraps a side of a file rule; watch (R1, R7)
│   ├── commands.rs              # pending, preview and ignore commands use open_rule_place; ignore refused for a file rule
│   ├── watch.rs                 # watch_file: the parent folder, filtered to one file (R7)
│   ├── agent.rs                 # draft_of carries source_kind
│   └── *_tests.rs               # extended: model, store, plan, run, rules, status, watch, agent
└── identity/
    ├── migrations_sync_rules_source_kind.rs        # new: SQL of the migration
    ├── migrations_sync_rules_source_kind_tests.rs  # new
    ├── migrations.rs, mod.rs                       # register it

src/
├── components/files/FolderPicker.vue     # allowFiles: files can be picked (R8)
├── components/files/SyncRuleDialog.vue   # the kind from the picker; no delete mode for a file; no exclusions hint
├── components/files/SyncRulesList.vue    # file icon, "Quelle ungültig", no exclusions action
├── components/files/SyncRuleConfirmDialog.vue  # the agent's question names a file source
├── lib/files/syncRules.ts                # kind helpers
├── lib/actions/syncRuleActions.ts        # sourceKind in the draft schema (R9)
├── i18n/locales/{en,de}.json
└── types/bindings/                       # SyncRuleSourceKind (new); SyncRule, SyncRuleDraft, SyncRuleOutcome, SyncRuleState, SyncRuleErrorCode

scripts/
├── check-files-sync-rules.ts             # kind helpers
└── e2e/scenarios/files-sync-file-rule.test.ts  # new
```

**Structure Decision**: The existing layout of `file_sync/` is kept. The wrapper lives next to the two place
implementations because it is a `Place`. Nothing moves.

## Complexity Tracking

| Violation                                                      | Why Needed                                                                                                                                            | Simpler Alternative Rejected Because                                                                                                                                              |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| One migration, against "no migration paths for holzi" (075)    | A rule has to remember whether its source is a file. The column is additive with a default, so existing rules read as folder rules unchanged.         | Inferring the kind at run time cannot tell a renamed file from a missing folder, which is the whole point of FR-006. Encoding it in the path (a marker) would be a hidden format. |
| A new `Place::check_root` method with a default implementation | The target folder of a file rule is never listed (that would walk a large folder for one file), but a missing target folder must still wait (FR-022). | Listing the target and filtering would read the whole folder at every run. A `stat` of the copy answers "missing copy" but not "missing folder".                                  |
