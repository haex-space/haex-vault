---
description: 'Task list for spec 059-device-versions'
---

# Tasks: Versionsstände der Geräte in der Föderationsübersicht

**Input**: Design documents from `/specs/059-device-versions/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/sync-protocol.md](./contracts/sync-protocol.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires tests with every behaviour change, in files of their own:
Rust unit tests as `*_tests.rs` declared `#[cfg(test)] mod x_tests;` next to the module (pattern of
`src-tauri/src/sync/handshake_refusal_tests.rs`), TypeScript helpers in `scripts/check-settings.ts`. Run cargo
only through `nix develop --command scripts/with-nix-host-bridge.sh cargo …` and only the affected targets with
`-j 4` (a full build crashed VS Code before).

**Organization**: Grouped by user story in priority order (P1: US1, P2: US2, P3: US3). Phase 2 is the shared
backend (storage, wire, handshake); US1 is the MVP and needs all of it.

**Shipping note**: one PR from `.worktrees/059-device-versions`, commits per phase with Conventional Commits
headers (`feat(sync): …`, `feat(settings): …`, `docs(spec): …`); no agent attribution lines in commits or the PR
(project rule). Before the PR merge `origin/main` into the branch; if another migration took `0040`, renumber
(T003).

**Rules for every task**: before authoring a new named function, component or module consult graphify
(`graphify query`) and extend a candidate if one fits (the plan lists the ones evaluated: `problems::
device_at_endpoint`, `seen::touch`, `deviceStatus.ts`); every new UI text in `de.json` and `en.json` (FR-010),
placeholders as `{name}`, never `<…>` (vue-i18n refuses it); say „Gerät“/Vault, never a bare „Relay“, the product
name in texts is „HaexVault“; `UiButton` in forms needs `type="button"`; no platform-specific code (FR-009);
keep every file under 300 lines (split into a sibling module if it would grow past that).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US3)

---

## Phase 1: Setup

**Purpose**: Check the one assumption the wire change rests on, before building on it.

- [x] T001 Spike for research R2 in `src-tauri/src/sync/wire_schema_tests.rs` (new, declare in `src-tauri/src/sync/mod.rs` under `#[cfg(test)]`): define a local copy `OldChallenge { v: u8, nonce_a: [u8; 32], endpoint_a: [u8; 32], list: ListRef }`, serialise the new `Message::Challenge` with `postcard::to_stdvec` and assert `postcard::from_bytes::<OldChallenge>` succeeds with `v == 2` and the byte after the variant tag (`bytes[0] == 0`, `Challenge` is variant 0) is `v`. If it fails, stop and amend research R2, the spec's edge case and `contracts/sync-protocol.md` before T006 (fallback: the dialing side checks only the version byte; an old dialer then reports a decode error instead of the version message). The test names the new field, so it does not compile until T005 adds it; it must be green when T005 is done.

---

## Phase 2: Foundational (blocks all user stories)

**Purpose**: Store what the handshake sees, and make the handshake show it before either side decides (FR-004,
FR-005, FR-006).

