---
description: 'Task list for spec 051-phone-contacts'
---

# Tasks: Adressbuch in holzi mit Abgleich zum Telefon-Adressbuch (Android)

**Input**: Design documents from `/specs/051-phone-contacts/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md),
[contracts/android-plugin.md](./contracts/android-plugin.md), [contracts/bridge.md](./contracts/bridge.md),
[contracts/contact-card.md](./contracts/contact-card.md), [contracts/nostr-invite.md](./contracts/nostr-invite.md),
[quickstart.md](./quickstart.md), [ADR 0014](../../docs/adr/0014-invitations-from-the-address-book.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic; FR-022/FR-023 (no
lost field), FR-016/FR-026 (no silent mass deletion), FR-031 (no leak on refusal) and the card checks
need dedicated tests. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. The phone sync is tested on the
desktop against `FakePhoneBook` (research R5); the emulator only runs E2E. Build only the affected
targets with `-j 4`.

**Organization**: Grouped by user story in priority order (P1: US1, US2, US3; P2: US4, US5; P3: US6,
US7).

**Shipping note** (plan.md, Lieferungen): PR A = Phase 1 (docs: #418 spec, #426 plan, this tasks PR);
PR B = Phase 2 + US1; PR C = US2; PR D = US3; PR E = US4; PR F = US5 + the vault-sdk PR; PR G = US6;
PR H = US7 with or after the first PR of spec 027 or 028. Each PR is reviewed and merged before the
next one starts on top of `main` (never stack on an open PR: a stacked PR merged after its base lands in
the dead base branch).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US7)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Spec, clarifications, plan, research, data model, contracts, quickstart and ADR 0014 in `specs/051-phone-contacts/` and `docs/adr/0014-invitations-from-the-address-book.md` (PRs #418, #426)
- [x] T002 [P] Add a row for 051 to `plans/README.md` after the 050 row (check whether 052 already has one and keep numeric order): "051: Adressbuch mit Abgleich zum Telefon-Adressbuch (`specs/051-phone-contacts/`)", priority P2, effort L, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-14; Lieferungen PR A–H", gate "024 und 043 im Einsatz; US5 nach 017 (im Einsatz); externe Agenten mit 021; US7 mit 027/028"
- [ ] T003 Run `pnpm format:check`, commit `specs/051-phone-contacts/tasks.md` and `plans/README.md`, open the tasks PR

---

## Phase 2: Foundational (PR B, first half)

**Purpose**: Shared caller type, all tables, core types and the access rules that every story uses.

- [x] T004 Prepare the worktree for code: real `pnpm install` (no symlinked `node_modules`), reflink `src-tauri/target` from a worktree with the same `Cargo.lock` if one exists (or the parked `.worktrees/.043-target-cache`), check `nix develop --command scripts/with-nix-host-bridge.sh cargo check -j 4 --manifest-path src-tauri/Cargo.toml`
- [x] T005 [P] Before the first new name, run bounded `graphify query … --budget 1200` for "caller enum shared by services", "service access check grants scope", "content addressed binaries orphan cleanup", "image downscale jpeg thumbnail", "preferences device scope"; note the candidates checked in the PR description (warn and continue if graphify fails)
- [x] T006 Shared caller: move `enum Caller` from `src-tauri/src/passwords/access.rs:19` into a new `src-tauri/src/caller.rs` (same variants `User`, `BuiltinAgent`, `Extension { id }`, `ExternalAgent { id }`, `Internal { feature }`, same derives and docs), `pub use crate::caller::Caller;` in `passwords/access.rs` so existing paths keep compiling; grep every `access::Caller` / `Caller::` user and leave them unchanged; move the caller-only tests (if any) to `src-tauri/src/caller_tests.rs` (research R2) **Note**: `Patch<T>` moved the same way to `src-tauri/src/patch.rs` (re-exported by `passwords::model`), so the address book takes partial updates without depending on the password manager
- [x] T007 Migration `0033_contacts` in a new `src-tauri/src/identity/migrations_contacts.rs` with the twelve tables of data-model.md exactly (`haex_contacts`, `haex_contacts_values`, `haex_contacts_cards`, `haex_contacts_groups`, `haex_contacts_group_members`, `haex_contacts_binaries`, `haex_contacts_targets`, `haex_contacts_phone_links`, `haex_contacts_phone_issues`, `haex_contacts_phone_runs`, `haex_contacts_own_card`, `haex_contacts_blocked_vaults`); rules from the header of `migrations_passwords.rs`: every table a PK, no `WITHOUT ROWID`, no UNIQUE, FKs `ON DELETE CASCADE` as listed ("→ `known_devices` CASCADE" for every `vault_device_uuid`), statement breakpoints between statements; register under the next free number in `holzi_migration_source()` in `src-tauri/src/identity/migrations.rs` (check `main` first; 0033 if still free), raise `HOLZI_TRIGGER_VERSION` by one with the doc line "`00NN_contacts` (spec 051) added the address book tables"; test in `src-tauri/src/identity/migrations_contacts_tests.rs`: all tables exist, a row in each survives a CRDT round trip, deleting a contact cascades to values, targets, links, issues, cards and group members
- [x] T008 [P] Core types in a new `src-tauri/src/contacts/mod.rs` (declare `pub mod contacts;` in `src-tauri/src/lib.rs`): `Contact`, `ContactHeader`, `Value { id, kind, label, value, position }`, `Origin { kind: OriginKind (holzi|phone|card), device, caller, account }`, `Group`, ts-rs exports into `src/types/bindings/`; `src-tauri/src/contacts/ids.rs` with the UUIDv5 derivations of data-model.md (`target_id(contact, device)`, `link_id(device, raw_contact_id)`, `member_id(group, contact)`, `issue_id(device, contact)`) under one namespace constant; tests in `src-tauri/src/contacts/ids_tests.rs` (stable, distinct per input) **Note**: the types live in `src-tauri/src/contacts/model.rs`; their TypeScript names carry a `Contact` prefix (`ContactValue`, `ContactGroup`, …) so they do not clash in `src/types/bindings/`
- [x] T009 [P] Kinds in a new `src-tauri/src/contacts/kinds.rs`: the known kinds of data-model.md (`phone`, `email`, `address`, `website`, `im`, `sip`, `event`, `relation`, `custom`, `holzi_vault`, `holzi_nostr_relay`, `holzi_iroh`), `fn is_known`, `fn goes_to_phone` (true only for `phone`, `email`, `address`, `website`, `im`, `sip`, `event`, `relation`; FR-014), `fn validate(kind, label, value)`: `address` JSON `{street, pobox, neighborhood, city, region, postcode, country}`, `im` JSON `{service, handle}`, `event` JSON `{date: "YYYY-MM-DD" | "--MM-DD"}`, label `home | work | mobile | other | custom:<text>`, `holzi_vault` "muss ein gültiger npub sein", `holzi_nostr_relay` "eine `wss://`- oder `ws://`-Adresse", `holzi_iroh` "64 Hex-Zeichen mit optionaler Relay-URL" (`@<url>`); unknown kinds pass validation unchanged (they are never written by this version); tests in `src-tauri/src/contacts/kinds_tests.rs` **Note**: the checks of Nostr and iroh relay addresses are the ones of the settings, now `sync::servers::is_nostr_relay` / `is_iroh_relay`
- [x] T010 Access rules in a new `src-tauri/src/contacts/access.rs` after `passwords/access.rs`: `GrantAction { Read, ReadWrite }`, `Scope { All, Groups(BTreeSet<String>) }`, `Grant`, `Denied { Forbidden, NotFound }`, `authorize_list`, `authorize_read(contact_groups)`, `authorize_write(contact_groups)`; `Caller::User` and `Caller::Internal` always pass (FR-028); a contact outside the scope is `NotFound`, exactly like a missing one (FR-031); tests in `src-tauri/src/contacts/access_tests.rs` (extension without grant, read-only grant writing, group scope, external agent, built-in agent without grant)
- [x] T011 Service skeleton in a new `src-tauri/src/contacts/service/mod.rs`: `ContactsService { db: VaultDb }` (field private to the module), constructor from the vault state like `passwords/service/mod.rs:40`, `fn caller_check` helpers; `src-tauri/src/contacts/commands/mod.rs` with `service(&state)` like `passwords/commands/mod.rs:28` **Note**: SQL in `src-tauri/src/contacts/store.rs`; the device of the origin is `VaultDb::device_id()`

