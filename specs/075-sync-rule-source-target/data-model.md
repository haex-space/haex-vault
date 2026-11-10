# Data model: the old names and the new ones

No entity, field or state is added or removed. Each row is one rename; A becomes source, B becomes target.

## Rust types (`src-tauri/src/file_sync/model.rs`)

| Old                                         | New                                 | Wire form (serde `camelCase`)  |
| ------------------------------------------- | ----------------------------------- | ------------------------------ |
| `SyncRuleSide::A`                           | `SyncRuleSide::Source`              | `"a"` to `"source"`            |
| `SyncRuleSide::B`                           | `SyncRuleSide::Target`              | `"b"` to `"target"`            |
| `SyncRuleDirection::AToB`                   | `SyncRuleDirection::SourceToTarget` | `"aToB"` to `"sourceToTarget"` |
| `SyncRuleDirection::Both`                   | unchanged                           | `"both"`                       |
| `SyncRulePlace.source: SourceRef`           | `SyncRulePlace.location: SourceRef` | `source` to `location`         |
| `SyncRulePlace.path`                        | unchanged                           | `path`                         |
| `SyncRuleDraft.place_a`, `SyncRule.place_a` | `source`                            | `placeA` to `source`           |
| `SyncRuleDraft.place_b`, `SyncRule.place_b` | `target`                            | `placeB` to `target`           |

`SyncRuleSide::other()` keeps its meaning: `Source` gives `Target` and back.

## Engine fields and parameters (`src-tauri/src/file_sync/`)

| Where                                                                          | Old                                                                                      | New                                                                                                |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `scan.rs` `Scanned`                                                            | `a`, `b`                                                                                 | `source`, `target`                                                                                 |
| `plan.rs` `Inputs`                                                             | `a`, `b`                                                                                 | `source`, `target`                                                                                 |
| `execute.rs` `Target`                                                          | `a`, `b`, `listing_a`, `listing_b`                                                       | `source`, `target`, `source_listing`, `target_listing`                                             |
| `run.rs` `RunContext` and calls                                                | `listing_a`, `listing_b`, `scanned.a`, `scanned.b`                                       | `source_listing`, `target_listing`, `scanned.source`, `scanned.target`                             |
| `store.rs` per-file row (`Seen` pair)                                          | `a`, `b`                                                                                 | `source`, `target`                                                                                 |
| `store.rs` `rule_of_row`, `execute.rs`, `pending.rs`, `ignore.rs`, `manage.rs` | locals and parameters `place_a`, `place_b`, `a`, `b`, `a_side`, `listing_a`, `listing_b` | `source`, `target`, `source_side`, `source_listing`, `target_listing`                              |
| `plan.rs` `Step::MergeIgnore`, `execute.rs`, `manage.rs`                       | fields and parameters `aside_a`, `aside_b`                                               | `aside_source`, `aside_target`                                                                     |
| `ignore.rs` `Exclusions::new`, `union_text`                                    | parameters `a`, `b`                                                                      | `source`, `target`                                                                                 |
| `commands.rs`                                                                  | `open_a`                                                                                 | `open_source`                                                                                      |
| `rules.rs`, `service.rs`, `schedule.rs`, `watch.rs`                            | `rule.place_a`, `rule.place_b`, and `place.source` for the storage reference             | `rule.source`, `rule.target`, `place.location`                                                     |
| tests of these modules (`perf_tests.rs`, `watch_tests.rs`, `run_tests.rs`, …)  | fixtures `w.a`, `w.b`, `place_a:`, `source:` of a place                                  | `w.source`, `w.target`, `source:`, `location:`; found by the compiler where the text search cannot |

The struct `Target` keeps its type name (research R1). Locals that already carry the name `source` or `target` in
the same scope (for example a `SourceRef` called `source`) are renamed first, so no new name shadows an old one; the
compiler finds the type clashes, the review finds the rest.

## Stored columns (`_no_sync` tables of migration 0044)

