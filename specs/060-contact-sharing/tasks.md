---
description: 'Task list for spec 060-contact-sharing'
---

# Tasks: Kontakte teilen und der Kontakt „Ich“

**Input**: Design documents from `/specs/060-contact-sharing/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/share-format.md](./contracts/share-format.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md), [quickstart.md](./quickstart.md)

**Tests**: Included. Rust unit tests as `*_tests.rs` next to the module, declared
`#[cfg(test)] #[path = "x_tests.rs"] mod tests;` (pattern of `src-tauri/src/contacts/card/own_card.rs`), never
inline `mod tests`. TypeScript helpers are pure functions in `src/lib/contacts/` checked by
`scripts/check-contacts-*.ts` (`pnpm check:contacts`). Run cargo only for the affected targets with `-j 4` and
check `df -h` first (a full build crashed the machine before).

**Organization**: Grouped by user story in priority order (P1: US1–US4, P2: US5). Phase 1 is a spike, Phase 2 is
the shared backend (migrations, card storage, envelope); each story phase ends with its own test tasks.

**Shipping note**: work in `.worktrees/060-contact-sharing`. The spec and plan are PR #576. The implementation
may ship as several PRs from the same line of work (suggested: A = Phase 1–3, B = Phase 4–5, C = Phase 6,
D = Phase 7–8); commits per phase with Conventional Commits headers (`feat(contacts): …`, `feat(deep-link): …`,
`docs(060): …`); **no agent attribution lines in commits or PR bodies** (project rule). Before every push
`git fetch` and, if another migration took `0042` or `0043`, renumber (T006).

**Rules for every task**: before authoring a new named function, component or module consult graphify
(`graphify query`) and extend a candidate if one fits (candidates the plan evaluated: `own_card::refresh`,
`received::save`, `format::sign`, `store::derived_name`, `ShareConfirm`/`PromptPark`, `useDeepLink` of the legacy
app); every new UI text in `de.json` and `en.json`, placeholders as `{name}`, never `<…>` (vue-i18n refuses it);
the product name in texts is „HaexVault“, never bare „Relay“ (say „Nostr-Relay“ or „Heim-Relay“); `UiButton` in
forms needs `type="button"`; interactive elements get `data-testid`; no `Element`/DOM globals in composables that
the Node sandbox checks load (run **all** `pnpm check:*` after a composable change); a new `#[ts(export)]` type
carries the `Contacts…`/`ContactShare…` prefix; keep every file under 300 lines (split into a sibling module);
imports at the top of the file; no legacy aliases or migration paths for the removed settings (holzi has no
users).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup (spike)

**Purpose**: Check the assumption that costs most if wrong: that a `haex-vault://` link reaches the app on Linux
and on Android (research R9).

- [x] T001 Deep-link spike (done 2026-10-26, result in `research.md` R9 „Ergebnis des Spikes“; local branch `060-spike-deep-link`, not for merge). Linux: a URL argument reaches `get_current()`; Android (emulator API 35): the plugin's `build.rs` generated the intent-filter into `AndroidManifest.xml`, a VIEW intent cold-starts `MainActivity`, `get_current()` is still `None` in `setup` and the URL arrives about 0.7 s later through `on_open_url`, a second link to the running app arrives through `on_open_url` too. Needed fixes: `rust-ini` pinned to 0.21.1 in `Cargo.lock` (0.21.3 breaks the plugin), `deep-link:default` only in `default.json`. Not covered: OS registration on Linux (`xdg-open`) and a JS listener on an Android cold start (both go to T062).
- [ ] T002 [P] Add `@tauri-apps/plugin-deep-link` 2.6.2 (the line the Rust crate resolved to) to `package.json` and `pnpm-lock.yaml` with `pnpm add`; no other change.

---

## Phase 2: Foundational (blocks all user stories)

**Purpose**: The own contact's row, the new card storage with its checks, and the envelope every story uses.