**Checkpoint**: tables exist, every later access goes through `ContactsService` and `contacts::access`.

---

## Phase 3: User Story 1 - Adressbuch in holzi nutzen (Priority: P1) 🎯 MVP (PR B, second half)

**Goal**: The built-in app "Kontakte" lists, searches, creates, edits and deletes contacts on every platform; changes reach all own devices.

**Independent Test**: On a desktop without a phone create, search, edit and delete contacts; a second own device shows every change after sync (quickstart: `contacts-basic`, `contacts-two-devices`, `contacts-unknown-kind`).

### Tests for User Story 1

- [x] T012 [P] [US1] Service tests in `src-tauri/src/contacts/service/contacts_tests.rs`: create with name, two phones (`mobile`, `home`), one email and a `custom` value "Kundennummer: 4711" → origin `holzi`, this device, caller `user`; create without any of "Namensteil, Organisation, Telefonnummer oder E-Mail-Adresse" is refused; `update` with a patch that changes one phone touches only that row (assert the other rows keep their HLC); a row with an unknown kind (inserted directly) survives an update of the contact and appears read-only in `get` (FR-007); `upsert`/`remove` of an unknown-kind row → `UnknownKindReadOnly`; no patch can change `origin_*` (FR-018a); delete cascades; `note` over 64 KiB is refused
- [x] T013 [P] [US1] Search tests in `src-tauri/src/contacts/search_tests.rs` reading shared cases from `src/lib/contacts/search-cases.json`: case-insensitive match on display name, name parts, organization, phone, email; phone numbers found without spaces, brackets and hyphens ("0170 123-45 67" by "01701234567" and by "123 45"); NFKC ("ﬁ" ↔ "fi") **Note**: normalization is `passwords::ids::fold_for_search` in Rust and `src/lib/search/fold.ts` in the window (accents dropped, lower case), not NFKC
- [x] T014 [P] [US1] Photo tests in `src-tauri/src/contacts/photos_tests.rs`: a 3000 × 2000 PNG becomes a JPEG of at most 512 × 512 and at most 200 KiB; the same image twice is stored once (hash PK); a photo no longer referenced gets `orphaned_at` and is deleted after 7 days; a non-image is refused
- [x] T015 [P] [US1] Group tests in `src-tauri/src/contacts/service/groups_tests.rs`: create, rename, add and remove members (member id = UUIDv5(group, contact)), adding twice is idempotent, deleting a group keeps its contacts
- [x] T016 [P] [US1] Frontend tests in `src/lib/contacts/search.test.ts` (same `search-cases.json`) and `src/lib/contacts/kinds.test.ts` (label texts, which kinds the editor offers, unknown kinds read-only) **Note**: holzi has no Vitest; the frontend checks are `scripts/check-contacts-search.ts`, `scripts/check-contacts-kinds.ts` and `scripts/check-contacts-draft.ts` under `pnpm check:contacts` (also in CI)

### Implementation for User Story 1

