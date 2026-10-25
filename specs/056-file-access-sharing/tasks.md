---
description: 'Task list for spec 056-file-access-sharing'
---

# Tasks: Dateizugriff zwischen eigenen Geräten und Dateifreigaben an Kontakte

**Input**: Design documents from `/specs/056-file-access-sharing/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/files-protocol.md](./contracts/files-protocol.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md), [quickstart.md](./quickstart.md),
[ADR 0015](../../docs/adr/0015-file-access-between-devices-and-contacts.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic; SC-004 (revoke
ends streams), SC-005 (every path escape refused), SC-007 (no access without a valid device list) and
SC-008 (no half files) need dedicated test catalogs. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. No network services and no `sleep` in
`cargo test`: two vaults in one process on loopback endpoints with `RelayMode::Disabled`
(`src-tauri/src/sync/endpoint_tests.rs:18`) and `nostr_sdk::local_relay::MockRelay`
(`src-tauri/src/sync/presence_loop_tests.rs:104`). Build only the affected targets with `-j 4`; ask the
operator before a full workspace build (OOM risk on the 31 GB machine).

**Organization**: Grouped by user story in priority order (P1: US1, US2; P2: US3, US4; P3: US5, US6).

**Shipping note** (plan.md, Lieferungen): PR A = Phase 1 + Phase 2 (foundation, no visible change);
PR B = US1; PR C = US2; PR D = US3; PR E = US4 + US5; PR F = US6 (incl. agents managing shares) + Polish. Each PR is reviewed and
merged before the next one starts on top of `main`. Work in `.worktrees/056-file-access-sharing` (other
sessions switch branches in the main checkout).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US6)

---

## Phase 1: Setup (PR A)

- [x] T001 Spec, plan, research, data model, contracts, quickstart and ADR 0015 in `specs/056-file-access-sharing/` and `docs/adr/0015-file-access-between-devices-and-contacts.md`; spaces specs 027 (kept for 028) and 029 (deleted) aligned
- [x] T002 [P] Row for 056 in `plans/README.md` replacing "Umbau 027/029"
- [x] T003 Prepare the worktree for code: real `nix develop --command pnpm install --frozen-lockfile`; check `df -h` and reflink `src-tauri/target` from a merged worktree with the same `Cargo.lock` if one exists (memory: worktree targets fill the disk); check `nix develop --command scripts/with-nix-host-bridge.sh cargo check -j 4 --manifest-path src-tauri/Cargo.toml`
- [x] T004 [P] Before the first new name, run bounded `graphify query … --budget 1200` for "file share grant per contact", "peer device storage", "device-local setting no_sync", "conflict on upload finish"; note the candidates checked in the PR description (plan.md Constitution Check lists the starting set) — checked in PR A (#540): no candidate fit; `contacts::access::Grant` is the contacts agent grant of spec 034, a different thing
- [x] T005 [P] Glossary: add **Dateifreigabe / Zugriffsrecht / Empfangene Dateifreigabe / Geräte-Dateizugriff / Anklopfen** (Anklopfen removed 2026-10-23, research R9) to `CONTEXT.md` (new section "Dateizugriff und Dateifreigaben (spec 056)"), with _Avoid_: "Freigabe" alone (spec 034 Grant), "Space", "Share" in user-facing text

---

## Phase 2: Foundational (PR A, no visible change)

**Purpose**: Tables, error codes, the `Peer` protocol, the wire frame, the handshake, the server
skeleton with its per-request check, and `PeerStore`. Nothing is offered in the UI yet; every later
story builds on these.

### Data

- [x] T006 Migration `0039_file_shares` in a new `src-tauri/src/identity/migrations_file_shares.rs` with exactly the four tables of data-model.md (`haex_file_shares` with FK `known_devices(vault_device_uuid) ON DELETE CASCADE`, `haex_file_share_grants` with FK `haex_file_shares(id) ON DELETE CASCADE`, `haex_file_shares_received`, `haex_file_device_access_no_sync`), no UNIQUE constraints, `#[cfg(test)] SYNCED_TABLES = ["haex_file_shares", "haex_file_share_grants", "haex_file_shares_received"]`; register it in `src-tauri/src/identity/migrations.rs` and bump `HOLZI_TRIGGER_VERSION` 26 → 27 with a changelog line
- [x] T007 [P] Migration test in `src-tauri/src/identity/migrations_file_shares_tests.rs` following `migrations_icon_sets_tests.rs`: tables exist, synced tables get CRDT triggers, `_no_sync` tables do not, deleting a share cascades its grants
- [x] T008 Share store in a new `src-tauri/src/files/share/store.rs` (+ `src-tauri/src/files/share/mod.rs` with `FileShareLevel { Read, ReadWrite }`, `FileShareItemKind { Folder, File }`, ts-rs names per data-model.md, `#[ts(export, export_to = "../../src/types/bindings/")]`): `share_id(device, source, path)`, `grant_id(share_id, vault_pubkey)` as UUIDv5 over `\n`-joined parts; `get_share`, `share_of(source, path, device)`, `shares_served_by(device)`, `grants_of(share)`, `grant_for(share, vault)`, `save_share(tx, …)` (check-then-write, sets `changed_at`), `save_grant`, `delete_grant` and `delete_share` (both set `changed_at` on the share), `set_served(tx, share, changed_at)`, `set_source_state(tx, share, state)`; reads take `&mut impl Query`, writes `&mut CrdtTransaction<'_>` — names as built: `share_at`, `create_share`, `rename_share`, `shares_for_vault`
- [x] T009 [P] Tests in `src-tauri/src/files/share/store_tests.rs`: derived ids are stable; saving the same source twice updates one row (FR-017); every grant change bumps `changed_at`; `served_at < changed_at` reads as pending; `source` refuses `peer:`/`share:` (FR-015); an update from another own device changes name and grants and bumps `changed_at` but keeps source and serving device (FR-019)
- [x] T010 [P] Device switch in a new `src-tauri/src/files/share/device_access.rs`: `enabled(q) -> Result<bool>` (missing row = true), `set_enabled(tx, bool)`; test in `src-tauri/src/files/share/device_access_tests.rs` (default on, off persists, table is `_no_sync`)
- [x] T011 [P] ~~Peer list high-water table~~ — superseded (2026-10-23): the generation rule of `sync::foreign_auth` (spec 057, card generation or one seen this session) applies; no table `haex_file_peer_lists_no_sync`; update the `ponytail:` comment on `SeenGenerations` in `src-tauri/src/sync/foreign_auth.rs` so it no longer points to that table