- [x] T002 [P] Migration SQL in `src-tauri/src/identity/migrations_peer_schema.rs` (new): `pub const PEER_SCHEMA_0040: &str` with the three `ALTER TABLE device_presence_no_sync ADD COLUMN peer_protocol INTEGER`, `peer_migration INTEGER`, `peer_trigger INTEGER` separated by `--> statement-breakpoint`, exactly as in data-model.md (all `NULL` = unknown, no `CHECK`, no `DEFAULT`). Module doc as in `migrations_file_shares.rs`: device-local, no CRDT table, `HOLZI_TRIGGER_VERSION` stays 27.
- [x] T003 Register it in `src-tauri/src/identity/migrations.rs`: `mod migrations_peer_schema;`, `use`, `m.insert(MigrationName::from("0040_device_peer_schema"), PEER_SCHEMA_0040.to_owned())` after `0039`. Do NOT change `HOLZI_TRIGGER_VERSION` (27) and add no changelog line for it. Add `src-tauri/src/identity/migrations_peer_schema_tests.rs` (pattern: `migrations_file_shares_tests.rs`): after migrating, the three columns exist and are `NULL` for an existing presence row; the migration count is 40. Fix every existing test that pins the count of 39 migrations (`rg '39' src-tauri/src/sync/*_tests.rs src-tauri/src/identity/*_tests.rs`, check each hit is that count).
- [x] T004 [P] Storage in `src-tauri/src/sync/peer_schema.rs` (new, ~80 lines, declare in `src-tauri/src/sync/mod.rs`): `pub fn record(replica: &Replica, device: &[u8; 32], schema: SchemaVersion) -> haex_crdt::Result<bool>` that `UPDATE`s the three columns of an existing `device_presence_no_sync` row and, when there is none, inserts `(device_pubkey, last_seen = 0, endpoint_addr = NULL, problem = NULL, peer_*)` like `problems::set`; returns whether any value changed (no change = `false`). `pub fn of(q: &mut impl Query, device: &[u8; 32]) -> haex_crdt::Result<Option<SchemaVersion>>`: `None` unless all three are non-`NULL` (a partial row reads as unknown, data-model.md). Neither `problems::set`, `problems::clear`, `seen::touch` nor `presence::record_seen` touches the three columns (check their `UPDATE`/`ON CONFLICT` lists). Tests in `src-tauri/src/sync/peer_schema_tests.rs` (new): record then read; second identical record returns `false`; a changed value replaces all three and returns `true`; `of` is `None` for a device never recorded and for a partial row; recording does not change `last_seen`, `endpoint_addr`, `problem`; `problems::set`/`seen::touch` afterwards keep the values; `removal.rs`'s delete drops them with the row.
- [x] T005 Wire in `src-tauri/src/sync/wire.rs`: `PROTOCOL_VERSION = 2`; add `schema: SchemaVersion` as the LAST field of `Message::Challenge` (after `list`), update the doc comment of `SchemaVersion`/`PROTOCOL_VERSION` and the file header; add `pub async fn read_challenge<R: AsyncRead + Unpin>(reader: &mut R, limit: usize) -> Result<Challenge, ChallengeError>` that reads the whole frame (same length check as `read_value`), returns `ChallengeError::Version(first_byte)` when `bytes[0] != PROTOCOL_VERSION` (including an empty frame → decode error) and decodes only after that. T001 turns green here. Fix every constructor of `Message::Challenge` in tests and `test_support.rs`.
- [x] T006 Handshake in `src-tauri/src/sync/handshake.rs` (it is already long; put new code in a sibling `src-tauri/src/sync/handshake_schema.rs` and call it): (a) `accept_with` writes `schema: local.schema` in the `Challenge`; (b) `dial_with` reads the `Challenge` with `read_challenge`; on `ChallengeError::Version(_)` it returns `HandshakeError::Refused(RejectCode::Incompatible)` and reports no schema (the old stand cannot be read); on success it reports `challenge.schema` through the new callback BEFORE comparing `v`/schema and before writing `Response`, and still refuses with `Refused(Incompatible)` when `schema != local.schema` (the halt is then flagged by the node via `note_failure`, unchanged); (c) `accept_with` reports `response.schema` through the callback right after the vault and signature checks and BEFORE the `v != PROTOCOL_VERSION || schema != local.schema` refusal, so an incompatible dialer is still recorded; a foreign vault or bad signature reports nothing (contracts/sync-protocol.md). The callback is `on_peer_schema: &(dyn Fn(SchemaVersion) + Send + Sync)`, added to `accept_with`/`dial_with` next to `on_applied`; `accept`/`dial` pass a no-op. The comparison rule (FR-029) does not change.
- [x] T007 Node wiring in `src-tauri/src/sync/endpoint.rs` and `src-tauri/src/sync/endpoint_devices.rs`: pass a callback to `accept_with`/`dial_with` (around `endpoint.rs:507-520`) that spawns `endpoint_devices::record_peer_schema(inner, remote_endpoint, schema)`: resolve the device with the existing `device_at(inner, endpoint)` (`problems::device_at_endpoint`, research R3), return when the list names none, call `peer_schema::record` in `spawn_blocking`, call `notify_devices_changed` only when it returned `true`; log failures at `debug`/`warn` like `flag`. No second endpoint→device lookup anywhere.
- [x] T008 Handshake tests in `src-tauri/src/sync/handshake_schema_tests.rs` (new; helpers of `handshake_refusal_tests.rs` and `test_support.rs`): (1) equal schemas: both callbacks saw the other side's `SchemaVersion` and the handshake is accepted; (2) acceptor with a different `holzi_migration`: it refuses with `Incompatible` AND both callbacks saw the other side's value (SC-005, 100 % of the attempts: run the three differing fields in turn); (3) dialer sends `Response` with `v = 1` and its old schema (hand-built frames): acceptor reports that schema and rejects with `Incompatible`; (4) acceptor writes a `Challenge` with `v = 1` and no `schema` (hand-built): the dialer returns `Refused(Incompatible)` without writing a `Response` and without calling the callback; (5) foreign vault and bad signature: the acceptor's callback is not called. In `src-tauri/tests/` extend an existing node-level sync test (find with `rg 'note_failure|incompatible' src-tauri/tests`) or add `sync_peer_schema.rs`: two nodes with different injected schemas end with `peer_schema::of` set on both and `problem = incompatible_version` on both.