- [x] T017 [US1] Search normalization in `src-tauri/src/contacts/search.rs`: `haystack(contact)` (lowercase NFKC of display name, name parts, organization, emails, phones plus phones as digits only) and `matches(haystack, query)` (query normalized the same way, digits-only query also matched against digits) (research R11)
- [x] T018 [US1] Photos in `src-tauri/src/contacts/photos.rs`: decode with `image`, downscale with `fast_image_resize` like `src-tauri/src/files/thumbnails.rs`, encode JPEG, "≤ 512 × 512, ≤ 200 KiB" (lower quality step by step until it fits), store in `haex_contacts_binaries`, orphan marking and a cleanup run on vault open (7 days, like `ORPHAN_GRACE_DAYS`) (research R10) **Note**: the clean-up runs from `src-tauri/src/contacts/maintenance.rs` after open and import
- [x] T019 [US1] Contacts in `src-tauri/src/contacts/service/contacts.rs`: `list` (headers), `get`, `create(caller, input, targets)` (targets ignored until US3, origin from caller and this device), `update(caller, id, patch)` writing only changed head columns and only the value rows in `upsert`/`remove` (never delete-and-reinsert, research R3), `delete(caller, ids)`, `set_photo`; every method calls `contacts::access` first; derive `display_name` from name parts when empty
- [x] T020 [P] [US1] Groups in `src-tauri/src/contacts/service/groups.rs`: `groups`, `group_save`, `group_members`
- [x] T021 [US1] Commands in `src-tauri/src/contacts/commands/contacts.rs` per contracts/tauri-commands.md "Kontakte" (`contacts_list`, `contacts_get`, `contacts_create`, `contacts_update`, `contacts_delete`, `contacts_set_photo`, `contacts_photo`, `contacts_groups`, `contacts_group_save`, `contacts_group_members`), caller fixed to `Caller::User`; register in `src-tauri/src/lib.rs`; errors `UnknownKindReadOnly` and validation errors as `HolziError` kinds in `src-tauri/src/error.rs`; regenerate ts-rs bindings
- [x] T022 [P] [US1] Frontend logic: `src/lib/contacts/kinds.ts` (kinds, labels, which go to the phone), `src/lib/contacts/search.ts` (same rule as T017), `src/lib/contacts/registry.ts` (`CONTACTS_LOCATIONS`: list, entry by opaque id, editor, like `src/lib/passwords/registry.ts`), `src/composables/useContacts.ts` (invoke wrappers), `src/stores/contacts.ts` (headers, reload on `vault-data-changed` for `haex_contacts*` via the `onVaultTablesChanged` pattern)
- [x] T023 [US1] App frame `src/components/apps/ContactsApp.vue` and views in `src/components/contacts/`: `List.vue` (alphabetical by display name, search field, multi-select), `ListItem.vue`, `EntryPage.vue` (all values grouped by kind, unknown kinds read-only with label and value, origin line "in holzi angelegt auf <Gerät>" / "vom Telefon <Gerät>, <Konto>" / "aus Visitenkarte"), `EntryEditor.vue` (head fields, add/remove values of every known kind incl. `custom`, photo pick via `tauri-plugin-dialog`, confirm delete), `Toolbar.vue`, `Sidebar.vue` (groups); reuse haex-ui inputs and holzi's `SettingsGroup`/`SettingsRow` as in passwords (COSMIC/GNOME style: toolbar row, big title, boxed lists, hideable sidebar) **Note**: multi-select in the list comes with US3 (bulk sync targets, T056); the editor sends only what changed through `src/lib/contacts/draft.ts`
- [x] T024 [US1] Open actions in `src/components/contacts/ValueActions.vue`: copy every value; on a phone (`useDeviceCapabilities`) open `tel:`, `sms:`, `mailto:`, `geo:0,0?q=` via `tauri-plugin-opener` (FR-006) **Note**: the opener capability got a scope for `tel:`, `sms:`, `mailto:`, `geo:`, `http(s):` in `src-tauri/capabilities/default.json`
- [x] T025 [US1] WM app: add `{ id: 'system.contacts', titleKey: 'wm.apps.contacts', icon: 'lucide:contact', multiInstance: true, tabTitle: 'app', defaultSize, minSize }` to `WM_APPS` in `src/lib/wm/apps.ts` after `system.passwords`; `contactsRoutes()` in `src/components/wm/appRoutes.ts`
- [x] T026 [US1] Settings category: `contacts` in the `SettingsCategoryId` union and `category('contacts', '/contacts', 'lucide:contact')` in `SETTINGS_CATEGORIES` in `src/lib/settings/registry.ts`, `categoryLocation('contacts')` in `SETTINGS_LOCATIONS`; `src/components/settings/ContactsView.vue` with a link "Kontakte öffnen"; the phone, card and blocked sections arrive with US2/US4 (FR-015a); route in `src/components/wm/appRoutes.ts`
- [x] T027 [P] [US1] i18n in `src/i18n/locales/de.json` and `src/i18n/locales/en.json`: `wm.apps.contacts`, `contacts.*` (list, editor, kinds, labels, origin, errors), `settings.categories.contacts.title`; no `<…>` in messages (vue-i18n refuses them)
- [x] T028 [US1] E2E scenarios `scripts/e2e/scenarios/contacts-basic.test.ts` (US1-1/2/3/5: create with two phones, email, custom value; search "ann"; copy), `scripts/e2e/scenarios/contacts-two-devices.test.ts` (US1-4: change and delete arrive on the second device), `scripts/e2e/scenarios/contacts-unknown-kind.test.ts` (US1-6: insert a row with `kind = 'future_kind'` through the test SQL helper, edit the contact, the row is still there and shown) **Note**: `contacts-unknown-kind` is not an E2E scenario: no command writes a value of an unknown kind; the Rust test `values_of_unknown_kinds_survive_and_stay_read_only` and `scripts/check-contacts-draft.ts` cover it
- [x] T029 [US1] Run `cargo fmt --check`, `pnpm lint:rust`, `cargo test -j 4 --lib -- contacts caller passwords identity::migrations`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm vitest run src/lib/contacts`, the three E2E scenarios plus `passwords-basic` (caller move); commit, open PR B **Note**: E2E `contacts-basic`, `contacts-two-devices`, `passwords-basic`, `settings-categories`, `settings-search` passed locally; `settings-categories` lists the new category

**Checkpoint**: The address book works on every platform; the MVP is usable.

---

## Phase 4: User Story 2 - Telefonkontakte übernehmen und abgleichen (Priority: P1) (PR C)

**Goal**: On Android the sync is switched on, the first run shows an overview and merges, then phone and holzi stay in sync both ways for contacts that came from the phone.

**Independent Test**: Emulator with prepared contacts in two accounts; switch on, compare, then change, create and delete on both sides (quickstart: `android-contacts-import`, `android-contacts-denied`, `android-contacts-two-way` parts US2, `android-contacts-mass-delete`).

### Tests for User Story 2

- [x] T030 [P] [US2] `FakePhoneBook` in `src-tauri/src/contacts/phone/test_support.rs`: in-memory raw contacts with id, version (incremented on every change), deleted flag, account, source id, lookup key (aggregation), read-only flag, data rows per mimetype; methods to edit "as the user" and to remove an account; records every op it receives — in `test_support.rs`; the run tests use `test_rig.rs` (`Rig`: vault, fake phone, device)
- [x] T031 [P] [US2] Index tests in `src-tauri/src/contacts/phone/index_tests.rs`: new id, higher version, missing id, `DELETED=1`, an own write whose returned version is stored is not seen as a phone change (research R6)
- [x] T032 [P] [US2] Mapping tests in `src-tauri/src/contacts/phone/mapping_tests.rs`: every mimetype of contracts/android-plugin.md ↔ values of data-model.md and back without loss (SC-004); raw contacts with the same lookup key become one holzi contact with the union of values (equal values once, research R7); `Im` read with `PROTOCOL_CUSTOM`; unknown mimetypes are not read — the union of raw contacts with one lookup key is covered by the run test `raw_contacts_of_one_person_become_one_contact_with_every_value_once` (`mod_tests.rs`); phone numbers in other notations (`+49 170 …`, `0170 …`) count as the same value, so a merged contact has the number once
- [x] T033 [P] [US2] Merge tests in `src-tauri/src/contacts/phone/merge_tests.rs` (research R8): "Anna Beck" + "+49 170 1234567" vs "Beck Anna" + "0170 1234567" → clear; same name only → uncertain; same email only → uncertain; two candidates → uncertain; uncertain default "getrennt lassen"; a merged contact keeps the older origin (FR-018a) and gets this phone as target (FR-018b) — key matching in `merge_tests.rs`, pairs and decisions in `matching_tests.rs`, origin and target after a merge in `mod_tests.rs`
- [x] T034 [P] [US2] Reconcile tests in `src-tauri/src/contacts/phone/reconcile_tests.rs`: phone changed field X, holzi changed field Y → both kept; same field changed on both → the later change wins (research R6): phone `lastUpdated` later than the HLC time of the holzi value → phone value imported as a change of this device; holzi later, equal or no timestamp → holzi value written to the phone (spec edge case "Gleichzeitige Änderung"); phone deleted, holzi changed → deletion wins (024 FR-017); holzi-only kinds (`custom`, `holzi_*`, unknown) never written to the phone and never removed by a phone change (FR-014); a field the phone has but holzi does not know stays untouched (FR-022); base updated after every applied change
- [x] T035 [P] [US2] Guard tests in `src-tauri/src/contacts/phone/guard_tests.rs`: 11 deletions out of 100 linked → paused `mass_delete`; 10 out of 50 → allowed; 11 out of 200 → allowed; all raw contacts of one account gone and the account missing from `contactsAccounts` → links dropped, no deletion (FR-026); permission missing → paused `permission`, nothing changed; decision "keep" re-creates on the phone / keeps in holzi (FR-016) — the decisions run in `run_tests.rs` against the fake phone
- [x] T036 [P] [US2] Run tests in `src-tauri/src/contacts/phone/mod_tests.rs`: state machine of data-model.md (no row while `contacts.phone.enabled` is off → `review` → `active`, `paused`; switching off deletes the run row and keeps links; re-enable goes to `review` and creates no duplicates); paging uses `limit` ≤ 200 for `contactsRead`; the 2.000-contact time budget is checked in T095 — plus `run_tests.rs` for later runs; the run reads 200 raw contacts per call and pages the index by 500 (`many_contacts_go_in_pages`)

### Implementation for User Story 2

- [x] T037 [US2] Android permission and manifest: in `src-tauri/plugins/holzi-android/android/src/main/AndroidManifest.xml` add `READ_CONTACTS`, `WRITE_CONTACTS`, `GET_ACCOUNTS` with `android:maxSdkVersion="22"`; in `HolziAndroidPlugin.kt` add a `Permission(strings = [Manifest.permission.READ_CONTACTS, Manifest.permission.WRITE_CONTACTS], alias = "contacts")`; `Permission::Contacts` with alias `contacts` in `src-tauri/plugins/holzi-android/src/lib.rs`
- [x] T038 [US2] Kotlin reader in a new `src-tauri/plugins/holzi-android/android/src/main/java/space/haex/holzi/android/ContactsProvider.kt` and `@Command`s in `HolziAndroidPlugin.kt`: `contactsAccounts`, `contactsIndex {offset, limit}` (incl. `lastUpdated` = `CONTACT_LAST_UPDATED_TIMESTAMP`), `contactsRead {ids}` (≤ 200), `contactsPhotoRead {id}`, `contactsGroups` exactly as contracts/android-plugin.md; all queries on `Dispatchers.IO`; answers ≤ 1 MiB — `contactsIndex {afterId, limit}` pages by id instead of an offset; the queries run on an `Executor` of the plugin (no coroutines dependency); `contactsGroups` and `contactsPhotoWrite` come with US3 (T054, T096)
- [x] T039 [US2] Kotlin writer in `ContactsProvider.kt`: `contactsApply {ops}` (≤ 400 ops, `applyBatch` with `withYieldAllowed` every 100, back references for `ref`, then read back `VERSION` of every touched raw contact), `contactsPhotoWrite` outside the batch; `OperationApplicationException` → `{error}` without personal data — without `contactsPhotoWrite` (T054)
- [x] T040 [US2] Rust bindings in `src-tauri/plugins/holzi-android/src/android.rs` (`run_mobile_plugin` per command, serde structs) and no-op stand-ins in `src-tauri/plugins/holzi-android/src/other.rs` returning an "unavailable" error
- [x] T041 [US2] Errors and capability: `HolziError::ContactsDenied` and `PhoneContactsUnavailable` in `src-tauri/src/error.rs`; `phone_contacts: bool` in `PlatformCapabilities` (`src-tauri/src/platform/mod.rs`: false in `desktop()`, true in `ANDROID`, false in `IOS` via override), update `src-tauri/src/platform/mod_tests.rs`, regenerate bindings — plus `PhoneContactsFailed` for a refused call
- [x] T042 [US2] `PhoneBook` trait and run in `src-tauri/src/contacts/phone/mod.rs` (on/off only from the pref `contacts.phone.enabled`, target account from `contacts.phone.target_account`, both `PrefScope::Device`; run state `review | active | paused` in `haex_contacts_phone_runs`, no row while off), Android implementation in `src-tauri/src/contacts/phone/android.rs` (permission request like `voice.rs::ensure_microphone` in `spawn_blocking`, anything but `Granted` → `ContactsDenied`)
- [x] T043 [US2] Index in `src-tauri/src/contacts/phone/index.rs`, mapping in `src-tauri/src/contacts/phone/mapping.rs`, merge in `src-tauri/src/contacts/phone/merge.rs`, reconcile (both directions for linked contacts) in `src-tauri/src/contacts/phone/reconcile.rs`, op building in `src-tauri/src/contacts/phone/write.rs` (updates and deletes for linked raw contacts; a changed value goes to the raw contact that holds it, research R7), guard in `src-tauri/src/contacts/phone/guard.rs`; imported contacts get origin `phone` with device and account and a target row for this phone and account (FR-018b) — also `apply.rs` (second half of a run), `gather.rs`, `jobs.rs`, `matching.rs`, `head.rs`, `sides.rs`, `labels.rs`, `store.rs`; holzi deletions need the device-local list `haex_contacts_phone_known_no_sync` (migration `0034`, data-model.md), because a contact's links go with it
- [x] T044 [US2] Triggers: run after vault open on Android, on `RunEvent::Resumed` (a new `Wake` variant or `Notify` in `src-tauri/src/sync/resume.rs`, wired at `src-tauri/src/lib.rs:606`), debounced to one run at a time; nothing on `Suspended` — `RunEvent::Resumed` and the open start `contacts::phone::trigger::start` directly; a lock with an "again" flag collapses starts into one run
- [x] T045 [US2] Commands in `src-tauri/src/contacts/commands/phone.rs`: `contacts_phone_enable`, `_disable`, `_accounts`, `_set_target`, `_review`, `_confirm`, `_status`, `_mass_delete`, `_run` per contracts/tauri-commands.md; events `contacts-phone-status` and `contacts-phone-mass-delete`; register in `src-tauri/src/lib.rs`; "another vault on this phone already syncs" check for FR-027 via a marker in the app's private storage listing the vault ids that enabled the sync
- [x] T046 [US2] Settings UI in `src/components/settings/ContactsView.vue` (shown with `capabilities.phoneContacts`, otherwise "Auf diesem Gerät nicht verfügbar"): switch "Mit Telefon-Adressbuch abgleichen" (saves on selection), target account select (writable accounts, default suggested), last run and counts, hint for FR-027; `src/components/contacts/PhoneReviewDialog.vue` (counts, list of uncertain pairs with "zusammenführen" / "getrennt lassen", default "getrennt lassen", confirm); `src/components/contacts/MassDeleteDialog.vue` (counts, "löschen" / "behalten"); permission refused explanation like the microphone one; `src/composables/useContactsPhone.ts` — the switch, target, state and issues live in `src/components/settings/ContactsPhoneSetting.vue`, which `ContactsView.vue` shows
- [x] T047 [P] [US2] i18n for the phone sync in `src/i18n/locales/de.json` and `en.json` (`settings.contacts.phone.*`, `contacts.review.*`, `contacts.massDelete.*`, `contacts.error.contactsDenied`)
- [x] T048 [US2] E2E helpers: `allowContacts()`, `refuseContacts()`, `insertContact({account, name, phones, emails})`, `queryContacts()`, `updateContact(id, …)`, `deleteContact(id)`, `removeTestContacts()` in `PhoneControls` (`scripts/e2e/lib/instance.ts`) and `phoneControls(adb)` in `scripts/e2e/lib/platform/android.ts` via `adb shell content insert|query|update|delete --uri content://com.android.contacts/…`; grant/refuse like `allowNotifications`/`refuseMicrophone` (`pm grant`, `pm set-permission-flags … user-set user-fixed`, never revoke); unit test with the fake runner in `scripts/e2e/lib/platform/android.test.ts` — in `scripts/e2e/lib/platform/android-contacts.ts` (`android.ts` is at the 500-line limit), with `updatePhone` instead of a general update and `revokeContacts` for a permission taken back; unit test `android-contacts.test.ts`
- [x] T049 [US2] E2E scenarios with `needs: { phone: true }`: `scripts/e2e/scenarios/android-contacts-import.test.ts` (US2-1/2, two accounts: local and a test account type), `android-contacts-denied.test.ts` (US2-6), `android-contacts-two-way.test.ts` (US2-3/4/5/7), `android-contacts-mass-delete.test.ts` (FR-016 with > 10 % deleted via adb, permission refused after enabling); every scenario removes its raw contacts at the end (`pm clear` does not) — the two-way scenario starts runs with `contacts_phone_run`; coming back to the foreground is not driven by the E2E (the Resumed start is the same function as after opening). The permission taken back after enabling is part of `android-contacts-denied`
- [x] T050 [US2] Run `cargo test -j 4 --lib -- contacts platform`, the Android clippy targets, `pnpm typecheck`/`lint`/`format:check`, `pnpm check:e2e-exclusions`, the four Android scenarios on `holzi-emulator`; commit, open PR C