- [x] T003 [P] `SELF_CONTACT_ID` in `src-tauri/src/contacts/ids.rs`: a UUID generated once (`uuidgen`), documented „never change“ like `NS_CONTACTS`, plus `pub fn is_self(id: &str) -> bool`. No other use yet.
- [x] T004 [P] Migration SQL `src-tauri/src/identity/migrations_contacts_self.rs` (new): `pub const CONTACTS_SELF_0042: &str` with one `INSERT INTO haex_contacts (id, origin_kind, origin_device, origin_caller, created_at, updated_at) VALUES (…)` using the constant of T003 as a literal, `'holzi'`, the nil UUID, `'internal:self'`, and one fixed timestamp in the format of `passwords::clock`; no HLC columns (data-model §1). Module doc names the stamping rule of haex-crdt.
- [x] T005 [P] Migration SQL `src-tauri/src/identity/migrations_contacts_cards.rs` (new): `pub const CONTACTS_CARDS_0043: &str` = `DROP TABLE haex_contacts_cards;` then `CREATE TABLE haex_contacts_cards (hash TEXT PRIMARY KEY NOT NULL, vault_pubkey TEXT NOT NULL, contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE, card BLOB NOT NULL, issued_at TEXT NOT NULL, received_at TEXT NOT NULL);` and two plain `CREATE INDEX` (`vault_pubkey`, `contact_id`), statements separated by `--> statement-breakpoint` like `migrations_contacts.rs`. No UNIQUE anywhere. Doc comment: drops existing rows on purpose (research R5).
- [x] T006 Register both in `src-tauri/src/identity/migrations.rs`: `mod`/`use` lines, `m.insert(MigrationName::from("0042_contacts_self"), …)` and `("0043_contacts_cards_hash", …)` after `0041`; `HOLZI_TRIGGER_VERSION` 27 → 28 with a line in the `/// - N:` list („0043 rebuilds a tracked table“); declare the test modules of T007/T008 in `src-tauri/src/identity/mod.rs`. Check the next free numbers on `origin/main` first.
- [x] T007 [P] Migration test `src-tauri/src/identity/migrations_contacts_self_tests.rs` (new; helpers of `migrations_tests.rs`, pattern of `migrations_contacts_tests.rs`): a fresh vault has exactly one `haex_contacts` row with `SELF_CONTACT_ID` and the constant values; an upgrade from `migration_source_before("0042_contacts_self")` adds it once; **two vaults opened separately and synced** end with one row (the stamping assumption of research R1; if this fails, switch the seed to `identity/bootstrap.rs` with `INSERT OR IGNORE` and the same values and say so in `research.md`).
- [x] T008 [P] Migration test `src-tauri/src/identity/migrations_contacts_cards_tests.rs` (new): the new columns, both indexes and the primary key exist, `unique_indexes == 0`, `has_hlc_column`, rows of an old-shape table are gone after the upgrade, `contact_id` cascade still removes cards when a contact is deleted.
- [x] T009 [P] `src-tauri/src/contacts/card/format.rs`: `pub fn payload_hash(card: &ContactCard) -> Result<String, HolziError>` (SHA-256 hex of `signing::canonical(card)`), `pub fn sign_with(card, secret, max_text: Option<usize>)` that is `sign` without the 2,048-character loop when `max_text` is `None`; `sign` calls `sign_with(…, Some(MAX_TEXT))` unchanged. Tests in `format_tests.rs`: hash is stable and differs per field, `sign_with(None)` signs a card with 32 devices that `sign` refuses.
- [x] T010 `src-tauri/src/contacts/card/received.rs` rewrite for the new table (data-model §1, §4): `save(tx, contact_id, &card)` computes `payload_hash`, check-then-write (no `ON CONFLICT`), inserts if the hash is absent; `verified(q, row)` (signature via `format::from_bytes`, `hash == payload_hash`, `vault_pubkey == hex(card.vault)`; failing rows are skipped with `log::warn!`); `stored(q, vault)` = newest verified card (`list_generation`, then `issued_at`, then smallest `hash`) with its contact; `addresses(q, vault)` = union of devices (by `endpoint_id`, relay of the newest card) and relays; `of(q, contact_id)` builds `ContactCardInfo` from the union with the newest card's name and `issued_at`; `stored_of_contact` = newest card of the contact; `with_cards`; new `delete_for_vault(tx, contact_id, vault)` for the old-card „replace“. Keep `blocked`/`block`/`unblock` as they are.
- [x] T011 [P] `src-tauri/src/contacts/card/received_tests.rs` (new): save twice with the same card → one row; two cards of one vault → both stored, `stored` is the newest, `addresses` is the union without duplicates; a row whose `hash` does not match its card, whose `vault_pubkey` is another vault's, or whose signature is broken is ignored by every reader; `delete_for_vault` removes only that vault's rows; deleting the contact cascades.
- [x] T012 Adapt the callers of the old one-card-per-vault API: `src-tauri/src/contacts/service/cards.rs` (`checked`, `card_accept`, `card_of_vault` compile against the new `received`; behaviour of the old `holzi:card:` path unchanged until T044), `src-tauri/src/files/peer/connect.rs` and `src-tauri/src/files/peer/contacts.rs` and `src-tauri/src/extensions/peers/invite.rs` (dial the devices of `received::addresses`), `src-tauri/src/sync/foreign_auth.rs` (`foreign_trust` keeps using `stored` = newest card for the generation threshold; add a comment why added addresses never lower it). Update the SQL that tests insert directly into `haex_contacts_cards` (`extensions/peers/test_support.rs`, `files/peer/test_support.rs`, `contacts/service/cards_tests.rs`, `sync/foreign_auth_tests.rs`) to the new shape. Run `cargo test -j 4 contacts:: sync::foreign_auth files::peer extensions::peers`.
- [x] T013 [P] `src-tauri/src/error.rs`: add `HolziError::ShareInvalid { reason }`, `ShareTooLarge`, `ContactsSelf` with stable codes; map them in `src/composables/useErrorString.ts` and add the texts to `src/i18n/locales/{de,en}.json` (`errors.share.*`, `errors.contacts.self`).
- [x] T014 `src-tauri/src/contacts/share/envelope.rs` (new; `share/mod.rs` declares `envelope`, `name`, `build`, `preview`, `merge`; declare `share` in `src-tauri/src/contacts/mod.rs`): `ShareEnvelope`, `NameParts`, `ShareHead`, `ShareValue` as in `contracts/share-format.md`; `PREFIX_LINK = "haex-vault://contact/"`, `MAX_SHARE_TEXT = 16_384`, `QR_MAX = 2_048`, `MAX_VALUES = 200`; `encode(&ShareEnvelope) -> Result<String>` (checks every limit, `ShareTooLarge` over `MAX_SHARE_TEXT`) and `decode(text) -> Result<ShareEnvelope>` (prefix, base64url, postcard, no trailing bytes, version 1, limits; values of a known kind go through `kinds::validate`, an invalid one is dropped and counted in a returned `skipped`; the card bytes go through `format::from_bytes`, own vault → `CardOwn`); `decode` also accepts `holzi:card:…` and returns it as an envelope with `values` from the card, `name` from the card and `legacy_card: true`.
- [x] T015 [P] `src-tauri/src/contacts/share/envelope_tests.rs` (new): round trip; every limit (201 values, a 4,097-character value, 33 devices, text over 16,384); wrong prefix, bad base64url, trailing bytes, version 2 → `ShareInvalid`; a flipped bit in the embedded card → `CardInvalid`; a card of the own vault → `CardOwn`; an unknown kind survives unchanged; a `phone` value that fails `kinds::validate` is skipped and counted; `holzi:card:` text decodes as a legacy envelope.
- [x] T016 [P] `src-tauri/src/contacts/share/name.rs` (new): `pub fn name_for(head: &Head, vault_for_npub: Option<[u8; 32]>) -> (String, NameParts)` (FR-011: name parts joined by spaces; else `received::npub`; else empty; display name, nickname, organization, phone and e-mail are never a substitute). Tests in `share/name_tests.rs` (new): parts only, npub when parts are empty, empty without parts and without vault, nickname and organization ignored.

