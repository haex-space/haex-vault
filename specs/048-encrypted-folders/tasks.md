---
description: 'Task list for spec 048-encrypted-folders'
---

# Tasks: Verschlüsselte Ordner in Speichern

**Input**: Design documents from `/specs/048-encrypted-folders/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/format.md](./contracts/format.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md),
[contracts/agent-actions.md](./contracts/agent-actions.md), [contracts/bridge.md](./contracts/bridge.md),
[quickstart.md](./quickstart.md), [ADR 0013](../../docs/adr/0013-encrypted-folder-format.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic, and SC-001,
SC-002, SC-008 and SC-010 need case corpora and test vectors. Rust tests live in `*_tests.rs` next to
the file (`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. No network services in
`cargo test`: `FakeStore` (`src-tauri/src/remote_storage/test_support.rs`) and
`src-tauri/src/files/test_support.rs` for storages. E2E scenarios run against RustFS
(`scripts/e2e/lib/rustfs.ts`, scenario option `needs.container`).

**graphify first** (`.spaex/constitution.md`): before authoring any new named function, type, module
or command, run a bounded `graphify query "<intent>" --budget 1200` and extend an existing candidate
where one fits; name the evaluated candidates in the PR.

**haex-vault references** use repository `https://github.com/haex-space/haex-vault` at revision
`8dce379d94e18fcd42c3b73686a06f984ca3f574` (constitution IV). holzi takes the model, not the code or
the format (research R3).

**Organization**: Grouped by user story in priority order (P1: US1, US2, US3; P2: US4, US5, US6).
US6 is split into two phases, agents and extensions, because they ship in separate PRs.

**Shipping note** (plan.md, Lieferungen): PR A = Phase 1 (docs); PR B0 and PR B = Phase 2; PR C = US1 +
US2; PR D = US4; PR E = US5; PR F = US3; PR G = US6 agents (after 044 T074, T076, T077, T082); PR H =
US6 extensions (with a vault-sdk PR). Each PR is reviewed and merged before the next one starts on top
of `main`, in its own worktree under `.worktrees/`.

**Every commit**: `cargo test` rewrites `src/types/bindings/InstanceInfo.ts` with trailing whitespace;
revert that file unless the PR changes it on purpose. New or changed bindings are committed without
trailing whitespace (`sed -i -E 's/[[:space:]]+$//' src/types/bindings/*.ts`). No agent attribution in
commits or PR bodies.

**Every new Tauri command**: register it in `src-tauri/src/lib.rs` (`generate_handler!`); not on the
list in `src-tauri/src/vault_gate/invoke.rs`.

**Every log line** in code touched by this spec: no name, path, object key or content from an
encrypted folder (FR-027). Use `Redacted` (T014) for paths that can come from one.

**Builds**: run only the relevant test targets with `-j 4` (`cargo test --lib files::encrypted -j 4`);
cargo needs `nix develop` in the worktree.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US6)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Write this `specs/048-encrypted-folders/tasks.md`; set the 048 row in `plans/README.md` to "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-10; Lieferungen PR A–H"
- [ ] T002 Run `pnpm format:check` in `nix develop`, commit as `docs(048): tasks for encrypted folders`, push `docs/048-tasks` with the haexmas token, open the PR against `main`

---

## Phase 2: Foundational (PR B0, PR B, start of PR C)

**Purpose**: the refactor, the pure format and the pieces every story needs. No user story starts
before this phase is merged.

### PR B0: split `browser_commands.rs` (no behaviour change)