**Checkpoint**: Phone contacts are in the vault and stay in sync both ways.

---

## Phase 5: User Story 3 - Änderungen von anderen Geräten landen im Telefon (Priority: P1) (PR D)

**Goal**: Sync targets per contact; new contacts reach a phone only when targeted; changes from other devices are written on the next run; failures are visible.

**Independent Test**: Desktop + emulator; on the desktop change a phone contact and create two contacts (with and without target), remove a target, set targets for many; check the phone (quickstart: `android-contacts-two-way` US3 parts, `android-contacts-targets`).

### Tests for User Story 3

- [x] T051 [P] [US3] Target tests in `src-tauri/src/contacts/service/targets_tests.rs`: create with "Nur in holzi" → no target (FR-018b); create with a phone → one target with the phone's target account; set for 40 selected and for a group; only phones with sync on are offered (`contacts_phones`); remove with `delete` vs `keep` (FR-018d); a target on a phone with sync off stays and is not offered for new targets
- [x] T052 [P] [US3] Write tests in `src-tauri/src/contacts/phone/write_tests.rs` with `FakePhoneBook`: a contact newly targeted is inserted in the target's account with all phone kinds; a contact without a target on this phone is never written (FR-021); a change to a contact in a company account goes there, not to the target account; read-only raw contact → issue `read_only`, holzi value kept (FR-025); a later successful write clears the issue; removed account → issue `account_removed`; photo written only when the holzi photo hash differs from `photo_base` (research R10)