**Checkpoint**: `cargo test -j 4 identity:: contacts::card contacts::share sync::foreign_auth` is green; the address book works as before for contacts and cards.

---

## Phase 3: User Story 1 - Meine eigenen Daten im Kontakt „Ich“ pflegen (Priority: P1) 🎯 MVP

**Goal**: A fixed „Ich“ contact at the top, editable, not deletable, hidden from the phone sync, visible to agents with its real name.

**Independent Test**: In a new vault open the contacts, find „Ich“, enter data, find it on a second device, fail to delete it (FR-001 to FR-006, FR-003a).

- [x] T017 [US1] `src-tauri/src/contacts/model.rs` and `src-tauri/src/contacts/store.rs`: `is_self: bool` on `Contact` and `ContactHeader` (`id == SELF_CONTACT_ID`, set in `load` and `headers`); `Contact.name` stays the derived name (FR-003a). Regenerate the ts-rs bindings (`cargo test -j 4 contacts::model`), then strip trailing spaces in `src/types/bindings/*.ts`.
- [x] T018 [US1] `src-tauri/src/contacts/service/contacts.rs`: `delete` fails with `ContactsSelf` if any id is the own contact (before anything is written, for every `Caller`); `update` refuses a `display_name` patch other than absent on the own contact; `check_identity` returns `Ok` for the own contact even when nothing is entered.
- [x] T019 [US1] `src-tauri/src/contacts/service/targets.rs`: refuse every target change for the own contact (`ContactsSelf`); `src-tauri/src/contacts/phone/run.rs` `unlinked_headers`: drop the own contact from the candidates so no phone contact is ever matched or merged into it.
- [x] T020 [P] [US1] Agent view: `src-tauri/src/contacts/commands/agent.rs` and `src/lib/actions/contactsActions.ts` — the search/get results carry `isSelf`; the action descriptions say that the contact marked `isSelf` is the user and that its name is the real name (FR-003a). No new permission.
- [x] T021 [P] [US1] Rust tests: `src-tauri/src/contacts/service/contacts_tests.rs` (delete refused for `User`, `BuiltinAgent`, `Extension`, a bulk delete with the own id deletes nothing; `display_name` patch refused, name-part patch works; an empty own contact can be updated), `src-tauri/src/contacts/service/targets_tests.rs` (targets refused), `src-tauri/src/contacts/phone/matching_tests.rs` or `run_targets_tests.rs` (own contact is not a candidate), `src-tauri/src/contacts/agent_tests.rs` (agent with „Kontakte lesen“ gets the real name and `isSelf`).
- [x] T022 [US1] Frontend list and editor: `src/lib/contacts/search.ts` (`selfFirst(headers)`; the search haystack keeps the real name, phones and e-mails), `src/components/contacts/List.vue` (the own contact in its own section above the letters, titled `t('contacts.self')`; selection and mass delete skip it: `SelectionBar.vue`, `MassDeleteDialog.vue`), `src/components/contacts/EntryPage.vue` (title „Ich“, no delete action, `TargetsSection.vue` not rendered), `src/components/contacts/EntryEditor.vue` (display-name field disabled with a hint), `src/components/contacts/Avatar.vue` (initials from the real name), i18n `contacts.self` („Ich“ / „Me“) and the hint.
- [x] T023 [P] [US1] `scripts/check-contacts-search.ts` and a new `scripts/check-contacts-self.ts`: the own contact is first regardless of name, is found by its real name, `contactTitle(header)` returns the translated „Ich“ for `isSelf` and `name` otherwise.

**Checkpoint**: US1 works alone: the contact exists, is editable and protected; `pnpm check:contacts` and the contacts Rust tests are green.

