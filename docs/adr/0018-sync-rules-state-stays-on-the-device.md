# Sync rules and their state stay on the device that runs them

Status: proposed
Date: 2026-10-26

## Context

Spec 061 adds sync rules: a rule keeps two folders equal and runs only on the device that created it
(FR-003). For each rule holzi keeps the size and modification time of every file on both sides after the
last success (the **Stand**), the deletions waiting for a confirmation, and a run log with one entry per
file action and run (FR-043 to FR-045). A rule over a photo folder has tens of thousands of files, so the
Stand and the log reach hundreds of thousands of rows per device.

[ADR 0001](./0001-device-scoped-data-convention.md) says persistent per-device data goes into CRDT-tracked
tables keyed by `vault_device_uuid` and reserves the `_no_sync` suffix for transient runtime state that
would waste sync bandwidth even in principle. Following it here would send every row of the Stand and the
log to every other device, which never reads them, and would make rules visible on devices the spec says
they must not reach.

## Decision

**Rules, their Stand, their pending deletions and their run log live in `*_no_sync` tables of the vault
database, each row carrying `vault_device_uuid` with a hard foreign key to `known_devices` (`ON DELETE
CASCADE`).**

- Migration `0044_sync_rules` creates `haex_sync_rules_no_sync`, `haex_sync_rule_files_no_sync`,
  `haex_sync_rule_pending_no_sync`, `haex_sync_runs_no_sync` and `haex_sync_run_items_no_sync`
  (spec 061 data-model.md). No CRDT triggers, no change of `HOLZI_TRIGGER_VERSION`.
- Every query filters on the current `vault_device_uuid`. A vault file copied to another device gets a new
  `vault_device_uuid` there (ADR 0001, bootstrap), so the copy sees no rules and runs none.
- The log depth setting stays a normal per-device preference (`PrefScope::Device`), since it is one row.

## Consequences

- ADR 0001 gets a second exception besides transient state: per-device data that must not be synced by
  requirement or would be large sync load without a reader on other devices. Such tables still carry the
  `vault_device_uuid` column and its foreign key, so the copy safety of ADR 0001 holds.
- A device that is lost takes its rules with it; there is no recovery from another device. Rules are cheap
  to create again, and the files themselves stay at both places.
- Removing a device from `known_devices` deletes its rules and their history through the cascade.
