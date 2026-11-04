# Research: source and target names for sync rule places

Each decision answers one open point of the spec or one risk found in the survey of 2026-11-01. The survey used
exact-text and structural search over `src-tauri/src`, `src`, `scripts` and `specs/061-sync-rules`.

## R1 - How far the old names reach

**Decision**: Rename by structure, not by text pattern, in these groups, and finish with a search for the old names (SC-001).

- **Types**: `SyncRuleSide::{A, B}`, `SyncRuleDirection::AToB`, the fields `place_a`, `place_b` of `SyncRuleDraft`
  and `SyncRule`, the field `source` of `SyncRulePlace`.
- **Engine fields and parameters** that mean "the first or second place": `Scanned.a/.b`, `Inputs.a/.b`,
  `Inputs.listing_a/.listing_b` in `run.rs`, the per-file `Seen` row's `a` and `b` in `store.rs` and `execute.rs`,
  the parameters `a`, `b`, `a_side` in `scan.rs`, `pending.rs`, `ignore.rs`, `plan.rs`, `manage.rs`.
- **Stored columns and values**: see R3 and R4.
- **Frontend**: `placeA` and `placeB` refs and props, `labels: { a, b }`, `hasA` and `hasB`, the folder picker names
  `a` and `b` and the test ids built from them (`files-picker-a-root`), `files-sync-rule-confirm-placeA`, the
  translation keys, `targetSides`, `SyncDirection`.
- **Agent tool**: the schema and its descriptions (R5).
- **Docs**: `specs/061-sync-rules/data-model.md`, `contracts/tauri-commands.md`, `research.md` (FR-008).

**Rationale**: A text search for "placeA" and similar misses the engine's bare `a` and `b`, and a first count of
505 hits was mostly noise (`replaceAll`, `placeAsync`, `marketplace_auth`). Those words do not belong to the rule
and stay.

**Not renamed on purpose**: `place_at` and `Place` in `files/agent` (a place of the file browser, not of a rule),
`placeAsync` (paste), `side: 'left' | 'right'` of the split view, the word "marketplace" (the survey's first count
matched `placeA` inside `MarketplaceAuthType`), and the type `SourceRef` with its other uses.

**The struct `Target`** in `execute.rs` ("where a run writes and what it runs against") is the run's context, not a
place: it holds the database, the rule and both places as `a`, `b`, `listing_a`, `listing_b`. Its type name stays,
since the spec renames no type; its four fields become `source`, `target`, `source_listing`, `target_listing`.
That gives `Target.target`, which reads oddly but is correct. A later change may rename the struct.

**Alternatives considered**: A search-and-replace over the whole tree. Rejected: it hits the unrelated names above and
cannot tell a place's `a` from any other `a`.

## R2 - The inner field of a place

**Decision**: `SyncRulePlace { location: SourceRef, path: String }`.

**Rationale**: With the operator's choice of `source`/`target` on the rule, a place with a field `source` would read
`rule.source.source` and be stored as `source_source`. `SourceRef` already has the variant `storage`
(`{ "kind": "storage", storageId }`), so `storage` would clash in meaning. `location` reads as "where the file
browser looks" (the doc comment of `SourceRef`).

**Alternatives considered**: `storage` (see above), `origin` (suggests direction), `sourceRef` (names the type, not the meaning).

## R3 - Names of the stored columns

**Decision**:

| Table                          | Old                              | New                              |
| ------------------------------ | -------------------------------- | -------------------------------- |
| `haex_sync_rules_no_sync`      | `place_a_source`, `place_a_path` | `source_location`, `source_path` |
| `haex_sync_rules_no_sync`      | `place_b_source`, `place_b_path` | `target_location`, `target_path` |
| `haex_sync_rule_files_no_sync` | `a_size`, `a_mtime_ms`           | `source_size`, `source_mtime_ms` |
| `haex_sync_rule_files_no_sync` | `b_size`, `b_mtime_ms`           | `target_size`, `target_mtime_ms` |

**Rationale**: The prefix follows the field (`source_*`, `target_*`) and the suffix follows the part of the place, so the
column of `rule.source.location` is `source_location`. Names stay short and grep-friendly.

## R4 - Migration, not an edit of 0044