---

## Phase 4: User Story 2 - Meinen Kontakt teilen und vorher wählen, was mit rausgeht (Priority: P1)

**Goal**: The share dialog for the own contact with per-item checkboxes (devices and Nostr relays included), link, QR code and copy.

**Independent Test**: Fill „Ich“, deselect two items and a relay, build the share, read the envelope in a test and see exactly the chosen items (FR-008 to FR-012, FR-015 to FR-019).

- [x] T024 [US2] `src-tauri/src/contacts/card/own_card.rs`: pull the body of `refresh` into `build_card(q, devices: Option<&[[u8; 32]]>, relays: Option<&[String]>) -> Result<Option<ContactCard>>` (`None` filter = all; `values` empty; `name` = the own contact's name); `refresh` calls it with no filter; new `build_for_share(q, selection)` signs it with `format::sign_with(card, secret, None)` and returns `Ok(None)` when `keys::vault_secret` is `None` (linked device); `issued_at` = `max(now, newest stored own card + 1)`. Remove `OwnCard`, `Choice`, `choice`, `set_choice`, `read`, `own_part`, `PREF_OWN_CONTACT`, `PREF_VALUES`; `refresh`, `refresh_quietly` and the table stay (FR-028).
- [x] T025 [P] [US2] `src-tauri/src/contacts/card/own_card_tests.rs`: drop the tests of the removed choice; add: `build_card` without filter equals what `refresh` stores; `build_for_share` with one device and no relay signs a card that `format::from_bytes` accepts and that carries only them, no values; a linked device (no secret) gives `None`; `issued_at` is never lower than the stored own card's; nothing is stored.
- [x] T026 [US2] `src-tauri/src/contacts/share/build.rs` (new): `options(q, contact_id) -> ContactShareOptions` (rows grouped by kind with ids, head rows only when filled, `holzi: None | Own{…}` here, the name per FR-011, `can_share` per FR-017) and `build(q, contact_id, &ShareSelection) -> ContactShareOutput` (checks every id against the contact and every device against the effective device list; assembles the envelope with the chosen values/head/name parts, `HolziSelection::Own` → `own_card::build_for_share`; `envelope::encode`; `qr_svg` only when `len <= QR_MAX`). The own contact's holzi values (`kinds::is_holzi_address`) are never rows (FR-005). The `ts-rs` types `ContactShareOptions`, `ContactShareRow`, `ContactShareDevice`, `ContactShareSelection`, `ContactShareOutput` live in `share/model.rs` (new).
- [x] T027 [P] [US2] `src-tauri/src/contacts/share/build_tests.rs` (new): everything unselected is absent from the decoded envelope (phone, birthday, note, organization); the own card carries only the chosen devices and relays; the name falls back to the npub when no name parts exist; a linked device builds without a card and `options` says `can_sign: false`; nothing left → `can_share: false`; a foreign value id or an unknown device → `ShareInvalid`; a share over 16,384 characters → `ShareTooLarge`; `qr_svg` only up to 2,048 characters and decodes to the same text as `link`.
- [x] T028 [US2] `src-tauri/src/contacts/service/share.rs` (new, declared in `service/mod.rs`): `share_options`/`share_build` with `require_user`; `src-tauri/src/contacts/commands/cards.rs`: commands `contacts_share_options` and `contacts_share_build`; remove `contacts_own_card`, `contacts_own_card_set` and their args/types from `commands/cards.rs`, `service/cards.rs` and `lib.rs`; register the new commands in `lib.rs`. Regenerate the bindings and delete `src/types/bindings/ContactsOwnCard*.ts`.
- [x] T029 [P] [US2] `src/lib/contacts/share.ts` (new, pure): `initialSelection(options)` (everything chosen), `toggle`, `selectAll`/`selectNone`, `toArgs(selection)`, `canBuild(options, selection)`; `scripts/check-contacts-share.ts` (new): defaults are all-on, All/None, deselecting a device removes it from the args, an empty selection with an empty name cannot build.
- [x] T030 [P] [US2] `src/composables/useContactShare.ts` (new): thin `invoke` wrappers `shareOptionsAsync(contactId)`, `shareBuildAsync(contactId, selection)` (preview/accept follow in T045); no DOM globals.
- [x] T031 [US2] `src/components/contacts/ShareDialog.vue` (new, ≤300 lines; split a `ShareRows.vue` if needed): rows with checkboxes per group, „Alle/Keine“, the own holzi section (vault identity always, each device and Nostr-Relay its own row, the hint when `canSign` is false), the name as the recipient sees it, a hint that photos are not shared, the result area (link, QR code only when present else „zu groß für QR“, copy button, size, the too-large message naming what to deselect, the nothing-to-share message); the selection is never stored (FR-010); i18n `contacts.share.*` in `de.json`/`en.json`.
- [x] T032 [US2] „Teilen“ entry: `src/components/contacts/EntryPage.vue` action menu opens `ShareDialog` for the contact (also for the own contact); `src/components/contacts/List.vue` row menu gets the same action if the list has one.
- [x] T033 [US2] Remove what the share replaces: `src/components/contacts/OwnCard.vue`, `src/components/settings/ContactsCardSetting.vue` and its use in `src/components/settings/ContactsView.vue`, the `card` entry of `src/components/wm/appRoutes.ts`, the „Eigene Visitenkarte“ items of `src/components/contacts/Toolbar.vue`, the own-card functions of `src/composables/useContactCards.ts`, `src/lib/contacts/registry.ts` references, and the keys `contacts.card.own*` and `settings.contacts.card.*` in both locales. `ContactsView.vue` keeps Abgleich and the blocked list (FR-027).

**Checkpoint**: US2 works alone: sharing „Ich“ gives a link and QR code that carry exactly the chosen items; the old own-card page and settings are gone.

---

## Phase 5: User Story 3 - Jeden Kontakt aus dem Adressbuch genauso teilen (Priority: P1)

**Goal**: The same dialog for every contact; a foreign contact's confirmed addresses go as one checkbox with the original card; contacts without holzi addresses share like any other.

**Independent Test**: Share a contact with phone, e-mail, note and a stored card, deselect note and e-mail, decode: the original card bytes are unchanged and the deselected items are absent (FR-013 to FR-015, FR-017a, FR-026).

- [x] T034 [US3] `src-tauri/src/contacts/share/build.rs`: `HolziSelection::Card` for a foreign contact: `options` returns `holzi: Card{npub, devices, relays, issued_at}` only when `received::stored_of_contact` finds a verified card; `build` embeds those card bytes **unchanged**; the name's npub fallback uses the stored card's vault (`name_for(…, Some(vault))`); unconfirmed hand-typed holzi values stay ordinary rows; photo, groups, targets, origin are never part of the envelope (FR-026); a contact without a card builds without one and `options` has `holzi: None`.
- [x] T035 [P] [US3] Extend `src-tauri/src/contacts/share/build_tests.rs`: the embedded card equals the stored bytes; deselecting the card row removes it; a contact without a card shares fine and the envelope has `card: None`; a nameless contact with a card gets the npub as name; unknown-kind values travel unchanged; the photo is not in the envelope; a stored card whose row fails `verified` is not offered.
- [x] T036 [US3] `src/components/contacts/ShareDialog.vue`: the foreign section: one row „holzi-Adressen (bestätigt)“ with the FR-013 hint (the original proof goes along, it also holds what that contact released, devices and relays only go together); the row is absent without a card; no holzi section for a contact without addresses; i18n.
- [x] T037 [P] [US3] `scripts/check-contacts-share.ts`: the `Card` option is one all-or-nothing choice; a contact without addresses has no holzi row; the args of a foreign share never contain device or relay lists.

**Checkpoint**: US3 works alone: every contact, own or foreign, opens the same dialog and builds the right text.

---

## Phase 6: User Story 4 - Eine geteilte Weitergabe einlesen, ohne etwas zu verlieren (Priority: P1)

**Goal**: Preview and per-item choices, add-only merge, checked cards, and the deep link that opens the preview.

**Independent Test**: Read the shares of US2 and US3 as a new contact and into an existing one; a contact with a mobile number keeps it when a share with only an e-mail address is added; a tap on a link opens the preview and saves nothing (FR-018a, FR-021 to FR-026, SC-005a, SC-011).

- [x] T038 [US4] `src-tauri/src/contacts/share/preview.rs` (new): `preview(q, text, target) -> ContactSharePreview` per data-model §2: decode, targets (vault already on a contact → that contact preselected and the only target; phone/e-mail matches as `Hint`; the own contact never a target), items with `state` (`New`, `Same`, `Conflict`) and the allowed `choices`/`default` for name parts, organization, job title, note, each value (with `existing` values of the same kind), and the holzi-addresses item (`Add`/`Skip`, `Replace` only for `legacy_card`); `identity` (`None`/`New`/`Matches`/`Mismatch`), `npub` short form, `reachable`, `skipped`, `blocked`, `vault_name`; read-only.
- [x] T039 [US4] `src-tauri/src/contacts/share/merge.rs` (new): `apply(tx, text, target, decisions, preview_revision, origin) -> Result<Contact>` recomputes envelope, card and items from the text (never from the UI), rejects a decision the item does not allow or an `Update` value that is not the target's, and for an existing target requires the exact `targetRevision` from the preview; a mismatch returns `SharePreviewStale` with a fresh preview before anything is written. It then writes in one transaction: new contact (origin `card`, name parts or display name from `envelope.name`), values `add`/`update`/`skip`, single fields `keep`/`update`, the holzi item: `received::save` (add) or `delete_for_vault` + `save` (legacy `replace`, older card → `CardOlder`); an existing target's confirmed vault is never replaced (`Mismatch` allows only a new contact); the own contact as a target and a blocked or own vault → refused; nothing is written when any check fails.
- [x] T040 [P] [US4] `src-tauri/src/contacts/share/preview_tests.rs` (new): own contact is never a target; the vault already on a contact preselects it and is the only target; phone/e-mail matches are only hints; an existing target gets an opaque revision; `Same` items carry no choice; `Conflict` for a filled single field; `Mismatch` for another vault identity (spec scenarios 8–11, 24–26); `reachable: false` without a card; the preview writes nothing.
- [x] T041 [P] [US4] `src-tauri/src/contacts/share/merge_tests.rs` (new): the mobile number stays when a share with only an e-mail is added (scenario 3); `add` is the default and `update(value_id)` replaces only that value; `skip` leaves it; an identical value is never duplicated; empty single fields fill, filled ones `keep`; two shares one after the other lose nothing; the holzi addresses are added next to the old ones and never removed (scenarios 11, 12), `skip` leaves them; an older card changes nothing; `replace` works only for a legacy card and removes only that vault's older cards; an `Update` pointing at another contact's value fails and writes nothing; a stale `targetRevision` returns `SharePreviewStale` with a refreshed preview and writes nothing; a card with a broken signature, the own vault or a blocked vault stops the whole share; the own contact as a target fails.
- [x] T042 [US4] `src-tauri/src/contacts/service/share.rs`: `share_preview(caller, text, target)` and `share_accept(caller, text, target, decisions, preview_revision)` (`require_user`, `db.read` / `db.write`); `share_accept` forwards the revision check and refreshed-preview result; remove `card_preview`, `card_accept`, `checked`, `add_values` and their types (`CardInto`, `CardPreview`, `CardContact`, `CardPreviewValue`) from `service/cards.rs` once `share` covers the legacy `holzi:card:` text; keep `card_of_vault`, `blocked`, `set_blocked`.
- [x] T043 [US4] `src-tauri/src/contacts/commands/cards.rs` and `lib.rs`: commands `contacts_share_preview` and `contacts_share_accept` (the latter accepts `previewRevision`); remove `contacts_card_preview` and `contacts_card_accept`; regenerate the bindings, delete `src/types/bindings/ContactsCardPreview.ts`, `ContactsCardInto.ts` and the other bindings of removed types; strip trailing spaces.
- [x] T044 [P] [US4] Update `src-tauri/src/contacts/service/cards_tests.rs` to the new commands (it covered accept as new contact, accept into existing, own card → `CardOwn`, older → `CardOlder`, vault on one contact, block/unblock) and move what is now `share` behaviour into `service/share_tests.rs` (new).
- [x] T045 [US4] `src/composables/useContactShare.ts`: add `sharePreviewAsync(text, target?)` and `shareAcceptAsync(text, target, decisions, previewRevision?)`, preserving the revision from the displayed preview; `src/lib/contacts/shareImport.ts` (new, pure): default decisions from a preview (`Add`/`Keep`/default per item), `validate(preview, decisions)`, `summary(preview, decisions)`; `scripts/check-contacts-share.ts`: defaults add everything new and replace nothing, a `Mismatch` identity cannot be accepted into the existing contact, and stale-preview acceptance is refreshed without writing.
- [x] T046 [US4] `src/components/contacts/ShareImport.vue` (new; replaces `CardImport.vue`, which is removed): paste field, camera scan on Android (`@tauri-apps/plugin-barcode-scanner` as `CardImport.vue` did), preview with name, `npub` in short form and the „bestätigt“ hint (FR-021a: „passt zur Vault dieses Kontakts“ or the first-confirmation note), the target choice, a row per item with its choice (Hinzufügen / Aktualisieren with a picker of the existing values / Nicht übernehmen; Behalten / Aktualisieren for single fields; Hinzufügen / Nicht übernehmen for the holzi addresses), the „nicht über holzi erreichbar“ note, skipped count, the blocked-vault note with the existing block action, the name of the vault; route `import` in `src/components/wm/appRoutes.ts` replaces `card/import`; `Toolbar.vue` entry „Einlesen“; i18n `contacts.import.*`.
- [ ] T047 [US4] Deep link wiring (after the T001 spike passed): `src-tauri/src/deep_link/mod.rs` (new): `init` registers `on_open_url` and emits `deep-link-received`; take over the plugin, the config, `deep-link:default` in `capabilities/default.json` and the generated intent-filter block of `AndroidManifest.xml` from the spike branch `060-spike-deep-link` (commit `fb615eed`; remove the temporary `eprintln!`), and keep the `Cargo.lock` pin `rust-ini` 0.21.1 (say why in the PR; a plain `cargo update` would undo it); declare the module in `lib.rs`; `register_all()` only in debug builds on Linux/Windows.
- [ ] T048 [US4] `src-tauri/src/instances/presence.rs`: `ProcessPresence::was_alone(&self) -> bool` (true when `announce` took the exclusive lock); test in `presence_tests.rs`.
- [ ] T049 [US4] `src-tauri/src/deep_link/inbox.rs` (new): `offer(dir, url) -> Offered` (writes `<uuid>.url`, waits up to 2 s for it to vanish, deletes it and reports `NoTaker` otherwise), `claim_next(dir) -> Option<String>` (atomic `rename` to `<uuid>.claimed-<pid>`, reads and removes it), `watch(app, dir)` (500 ms interval, started only while a vault is open, emits `deep-link-received` and brings the window to the front); start the watcher in **both** `src-tauri/src/instances/open.rs` and `src-tauri/src/instances/import.rs` (session services start in both places); in the startup of a process launched with a URL and `!was_alone()`, call `offer` and exit on `Offered`, else keep the URL. Tests in `src-tauri/src/deep_link/inbox_tests.rs` (new, temp dir): offer + claim, two claimers → exactly one wins, no taker → file removed and `NoTaker`, a stale file is ignored after a timeout.
- [ ] T050 [P] [US4] `src/composables/useDeepLink.ts` (new, ported from the legacy `useDeepLink.ts`, without its `isDesktop()` guard): `getCurrent` **and** `onOpenUrl` (on Android a cold start delivers the URL through the event, not through `getCurrent` in `setup`), the same URL arriving through both is handled once, listener for `deep-link-received`; a module-level pending URL; `handleDeepLink(url)` accepts only `haex-vault://contact/…` and ignores everything else; `src/stores/shareInbox.ts` (new, Pinia) holds the text; `src/app.vue` calls `init()` on mount; the pure core `parkOrOpen(url, vaultOpen)` lives in `src/lib/contacts/deepLink.ts` (new) so Node can check it.
- [ ] T051 [US4] `src/pages/workspace/[instance].vue`: after `sessionRestored`, if `shareInbox` holds a text, `wm.app.open` the contacts app at `/import` (the text stays in the store, not in the URL); a link that arrives later is opened the same way; while no vault is open the pending text survives the navigation from `/` to `/workspace/<name>` (same process). `ShareImport.vue` reads the store, runs the preview and clears it; opening a link only previews, it never calls accept (FR-018a).
- [ ] T052 [P] [US4] `scripts/check-contacts-deeplink.ts` (new): `parkOrOpen` parks while no vault is open and opens after, a foreign scheme or a malformed URL is ignored, a second link replaces the first only after the first was consumed or dismissed; a short Rust test in `src-tauri/src/deep_link/mod_tests.rs` that a non-`haex-vault://` URL is not emitted.

**Checkpoint**: US4 works: a shared contact can be previewed and merged without loss, and a link or QR scan lands in the preview.

---

## Phase 7: User Story 5 - Ein Agent teilt einen Kontakt, und ich bestätige (Priority: P2)

**Goal**: `contacts.share` as a destructive action with exactly one confirmation dialog per call, a changeable selection, no standing permission.

**Independent Test**: Via the built-in agent call the action, confirm, change and decline; check what the agent gets in each case (FR-029 to FR-032, SC-010).

- [x] T053 [US5] `src-tauri/src/chat/tools/mod.rs`: `Tool::confirms_itself(&self) -> bool` (default `false`); `src-tauri/src/chat/tools/native_action.rs`: `true` when the action def says so; `src-tauri/src/chat/turn/tool_round.rs`: for a `confirms_itself` tool skip the general `tool-permission-request` in Manual and Auto but keep `Deny` in Plan (next to the `ask_user` branch); `AgentActionDef` gets `confirms_itself: bool` (default false) and `src/lib/actions/types.ts`/`agentTools.ts` pass it. Tests in `src-tauri/src/chat/tools/permission_tests.rs` (or the existing tool_round tests): Manual and Auto ask nothing for a self-confirming `Risky` tool, Plan denies it, a normal `Risky` tool still asks.
- [x] T054 [P] [US5] `src-tauri/src/contacts/agent_share_confirm.rs` (new; pattern of `src-tauri/src/files/agent/share_confirm.rs`): `ContactShareConfirm { park: PromptPark<ContactShareConfirmRequest, Option<ShareSelection>> }`, `EVENT_CONTACT_SHARE_CONFIRM = "contact-share-confirm-request"`, `CONFIRM_TIMEOUT = 120 s`, command `contact_share_confirm_answer`; managed state and command registered in `lib.rs`. `ContactShareConfirmRequest` (ts-rs) per data-model §2.
- [x] T055 [US5] `src-tauri/src/contacts/agent_share.rs` (new): the native executor `ContactsShareAgent` (pattern of `files/agent/shares.rs`): `ContactsAgent::grants(Read)`; load the contact through the service with `hidden()`; restrict the agent's proposed `selection` to what `share::options` offers (never widen, FR-031); `ContactShareConfirm::ask`; on confirm `share::build` with the **confirmed** selection and return `{link, qrAvailable, size}`; on decline, cancel or timeout `NativeError::new("denied_by_user", …)` without any contact content; register it in `lib.rs` next to `SharesAgent`.
- [x] T056 [US5] `src/lib/actions/contactsActions.ts`: `contacts.share` (`effect: 'destructive'`, `scope: 'contacts.read'`, `agentCallable: true`, `runner: 'native'`, `confirmsItself: true`, input `{contactId, selection?}`, result `{link, qrAvailable, size}`, titles and description in `de`/`en`, saying that the user confirms every call); add it to `src/lib/actions/catalog.ts` if the catalog is explicit.
- [x] T057 [US5] `src/components/contacts/ShareConfirmDialog.vue` (new): listens to `contact-share-confirm-request` (queue like `FilesShareConfirmDialog.vue`), hosts `ShareDialog` in confirm mode (proposal pre-selected, buttons „Freigeben“/„Ablehnen“, the agent's name, the editable selection) and answers with `contact_share_confirm_answer`; mount it in `src/pages/workspace/[instance].vue` next to `FilesShareConfirmDialog`; `ShareDialog.vue` gets the `mode: 'share' | 'confirm'` prop.
- [x] T058 [P] [US5] Tests: `src-tauri/src/contacts/agent_share_tests.rs` (new): without the „Kontakte lesen“ grant → `NotFound`, no existence leak; confirm → the link carries exactly the confirmed selection; the user removes a phone number in the dialog → the agent's text lacks it; decline, a cancelled turn and a timeout → `denied_by_user` and no content; the agent cannot add an item the dialog does not offer; two calls → two questions (no remember); `scripts/check-agent-actions.ts`: `contacts.share` is `destructive`, `confirmsItself`, scope `contacts.read`, native, and has no extension surface.

**Checkpoint**: US5 works: one dialog per call, the agent gets the text only after confirmation.

---

## Phase 8: Polish & Cross-Cutting

- [x] T059 [P] `docs/adr/0017-contacts-share-envelope.md`: record the envelope with an embedded card, add-only import, the hash-keyed card table with several cards per vault, the `haex-vault://` scheme and the destructive agent action; include status, context, decision, consequences (dropped addresses at the update, trigger version 28, inbox directory), and alternatives (single-instance plugin, vCard, random UUID keys).
- [x] T060 [P] `CONTEXT.md`: terms **Ich-Kontakt**, **Weitergabe**, **Teilen-Dialog**, **Echtheitsnachweis der holzi-Adressen** next to the Adressbuch entry (spec 051); say that the UI never shows the word „Karte“.
- [x] T061 [P] Annex notes in `specs/051-phone-contacts/` (`spec.md` FR-009, FR-011, FR-015a, FR-012; `contracts/tauri-commands.md` § Visitenkarten; `data-model.md` for `haex_contacts_cards`) pointing to Spec 060, as 059 did for 024; no change of their requirements text beyond the pointer.
- [ ] T062 [P] E2E (run the host-bridge way of the project; check `gh pr checks` first, CI may have run it): `scripts/e2e/scenarios/contacts-share.test.ts` (new): fill „Ich“, share with deselections, read in a second vault, the deselected items are missing; add to a contact with a mobile number, the number stays; a vault identity mismatch is refused; extend `agent-contacts.test.ts` (share, change, decline); `scripts/e2e/scenarios/android-contacts-deeplink.test.ts` (new; waits for the deep link of part 4, #601; `contacts-share` and the agent part of `agent-contacts` are in): `adb shell am start -a android.intent.action.VIEW -d <link>` opens the preview with the vault unlocked and after unlocking.
- [ ] T063 Verification pass from `quickstart.md`: targeted `cargo test -j 4`, all `pnpm check:*`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm typecheck`, `pnpm lint:rust`, `cargo clippy --target aarch64-linux-android --all-targets -- -D warnings`; strip trailing spaces in `src/types/bindings/*.ts`; the manual run of `quickstart.md` steps 1–8.
- [ ] T064 PR hygiene: `git fetch`; confirm `0042`/`0043` and ADR `0017` are still free (renumber otherwise); the PR text names the dropped addresses of the cards migration, the trigger version 28, the Ich seed assumption and the inbox directory; update `research.md` R9 with the spike result; no attribution lines.

---

## Dependencies & Execution Order

- Phase 1: T001 first (it gates T047–T052); T002 ∥ T001. Phase 2 does not need the spike except T013 ∥ others.
- Phase 2: T003 ∥ T005 ∥ T009 ∥ T013 ∥ T016; T004 after T003; T006 after T004 and T005; T007 and T008 after T006; T010 after T005 and T009; T011 after T010; T012 after T010; T014 after T009 and T013; T015 after T014.
- US1 (T017–T023) needs T003 and T006; it is independent of the share work. US2 needs Phase 2 and T017 (the own contact exists): T024 → T025; T026 needs T014, T016, T024; T027 after T026; T028 after T026; T029 ∥ T030 after T028's bindings; T031 after T029, T030; T032 after T031; T033 after T032.
- US3 (T034–T037) extends T026/T031. US4: T038 and T039 need T010 and T014; T040 ∥ T041 after them; T042 after T038, T039; T043 after T042; T044 after T043; T045 after T043; T046 after T045; T047 after T001; T048 → T049 (T049 needs T047, T048); T050 ∥ T047; T051 needs T046, T050; T052 after T050.
- US5 (T053–T058) needs T026 (build) and T031; T053 ∥ T054; T055 after both and T056; T057 after T054 and T031; T058 last.
- Polish after the stories it documents or tests (T062 after US4 and US5; T063 last before T064).

## Parallel Example

```text
Phase 2 start:  T003 (ids.rs) ∥ T004 (migrations_contacts_self.rs) ∥ T005 (migrations_contacts_cards.rs) ∥ T009 (format.rs) ∥ T013 (error.rs) ∥ T016 (name.rs)
After T026:     T027 (build tests) ∥ T029 (lib/contacts/share.ts + check) ∥ T030 (composable)
After T039:     T040 (preview tests) ∥ T041 (merge tests) ∥ T050 (useDeepLink.ts)
```

## Implementation Strategy

MVP = Phase 1 + 2 + US1 (T001–T023): the own contact exists, is protected and visible to agents, and the new card
storage with its checks is in place. The feature is only useful with US2 to US4 together (share and read), so
ship P1 as A (Phase 1–3), B (Phase 4–5) and C (Phase 6), each green on its own; US5 (P2) and the polish follow as D.
Do not merge Phase 2's table rebuild without T012: every reader of the old table has to move in the same PR.
