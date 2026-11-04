# Contract: migration `0053_sync_rules_source_target`

Registered in `holzi_migration_source` after `0050_voice_conversations`, with a comment in the style of the neighbours
("Spec 075: the two places of a sync rule are source and target"). The SQL lives in
`identity/migrations_sync_rules_source_target.rs` as the constant `SYNC_RULES_SOURCE_TARGET_0053`.

All five tables are `_no_sync` (ADR 0018): not tracked, not synced, so `HOLZI_TRIGGER_VERSION` stays, as for `0046`.

## Statements

Separated by `--> statement-breakpoint` like `0044`.

```sql
ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_a_source TO source_location;
ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_a_path TO source_path;
ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_b_source TO target_location;
ALTER TABLE haex_sync_rules_no_sync RENAME COLUMN place_b_path TO target_path;
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN a_size TO source_size;
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN a_mtime_ms TO source_mtime_ms;
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN b_size TO target_size;
ALTER TABLE haex_sync_rule_files_no_sync RENAME COLUMN b_mtime_ms TO target_mtime_ms;
UPDATE haex_sync_rules_no_sync SET direction = 'sourceToTarget' WHERE direction = 'aToB';
UPDATE haex_sync_rule_pending_no_sync
  SET side = CASE side WHEN 'a' THEN 'source' WHEN 'b' THEN 'target' ELSE side END;
UPDATE haex_sync_run_items_no_sync
  SET side = CASE side WHEN 'a' THEN 'source' WHEN 'b' THEN 'target' ELSE side END;
```

## Guarantees

1. Row count of each of the five tables is the same before and after.
2. Every `PRIMARY KEY`, foreign key and the index `haex_sync_runs_no_sync_rule` survive; no table is rebuilt.
3. The `UPDATE`s change only the three old values and leave every other value as it is.
4. A fresh vault runs `0044` and then `0053`, and ends with the new column names.
5. The migration is recorded under its name and runs once; it is not written to be run twice.

## Test (`migrations_sync_rules_source_target_tests.rs`)

Built like `migrations_sync_rules_tests.rs`, with `migration_source_before` for the schema just before `0053`:

- **Existing vault**: fill rows as `fill()` does in the `0044` test (a rule with direction `aToB`, a state row, a
  pending deletion on side `b`, a run, a run item on side `b`), run `0053`, then read them with the new column names
  and assert: the same counts, the rule's places and direction under the new names (`sourceToTarget`), the state row's
  numbers in `source_*` and `target_*`, the pending deletion on `target`, the run item on `target`.
- **Both sides**: a pending deletion on `a` ends on `source`, one on `b` on `target`.
- **Unknown value untouched**: a `side` value `x` stays `x`.
- **Fresh vault**: has the new columns and no `place_a_*`, `a_size` column.
- **Readers**: `store::rules`, `store::pending`, `store::state` and `run_log::items` read the migrated rows back
  through the Rust types, so a dropped row (`parse` returning `None`) fails the test.
