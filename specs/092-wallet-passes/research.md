# Research: Cards and tickets in the password manager (wallet passes)

Each entry gives the decision, the reason, and what was considered. File references are to the repository at the commit this
plan was written against.

## R1. Migration, trigger version and the sync gate (FR-001, FR-005, FR-042)

**Decision.** One migration `0061_passwords_pass` in a new file `migrations_passwords_pass.rs`: the column `kind TEXT NOT NULL DEFAULT 'login'` on
`haex_passwords_item_details` (an `ALTER TABLE ... ADD COLUMN`), `CREATE TABLE haex_passwords_pass_details`,
`CREATE TABLE haex_passwords_pass_barcodes` and plain indexes. `HOLZI_TRIGGER_VERSION` goes from 39 to 40 and its doc list
gets the line. Templates: `migrations_passwords_owner.rs` (column on this very table), `migrations_passwords_refs.rs` (new
table), `migrations_voice.rs` (ALTER and CREATE in one migration), and the test pattern of
`migrations_passwords_owner_tests.rs` and `tests/vault_upgrade.rs`.

**The gate (resolves Open Question 1).** The sync handshake compares three numbers for equality: protocol, the count of
holzi migrations and the CRDT trigger version (`sync/handshake.rs`, `local_schema()`; spec 024 FR-029). Migration `0061`
raises the count and the bump raises the trigger version, so an older build and a newer one refuse each other before any
pass row moves; both show the reason and nothing is lost. An old build also cannot open a vault file that holds a migration
it does not know (`haex-crdt` `MigrationMissingFromSource`, `instances/open.rs`; test `open_tests.rs` "refuses a vault that
holds a migration this build does not know"), which covers a copied or restored vault. Remote rows for an unknown table or
column are skipped or abort the pull and never raise the origin's progress (`sync/inbound.rs`).

**What stays for FR-042.** The gate compares versions, not meanings. A later type (payment card, identity) or a new kind of
pass that arrives **without** a migration would sync to an equal-version build and be read as something it is not. Two rules
close this: every new entry type ships with a migration (a real one or a documented no-op) so the gate stays the enforcement,
and the user interface reads an unknown `kind` as "needs a newer holzi" (never as an empty login, no edit, no write) and an
unknown kind of pass as "other" (FR-004). Both are tasks in slice A.

**Alternatives.** A separate `haex_passwords_passes` table with copies of every satellite (rejected in the spec: six foreign
keys, a second path through trash, search, grants and sync). A `type` column without default (rejected: existing rows would
need a rewrite and the trigger would not cover them).

**Existing rows.** `DEFAULT 'login'` is applied when the column is read for old rows (no rewrite). The vault upgrade test
opens a vault built with `migration_source_before("0061_passwords_pass")`, inserts a login, reopens with the production source
and asserts the column, the default, the two tables and that `update_trigger_sql` names `kind`.

## R2. History format version 2 (FR-023)

**Decision.** `SnapshotData` (`passwords/snapshots.rs`) gains an optional pass block (`kind`, kind of pass, start fields,
codes) with `#[serde(default)]`, and `CURRENT_VERSION` becomes 2.

**Gotcha found.** Today `version` is `#[serde(default = "current_version")]`, so a stored stand **without** a version reads as
the current version, and legacy haex-vault stands have none (`snapshots_tests.rs` "legacy state without version or OTP
parameters"). Raising the constant would label every old stand version 2. The default for a **missing** version therefore
becomes the literal 1, while `Default::default()` for a **new** state keeps using the current version. A test reads a version-less
stand, a version-1 stand and a version-2 stand.

**Wiring.** `build_current` selects the pass row and codes for a pass; `changed_fields` names `start`, `kindOfPass` and `codes`;
`get` returns the codes' labels and formats but not values; `reveal` returns one code value on request (like a free field);
`restore` writes the typed fields and the codes back but never the parent or the folder (FR-023); the stand equality check
(`parse == current`) includes the block, so a no-op save writes nothing. Constructors of `SnapshotData` elsewhere
(`import/mod.rs` `base_state`, `import/apply.rs`, `import/legacy_haex_vault/history.rs`) set no pass block.

## R3. Codes: one Rust module on `rxing` (FR-003, FR-008, FR-009, FR-034, SC-002)

**Decision.** A single module `passwords/pass/codes.rs` validates (format rules and check digits), draws (SVG, dark on white,
with the quiet zone) and decodes (from image bytes) all five formats using the `rxing` crate (Apache-2.0, pure Rust, encoders
and readers for QR, Code 128, EAN-13, PDF417 and Aztec). The user interface asks the backend for the SVG of one code through a
single-entry command, so the value never travels in a list and the drawing rule exists once.

**Why.** Memory rule "one implementation per rule": check digits and drawing in both Rust and TypeScript would drift. The
present code has only QR (`qrcode` for drawing, `rqrr` and `jsqr` for reading); nothing handles the other four formats. A round
trip in Rust (draw, rasterise, decode) is the proof SC-002 asks for and runs on Linux CI.

**Alternatives.** `bwip-js` in the frontend (draws everything, but a second implementation, a second validation, and a bigger
bundle; it cannot decode). A different Rust crate per format (`barcoders`, `qrcode`, `rqrr`): no PDF417 or Aztec reader.

**Risk and check.** `rxing`'s licence and size are checked in the first task of slice B; if it fails, the fallback is
`bwip-js` for drawing with the validation kept in Rust and a decode-only Rust path for photos. Link QR (spec 085) stays on
`rqrr` and is not touched.

## R4. Dates and time zones (FR-002, SC-008)

**Decision.** `pass_details` stores the start as one of two shapes: a date alone (`start_date`, `YYYY-MM-DD`), or a point in
time (`start_at`, UTC, RFC 3339 with milliseconds like every timestamp, `passwords/clock.rs`) plus `start_zone` (an IANA
name such as `Europe/London`, or a fixed offset such as `+02:00` when only an offset is known). The end is the existing
`expires_at`.

**Display and sort.** Conversion is a pure TypeScript module `passDate.ts` built on `Intl.DateTimeFormat` with the `timeZone`
option (IANA names) and plain arithmetic for fixed offsets; it shows the time of the place, and also the device time when the
two offsets at that instant differ (the comparison is at that instant, so the days around a daylight-saving change come out
right). Sorting uses the UTC instant, or the date at 00:00 of the device's zone for a date alone. Tests run in Node with fixed
zones (`TZ` is varied in the test) over the days around the 2026 changes in Europe and North America.

**Why not Rust.** There is no time-zone database crate in the build (`chrono-tz` and `jiff` are absent) and the webview has the
database in `Intl`; Rust only checks the shape of the stored strings. One display rule, in TypeScript.

**Edge cases.** A pkpass gives an offset, kept as the zone (FR-028); a typed start defaults to the device's zone and may be
changed; an unknown zone name is kept and shown as UTC with the name.

## R5. Sorting, filtering and list state (FR-010, FR-021)

**Decision.** Pure functions in `src/lib/passwords/sortFilter.ts`: `sortByDate(headers, now)` (upcoming nearest first, then
past most recent first, then none; ties by title), `filterByType`, `filterByKind`, `hideExpired`. They run in `List.vue` over
`store.headers` next to `filterHeaders` (search); sort and filter state lives in a new Pinia store
`passwordsListView.ts` (modelled on `passwordsSelection.ts`), so it stays while the vault is open (one process is one vault
session, spec 013) and is not part of the route.

**Why not the route.** `registry.ts` `isSecretFreeLocation` allows only the query keys `q`, `tag`, `edit` and `tab`; the filter
is not part of a place. The tag filter stays a query as it is.

**Header fields.** The header (a list row) gains `kind`, `passKind`, `startAt`, `startZone`, `startDate` and `parentId` (R8),
never a code. A login's date for sorting is its `expiresAt`.

## R6. pkpass reading and the optional signature check (FR-028 to FR-033)

**Reading.** `passwords/pass/pkpass.rs` opens the ZIP with the existing `zip` crate (deflate), reads `pass.json` and the images
with hard limits (file count, total and per-entry uncompressed size, compression ratio, names with `..` or absolute paths
refused, nothing executed), and maps it: the pass style to the kind of pass, `organizationName` to the provider,
`relevantDate` to the start (with its offset as zone), `expirationDate` to the end (as a date), `barcodes` (or the legacy
`barcode`) to codes, `fields`/`backFields` to free fields, the logo to the icon, localised strings by the UI language with the
base string as fallback, anything else to the report. The file's identity `passTypeIdentifier` plus `serialNumber` becomes
`pass_details.source_id` (R7) for duplicate detection (FR-031). `webServiceURL` is read as text and never called (FR-029).

**Signature (optional, off by default).** `passwords/pass/signature.rs` verifies the detached PKCS#7 `signature` over
`manifest.json` and the SHA-1 hashes of the files in the manifest, offline, against Apple's root and intermediate certificates
bundled as public data (the issuer name is shown whatever the result). It returns valid, invalid or not checkable with a reason
and never decides the import. It uses `cms` and `x509-cert` from RustCrypto (pure Rust, fits the existing `p256`, `rsa`,
`pkcs8`). This ships as its own task at the end of slice D so the reader can land without it.

**Alternatives.** `openssl` (a native dependency on every platform, against the every-device rule). Skipping the chain and only
checking the manifest hashes (kept as the "not checkable" fallback result).

**Test files.** A pkpass is built in the tests from fixtures generated with throwaway keys (no real pass in the repository).

## R7. Where the import identifier lives (FR-031)

**Decision.** A nullable `source_id TEXT` on `pass_details`, not a free field: the duplicate check is a query on it and it
must not be edited as a user field. It holds `passTypeIdentifier/serialNumber` and is not a secret. The check looks for the
same `source_id` among non-trash entries; the dialog asks. No UNIQUE constraint (034 FR-037); the check is in the service.

## R8. Linking (FR-013 to FR-017)

**Decision.** `parent_id` on `pass_details`, no foreign key (the parent may not have arrived yet by sync, and a foreign key
would make that an error; a remote delete is applied without cascade anyway). The service enforces:

- **Refusal at write**: a recursive query walks the parents of the chosen parent; if it reaches the pass, the write is refused
  (self and descendants, FR-013).
- **Clearing at delete**: `purge_item` runs `UPDATE pass_details SET parent_id = NULL WHERE parent_id = ?` before it deletes
  the entry's own rows (FR-015), and trashing a parent changes nothing.
- **States**: the header carries `parentId`; the client resolves it against the headers it has: found (open), found in trash
  ("(in trash)"), not found ("(not available)"). No second query.
- **Loops from sync** (two devices linking in opposite directions): `links.ts` walks parents with a visited set, marks every
  pass on a loop, and never recurses without a bound; the editor offers "Clear link" on each. The list is never blocked.

The "Linked" section is `headers.filter(h => h.parentId === id)` sorted by `sortByDate`. It has no storage of its own.
`ReferencePicker` excludes passes (a `{REF}` resolves login fields).

## R9. Routes, the New action and the type of an entry (FR-001, FR-011)

**Decision.** The type is chosen once, when creating. `/entry/new?type=pass` opens the pass editor; `type` is added to the
route whitelist for that one place (`registry.ts`, `scripts/check-passwords-routes.ts`), with only the value `pass` accepted.
`EntryPage.vue` dispatches on `detail.kind` for an existing entry and on the query for a new one; a `kind` it does not know shows
the "needs a newer holzi" panel (FR-042). The toolbar's "New" menu and the empty-area context menu of `List.vue` get the third
item (`passwords.newMenu.pass`). The pass editor and view reuse `EntryTabs`, `HistoryTab`, `KeyValues`, `Attachments`, the
dialogs and `Breadcrumbs`; the login-only Details tab has a pass twin (`PassDetails`). Window titles stay static (034 FR-040).

## R10. Search over folder names (FR-022)

**Decision.** In the frontend `search.ts`, `SearchableHeader` gains `folders: string[]` (the names on the entry's path, from
`groupPath` in `tree.ts` over `store.groups`), added to `KEYS`, `fieldOf` and `abbreviates`; an entry at the root or in the
trash has none; a renamed or moved folder takes effect at the next store update, which is the live reload already in place.
`List.vue` builds the folder list per header once per store change; `ReferencePicker` passes the same. The Rust agent search
(`service/items.rs` `agent_search`) matches the direct folder name today; it moves to the whole path so both searches agree
(the one rule, FR-022), and 034 FR-007 is amended.

## R11. Who sees a pass from outside, and the tag scope (FR-025 to FR-027a)

**Findings.** Nothing outside `passwords/` reads `haex_passwords_item_details` directly; every outside caller goes through
`PasswordsService`, so the filter belongs in about five places:

- `items.rs` `headers_in_scope` (feeds `list_headers`, `headers_for_page`, `list_autofill_shapes`, the agent search and
  remote-storage credentials) and `agent_headers`: a `kinds` parameter that **defaults to logins**; callers that want passes ask
  for them.
- `access.rs` `ItemState`: a kind check, so a single-entry read, a TOTP request or a passkey operation on a pass by an outside
  caller is refused.
- `service/items.rs` `read_secret_item` and `totp_code`: login only for outside callers, as defence in depth.
- the bridge's passkey `entries()`, and `passkey_create`, `link`, `authorize_existing`, `readable` in the passkey code: a pass
  never owns or links a passkey.
- `extensions/passwords.rs` `summary` carries the type and `list` takes a type filter (default logins);
  `remote_storage/credentials.rs` and `files/local/drives/unlock.rs` skip passes;
  `service/import.rs` `existing_keys` (duplicate detection of imports) counts logins only.

**Codes never leave through a header.** `ItemHeader`, `AgentHeader`, the extension summary, the history list and the tab
history carry no code; the only reads are one single-entry command for the owner (the user) (`passwords_pass_codes`) and the
history `reveal`. A test searches every read path for a seeded code value (SC-004).

**Tag scope, as the code is.** External agents (`haex_agent_profile_grants`, `mcp_server/grants.rs`), extensions
(`extensions/passwords.rs`) and the bridge (`haex_browser_clients`) enforce the tag scope per entry in `access.rs`, and the
built-in agent's browser fill takes the profile's `passwords.tags` (`agent_browser/env.rs`). All of them go through
`headers_in_scope` and `authorize_*`, so a pass is covered **without new code**. The built-in agent's own search is a
different case: it runs with no grants on purpose (`agent_actions.rs`, "searches without a grant (Z2) and never reads a
secret") and returns the narrowed header of every non-trash, non-owned entry. Passes follow that: the agent sees id, title,
kind, dates, tags and folder name of a pass, never a code. **This is a finding for the operator:** the spec text of the
clarification suggested that the built-in agent's search can already be limited by tag; it cannot. Limiting it is a separate
change (it would apply to logins too) and is not part of this plan; FR-027a is corrected to say so.

**Agent shape.** `AgentHeader` and `header_json` add `kind` (`login` or `pass`), `passKind`, `startAt`, `startZone`,
`startDate` and `expiresAt` for a pass; the tool schema in `chat/eval/tools.json` and `src/lib/actions/passwordsActions.ts`
gets an optional `type` filter ("login" default, "pass", "any") and the description says logins are the default.
`passwords_secret_get` refuses a pass.

## R12. Satellite tables that are listed by hand (FR-005, FR-018)

The sync applies a remote delete without foreign keys, so cascade is not enough; these lists name tables:

- `trash.rs` `purge_item` (and `purge_group`): add `pass_barcodes`, `pass_details`, and the parent clear (R8), children first.
- `copy.rs`: the insert into `item_details` names its columns (add `kind`), and a copy of a pass copies the detail row and
  the codes (new ids, same parent, `source_id` cleared so the copy is not a duplicate of the import).
- `items.rs` `create_item`, `update_item` (named columns) and `sets.rs` (per-column CRDT merge): a login's input leaves
  `kind` as `login`; a pass is written through its own service (R13).
- `import/apply.rs`, `references_db.rs` and `legacy_haex_vault/*` build `SnapshotData` and rows for logins only; they set no
  pass block and need no change beyond the compile fix for the new field.
- `src/lib/storage/overviewTables.ts` lists tables for the storage overview; the two new tables are not added (they hold no
  stored files).

Live update (FR-041) needs no registration: `vault_events.rs` reports every tracked table and `stores/passwords.ts`
subscribes to `haex_passwords_*`.

## R13. Service shape for passes

**Decision.** A separate `service/pass.rs` and `commands/pass.rs` (Tauri commands `passwords_pass_*`, contracts/) instead of
widening `create_item` and `update_item`, whose inputs are shaped like a login. A pass write is one transaction: the
`item_details` row (title, note, url, icon, colour, expiry; the login columns stay empty), the `pass_details` row, the codes,
the free fields, the tags, the folder, and one `take_snapshot`. Conflict handling uses the same `updated_at` token as
`update_item` (034). Code rows keep their ids across an edit (upsert by id, delete only the ids that are missing), so two replicas that edit different codes merge row by row instead of leaving two sets. The generated ts-rs bindings follow (`pnpm generate:ts-types`).

## R14. Screen brightness and keeping the screen on (FR-008, FR-036)

**Decision.** On Android, two commands in the `holzi-android` plugin (`@Command`s with `@InvokeArg` classes, `android.rs`,
no-ops in `other.rs`, following `set_secure`): `setScreenBright(on)` sets `window.attributes.screenBrightness` to full and
restores `BRIGHTNESS_OVERRIDE_NONE`, and `setKeepScreenOn(on)` sets `FLAG_KEEP_SCREEN_ON`. A composable
`useScreenBright.ts` turns both on when "Show large" opens and off when it closes, the tab closes or the window loses focus.
On desktop, the Screen Wake Lock web API is used where the webview has it; otherwise the rest still works (FR-008). Brightness
on desktop is not touched.

## R15. Capture (FR-034, US7)

**Decision.** One decode path in Rust (`codes.rs` on `rxing`) for photos and for pages of a PDF; the user interface sends image
bytes. The camera: on Android the existing `@tauri-apps/plugin-barcode-scanner` is asked for the five formats (it supports them;
the first task of slice E verifies this on the emulator and a phone) and returns text and format; on Linux the native camera
path of spec 085 (`link_camera`, v4l) is generalised so its frames can go to the `rxing` reader and not only to the link
decoder, because the web camera freezes on WebKitGTK; on Windows and macOS the webview camera with a frame loop decodes through
the same Rust command (spec 085 said these are untested; they stay "untested until checked" in the quickstart). The session
sheet of 085 is tied to the link flow (`accept(text)` accepts link texts only); a decoder and `accept` are passed in instead.
PDF: `pdfjs-dist` is already a dependency and spec 058 shows PDFs, so the page is rendered to a canvas in the webview and its
PNG goes to the decoder. A scan is saved only after the user confirms (FR-034), as text, and a value is never opened or called.

## R16. Android share target and pinned shortcut (FR-037 to FR-039)

**Share.** Intent filters for `ACTION_SEND` and `ACTION_SEND_MULTIPLE` (types `application/vnd.apple.pkpass`,
`application/pdf`, `image/*`) in `AndroidManifest.xml`; `MainActivity.kt` (launch mode `singleTask`, so a second share arrives
in `onNewIntent`) copies the shared `content://` stream **at once** into the app's own temporary space (the read permission of
the URI is transient) and forwards the path through the `holzi-android` plugin to the Rust inbox. The inbox is the pattern of
`deep_link/inbox.rs` and `stores/shareInbox.ts`: the file waits in the inbox, nothing is imported, and the import dialog opens
once a vault is open (spec 013: a cold process starts, the vault is locked, the user unlocks, the dialog opens). A cancelled or
finished dialog removes the copied file (FR-035). The file never leaves the device.

**Shortcut.** `ShortcutManager.requestPinShortcut` with a label the user types (default neutral, never the title) and an intent
carrying `haex-vault://pass/<id>` through the existing deep-link route into the same inbox; the id is an opaque UUID and no
code is in it. `deep_link/known_link` accepts the new path with the 16 KiB limit unchanged. Opening it with the vault locked
waits for the unlock and then opens `/entry/<id>`. A pass that no longer exists shows the usual "not found" page. Biometric
unlock stays out (FR-039).

## R17. Icons (FR-043)

Spec 053's icon sets serve the file browser only (`useFileIcons`, `files_icons_resolve`); the password manager uses literal
`lucide:` names that the Nuxt icon scan finds in `.ts` and `.vue` files. The pass types use literal names in a table in
`passType.ts` (already present in the app: `lucide:ticket`, `lucide:train-front`, `lucide:plane`, `lucide:wallet`,
`lucide:id-card`, `lucide:gift`, `lucide:award`, `lucide:store`). FR-043 is corrected in the spec. No new icon dependency.

## R18. Tests

- **Rust unit**: codes (validate, draw, decode round trip per format, bad check digit), pkpass (each pass style, limits, path
  traversal, bad zip), parent (cycle, self, clear on delete), place rule, snapshots (v1, v2, missing version), access (login-only
  default, pass refused), migration upgrade.
- **Rust integration** (`src-tauri/tests/`): `vault_upgrade.rs` (pre-0061 vault), `pre_rename_vault.rs` (every old entry is a
  login), `passwords_sync.rs` (two replicas: pass rows, codes, parent, delete markers), `passwords_access.rs` and
  `mcp_server_passwords.rs` and `browser_bridge_passwords.rs` (no pass in any credential lookup, SC-005), a read-path test that
  finds no code value anywhere (SC-004).
- **Frontend pure** (`scripts/check-passwords-*.ts`): dates and zones, sort and filter, links and cycles, search with folder
  names, routes (the new query key), tabs.
- **E2E** (`scripts/e2e/scenarios/passwords-pass-*.test.ts`): create and show large, link and trash, sort and filter, pkpass
  import; helpers in `scripts/e2e/lib/passwords.ts` get `createPass`.
- **Android**: emulator where it can (share target and shortcut intents via `adb`), manual otherwise (brightness, camera).
- Gates per slice: `pnpm lint:rust` (both feature sets), `pnpm typecheck`, `pnpm check:passwords`, `pnpm check:templates`,
  `pnpm format:check`, targeted `cargo test` (no whole-workspace build: memory "build only relevant test targets").