| Table                          | Old column       | New column        |
| ------------------------------ | ---------------- | ----------------- |
| `haex_sync_rules_no_sync`      | `place_a_source` | `source_location` |
| `haex_sync_rules_no_sync`      | `place_a_path`   | `source_path`     |
| `haex_sync_rules_no_sync`      | `place_b_source` | `target_location` |
| `haex_sync_rules_no_sync`      | `place_b_path`   | `target_path`     |
| `haex_sync_rule_files_no_sync` | `a_size`         | `source_size`     |
| `haex_sync_rule_files_no_sync` | `a_mtime_ms`     | `source_mtime_ms` |
| `haex_sync_rule_files_no_sync` | `b_size`         | `target_size`     |
| `haex_sync_rule_files_no_sync` | `b_mtime_ms`     | `target_mtime_ms` |

## Stored values

| Table and column                      | Old value | New value          |
| ------------------------------------- | --------- | ------------------ |
| `haex_sync_rules_no_sync.direction`   | `aToB`    | `sourceToTarget`   |
| `haex_sync_rule_pending_no_sync.side` | `a`, `b`  | `source`, `target` |
| `haex_sync_run_items_no_sync.side`    | `a`, `b`  | `source`, `target` |

`both`, `delete_mode`, `outcome`, `cause`, `action` and the `kind` of the stored storage text (`device`,
`storage:…`) do not change. Columns that only look similar do not change either: `kept`, `hash`, `path`,
`found_run_id`.

## Validation rules

- A row is never dropped by the migration: row counts per table are equal before and after (SC-003).
- A stored value that is neither an old nor a new one is left as it is (`CASE … ELSE`); the readers keep ignoring
  such a row as they do today for one "a newer version wrote".
- The per-file state keeps its meaning: the old `a_*` columns become `source_*` with the same numbers, so a rule that
  had nothing to do before plans no step after (FR-005).

## State transitions

None change. The rule states (`active`, `running`, `waiting`, `error`, `paused`, `inactive`), the run outcomes and the
plan steps keep their names and their transitions.

## Frontend names

| File                                                                              | Old                                                                                                                   | New                                                                                                                         |
| --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `SyncRuleDialog.vue`                                                              | `placeA`, `placeB` refs; `hasA`, `hasB`; `labels: { a, b }`; picker `name="a"`/`"b"`; `side === 'a'`                  | `source`, `target`; `hasSource`, `hasTarget`; `labels: { source, target }`; `name="source"`/`"target"`; `side === 'source'` |
| `SyncRulesList.vue`, `SyncRulePlan.vue`, `SyncRuleLog.vue`, `SyncRulePending.vue` | `rule.placeA`, `rule.placeB`; `labels: { a, b }`; `labels[step.side]`                                                 | `rule.source`, `rule.target`; `labels: { source, target }`; indexing by the new side values                                 |
| `SyncRuleConfirmDialog.vue`                                                       | `placeA`, `placeB` fields, test ids `files-sync-rule-confirm-placeA`/`placeB`                                         | `source`, `target`, `files-sync-rule-confirm-source`/`target`                                                               |
| `lib/files/syncRules.ts`                                                          | `SyncDirection = 'aToB' \| 'both'`; `targetSides` returning `'b'`                                                     | `'sourceToTarget' \| 'both'`; returning `'target'`                                                                          |
| `lib/files/syncRules.ts`, `scripts/check-files-sync-rules.ts`                     | `PlaceRef.source` (storage reference); `canSave({ hasA, hasB })`                                                      | `PlaceRef.location`; `canSave({ hasSource, hasTarget })`                                                                    |
| `SyncRuleDialog.vue`                                                              | test ids `files-sync-rule-stored-a`, `-b`; the folder picker's value keeps its `source` (it is the picker's own type) | `files-sync-rule-stored-source`, `-target`; `pickedOf(place)` converts `location` to the picker's `source`                  |
| picker test ids                                                                   | `files-picker-a-root`, `files-picker-b-root`                                                                          | `files-picker-source-root`, `files-picker-target-root`                                                                      |
| `i18n/locales/{en,de}.json`                                                       | `files.syncRules.placeA`, `placeB`, `direction.aToB`                                                                  | `files.syncRules.source`, `target`, `direction.sourceToTarget`                                                              |
