---
description: 'Task list for spec 092-wallet-passes'
---

# Tasks: Cards and tickets (the Wallet app)

**Input**: Design documents from `/specs/092-wallet-passes/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md), [contracts/external-callers.md](./contracts/external-callers.md), [contracts/android-intents.md](./contracts/android-intents.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic: Rust unit tests in sibling `*_tests.rs` files, Rust integration tests in `src-tauri/tests/`, pure frontend checks as `scripts/check-passwords-*.ts` (run by `pnpm check:passwords`) and e2e scenarios. Write each test task before its implementation and see it fail first. Test data never contains real secrets or real passes; fixtures use invented values and the marker pattern `SECRET-MARKER-<name>`; pkpass fixtures are built in the tests with throwaway keys.

**Organization**: Grouped by user story in priority order (P1: US1, US2, US3; P2: US4, US5, US6, US7; P3: US8). The branch `092-wallet-passes` starts from `main`. Modules under `src/lib/**` import siblings relatively with a `.ts` suffix so the Node check scripts can load them. Every file stays ≤ 500 lines. Commits follow Conventional Commits and carry no agent attribution (no `Co-Authored-By`, no "Generated with" line; the constitution and the operator rule beat the harness reminder). Rust commands run as `nix develop --command scripts/with-nix-host-bridge.sh cargo …` with `-j 4` and targeted test targets (a full local `cargo test` once took the editor down); frontend commands as `nix develop --command bash -c '…'`. Everything new in Rust must compile in default and `--no-default-features`. Do not edit the worktree a running `tauri:dev` uses.

**Shipping note**: Six stacked pull requests, one per slice of [plan.md](./plan.md): A = Phase 2 and US1, B = US2 and US4, C = US3, D = US6, E = US7, F = US8, then Polish. Every slice is shippable alone and leaves logins unchanged. Push with the token of the `haexmas` account (other sessions switch the active account); a stacked PR is retargeted to `main` when its base lands. After a pull request is opened, trigger CodeRabbit with `@coderabbitai review` only when it is larger than 1 000 lines.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US8)

---

## Phase 1: Setup

- [x] T001 Prepare the worktree `.worktrees/092-wallet-passes`: real `pnpm install` (no symlinked `node_modules`, done), reflink the Rust build cache from the primary checkout if `src-tauri/Cargo.lock` matches (`cp -a --reflink=always ../../src-tauri/target src-tauri/target`); record baseline counts of `pnpm check:passwords`, `pnpm check:templates` and `cargo test --manifest-path src-tauri/Cargo.toml --lib passwords::` in this task's note
- [x] T002 [P] Write `docs/adr/0025-entry-types-share-identity.md` (status accepted, date 2026-11-12): an entry type is a `kind` on `haex_passwords_item_details` plus a 1:1 detail table; satellites keyed by item id serve every type; a new type costs a detail table, a view and an import mapping and ships with a migration so the sync gate of spec 024 stays the enforcement (research R1); rejected: a second table with copies of six satellites. Content per [research.md](./research.md) R1, R12
- [ ] T003 [P] Before the first new name, run `graphify query` for: "barcode qr encode decode", "screen brightness wake lock", "share intent android", "deep link inbox park until unlock", "zip read limits", "folder path names search"; prefer an existing candidate over a new artifact and note in this task which candidates were evaluated and why they did not match
- [x] T004 [P] `rxing` check ([research.md](./research.md) R3): add `rxing` to `src-tauri/Cargo.toml` with the newest compatible version and a comment citing spec 092; build both feature configurations (`cargo check --manifest-path src-tauri/Cargo.toml` and `--no-default-features`, also the Android target check as the other mobile dependencies are checked); write a throw-away test that encodes and decodes one value of each of QR, Code 128, EAN-13, PDF417 and Aztec through a rasterised image; record version, licence, binary size change and the verdict in `research.md` R3. If the verdict is negative (licence, size, a format missing, no Android build), stop and ask the operator before switching to the fallback named there (`bwip-js` drawing, Rust validation and decode-only). Remove the throw-away test afterwards (the real tests come in T031)

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: The migration, the type on every entry, the login-only default at every outside reader, history format version 2 and the hand-kept lists of tables. After this phase logins behave exactly as before.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T005 Create `src-tauri/src/identity/migrations_passwords_pass.rs` with `pub const PASSWORDS_PASS_0061: &str` (statements separated by `\n--> statement-breakpoint\n`; no HLC columns, haex-crdt adds them): `ALTER TABLE haex_passwords_item_details ADD COLUMN kind TEXT NOT NULL DEFAULT 'login'`; `CREATE TABLE haex_passwords_pass_details` with `item_id TEXT PRIMARY KEY NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE`, `pass_kind TEXT NOT NULL DEFAULT 'other'`, `start_date TEXT`, `start_at TEXT`, `start_zone TEXT`, `parent_id TEXT` (**no foreign key**), `source_id TEXT`; `CREATE TABLE haex_passwords_pass_barcodes` with `id TEXT PRIMARY KEY NOT NULL`, `item_id TEXT NOT NULL REFERENCES haex_passwords_item_details(id) ON DELETE CASCADE`, `format TEXT NOT NULL`, `value TEXT NOT NULL`, `label TEXT`, `position INTEGER NOT NULL DEFAULT 0`, `flag TEXT`; indexes `idx_haex_passwords_pass_details_parent_id (parent_id)` and `idx_haex_passwords_pass_barcodes_item_id (item_id)`, both non-unique. No UNIQUE besides primary keys, no `WITHOUT ROWID` (034 FR-037). Template: `migrations_passwords_owner.rs` and `migrations_passwords_refs.rs`
- [x] T006 Register the migration: declare `mod migrations_passwords_pass;` in `src-tauri/src/identity/mod.rs`, add `m.insert(MigrationName::from("0061_passwords_pass"), PASSWORDS_PASS_0061.to_owned())` in `holzi_migration_source()` of `src-tauri/src/identity/migrations.rs`, raise `HOLZI_TRIGGER_VERSION` from 39 to 40 and add one line to the constant's doc comment naming spec 092 (the file is large; keep the change to a few lines)
- [x] T007 Write `src-tauri/src/identity/migrations_passwords_pass_tests.rs` first (template `migrations_passwords_owner_tests.rs`, helpers `open`, `column_names`, `migration_source_before`, `table_exists` from `migrations_tests.rs`): a vault opened with `migration_source_before("0061_passwords_pass")` holding a login reopens with the production source and the old row has `kind = 'login'`; `column_names` of both new tables match [data-model.md](./data-model.md); `update_trigger_sql` of `item_details` names `kind`; the two new tables have `z_dirty_*` triggers; there is no unique index on either new table beyond the primary key (`pragma_index_list`); deleting an `item_details` row removes its `pass_details` and `pass_barcodes` rows (cascade); a `parent_id` pointing at no entry is accepted (no foreign key)
- [x] T008 [P] Extend `src-tauri/tests/vault_upgrade.rs` with `reopening_a_pre_0061_vault_tracks_kind_and_the_pass_tables` (pattern of `reopening_a_pre_0027_vault_tracks_the_owner_of_a_password_entry`, `source_without("0061_...")`, old trigger version 39, reopen through `vault_config`); extend `src-tauri/tests/pre_rename_vault.rs` so every entry of the frozen vault reads `kind = login` with its fields, tags, folder, attachment and history unchanged (SC-003)
- [x] T009 Add the type to the model in `src-tauri/src/passwords/model.rs`: `EntryType { Login, Pass, Unknown(String) }` (serde as text; an unknown value is carried, never defaulted), `ItemHeader` gains `kind`, `pass_kind`, `start_date`, `start_at`, `start_zone`, `parent_id` (none carries a code), `AgentHeader` gains `kind`, `pass_kind`, the start fields and `expires_at`; `ItemInput` and `ItemPatch` stay login-shaped. Update `load_headers` in `src-tauri/src/passwords/items.rs` with `d.kind` and a `LEFT JOIN haex_passwords_pass_details`; update `agent_headers`; regenerate bindings (`pnpm generate:ts-types`) so `src/types/bindings/ItemHeader.ts` and friends change
- [x] T010 Write `src-tauri/src/passwords/items_kind_tests.rs` first, then implement: `headers_in_scope` and `agent_headers` take a `kinds` filter that **defaults to logins**; `list_headers`, `headers_for_page` (`service/by_site.rs`), `list_autofill_shapes` (`service/items.rs`) and `agent_search` use the default; `read_secret_item` and `totp_code` refuse an entry whose kind is not `login` for an outside caller; `access.rs` `ItemState` carries the kind and `visible()` / `authorize_read` honour it. Tests: a seeded pass is returned by none of these for any caller unless the caller asked for passes and has the scope; a login still is
- [x] T011 [P] Passkeys never touch a pass: in `src-tauri/src/passwords/passkeys_ops.rs` (`readable`, `candidates`, `authorize_existing`, `create`) and `passkey_links.rs` (`link`) refuse an entry whose kind is not `login`; in `src-tauri/src/browser_bridge/methods/passkeys.rs` `entries()` and `passkey_create` skip passes. Test in `src-tauri/tests/passwords_passkeys.rs`: a passkey create or link onto a pass is refused and the bridge offers no pass as a presence choice
- [ ] T012 [P] Other outside readers skip passes: `src-tauri/src/remote_storage/credentials.rs` (the two `headers_in_scope` loops), `src-tauri/src/files/local/drives/unlock.rs`, `src-tauri/src/passwords/service/import.rs` `existing_keys` (duplicate detection counts logins only), `src-tauri/src/browser_bridge/methods/items.rs`, `src-tauri/src/agent_browser/act_credentials.rs` and `act_entry_fill.rs`; add a unit or integration test per file that seeds a pass matching a login by title, address and tag and asserts 0 passes _(Slice A: every one of these readers goes through `headers_in_scope`, which is now logins-only; `import.rs` `existing_keys` filters `kind = 'login'`; one test seeds a pass that matches a login and runs the list, autofill-by-site and autofill-shape readers in `tests/passwords_pass_access.rs`. A test per reader file stays open as part of T021.)_
- [x] T013 [P] Extension SDK and agent tool: in `src-tauri/src/extensions/passwords.rs` `summary` carries `type` and `list` takes a `type` filter (default `login`), `read`, `update`, `delete` refuse a pass; in `src-tauri/src/passwords/agent_actions.rs` `SearchInput` gains `type` (`login` default, `pass`, `any`) and `header_json` adds `kind`, `passKind`, `start`, `expiresAt` for a pass; `passwords.secret.get` refuses a pass; update `src-tauri/src/chat/eval/tools.json` and `src/lib/actions/passwordsActions.ts` (schema and description: logins are the default); extend `src-tauri/tests/mcp_server_passwords.rs` and `src-tauri/src/passwords/agent_actions_tests.rs`
- [ ] T014 Write the read-path leak test first: `src-tauri/tests/passwords_pass_no_code_leak.rs` seeds a pass with a code value `SECRET-MARKER-pass-code` and runs every read path (list, headers, `get_item`, agent search and `header_json`, MCP search and get, extension list and read, bridge `get_items`, history list and get, the text of a failing call, the log capture) and asserts the marker appears in none (SC-004). It cannot pass until the pass service exists; mark it `#[ignore]` with a note and enable it in T033 _(Moved to slice B: the test needs the pass service that writes codes; it joins T031. Slice A's `tests/passwords_pass_access.rs` and `snapshots_pass_tests.rs` already assert no code in the overview, the agent rows, the history view and `Debug`.)_ _(Service level done in T031 (open until the adapters are covered): `passwords_pass_no_code_leak.rs` covers the overview, lists, entry, history list and view, agent search and secret read, drawing, errors and the log; the bridge and the MCP server serve logins by the shared default, which `passwords_pass_access.rs` covers; the extension SDK adapter has its own test in `src-tauri/src/extensions/passwords_pass_tests.rs` (its list returns a pass only on request and with no code, its read, update and delete refuse a pass). An adapter-level marker test for the bridge `get_items` and the MCP search and get stays open.)_
- [x] T015 History format version 2 in `src-tauri/src/passwords/snapshots.rs`; write `snapshots_pass_tests.rs` first: `SnapshotData` gains `pass: Option<PassBlock>` (`#[serde(default)]`: kind, pass kind, start fields, codes with format, value, label, position); `CURRENT_VERSION` becomes 2 **but a stored stand without `version` reads as 1** (change the serde default for a missing version to the literal 1 while `Default::default()` keeps the current version; research R2); `build_current` reads the pass row and codes for a pass; `changed_fields` names `kindOfPass`, `start`, `codes`; `get` returns code formats and labels only; `reveal` returns one code value on request; `restore` writes typed fields and codes back and never the parent or the folder. Tests: a version-less legacy stand (`snapshots_tests.rs` "legacy state") reads as 1, a version-1 stand reads as a login, a version-2 stand round-trips, equal states write no new stand, restore leaves parent and folder
- [x] T016 Hand-kept lists of tables (research R12): `src-tauri/src/passwords/trash.rs` `purge_item` and `purge_group` delete `pass_barcodes`, then clear `parent_id` of passes pointing at the entry (`UPDATE haex_passwords_pass_details SET parent_id = NULL WHERE parent_id = ?`), then `pass_details`, children first; `src-tauri/src/passwords/copy.rs` adds `kind` to its column list and copies the detail row and codes of a pass (new ids, same parent, `source_id` cleared); `import/apply.rs`, `references_db.rs` and `import/legacy_haex_vault/history.rs` compile with the new `SnapshotData` field and write logins only. Tests: purge of a pass leaves no `pass_*` row; purge of a parent clears its children's link; copy of a pass copies codes
- [x] T017 [P] Two-replica sync test in `src-tauri/tests/passwords_sync.rs` (pattern `deleting_an_entry_with_children_removes_every_row_on_the_other_device`): a pass with codes created on device A arrives on B with its detail row and codes; deleting it on A removes every row on B including `pass_*` rows and leaves delete markers; a remote delete of a parent clears the link on the child
- [x] T018 [P] Unknown type and the second layer of FR-042 (frontend): in `src/lib/passwords/passType.ts` (new, pure) `entryTypeOf(header)` returns `login`, `pass` or `unknown`; `src/components/passwords/EntryPage.vue` shows a "needs a newer holzi" panel for `unknown` (no editor, no write, no delete other than trash), `List.vue` and `ListItem.vue` show the row with a notice and the type icon; add `scripts/check-passwords-pass-type.ts` and the i18n keys `passwords.unknownType.title` and `.body` in `src/i18n/locales/de.json` and `en.json` (no `<…>` in any message). Add the rule to ADR 0025 (T002) that every new type ships with a migration
- [x] T019 Amend spec 034 in `specs/034-password-manager/spec.md` and `data-model.md`: FR-036 deviations list gains the `kind` column and the two new tables (this spec FR-005), FR-007 names folder names as a searched field (this spec FR-022); each amendment cites spec 092. Run `nix develop --command pnpm exec prettier --write` on the touched files
- [ ] T020 Gates for slice A: `pnpm generate:ts-types`, `pnpm check:passwords`, `pnpm typecheck`, `pnpm lint`, `pnpm lint:rust` (both feature sets), `cargo test --manifest-path src-tauri/Cargo.toml --lib passwords::` and `... --lib identity::` (two commands, `cargo test` takes one filter; targeted, `-j 4`), `--test vault_upgrade`, `--test pre_rename_vault`, `--test passwords_sync`, `--test passwords_access`, `--test passwords_passkeys`, `--test mcp_server_passwords`, `--test browser_bridge_passwords`; commit with Conventional Commits (no agent attribution), push with the `haexmas` token, open pull request A

**Checkpoint**: A vault from before this spec opens unchanged; every entry is a login; no outside reader can return a pass.

---

## Phase 3: User Story 1 - Entries have a type, and nothing existing changes (Priority: P1)

**Goal**: Every part that handles entries from outside knows the type and treats a pass as a pass; every login behaves as before.

**Independent Test**: [quickstart.md](./quickstart.md) slice A: the frozen old vault reads unchanged; with a seeded pass every credential lookup returns 0 passes and no read path carries a code; an unknown type is not shown as an empty login.

Most of this story is delivered by Phase 2; these tasks close the remaining acceptance scenarios.

- [ ] T021 [US1] Credential-lookup acceptance test (SC-005) in `src-tauri/tests/passwords_access.rs`, `browser_bridge_passwords.rs` and the tests of `src-tauri/src/agent_browser/`: a pass whose title, address and tag equal a login's is returned by autofill by site, `entry.fill`, bridge `get_items`, credential tools, remote-storage credentials, drive unlock and import duplicate detection in 0 of 0 cases; the same login is still returned (scenario 2 of US1)
- [ ] T022 [US1] Agent and MCP acceptance test: a credential search returns only logins unless the caller asks for passes; every row carries its type; no row carries a code (scenario 3 of US1); an external agent with a tag grant sees a pass only when the tag is covered
- [x] T023 [US1] Old-device scenario (scenario 4, FR-042): add a Rust test in `src-tauri/src/sync/handshake_schema_tests.rs` that a peer with 60 migrations is refused by a build with 61 (`RejectCode::Incompatible`, reason shown) and a frontend check (T018) that an entry with an unknown `kind` is neither editable nor shown as an empty login; document the result in `research.md` R1 as verified

---

## Phase 4: User Story 2 - Create a pass and show its code (Priority: P1)

**Goal**: The user creates a pass with title, kind, codes, start and end and free fields and shows the code large, bright and with the screen on.

**Independent Test**: Create a pass with one code of each format, open it, show large, read each rendered code back with the decoder: it yields the stored value; the start shows in the time of the place and, when the zones differ, in the device's time too.

### Tests (write first)

- [x] T024 [P] [US2] `src-tauri/src/passwords/pass/codes_tests.rs`: for each of QR, Code 128, EAN-13, PDF417, Aztec a code is validated, drawn as SVG, rasterised and decoded back to the stored value (SC-002); EAN-13 with a wrong check digit is refused by hand with a reason and kept with `flag = invalid_value` from import; the SVG is dark on white with the quiet zone of its format; an unknown format is kept as text with `flag = unknown_format`
- [x] T025 [P] [US2] `scripts/check-passwords-pass-date.ts` for `src/lib/passwords/passDate.ts`: start 20:00 in `Europe/London` shows `20:00` for London and `21:00` for a `Europe/Berlin` device, both labelled; on a London device only one; a fixed offset `+02:00` works; a date alone shows a date with no zone; the days around the 2026 changes to and from daylight saving time in Europe and North America (run with several `TZ` values); an unknown zone name falls back to UTC with the name; the end before the start is allowed
- [x] T026 [P] [US2] `src-tauri/src/passwords/pass/pass_tests.rs` (service level on `test_support::open_test_vault`): create writes `item_details` (kind `pass`, login columns empty), `pass_details`, codes, free fields, tags, folder and one stand in one transaction; `start_date` and `start_at` are never both set; a patch with an old `updatedAt` gives `PasswordsConflict`; `passwords_create_item` refuses `kind = pass`; an entry's kind cannot change (FR-001); a `pass_details` row missing for a pass reads as `other`

### Implementation

- [x] T027 [US2] `src-tauri/src/passwords/pass/model.rs` and `mod.rs`: `PassInput`, `PassPatch`, `PassKind { Ticket, LoyaltyCard, Voucher, Membership, BoardingPass, Other, Unknown(String) }` (unknown text reads as `Unknown`, shows as "other" and is written back unchanged, FR-004, as T009 does for the entry kind), `Start { Date(String) | At { at, zone } }`, `Code { id, format, value, label, position, flag }`; ts-rs exported; declare the module in `src-tauri/src/passwords/mod.rs`
- [x] T028 [US2] `src-tauri/src/passwords/pass/codes.rs` on `rxing` (research R3): `check(format, value) -> Result<(), Reason>` (format rules and check digits), `draw(format, value) -> Svg` (dark on white, quiet zone), `decode(image_bytes) -> Vec<Found { format, value }>`; no network, no global state
- [x] T029 [US2] `src-tauri/src/passwords/service/pass.rs`: `create`, `update` (same `updated_at` token as `update_item`), `codes`, `code_svg`, `check`, each one transaction with `take_snapshot`; the service writes no value into logs or errors; wire `Caller::User` only (FR-027: no outside writer)
- [x] T030 [US2] `src-tauri/src/passwords/commands/pass.rs`: `passwords_pass_create`, `passwords_pass_update`, `passwords_pass_codes`, `passwords_pass_code_svg`, `passwords_pass_code_check` per [contracts/tauri-commands.md](./contracts/tauri-commands.md); register them in `src-tauri/src/lib.rs` next to the other `passwords_*` commands; error codes `pass.code_invalid`, `pass.not_a_pass`; regenerate bindings
- [x] T031 [US2] Remove the throw-away `rxing` test of T004; enable `passwords_pass_no_code_leak.rs` of T014 (remove `#[ignore]`) and make it pass for the pass service, the history and the logs
- [x] T032 [P] [US2] `src/lib/passwords/passDate.ts` (pure, `Intl.DateTimeFormat` with `timeZone` for IANA names, arithmetic for fixed offsets): `formatStart(header, deviceZone)` returns `{ place, device? }`; `startInstant(header)`; `isExpiredPass`; used by list, view, "Linked" and history (FR-002)
- [x] T033 [P] [US2] `src/composables/usePasses.ts`: thin `invoke` wrappers for the `passwords_pass_*` commands (all arguments in one object); `src/lib/passwords/passType.ts` extended with the icon table of literal `lucide:` names per kind (`lucide:ticket`, `train-front`, `plane`, `wallet`, `id-card`, `gift`, `award`, `store`; research R17) so the Nuxt icon scan finds them
- [x] T034 [P] [US2] `src/composables/useScreenBright.ts` and `src/components/passwords/PassCodeLarge.vue`: full-screen white ground, the code from `passwords_pass_code_svg`, its label; on open call brightness and keep-awake on, on leave, tab close, window hide and background call them off; where unsupported the code is still shown large and nothing throws (FR-008); desktop uses the Screen Wake Lock API where the webview has it
- [x] T035 [US2] Core commands `screen_set_bright(on)` and `screen_set_keep_on(on)` in `src-tauri/src/lib.rs` plus the plugin side per [contracts/android-intents.md](./contracts/android-intents.md): `@Command setScreenBright` and `setKeepScreenOn` with `@InvokeArg` classes in `src-tauri/plugins/holzi-android/android/src/main/java/space/haex/holzi/android/HolziAndroidPlugin.kt`, methods in `src-tauri/plugins/holzi-android/src/android.rs`, no-ops in `other.rs` (pattern `set_secure`)
- [x] T036 [US2] Pass view and editor: `src/components/passwords/PassView.vue`, `PassDetails.vue`, `PassCodes.vue`, `PassEditor.vue` (typed fields, codes: add, change, reorder, remove, copy a value as text, free fields, attachments) reusing `EntryTabs`, `HistoryTab`, `KeyValues`, `Attachments`, `ConflictDialog`, `UnsavedDialog`, `Breadcrumbs`; `src/lib/passwords/passDraft.ts` with `emptyPassDraft`, `toPassInput`, `toPassPatch`, `isDirty` (pure) and `scripts/check-passwords-pass-draft.ts`; `src/components/passwords/EntryPage.vue` dispatches on `detail.kind` for an existing entry and on `?type=pass` for a new one; a code is drawn dark on white in every theme (FR-009); the expired mark shows (034 FR-008); at 360 px without horizontal scrolling _(Done with one shell for both types: `EntryView.vue` shows a pass through `PassDetails.vue` and `PassCodes.vue` and hides autofill aliases and passkeys in `ViewExtra.vue` (a `pass` prop), instead of a second `PassView.vue`; the editor is `PassEditor.vue` with `PassEditorDetails.vue` and `PassCodesEditor.vue`.)_
- [x] T037 [US2] "New" and routes: add `passwords.newMenu.pass` item to `src/components/passwords/Toolbar.vue` (`data-testid="passwords-new-pass"`) and to the empty-area menu of `List.vue` (`run()` case); extend `src/lib/passwords/registry.ts` `isSecretFreeLocation` with `type=pass` (only that value) and `parent=<id>` for `entry/new` only, and `scripts/check-passwords-routes.ts` and `check-passwords-tabs.ts` for both keys; window titles stay static
- [x] T038 [P] [US2] i18n: add every new text in German and English to `src/i18n/locales/de.json` and `en.json` under `passwords.pass.*`, `passwords.newMenu.pass`, `passwords.kinds.*` ("Karte/Ticket", "Ticket", "Kundenkarte", "Gutschein", "Mitgliedschaft", "Bordkarte", "Sonstiges"), code formats, "Show large", error texts; no `<provider>`-style placeholders in any message; the label for the type icon in the list
- [x] T039 [US2] E2E scenario `scripts/e2e/scenarios/passwords-pass-basic.test.ts` (file name equals the `scenario('…')` string): helper `createPass` in `scripts/e2e/lib/passwords.ts`; create a pass with a QR and a Code 128, open it, "Show large", leave; assert the dialog and testids; expired pass is marked _(Written and type-checked; the local rig is blocked, so CI runs it first.)_
- [ ] T040 [US2] Gates and pull request B part 1: `pnpm check:passwords`, `pnpm check:templates`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm lint:rust`, targeted `cargo test --lib passwords::pass`, then `nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --grep passwords-pass-basic` (if the local rig is blocked, rely on CI and say so in the pull request)

**Checkpoint**: A pass can be created, shown large and edited; its code appears nowhere but on its own page.

---

## Phase 5: User Story 3 - A pass belongs to an account or another pass (Priority: P1)

**Goal**: Link a pass to one entry; the parent lists its passes; trash, delete and sync never cascade.

**Independent Test**: Link two vouchers with the same free fields to one card; open the card: two separate rows; trash the card: both stay with "(in trash)"; delete it for good: the links are cleared; two devices linking in opposite directions end with a flagged loop that can be repaired.

### Tests (write first)

- [x] T041 [P] [US3] `src-tauri/src/passwords/pass/parent_tests.rs`: setting the parent to itself or to a descendant is refused with `pass.parent_loop` (recursive query over `parent_id`); a parent not in the vault is accepted; clearing works; the cycle check terminates on a stored loop (visited set / depth bound)
- [x] T042 [P] [US3] `scripts/check-passwords-links.ts` for `src/lib/passwords/links.ts`: `parentState(header, headers)` gives `open`, `in_trash`, `not_available`, `loop`; `linkedOf(id, headers, now)` lists direct children in the order of FR-021; a loop of two or more passes is found with a visited set and flags every pass on it; a chain is never followed forever; hundreds of children stay usable
- [x] T043 [P] [US3] Extend `src-tauri/tests/passwords_sync.rs`: two replicas link in opposite directions and converge with both rows intact; purge of the parent on A clears the child's link on B; trash of a parent changes nothing in the child

### Implementation

- [x] T044 [US3] `src-tauri/src/passwords/pass/parent.rs` and the commands `passwords_pass_set_parent` (service `set_parent` in `service/pass.rs`, same conflict token) and `parent` handling inside create and update; `trash.rs` leaves children untouched when a parent is trashed (FR-015)
- [x] T045 [P] [US3] `src/lib/passwords/links.ts` (pure, see T042) and `src/components/passwords/PassLinks.vue`: "Belongs to: …" at the top of a pass (link to the parent, with "(in trash)" or "(not available)"), a "Linked" section on every entry that has children (title, start or end date, valid or expired, in the order of FR-021; hide-expired applies), a loop flag with "Clear link"; used in `PassView.vue` and the login's `EntryView.vue` _(Done as `PassBelongsTo.vue` and `PassLinked.vue`; moves to the Wallet app in W02 and W04.)_
- [x] T046 [P] [US3] `src/components/passwords/PassParentPicker.vue` (entry picker: any login or pass, search, a probable match by provider name or address first, never auto-links) used in `PassEditor.vue`; "Add linked pass" button in `EntryView.vue` and `PassView.vue` opens `/entry/new?type=pass&parent=<id>` with the parent preset; exclude passes from `ReferencePicker.vue` (a `{REF}` resolves login fields; `{REF}` placeholders unchanged, FR-017) _(The "Add linked pass" button is on `EntryView.vue`; the parent picker is in the pass editor.)_
- [x] T047 [P] [US3] i18n for linking in `de.json` and `en.json`: "Gehört zu", "Verknüpft", "(im Papierkorb)", "(nicht verfügbar)", "Verknüpfung lösen", loop notice
- [ ] T048 [US3] E2E scenario `scripts/e2e/scenarios/passwords-pass-links.test.ts` (and a multi-device case in `passwords-pass-sync-two-devices.test.ts` if the rig supports it): link, trash the parent, restore, delete for good; two vouchers with the same field names stay apart (scenario 6)
- [ ] T049 [US3] Gates and pull request C (same gate list as T040, plus `--test passwords_sync`)

**Checkpoint**: Linking works in every state; no operation on a parent changes a child except clearing the link on a hard delete.

---

## Phase W: The Wallet app (decision of 2026-11-12, see the clarification in [spec.md](./spec.md))

**Goal**: Passes live in an app of their own, **Wallet**, over the same data and service; the password manager shows logins only. Slice B1 (#968) and the linking backend (slice C) are done and stay; this phase moves their screens and adds the app.

**Independent Test**: Open Wallet from the launcher: a flat list of passes by date; create one, show its code large; open the password manager: no pass anywhere in it (list, folders, counts, search, trash); a login that has linked passes lists them and opens each in Wallet; a pass opens the login it belongs to in the password manager.

**What changes in later tasks.** Every screen task of phases 6 to 11 lands in `src/components/wallet/` with the prefix `Wallet` (the names `PassX` and `passwords-pass-*` in those tasks read as `WalletX` and `wallet-*`), and every e2e scenario of them is `wallet-*` with an `openWallet` helper. T051 and T055 (search over folder names) apply to Wallet's own search only and the amendment of 034 FR-007 is dropped; T052 keeps only its `kind` half; T056 proposes the tag `Ticket`, and a folder only when the user has made one; T053 and T054 build the Wallet list (kind filter, sort, hide expired, no type filter); T058 is `wallet-organize`. T085 uses "Wallet" in German as well.

- [x] W01 App shell: `system.wallet` in `src/lib/wm/apps.ts` (several windows, tab shows the app's name, icon `lucide:wallet`, `public/app-icons/wallet.svg`), `src/lib/wallet/registry.ts` (places `''`, `entry/:id`, `entry/:id/history`, `trash`; secret-free query keys only `kind`, `q`, `type`-free; `entry/new` with `parent=<id>`), `WalletApp.vue` frame like `PasswordsApp.vue` with its own toolbar, route table in `components/wm/appRoutes.ts`, i18n `wm.apps.wallet` and `wm.wallet.*` in German and English, and the pure checks that list apps (`check-wm-app-match`, `check-wm-dock`, a new `check-wallet-registry.ts`)
- [x] W02 Wallet screens (W01 already brought `WalletApp.vue`, the toolbar, a flat `List.vue` with `ListItem.vue`, and an `EntryPage.vue` that still shows the password manager's view and pass editor; the `trash` place is registered but has no view yet): move `PassEditor`, `PassEditorDetails`, `PassCodesEditor`, `PassDetails`, `PassCodes`, `PassCodeLarge`, `PassBelongsTo`, `PassLinked`, `PassParentPicker` from `components/passwords/` to `components/wallet/` (`Wallet` prefix), the pure libs stay in `src/lib/passwords/` next to the shared header types (`passType`, `passDate`, `passDraft`, `links`, `sortFilter`), add `WalletList.vue` (flat list of the passes of the store, `sortByDate` by default, search over title, address and tag names, a kind filter), `WalletEntryPage.vue` (view and editor of a pass, the history tab), `WalletTrash.vue`; "New" in the Wallet toolbar and empty-area menu; remove `passwords-new-pass`, the `newPass` menu command and `?type=pass` from the password manager (`menus.ts`, its check, `registry.ts`, `List.vue`, `Toolbar.vue`) _(The screens are `components/wallet/{Editor,EditorDetails,CodesEditor,Details,Codes,CodeLarge,BelongsTo,Linked,ParentPicker,EntryView,EntryPage,List,ListItem,Toolbar,Trash}.vue`; the list sorts by date and searches title, address and tag names, the kind filter comes with T053. One trash serves both apps: `TrashView.vue` takes a `scope` and each app lists and empties only its own entries (`lib/passwords/scope.ts`, `emptyTargets`). The view's data and freshness logic is the composable `useEntryDetail`, shared by both views.)_
- [x] W03 The password manager shows logins only: the store keeps all headers, a `loginHeaders` view feeds the list, the folder tree and its counts, the search, the tag filter and counts, the trash view and the pickers (`ReferencePicker`, the parent picker's login side); `EntryView.vue`, `ViewExtra.vue`, `EntryPage.vue` lose their pass branches; an address `entry/<id>` of a pass in the password manager's window is redirected to Wallet; `PassLinked` stays in the login's view as the "Linked" section and "Add linked pass" opens Wallet _(`store.loginHeaders` and `store.loginTags` are the views; a pass id in the password manager shows a notice with a button that opens it in Wallet instead of redirecting by itself; `ViewExtra.vue` keeps its one `pass` switch because Wallet's Extra tab shows the same fields without aliases and passkeys. A pass that sits in a password-manager folder from before Wallet is not counted there; deleting that folder for good still removes it.)_
- [x] W04 Cross-app links: a wm call that opens an app at a place (`openAppAt('system.wallet', '/entry/<id>')`, to the open window of the app when there is one, else a new one, the way the contacts app is reached from chat) used by "Linked" (login to pass), "Belongs to" (pass to login or pass) and the "Add linked pass" button; a test of the place builder in `check-wallet-registry.ts` _(`useOpenEntry` and `entryTarget` (`lib/passwords/scope.ts`); the new pass for a login opens in Wallet with `parent=<id>`)_
- [ ] W05 Re-point the rest (the amendment of 034 FR-007 is taken back; the launcher shortcut and share target remain with US8): the launcher shortcut and share target of US8 name `system.wallet`; the tab-history and session-restore locations of a pass are Wallet's; the amendments of 034 (FR-007 search over folder names) made in slice A are taken back, and 034 FR-036 keeps only the data-model deviation
- [x] W06 Move the e2e scenario `passwords-pass-basic` to `scripts/e2e/scenarios/wallet-basic.test.ts` with an `openWallet` helper in `scripts/e2e/lib/wallet.ts` (create in the editor, show large, expired mark, no code as text), and add a case to `passwords-basic.test.ts` that a pass in the vault shows nowhere in the password manager _(written and type-checked; the local rig is blocked, so CI runs it first)_
- [ ] W06a Wallet folders: a fixed root group `wallet` (like the trash's fixed id, created on first use, no migration), `src/lib/wallet/folders.ts` (the Wallet subtree of the group rows, pure), a sidebar and folder dialogs reusing the password manager's tree components scoped to that subtree, move and drag of passes into folders, "All" flat by date and a folder view; the password manager's tree, counts and pickers hide the `wallet` subtree; tests for the subtree rule and a case in `wallet-basic`
- [ ] W07 Gates and pull request W (same list as T040, plus `pnpm check:wm` or the checks that list apps, and the narrow-window case)

**Checkpoint**: Two apps over one vault; the password manager is unchanged for a user who has no passes, and a user who has passes sees them only in Wallet.

---

## Phase 6: User Story 4 - Keep tickets in order and find them again (Priority: P2)

**Goal**: Sort by date, filter by type and kind, hide expired, find a ticket by a word of its folder name.

**Independent Test**: With twenty tickets of three providers and five loyalty cards, filter by type then kind, sort by date (nearest upcoming first), hide expired, search by a word of the provider's folder: the expected list in 100 % of the checks (SC-009).

### Tests (write first)

- [ ] T050 [P] [US4] `scripts/check-passwords-sort-filter.ts` for `src/lib/passwords/sortFilter.ts`: `sortByDate(headers, now)` (upcoming nearest first, then past most recent first, then undated; ties by title; a date alone sorts at 00:00 of the device's zone; a login sorts by `expiresAt`), `filterByType`, `filterByKind`, `hideExpired`; the combination of filter, search words and sort (scenarios 1 to 3)
- [ ] T051 [P] [US4] Extend `scripts/check-passwords-search.ts`: `folders: string[]` on `SearchableHeader`; "bahn" finds a ticket in `Tickets/Deutsche Bahn` whose title does not name the provider; after a rename the new name finds it; an entry at the root or in the trash has no folder name; whole path matches (FR-022)
- [ ] T052 [P] [US4] Rust: extend `src-tauri/src/passwords/items_kind_tests.rs` (or `service` tests) so `agent_search` matches the whole folder path and not only the direct folder (research R10) and carries `kind`; one test for the place rule in `src-tauri/src/passwords/pass/place_tests.rs`: ticket-like and dated gives `Tickets` or `Tickets/<provider>` and tag `Ticket`; a loyalty card, a voucher or an undated ticket gives the root; an existing folder of another case is reused, not duplicated (FR-019)

### Implementation

- [ ] T053 [US4] `src/lib/passwords/sortFilter.ts` (see T050) and `src/stores/passwordsListView.ts` (Pinia; sort mode, type, kind, hide-expired; modelled on `passwordsSelection.ts`; state kept while the vault is open, not in the route)
- [ ] T054 [US4] `src/components/passwords/ListFilters.vue` placed beside `TagFilter.vue` in `Toolbar.vue` (type, kind, sort by title or date, hide expired); `List.vue` builds `visible` from the store: filter, then `filterHeaders`, then the chosen sort (a query only filters; the matches keep the chosen sort, so the next event comes first as in US4 scenario 3); `ListItem.vue` shows the type icon, the kind and the start (`formatStart`) and the expired mark (FR-010)
- [ ] T055 [US4] `src/lib/passwords/search.ts`: add `folders` to `SearchableHeader`, `KEYS`, `fieldOf` and `abbreviates`; `List.vue` and `ReferencePicker.vue` build the folder names once per store change with `groupPath` from `tree.ts`; the Rust `agent_search` in `src-tauri/src/passwords/service/items.rs` matches the whole path (`agent_headers` carries the path, `AgentHeader.folder` stays the direct name)
- [ ] T056 [P] [US4] `src-tauri/src/passwords/pass/place.rs` (the rule of FR-019, pure) and the command `passwords_pass_place_proposal`; used by the import and capture dialogs; creates nothing
- [ ] T057 [P] [US4] i18n and a11y for the filters in `de.json` and `en.json` (list filters, sort labels, hide expired); the controls work at 360 px
- [ ] T058 [US4] E2E scenario `scripts/e2e/scenarios/passwords-pass-organize.test.ts`: twenty tickets, five cards; filter, sort, hide expired, search by folder word; folder rename takes effect
- [ ] T059 [US4] Gates and pull request B part 2 (US2 and US4 land together as slice B if the review prefers one pull request; otherwise B1 = T024 to T040 and B2 = T050 to T058)

**Checkpoint**: The list answers "only tickets, next event first" in two choices.

---

## Phase 7: User Story 5 - See what changed on a ticket and keep the file (Priority: P2)

**Goal**: The history shows a changed date; a PDF or photo is attached and opens in the viewer.

**Independent Test**: Change the start, read the history stand as a changed field; attach a PDF and open it; restore an old stand.

- [ ] T060 [P] [US5] `src/components/passwords/HistorySnapshot.vue`, `HistoryTimeline.vue` and `HistoryTab.vue` render a pass stand: changed fields `start`, `kindOfPass`, `codes` with labels, code values only after "Show" (command `passwords_history_reveal` with `{ code }`, `usePasswords.revealAsync` pattern, as in `HistorySnapshot.vue`), start shown with `formatStart`; version 1 stands render as before
- [ ] T061 [P] [US5] Test in `src-tauri/tests/passwords_pass_history.rs`: change the start of a pass and read the stand as `start`; restore an old stand returns the fields and codes and leaves parent and folder; stands written before the migration still list and read; attach a PDF to a pass and open it through `passwords_attachment_preview` (spec 058 viewer)
- [ ] T062 [US5] `PassEditor.vue` and `PassView.vue` show `Attachments.vue` (add, open, remove; limit 25 MiB, 034 FR-020) once the pass has an id; check `scripts/e2e/scenarios/passwords-attachments.test.ts` still passes unchanged

---

## Phase 8: User Story 6 - Import a pass from a pkpass file (Priority: P2)

**Goal**: A pkpass becomes a pass with code, dates, fields and logo; the dialog proposes the place, offers a link and an optional signature check.

**Independent Test**: Import one generated file of each pass style; check kind, dates, codes and fields, and that anything not stored is in the report; import the same file twice; turn on "Check signature".

### Tests (write first)

- [ ] T063 [P] [US6] `src-tauri/src/passwords/pass/pkpass_tests.rs` with fixtures built in code (a helper `build_pkpass(style, …)` writing a ZIP with `pass.json`, images, `manifest.json`; throwaway key for the signature, no real pass): each of boardingPass, eventTicket, storeCard, coupon, generic gives the right kind, provider, start with offset as zone, end as date, codes with formats (also the legacy single `barcode`), fields and back fields as free fields, logo as icon; localised strings by UI language with base fallback; a code in an unknown format is stored as text with `flag = unknown_format`; `webServiceURL` is never fetched (assert no network use in the reader); limits and refusals: path with `..` or absolute path, too many entries, an entry or total over the limit, an inflated compression ratio, a file that is no ZIP or has no `pass.json` give `pass.not_a_pass_file` and leave nothing behind (FR-033)
- [ ] T064 [P] [US6] `src-tauri/src/passwords/pass/signature_tests.rs`: with a throwaway root and a manifest signed by it (test only), the check returns `valid` with the issuer name; a changed file returns `invalid` with the reason; a signature chaining to no known root returns `not_checkable` with the reason; none of the results stops the import; the check uses no network

### Implementation

- [ ] T065 [US6] `src-tauri/src/passwords/pass/pkpass.rs` (research R6): open the ZIP with the existing `zip` crate under hard limits (entry count, per-entry and total uncompressed size, compression ratio, name sanitising), map `pass.json` as in FR-028, build `source_id = passTypeIdentifier/serialNumber`, return a `PassImportPreview` model with a `notKept` list; the reader never executes, extracts to disk or calls a URL; temporary data only in memory or the vault's own temporary space (spec 084, FR-035)
- [ ] T066 [US6] `service/pass.rs` and `commands/pass.rs`: `passwords_pass_import_preview { file, checkSignature }` (duplicate check by `source_id` among non-trash entries, the place proposal of `place.rs`), `passwords_pass_import_save { previewId, choices }` (creates folder and tag when confirmed, reusing existing ones of the same name with case ignored; same transaction as create; report entry without any code), `passwords_pass_import_cancel`; the preview registry removes temporary files on save, cancel and drop; several files are several calls and one failing does not stop the others (FR-031)
- [ ] T067 [US6] `src-tauri/src/passwords/pass/signature.rs` (research R6, last task of the slice): add `cms` and `x509-cert` (RustCrypto) to `src-tauri/Cargo.toml` with a comment citing spec 092; verify the detached PKCS#7 `signature` over `manifest.json` and the manifest's SHA-1 hashes of the files, offline, against public Apple root and intermediate certificates embedded with `include_bytes!` from `src-tauri/src/passwords/pass/roots/` (public data, with a README naming source and date; there is no `bundle.resources`, so a resource directory would not ship on Android), the chain checked at the signing time of the CMS signature, not today; returns `valid`, `invalid` or `not_checkable` with issuer and reason; never decides the import. Build both feature sets and the Android target
- [ ] T068 [US6] `src/components/passwords/PassImportDialog.vue` and an entry in the import wizard route (`ImportWizard.vue` keeps its list; add the pkpass source with extension `.pkpass` and open the dialog instead of the bulk preview): shows kind, dates, codes by label, fields, the proposal (folder and tag, changeable), "Belongs to" picker (T046, probable match first, never automatic), the signature option "Check signature" (off by default; the result shows before saving and never blocks), the duplicate notice with "skip" and "add anyway", the report with what was not kept and a save-as-text action; multiple files processed one by one
- [ ] T069 [P] [US6] i18n for the import dialog, report and signature result in `de.json` and `en.json`
- [ ] T070 [US6] E2E scenario `scripts/e2e/scenarios/passwords-pass-import.test.ts` using a pkpass generated by the scenario helper: import with the proposal for a dated ticket (folder `Tickets/<provider>`, tag `Ticket`) and for a store card (root); the same file twice asks; offline throughout (FR-029)
- [ ] T071 [US6] Gates and pull request D; the pull request description names that the signature check ships behind the optional switch

---

## Phase 9: User Story 7 - Capture a code by camera, photo or PDF (Priority: P2)

**Goal**: A code on a card or printout becomes a code of a pass after the user confirms.

**Independent Test**: Scan a printed code of each of the five formats, then read the same from a photo and from a PDF; check format and value.

- [ ] T072 [P] [US7] `src-tauri/src/passwords/pass/codes_scan_tests.rs`: `decode` on a photo with two codes returns both; on a blurred image returns none without error; the command `passwords_pass_codes_scan { image }` returns `[{ format, value }]`, saves nothing and logs no value
- [ ] T073 [US7] `passwords_pass_codes_scan` in `commands/pass.rs` and `service/pass.rs` (uses `codes::decode`); `src/components/passwords/PassCapture.vue`: choose camera, photo or PDF; for a PDF render every page up to a bound (20 pages, one at a time) with `pdfjs-dist` to a canvas and send each PNG (research R15); a longer PDF lets the user choose the pages; show format and value, let the user pick when several are found, ask the kind of pass when nothing says (proposal: root), save only after confirmation into a new pass or an existing one; a value is stored as text and never opened or called
- [ ] T074 [US7] Camera path: generalise the session of spec 085 so the decoder and `accept` are passed in (today `accept(text)` accepts link texts only) in `src/components/onboarding/LinkScanner.vue` / `src/lib/sync/cameraScan.ts` or a new `src/lib/passwords/codeScan.ts`; Linux uses the native path (`src-tauri/src/link_camera`) with frames also offered to the `rxing` reader (new command or a flag on `link_camera_start`; the link flow keeps `rqrr`); Android uses `src/composables/useQrScan.ts` extended with a formats list for `@tauri-apps/plugin-barcode-scanner` (QR, Code 128, EAN-13, PDF417, Aztec) returning text and format; extend `scripts/check-camera-scan.ts` (fake camera and clock, new frames counted) for the generalised session
- [ ] T075 [P] [US7] i18n for capture in `de.json` and `en.json`; where no camera is usable the dialog offers photo, PDF and typing (scenario 3)
- [ ] T076 [US7] Verify the plugin's formats on the emulator and a phone (Android) and note the result in `research.md` R15; Windows and macOS stay "untested" in the quickstart until a device is at hand
- [ ] T077 [US7] Gates and pull request E (decode tests, `pnpm check:settings` for the camera check, `pnpm check:passwords`, templates, lint)

---

## Phase 10: User Story 8 - Share a ticket to holzi on Android, open it from the home screen (Priority: P3)

**Goal**: Share a pkpass, PDF or image to holzi and get the import or capture dialog; pin a pass to the launcher.

**Independent Test**: On the emulator or a phone, share a pkpass with the vault locked and unlocked; pin a pass and open it from the launcher.

- [ ] T078 [US8] Share target per [contracts/android-intents.md](./contracts/android-intents.md): the `ACTION_SEND` / `ACTION_SEND_MULTIPLE` intent filter in `src-tauri/gen/android/app/src/main/AndroidManifest.xml`; `MainActivity.kt` (`onCreate` and `onNewIntent`) copies each `EXTRA_STREAM` at once into the app's cache directory (swept of stale copies at start) (refuse over 25 MiB), forwards the paths through `HolziAndroidPlugin.kt` to the Rust inbox; a Kotlin buffer for the paths of a cold start (the plugin pulls them in `load`) and a new in-process store `sharedFiles` in `src-tauri/src/shared_files.rs` hold them until a vault is open, nothing imported before (`deep_link/inbox.rs` is not reused); `src/composables/useSharedFile.ts` with `src/stores/shareInbox.ts` opens `PassImportDialog` for a pkpass and `PassCapture` for a PDF or image after unlock; cancel or finish removes the copy (FR-035). Unit test of the store (park, take after unlock, remove on cancel, sweep) in `src-tauri/src/shared_files_tests.rs`
- [ ] T079 [US8] Pinned shortcut: plugin command `pinPassShortcut { id, label }` in `HolziAndroidPlugin.kt` (`ShortcutManager.requestPinShortcut`, intent opens `haex-vault://pass/<id>`), Rust side in `src-tauri/plugins/holzi-android/src/android.rs` with `unsupported` elsewhere; `tauri.conf.json` `plugins.deep-link.mobile` gets `{ "scheme": ["haex-vault"], "host": "pass" }` (the manifest intent filter is generated from it); `src-tauri/src/deep_link/mod.rs` `known_link` accepts `haex-vault://pass/<uuid>` (16 KiB limit unchanged) and its test; `useDeepLink.ts` dispatches by host (contact or pass) and routes it through the inbox to `/entry/<id>` after unlock; the "Pin to home screen" action in `PassView.vue` asks for a label (neutral default text from i18n, never the title); the shortcut holds no code and no title (FR-038). Tests: `known_link` accepts the new form and rejects a malformed id
- [ ] T080 [US8] Emulator checks with `adb` per the contract (SEND with the vault locked and unlocked, a second share to a running app, pin and open a shortcut); record results; list the real-phone checks as manual steps in `quickstart.md` (brightness restored on leave, keep-awake, camera formats, shortcut label)
- [ ] T081 [US8] Gates and pull request F; note in the description that speed goals wait for biometric unlock (FR-039)

---

## Phase 11: Polish & cross-cutting

- [ ] T082 [P] 360 px check (SC-010): every new view and dialog (`PassView`, `PassEditor`, `PassCodeLarge`, `PassLinks`, `PassImportDialog`, `PassCapture`, `ListFilters`) without horizontal scrolling; extend `scripts/e2e/scenarios/passwords-narrow-window.test.ts`
- [ ] T083 [P] Session restore (FR-040): a pass is a location in the tab and returns with the session without a code on screen; a code shown large ends when the user leaves or the tab closes; extend `scripts/e2e/scenarios/passwords-session-restore.test.ts` and `scripts/check-passwords-tabs.ts`
- [ ] T084 [P] Live update (FR-041): an edit of a pass on another device or window shows without overwriting an open editor's draft; extend `scripts/e2e/scenarios/passwords-sync-two-devices.test.ts`
- [ ] T085 [P] In-app texts are the user docs: review all `passwords.pass.*` strings in German and English for parity and tone (the app is called "Wallet" in German too; "Karte/Ticket" names one entry, not "Pass"; no bare "Relay"); run `nix develop --command pnpm exec prettier --write` on every touched JSON and Markdown file
- [ ] T086 Full-slice regression: `pnpm check:passwords`, `pnpm check:settings`, `pnpm check:templates`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`, `pnpm lint:rust`, the targeted Rust test targets of all slices, then the e2e scenarios `passwords-*` once on the built app; record the numbers in the final pull request
- [ ] T087 Walk through [quickstart.md](./quickstart.md) slice by slice on a real vault copy; fix what differs; tick the manual items that need a phone and leave the others named as open in the final report (open manual checks stay listed)
- [ ] T088 Update the memory index entry for spec 092 (stages merged, open manual checks) after the last pull request lands

---

## Dependencies and order

```text
Phase 1 Setup
  └─ Phase 2 Foundational (slice A)  ──────────────┐
       └─ Phase 3 US1 (acceptance of slice A)       │
            └─ Phase 4 US2 (slice B part 1)         │
                 ├─ Phase 5 US3 (slice C)  ── needs the editor of US2
                 ├─ Phase 6 US4 (slice B part 2) ── needs headers and views of US2
                 ├─ Phase 7 US5 (history view) ── needs the pass service of US2 and version 2 of Phase 2
                 ├─ Phase 8 US6 (slice D) ── needs US2 (create), US4 (place rule T056), US3 (picker T046)
                 ├─ Phase 9 US7 (slice E) ── needs US2 (codes decode, T028)
                 └─ Phase 10 US8 (slice F) ── needs US6 (shared pkpass) and US7 (shared image); T035 needs T034
Phase 11 Polish after the slices it covers
```

- Inside a phase, a test task comes before its implementation and must fail first.
- `T005` to `T007` before `T009`; `T009` before `T010`, `T013`, `T015`; `T028` before `T029`; `T029` before `T030`; `T030` before `T033`, `T036`; `T044` before `T045`; `T056` before `T068`; `T065` before `T066`; `T066` before `T068`.

## Parallel opportunities

- Phase 1: T002, T003, T004 together.
- Phase 2 after T009: T011, T012, T013, T017, T018 touch different files; T015 and T016 in one backend stream.
- Phase 4: tests T024, T025, T026 together; then T032, T033, T034, T038 (frontend) in parallel with T027 to T030 (backend).
- Phase 5: T041, T042, T043, then T045, T046, T047.
- Phase 6: T050, T051, T052 together; T056, T057 in parallel with T053 to T055.
- After Phase 4 the stories US3, US4, US5, US6 (reader tasks T063, T064, T065, T067), US7 (T072) and US8's Android code (T078, T079) can run as separate streams by different people; the pull requests stack in the order A, B, C, D, E, F.

## Implementation strategy

- **MVP** = Phase 2, US1 and US2 (slice A and B part 1): a user can create a card or ticket, show its code large and bright, with nothing existing changed. Ship A and B first.
- **Next**: US4 (find tickets in order) and US3 (linking) turn the pile into an organised set; then US6 (pkpass import), US7 (capture) and US8 (Android) make it quick.
- **Gate discipline**: every slice runs the gates of T020/T040 and its quickstart section before its pull request; a slice that cannot finish a manual check says so in the pull request and keeps it listed.
- **Risks to watch**: the `rxing` verdict (T004) decides R3; the pkpass signature (T067) is isolated so the reader can ship without it; Windows and macOS camera stay untested until a device is at hand (FR-036 names the limit).
