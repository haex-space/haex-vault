---
description: 'Task list for spec 052-storage-credentials-picker'
---

# Tasks: Zugangsdaten der Cloud-Speicher aus dem Passwortmanager wählen

**Input**: Design documents from `/specs/052-storage-credentials-picker/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md),
[quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic; SC-002 (no
duplicate entry), SC-003 (tag on every entry), SC-004 (changes in the password manager apply) and SC-005
(no secret leaves Rust) need dedicated tests. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] mod x_tests;` in `mod.rs`), never inline; helpers `setup`, `typed`, `input`, `entries` of
`service_tests.rs` are reused. No network services in `cargo test` (the fake store of
`test_support.rs`). Build only the affected targets with `-j 4`.

**Organization**: Grouped by user story in priority order (P1: US1, US2; P2: US3; P3: US4). US1 is the
MVP.

**Shipping note** (plan.md): PR A = Phase 1 (docs, #424); PR B = Phase 2 + US1; PR C = US2; PR D = US3 +
US4 + Polish. Each PR is reviewed and merged before the next one starts on top of `main`.

**Rules for every task**: no secret in any answer, log or error (SC-005); `Zeroizing` for values in
Rust; `service.rs` (624 lines) only gets calls, new logic goes to `service_link.rs`; `ConnectionForm.vue`
(476 lines) must not grow; before authoring a new named function, component or module consult graphify
(`graphify query`) and extend a candidate if one fits (plan.md lists those evaluated).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US4)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Spec, plan, research, data model, contract, quickstart in `specs/052-storage-credentials-picker/`; pointers in `specs/038-storage-connections/spec.md` and `specs/050-webdav-sftp-storage/spec.md` (PR #424)
- [x] T002 [P] Add a row for 052 to `plans/README.md` after the 050 row: "052: Zugangsdaten der Cloud-Speicher aus dem Passwortmanager wählen (`specs/052-storage-credentials-picker/`)", priority P2, effort M, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-14; Lieferungen PR A–D", gate "034, 038 und 050 im Einsatz"
- [x] T003 Run `pnpm format:check` (specs are covered), commit `tasks.md` and the README row, push to PR #424

---

## Phase 2: Foundational (PR B, blocks all user stories)

**Purpose**: The one mapping rule, the state that notices missing values, and the new types. No
behaviour visible to the user yet.

- [ ] T004 In `src-tauri/src/remote_storage/credentials.rs` extract the pure mapping of `read` into `pub fn from_item(item: &SecretItem, protocol: Protocol) -> Option<Credentials>` exactly as in data-model.md (S3: username = access key id, password = secret, custom field `sessionToken` optional; WebDAV and SFTP: username and password; SFTP: username, empty password and custom field `privateKey`, optional `passphrase`; "Leere Werte zählen als fehlend"); `read` calls it and keeps its errors (`StorageCredentialsUnavailable { state }`)
- [ ] T005 [P] In `src-tauri/src/remote_storage/credentials_tests.rs` add tests for `from_item` per protocol: S3 with and without token, WebDAV, SFTP with password, SFTP with key and passphrase, empty username or empty secret gives `None`, SFTP without password and without `privateKey` gives `None`, S3 entry without `sessionToken` has no token
- [ ] T006 Change `credentials::state_in(q, item_id)` and `credentials::state(db, item_id)` in `src-tauri/src/remote_storage/credentials.rs` to take the `Protocol`: an entry that is present and not in the trash but where `from_item(secret_item(q, id), protocol)` is `None` is `CredentialsState::Missing` (spec FR-010; before it was `Present`); update all callers: `service.rs` in `overview` and in `save_connection_in`, and `service_host_key.rs`. `crate::passwords::reveal::secret_item` is already `pub`
- [ ] T007 [P] In `src-tauri/src/remote_storage/credentials_tests.rs` add tests for the state: password cleared on a present entry gives `Missing`, `privateKey` field removed from an SFTP key entry gives `Missing`, restored value gives `Present` again, entry in the trash `Missing`, deleted entry `Missing`, not arrived `Syncing` (existing cases stay green with the protocol argument)
- [ ] T008 In `src-tauri/src/remote_storage/model.rs` add `CredentialsInput::Entry { item_id: String }` (`kind: 'entry'`, camelCase) with its `Debug` arm (no value besides the opaque id) and the new structs `CredentialsCandidate { id, title: Option<String>, username: String, url: Option<String>, tagged: bool, owned: bool }` and `CredentialsEntry { id, title: Option<String>, owned: bool }` (`Serialize`, `TS`, export to `../../src/types/bindings/` like the other types); `credentials_of` in `service_credentials.rs` keeps rejecting every kind it does not know with `invalid("credentials")`, so `Entry` never reaches it
- [ ] T009 Regenerate the ts-rs bindings in `src/types/bindings/` (`CredentialsInput.ts`, `CredentialsCandidate.ts`, `CredentialsEntry.ts`) and normalize trailing whitespace (`sed -E 's/[[:space:]]+$//'`, the CI checks it); run `cargo test -j 4 -p holzi --lib remote_storage::credentials` and fix every caller that matched `CredentialsInput` exhaustively

**Checkpoint**: Mapping rule in one place, state sees missing values, types exist. Commit, PR B continues with US1.

---

## Phase 3: User Story 1 - Zugangsdaten aus einem vorhandenen Eintrag nehmen (Priority: P1) 🎯 MVP

**Goal**: Anna picks an existing password manager entry instead of typing credentials; holzi tests with
its values and saves a connection that points at it (spec US1, FR-001 to FR-004, FR-013 for the
backend).

**Independent Test**: Create an entry with username and password in the password manager, pick it in
the form of an S3 or WebDAV connection, "Testen und speichern": the connection exists, the password
manager has no new entry, a call of the storage uses the entry's values (quickstart §1 and §2 steps 1–2).

### Tests for User Story 1

- [ ] T010 [P] [US1] Create `src-tauri/src/remote_storage/service_link_tests.rs` and declare `#[cfg(test)] mod service_link_tests;` in `src-tauri/src/remote_storage/mod.rs`; tests with the helpers of `service_tests.rs`: (a) `a_chosen_entry_makes_a_connection_without_a_new_entry` (`entries(&db)` unchanged, `credentials_item_id` is the chosen id, test ran with the entry's values on the fake store); (b) `a_failed_test_with_a_chosen_entry_saves_nothing`; (c) an entry that is not there, in the trash, owned by another caller, or without the values of the protocol is `StorageCredentialsUnavailable`; (d) an SFTP key entry works with its passphrase (use the SFTP fake of `service_sftp_tests.rs`); (e) a chosen entry for an existing connection replaces the link only after a passed test; (f) an owned entry of the user's own `storage` is also choosable
- [ ] T011 [P] [US1] In `src-tauri/src/remote_storage/credentials_tests.rs` add tests for the candidates: only fitting entries for the protocol, none from the trash, an entry with another owner than the user or `storage` is not listed, tagged first then title without regard to case, `username` is the access key id for S3, and the serialized JSON of the list contains none of the test secrets (SC-005)

### Implementation for User Story 1

- [ ] T012 [US1] In `src-tauri/src/remote_storage/credentials.rs` add `pub async fn candidates(db: &VaultDb, protocol: Protocol) -> Result<Vec<CredentialsCandidate>>` per research R3: `items::headers_in_scope(q, &Scope::All, &Caller::User)` (no trash), prefilter by header (`username` not empty; S3 and WebDAV need `has_password`), then `reveal::secret_item` and `from_item` for the rest; `tagged` from the header tags (compare by `tags::fold`) with the new `pub const TAG: &str = "haex-cloud-storage"` defined here, the one definition of the name, `owned = header.owner.as_deref() == Some(FEATURE)`; sort tagged first, then `title` case-insensitively; `Protocol::Local` is `invalid("protocol")`
- [ ] T013 [US1] In `src-tauri/src/remote_storage/commands.rs` add `#[tauri::command] storage_credentials_candidates(state, protocol: Protocol) -> Result<Vec<CredentialsCandidate>>` and register it in `src-tauri/src/lib.rs` next to `storage_connection_save`
- [ ] T014 [US1] Create `src-tauri/src/remote_storage/service_link.rs` (module `#[path = "service_link.rs"] mod link;` in `service.rs` like the existing `service_host_key.rs`) with the resolution of `CredentialsInput::Entry` for `save_connection_in`: `async fn resolve(&self, input: CredentialsInput, protocol: Protocol) -> Result<Resolved>` where `Resolved` is `Typed(Credentials)` (through `credentials_of`) or `Linked { item_id, credentials }` (through `credentials::read(&self.passwords, &self.db, &item_id, protocol)`); in `service.rs` replace `new_credentials` handling so that: the test uses the resolved credentials (`test_with`), `credentials::create` runs only for `Typed`, `row.credentials_item_id = item_id` for `Linked`, the rollback after a failed `put_connection` deletes only an entry this call created, and `delete_credentials_if_unused(&old)` runs when the link changed to a different id; `generated` key handling stays for `Typed`
- [ ] T015 [US1] Run the tests of T010 and T011 and `cargo test -j 4 -p holzi --lib remote_storage` (the existing service tests must stay green); fix `service.rs` call sites; no new `unwrap()` or `expect()` on input
- [ ] T016 [P] [US1] Create `src/components/settings/storage/credentialsInput.ts` (pure, no Vue): `credentialsInputOf(draft, protocol): CredentialsInput | null` moved out of `ConnectionForm.vue` (the existing builder of the typed credentials) with a new draft part `source: 'new' | 'entry'` and `entryId: string`; for `source === 'entry'` it returns `{ kind: 'entry', itemId: entryId }` (or `null` when `entryId` is empty); `CredentialsDraft` in `CredentialsFields.vue` gets `source` and `entryId` (default `'new'`, `''`)
- [ ] T017 [P] [US1] Create `scripts/check-storage-credentials.ts` (`node:test`, like `scripts/check-passwords-draft.ts`) for `credentialsInput.ts`: each protocol and auth method gives the right kind, `entry` with and without id, empty fields give `null`; add `"check:storage": "node --test scripts/check-storage-*.ts"` to `package.json` and a step "Check storage" with `corepack pnpm check:storage` after "Check passwords" in `.github/workflows/ci.yml`
- [ ] T018 [US1] Create `src/components/settings/storage/CredentialsEntryPicker.vue` (under 500 lines): props `protocol`, v-model `entryId`; on mount `invoke('storage_credentials_candidates', { protocol })`; search field (title, username, url, client side, case-insensitive); boxed list with `Group`/`Row`/`OptionRow` (UI style COSMIC/GNOME), tagged entries first as delivered; empty state text "Keine passenden Einträge" with the hint that only fitting entries appear (FR-002); shows title, username and url, never a secret; `data-testid` `storage-entry-picker`, `storage-entry-search`, `storage-entry-option-<id>`
- [ ] T019 [US1] In `src/components/settings/storage/ConnectionForm.vue` add the source choice ("Neu eingeben" / "Eintrag aus dem Passwortmanager", `SettingsSelect` like `authMethod`, `data-testid="storage-credentials-source"`, "Neu" preselected for a new connection, offered together with `replaceCredentials` for a saved one); with `entry` show `CredentialsEntryPicker` instead of `CredentialsFields`; submit through `credentialsInputOf` from T016; keep the file at or under its current length by moving what is replaced into `credentialsInput.ts`
- [ ] T020 [P] [US1] Add the texts in `src/i18n/locales/de.json` and `en.json` under `settings.storage` (`credentialsSource` with `new` and `entry`, `entryPicker` with `search`, `empty`, `onlyFitting`); the label "Passwortmanager" and "Cloud-Speicher" as elsewhere; never a bare "Relay"; no `<…>` in messages (vue-i18n refuses it)
- [ ] T021 [US1] Create `scripts/e2e/scenarios/storage-credentials-entry.test.ts` (model: `storage-webdav.test.ts`, Docker WebDAV like there): part 1 = quickstart §2 steps 1–2: create an entry in the password manager (`scripts/e2e/lib/passwords.ts`), pick it in a new WebDAV connection, assert the connection is saved, the entry count is unchanged and `storage_list` shows `credentials: 'present'`
- [ ] T022 [US1] Local checks: `pnpm check:storage`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, clippy of the touched crate targets; commit in small Conventional-Commit steps (`feat(storage): …`) and open PR B

**Checkpoint**: A connection can be created from an existing entry, tested, with no duplicate. MVP done.

---

## Phase 4: User Story 2 - Neue Zugangsdaten liegen sichtbar im Passwortmanager (Priority: P1)

**Goal**: Credentials holzi creates, and entries the user chooses, carry the tag `haex-cloud-storage`;
the form names the entry and opens it (spec US2, FR-005 to FR-008).

**Independent Test**: Create a connection with typed credentials; filter by the tag in the password
manager: the entry is there with title, username, url and can be edited; "Eintrag öffnen" in the form
shows exactly this entry (quickstart §2 step 5, §4).

### Tests for User Story 2

- [ ] T023 [P] [US2] In `src-tauri/src/remote_storage/credentials_tests.rs` add: `create` gives the entry the tag `haex-cloud-storage`, title `<Protokoll>: <Name>`, username, url; `ensure_tagged` tags every entry owned by `storage` that lacks the tag and leaves title and values alone; running it twice changes nothing (derived link ids); an entry of another owner and the user's own entries are not tagged by it
- [ ] T024 [P] [US2] In `src-tauri/src/remote_storage/service_link_tests.rs` add: a chosen entry gets the tag after the connection is saved, its owner, folder and grants are unchanged (FR-008); a failed test tags nothing; a failing tag write (entry deleted between test and tag) does not fail the saved connection; `storage_list` carries `credentialsEntry` with the entry's title and `owned` for both kinds and none when the entry is missing
- [ ] T025 [P] [US2] In `src-tauri/tests/passwords_owner.rs` add: the tag on an owned entry changes nothing about Z14 (an extension with a grant for `*` and one for the tag still do not reach it); in `src-tauri/tests/passwords_access.rs` add: a grant for the tag `haex-cloud-storage` reaches a chosen (unowned) entry but not an owned one (documents the assumption in spec.md)

### Implementation for User Story 2

- [ ] T026 [US2] In `src-tauri/src/remote_storage/credentials.rs` use `TAG` (defined in T012) to put `tags: vec![TAG.to_owned()]` into the `ItemInput` of `create`, and add `pub fn ensure_tagged(tx: &mut CrdtTransaction<'_>) -> Result<u32>` that selects the ids of `haex_passwords_item_details` with `owner = 'storage'` and calls `tags::bulk_set(tx, &ids, &[TAG.to_owned()], &[])`; add `pub fn start_after_open(state: &AppState)` in the style of `passwords::maintenance::start_after_open` (tracked session work, failure logged without value, never blocks opening) and call it in `src-tauri/src/instances/open.rs` (next to line 154) and `src-tauri/src/instances/import.rs` (next to line 208)
- [ ] T027 [US2] In `src-tauri/src/remote_storage/service_link.rs` add the tagging of a chosen entry after `put_connection` succeeded: `self.passwords.set_tags(&Caller::User, vec![item_id], vec![TAG.to_owned()], vec![])`; an error is logged with `log::warn!` (no value) and does not fail the save (research R4)
- [ ] T028 [US2] Add `credentials::entry_in(q, item_id, protocol) -> Result<Option<CredentialsEntry>>` in `credentials.rs` (present, not in the trash, fits; title from the header, `owned` from the owner) and carry it as `credentials_entry: Option<CredentialsEntry>` in `ConnectionView` (`#[ts(optional)]`, `skip_serializing_if = "Option::is_none"`); fill it where `view(row, state)` is built in `service.rs` (`overview`, `save_connection_in`) and `service_host_key.rs` (extend `view` by the argument, one place); regenerate the bindings (`ConnectionView.ts`) with the whitespace normalization of T009
- [ ] T029 [US2] In `src/components/settings/storage/ConnectionForm.vue` show for a saved connection the title of `existing.credentialsEntry` ("Zugangsdaten im Passwortmanager: <Titel>", never a secret, `data-testid="storage-credentials-entry"`) and a button "Eintrag öffnen" (`data-testid="storage-credentials-open"`) calling `useWmTab().openApp('system.passwords', `/entry/${id}`)` (the path carries only the opaque id, 034 FR-039); texts in `de.json` and `en.json` under `settings.storage.credentialsEntry`; if the form would exceed 500 lines, move the display into a small component `CredentialsEntryNote.vue` in `src/components/settings/storage/`
- [ ] T030 [US2] Extend `scripts/e2e/scenarios/storage-credentials-entry.test.ts` with quickstart §2 step 5: a connection with typed credentials has an entry with the tag (filter in the password manager), the form names it and "Eintrag öffnen" shows it; removing the connection removes that entry
- [ ] T031 [US2] Local checks as T022; commit and open PR C

**Checkpoint**: Every entry holzi made or the user chose is findable by one tag, and the form leads there.

---

## Phase 5: User Story 3 - Änderungen und Löschen wirken wie erwartet (Priority: P2)

**Goal**: The entry is the only source of the values: edits apply on the next call, deleting warns, then
the connection asks for new credentials (spec US3, FR-009, FR-010, FR-012, FR-014).

**Independent Test**: Quickstart §2 steps 3–4.

### Tests for User Story 3

- [ ] T032 [P] [US3] In `src-tauri/src/remote_storage/service_link_tests.rs` add: after the password of a chosen entry changed, `access_of(storage_id)` uses the new value (`FakeStore` sees it); removing a connection keeps a chosen entry and deletes an owned one only when no other connection uses it; two connections on one entry keep it until the last goes; switching from an owned entry to a chosen one deletes the old owned entry after the passed test; switching to the same id deletes nothing; an entry in the trash or deleted gives state `missing` and `access_of` fails with `StorageCredentialsUnavailable` without any value in the message
- [ ] T033 [P] [US3] In `src-tauri/src/remote_storage/credentials_tests.rs` extend the usage test: `StorageUsage::uses` is true for a chosen (unowned) entry used by a connection, and the password manager's delete path names the connection for it (034 FR-034); the delete itself stays allowed

### Implementation for User Story 3

- [ ] T034 [US3] If T033 fails, fix the cause in `credentials.rs` (`StorageUsage`) or in the delete path of `src-tauri/src/passwords/` for unowned entries (grep every caller of the usage registry; root cause, not the symptom path); otherwise record "no change needed" in the PR description
- [ ] T035 [US3] In `src/components/settings/storage/ConnectionForm.vue` and `StorageListView.vue`: when `credentials` is `missing` for a connection whose entry was deleted or no longer fits, show "neue Zugangsdaten nötig" with the two actions "Anderen Eintrag wählen" and "Neue Zugangsdaten eingeben" (they set `source` to `entry` or `new` with `replaceCredentials` on, `data-testid` `storage-credentials-choose-other`, `storage-credentials-enter-new`); `syncing` keeps its existing text; texts in `de.json` and `en.json`
- [ ] T036 [US3] Extend `scripts/e2e/scenarios/storage-credentials-entry.test.ts` with quickstart §2 steps 3–4 (wrong password in the entry gives "Zugang abgelehnt", restored passes; delete with the warning naming the connection, then "neue Zugangsdaten nötig", then picking another entry runs again)
- [ ] T037 [US3] Local checks as T022; commit; include in PR D

**Checkpoint**: Edits and deletes behave like one source of truth.

---

## Phase 6: User Story 4 - Auch im Fenster für Speichervorschläge von Erweiterungen (Priority: P3)

**Goal**: The same choice in holzi's window over the app; the extension learns nothing of it (spec US4,
FR-011, FR-013).

**Independent Test**: Quickstart §3, second bullet.

### Tests for User Story 4

- [ ] T038 [P] [US4] In `src-tauri/src/extensions/remote_storage_credentials_tests.rs` add: an answer with `credentials: Entry { item_id }` creates connection and storage and grants the permission; the answer the extension gets carries no entry id and no title; a bridge call with `credentials` or an entry id in its params is still refused (existing refusal test stays green)

### Implementation for User Story 4

- [ ] T039 [US4] In `src/components/StorageCredentialsModal.vue` offer the same source choice and `CredentialsEntryPicker` (protocol of the proposal), send `{ kind: 'entry', itemId }` through `storage_dialog_resolve` (a failed test keeps the window open, as for typed values); reuse `credentialsInputOf` from T016 where its draft fits the modal's fields, otherwise add only the `entry` branch to the modal's own builder; texts reuse the keys of T020
- [ ] T040 [P] [US4] Extend `scripts/e2e/scenarios/extension-storage-webdav.test.ts` (Docker) with a step: the test extension proposes a WebDAV storage without credentials; in holzi's window choose an existing entry; the storage exists and the extension's answer has no entry id
- [ ] T041 [US4] Local checks as T022; commit; include in PR D

**Checkpoint**: Both entry points use the same choice.

---

## Phase 7: Polish & Cross-Cutting (PR D)

- [ ] T042 [P] Mark the tasks done in this file; set the status of the 052 row in `plans/README.md`
- [ ] T043 [P] Check `src-tauri/src/remote_storage/service.rs`, `ConnectionForm.vue`, `credentials.rs` and every new file against the 500-line rule (`wc -l`); a file over it needs a documented reason and a split plan (constitution)
- [ ] T044 Full local parity before the PR: `pnpm check:storage`, `pnpm check:passwords`, `pnpm check:settings`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm check:templates`, `pnpm lint:rust`, the targeted `cargo test -j 4` of `remote_storage`, `passwords_owner`, `passwords_access`, the extension storage tests, and the e2e scenarios `storage-credentials-entry`, `storage-webdav`, `extension-storage-webdav` (needs Docker; skipped without it, say so in the PR)
- [ ] T045 Manual quickstart §3 (Android emulator `holzi-emulator`: list scrolls, keyboard does not cover the search) and §4 (older vault: all entries "S3: …", "WebDAV: …", "SFTP: …" carry the tag, second open changes nothing) by the operator; open until done, like T081 of spec 024

---

## Dependencies & Execution Order

- Phase 1 (docs) is PR #424. Phase 2 blocks everything else (T004 → T006 → T008/T009).
- US1 needs Phase 2. US2 needs US1 (the tag on a link needs the link; `credentialsEntry` needs the form of US1). US3 needs US1 (tests and form actions) and uses `credentialsEntry` of US2 only for the form text. US4 needs US1 (picker and input form).
- Within a story: tests first (they fail), then implementation, then UI, then e2e.
- Order of PRs: B (Phase 2 + US1), C (US2), D (US3 + US4 + Polish), each on top of the merged one.

### Parallel Opportunities

- Phase 2: T005 and T007 (tests in one file, run as one task if two writers clash) while T008 is written.
- US1: T010 and T011 together; T016, T017 and T020 together (different files); T018 after T016.
- US2: T023, T024 and T025 together (three files).
- US3: T032 and T033 together.
- US4: T038 and T040 together.

---

## Implementation Strategy

### MVP First

1. Phase 2 (mapping, state, types) then US1: stop and validate with quickstart §1 and §2 steps 1–2. This
   alone removes retyping of credentials and duplicate entries.
2. US2 adds the tag and the visible entry; US3 the behaviour on change and delete; US4 the second entry
   point. Each can ship alone after US1.

### Notes

- The tag name `haex-cloud-storage` is a single constant (`credentials::TAG`); renaming it touches that
  constant, the tests and the e2e scenario only.
- No schema change and no migration; nothing to bump in the trigger version.
- Commit messages: Conventional Commits, no agent attribution (constitution); merge, not squash.