### Errors and protocol variant

- [x] T012 Add `StorageError::Conflict` in `src-tauri/src/remote_storage/mod.rs` and `FilesErrorCode::Conflict` and `FilesErrorCode::Unreachable` in `src-tauri/src/files/mod.rs`; map them in `storage_error` (`src-tauri/src/files/storage_source.rs:37`); add matching i18n keys `files.error.conflict` and `files.error.unreachable` in `src/i18n/locales/de.json` and `en.json` ("Die Datei wurde inzwischen geändert." / "Nicht erreichbar.") — `FilesErrorCode::Unreachable` and its key existed already
- [x] T013 Add `Protocol::Peer` in `src-tauri/src/remote_storage/mod.rs` like `Protocol::Local` (`#[serde(skip)] #[ts(skip)]`, `is_tree() == true`, `Protocol::parse` refuses it); `ProtocolStore` (`src-tauri/src/remote_storage/protocol.rs:175`) gets an optional `peer: Arc<dyn ObjectStore>` field routed in `of()`; storage settings, `remove_storage` of agent permissions (`src-tauri/src/files/permissions.rs:345`) and extension scopes skip ids starting with `peer:` or `share:` (research R2 Folgen); test in `src-tauri/src/remote_storage/protocol_tests.rs` that a stored connection can never carry `Peer` — as built: `ProtocolStore` routes `Peer` to no store and the file browser builds `StorageFiles` over `PeerStore` directly (T034); no stored storage carries a `peer:`/`share:` id, so settings, `remove_storage` and extension scopes need no skip until agents reach other devices (T069)

### Wire

- [x] T014 `FILES_ALPN = b"haex-vault-files/1"` in `src-tauri/src/sync/wire.rs` (naming scheme of spec 054, like `EXT_PEER_ALPN`); `Domain::FilesAuth` with the domain string `haex-vault-files-auth/v1` in `src-tauri/src/sync/signing.rs`; test in `src-tauri/src/sync/signing_tests.rs` that a signature in another domain does not verify as `FilesAuth`
- [x] T015 Frame in a new `src-tauri/src/files/peer/frame.rs`: `write_head<T: Serialize>(send, &T)`, `read_head<T: DeserializeOwned>(recv) -> Result<T, FrameError>` (u32 BE length + postcard, max 64 KiB, `v` checked), raw body copy with length in 256 KiB chunks; message types of contracts/files-protocol.md (`Welcome`, `Mode`, `Request` enum, `Response` enum, `WireEntry`, `WireError`); reuse `sync::wire::write_value`/`expect_value` for heads where they fit in `src-tauri/src/files/peer/mod.rs`; tests in `src-tauri/src/files/peer/frame_tests.rs` (round trip, head over 64 KiB refused, truncated body is an error, unknown `v` refused) over in-memory streams — as built in `src-tauri/src/files/peer/wire.rs` (+ `wire_tests.rs`); the read body streams raw after its head

### Handshake

- [x] T016 Contacts lookup: `ContactsService::card_of_vault` exists from spec 057 (`ca08356a`, `src-tauri/src/contacts/service/cards.rs`) and admits `Caller::Internal { feature: "fileShares" }`; used through `sync::foreign_auth`
- [x] T017 Handshake in a new `src-tauri/src/files/peer/handshake.rs` over `sync::foreign_auth::{dial, accept}` (spec 057) with `PURPOSE = Purpose { alpn: FILES_ALPN, domain: Domain::FilesAuth }` and `Local { feature: "fileShares", seen }`; the serving side maps `Trust` to `Mode` per contracts/files-protocol.md (`OwnVault` + switch on → `OwnDevice`, switch off → `AccessOff`; `Contact` + ≥ 1 grant on a share served by this device → `Contact { vault }`; else close without reason) and sends `Welcome { v, mode, trash }`; the dialing side reads `Welcome`; the whole exchange within `foreign_auth::HANDSHAKE_LIMIT`; `SeenGenerations` per session in the server and client state
- [x] T018 Handshake tests in `src-tauri/src/files/peer/handshake_tests.rs` with the fixtures of `src-tauri/src/sync/foreign_auth_tests.rs` / `src-tauri/src/sync/test_support.rs` (two vaults, a linked device) over in-memory streams (SC-007): own linked device → `OwnDevice`; switch off → `AccessOff`; foreign vault with card and grant → `Contact`; with card but no grant on a share of this device, blocked, unknown → closed; the list and endpoint checks are covered by `foreign_auth_tests.rs` and not repeated — as built over loopback endpoints (T026), not in-memory streams

### Server skeleton and check