- [x] T003 Split `src-tauri/src/files/browser_commands.rs` (817 lines) into `src-tauri/src/files/commands/` with `mod.rs` (shared helpers `storage_of`, `storage_files`, `side_of`, `device_path`, `blocking`), `navigate.rs` (sources, list, stat, watch), `view.rs` (open, read_text, thumbnail, release, open_system, `download_to_open`), `change.rs` (create_folder, rename, transfer start/answer/cancel/retry, import_dropped) and `search.rs`; each file under 500 lines; keep every command name and signature; move the tests that exist for these helpers along; update `src-tauri/src/files/mod.rs` and the handler list in `src-tauri/src/lib.rs` **Note**: `files/commands.rs` belongs to spec 043 (picked files), so the split became the directory `src-tauri/src/files/browser_commands/` (`mod.rs`, `navigate.rs`, `view.rs`, `change.rs`, `search.rs`) instead of `files/commands/`; `files/agent/` keeps importing `storage_files`, `KnownPlace` and `StorageSource` from `files::browser_commands`; `lib.rs` names the sub modules. Later tasks that say `files/commands/view.rs` or `files/commands/change.rs` mean `files/browser_commands/view.rs` and `…/change.rs`
- [x] T004 Run `cargo test --lib files -j 4`, `pnpm typecheck`, and the E2E scenarios `files-basic`, `files-manage`, `files-storage` to show no change; open PR B0 **Note**: `cargo test --lib files` (231 passed), `pnpm typecheck`, and the three named E2E scenarios passed locally/through CI; the split is open as PR B0 (#368).

### PR B: the format (pure, no I/O)

- [ ] T005 Add `data-encoding` as a direct dependency in `src-tauri/Cargo.toml` (the version already in `Cargo.lock`, features `std`) for Base32 (RFC 4648, no padding, lowercase); note in the PR that it adds no new crate to the build
- [ ] T006 [P] Write the test vectors first: `src-tauri/tests/fixtures/encrypted/v1-vectors.json` with fixed `VCK_g`, `FK`, `DEK`, ids and nonces covering every case of contracts/format.md "Testvektoren" 1–5 (keys; a head with one recipient and name "Unterlagen"; content objects with 0, 1, 65 536 and 65 537 bytes; sidecars for a file, its copy with new `oid` and same `cid`, and a folder; all negative cases including "extended by an empty block after a full last one"); generate them once with a small throwaway program outside the repo or by hand from the format, never from the implementation under test, and record how in a `_comment` field
- [ ] T007 [P] `src-tauri/src/files/encrypted/format/keys.rs` + `keys_tests.rs`: `kek(vck: &[u8;32]) -> Zeroizing<[u8;32]>` (`kdf(VCK_g, "holzi/encrypted-folder/kek/v1")`, salt the 5 bytes `holzi`), `FolderKeys { meta, wrap }` from `FK` (`holzi/encrypted-folder/meta/v1`, `…/wrap/v1`), fresh `FK`/`DEK`/ids from `sync::keys::random_bytes`; tests against the vectors
- [ ] T008 [P] `src-tauri/src/files/encrypted/format/head.rs` + `head_tests.rs`: build and parse the head `MAGIC | VERSION | 0x00 | fid | n | recipients | name_nonce | name_ct` with length-prefixed recipients (`art(1) | len(2) | daten(len)`, art `0x00` has `len = 88`, other arts skipped by `len`); wrap `FK` with `aad = MAGIC|VERSION|0x00|fid|art|key_id`, name with `aad = MAGIC|VERSION|0x00|fid|"name"`; outcome `Readable { fk, name, key_id } | OtherVault | NewerFormat | Damaged`; `fid` must equal the prefix's Base32; rename keeps recipients and writes a new `name_nonce`; tests: vectors, unknown art skipped, art 0 with wrong `len` → damaged, `VERSION` 2 → newer format, wrong `fid` → damaged
- [ ] T009 [P] `src-tauri/src/files/encrypted/format/content.rs` + `content_tests.rs`: 38-byte header `MAGIC | VERSION | 0x01 | cid | nonce_prefix`; block `i` = `AEAD(DEK, nonce_prefix | u64le(i), block, aad = header | last_i)`; `CHUNK` = 65 536; a streaming sealer that holds back one block until it knows whether another follows (a file of a multiple of `CHUNK` ends with a full block with `last = 0x01`, only an empty file has an empty block); each block is sealed exactly once (no API that re-seals block `i`); an opener for one block and for a range `[a, b)` (blocks `⌊a/CHUNK⌋..⌊(b−1)/CHUNK⌋`, block `i` at `38 + i × (CHUNK + 16)`); `ciphertext_len(size) = 38 + 16 × max(1, ⌈size/CHUNK⌉) + size`; checks: `cid` = expected, object length = expected from size, `last` only on the final block; tests: vectors including all negative content cases, and a property test that random sizes round-trip and any truncation, extension or block swap is refused
- [ ] T010 [P] `src-tauri/src/files/encrypted/format/sidecar.rs` + `sidecar_tests.rs`: `MAGIC | VERSION | 0x02 | sid | nonce | AEAD(K_meta, nonce, json, aad = MAGIC|VERSION|0x02|fid|sid)`; `Sidecar` struct serialised without whitespace in the field order of contracts/format.md (`v, entry, parent, name, kind, revision, base, written, size, modified, type, sha256, content, cid, dek`; file-only fields absent for folders); `dek` = `nonce(24) | AEAD(K_wrap, nonce, DEK, aad = fid | entry | cid)`, Base64; `sid` in the object must equal the `sid` from the object name; names checked with `files::local::edit::check_name`; tests: vectors, sidecar from another folder refused, copied under another `sid` refused, `dek` rewrapped for a new `entry` opens only there
- [ ] T011 [P] `src-tauri/src/files/encrypted/format/mod.rs`: constants (`MAGIC`, `VERSION`, `CHUNK`, type bytes), Base32 helpers for 16-byte ids, the `.hxef/` name rule (exactly 26 lowercase Base32 chars + `.hxef`) as `is_folder_prefix(name)`, and `FormatError`; no I/O and no dependency on `files` outside `format` (FR-041)
- [ ] T012 Move contracts/format.md to `docs/formats/encrypted-folder-v1.md` (the spec keeps a link) and link it from `docs/adr/0013-encrypted-folder-format.md`
- [ ] T013 Run the vectors in the platform probe: extend `src-tauri/src/platform_probe.rs` with a step `encrypted-vectors` that runs the same checks as T007–T010 against `v1-vectors.json` (embedded with `include_str!`), so Windows, macOS and Android check them in CI (SC-010); open PR B

### Start of PR C: shared pieces

- [ ] T014 [P] `src-tauri/src/files/encrypted/redacted.rs` + `redacted_tests.rs`: `Redacted<'a>(&'a str)` whose `Display` and `Debug` print `<verschlüsselt>`; replace path output in the log lines that can see a storage path: `src-tauri/src/files/thumbnails.rs:107`, the open_system warning (now in `files/commands/view.rs`), and any new log in `files/encrypted/`; grep `log::` under `src-tauri/src/files/` and list the remaining ones in the PR with the reason they cannot see an encrypted path
- [ ] T015 [P] Migration `src-tauri/src/identity/migrations_encrypted_folders.rs`, registered in `holzi_migration_source()` in `src-tauri/src/identity/migrations.rs` with the next free number: tables `encrypted_folder_heads_no_sync` (`storage_id` TEXT NOT NULL, `object_key` TEXT NOT NULL, `etag` TEXT NOT NULL, `folder_id` TEXT NOT NULL, `name` TEXT "NULL, wenn nicht `readable`", `state` TEXT NOT NULL "`readable`, `otherVault`, `newerFormat`, `damaged`", `key_id` BLOB, `open_ack` INTEGER NOT NULL, `loaded_at` INTEGER NOT NULL; PRIMARY KEY (`storage_id`, `object_key`)) and `encrypted_folder_entries_no_sync` (columns as in data-model.md including `object_id` "`oid` (`content`)" and `content_id` "`cid`", `wrapped_dek` BLOB "bleibt verpackt"; PRIMARY KEY (`storage_id`, `folder_id`, `sidecar`); INDEX (`storage_id`, `folder_id`, `parent_id`)); `_no_sync`, so no trigger version change; test in the migration tests that both tables exist after upgrade
- [ ] T016 `src-tauri/src/files/encrypted/cache.rs` + `cache_tests.rs`: read and write both tables; `open_ack` survives a reload with a new ETag or name and moves with the folder when its `object_key` changes for the same `folder_id` (data-model.md); `remote_storage::store::remove_storage` (`src-tauri/src/remote_storage/store.rs`) deletes the rows of the storage in the same transaction (test in `store_tests.rs`)
- [ ] T017 `src-tauri/src/files/encrypted/vault_keys.rs` + `vault_keys_tests.rs`: `current()` from `sync::content_keys::current_key` and `by_key_id(key_id)` over all generations held in `vault_content_keys_no_sync` (as `sync::envelopes::held_keys`, no limit; not `listening_keys`, research R1); no key → `FilesErrorCode::NoKey`; add a comment at the place that would delete old generations (or in `content_keys.rs` next to the table) that encrypted folders still need them
- [ ] T018 Reserve the `.hxef` name form for ordinary entries: `files::local::edit::check_name` callers for storages (`files/storage_source.rs` create_folder/rename and `transfer/remote_plan.rs` targets) refuse names for which `format::is_folder_prefix` holds with `invalidName`; test in `storage_source_tests.rs`

**Checkpoint**: format with vectors merged (PR B); shared pieces ready for US1.

---

## Phase 3: User Story 1 - Verschlüsselten Ordner anlegen und nutzen (Priority: P1) 🎯 MVP

**Goal**: create an encrypted folder in a storage, put files and folders into it, browse it and open
text, PDF and images.

**Independent Test**: quickstart §2 (`pnpm test:e2e files-encrypted`, part 1).

### Tests for User Story 1

- [ ] T019 [P] [US1] `src-tauri/src/files/encrypted/tree_tests.rs` first: view from sidecars (newest revision per `entry` = the one no other names as `base`; two with the same `base` → both, the one with smaller `written` named "Name (Konflikt JJJJ-MM-TT hh-mm)"; same name of different entries in one parent → same naming; entries whose parent is missing → virtual folder "Wiederhergestellt" at the root; empty folders stay; path lookup `/a/b/c` → entry)
- [ ] T020 [P] [US1] `src-tauri/src/files/encrypted/load_tests.rs` first with `FakeStore`: one `list` over `<P>m/` (paged by 1 000), sidecars fetched with at most 32 at a time, only those whose ETag is new or missing in the cache, vanished ones dropped from the cache; heads of a folder listing (`*.hxef/` prefixes) loaded in parallel and cached by ETag
- [ ] T021 [P] [US1] `src-tauri/src/files/encrypted/ops_tests.rs` first (create part): create writes exactly one object `<E><base32(fid)>.hxef/h` with one recipient for the current generation; nothing else in the bucket; refused inside an encrypted folder (`inEncrypted`), inside an extension area `holzi-ext/` or `holzi-ext-dev/` (`inExtensionArea`), on a name clash with a decrypted folder name or an ordinary entry in the same parent (`exists`), without a content key (`noKey`)
- [ ] T022 [P] [US1] `src-tauri/src/files/encrypted/transfer_tests.rs` first (upload/download part): device → encrypted folder writes content object, then sidecar, nothing visible before the sidecar; storage → device decrypts into the part file and checks SHA-256 at the end; a cancelled upload leaves no sidecar; a multipart upload of a file > 8 MiB retries a failed part by resending the same ciphertext bytes (never re-sealing), else aborts and starts a new content object with a new `DEK`, `cid` and `nonce_prefix`

### Implementation for User Story 1

- [ ] T023 [US1] `src-tauri/src/files/encrypted/tree.rs`: the rules of T019; entries as `files::Entry` with the new fields `encrypted: { role: 'inside', state }` and `conflict` (contracts/tauri-commands.md); `Entry` in `src-tauri/src/files/mod.rs` gets `encrypted` and `conflict` with `ts-rs` export
- [ ] T024 [US1] `src-tauri/src/files/encrypted/load.rs`: the loading of T020 on `RemoteStore` (`list`, `get`) with a `tokio::sync::Semaphore` of 32; `src-tauri/src/files/encrypted/mod.rs`: `EncryptedFolders` in `FilesState` (`src-tauri/src/files/state.rs`) holding opened folders (`FK`, `FolderKeys`, tree) in `Zeroizing` until the vault gate token is cancelled (`vault_gate` `token()`)
- [ ] T025 [US1] `src-tauri/src/files/encrypted/ops.rs`: `create(parent, name)` per T021; `files_create_encrypted_folder` in `src-tauri/src/files/commands/change.rs`; `files_encrypted_notice` / `files_encrypted_notice_ack` on the device preference `files.encryptedNotice`; `files_encrypted_refresh`
- [ ] T026 [US1] Switch in `StorageFiles` (`src-tauri/src/files/storage_source.rs`): `list` shows `*.hxef/` prefixes as folders with `encrypted: { role: 'folder', state }` and the decrypted name (or the neutral label for `otherVault`, with the head's modification time); paths through an encrypted folder go to `files/encrypted` for `list`, `stat`, `read_start` (text viewer reads its window through T029's block reader), `create_folder` (an entry of kind `folder`, no marker object); keep the file under 500 lines by putting the routing in `files/encrypted/route.rs`
- [ ] T027 [US1] `src-tauri/src/files/encrypted/transfer.rs`: `Side::Encrypted` in `src-tauri/src/files/transfer/remote_plan.rs` (`side_of`) and the read/write steps in `src-tauri/src/files/transfer/remote.rs` (`upload`, `fetch`, `fill`, `place_of`): encrypt on the way in, decrypt on the way out, conflicts by decrypted names with the 044 hooks, retries per T022
- [ ] T028 [US1] Thumbnails without disk: a bytes-based `render` in `src-tauri/src/files/thumbnails.rs` and an LRU of 64 MB in `FilesState` for entries inside encrypted folders; `files_thumbnail` (in `files/commands/view.rs`) answers those from memory and never touches `files-thumbnails` or `.download-*` files; test in `thumbnails_tests.rs` that no file is written for an encrypted source
- [ ] T029 [US1] Block reader in `src-tauri/src/files/encrypted/source.rs`: read a plaintext range of a file through one `get_range` over the covering blocks (windows up to 8 MiB) and the opener of T009; used by `read_start` (text viewer) and `files_open` for `image` and `pdf` until T039 adds media
- [ ] T030 [P] [US1] Window: `src/components/files/EntryIcon.vue` lock icon and state badges (`otherVault`, `newerFormat`, `damaged`); "Neuer verschlüsselter Ordner" in `src/components/files/Menu.vue` and `src/lib/files/menus.ts` (not offered inside an encrypted folder or an extension area) using `NameDialog.vue`; the one-time notice from FR-002 before the first create; i18n in `src/i18n/locales/{de,en}.json`; logic without DOM in `src/lib/files/` checked by a `scripts/check-files-encrypted.ts` (`node --test`)
- [ ] T031 [US1] E2E `scripts/e2e/scenarios/files-encrypted.test.ts` part 1 (quickstart §2): create with notice, copy a text file, a PDF, a PNG and a sub folder in, list shows names/sizes/thumbnail, open text, PDF and image; check the bucket directly through the RustFS client: only `h`, `m/…`, `c/…` under the folder, no plaintext name anywhere (SC-001); no new file under the thumbnail cache

**Checkpoint**: US1 works end to end on one device.

---

## Phase 4: User Story 2 - Der Anbieter erfährt nichts über den Inhalt (Priority: P1)

**Goal**: nothing readable at the provider; tampering is detected per entry.

**Independent Test**: quickstart §3 (`files-encrypted` part 2) and the manipulation tests.

- [ ] T032 [P] [US2] `src-tauri/src/files/encrypted/folder_tests.rs` with `FakeStore`: the manipulation catalogue of SC-002 on a filled folder (changed byte in content, sidecar and head; content truncated at a block boundary; extended; two content objects swapped between their names; a block moved; a sidecar copied under another `sid`; a sidecar from another folder; an old content object put back under the current `oid`) → the affected entry is `damaged`, every other entry stays readable, no changed plaintext reaches a reader (FR-022 to FR-025)
- [ ] T033 [P] [US2] SC-001 catalogue in `src-tauri/src/files/encrypted/leak_tests.rs`: names with umlauts and special characters, deep paths, empty folders, every content type from 044 → no name, path part, content type or plaintext excerpt in any object key, object body or object metadata written to `FakeStore`; content type of every object `application/octet-stream`
- [ ] T034 [US2] `damaged` handling: `tree.rs` marks entries whose sidecar or content fails; `files_open`, `files_read_text`, transfers and thumbnails answer `damaged` without data; a damaged entry can only be deleted (`src/components/files/FolderView.vue`, `Viewer.vue` show the state)
- [ ] T035 [US2] E2E `files-encrypted.test.ts` part 2 (quickstart §3): change one byte of a content object and truncate another through the RustFS client; the browser shows both as damaged and still opens the others

**Checkpoint**: US1 + US2 = PR C.

---

## Phase 5: User Story 3 - Auf einem anderen eigenen Gerät lesen (Priority: P1)

**Goal**: every device of the vault reads without input; concurrent writes keep both versions; other
vaults see a neutral folder.

**Independent Test**: quickstart §4 (`pnpm test:e2e files-encrypted-two-devices`).

- [ ] T036 [P] [US3] Tests in `src-tauri/src/files/encrypted/vault_keys_tests.rs` and `head_tests.rs`: a folder wrapped with generation 1 opens after the device received generation 2 (removal of another device); a new folder uses generation 2; a head with no recipient this device can unwrap is `otherVault`
- [ ] T037 [P] [US3] Concurrency tests in `src-tauri/src/files/encrypted/ops_tests.rs`: two writers with the same `base` for one entry → both kept, the older as conflict copy; two creates of the same name in one parent (files or encrypted folders) → both shown, one as conflict copy (spec edge case "Gleicher Name zweimal")
- [ ] T038 [US3] E2E `scripts/e2e/scenarios/files-encrypted-two-devices.test.ts` (rig of spec 033, `needs.container`): device 1 creates and fills; device 2 opens without input (SC-007), adds a file, device 1 sees it after refresh; both write `notiz.txt` at once → both visible, one "notiz (Konflikt …).txt"; a second vault on the same bucket sees "Verschlüsselter Ordner einer anderen Vault" and cannot write

**Checkpoint**: US3 = PR F.

---

## Phase 6: User Story 4 - Videos und große Dateien abspielen (Priority: P2)

**Goal**: playback and seeking through the media server, never the whole file in memory.

**Independent Test**: quickstart §5.

- [ ] T039 [US4] `EncryptedFileSource` in `src-tauri/src/files/encrypted/source.rs` implementing `StreamingSource` (`src-tauri/src/files/streaming.rs`): `size()` from the sidecar, `open_range(start, len)` through the block reader of T029, the last two windows (≤ 8 MiB each) kept in memory, content type from the sidecar; register it in `files_open` for `video` and `audio`; tests in `source_tests.rs` with `FakeStore`: a range in the middle fetches only its blocks (count `get_range` calls and bytes), memory stays bounded for a 1 GB fake object
- [ ] T040 [US4] E2E `files-encrypted.test.ts` part 3: the MP4 and MP3 fixtures of 044 (`src-tauri/tests/fixtures/files/`) play from the encrypted folder; a range request from the page returns 206 with the right bytes; seek to the middle works
- [ ] T041 [US4] Manual check SC-004 (quickstart §5): a 4 GB video against a real provider, start and seek under 5 s, memory growth of holzi under 100 MB; record in the PR

**Checkpoint**: US4 = PR D.

---

## Phase 7: User Story 5 - Im verschlüsselten Ordner aufräumen (Priority: P2)

**Goal**: rename, move, copy, delete, search, copy-out question, "Mit System-App öffnen".

**Independent Test**: quickstart §6 and §9.

### Tests for User Story 5

- [ ] T042 [P] [US5] `ops_tests.rs` (manage part): rename and move of an entry write one new sidecar and delete the old, no content object (SC-006 with 1 000 entries in a sub folder: 0 writes under `c/`); rename of the encrypted folder writes only `h`; move of the encrypted folder into another ordinary folder copies every object at the provider and deletes the old ones; copy inside one folder and into another encrypted folder of the same vault copies the content object under a new `oid`, keeps `cid` and `DEK`, writes a new sidecar with a new `entry` and `dek` rewrapped for the target (contracts/format.md "Kopieren beim Anbieter"); deleting an entry removes content and sidecars of all descendants, its own sidecar last; deleting the folder removes everything under `P`, `h` last
- [ ] T043 [P] [US5] `src-tauri/src/files/encrypted/cleanup_tests.rs`: superseded sidecars and content objects without a reference are deleted only when their provider modification time is older than 24 h; younger ones stay (another device may be uploading)

### Implementation for User Story 5

- [ ] T044 [US5] `ops.rs` manage operations per T042; `files_rename` routes a rename of an encrypted folder to the head and of an entry to its sidecar; transfers (`transfer/remote.rs`) use the provider copy only inside a folder or between encrypted folders of the same vault and rewrap per T042
- [ ] T045 [US5] `src-tauri/src/files/encrypted/cleanup.rs`: run after a folder is loaded, in the background on the gate's blocking pool, per T043
- [ ] T046 [US5] Copy-out question (FR-019): in `transfer/remote_plan.rs` (`prepare_remote`), entries from an encrypted folder into an ordinary folder of a storage raise `conflict { leavesEncryption { count } }` once before the start; `files_transfer_answer` accepts `proceed`/`cancel`; target device asks nothing; `src/components/files/ConflictDialog.vue` shows the question; test in `remote_tests.rs`
- [ ] T047 [US5] Search: `search_storage` (`src-tauri/src/files/search.rs`) goes through encrypted folders via the decrypted tree without new requests once a folder is loaded; test in `search_tests.rs` that "strom" finds `Belege/2026/strom.pdf` inside an encrypted folder
- [ ] T048 [US5] "Mit System-App öffnen" (FR-028, FR-028a): `files_open_system` gets `confirmed` and `dontAskAgain`; for entries inside an encrypted folder it answers `needsNotice` unless `confirmed` or `open_ack` is set; copies go to `<AppCache>/files-opened-encrypted/<uuid>/` (never `files-opened`); a task on the vault gate token deletes the folder on lock/close and a startup sweep deletes leftovers (pattern `prune_scratch` in `src-tauri/src/extensions/fs/dialogs.rs`, without the 24 h delay); on Android the action stays hidden while 044 answers `unsupported` (research R11); window dialog in `src/components/files/` with the "Nicht mehr fragen" checkbox; tests in `files/commands/view_tests.rs` (or the existing test file of the split) for notice, ack and cleanup
- [ ] T049 [US5] E2E `files-encrypted.test.ts` part 4 (quickstart §6, §9): rename a sub folder with 1 000 files (bucket: no new `c/` object, under 30 s), move, copy, delete, search "strom", copy into an ordinary folder asks first, open with system app shows the notice; after closing the vault `files-opened-encrypted` is empty and the instance log contains no name from the folder (SC-009)

**Checkpoint**: US5 = PR E.

---

## Phase 8: User Story 6 - Agents nur mit ausdrücklicher Freigabe (Priority: P2), PR G

**Prerequisite**: 044 T074 (`agent_file_permissions`), T076 (prompt bridge), T077 (settings) and T082
(executors) are merged. External agents follow when spec 021 exists; until then the rules are built and
tested for the built-in agent and through `Caller::ExternalAgent` in unit tests.

### Tests for agents

- [ ] T050 [P] [US6] `src-tauri/src/files/access_tests.rs`: target `EncryptedFolder { storage, folder }` with `Read`, `ReadWrite`, `Denied` and reach `LocalOnly`/`AlsoCloud`; folder grant never exceeds the storage grant; no row → ask; stored `LocalOnly` with a cloud model → ask; held answers apply only to their turn
- [ ] T051 [P] [US6] SC-008 catalogue in `src-tauri/src/files/agent/encrypted_tests.rs` (list, stat, read, search over the whole storage, direct paths, raw object keys through the storage): without a grant the agent learns only "verschlüsselter Ordner <n>" with an opaque `ref` valid for the task, no name, entries, count, hits or objects

### Implementation for agents

- [ ] T052 [US6] `src-tauri/src/files/access.rs`: the target and reach of T050; migration adds `reach` TEXT ("`local` oder `cloud` bei `encryptedFolder`, sonst NULL") to `agent_file_permissions` (synced table: raise `HOLZI_TRIGGER_VERSION` in `src-tauri/src/identity/migrations.rs`), `kind = 'encryptedFolder'`, `target = '<storage_id>/<folder_id>'`; rows only with "Erlaubnis merken"
- [ ] T053 [US6] Held grants in memory per agent and task: the built-in agent's turn (`assistant_message_id`, `src-tauri/src/chat/turn/mod.rs`), dropped at the end of the turn and on lock; locality from `ProviderKind` (`src-tauri/src/storage/providers.rs`): `Local` = local, `ApiKey`, `CliDelegate` and external agents = cloud
- [ ] T054 [US6] Agent views (contracts/agent-actions.md): `files.list` anonymises encrypted folders without a grant, `files.search` skips them, the opaque `ref` resolves only within the task; prompt payload with agent, storage, decrypted folder name, model name and "auf diesem Gerät"/"in der Cloud", answers, "Erlaubnis merken" (default off) and "auch für Cloud-Modelle" (only with the checkbox and a cloud model); 60 s without answer or no window → refused for this call, nothing stored
- [ ] T055 [P] [US6] Window: the agent permission dialog of 044 (`AgentPermissionDialog.vue`) shows the folder case; the agent file permissions view of 044 T077 lists encrypted folder grants with reach and lets the user revoke them, effective immediately (FR-033); i18n
- [ ] T056 [US6] E2E `scripts/e2e/scenarios/files-encrypted-agent.test.ts` with the scripted model of the rig (quickstart §7): anonymised listing, question naming the model and locality, "Lesen" without checkbox asks again in the next turn, stored "nur lokal" asks again after switching to a cloud model, revoke works

**Checkpoint**: agents = PR G.

---

## Phase 9: User Story 6 - Erweiterungen nur mit ausdrücklicher Freigabe (Priority: P2), PR H

### Tests for extensions

- [ ] T057 [P] [US6] `src-tauri/src/extensions/permissions/manifest_map_tests.rs` and `src-tauri/src/extensions/registry/install_tests.rs`: a manifest with `encryptedFolder` (also under an alias) is refused by `install_preview`, `install`, update and dev loading (`src-tauri/src/extensions/dev.rs`) with `BundleRejection::ForbiddenPermission { category }`; an update cannot inherit stored grants of another extension or a dev version (FR-037)
- [ ] T058 [P] [US6] `src-tauri/src/extensions/encrypted_folder_tests.rs`: without `remoteStorage` for the storage every function returns 1002 without a prompt; with it, `choose` prompts; answers without "merken" are held until the frame unloads or reloads (`forget` on frame teardown), with "merken" stored; `read` refuses upload and delete; `path` validated like 038 FR-011 (3001); no function returns object keys, keys, sidecars or raw bytes; damaged entry → 3010

### Implementation for extensions

- [ ] T059 [US6] Permission kind `encryptedFolder` in `src-tauri/src/extensions/permissions/model.rs` (`PermissionKind::ALL`, `Action::parse` with `read`/`readWrite`), target `<storage_id>/<folder_id>` in `target.rs`; `manifest_map.rs` (`category_kind`) reports it as forbidden; `BundleRejection::ForbiddenPermission` in `src-tauri/src/extensions/registry/install.rs` (`install_preview`, `install`) and `dev.rs`
- [ ] T060 [US6] Held answers until frame end: in `src-tauri/src/extensions/bridge/prompts.rs` (`PermissionState::hold`/`forget`) forget `encryptedFolder` answers when the extension's frame is unloaded or reloaded, not only at process end; the remember checkbox of `src/components/extensions/PermissionRequestDialog.vue` defaults to off for this kind and the dialog names storage and decrypted folder name
- [ ] T061 [US6] Bridge functions `extension_encrypted_folder_{choose,list,download,upload,delete}` in `src-tauri/src/extensions/encrypted_folder.rs`, routed in `src-tauri/src/extensions/bridge/dispatch.rs`, using `files/encrypted` (no second implementation, FR-041) and the size and time limits of 017 (contracts/bridge.md)
- [ ] T062 [P] [US6] `src/components/extensions/EncryptedFolderChooser.vue`: list of readable encrypted folders of the storage with names, the question in the same view (Lesen erlauben / Lesen und Schreiben erlauben / Ablehnen, "Erlaubnis merken" default off); mounted next to `PermissionRequestDialog.vue` in `src/components/wm/Desktop.vue`; i18n
- [ ] T063 [P] [US6] `src/components/settings/extensions/ExtensionPermissionsView.vue`: stored and held `encryptedFolder` grants with folder name, revoke effective immediately; uninstalling the extension removes its grants (FR-038)
- [ ] T064 [US6] vault-sdk PR in `~/Projekte/vault-sdk` (own worktree, haexmas fork, no agent attribution): types and functions for `extension_encrypted_folder_*`, changelog; bump the pin in holzi only if the E2E fixture needs it
- [ ] T065 [US6] E2E `scripts/e2e/scenarios/extension-encrypted-folder.test.ts` with the probe extension (`src-tauri/tests/fixtures/extension_e2e/`, quickstart §8): manifest with the kind refused; without `remoteStorage` refused without prompt; with it chooser and question; without checkbox asked again after reloading the extension; with checkbox no question; revoke works

**Checkpoint**: extensions = PR H.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [ ] T066 [P] Performance check against RustFS in `files-encrypted.test.ts` or a separate measurement noted in the PR: 1 000 entries first open < 5 s and second < 1 s, 10 000 < 30 s, 20 heads < 2 s (SC-003); upload/download at most 20 % slower than an ordinary folder and ≤ 1 % larger for files ≥ 1 MB (SC-005)
- [ ] T067 [P] Log audit (FR-027): grep every `log::` under `src-tauri/src/files/`, `src-tauri/src/extensions/encrypted_folder.rs` and `src-tauri/src/remote_storage/` for paths or names that can come from an encrypted folder; fix with `Redacted`; result in the last PR
- [ ] T068 Update `plans/README.md` (048 row status per merged PR) and note in the rows "Sync-Regeln" and "Umbau 027/029" that the format module `files/encrypted/format` is ready to use
- [ ] T069 Run quickstart §1–§9 once after the last PR and record the results in this file

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1** (docs): none.
- **Phase 2**: B0 first (T003–T004), then PR B (T005–T013), then T014–T018 at the start of PR C.
- **US1** (Phase 3): needs Phase 2.
- **US2** (Phase 4): needs US1 (it checks what US1 writes); ships with US1 in PR C.
- **US4** (Phase 6): needs US1 (block reader T029).
- **US5** (Phase 7): needs US1; T046 and T044 touch the same transfer files as T027.
- **US3** (Phase 5): needs US1 and US5's conflict handling in writes (T044).
- **US6 agents** (Phase 8): needs US1 and 044 T074, T076, T077, T082.
- **US6 extensions** (Phase 9): needs US1; independent of Phase 8.
- **Polish**: after the stories it measures.

### Within each story

Tests first, then Rust core, then commands, then window, then E2E.

### Parallel Opportunities

- PR B: T006–T011 are separate files ([P]); T012 and T013 after them.
- Start of PR C: T014, T015 in parallel; T016–T018 after T015/T011.
- US1: T019–T022 (tests) in parallel; T030 (window) in parallel with T026–T029.
- Phases 8 and 9 can run in parallel once US1 is merged and 044's agent work is in.

## Parallel Example: PR B

```text
T007 keys.rs + keys_tests.rs
T008 head.rs + head_tests.rs
T009 content.rs + content_tests.rs
T010 sidecar.rs + sidecar_tests.rs
T011 format/mod.rs
(T006 vectors first; each test file reads them)
```

## Implementation Strategy

### MVP

PR B0, PR B, then PR C (US1 + US2): one device can create, fill, browse and open an encrypted folder,
and the provider sees nothing readable. Stop and validate with quickstart §1–§3.

### Incremental Delivery

PR D (media) → PR E (manage) → PR F (two devices) → PR G (agents, when 044 is ready) → PR H
(extensions). Each PR is usable on its own and keeps earlier stories working.
