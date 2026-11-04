# Implementation Plan: Name the two places of a sync rule "source" and "target" in code

**Branch**: `075-sync-rule-source-target` | **Date**: 2026-11-01 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/075-sync-rule-source-target/spec.md`

## Summary

A pure rename. Everything that calls the two places of a sync rule "A" and "B" says "source" and "target":
the rule and draft fields, the engine's sides and one-way direction, the stored columns and values, the
generated types, the agent tool schema, the translation keys, the test ids and the checks. The storage
reference inside a place, called `source` today, becomes `location`. One new migration, `0053_sync_rules_source_target`,
renames the columns and rewrites the stored side and direction values so that existing vaults keep their rules,
per-file state, pending deletions and run log. No alias for the old names anywhere else. Behaviour does not change;
every test keeps its assertions and only its names move. The research (R1 to R7) found no open question.

## Technical Context

**Language/Version**: Rust (workspace edition of `src-tauri`), TypeScript 5 with Vue 3 / Nuxt 3

**Primary Dependencies**: Tauri 2, `haex_crdt` (SQLite through `rusqlite`), `ts-rs` for the generated
bindings, `serde`. No new dependency.

**Storage**: SQLite, five `_no_sync` tables of migration `0044_sync_rules` (never tracked or synced, ADR 0018);
one new migration

**Testing**: `cargo test` (tests live in `*_tests.rs` files next to the module), the node-based checks
`pnpm check:*` (among them `check-files-sync-rules.ts`, `check-agent-actions.ts`), `pnpm typecheck`,
`pnpm typecheck:scripts`, the WebDriver E2E scenarios `files-sync-rules` and `agent-sync-rules` (they run on
`main` only, so the branch is dispatched by hand before the merge)

**Target Platform**: Desktop (Linux, macOS, Windows) and Android, one code base

**Project Type**: Desktop and mobile app (Tauri backend, Nuxt frontend)

**Performance Goals**: None; the migration touches only a handful of rows per vault

**Constraints**: Behaviour of rules unchanged (FR-005); no data lost for existing vaults (FR-004); no alias for
the old names in the agent tools (FR-006); the already-applied migration `0044` stays untouched

**Scale/Scope**: The module `src-tauri/src/file_sync/` (about 25 production files and their `*_tests.rs`), one
migration, 5 generated bindings, 6 Vue components, `lib/files/syncRules.ts`, 1 tool schema, 2 E2E scenarios,
1 check script, 2 translation files, and the docs of spec 061

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Gate                                                                                         | Source                                      | Result                                                                                                                                                                                                        |
| -------------------------------------------------------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| No secrets in git                                                                            | Spec Kit constitution I, spaex              | Pass: no secret or key material is touched                                                                                                                                                                    |
| No local absolute paths in versioned config                                                  | Spec Kit constitution II                    | Pass: only repository-relative paths in the spec and plan                                                                                                                                                     |
| Cross-repo references pin a revision                                                         | Spec Kit constitution IV                    | Pass: no cross-repository reference                                                                                                                                                                           |
| Self-modifying instructions are review-gated                                                 | Spec Kit constitution VI                    | Pass: the change goes through a pull request; no skill, constitution or permission changes                                                                                                                    |
| Work lands through a pull request from a topic branch, no squash-merge, Conventional Commits | Spec Kit constitution, Development Workflow | Pass: branch `075-sync-rule-source-target`; commits `refactor(file-sync): …`                                                                                                                                  |
| Speckit workflow for the primary task                                                        | spaex `speckit-workflow-adherence`          | Pass: specify, plan, tasks, implement; this plan is the second stage                                                                                                                                          |
| Smallest correct change that fits the crate's conventions                                    | spaex `coding-guideline-rust`, `ponytail`   | Pass: a mechanical rename, no new abstraction, no new dependency                                                                                                                                              |
| One runnable check per non-trivial logic                                                     | spaex `ponytail`                            | Pass: the migration gets a test in `migrations_sync_rules_source_target_tests.rs` (R4); the rest is covered by the existing tests under their new names                                                       |
| Fix the root cause everywhere, grep every caller                                             | spaex `ponytail`                            | Pass: R1 enumerates the sites by structure; SC-001 is a search for the old names                                                                                                                              |
| Graphify before raw search                                                                   | spaex `graphify-context-map`                | Partly: the one `graphify query` returned unrelated nodes (JavaScript fixtures); the survey then used exact-text and structural search, which is the right tool for a rename. Noted as a failed consultation. |
| Tests in separate files, never inline `#[cfg(test)] mod tests`                               | project convention                          | Pass: the new test file sits next to the migration module                                                                                                                                                     |
| Prettier on every Markdown and JSON touched                                                  | spaex `holzi-devshell-commands`             | Pass: part of the tasks                                                                                                                                                                                       |
| New `#[ts(export)]` names with a domain prefix; check i18n groups for collisions             | project convention                          | Pass: no new exported type; `files.syncRules.source` and `.target` are free (R6)                                                                                                                              |
| Do not introduce a new dependency; no speculative abstraction                                | spaex `ponytail`                            | Pass                                                                                                                                                                                                          |