- [x] T019 Per-request check in a new `src-tauri/src/files/peer/serve/check.rs`: `check(mode, caller, request, db) -> Result<Target, WireError>` (`caller: User | Agent` sent with every request) re-reading grant and level from the database on every call (FR-005), mapping the request to a share root (contact) or the device (own), refusing write requests for `Read`, root rename/move/delete (FR-037), anything but `Read`/`Stat`/upload with `expected = Version` on a file share; holzi's own data: `OwnDevice` + `User` read-only like the local file browser (`access::check`, `src-tauri/src/files/access.rs:106`), `OwnDevice` + `Agent` and every `Contact` blocked (FR-007); pure decisions separated from I/O — as built: `allowed(mode, op, grant)`, `own_data(mode, caller, writes, own)`, `hides_own`; the re-reads (switch, card, grant) in `serve/mod.rs` `permit`
- [x] T020 Roots in a new `src-tauri/src/files/peer/serve/roots.rs`: window path → `StorageFiles` + inner path (research R3): device paths `/` and `/C:/…` to a `LocalStore` per drive like `DeviceRoot` (`src-tauri/src/files/encrypted/device.rs:32`) with the encrypted-folder scope; `storage:<id>` sources through `storage_files` with scope; a share root becomes the `LocalStore` root or the storage prefix; every resolved device path checked with `OwnPlaces::contains_resolved`/`touches` (`src-tauri/src/files/local/places.rs:48`) and `has_raw_part`, the decision taken by T019 — PR A: device paths and device sources; `storage:` and encrypted sources answer `Unsupported` until T081, the root listing of Windows drives comes with T028
- [x] T021 Path containment catalog in `src-tauri/src/files/peer/serve/roots_tests.rs` (SC-005): `..` in every position, absolute paths, symlink out of the root (file and folder), symlink inside the root allowed, link into holzi's own data, Windows `:` and `\`, case variants on a case-insensitive temp dir where supported, move and rename targets outside the root; every case refused; holzi's own data readable but not writable for `OwnDevice` + `User`, refused for `OwnDevice` + `Agent` and for `Contact` — PR A: path parts and links in `roots_tests.rs` and `serve_tests.rs`; move and rename targets with T030
- [x] T022 Server in a new `src-tauri/src/files/peer/serve/mod.rs`: `FilesProtocol` implementing iroh `ProtocolHandler`; per connection run the handshake on the first stream, then accept bi-streams, each request on the session tracker; limits 16 streams per connection, 64 per device → `Busy` (research R6); file-system work on `spawn_blocking`; errors to the peer as `WireError` without text, logged locally with cause (FR-042)
- [x] T023 Endpoint wiring: register `FILES_ALPN` with `Protocols::with` beside the extension peers in `sync::service::run` (`src-tauri/src/sync/service.rs`, `peer_protocols` as the model) — the `FilesProtocol` state holds db, keys, vault, `FilesState`, storage service and the session `SeenGenerations`; dial with `SyncNode::dial_alpn(addr, FILES_ALPN)`; extend `src-tauri/src/sync/endpoint_protocols_tests.rs` so both extra ALPNs are accepted on one endpoint, and assert the endpoint still uses `presets::Minimal` without address publishing (FR-010) — no extra assertion on `presets::Minimal`: the endpoint is built once in `sync/endpoint.rs`, unchanged

### Client

- [x] T024 Connections in a new `src-tauri/src/files/peer/connect.rs`: one `haex-vault-files/1` connection per target device (`dial_alpn`), dialed with the address from presence (own devices) or from the stored card (contacts, US2), handshake (T017) on open, reconnect on demand, dropped on vault lock — `src-tauri/src/files/peer/connect.rs`, held by `SyncRuntime::files`; contacts (`share:`) answer `Unsupported` until US2 (dialed from the card, research R9)
- [x] T025 `PeerStore` in a new `src-tauri/src/files/peer/store.rs` implementing `ObjectStore` (`src-tauri/src/remote_storage/object_store.rs:13`): target from `Location.endpoint` (`peer:<uuid>` / `share:<id>`), keys ↔ window paths for a tree protocol, `list_dir`/`head`/`get_range` (raw body as `AsyncRead`, no deadline after start)/`list` (recursive by walking `ListDir`, bounded by `max`); write methods answer `Unsupported` until US1; `copies_on_provider == false`; `WireError` → `StorageError` per contracts/files-protocol.md
- [x] T026 Test support in a new `src-tauri/src/files/peer/test_support.rs`: two vaults (`Member::genesis()`), a linked second device, loopback endpoints with the files handler, a temp dir as device file system, helpers to save a card of the other vault and a grant
- [x] T027 Read round trip in `src-tauri/src/files/peer/store_tests.rs`: own device lists, stats and reads a range through `PeerStore`; the protocol run of `src-tauri/src/remote_storage/protocol_tests.rs` and the 048 acceptance over all stores in `src-tauri/src/files/encrypted/protocols_tests.rs` are extended by `PeerStore` for the read methods — the 048 run over all stores in `files/encrypted/protocols_tests.rs` needs writes and follows with T032

**Checkpoint**: PR A complete — `cargo test` for `files::peer`, `files::share`, `identity::migrations`, `sync::endpoint` green, no UI change.

---

## Phase 3: User Story 1 - Auf die Dateien eines anderen eigenen Geräts zugreifen (Priority: P1) 🎯 MVP (PR B)

**Goal**: Every other own device appears under "Meine Geräte" and can be browsed, viewed, copied into, renamed in and deleted from.

**Independent Test**: Two devices of one vault; list, open a PDF, play a video, upload, rename, delete; check the result on the serving device (spec US1).

### Server operations