**Done notes**: T006 needed no sibling module (the change is ~30 lines in `handshake.rs`). T007 records the seen state in `run_connection` right after the handshake returns (Accept or refusal) instead of spawning a task from the callback, so the write is ordered before the failure is flagged and no untracked task outlives the node. A `Refused(Incompatible)` (a `Challenge` of another protocol version) now also flags the device as `incompatible_version` in `note_failure`. T003: no test pinned the count of migrations, so nothing else needed fixing. T001's postcard assumption holds (`wire_tests.rs`); the version byte is the one after the variant tag, not the first byte (spec documents corrected).

**Checkpoint**: both sides of any handshake attempt know the other's `SchemaVersion`; run `cargo test -j 4 --lib sync::` and the extended node test.

---

## Phase 3: User Story 1 - Sehen, welches Gerät hinterherhinkt (Priority: P1) 🎯 MVP

**Goal**: Every device row shows the three values, differing values are marked and readable as lower/higher,
and the red message names the device to update when the direction is clear (FR-001, FR-003, FR-008, FR-011,
FR-012).

**Independent Test**: Two stored values differ in `holziMigration` only: the other device's row shows the three
numbers, the value is marked lower, the message names the other device; after the values equal, the marks and the
named message are gone without reopening the view.

- [x] T009 [US1] `src-tauri/src/sync/device_view.rs`: add `pub struct DeviceSchema { protocol: u32, holzi_migration: u32, crdt_trigger: u32 }` (`Serialize, TS`, `export_to = "../../src/types/bindings/"`, `rename_all = "camelCase"`, `From<SchemaVersion>`) and `schema: Option<DeviceSchema>` on `VaultDevice`; `Facts` gets `schema: Option<SchemaVersion>`; `load` reads the three columns in the existing presence query (`SELECT device_pubkey, last_seen, problem, peer_protocol, peer_migration, peer_trigger`; only all three non-`NULL` count) instead of a second query; `build` sets `schema` from `Facts` for others and from `Inputs.own_schema` (new field, filled with `handshake::local_schema()` in `load`) for the row with `is_current` (never `None`). Regenerate bindings (the repo's ts-rs export; do not `git checkout` new binding files, see memory note: strip whitespace instead). Extend `src-tauri/src/sync/device_view_tests.rs`: own row has the local schema; other row with stored values; other row without (`None`); partial row reads `None`; sorting unchanged.
- [x] T010 [P] [US1] `src/lib/sync/deviceStatus.ts` (pure): `DeviceSchemaFacts = { protocol: number; holziMigration: number; crdtTrigger: number }`; `SchemaDiff = { protocol: 'equal'|'lower'|'higher'; holziMigration: …; crdtTrigger: … }`; `schemaDiff(own, other): SchemaDiff | null` (`null` when `other` is `null`; `lower` = the other device's value is smaller than this device's); `problemMessageKey(problem, diff, deviceNamed)` returning `settings.federation.problem.incompatible_version_other` when at least one value differs and every differing value is `lower`, `…incompatible_version_this` when every differing value is `higher`, else the existing `settings.federation.problem.incompatible_version` (also for `duplicate`, unchanged, and for `diff === null` or mixed directions) per FR-012. Extend `scripts/check-settings.ts` (`node --test`, `pnpm check:settings`): all-equal, one lower, one higher, mixed directions, unknown (`null`) → plain text, `duplicate` unchanged.
- [x] T011 [P] [US1] Texts in `src/i18n/locales/de.json` and `en.json` under `settings.federation`: `problem.incompatible_version_other` („{name} muss aktualisiert werden, sonst synchronisieren sie nicht.“ / „{name} has to be updated, or they will not sync.“), `problem.incompatible_version_this` („Dieses Gerät muss aktualisiert werden, sonst synchronisieren sie nicht.“ / „This device has to be updated, or they will not sync.“), `schema.protocol` („Sync-Protokoll“ / „Sync protocol“), `schema.migration` („Vault-Schema“ / „Vault schema“), `schema.trigger` („Änderungsverfolgung“ / „Change tracking“), `schema.lower` („niedriger als dieses Gerät“ / „lower than this device“), `schema.higher` („höher als dieses Gerät“ / „higher than this device“); keep the existing `incompatible_version` text. Run the repo's i18n key check if one exists (`rg 'check:i18n|i18n' package.json`).
- [x] T012 [US1] `src/components/settings/DeviceRow.vue` (+ a small `DeviceSchemaValues.vue` next to it if the row would pass 300 lines): below role/status show `Sync-Protokoll 2 · Vault-Schema 39 · Änderungsverfolgung 27` in a `flex flex-wrap` line (no sideways scroll, FR-009), `data-testid="settings-device-schema"`; a differing value carries `text-destructive`, a `↓`/`↑` mark and an `aria-label` from `schema.lower`/`schema.higher` plus the label (more than colour, FR-003); the red message uses `problemMessageKey` with `{ name: device.alias ?? t('settings.federation.unnamed') }`. The own schema comes from the list: pass the `isCurrent` device's `schema` into the row as a prop `own` from `src/components/settings/FederationView.vue` (find it with `rg 'DeviceRow' src`). Live update needs no code: the list already reloads on `sync-devices-changed` (FR-008); confirm by reading the listener.
- [x] T013 [P] [US1] Agent action (FR-011): `src/stores/settingsActionHandlers.ts` (`settings.devices.list`) adds `...(device.schema === null ? {} : { schema: device.schema })`; `src/lib/actions/settingsActions.ts` extends the description („…and, when known, its sync versions: protocol, vault schema and change tracking“). Extend the check that pins the action's result shape (`rg 'settings.devices.list' scripts`) and `src-tauri/src/chat/eval/tools.json` only if that file is generated from the description (`rg 'tools.json' scripts package.json`); the extension bridge gets nothing (no change under `src-tauri/src/extensions/`).

**Done notes (US3, polish)**: T016 is covered by `device_view_tests.rs` (state `None` before, `Some` after), `peer_schema_tests.rs` (an identical second record returns `false`, so the list is not told again) and the extended `tests/sync_devices.rs` (both rows show the state after a real handshake). T018: rebased onto `origin/main` (a53cf9b6); `0040` was free. T019 ran: lib tests `sync::` and `identity::` (390 pass), `sync_devices` online test, clippy with and without default features, `check:settings`, `check:agent-actions`, `check:templates`, `typecheck`, `typecheck:scripts`, eslint and prettier on the touched files. Not run: the e2e scene (T014) and quickstart M1/M2 (T020), because the disk had about 6 GB free and an e2e build needs far more.

**Checkpoint**: US1 works end to end with the Phase 2 backend; `pnpm check:settings`, `pnpm check:agent-actions`, `pnpm check:templates`.

---

## Phase 4: User Story 2 - Den eigenen Stand kennen (Priority: P2)

**Goal**: „Dieses Gerät“ shows the same three values, also in a vault with one device (FR-002).

**Independent Test**: A fresh vault with one device: the row shows three numbers and nothing is marked.

- [ ] T014 [US2] Make the own row work without a second device in `src/components/settings/DeviceRow.vue`: the `isCurrent` row shows the values from its own `schema` and never a mark (it is the reference); add a `device_view_tests.rs` case „vault with only this device“ (list = one device) and extend the scene `scripts/e2e/scenarios/settings-federation.test.ts`: after `createAndUnlock`, the row `[data-testid="settings-device"][data-current]` contains `[data-testid="settings-device-schema"]` with three numbers and no element marked lower/higher (the `rows` helper there reads `title`/`mark`; add a `schema` text read). Run it as described in `scripts/e2e/README.md` (`--grep federation`).

---

## Phase 5: User Story 3 - Gerät, dessen Stand noch unbekannt ist (Priority: P3)

**Goal**: A device whose values were never seen shows no values and a hint instead (FR-007).

**Independent Test**: A linked device the list names but this device never handshook with: its row shows „Stand noch nicht bekannt“ and no numbers; after a handshake the numbers appear.

- [x] T015 [US3] `src/components/settings/DeviceRow.vue` + `de.json`/`en.json`: when `device.schema === null` and the row is not the current device, show `settings.federation.schema.unknown` („Stand noch nicht bekannt“ / „State not known yet“) in the same line (`data-testid="settings-device-schema-unknown"`), no numbers, no marks; the red message stays the plain text then (T010 returns it for `diff === null`). Rust side is covered by T004/T009 (partial row → unknown); add a `scripts/check-settings.ts` case that `problemMessageKey` with `diff === null` is the plain key.
- [x] T016 [US3] Node-level test in `src-tauri/tests/` (same file as the T008 node test): before the first handshake `device_view::load` gives `schema = None` for the other device; after it, `Some(..)`, and the `sync-devices-changed` hook fired once for the first value and not again for an identical second handshake (R5).

---

## Phase 6: Polish & Cross-Cutting

- [x] T017 [P] Spec 024 documents (they describe the shipped wire and view): fold `specs/059-device-versions/contracts/sync-protocol.md` into `specs/024-own-device-sync/contracts/sync-protocol.md` (the `Challenge` line, `v = 2`, the version-byte rule, the new bullet on what flows before `Accept`), `…/contracts/tauri-commands.md` (`VaultDevice.schema`, `settings.devices.list`), `…/data-model.md` (`device_presence_no_sync` columns), `…/research.md` R14 (ALPN stays, `v` is the check) and `spec.md` FR-029/FR-033 with a pointer „ergänzt durch Spec 059“. Keep the 059 contract files as the change record.
- [x] T018 Merge `origin/main` into the branch, resolve conflicts; if `0040` is taken renumber in `migrations.rs`, the SQL module, its test and data-model.md/research.md; re-run T003's count fix.
- [x] T019 Checks, targeted only: `nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --lib sync:: identity::`, the node test of T008, `cargo clippy -j 4 --all-targets -- -D warnings` for the touched crate, `pnpm check:settings check:agent-actions check:templates`, prettier (`pnpm prettier --check` on touched files; corepack gotcha in memory). Then the e2e of T014. Report anything skipped.
- [ ] T020 Walk `quickstart.md` M1 and M2 as far as the machine allows (two instances via the e2e rig or two builds; M2 by resizing); note in the PR description what was and was not hand-tested. Open the PR against `main` (title `feat(sync): show the sync versions of each device`), description in German like the spec, without agent attribution lines; rebase-merge, never squash.

---

## Dependencies & Execution Order

- Phase 1 (T001) first; T005 completes it. T002 ∥ T004; T003 after T002; T004 needs T003 (columns) for its tests; T005 → T006 → T007 → T008.
- Phase 2 blocks all stories. US1: T009 needs T004; T010 ∥ T011 ∥ T013 (T013 needs T009's `schema` on `VaultDevice`); T012 needs T009–T011. US2 (T014) and US3 (T015, T016) edit `DeviceRow.vue` after T012, so they run in order, not parallel.
- Polish after all stories; T018 before T019.

## Parallel Example

```text
After T003: T004 (peer_schema.rs) alongside T005 (wire.rs)
After T009: T010 (deviceStatus.ts + check) ∥ T011 (locales) ∥ T013 (agent action)
```

## Implementation Strategy

MVP = Phase 1 + 2 + US1 (T001–T013): the owner already sees which device lags and gets the named message. US2 and
US3 are small additions on `DeviceRow.vue`. Ship as one PR; do not split the wire change from the display, because
the display needs the values and the values need the wire change.