**Post-design re-check**: unchanged, all pass. The one tension is the project's rule "no migration paths for holzi,
it has no users yet": FR-004 keeps one schema migration. It is not compatibility code, and the operator's own vault
and test vaults would otherwise lose their rules; see Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/075-sync-rule-source-target/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1 to R7
├── data-model.md        # Phase 1: every old name and its new name
├── quickstart.md        # Phase 1: how to check the result
├── contracts/
│   ├── storage-migration.md   # the migration 0053: SQL, order, test
│   └── wire-names.md          # commands, events, generated types, agent tool schema
└── tasks.md             # Phase 2 ($speckit-tasks, not created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── file_sync/
│   ├── model.rs                # SyncRuleSide, SyncRuleDirection, SyncRulePlace, SyncRuleDraft, SyncRule
│   ├── store.rs                # columns, the per-file Seen row, pending rows
│   ├── scan.rs  plan.rs  execute.rs  pending.rs  run.rs  run_log.rs  ignore.rs  # `Target` keeps its name
│   ├── rules.rs  manage.rs  commands.rs  schedule.rs  service.rs  agent.rs
│   └── *_tests.rs              # renamed with their module
├── identity/
│   ├── migrations_sync_rules.rs            # untouched (0044)
│   ├── migrations_sync_rules_tests.rs      # fill() gets the new names; the old ones stay as fill_before_0053
│   ├── migrations_sync_rules_source_target.rs        # new: SQL of 0053
│   ├── migrations_sync_rules_source_target_tests.rs  # new: migration test
│   └── migrations.rs                       # registers 0053
└── chat/eval/tools.json        # regenerated, not edited

src/
├── components/files/           # SyncRuleDialog, SyncRulesList, SyncRuleConfirmDialog, SyncRulePlan,
│                               # SyncRuleLog, SyncRulePending
├── lib/files/syncRules.ts      # SyncDirection, targetSides
├── lib/actions/syncRuleActions.ts   # the agent tool schema
├── i18n/locales/{en,de}.json   # files.syncRules.placeA/placeB/direction.aToB
└── types/bindings/             # regenerated by `cargo test`, then trailing blanks stripped

scripts/
├── check-files-sync-rules.ts   # the direction and target sides
└── e2e/scenarios/{files-sync-rules,agent-sync-rules}.test.ts   # drafts, test ids

specs/061-sync-rules/           # data-model.md, contracts/tauri-commands.md, research.md: FR-008
```

**Structure Decision**: The existing layout stays. The rename follows the code, and the only new files are the
migration and its test, next to the earlier sync rule migrations as the project does for every migration.

## Complexity Tracking

| Violation                                        | Why Needed                                                                                                                                                                                                                                                                           | Simpler Alternative Rejected Because                                                                                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A schema migration although holzi "has no users" | The operator's own vault and the test vaults hold rules. Renamed columns without a migration make the store fail to read them. The `side` and `direction` values in stored rows also have to be rewritten, or `parse::<SyncRuleSide>` returns `None` and the callers drop those rows | Editing the applied migration `0044` breaks every vault that already ran it. Dropping the old tables loses rules, state, pending deletions and the run log |