- [x] T028 [US1] Read operations in a new `src-tauri/src/files/peer/serve/ops.rs`: `Shares` for `OwnDevice` answers `roots` (drives and known places, same data as `files_sources` for the device, `src-tauri/src/files/browser_commands/navigate.rs:63`), `ListDir`, `Stat`, `Read` (head with size and content type, then raw body from `StorageFiles::read_start`/`get_range`) over T020 — PR A pulled `ListDir`, `Stat`, `Read` and `Shares` for contacts forward for T027; open: `Shares` for `OwnDevice` — PR B: `Shares` of an own device answers `Roots { drives, known }`; `ListDir /` on a Windows device lists the drives; `Read` answers `Reading { size, len }` without a content type (the window infers it from the name)
- [x] T029 [US1] Upload sessions in a new `src-tauri/src/files/peer/serve/upload.rs`: `BeginUpload` (records target and `expected`), `UploadPart` (raw body), `FinishUpload` (re-checks `expected` against the current `head().etag`/absence → `Conflict`, then finishes through the target's upload path: `LocalStore` temp + rename, storage multipart, encrypted `write_version`), `AbortUpload`; sessions belong to the connection and are cleaned up on close, cancel or lock (FR-035, FR-044) — as built for device paths; storage and encrypted share sources follow with T081
- [x] T030 [US1] Change operations in `src-tauri/src/files/peer/serve/ops.rs`: `CreateFolder`, `Rename`, `Move` (same root only), `Delete` (device: `transfer::entries::remove(path, Removal::Trash)` made `pub(crate)` in `src-tauri/src/files/transfer/entries.rs:20`; storages and encrypted folders: their delete); `Challenge.trash` from `platform_removal()` (`src-tauri/src/files/transfer/mod.rs:434`) (research R8) — as built: `CreateFolder { path }`, `Move` also renames (no separate `Rename`), entries are moved and removed as themselves (links not followed); storage and encrypted sources with T081
- [x] T031 [US1] Watch on the server in a new `src-tauri/src/files/peer/serve/watch.rs`: `Watch { share?, path }` on its own long-lived stream starts `local::watch::watch_folder` (`src-tauri/src/files/local/watch.rs:60`; encrypted folders on the device as in `files_watch`, `src-tauri/src/files/browser_commands/navigate.rs:205`), sends `FolderChanged { paths }` as window paths per debounced batch, ends with the stream; storage sources answer `Unsupported` (FR-046, research R16) — as built: `Done`, then `Changed` per batch without paths; checked again with every batch and every 10 s
- [x] T032 [P] [US1] Tests in `src-tauri/src/files/peer/serve/ops_tests.rs` and `upload_tests.rs`: listing equals the local listing; a range read returns exactly the bytes; upload aborted mid-way leaves no file and no temp file (SC-008); finish with a stale `expected` → `Conflict` and the old file intact; delete goes to the trash where `trash = true` (fake removal hook); holzi's own data is listable and readable but refuses uploads, finishes and deletes for `OwnDevice + User`, while every operation remains refused for `OwnDevice + Agent` and `Contact` — as built in `serve/upload_tests.rs`, `serve/check_tests.rs` and `peer/own_device_tests.rs`; the server takes `Removal` from its context, tests run with `Permanent`, so the trash path itself (the `trash` crate) is not run in tests

### Client

- [x] T033 [US1] Write methods of `PeerStore` in `src-tauri/src/files/peer/store.rs`: `begin_upload` heads the target first and sends `expected`, `upload_part`, `finish_upload`, `abort_upload`, `put` as one-part upload, `delete`/`remove_tree` → `Delete`, `rename_tree` → `Move`/`Rename`, `copy` → `Unsupported`; extend the catalog run of T027 to the write methods
- [x] T034 [US1] Resolve `peer:<vault_device_uuid>` in `storage_files` (`src-tauri/src/files/browser_commands/mod.rs:34`) and `window_storage` without `haex_storages`: `StorageFiles::new(stores(), peer_access(id))` with the encrypted-folder scope off (the serving device decrypts); unknown or own device → `NotFound` — agents get `peer:` storages with `Caller::Agent` in `files/agent/env.rs`; their permissions follow with T069
- [x] T035 [US1] `files_watch` (`src-tauri/src/files/browser_commands/navigate.rs:205`) accepts `peer:`/`share:` storages: opens a `Watch` stream through `files/peer/connect.rs` and forwards each `FolderChanged` to the window's `Channel<FolderChanged>`; on stream end sends a final event so the window shows "nicht erreichbar"; `src/composables/useFilesFolder.ts:67,77` watches these sources like the device
- [x] T036 [US1] `Sources.devices: Vec<FilesPeerDevice>` in `src-tauri/src/files/browser_commands/navigate.rs:37`: every other device of the effective list with name (from `known_devices`) and state `reachable` / `unreachable` (presence) / `accessOff` (authenticated handshake returned `AccessOff` because of the switch, remembered per session) — `FilesPeerDevice { id, name, state }`; `reachable` = a sync session is open; plus `files_peer_roots` for the start folder
- [x] T037 [P] [US1] Integration test in `src-tauri/src/files/peer/own_device_tests.rs`: linked device browses, reads a range, uploads, renames, deletes on the other device through `storage_files("peer:…")` and `StorageFiles`; switch off on the serving device → refused; holzi's own data listed and readable but not writable (scenario 7); a file created in a watched folder arrives as `FolderChanged` within 2 s (scenario 8, SC-009) (spec US1 scenarios 2, 4, 5, 7, 8); copying a file whose name exists at the target asks the conflict question and "Beide behalten" picks a free name (FR-039) — range reads stay in `store_tests.rs`

### Frontend

- [x] T038 [US1] Sidebar group "Meine Geräte" in `src/components/files/Sidebar.vue`: one row per `FilesPeerDevice` with state badge, unreachable and `accessOff` rows not clickable (FR-011); `pickStorage` with the `peer:` id; i18n `files.sidebar.devices`, `files.peer.reachable`, `files.peer.unreachable`, `files.peer.accessOff` in `src/i18n/locales/de.json` and `en.json`
- [x] T039 [US1] Routing and titles: check that `OPAQUE_ID` in `src/lib/files/registry.ts` accepts `peer:` ids (and later `share:`); tab title from the device name; `src/lib/files/encrypted.ts` `canCreateEncrypted` false for `peer:` sources (the serving device owns encryption) — route `peer/:id` with the title "Anderes Gerät" (the window manager titles by route params only); the breadcrumb root shows the device name
- [x] T040 [US1] Run `nix develop --command pnpm typecheck`, `pnpm lint`, `pnpm typecheck:scripts`, `pnpm format:check`, and all `check:*` scripts; trim trailing whitespace in `src/types/bindings/*.ts` as CI does — plain `pnpm` on Pop!_OS, as CI does

**Checkpoint**: PR B — US1 testable on two own devices (quickstart.md §1 steps 1–5).

---

## Phase 4: User Story 2 - Einen Ordner an einen Kontakt freigeben und dort öffnen (Priority: P1) (PR C)

**Goal**: The owner shares a folder or file with contacts (read); the contact reaches the owner's devices from the stored card and opens it as a storage.

**Independent Test**: Two vaults with each other's card; share a folder read-only; the other vault opens it, views and copies out, sees nothing outside the root and cannot write (spec US2).

### Owner side

- [x] T041 [US2] Share commands in a new `src-tauri/src/files/share/commands.rs` per contracts/tauri-commands.md: `file_shares_list`, `file_share_of`, `file_share_save` (`shareId` absent: create, only sources of this device — `device` or a `haex_storages` id, else `Refused`, serving device = this device; `shareId` present: update name and grants from any own device, source and serving device unchanged; grants only for vaults with a stored card; FR-015–FR-019), `file_share_revoke`, `file_share_delete`, `file_shares_of_contact`; register in `src-tauri/src/lib.rs` — logic in `src-tauri/src/files/share/manage.rs` (+ `manage_tests.rs`), commands thin; a new share takes only `device` sources until T081 (storages answer `Unsupported`); a change made on the serving device sets `served_at` at once, other devices leave the share pending until US4
- [x] T042 [P] [US2] Catalog actions `files.shares.list|of|save|revoke|delete|ofContact` with a new scope `files.shares` (`src/lib/actions/scopes.ts`) in `src/lib/actions/filesActions.ts` and handlers in `src/stores/filesActionHandlers.ts`; until US6 the agent executor refuses every `files.shares.*` call (FR-045); test that an agent call is refused — in a new `src/lib/actions/fileShareActions.ts` with handlers in `src/stores/fileShareActionHandlers.ts`; `agentCallable: false` until US6, the catalog invariant in `scripts/check-wm-actions.ts` names the exception
- [x] T043 [US2] Share dialog in a new `src/components/files/ShareDialog.vue` and `src/composables/useFileShares.ts`: opened from "Freigeben …" in the context menu of a folder or file of this device (not on `peer:`/`share:` sources); contact picker only with confirmed cards; level per contact (default "Lesen"); name (default item name); existing grants editable (FR-016, FR-017); i18n `files.shares.*` — "Freigeben …" in the entry menu (`share` command) for one entry of this device outside holzi's own data and encrypted folders

### Reaching contacts

- [x] T044 [US2] ~~Knock in a new `src-tauri/src/files/peer/knock.rs` per contracts/nostr-knock.md: kinds 24103/24104 inside `presence::wrap_payload`/`unwrap_rumor` (`src-tauri/src/sync/presence.rs:225`, `:283`) addressed to device keys; own `nostr_sdk` client subscribing to gift wraps for the own device key on `effective_nostr_relays()`; answer only after the list check with the rule of `sync::foreign_auth`, card stored and not blocked, ≥ 1 grant on a share served by this device; freshness 150 s / 30 s, nonce replay set, at most one answer per vault in 5 s; start it in `sync::service::run` next to the presence loop and reconnect on the same relay wakeups~~ — superseded (2026-10-23): contacts are dialed from the card like spec 057, no knock
- [x] T045 [US2] ~~Knocking side in `src-tauri/src/files/peer/knock.rs`: `knock(contact) -> Result<Vec<EndpointAddr>>` to every device of the card on the card's relays, waits up to 10 s for answers whose nonce and signer match, adds addresses to the endpoint's `MemoryLookup` (`note_candidate`, `src-tauri/src/sync/endpoint.rs:354`)~~ — superseded (2026-10-23): contacts are dialed from the card like spec 057, no knock
- [x] T046 [P] [US2] ~~Knock tests in `src-tauri/src/files/peer/knock_tests.rs` with `MockRelay`: entitled vault gets an answer with the endpoint address; no grant, no card, blocked vault, stale `ts`, replayed nonce, wrong list → no answer; throttling per vault; the knock client publishes nothing but gift wraps (FR-010)~~ — superseded (2026-10-23): contacts are dialed from the card like spec 057, no knock

### Contact side

- [x] T047 [US2] `Shares` for `Mode::Contact` in `src-tauri/src/files/peer/serve/ops.rs`: the shares of this device with a grant for that vault (id, name, item kind, level); every other request needs `share` and goes through T019/T020 with the share root — built in PR A
- [x] T048 [US2] Received shares in a new `src-tauri/src/files/share/received.rs`: `apply_answer(tx, owner_vault, owner_device, shares)` creates/updates rows and deletes only rows of the same `owner_device` missing from the answer (FR-030, FR-031); `files_contact_query { contactId }` in `src-tauri/src/files/share/commands.rs` dials every device of the contact's stored card in parallel (`endpoint_addr_of(endpoint_id, iroh_relay)`, 10 s, research R9), asks each for `Shares`, applies each answer per device, returns the union as `FilesContactShares` with `reachable` per share (devices that did not answer leave their known shares unreachable); test in `src-tauri/src/files/share/received_tests.rs` (two devices of the contact, one answers: the other device's shares stay) — `files_contact_query` in `src-tauri/src/files/browser_commands/peers.rs` over `src-tauri/src/files/peer/contacts.rs`; a device that refuses or does not answer keeps its known shares, unreachable
- [x] T049 [US2] Resolve `share:<received_id>` in `storage_files`/`window_storage` (`src-tauri/src/files/browser_commands/mod.rs`): `PeerStore` targeting the owner device with the share id; opening a known share dials the owner device from the stored card — the only dial without an explicit expand (FR-029) — also `files_watch` and agents (`Caller::Agent`); all shares of one device share one connection
- [x] T050 [US2] `Sources.contacts: Vec<FilesContactShares>` in `src-tauri/src/files/browser_commands/navigate.rs`: contacts with a stored card, their known received shares with `reachable` from the last contact — `reachable` = a live link to the serving device this session
- [x] T051 [P] [US2] Integration test in `src-tauri/src/files/peer/contact_tests.rs`: vault B with card and read grant lists and reads A's share root through `storage_files("share:…")`; listing above or beside the root refused; write requests refused at level read even when sent directly; second device of B gets access without anything done on A (spec US2 scenarios 2–4, 6); single-file share shows exactly that file (scenario 8); holzi's own data inside a shared folder refused; `Watch` on a shared device folder delivers changes; copying a file whose name exists at the target asks the conflict question and "Beide behalten" picks a free name (FR-039) — scenario 7 (storage and encrypted sources) moved to T081 (2026-10-23) — single-file shares and holzi's own data inside a share stay covered by `serve/serve_tests.rs`