### Implementation for User Story 3

- [x] T053 [US3] Targets in `src-tauri/src/contacts/service/targets.rs` and commands `contacts_phones`, `contacts_targets_set`, `contacts_targets_remove` in `src-tauri/src/contacts/commands/targets.rs`; `contacts_create` now writes the targets from its input (FR-018c)
- [x] T054 [US3] Inserts for new targets, deletes for removed targets with `delete`, unlinking for `keep`, issues in `haex_contacts_phone_issues` in `src-tauri/src/contacts/phone/write.rs` and `reconcile.rs` — a target removed with `delete` keeps the links, and the phone deletes the raw contacts it finds linked without a target (not held back by the guard: the user decided on exactly these); `keep` drops the links in the same transaction (`contacts/targets.rs`), and a target set again merges with the raw contacts the phone kept (known list); photos go to the phone in `phone/photo_write.rs` (hash against `photo_base`; a photo removed in holzi stays on the phone)
- [x] T055 [US3] Trigger on incoming changes: subscribe to vault table changes of `haex_contacts*` (the `vault-data-changed` / `onVaultTablesChanged` pattern on the Rust side) and start a debounced run on the phone (SC-003) — `trigger::watch` follows `AppState::vault_changes` and leaves out the rows a run writes about itself (`haex_contacts_phone_*`), so a run does not start the next; 500 ms settle time. Such a run (`Mode::Changes`) takes no deletions over from the phone: a phone deleting contacts one by one while holzi is open would otherwise be followed run by run, each below the guard; the next full run (open, foreground, by hand) sees them all at once
- [x] T056 [US3] UI: target question in `EntryEditor.vue` when creating (phones with sync on, each with its suggested account, preselected "Nur in holzi"; question absent when no phone has sync on); target section in `EntryPage.vue` (list, add, remove with the delete/keep question); bulk "Aufs Telefon …" in `List.vue` multi-select and in the group sidebar; issue badge and text on the contact; issue list in `ContactsView.vue` — the list got a selection mode (`SelectionBar.vue`) with "Aufs Telefon …" and "Vom Telefon nehmen …"; the targets section is `TargetsSection.vue` with `TargetRemoveDialog.vue`; the issue list of this phone stays in the settings (PR C), every device shows the issues per contact
- [x] T057 [P] [US3] i18n for targets and issues in `src/i18n/locales/de.json` and `en.json`
- [x] T058 [US3] E2E: extend `android-contacts-two-way.test.ts` with a desktop instance (US3-1/2/3/6) and add `scripts/e2e/scenarios/android-contacts-targets.test.ts` (US3-4/5, mixed group: phone host + Linux host like spec 043 stage 2) — the desktop parts are a scenario of their own, `android-contacts-desktop.test.ts` (mixed group, US3-1/2/4/6, editor and remove dialog on the laptop), so `android-contacts-two-way` stays a one-device scenario; `android-contacts-targets` runs on the phone alone (selection in the list, a group, keep and delete); US3-3 (company account) only in the Rust tests: the emulator has no writable account besides the local one
- [x] T096 [US3] Groups between phone and holzi: `contactsGroups`, `GroupMembership` in `mapping.rs` (phone group title ↔ holzi group by name, per account), `insertGroup`/`addToGroup` ops; moved from T032/T038 in PR C — a membership travels as the group's title: `contactsRead` names the group of each `GroupMembership` row and marks the system's own groups read-only (never mapped); holzi compares memberships as values of the kind `group` by name; groups an account lacks are made with `insertGroup` in a call of their own before the batch, and a membership is an `insertData` with the group id (no `addToGroup` op); joining, leaving, renaming or deleting a group stamps its contacts
- [ ] T059 [US3] Run the checks of T050 plus `contacts-two-devices`; commit, open PR D

