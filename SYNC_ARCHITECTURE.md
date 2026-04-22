# Sync Architecture — CRDT Delete-Log Model

This document describes how CRDT sync handles INSERT / UPDATE / DELETE across
devices. It is the current, authoritative overview after the transaction-scope
HLC + delete-log refactor.

> **Note:** Earlier revisions of this document described a *soft-delete* model
> (inline `haex_tombstone` column, partial unique indexes, `WHERE
> haex_deleted = 0` filters). That model is **no longer in use**. Deletes are
> now recorded in a separate `haex_deleted_rows` table and propagated as
> ordinary CRDT changes, which removes the need for tombstone columns and
> partial indexes on every syncable table.

---

## Table-scan approach

Each CRDT-enabled table carries two runtime-added columns:

| Column             | Meaning                                                          |
|--------------------|------------------------------------------------------------------|
| `haex_hlc`         | HLC timestamp of the most recent write to any column in this row |
| `haex_column_hlcs` | JSON object `{columnName: hlc}` for per-column LWW decisions     |

Sync pushes work by scanning each dirty table for rows whose `haex_hlc` has
advanced past `lastPushHlcTimestamp` and emitting one `ColumnChange` per
changed column. The CRDT engine automatically adds `haex_hlc` /
`haex_column_hlcs` to any non-`_no_sync` table at `CREATE TABLE` time — app
code never has to think about them.

## Transaction-scope HLC

All writes performed inside a single SQLite transaction (explicit `BEGIN…
COMMIT` or a single auto-commit statement) share one HLC value. The HLC is
drawn from the per-connection `ConnectionContext` slot, guarded by an
`update_hook`-driven `write_pending` flag so that a stray read-only
`SELECT current_hlc()` cannot poison the next write. On receive, changes
with the same HLC form a **group** and are applied atomically; incomplete
groups are discarded and re-pulled on the next sync tick.

## DELETE handling — the delete-log table

There is no tombstone column on main tables. Instead, a `BEFORE DELETE`
trigger on every CRDT table writes an event to `haex_deleted_rows`:

```sql
INSERT INTO haex_deleted_rows
    (id, table_name, row_pks, haex_hlc, haex_column_hlcs)
VALUES
    (gen_uuid(), '<target>', json_object('id', OLD.id, …),
     current_hlc(), '{}');
```

`haex_deleted_rows` itself is a CRDT-synced table, so:

1. The scanner picks the new delete-log row up like any other change and
   pushes it to the server.
2. On receive, the apply-path sees a change for `haex_deleted_rows`, performs
   the actual `DELETE FROM <target> WHERE …` locally, and the
   `triggers_enabled='0'` config keeps the local trigger from re-logging.
3. Because the delete lives in its own table, *main-table* UNIQUE indexes can
   stay full (no `WHERE haex_deleted = 0` partial-index hack), so they remain
   eligible as foreign-key parents.

Re-inserts of a previously deleted PK are a plain INSERT on the main table;
the stale delete-log entry is cleaned up by the periodic
`cleanup_deleted_rows` job based on `haex_hlc` age, not by the insert path.

## ColumnChange wire format

```typescript
interface ColumnChange {
  tableName: string          // target table on the receiver
  rowPks: string             // JSON-encoded PK map, e.g. '{"id":"123"}'
  columnName: string         // one change row per (row, column)
  hlcTimestamp: string       // transaction-scope HLC shared by every
                             // change from one local transaction
  encryptedValue: string     // ciphertext (base64)
  nonce: string              // XChaCha20-Poly1305 nonce (base64)
}
```

Delete events travel as `ColumnChange`s with `tableName: "haex_deleted_rows"`
and columns `table_name` / `row_pks` / `haex_hlc`. The receiver's apply-path
recognises this and issues the real delete on the target table.

## Cleanup

`cleanup_deleted_rows` removes rows from `haex_deleted_rows` whose `haex_hlc`
is older than the configured retention window (default 30 days). A
`retention_days == 0` fully empties the table. Main-table rows that were
already deleted stay deleted — we only prune the log, not the effect.

## Source pointers

- Triggers + column layout: [src-tauri/src/crdt/trigger.rs](src-tauri/src/crdt/trigger.rs)
- HLC service + transaction slot: [src-tauri/src/database/connection_context.rs](src-tauri/src/database/connection_context.rs)
- `current_hlc()` UDF: [src-tauri/src/database/core.rs](src-tauri/src/database/core.rs)
- Scanner: [src/stores/sync/tableScanner.ts](src/stores/sync/tableScanner.ts)
- Delete-log cleanup: [src-tauri/src/crdt/cleanup.rs](src-tauri/src/crdt/cleanup.rs)
- Drizzle schema for `haex_deleted_rows`: [src/database/schemas/crdt.ts](src/database/schemas/crdt.ts)