**Decision**: A new migration `0053_sync_rules_source_target`. It was planned as `0050`; `origin/main` took that number for `0050_voice_conversations` (spec 066) on 2026-11-02, so this one followed it as `0051`. `0051_assistant_tones` (spec 066) took that on 2026-11-03, so this one became `0052`; `0052_tts_voices` (spec 066) took that on 2026-11-04, so this one is `0053`. A vault that ran it on this branch as `0052_sync_rules_source_target` opens through the renamed-migration table in `identity/migrations_legacy_names.rs`.
It runs `ALTER TABLE … RENAME COLUMN` for the eight columns of R3 and three `UPDATE`s: `direction` `aToB` to
`sourceToTarget` in the rules; `side` `a`/`b` to `source`/`target` in the pending deletions and in the run items.
The `UPDATE`s use `CASE … ELSE side END`, so they never touch a value they do not know. Details in
[contracts/storage-migration.md](contracts/storage-migration.md).

**Rationale**:

- `RENAME COLUMN` keeps every row and the `PRIMARY KEY`s and foreign keys; no table is rebuilt.
- The tables are `_no_sync` (ADR 0018), so haex-crdt neither tracks nor syncs them and
  `HOLZI_TRIGGER_VERSION` stays, as for `0046_sync_rules_decrypt`, which also uses `ALTER TABLE`.
- Without the value `UPDATE`s, `parse::<SyncRuleSide>` in `store.rs` and `run_log.rs` returns `None` for `a` and `b`,
  and the callers drop those rows silently: pending deletions and log items would vanish. The migration test asserts
  that they survive.
- `direction` is read through the same kind of parse; an unknown value makes the rule unreadable.

**Alternatives considered**: Editing `0044` (breaks vaults that applied it, and the migration source is keyed by name);
dropping and recreating the tables (loses rules and their state); keeping the old column names and only renaming in
Rust (leaves the old vocabulary in the stored data and in every query, against FR-001).

**To check while implementing**: that the haex-crdt migration runner accepts `RENAME COLUMN` on a `_no_sync` table
(SQLite 3.25 or newer; `0046` already runs `ALTER TABLE ADD COLUMN` there), and that no other open branch took the
number `0053`; recheck before the merge.

## R5 - The agent tool schema and the eval fixture

**Decision**: Edit `src/lib/actions/syncRuleActions.ts`: properties `source` and `target`, each a place of
`{ location, path }`, direction enum `sourceToTarget` | `both`, descriptions in the same words. Regenerate
`src-tauri/src/chat/eval/tools.json` with `scripts/export-eval-tools.ts`; do not edit it by hand. No alias for
`placeA` and `placeB`. A call with the old names is refused, but not by an unknown-field check: the sync tools take a
complete draft (`required` in the schema), `agent.rs` parses it with `serde_json::from_value::<SyncRuleDraft>`, which
ignores unknown fields, and the required `source` and `target` are then missing, so the call fails with an invalid-input
error and no rule changes (FR-006, SC-004). A test calls the tool with the old names to keep this true.

**Rationale**: `tools.json` is the output of that script and `include_str!`-ed by `chat/eval`; a hand edit would drift
from the schema. The property name `source` at the top of the sync tool and the `location` inside a place differ from the
file-browser tools, whose `source` is the storage string; the two tool families do not share a schema, so this does not
collide.

**Alternatives considered**: Accepting both names for a while. Rejected by the project rule (no legacy aliases) and by FR-006.

## R6 - Translation keys

**Decision**: `files.syncRules.placeA`/`placeB` become `files.syncRules.source`/`target`;
`files.syncRules.direction.aToB` becomes `direction.sourceToTarget`. The displayed texts ("Quelle", "Ziel",
"Quelle → Ziel") stay, in `de.json` and `en.json`.

**Rationale**: The keys `files.syncRules.source` and `.target` do not exist yet (checked in both files), so there is no
collision. The key follows the value of the direction (`direction.${rule.direction}` builds it at runtime in
`SyncRulesList.vue`, `SyncRuleDialog.vue` and `SyncRuleConfirmDialog.vue`), so it has to move with the value.

## R7 - Generated bindings

**Decision**: Let `cargo test` regenerate `src/types/bindings/*.ts`, then strip trailing blanks with
`sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` as the CI does, and commit the result.

**Rationale**: The bindings are generated; a hand edit drifts. Five files change: `SyncRule`, `SyncRuleDraft`,
`SyncRulePlace`, `SyncRuleSide`, `SyncRuleDirection`. `SyncRulePending`, `SyncRulePlanStep` and `SyncRuleRunItem` only name
the type `SyncRuleSide`, so their files stay as they are. No new exported type, so no name collision.