**Checkpoint**: holzi is the leading address book; the phone follows the targets.

---

## Phase 6: User Story 4 - Visitenkarten austauschen (Priority: P2) (PR E)

**Goal**: Each vault shows a signed card; reading a card in gives a contact confirmed holzi addresses; newer cards replace older ones; blocked vaults can be managed.

**Independent Test**: Two vaults; one shows its card as link, the other reads it in as new contact and into an existing contact; a modified and an older card are refused (quickstart: `contacts-card`, M7).

### Tests for User Story 4

- [x] T060 [P] [US4] Format tests in `src-tauri/src/contacts/card/format_tests.rs` (contracts/contact-card.md): round trip; wrong prefix, bad base64url, trailing bytes, `version != 1` → invalid; one flipped bit in payload or signature → `CardInvalid`; more than 10 relays or 32 devices or a string over 1 KiB → invalid; non-`ws(s)://` relay → invalid; text longer than 2.048 bytes is never produced (values dropped from the end, `omitted` lists them; too large without values → `CardTooLarge`)
- [x] T061 [P] [US4] Card service tests in `src-tauri/src/contacts/service/cards_tests.rs`: accept as new contact (origin `card`, no target); accept into an existing contact; own card → `CardOwn`; older `issued_at` → `CardOlder`, nothing changed; newer replaces devices and relays (FR-012); the same vault pubkey cannot sit on two contacts (PK, FR-013: accepting offers the existing contact); block/unblock
- [x] T062 [P] [US4] Own card tests in `src-tauri/src/contacts/card/own_card_tests.rs`: only a main device signs (`vault_secret()` is `Some`); a linked device returns the newest synced card and `can_sign = false`; refresh after a device list change, a Nostr relay change and a change of the chosen values

### Implementation for User Story 4

