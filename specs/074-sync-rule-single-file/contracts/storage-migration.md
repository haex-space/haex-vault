# Contract: migration `0060_sync_rules_source_kind`

The number is provisional; main's last migration on 2026-11-09 is `0059_drop_agent_files`. Recheck before the merge
(research R10). The SQL lives in `identity/migrations_sync_rules_source_kind.rs` as the constant
`SYNC_RULES_SOURCE_KIND_0060`, registered in `identity/migrations.rs` after the last existing migration, with
`mod` lines in `identity/mod.rs`.

## Statement

```sql
ALTER TABLE haex_sync_rules_no_sync ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'folder';
```

One statement, no `--> statement-breakpoint`.

## Properties

1. Every existing row gets `source_kind = 'folder'`; no row is lost or changed otherwise.
2. The table is `_no_sync` (ADR 0018), so `HOLZI_TRIGGER_VERSION` stays.
3. The migration runs once; opening the migrated vault again changes nothing.
4. A fresh vault runs `0044`, the `0053` rename and this one, and ends with the column.
5. `store.rs` reads and writes the column (`RULE_COLUMNS`, the insert, the update). An unknown stored value
   (`parse::<SyncRuleSourceKind>` is `None`) makes the reader skip the row, as it does for an unknown direction.

## Test (`migrations_sync_rules_source_kind_tests.rs`)

Built like `migrations_sync_rules_source_target_tests.rs`, with `migration_source_before` for the schema just before
this migration:

- fill a vault from before it with a rule (and its state, a pending deletion, a run), run the migration, then check
  that every table keeps its row count, the rule reads through `store::rules` as a folder rule, and the other
  fields are unchanged;
- a vault opened twice does not change on the second open;
- a fresh vault has the column `source_kind` with the default `folder`;
- a file rule written through `store` and read back keeps `file`.