### Frontend

- [x] T052 [US2] Sidebar group "Von Kontakten" in `src/components/files/Sidebar.vue`: contacts expandable; expanding calls `files_contact_query`; shares listed with level; known but unreachable shares greyed out (FR-028, FR-031); path bar ends at the share root (FR-033) in `src/components/apps/FilesApp.vue`; read-only shares hide write actions (FR-034) — route `share/:id` with the title "Freigabe"; the breadcrumb root shows the share name; a read share disables every change (FR-034)
- [x] T053 [US2] Run the frontend checks of T040

**Checkpoint**: PR C — US2 testable with two vaults (quickstart.md §2).

---

## Phase 5: User Story 3 - Gemeinsam an einem Ordner arbeiten (Priority: P2) (PR D)

**Goal**: Contacts with "Lesen und Schreiben" create, change, rename, move and delete below the root; a concurrent change ends in a conflict question.

**Independent Test**: Share read-write with two contacts; both write; change a file at the owner while a contact uploads it (spec US3).

- [x] T054 [US3] Contact writes in `src-tauri/src/files/peer/serve/check.rs` and `ops.rs`: `ReadWrite` allows upload, create folder, rename, move, delete below the root; root protected; file share only content upload with `expected = Version` (FR-037) — already in place with PR B/C (`check::allowed`, `check::at_root`); PR D adds the end-to-end tests of T057
- [x] T055 [US3] Conflict in transfers: in `src-tauri/src/files/transfer/remote.rs` and `remote_upload.rs` map `StorageError::Conflict` at `finish_upload` to the existing conflict question (`Hooks.ask`): "Ersetzen" retries without `expected`, "Beide behalten" uploads under `free_name`, "Überspringen" aborts the upload (research R7); `ponytail:` comment on the unchecked window between question and `begin_upload` — "Ersetzen" sends again, so `begin_upload` names the version there now; a second conflict on the same file ends the transfer instead of asking in a loop
- [x] T056 [US3] Delete without trash: when the connection reported `trash = false`, `StorageFiles::delete` on a `peer:`/`share:` source asks first like `asksBeforeDelete` in `src/lib/files/encrypted.ts:60` (FR-038); the answer `trashed` is shown in the transfer bar — new command `files_peer_trash` reads `trash` of the connection (a device's `trashed` always equals it); the transfer bar drops finished transfers, so a deletion into the other device's trash ends with a notice instead
- [x] T057 [P] [US3] Tests in `src-tauri/src/files/peer/contact_write_tests.rs`: two contacts write into one share; root rename/delete refused; file share rename/delete refused, content upload allowed; owner changes a file during a contact upload → `Conflict`, old content intact; "Ersetzen" and "Beide behalten" paths in `src-tauri/src/files/transfer/remote_tests.rs` with a fake store answering `Conflict` once — in `transfer/remote_conflict_tests.rs` (a child of `remote_tests.rs`, which is near the line limit)

**Checkpoint**: PR D — US3 testable (quickstart.md §3).

---

## Phase 6: User Story 4 - Dateifreigaben verwalten und entziehen (Priority: P2) (PR E)

**Goal**: Overview of all shares with state; revoke and downgrade take effect on the next request and end running streams; pending state when the serving device has not seen a change.

**Independent Test**: Revoke while the contact reads a large file; revoke from another own device while the serving device is off (spec US4).

- [x] T058 [US4] Served watcher in a new `src-tauri/src/files/share/served.rs`: subscribes to `AppState::vault_changes()` (`src-tauri/src/vault_events.rs:42`) for `haex_file_shares*` on the serving device, sets `served_at = changed_at` for its shares, cancels the stream tokens of (share, vault) pairs whose grant is gone or downgraded (FR-024); started with the files server in `sync::service::run` — `share::served::follow` runs in the session's select; it applies at the start and after every change of `haex_file_share*` or `haex_contacts*` (deleting a contact removes its card by cascade)
- [x] T059 [US4] Stream tokens in `src-tauri/src/files/peer/serve/mod.rs`: every request of a contact runs under a child token per (share, vault); `served.rs` cancels them; a cancelled `Read` ends with `STREAM_CANCELLED`, an upload aborts and removes its temp file — tokens per (share, vault, level) in `serve/streams.rs`; a grant that is lower now cancels; the checks moved to `serve/permit.rs`; a refused `again` drops the upload
- [x] T060 [US4] Source state: the serving device writes `source_state` (`ok` / `missing` / `unavailable`) on vault open, on save and whenever a request fails at the source (`NotFound` on the root → `missing`, storage or encrypted errors → `unavailable`) in `src-tauri/src/files/share/served.rs` (FR-022) — on start and every pass of `served.rs`, and in `permit::locate` when a contact's request reaches the root or not; storage and encrypted sources come with T081
- [x] T081 [US4] Storage and encrypted share sources on the serving side: `storage:<id>` and encrypted folders as `in_share` roots through `StorageFiles` (list, stat, read, uploads, changes; encrypted folders decrypted on the serving device), with the US2 scenario-7 tests in `src-tauri/src/files/peer/contact_tests.rs` (storage folder and encrypted folder return plain text, with `FakeStore` and the encrypted test scene); own PR after PR C (decided 2026-10-23) — as built: `serve/target.rs` dispatches device and storage targets, `serve/storage.rs` (opener `StorageOpener` in `ServeContext`, list/stat/read/folder/rename/delete through `StorageFiles` with the window's scope), `serve/storage_upload.rs` (parts passed on, sealed into encrypted folders); a move in a storage only renames in its folder, `Watch` answers `Unsupported`; `manage::save_from` creates storage shares (agents only outside encrypted folders); `served.rs` asks the storage for its root; tests in `serve/storage_tests.rs` instead of `contact_tests.rs` (line limit), plus `share/manage_tests.rs`
- [x] T061 [P] [US4] Tests in `src-tauri/src/files/share/served_tests.rs` and `src-tauri/src/files/peer/revoke_tests.rs` (SC-004): revoke during a range read ends the stream and the next request is refused; downgrade refuses the next write; change written on another device leaves the share pending until the serving device applies it; missing root sets `missing`
- [x] T062 [US4] Contact cleanup: subscriber on `haex_contacts_cards` in `src-tauri/src/files/share/served.rs` deletes grants and received shares of vaults without a card (FR-023); `contacts_delete_impact { contactId }` in `src-tauri/src/files/share/commands.rs`; test in `src-tauri/src/files/share/served_tests.rs` — no `contacts_delete_impact`: the dialog uses `files.shares.ofContact`; grants and received shares go only once a pass saw the card and a later one does not (a grant that sync brings before its card stays); a blocked vault keeps its grants (refused on every request) and gets them back when unblocked
- [x] T063 [US4] Overview "Meine Dateifreigaben" in a new `src/components/files/SharesOverview.vue` (reachable from the file browser sidebar): name, source, serving device, contacts with level, state `active` / `pending` ("Änderung wartet auf ‹Gerät›") / `sourceMissing` / `sourceUnavailable`; edit opens `ShareDialog.vue`, revoke per contact (FR-020) — a dialog opened from the sidebar entry "Meine Dateifreigaben"; `ShareDialog.vue` also edits a given share, served by any own device
- [x] T064 [P] [US4] Contact page section "Dateifreigaben" in `src/components/contacts/EntryPage.vue` (shares given to this contact, change and revoke, FR-021) and the list of affected shares in `src/components/contacts/DeleteDialog.vue` from `contacts_delete_impact` (FR-023); i18n `contacts.fileShares.*` — section in `src/components/contacts/FileSharesSection.vue`
- [x] T065 [US4] Run the frontend checks of T040

---

## Phase 7: User Story 5 - Ein Gerät vom Geräte-Dateizugriff ausnehmen (Priority: P3) (PR E)

**Goal**: A per-device switch, on by default, that only this device can turn off.

**Independent Test**: Switch off on device A; device B cannot open A; contacts still reach A's shares (spec US5).

- [x] T066 [US5] Commands `files_device_access_get`/`files_device_access_set` in `src-tauri/src/files/share/commands.rs` over T010; actions `settings.files.deviceAccess.get|set` with `scope: 'guardrails'` in `src/lib/actions/settingsActions.ts` and handlers in `src/stores/settingsActionHandlers.ts` — like the screen capture switch only `set` is an action; the window reads with the command
- [x] T067 [US5] Switch UI in a new `src/components/settings/FileDeviceAccessSetting.vue` following `ScreenCaptureSetting.vue:57-74` (SettingsGroup > SettingsRow > ShadcnSwitch, hint `settings.deviceOnly`), rendered in `src/components/settings/FederationView.vue`; `settingKeys` in `src/lib/settings/registry.ts` for search; i18n `settings.federation.deviceAccess.*`
- [x] T068 [P] [US5] Test in `src-tauri/src/files/peer/own_device_tests.rs`: switch off refuses own devices with the `accessOff` state and leaves contact shares working (spec US5 scenarios 2–4)

**Checkpoint**: PR E — US4 and US5 testable (quickstart.md §1 step 6, §4).

---

## Phase 8: User Story 6 - Agents arbeiten mit Dateien anderer Geräte und Dateifreigaben (Priority: P3) (PR F)

**Goal**: Agents reach `peer:`/`share:` sources only with a per-storage permission; the narrower of agent permission and contact level wins.

**Independent Test**: Grant or deny an agent "Laptop" and a received share; read and write through agent actions (spec US6).

- [x] T069 [US6] Agent reach in `src-tauri/src/files/agent/reach.rs` and `src-tauri/src/files/access.rs`: `storage:peer:…` and `storage:share:…` targets work like storages (`AgentFileKind::Storage`); the source name for questions and settings comes from the device name or the received share name instead of `haex_storages`; a write by an agent on a `share:` source with level `Read` is refused locally before sending (FR-041) — `AgentEnv::storages` adds the devices and received shares by name, `AgentEnv::read_only` answers for a share at level `Read`, `reach::storage` refuses with `files_read_only`
- [x] T070 [US6] Permission kind: `AgentFileKind::FileShares` in `src-tauri/src/files/permissions.rs:28` (`as_str`/`parse` `fileShares`, target empty, status `granted`/`denied`, `fits`, `set` target check, `grants_of` → new field in `AgentGrants`, `src-tauri/src/files/access.rs:75`); test in `src-tauri/src/files/permissions_tests.rs`
- [x] T071 [US6] Agent gate in the executor for `files.shares.*` (`src-tauri/src/files/agent/`): no `FileShares` grant → refused with the missing permission named; create, new contact or higher level → confirmation question `FileShareConfirmRequest` (data-model.md) through the window like `FilesAgentPermissionRequest` (`src-tauri/src/files/agent/prompt.rs:93`), always, no "remember", independent of the approval mode; no window or no answer in 120 s → refused; revoke, delete, downgrade without question; the contact resolved through the agent's contacts permission (spec 051) (FR-045, research R17) — `files/agent/shares.rs` with `SHARE_ACTION_IDS`, the confirmation in `files/agent/share_confirm.rs`; the actions are `runner: 'native'` for agents while the window keeps its handlers for the user; one confirmation names every contact that is new or gets more; a new share needs a folder the agent may read itself
- [x] T072 [US6] Confirmation dialog in a new `src/components/files/ShareConfirmDialog.vue`: source, path, device, contact name, level; buttons "Freigeben" / "Ablehnen", no remember option; i18n `files.shares.confirm.*`
- [x] T073 [US6] Agent settings UI: `src/lib/files/agentPermissions.ts` and `src/components/settings/AgentFilesSetting.vue` list own devices and received shares as storages with their names, plus the switch "Dateifreigaben verwalten" per agent (default off)
- [x] T074 [P] [US6] Tests in `src-tauri/src/files/agent/` (`FakeEnv`, `src-tauri/src/files/agent/test_env.rs`): no permission → question/refusal; `Read` permission refuses write; read-write permission on a read share refuses write (spec US6 scenarios 1–3); without `FileShares` a share action is refused (scenario 4); with it, create without confirmation never creates a share, also in mode "Auto" (scenario 5, SC-010); revoke runs without question (scenario 6) — `files/agent/remote_sources_tests.rs` (scenarios 1–3) and `files/agent/shares_tests.rs` (4–6)

---

## Phase 9: Polish & Cross-Cutting Concerns (PR F)

- [ ] T075 E2E per spec 033 in `scripts/e2e/scenarios/file-shares.test.ts`, helpers in `scripts/e2e/lib/` with checks via `pnpm check:e2e-lib`: second vault, card exchange, own-device browse and copy, contact share read, revoke; run locally only after asking (OOM risk), otherwise via CI
- [ ] T076 [P] Performance check against SC-001/SC-002/SC-006 on two devices in one network; note results in the PR (quickstart.md)
- [x] T077 [P] Docs: mark the 056 row in `plans/README.md` per merged PR; `specs/044-file-browser/spec.md` "Folgende Specs" points to the built feature; `CONTEXT.md` entries from T005 checked against the final names — done with PR G; the README row lists the PRs, the CONTEXT.md entries match the built names
- [x] T078 Size and structure: every new or touched hand-written file ≤ 500 LoC (`files/peer/serve/*`, `files/peer/store.rs`, `sync/endpoint.rs` already has a documented exception — do not grow it beyond the handler registration) — checked with PR G: every file 056 added or touched is ≤ 500 LoC (largest `share/manage.rs` 444, `peer/serve/mod.rs` 434); the files above 500 in `sync/` (`handshake.rs`, `presence_loop.rs`, `endpoint.rs` with its exception) were not grown by 056
- [ ] T079 Run quickstart.md §1–§5 by hand on two devices and two vaults; record open findings

- [ ] T080 Delete `specs/029-own-s3-storage/` once 056 is in use (it stays until then, commit `fe7c2784`); check that no document still links it and mark the 056 row in `plans/README.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none; T003 before any code.
- **Foundational (Phase 2)**: after Setup; blocks all stories. T006 → T008–T010; T012–T013 before T025; T014–T015 before T017; T017 before T018; T019–T021 before T022; T022 before T023; T024–T025 before T027.
- **US1 (Phase 3)**: after Phase 2. T028–T031 before T032; T033 after T029; T034 and T035 after T025; T036 before T038.
- **US2 (Phase 4)**: after US1 (connection layer, server ops, `Shares`). T041 before T043; T047–T050 before T051. T081 after T047.
- **US3 (Phase 5)**: after US2 (contact mode) and US1 (upload sessions).
- **US4 (Phase 6)**: after US2 (grants and contact streams).
- **US5 (Phase 7)**: after Phase 2 only (T010 exists); shipped with PR E for review size.
- **US6 (Phase 8)**: after US1 and US2. T070 before T071; T071 before T072 and T074.
- **Polish (Phase 9)**: after all stories.

### Parallel Opportunities

- Phase 2: T007, T009, T010 in parallel after T006; T014 parallel to T012/T013.
- US1: T032 and T037 in parallel once T028–T031 and T034–T035 are done.
- US2: T042 parallel to T041; T046 parallel to T047–T050.
- US4: T061 and T064 in parallel after T058–T060.

## Parallel Example: User Story 1

```text
Task: "T032 [P] [US1] Tests in src-tauri/src/files/peer/serve/ops_tests.rs and upload_tests.rs"
Task: "T037 [P] [US1] Integration test in src-tauri/src/files/peer/own_device_tests.rs"
```

## Implementation Strategy

### MVP First (User Story 1)

1. Phase 1 + Phase 2 (PR A) — foundation without visible change.
2. Phase 3 (PR B) — own devices: already useful on its own (laptop ↔ phone).
3. **Stop and validate** with quickstart.md §1.

### Incremental Delivery

1. PR C (US2): sharing read-only with contacts; then T081 (storage and encrypted share sources) as its own PR.
2. PR D (US3): writing and conflicts.
3. PR E (US4 + US5): management, revoke, switch.
4. PR F (US6 + Polish): agents, E2E, hand check.

## Notes

- Never `git stash` bare; never force-push; fetch before push (the operator pushes review fixes).
- Commits follow Conventional Commits without agent trailers.
- New ts-rs types use the prefixes `FileShare`/`FilesPeer`; check `ls src/types/bindings/<Name>.ts` before adding one.
- Clippy also for `aarch64-linux-android` before pushing code with `cfg` branches.