- [x] T063 [US4] `Domain::ContactCard` with tag `holzi-contact-card/v1` in `src-tauri/src/sync/signing.rs`; card struct, text encoding and checks in `src-tauri/src/contacts/card/format.rs`; QR as SVG with error correction M like `LinkCode::qr_svg` in `src-tauri/src/sync/link/code.rs`
- [x] T064 [US4] Own card in `src-tauri/src/contacts/card/own_card.rs`: build from `device_list::effective()` (device pubkey, endpoint id, iroh relay from presence when known), `ServerConfig::effective_nostr_relays()`, vault display name and the chosen values (prefs `contacts.card.own_contact`, `contacts.card.values`, vault scope); store in `haex_contacts_own_card`; call `own_card::refresh` after each of the five list-signing places (`sync/genesis.rs`, `sync/link/host.rs`, `sync/admission_decision.rs` twice, `sync/removal.rs`; grep `sign_list(` again on the PR base), after `ServerConfig::write` and after `contacts_own_card_set` — `own_card::refresh_quietly` follows every `device_list::insert` outside tests (seven places: genesis, link host completion, join, handshake, removal, admission twice; it does nothing without the vault secret) and `servers::write`; the card's name is the own contact's name, holzi has no display name of a vault
- [x] T065 [US4] Card service in `src-tauri/src/contacts/service/cards.rs` and blocked vaults; commands `contacts_own_card`, `contacts_own_card_set`, `contacts_card_preview`, `contacts_card_accept`, `contacts_blocked`, `contacts_block`, `contacts_unblock` in `src-tauri/src/contacts/commands/cards.rs`
- [x] T066 [US4] QR scanner: `tauri-plugin-barcode-scanner` (newest published 2.x, exact version in `Cargo.lock`) under `[target.'cfg(any(target_os = "android", target_os = "ios"))'.dependencies]` in `src-tauri/Cargo.toml`, registered in `src-tauri/src/lib.rs` under `cfg(mobile)`, permission `barcode-scanner:default` in a mobile-only capability file under `src-tauri/capabilities/`; `@tauri-apps/plugin-barcode-scanner` in `package.json`; check that the Android build and the Android E2E still pass (camera permission comes from the plugin)
- [x] T067 [US4] UI: `src/components/contacts/OwnCard.vue` (QR, copy link, issued at, hint on linked devices without card), `src/components/contacts/CardImport.vue` (paste link; on phones a "Scannen" button; preview with name, npub, number of devices, values; "Neuer Kontakt" / "Zu Kontakt hinzufügen …"), confirmed holzi addresses in `EntryPage.vue` (devices, relays, copy and QR per value, FR-009) — the own card and the import are places of the app (`card`, `card/import`) behind a card menu in the toolbar; confirmed addresses offer copy, no QR per value (the card's QR carries them all); the preview can block the vault, unconfirmed manual values marked "unbestätigt"; card values and blocked vaults sections in `src/components/settings/ContactsView.vue`; toolbar entries "Meine Visitenkarte" and "Visitenkarte einlesen"
- [x] T068 [P] [US4] i18n for cards and blocking in `src/i18n/locales/de.json` and `en.json`
- [x] T069 [US4] E2E `scripts/e2e/scenarios/contacts-card.test.ts` with two desktop instances of different vaults (US4-1 to US4-6; the modified card by flipping one character of the link) — US4-1 to US4-5; US4-6 (the phone keeps holzi addresses) is the rule FR-014 of the phone sync with its own tests
- [x] T070 [US4] Run `cargo test -j 4 --lib -- contacts sync::signing sync::device_list`, frontend checks, `contacts-card`, the Android build; commit, open PR E

**Checkpoint**: Contacts can carry confirmed holzi addresses.

---

## Phase 7: User Story 5 - Erweiterungen lesen Kontakte (Priority: P2) (PR F)

**Goal**: Extensions read contacts through the permission `contacts` (read, target `*` or group), never write.

**Independent Test**: The probe extension queries contacts without, with and with a group-only permission (quickstart: `extension-contacts`).

### Tests for User Story 5

- [x] T071 [P] [US5] Permission tests in `src-tauri/src/extensions/permissions/model_tests.rs` (or the existing tests file): `contacts` parses, only action `read` is allowed, `write` is refused at manifest load — in `target_tests.rs` and `manifest_map_tests.rs`; a write is listed as an unreadable declaration and never becomes a permission
- [x] T072 [P] [US5] Bridge tests in `src-tauri/src/extensions/contacts_tests.rs`: no permission → 1002 without revealing existence; state "fragen" → 1004 with `{resourceType: "contacts", action, target}`; `*` → all; group "Arbeit" → only members; denied `*` beats a granted group; `ContactDetail` contains unknown kinds and confirmed holzi addresses but no origin, targets or links; photo bytes only for a readable contact

### Implementation for User Story 5

- [x] T073 [US5] `PermissionKind::Contacts` in `src-tauri/src/extensions/permissions/model.rs` (`ALL` array, `as_str`/`parse`, allowed actions `read` only), `src-tauri/src/extensions/permissions/manifest_map.rs`, target parsing in `src-tauri/src/extensions/permissions/target.rs` (`*` → `Any`, else group name as `Tag`)
- [x] T074 [US5] Handler `src-tauri/src/extensions/contacts.rs` after `src-tauri/src/extensions/passwords.rs` (`permissions`, `grants_of`, `refused`, caller `Caller::Extension{id}`) and methods `extension_contacts_list`, `extension_contacts_read`, `extension_contacts_search`, `extension_contacts_photo` in `src-tauri/src/extensions/bridge/dispatch.rs` — the method table moved to `bridge/allowlist.rs` (dispatch.rs would have passed 500 lines); the service got `search` and lets holzi's own functions read the groups to turn group names into grants
- [x] T075 [US5] Install and prompt texts for the new kind in `src/i18n/locales/de.json` and `en.json` and wherever permission kinds are labeled (grep `remoteStorage` in `src/components/extensions/`)
- [ ] T076 [US5] vault-sdk: `src/api/contacts.ts`, `src/commands/contacts.ts`, export in `src/index.ts` in `https://github.com/haex-space/vault-sdk` (push as `haex-space`); after merge, bump the dependency in holzi and note the full commit SHA in the PR
- [x] T077 [US5] Probe extension: add contacts calls to `src-tauri/tests/fixtures/extension_e2e/probe.js`, declare `contacts` in its manifest and rebuild `probe.xt` per `SOURCE.md`; E2E `scripts/e2e/scenarios/extension-contacts.test.ts` (US5-1/2/3) — the probe page already sends any bridge method, so only its manifest (built in `src-tauri/tests/extension_e2e_fixtures.rs`, `HOLZI_WRITE_E2E_FIXTURES=1`) declares `contacts` for the group "Arbeit"
- [x] T078 [US5] Run `cargo test -j 4 --lib -- extensions contacts`, frontend checks, `extension-contacts`, `extension-permission-prompt`; commit, open PR F

**Checkpoint**: Extensions such as haex-mail can suggest recipients.

---

## Phase 8: User Story 6 - Agenten fragen Kontakte ab (Priority: P3) (PR G)

**Goal**: The built-in agent reads and (with grant) changes contacts after an ask-and-remember prompt; the actions are ready for spec 021.

**Independent Test**: Call the contact actions through the built-in agent with and without grant (quickstart: `agent-contacts`).

### Tests for User Story 6

- [x] T079 [P] [US6] Grant tests in `src-tauri/src/files/permissions_tests.rs` (or a new `src-tauri/src/contacts/agent_tests.rs`): kind `contacts`, target `*` or `group:<id>`, read vs write, remembered answer, revoke — in `src-tauri/src/contacts/agent_tests.rs`
- [x] T080 [P] [US6] Agent command tests in `src-tauri/src/contacts/commands/agent_tests.rs`: without grant the command parks on the prompt; refused → same error as a missing contact (FR-031); read grant cannot update; write grant update reaches the phone targets like a user change — in `src-tauri/src/contacts/agent_tests.rs` against `ContactsAgent` (the commands are thin): no window or no answer refuses and keeps nothing; "Einmal erlauben" holds for the call; a denied write keeps the read grant; agents never change groups

### Implementation for User Story 6

- [x] T081 [US6] Agent grants of kind `contacts` in `agent_file_permissions` (research R15): extend `src-tauri/src/files/permissions.rs` and `src-tauri/src/files/agent/prompt.rs` (event `files-agent-permission-request` with `kind: "contacts"`), and `src/components/files/AgentPermissionDialog.vue` / `src/lib/files/agentPermissions.ts` to show the contacts wording — the question got `kind: "storage" | "contacts"`; the dialog shows "Ablehnen", "Einmal erlauben", "Immer erlauben" for the contacts
- [x] T082 [US6] Commands `contacts_agent_search`, `contacts_agent_get`, `contacts_agent_update` in `src-tauri/src/contacts/commands/agent.rs` with `Caller::BuiltinAgent` fixed in the body — the logic lives in `src-tauri/src/contacts/agent.rs` (`ContactsAgent`); what agents and extensions see is one view, `src-tauri/src/contacts/outside.rs`
- [x] T083 [US6] Scopes `contacts.read` and `contacts.write` in `src/lib/actions/scopes.ts` (with descriptions and `actions.scopes.*` title keys); actions `contacts.search`, `contacts.get` (read) and `contacts.update` (write) in a new `src/lib/actions/contactsActions.ts`, registered in `src/lib/actions/catalog.ts`; handlers in `src/stores/contactsActionHandlers.ts`; contacts in `src/components/settings/AgentFilesSetting.vue` (or a sibling view) to see and revoke remembered answers — in `AgentFilesSetting.vue`: a row "Kontakte" ("Fragt nach" revokes) and a revoke per remembered group
- [x] T084 [US6] E2E `scripts/e2e/scenarios/agent-contacts.test.ts` (US6-1/2/3 through the built-in agent's stand-in model like `files-agent`)
- [ ] T085 [US6] Run `cargo test -j 4 --lib -- contacts files::permissions files::agent`, frontend checks, `agent-contacts`, `files-agent`; commit, open PR G

**Checkpoint**: Agents get contacts only with a remembered grant.

---

## Phase 9: User Story 7 - Aus dem Adressbuch einladen (Priority: P3) (PR H, with spec 027/028)

**Goal**: Invite a contact with a confirmed card to a space or data share; deliver over Nostr; unknown senders separate and blockable.

**Gate**: Start only when the first PR of spec 027 or 028 with invitation links is merged (phasing rule). Re-check this phase against their plans first.

- [ ] T086 [US7] Before starting: compare contracts/nostr-invite.md with the plans of 027/028; record the tables for outgoing and incoming invitations there, adding `recipient_contact_id` and `delivery_state` (`pending | delivered | undeliverable`) (data-model.md "Später mit 027/028")
- [ ] T087 [P] [US7] Tests for the gift-wrap layers in `src-tauri/src/sync/presence_tests.rs` (or a new `src-tauri/src/contacts/invite/wire_tests.rs`): kind 1059 with `p` tag and NIP-40 `expiration`, seal by the sender device key, rumor kind 24110, size ≤ 16 KiB, a seal key not on `senderCard` is dropped, a blocked sender is dropped without acknowledgement
- [ ] T088 [US7] Sending and receiving in `src-tauri/src/contacts/invite/` (reuse `presence::wrap_payload`/`unwrap_rumor` extended for the stored kind; subscription `{"kinds":[1059], "#p":[own device key]}` on the own Nostr relays; acknowledgement rumor 24111)
- [ ] T089 [US7] UI: "Aus dem Adressbuch" in the invite dialogs of 027/028 (only contacts with a confirmed card), delivery state, "Von Unbekannten" list without notification with "Absender blockieren"
- [ ] T090 [US7] E2E with two vaults and the local Nostr relay (`e2e_nostr_relay`), recipient offline at send time (US7-1 to US7-6)

---

## Phase 10: Polish & Cross-Cutting Concerns

- [x] T091 [P] Update `CONTEXT.md` with the terms Adressbuch, Kontakt, Angabe, Herkunft, Abgleichsziel, Visitenkarte, holzi-Adressen (and the note that haex-vault "Kontakte" were sharing identities)
- [x] T092 [P] Update the 051 row in `plans/README.md` after each merged PR
- [ ] T093 Manual checks M1–M7 of quickstart.md on real phones (operator); record results in the PR of the last story
- [x] T094 Keep each new source file under 500 lines; split `contacts/phone/` or the Vue views further if one grows — every file of 051 is below 500 lines (largest `contacts/store.rs` 492, `contacts/phone/apply.rs` 474); `src/lib/actions/settingsActions.ts`, over 500 before 051, gave its agent permission actions to `settingsAgentActions.ts`
- [x] T095 [P] Add deterministic acceptance checks for the measurable scale and latency criteria: a release-like fake-phone run with 2,000 contacts completes in under 60 seconds, search over 5,000 contacts returns within 200 ms per input, foreground import of a phone change and write-back after vault sync each complete within 5 seconds, and the two-phone 500-contact fixture produces no duplicates; retain M1, M2 and M6 as real-device validation (SC-001–SC-007) — in `src-tauri/src/contacts/phone/acceptance_tests.rs` (a debug build: first run of 2,000 about 8 s, a phone change and a write-back about 0.15 s each, the search rule over 5,000 about 65 ms per input) and for the window's list in `scripts/check-contacts-search.ts`; the search measures the rule over the headers already held, as the window does, since reading 5,000 headers from the vault takes about as long in a debug build as the whole limit

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none.
- **Foundational (Phase 2)**: after PR A; blocks every story. Ships in PR B together with US1.
- **US1 (PR B)**: after Phase 2.
- **US2 (PR C)**: after US1 (uses the service and the app).
- **US3 (PR D)**: after US2 (uses the phone run, links and writer).
- **US4 (PR E)**: after US1; independent of US2/US3, can be built in parallel to them on its own branch from `main`.
- **US5 (PR F)**: after US1 (and after US4 if `ContactDetail` should carry confirmed holzi addresses; otherwise add them in PR E).
- **US6 (PR G)**: after US1.
- **US7 (PR H)**: after US4 and the first invitation PR of 027 or 028.
- **Polish**: T091 with PR B; T092 after each PR; T093 at the end of US3 and US4; T094 in every PR.

### Within Each User Story

- Tests first; they fail before the implementation.
- Types and rules before the service, service before commands, commands before UI, UI before E2E.

### Parallel Opportunities

- Phase 2: T008 and T009 next to each other after T007.
- US1: T012–T016 together; T020 next to T019; T022 and T027 next to the views.
- US2: T030–T036 together; Kotlin (T037–T039) next to the Rust rules (T042–T043).
- US3: T051 and T052 together.
- US4: T060–T062 together; T066 next to T063–T065.
- US5: T071 and T072 together; T076 (vault-sdk) next to T073–T075.
- US4, US5 and US6 can start in parallel after PR B, each on its own branch from `main`.

## Parallel Example: User Story 2

```text
Task: "Index tests in src-tauri/src/contacts/phone/index_tests.rs"
Task: "Mapping tests in src-tauri/src/contacts/phone/mapping_tests.rs"
Task: "Merge tests in src-tauri/src/contacts/phone/merge_tests.rs"
Task: "Reconcile tests in src-tauri/src/contacts/phone/reconcile_tests.rs"
Task: "Guard tests in src-tauri/src/contacts/phone/guard_tests.rs"
```

## Implementation Strategy

### MVP First (User Story 1)

1. PR A (docs) and PR B (foundation + US1).
2. **Stop and validate**: address book on desktop and phone, sync between own devices.

### Incremental Delivery

1. PR C: phone contacts in the vault (US2), validate on the emulator and by hand (M1, M2, M5).
2. PR D: holzi as the leading address book with targets (US3), M3, M4, M6.
3. PR E: business cards (US4), M7.
4. PR F, PR G: extensions and agents.
5. PR H with spaces or data shares.

## Notes

- Every new Rust file gets a `*_tests.rs` next to it; `FakePhoneBook` lives in a test-only file.
- No commit trailers or PR lines that name an agent (constitution).
- "Every call site" (operator memory): before moving `Caller` (T006), adding a capability field (T041) or hooking the list-signing places (T064), grep all users again on the PR's base.
- Never run `cargo test` during an E2E run (it overwrites `target/debug/holzi`).
