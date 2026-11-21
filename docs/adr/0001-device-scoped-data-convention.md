# Device-scoped data uses sync-tracked tables with a `vault_device_uuid` FK

HaexVault vaults sync via CRDT across a small set of devices belonging to one
operator. Data that is _scoped to a specific device_ (which model this
device auto-loads, which GGUFs are installed on which laptop, later
preferences like UI theme) needs to be distinguishable from vault-wide
data even after the entire vault file is copied to another device. We
put every persistent per-device row into a **CRDT-tracked** table with a
composite primary key that includes `vault_device_uuid`, hard-referencing
`known_devices(vault_device_uuid) ON DELETE CASCADE`.

## Why this shape

The `_no_sync` suffix in `haex-crdt` only means "no CRDT sync payload" —
it does **not** mean "won't travel with the SQLite file". A `cp vault.db
other-machine.db` carries every `_no_sync` row along, including any
device-local settings. The observed bug in the pre-ADR
`device_downloaded_models_no_sync` was exactly this: the copy target
believed it had every GGUF the source had.

The `HaexBootstrap` hook already mints a fresh `vault_device_uuid` on
first-open on a new device (via the `installation_uuid` lookup), so
gating device-scoped rows on `vault_device_uuid` is automatically
copy-safe — the adopted device queries under its new UUID and finds no
old rows. The FK adds a hard invariant that no application code can
insert an orphan preference for a nonexistent device.

`haex-crdt` supports hard FKs — it enables `PRAGMA foreign_keys = ON` on
open and turns FKs off via an RAII guard while applying a sync payload,
so out-of-order remote inserts don't fail the constraint.

## Vault-wide entries: sentinel row in `known_devices`

Some rows in `preferences` need to represent "this applies to the whole
vault, not one device" (the vault-wide fallback default model). Rather
than split into two tables or use a nullable FK, we reserve the nil
UUID `00000000-0000-0000-0000-000000000000` as a **sentinel row in
`known_devices`** representing vault-scope. Bootstrap inserts it
idempotently on every open (`INSERT OR IGNORE`). `preferences` rows
with `vault_device_uuid = nil` are vault-wide; every other value is a
real device. Storage wrappers that surface the device list to the UI
filter the sentinel out.

## Naming rule

- Persistent per-device data → CRDT-tracked table, composite PK on
  `vault_device_uuid`, hard FK to `known_devices(vault_device_uuid)
ON DELETE CASCADE`. No `_no_sync` suffix.
- `_no_sync` reserved for transient runtime state that would waste sync
  bandwidth even in principle (e.g. the planned
  `app_settings_no_sync` for in-flight generation buffers that either
  land in `chat_messages` on finalisation or are dropped).
- Any table added later must answer: "if this vault file is copied to
  another device, whose rows apply there?" If the answer is "only rows
  for the target device", it needs a `vault_device_uuid` column and
  the FK.

## Considered alternatives

- **Two tables (`vault_preferences` + `device_preferences`)**. Cleaner
  semantics per table, but doubles migrations, storage wrappers and
  resolver plumbing; the sentinel row is a lighter tax.
- **Nullable `vault_device_uuid`**. SQLite allows NULL in PRIMARY KEY
  columns (documented historical bug), breaking the uniqueness
  guarantee we'd need. Workarounds via `WITHOUT ROWID` or expression
  UNIQUE indexes fight haex-crdt's row-identity assumptions.
- **String sentinel like `'vault'` in a `scope` column, no FK**. Loses
  the hard FK safety without any offsetting simplicity gain.

## Consequences

- `HaexBootstrap` grows an idempotent `INSERT OR IGNORE` for the
  sentinel row on every open.
- Storage wrappers listing devices filter `WHERE vault_device_uuid !=
'00000000-0000-0000-0000-000000000000'`.
- Cross-device UIs display `known_devices.alias` (human name), never
  the raw UUID — this ADR is why the alias exists on that table.
- `device_downloaded_models_no_sync` is dropped rather than retrofitted
  — the filesystem under `AppLocalData/models/<slug>/` is authoritative
  for "installed here" and requires no schema at all.
