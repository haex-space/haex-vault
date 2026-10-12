---
description: 'Task list for spec 050-webdav-sftp-storage'
---

# Tasks: WebDAV- und SFTP-Speicher neben S3

**Input**: Design documents from `/specs/050-webdav-sftp-storage/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/remote-store.md](./contracts/remote-store.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md), [contracts/bridge.md](./contracts/bridge.md),
[quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic; SC-002 (same
results on every protocol), SC-003 (no half files) and SC-004 (no login on an unconfirmed host key) need
dedicated test catalogs. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. No network services in `cargo test`:
`wiremock` for WebDAV requests, an in-process WebDAV server and an in-process russh SFTP server for
stateful tests (research R14). Build only the affected targets with `-j 4`.

**Organization**: Grouped by user story in priority order (P1: US1, US2, US3; P2: US4, US5, US6; P3:
US7). The operator ordered WebDAV before SFTP; US6 is split into a WebDAV part (with PR E) and an SFTP
part (with PR G).

**Shipping note** (plan.md, Lieferungen): PR A = Phase 1 (docs, #372); PR B = Phase 2 (refactor without
new behaviour); PR C = US1; PR D = US2; PR E = US3 + US6 (WebDAV) + the vault-sdk PR; PR F = US4; PR G =
US5 + US6 (SFTP); PR H = US7 after spec 048 is built. Each PR is reviewed and merged before the next one
starts on top of `main`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US7)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Spec, plan, research, data model, contracts, quickstart in `specs/050-webdav-sftp-storage/` (PR #372)
- [x] T002 [P] Add a row for 050 to `plans/README.md` after the 049 row: "050: WebDAV- und SFTP-Speicher neben S3 (`specs/050-webdav-sftp-storage/`)", priority P2, effort L, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-11; Lieferungen PR A–H", gate "038 und 044 im Einsatz; erst WebDAV, dann SFTP (Betreiber); nur Speicher der eigenen Vault, kein Teilen; US7 wartet auf 048"
- [x] T003 [P] Align spec 038: in `specs/038-storage-connections/spec.md` under "Nicht im Umfang" change the bullet "Andere Protokolle (IMAP, SMTP, ICS, WebDAV, FTP)" to name WebDAV and SFTP as built by spec 050 (`specs/050-webdav-sftp-storage/spec.md`); in `specs/044-file-browser/spec.md` under "Nicht im Umfang" change "Andere Protokolle als S3 (WebDAV, SMB, FTP)" so that WebDAV and SFTP point to spec 050
- [x] T004 [P] Align spec 048: in `specs/048-encrypted-folders/spec.md` under "Beziehung zu bestehenden Specs" add a bullet that spec 050 brings WebDAV and SFTP storages and requires encrypted folders to work there (050 FR-024), with `ObjectInfo.etag` introduced by whichever spec lands first (050 research R8)
- [x] T005 Run `pnpm format:check` (specs are covered), commit, push to PR #372

---

## Phase 2: Foundational (PR B, no behaviour change)

**Purpose**: Remove the assumption that every storage speaks S3 (FR-018) before any protocol lands. After this phase everything still works exactly as before, only through `ProtocolStore` with S3 as the only protocol in use.

- [x] T006 Prepare the worktree for code: real `pnpm install` (no symlinked `node_modules`), reflink `src-tauri/target` from a worktree with the same `Cargo.lock` if one exists, check `nix develop --command scripts/with-nix-host-bridge.sh cargo check -j 4 --manifest-path src-tauri/Cargo.toml`
- [x] T007 [P] Before the first new name, run bounded `graphify query … --budget 1200` for "protocol dispatch store per connection", "upload session multipart rename", "pinned http client builder", "tree folder marker prefix listing"; note candidates checked in the PR description (constitution graphify rule; the plan's candidate list in plan.md "Constitution Check" is the starting point; warn and continue if graphify fails)
- [x] T008 Migration: in `src-tauri/src/identity/migrations_storage.rs` add `STORAGE_PROTOCOLS` with exactly `ALTER TABLE haex_storage_connections ADD COLUMN protocol TEXT NOT NULL DEFAULT 's3';`, `ALTER TABLE haex_storage_connections ADD COLUMN host_key TEXT;`, `ALTER TABLE haex_storage_connections ADD COLUMN host_key_confirmed_at TEXT;` (statement breakpoints between), register it in `holzi_migration_source()` in `src-tauri/src/identity/migrations.rs` under the next free number (check `main` and spec 048's open PRs first), raise `HOLZI_TRIGGER_VERSION` by one with a doc line "`00NN_storage_protocols` (spec 050) added three columns to `haex_storage_connections`"; test in `src-tauri/src/identity/migrations_storage_tests.rs` (create if missing, registered in `migrations_storage.rs`): columns exist, an existing row reads `protocol = 's3'`, `host_key` NULL; extend `src-tauri/tests/vault_upgrade.rs` with a vault from before the migration **Note**: `0031_storage_protocols` (no clash with 048, which took 0030); trigger version 20; upgrade test `reopening_a_pre_0031_vault_tracks_the_protocol_of_a_connection` in `src-tauri/tests/vault_upgrade.rs` (PR #380)
- [x] T009 Types in `src-tauri/src/remote_storage/mod.rs`: add `Protocol { S3, WebDav, Sftp }` with `text_enum!` texts `s3`, `webdav`, `sftp` and ts-rs export; extend `ProviderKind` with `HetznerStorageBox = "hetznerStorageBox"`, `Nextcloud = "nextcloud"`, `Koofr = "koofr"`, `Kdrive = "kdrive"`; add `protocol: Protocol`, `host_key: Option<String>` to `ConnectionRow` and `Location` (`Location::of` copies them), add `Location::is_tree()` (WebDAV or SFTP); add `etag: Option<String>` to `ObjectInfo`; add `StorageError::{QuotaExceeded, HostKeyChanged, HostKeyUnknown, Unsupported}`; add `TestOutcome::{FolderMissing = "folderMissing", NotWebDav = "notWebDav", HostKeyUnknown = "hostKeyUnknown", HostKeyChanged = "hostKeyChanged"}`; update every construction (grep `ObjectInfo {` and `Location {`: `s3.rs`, `s3_ext.rs`, `test_support.rs`, `s3_tests.rs`, `s3_ext_tests.rs`, `address_tests.rs`) **Note**: `ObjectInfo.etag` was already added by spec 048 as a `String`; the trait moved to `src-tauri/src/remote_storage/remote_store.rs` to keep `mod.rs` under 500 lines
- [x] T010 `Credentials` as an enum in `src-tauri/src/remote_storage/mod.rs`: `S3 { access_key_id, secret_access_key, session_token }`, `Password { username, password }`, `SshKey { username, private_key, passphrase: Option<_> }`, every secret in `Zeroizing<String>`, `Debug` prints only `access_key_id`/`username`; update every user (grep `Credentials {` in `src-tauri/src/remote_storage/`: `credentials.rs`, `service.rs` `credentials_of`, `s3.rs` (match `S3`, any other variant → `StorageError::AccessDenied` with a log line), `test_support.rs`, `s3_tests.rs`, `s3_ext_tests.rs`); test in `src-tauri/src/remote_storage/mod_tests.rs` (or the existing tests file of `mod.rs`) that `format!("{:?}")` of each variant contains no secret **Note**: the S3 fields live in `S3Credentials`, next to `PasswordCredentials` and `SshKeyCredentials`; tests in `src-tauri/src/remote_storage/mod_tests.rs`
- [x] T011 Store: in `src-tauri/src/remote_storage/store.rs` read and write the three new columns (`connection_of`, `put_connection`), `check_connection` per protocol (data-model.md "Prüfregeln": `protocol` only `s3`/`webdav`/`sftp`; `provider_kind` must fit the protocol; WebDAV endpoint `https` or, only `local`, `http`, no user, query, fragment, "holzi ergänzt `/` am Ende"; SFTP endpoint `sftp://host` or `sftp://host:port` "(Port 1–65535), ohne Pfad"; region empty and addressing `path` for WebDAV/SFTP), `check_storage` per protocol (S3 `bucket_ok`; WebDAV/SFTP folder "leer für den Wurzelordner, sonst Teile mit `/` getrennt, ohne `/` am Anfang und Ende, kein Teil `.` oder `..`, kein Teil länger als 255 Byte, keine Steuerzeichen"); `put_connection` refuses a change of `protocol` on an existing id; tests in `src-tauri/src/remote_storage/store_tests.rs`
- [x] T012 [P] Shared HTTP client: move `S3Store::client_for` (`src-tauri/src/remote_storage/s3.rs:59-80`) to `pub(super) async fn pinned_client(url, scope, resolver)` in a new `src-tauri/src/remote_storage/http.rs` (no redirects, no proxy, user agent, `resolve_to_addrs`); `s3.rs` calls it; test in `src-tauri/src/remote_storage/http_tests.rs` (a redirect answer is not followed; a refused address is `Network`)
- [x] T013 Trait in `src-tauri/src/remote_storage/mod.rs`: rename `create_multipart`/`upload_part`/`complete_multipart`/`abort_multipart` to `begin_upload`/`upload_part`/`finish_upload`/`abort_upload` (same shapes, doc comments per contracts/remote-store.md); add `copies_on_provider(&self, access: &Access) -> bool`, `rename_tree(access, from, to, deadline)`, `remove_tree(access, prefix, deadline)`; update every implementation and caller (grep: `src-tauri/src/remote_storage/s3.rs`, `s3_multipart.rs`, `s3_ext_tests.rs`, `test_support.rs`, `src-tauri/src/files/transfer/remote.rs`); `S3Store`: `copies_on_provider` = true, `rename_tree`/`remove_tree` = `Err(StorageError::Unsupported)`; the fake in `test_support.rs` gets the same three methods with a tree mode switch for later tests **Note**: the fake's helpers moved to `src-tauri/src/remote_storage/test_support_env.rs` (re-exported) to keep `test_support.rs` under 500 lines; the fake of `src-tauri/src/files/encrypted/load_tests.rs` follows the trait too
- [x] T014 `ProtocolStore` in a new `src-tauri/src/remote_storage/protocol.rs`: holds `Arc<dyn RemoteStore>` per protocol (in PR B only S3; WebDAV and SFTP answer `Unsupported` until their PRs), implements `RemoteStore` by `match access.location.protocol`; replace `s3_store()` in `src-tauri/src/remote_storage/commands.rs` with `stores()` returning the `ProtocolStore`, update the callers `service()` in `commands.rs`, `src-tauri/src/files/browser_commands.rs:89` and `StorageState::store()` in `src-tauri/src/extensions/remote_storage_dialog.rs:114`; test in `src-tauri/src/remote_storage/protocol_tests.rs` (each protocol reaches its store; one fake per protocol records the calls)
- [x] T015 Transfer: in `src-tauri/src/files/transfer/remote.rs` use the renamed upload-session methods and copy on the provider only when `from.location == to.location && store.copies_on_provider(access)`, otherwise take the existing stream path (get_range + upload); test in `src-tauri/src/files/transfer/remote_tests.rs` with the fake answering `copies_on_provider = false` **Note**: also `same_bucket` in `src-tauri/src/files/transfer/remote_encrypted.rs` (spec 048) asks `copies_on_provider`
- [x] T016 Model and commands in `src-tauri/src/remote_storage/model.rs`: `ConnectionInput.protocol` (default `s3` for the existing frontend until PR C), `ConnectionInput.host_key: Option<String>`, `CredentialsInput` tagged by `kind` (`s3 | password | sshKey`, contracts/tauri-commands.md), `ConnectionView.protocol` and `host_key: Option<{fingerprint, confirmedAt}>`, `TestResult.host_key: Option<{key, fingerprint}>`; `service.rs` refuses credentials that do not fit the protocol ("`s3` nur für S3-Verbindungen, `password` für WebDAV und SFTP, `sshKey` nur für SFTP; sonst Fehler `invalid credentials`"); regenerate ts-rs bindings into `src/types/bindings/`; adapt `src/components/settings/storage/ConnectionForm.vue` and `src/components/StorageCredentialsModal.vue` to send `{ kind: 's3', … }` (no visible change) **Note**: `ConnectionView.hostKey` and `TestResult.hostKey` come with SFTP in PR F (T076); service tests in `src-tauri/src/remote_storage/service_protocol_tests.rs` (`service_tests.rs` is near 500 lines)
- [x] T017 Credentials entry in `src-tauri/src/remote_storage/credentials.rs` per data-model.md "Zugangsdaten-Eintrag": title `S3: <Anbieter>`, `WebDAV: <Anbieter>`, `SFTP: <Anbieter>`; WebDAV/SFTP password in `username`/`password`; SSH key in custom fields `privateKey` and optional `passphrase` with an empty `password`; `read` builds the variant from the connection's protocol and reports an entry without the needed fields as `missing`; tests in `src-tauri/src/remote_storage/credentials_tests.rs` (round trip per variant; an extension with a `*` grant still never reaches the entry, Z14)
- [x] T018 Run `cargo fmt --check`, `pnpm lint:rust`, `cargo test -j 4 --lib -- remote_storage files::transfer files::storage_source extensions::remote_storage identity::migrations`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, the E2E scenarios `files-storage`, `extension-storage`, `storage-two-devices` (no change expected); commit, open PR B **Note**: PR #380, merged 2026-10-12; E2E scenarios ran in CI

**Checkpoint**: S3 works as before through `ProtocolStore`; the trait and data model can carry WebDAV and SFTP.

---

## Phase 3: User Story 1 - WebDAV-Speicher in den Einstellungen verbinden (Priority: P1) 🎯 MVP (PR C)

**Goal**: A user connects a WebDAV storage in the settings with presets, the test runs before saving, a missing folder can be created on confirmation.

**Independent Test**: Against a local WebDAV test server create a connection and a storage, test, rename, change the credentials, remove; afterwards no credentials remain in the vault and the server's files are unchanged (quickstart §2, §5).

### Tests for User Story 1

- [x] T019 [P] [US1] Parser tests in `src-tauri/src/remote_storage/webdav/multistatus_tests.rs`: Nextcloud-, Apache- and SabreDAV-shaped `multistatus` samples (namespaces with and without prefix), `href` as absolute path and as full URI, percent-encoded names, the first entry (the folder itself) dropped, an `href` outside the requested path dropped and logged, `getlastmodified` RFC 1123, `getetag` without quotes, collections recognized by `resourcetype`
- [x] T020 [P] [US1] Request tests in `src-tauri/src/remote_storage/webdav/request_tests.rs` with `wiremock` (`method("PROPFIND")`, `header("Depth","1")`): folder URLs always end in `/`; a 3xx answer is not followed and is `Network`; 401 → `AccessDenied`, 403 → `MissingRight`, 404 → `NotFound`, 507 → `QuotaExceeded`, 5xx → `Network`; Basic auth header present only over `https` or a local `http` endpoint; `Destination` is the full URL on the same host with `Overwrite: T`
- [x] T021 [P] [US1] Store tests in `src-tauri/src/remote_storage/webdav/mod_tests.rs` with `wiremock`: `put` writes `.holzi-part-<UUID>` then `MOVE`s it; a failed `PUT` leaves no `MOVE` and deletes the temporary file; 409 on `PUT` below the storage folder → `MKCOL` step by step (405 = exists) and one retry; missing storage folder (`PROPFIND` 404) → `NotFound` without any `MKCOL`; `get_range` with 206 and with a 200 full answer; `head` via `PROPFIND` `Depth: 0`; with `tokio::time::pause` a `PROPFIND` whose answer never ends is `TimedOut` at the call's deadline (FR-013) **Note**: plus a test that a missing folder of the storage itself is never made
- [x] T022 [P] [US1] Probe tests in `src-tauri/src/remote_storage/probe_tests.rs`: WebDAV outcomes `folderMissing` (storage folder missing), `notWebDav` (405 on `PROPFIND` of the base URL, or a 3xx), the test folder `holzi-test/` removed when empty after the test **Note**: `folderMissing` comes from a `head` of the storage's folder before any write; `notWebDav` from the new `StorageError::Protocol`

### Implementation for User Story 1

- [x] T023 [US1] Add `quick-xml = "0.42"` (the version already in `Cargo.lock`) to `src-tauri/Cargo.toml`; check `cargo tree -i quick-xml` shows one 0.42 copy **Note**: holzi already depends on `quick-xml` 0.41 directly (spec 044 US6); the parser uses that version, no second copy
- [x] T024 [P] [US1] `src-tauri/src/remote_storage/webdav/multistatus.rs`: parse a `PROPFIND` answer with `quick_xml::Reader` (namespace-aware on `DAV:`) into entries `{ path, is_dir, size, last_modified, etag }`, decode and check `href` against the request path (research R2), cap the answer size at the listing limit
- [x] T025 [US1] `src-tauri/src/remote_storage/webdav/request.rs`: build URLs from `Location.endpoint` + folder + key (each part percent-encoded, folder URLs with `/`), send `PROPFIND`/`GET`/`HEAD`/`PUT`/`DELETE`/`MKCOL`/`MOVE`/`COPY` through `http::pinned_client` with Basic auth from `Credentials::Password`, map status codes (research R2), log status only, never URL or answer text
- [x] T026 [US1] `src-tauri/src/remote_storage/webdav/upload.rs`: upload sessions (research R7): `begin_upload` starts a streamed `PUT` of `.holzi-part-<UUID>` fed by a bounded channel in a spawned task; `upload_part` sends into the channel (a broken stream poisons the session → `Network` for every later call); `finish_upload` closes, awaits the answer, `MOVE`s onto the key; `abort_upload` drops the stream and deletes the temporary file; sessions in a `Mutex<HashMap<String, Session>>`, dropped after 30 minutes without a part; tests in `src-tauri/src/remote_storage/webdav/upload_tests.rs` (including, with `tokio::time::pause`, a part the server does not accept before the part's deadline → `TimedOut` and the session poisoned; FR-013) **Note**: `finish` returns the key of the temporary file with the outcome so the store deletes it on failure; deadline tests with paused time are left for PR D (T040)
- [x] T027 [US1] `src-tauri/src/remote_storage/webdav/mod.rs`: `WebDavStore` implementing `RemoteStore` per contracts/remote-store.md (`put`, `get`, `head`, `get_range`, `delete`, `copy` = `COPY`, `copies_on_provider` = true; in PR C `list` covers only what the probe needs: one level below a prefix such as `holzi-test/`; `list_dir`, `rename_tree`, `remove_tree` answer `Unsupported` until T043 in PR D); a semaphore of 4 requests per connection id (research R12); register it in `ProtocolStore` (`src-tauri/src/remote_storage/protocol.rs`) **Note**: `list` already walks all levels through the new `src-tauri/src/remote_storage/tree.rs` (planned for PR D, T042: part names, parents, the walk); `list_dir`, `rename_tree` and `remove_tree` are implemented too. Helpers in `src-tauri/src/remote_storage/webdav/ops.rs`
- [x] T028 [US1] Address checks in `src-tauri/src/remote_storage/address.rs`: WebDAV endpoints keep the existing `http`/`https` rules and are stored with a trailing `/`; tests in `src-tauri/src/remote_storage/address_tests.rs` (path kept, `/` added, `http` public refused) **Note**: no change needed: `address.rs` already keeps the path of an `http(s)` endpoint; the service adds the trailing `/`, the store checks it
- [x] T029 [US1] Probe in `src-tauri/src/remote_storage/probe.rs`: map WebDAV failures to `folderMissing` and `notWebDav` (research R10), remove an empty `holzi-test/` folder after the test on tree protocols
- [x] T030 [US1] Service in `src-tauri/src/remote_storage/service.rs`: `save_connection_in` and `save_storage` test WebDAV like S3 (re-test on changed endpoint, credentials or folder), `save_storage` returns `warning: "nestedFolder"` when another storage of the same connection has a folder inside or around this one (FR-003); new `create_folder` (with missing parents, only on an explicit call) followed by a test; tests in `src-tauri/src/remote_storage/service_tests.rs` (the fake store in tree mode) **Note**: a missing folder is made through `createFolder: true` on `storage_connection_save`/`storage_save` (a new connection has no id before it is saved); `StorageInvalid` names `folder` and `login` for WebDAV
- [x] T031 [US1] Command `storage_folder_create` (`{ storageId }` or `{ connectionId, folder }`) in `src-tauri/src/remote_storage/commands.rs`, registered in the invoke handler (`src-tauri/src/lib.rs`), answer `TestResult` **Note**: `storage_folder_create` takes `{ id }` of a saved storage only (contracts/tauri-commands.md updated)
- [x] T032 [P] [US1] Settings UI in `src/components/settings/storage/ConnectionForm.vue`: protocol choice (S3, WebDAV; SFTP added in PR F) only for a new connection, read-only afterwards; WebDAV presets in `PRESETS` per research R11 (Hetzner Storage Box `https://<benutzer>.your-storagebox.de/`, Nextcloud `https://<server>/remote.php/dav/files/<benutzer>/`, Koofr `https://app.koofr.net/dav/Koofr/`, kDrive `https://<kdrive-id>.connect.kdrive.infomaniak.com/`, Anderer Server `https://`) with the hint per preset; placeholders `<…>` marked as still to fill; region and addressing hidden for WebDAV; credentials fields username and password; reuse Group/Row/OptionRow (COSMIC/GNOME style), no save button beyond the existing "Testen und speichern" **Note**: the presets moved to `src/components/settings/storage/presets.ts` to keep the form under 500 lines
- [x] T033 [P] [US1] `src/components/settings/storage/StorageForm.vue`: label "Ordner" for WebDAV/SFTP (bucket field), empty allowed for the root; on outcome `folderMissing` show "Ordner fehlt" with the action "Ordner anlegen" calling `storage_folder_create`, then show the new test result; show the `nestedFolder` warning
- [x] T034 [P] [US1] `src/components/settings/storage/StorageListView.vue` and `RemovalView.vue`: show the protocol, the folder instead of the bucket, "Dateien beim Anbieter bleiben" for WebDAV; `src/composables/useStorage.ts` passes `protocol` through
- [x] T035 [P] [US1] i18n in `src/i18n/locales/de.json` and `en.json` under `settings.storage`: `protocols.{s3,webdav,sftp}`, `kinds.{hetznerStorageBox,nextcloud,koofr,kdrive}`, preset hints (research R11), `fields.{folder,username,password}`, `outcome.{folderMissing,notWebDav}`, `actions.createFolder`, `warnings.nestedFolder`, `outcome.tooManyAttempts` (Koofr 429); change `intro` and `addConnectionHint` that say "S3-kompatibel" to name all protocols **Note**: vue-i18n refuses `<…>` in messages, so the hints say the placeholders in words
- [x] T036 [US1] E2E: `scripts/e2e/lib/sftpgo.ts` starts `drakkan/sftpgo:v2.7.6` on free loopback ports with WebDAV on (`SFTPGO_WEBDAVD__BINDINGS__0__PORT`), a user from a JSON file written at run time into a temp dir (`SFTPGO_LOADDATA_FROM`), skipped without Docker like `rustfs.ts`, reachable on Android via `reachFromDevice`; new scenario `scripts/e2e/scenarios/storage-webdav.test.ts`: connect with the preset "Anderer Server", folder missing → create, test passes, rename storage, change password, remove (US1 scenarios 1–4, 6); extend `scripts/e2e/scenarios/storage-two-devices.test.ts` with a WebDAV storage created on device 1 and used on device 2 without input (US1 scenario 7, SC-007); list the scenario in `scripts/e2e/platform-exclusions.ts` only if a platform cannot run Docker **Note**: `rclone/rclone:1.71.1` with `serve webdav` instead of SFTPGo (`scripts/e2e/lib/webdav.ts`, research R14 updated); the scenario `storage-webdav` covers two devices itself (US1 scenario 7) instead of extending `storage-two-devices`; passed locally with the other storage scenarios
- [x] T037 [US1] Run `cargo fmt --check`, `pnpm lint:rust`, `cargo test -j 4 --lib -- remote_storage`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, the new and the existing storage E2E scenarios; commit, open PR C

**Checkpoint**: A WebDAV storage can be created, tested and removed in the settings; it syncs like S3 (US1 scenario 7 via the existing two-device scenario run against SFTPGo).

---

## Phase 4: User Story 2 - WebDAV-Speicher im Dateibrowser und für Agents (Priority: P1) (PR D)

**Goal**: A WebDAV storage works in the file browser and for agents exactly like an S3 storage.

**Independent Test**: All scenarios of spec 044 US5 against a local WebDAV server plus rename, move, copy and delete (quickstart §4, §5).

### Tests for User Story 2

- [ ] T038 [P] [US2] Tree tests in `src-tauri/src/remote_storage/tree_tests.rs`: `list` yields `ordner/` entries of size 0 for every folder (also empty ones) and files with their keys; `list_dir` yields subfolders as prefixes and files as objects; `.holzi-part-*` never appear; one older than 24 hours is deleted while listing and a failed delete is ignored; a walk reads at most 4 folders at once
- [ ] T039 [P] [US2] File browser tests in `src-tauri/src/files/storage_source_tests.rs` (fake store in tree mode): create folder = one `put` of `ordner/`; rename and delete of a folder = one `rename_tree` / `remove_tree`; rename of a file uses `rename_tree` too; S3 (flat mode) keeps the per-object loops and never calls `rename_tree`/`remove_tree`
- [ ] T040 [P] [US2] Stateful tests against the in-process WebDAV server (T041) in `src-tauri/src/remote_storage/webdav/server_tests.rs`: upload over `PART` in sessions, cancel mid-way → no file under the target name and an existing file unchanged (SC-003); `MOVE` of a folder with 100 files; range reads for a seek

### Implementation for User Story 2

- [ ] T041 [US2] Test support: add the in-process WebDAV server as a dev-dependency after checking it (candidate `dav-server` with its in-memory or local file system: version at least two weeks old, permissive license, `Depth: 1` answers, `MOVE`/`COPY` with `Overwrite`, `Range`); if it does not fit, a small `hyper` server over a temp dir in `src-tauri/src/remote_storage/webdav/test_server.rs`; note the choice in research.md R14
- [ ] T042 [US2] `src-tauri/src/remote_storage/tree.rs`: the shared tree logic for WebDAV and SFTP (research R7–R9): marker entries `ordner/`, hiding and cleaning `.holzi-part-*`, a breadth-first walk with a concurrency of 4 over a `read_dir`-like closure, the prefix filter for extension listings (start at the folder up to the last `/` of the prefix, keep keys starting with the prefix), and `case_clash(parent_listing, name)` that finds a name differing only in case (research R9, used by T044 and T053)
- [ ] T043 [US2] `WebDavStore` in `src-tauri/src/remote_storage/webdav/mod.rs`: full `list` and `list_dir` via `tree.rs` and `PROPFIND` `Depth: 1`; `put`/`delete` of a key ending in `/` = `MKCOL` / `DELETE` of an empty folder (check emptiness with `PROPFIND` first); `rename_tree` = `MOVE` with `Overwrite: T`; `remove_tree` = `DELETE` on the folder; `ObjectInfo.etag` from `getetag`
- [ ] T044 [US2] File source in `src-tauri/src/files/storage_source.rs`: when `access.location.is_tree()`, `rename` (file and folder) calls `rename_tree` and `delete` of a folder calls `remove_tree`, with progress reported as one step; `taken` (the conflict check before writing) also counts a name in the target folder that differs only in case as taken, via `tree::case_clash` (spec edge case, research R9); `stat` for a folder uses `head` on `ordner/` semantics of the tree store; leave the flat (S3) code paths unchanged
- [ ] T045 [US2] Errors: map `StorageError::QuotaExceeded` → `FilesErrorCode::NoSpace`, `HostKeyChanged`/`HostKeyUnknown` → a new `FilesErrorCode::HostKey` (doc: "A storage's server showed a host key other than the confirmed one (spec 050)") in `storage_error` (`src-tauri/src/files/storage_source.rs:34`) and `src-tauri/src/files/mod.rs`; frontend messages for both in `src/i18n/locales/{de,en}.json` under the files error keys with a link to the storage settings (US2 scenario 5)
- [ ] T046 [US2] Agents: confirm through `src-tauri/src/files/agent/` tests that search and read on a tree-mode storage go through the same `StorageFiles` paths and need the per-storage permission (044 FR-031a, FR-019); add one test in `src-tauri/src/files/agent/reach_tests.rs` (or the existing agent tests file) with a WebDAV location
- [ ] T047 [US2] E2E scenario `scripts/e2e/scenarios/files-storage-webdav.test.ts` against SFTPGo WebDAV: open the storage, change folders, play a video and seek, upload and cancel an upload (no file left), rename and delete a folder, download a file (US2 scenarios 1–5)
- [ ] T048 [US2] Run `cargo fmt --check`, `pnpm lint:rust`, `cargo test -j 4 --lib -- remote_storage files`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, the files E2E scenarios; commit, open PR D

**Checkpoint**: WebDAV storages are full sources in the file browser and for agents.

---

## Phase 5: User Story 3 - Erweiterungen nutzen einen WebDAV-Speicher (Priority: P1) (PR E)

**Goal**: Extensions get the same results on WebDAV as on S3, without knowing the protocol.

**Independent Test**: The 038 test catalog runs unchanged against an S3 and a WebDAV storage with identical results (SC-002).

### Tests for User Story 3

- [ ] T049 [P] [US3] Parity catalog in `src-tauri/src/extensions/remote_storage_parity_tests.rs` (registered in `src-tauri/src/extensions/remote_storage.rs`): every case of the 038 catalog (area, keys with `..`/leading `/`/empty parts/control characters, limits, read vs read-write, two extensions on one storage) as one test function run against the S3 test support and the in-process WebDAV server, asserting identical answers; plus `list` with a prefix ending mid-name (`a/b` finds `a/bc`), no folder entries and no `.holzi-part-*` in answers
- [ ] T050 [P] [US3] Key tests in `src-tauri/src/extensions/remote_storage_keys_tests.rs`: on tree protocols refuse a part longer than 255 bytes, a part `.`, `\`, `:*?"<>|`, a part starting with `.holzi-part-` (3001); S3 unchanged
- [ ] T051 [P] [US3] Case and empty-folder tests in `src-tauri/src/extensions/remote_storage_tests.rs`: writing `A.txt` where `a.txt` exists in the parent folder → 3001 on tree protocols; deleting the last file of `x/y/` removes `y/` and `x/` up to the area root and never above it

### Implementation for User Story 3

- [ ] T052 [US3] Key checks per protocol in `src-tauri/src/extensions/remote_storage_keys.rs` (`valid` gets the protocol; research R9); total length stays 1024 bytes with the prefix
- [ ] T053 [US3] Bridge in `src-tauri/src/extensions/remote_storage.rs`: `list` on tree protocols through the prefix filter of `tree.rs` (no folder entries, no temporary files); before `upload` of a new key on tree protocols read the parent folder (one level) and refuse a name differing only in case (3001); after `delete` remove empty parent folders up to the extension's area root (research R9)
- [ ] T054 [US3] List of storages in `src-tauri/src/extensions/remote_storage.rs` (`list_backends`, around line 206): `type` from the connection's protocol, `folder` instead of `bucket` for `webdav`/`sftp` (contracts/bridge.md); no address, host, port or host key
- [ ] T055 [US3] Error mapping for extensions in `src-tauri/src/extensions/remote_storage.rs`: `QuotaExceeded` → 7000, `HostKeyChanged`/`HostKeyUnknown` → 2002 `{kind: "network"}`, `Unsupported` → 2002 `network` with a log line (contracts/bridge.md "Fehler")

**Checkpoint**: Existing extensions work on WebDAV storages unchanged.

---

## Phase 6: User Story 6 (WebDAV part) - Eine Erweiterung stößt einen WebDAV-Speicher an (Priority: P2) (PR E)

**Goal**: An extension can propose a WebDAV storage like an S3 storage; credentials only in holzi.

**Independent Test**: The test extension proposes a WebDAV storage; confirm, enter credentials in the window over the app → storage created, permission granted; a proposal with credentials → 3001 without a dialog.

### Tests for User Story 6 (WebDAV)

- [ ] T056 [P] [US6] Tests in `src-tauri/src/extensions/remote_storage_manage_tests.rs`: `type: "webdav"` with `config: {url, folder}` → dialog; `sameProviderAs` pointing at an S3 storage → 3001; any of `username`, `password`, `privateKey`, `passphrase`, `hostKey`, `fingerprint` in `config` or `request` → 3001 before a dialog; permission `add` target is `host:port` of the URL (port from the scheme when absent); `update_backend` with `config: {folder}` re-tests

### Implementation for User Story 6 (WebDAV)

- [ ] T057 [US6] Proposals in `src-tauri/src/extensions/remote_storage_manage.rs`: accept `type: "webdav"` (`url`, `folder`), extend the refused field list (`remote_storage_manage.rs:32`) by `username`, `password`, `privateKey`, `passphrase`, `hostKey`, `fingerprint`, `sameProviderAs` only with the same protocol; `update_backend` accepts `folder` for tree protocols
- [ ] T058 [US6] Endpoint check in `src-tauri/src/extensions/remote_storage_endpoint.rs`: `Proposed::check` for a WebDAV URL (`may_add` with host and port of the URL), `Proposed::fits` compares protocol and endpoint (no region for WebDAV)
- [ ] T059 [P] [US6] Dialogs: `src/components/extensions/StorageDialog.vue` shows a WebDAV proposal (provider, address, folder, local/unencrypted marks) and the "Ordner fehlt → anlegen" step; `src/components/StorageCredentialsModal.vue` asks for username and password for WebDAV; `src-tauri/src/extensions/remote_storage_dialog.rs` `StorageAnswer::Confirm` carries the tagged credentials
- [ ] T060 [US6] vault-sdk PR (separate repository, via the haexmas fork, no agent attribution): `src/api/remoteStorage.ts` types for `list_backends` (`type: 's3' | 'webdav' | 'sftp'`, `folder?`) and `add_backend` proposals for `webdav` (`url`, `folder`) and `sftp` (`host`, `port?`, `folder`), no credential fields; pin the released version in holzi only if holzi's own code uses the types
- [ ] T061 [US6] E2E: extend `scripts/e2e/scenarios/extension-storage.test.ts` (or a new `extension-storage-webdav.test.ts`) so the test extension uploads, lists, downloads and deletes on a WebDAV storage and proposes one (US3, US6)
- [ ] T062 Run T093 for WebDAV, then `cargo fmt --check`, `pnpm lint:rust`, `cargo test -j 4 --lib -- extensions::remote_storage remote_storage`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, the extension storage E2E scenarios; commit, open PR E

**Checkpoint**: Stage 1 (WebDAV) complete: US1, US2, US3 and the WebDAV part of US6.

---

## Phase 7: User Story 4 - SFTP-Speicher verbinden (Priority: P2) (PR F)

**Goal**: A user connects an SFTP storage with password or key, confirms the host key on first use; a changed host key stops every call before credentials are sent.

**Independent Test**: Against an in-process SFTP server: connection with password and with key, confirm the host key, test, remove; swap the server's host key → refused with zero login attempts (quickstart §3).

### Tests for User Story 4

- [ ] T063 [P] [US4] SFTP test server in `src-tauri/src/remote_storage/sftp/test_server.rs` (test-only, registered under `#[cfg(test)]`): `russh::server::run_stream` + `russh_sftp::server::run` over a temp dir, configurable host key, password and public key auth, a counter of authentication attempts, optional `posix-rename@openssh.com`
- [ ] T064 [P] [US4] Connect tests in `src-tauri/src/remote_storage/sftp/connect_tests.rs`: unknown host key → `HostKeyUnknown` with the shown key and `SHA256:` fingerprint and **0** authentication attempts on the server; changed host key → `HostKeyChanged`, 0 attempts (SC-004); confirmed key → password login and key login (Ed25519, RSA with `best_supported_rsa_hash`, OpenSSH key with passphrase) succeed; wrong password → `AccessDenied`; connect beyond the deadline → `TimedOut`; a refused address never opens a TCP connection
- [ ] T065 [P] [US4] Key tests in `src-tauri/src/remote_storage/sftp/keys_tests.rs`: generated Ed25519 key round-trips through `decode_secret_key`; public key line starts with `ssh-ed25519 ` and ends with ` holzi`; PKCS#8 and PPK samples decode; a wrong passphrase is `invalid credentials`
- [ ] T066 [P] [US4] Store tests in `src-tauri/src/remote_storage/sftp/mod_tests.rs` against the test server: `put` via temporary file and rename (with and without `posix-rename@openssh.com`), `get`, `head`, `get_range` at an offset, `delete`, upload session with a retried part written at the same offset, abort removes the temporary file, missing storage folder → `NotFound` without creating it
- [ ] T067 [P] [US4] Pool tests in `src-tauri/src/remote_storage/sftp/pool_tests.rs` with `tokio::time::pause`: two calls reuse one SSH session; at most two sessions per connection; idle sessions close after 60 s; new credentials or a new host key drop the pooled sessions

### Implementation for User Story 4

- [ ] T068 [US4] Add `russh = { version = "0.63.3", default-features = false, features = ["ring", "rsa", "flate2"] }` and `russh-sftp = "3.0.0"` to `src-tauri/Cargo.toml`; check with `cargo tree -i aws-lc-rs -e features` that russh adds no path to `aws-lc-rs` (note the pre-existing paths through quinn/rustls in the PR, research R3), and that the Android CI clippy run (spec 043) stays green
- [ ] T069 [US4] Address checks in `src-tauri/src/remote_storage/address.rs`: accept `sftp://host[:port]` (no path, user, query, fragment; port default 22), scope and `pin` as for http, never "insecure"; tests in `src-tauri/src/remote_storage/address_tests.rs`
- [ ] T070 [US4] `src-tauri/src/remote_storage/sftp/connect.rs`: resolve and pin via `address::pin`, `TcpStream::connect` to one checked address, `russh::client::connect_stream` inside `tokio::time::timeout`, a `client::Handler` whose `check_server_key` compares with `Location.host_key` and records the shown key, password or public-key authentication from `Credentials`, `Config` with `inactivity_timeout` and `keepalive_interval`, then the `sftp` subsystem into `SftpSession`; map russh and SFTP errors to `StorageError` (log codes only)
- [ ] T071 [US4] `src-tauri/src/remote_storage/sftp/keys.rs`: generate (Ed25519, comment `holzi`, no passphrase) into a store of pending keys (`keyId` → `Zeroizing` private key, dropped after 30 minutes, on vault lock via the vault gate token, and after saving; FR-008), decode user keys via `russh::keys::decode_secret_key`, public key line and `SHA256:` fingerprint of a host key line
- [ ] T072 [US4] `src-tauri/src/remote_storage/sftp/pool.rs`: up to two SSH sessions per (connection id, host key, username), idle close after 60 s, invalidation on credential or host key change (research R3, R12)
- [ ] T073 [US4] `src-tauri/src/remote_storage/sftp/rename.rs`: a second channel as `RawSftpSession`, read the extensions from `init()`, use `posix-rename@openssh.com` when offered, otherwise remove the target then rename (research R3)
- [ ] T074 [US4] `src-tauri/src/remote_storage/sftp/upload.rs`: upload sessions with a temporary file, parts written at their offsets (offset recorded per part number), `fsync@openssh.com` when offered, close and rename; abort deletes; after a failed write ask `statvfs@openssh.com` (when offered) and answer `QuotaExceeded` when less space is free than the rest of the file, else `Network` (research R10, spec edge case); sessions dropped after 30 minutes without a part
- [ ] T075 [US4] `src-tauri/src/remote_storage/sftp/mod.rs`: `SftpStore` implementing `RemoteStore` per contracts/remote-store.md (`copies_on_provider` = false; `list`/`list_dir` via `tree.rs` and `read_dir`; `etag` = `"<size>-<mtime>"`; `rename_tree` = rename; `remove_tree` bottom-up), register it in `ProtocolStore`
- [ ] T076 [US4] Service in `src-tauri/src/remote_storage/service.rs`: an SFTP connection without a confirmed host key ends the test with `hostKeyUnknown` and the shown key in `TestResult.host_key` and saves nothing; saving with `ConnectionInput.host_key` sets `host_key` and `host_key_confirmed_at` and accepts only that key; re-test on a changed port; remember the last shown host key per connection in the process on `hostKeyChanged`; `storage_host_key_accept` only when that remembered key is exactly the given one (research R5); credentials `{ kind: 'generatedKey', keyId }` take the pending key from T071 and store it as `privateKey`, an unknown or expired `keyId` is `invalid credentials`; tests in `src-tauri/src/remote_storage/service_tests.rs`
- [ ] T077 [US4] Commands `storage_ssh_key_generate` (answer `{ keyId, publicKey }`, never the private key) and `storage_host_key_accept` in `src-tauri/src/remote_storage/commands.rs` per contracts/tauri-commands.md, add `generatedKey` to `CredentialsInput` in `src-tauri/src/remote_storage/model.rs` (ts-rs bindings regenerated), registered in `src-tauri/src/lib.rs`
- [ ] T078 [P] [US4] Settings UI: SFTP in `src/components/settings/storage/ConnectionForm.vue` (presets Hetzner Storage Box `sftp://<benutzer>.your-storagebox.de:23` with the hint on Hetzner's published fingerprints, Anderer Server `sftp://` port 22; host, port, username; auth choice password or key; key paste, read from a file, or "Schlüssel erzeugen" showing the public key with a copy button and sending `{ kind: 'generatedKey', keyId }` on save; the form never holds a generated private key); new `src/components/settings/storage/HostKeyConfirm.vue` (fingerprint to compare, confirm/cancel; for a changed key old and new fingerprint and "Neuen Schlüssel annehmen"); warning in `StorageListView.vue` for `hostKeyChanged`
- [ ] T079 [P] [US4] i18n in `src/i18n/locales/{de,en}.json`: `settings.storage.fields.{host,port,authMethod,privateKey,passphrase}`, `actions.{generateKey,copyPublicKey,acceptHostKey}`, `hostKey.{unknown,changed,compareHint,hetznerHint}`, `outcome.{hostKeyUnknown,hostKeyChanged}`
- [ ] T080 [US4] E2E: extend `scripts/e2e/lib/sftpgo.ts` with SFTP (port, host key generated at run time and mounted, user with a public key); new scenario `scripts/e2e/scenarios/storage-sftp.test.ts`: generate a key in holzi, register its public key with the SFTPGo user, confirm the fingerprint, test passes; restart SFTPGo with another host key → warning, accept, test passes (US4 scenarios 1–5); two devices see the connection with the confirmed key (US4 scenario 6)
- [ ] T081 [US4] Run `cargo fmt --check`, `pnpm lint:rust`, `cargo test -j 4 --lib -- remote_storage`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, the storage E2E scenarios; commit, open PR F

**Checkpoint**: SFTP connections can be created, tested and secured with host keys.

---

## Phase 8: User Story 5 - SFTP-Speicher im Dateibrowser, für Agents und Erweiterungen (Priority: P2) (PR G)

**Goal**: Everything US2 and US3 define for WebDAV works for SFTP.

**Independent Test**: The tests of US2 and US3 run unchanged against an SFTP storage with the same results as S3 and WebDAV.

- [ ] T082 [P] [US5] Add the SFTP test server as a third target to the parity catalog `src-tauri/src/extensions/remote_storage_parity_tests.rs` and to the tree tests in `src-tauri/src/files/storage_source_tests.rs`
- [ ] T083 [P] [US5] Transfer test in `src-tauri/src/files/transfer/remote_tests.rs`: a copy inside one SFTP storage streams through holzi (no `copy` call), shows progress and leaves no file under the target name after a cancel (US5 scenario 2)
- [ ] T084 [US5] Fix whatever the parity runs reveal in `src-tauri/src/remote_storage/sftp/` and `src-tauri/src/remote_storage/tree.rs` (no protocol checks in callers; FR-018)
- [ ] T085 [US5] E2E scenarios `scripts/e2e/scenarios/files-storage-sftp.test.ts` (browse, video seek, upload and cancel, rename and delete a folder, copy within the storage) and the SFTP case in the extension storage scenario

## Phase 9: User Story 6 (SFTP part) - Eine Erweiterung stößt einen SFTP-Speicher an (Priority: P2) (PR G)

- [ ] T086 [P] [US6] Tests in `src-tauri/src/extensions/remote_storage_manage_tests.rs`: `type: "sftp"` with `config: {host, port?, folder}` → dialog; permission `add` target `host:port` (22 when absent); the credentials window shows the host key fingerprint after the test and only continues after confirmation; declining → 1002; a proposal with `hostKey` or `fingerprint` → 3001 before a dialog
- [ ] T087 [US6] Proposals for `type: "sftp"` in `src-tauri/src/extensions/remote_storage_manage.rs` and `remote_storage_endpoint.rs` (`Proposed::check` for `host`/`port`), `StorageAnswer::Confirm.host_key` in `src-tauri/src/extensions/remote_storage_dialog.rs`, `StorageTrial.failed` with `hostKeyUnknown` and `hostKey`
- [ ] T088 [P] [US6] `src/components/StorageCredentialsModal.vue`: SFTP credentials (password or key, generate key via `keyId` as in T078) and the fingerprint confirmation step (reuse `HostKeyConfirm.vue`); `src/components/extensions/StorageDialog.vue`: SFTP proposal (host, port, folder)
- [ ] T089 Run `cargo fmt --check`, `pnpm lint:rust`, `cargo test -j 4 --lib -- remote_storage extensions::remote_storage files`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, all storage E2E scenarios; commit, open PR G

**Checkpoint**: Stage 2 (SFTP) complete: US4, US5 and the SFTP part of US6.

---

## Phase 10: User Story 7 - Verschlüsselte Ordner in WebDAV- und SFTP-Speichern (Priority: P3) (PR H)

**Precondition**: Spec 048 is built and in use (phase rule of the constitution).

- [ ] T090 [US7] Run the acceptance tests of spec 048 (`src-tauri/src/files/encrypted/` tests and its E2E scenarios) against the in-process WebDAV and SFTP servers and SFTPGo; fix gaps in `src-tauri/src/remote_storage/` (etag stand-ins, modification time for 048 FR-016, range reads), never by protocol checks in `files/encrypted/`
- [ ] T091 [US7] Conflict test: two devices write the same encrypted folder on a WebDAV storage → conflict copy as on S3 (048 FR-021), in the 048 test module
- [ ] T092 [US7] Commit, open PR H

---

## Phase 11: Polish & Cross-Cutting Concerns

- [ ] T093 [P] Leak check (SC-005), first in PR E for S3 and WebDAV, extended to SFTP in PR G: one test in `src-tauri/src/extensions/remote_storage_parity_tests.rs` that greps every bridge answer, error message and captured log line of the parity catalog for the test password, private key, passphrase, host and port and finds none
- [ ] T094 [P] Manual checks per quickstart §6 (Hetzner Storage Box WebDAV and SFTP, Nextcloud, second device, Android; iOS once a build exists); record results in the PR of the last stage (SC-001, SC-006, SC-007, SC-008)
- [ ] T095 [P] Update `specs/050-webdav-sftp-storage/research.md` with what the manual checks found (Koofr/Hetzner `Depth` and range behaviour, Hetzner `posix-rename`) and the presets if a URL pattern turned out wrong
- [ ] T096 Mark `plans/README.md` row 050 as built when PR G is merged; US7 stays open until PR H

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none.
- **Foundational (Phase 2, PR B)**: after PR A; blocks every story.
- **US1 (PR C)**: after PR B.
- **US2 (PR D)**: after US1 (uses `WebDavStore` and the settings to create storages).
- **US3 + US6 WebDAV (PR E)**: after US2 (uses `tree.rs` and the in-process WebDAV server from PR D).
- **US4 (PR F)**: after PR B; by operator decision after PR E.
- **US5 + US6 SFTP (PR G)**: after US4 and PR E (parity catalog, proposal code).
- **US7 (PR H)**: after PR G and spec 048 in use.
- **Polish**: T093 with PR E (WebDAV) and PR G (SFTP); T094–T096 at the end of each stage.

### Within Each User Story

- Tests first; they fail before the implementation.
- Parser/request/store before service, service before commands, commands before UI, UI before E2E.

### Parallel Opportunities

- PR B: T012 (http.rs) next to T008–T011.
- US1: T019–T022 together; T024 next to T025; T032–T035 (four UI files) together after T031.
- US2: T038–T040 together.
- US3: T049–T051 together.
- US4: T063–T067 together; T078 and T079 together after T077.
- US5: T082 and T083 together.

## Parallel Example: User Story 1

```text
Task: "Parser tests in src-tauri/src/remote_storage/webdav/multistatus_tests.rs"
Task: "Request tests in src-tauri/src/remote_storage/webdav/request_tests.rs"
Task: "Store tests in src-tauri/src/remote_storage/webdav/mod_tests.rs"
Task: "Probe tests in src-tauri/src/remote_storage/probe_tests.rs"
```

## Implementation Strategy

### MVP First (User Story 1)

1. PR A (docs) and PR B (refactor, no behaviour change).
2. PR C: WebDAV in the settings. **Stop and validate** against SFTPGo and by hand against the Hetzner Storage Box (WebDAV).

### Incremental Delivery

1. PR D: WebDAV in the file browser → usable for the operator's own files.
2. PR E: WebDAV for extensions → stage 1 done.
3. PR F, PR G: SFTP → stage 2 done.
4. PR H after spec 048.

## Notes

- Every new Rust file gets a `*_tests.rs` next to it; test servers live in test-only files.
- No commit trailers or PR lines that name an agent (constitution).
- Keep each source file under 500 lines; split `webdav/` and `sftp/` further if one grows.
- "Every call site" (operator memory): before changing a trait method or type, grep all users; the lists in T009, T010, T013 and T014 come from a grep on 2026-10-11 and must be re-checked on the PR's base.
