# Feature Specification: Marketplace for extensions

**Feature Branch**: `069-marketplace`

**Created**: 2026-10-28

**Status**: Draft

**Input**: "I would like to add the marketplace to haex-vault, the way we did it in haex-vault-legacy"
(operator, 2026-10-28, translated from German). HaexVault is to let users browse and install
extensions from a marketplace, the way haex-vault 2.x (legacy) does, but adapted to holzi. References:
haex-vault 2.x @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`
(`src/composables/useMarketplaces.ts`, `src/composables/useMarketplaceCatalog.ts`,
`src/components/haex/system/marketplace*.vue`, `src/components/haex/system/settings/extensions/marketplaces.vue`,
`src/stores/extensions/index.ts` `downloadAndPreviewAsync`).

## Clarifications

### Session 2026-10-28

- Q: Where does the network traffic to the marketplace run? → A: In the host (Rust), not in the webview:
  catalog, categories, download address and loading the install file including its checksum. The loaded
  bytes go straight into the existing install preview (spec 017), not through the webview. There is no
  second HTTP path in the frontend (one implementation per rule).
- Q: Where do the credentials of an own (private) marketplace live? → A: In the password manager. The
  marketplace points to an entry (pattern from spec 052); the marketplace list holds no secret. Auth types
  in this spec: none, bearer token, basic. DID sign-in as in the legacy app comes later.
- Q: How do we proceed and where does the UI live? → A: The spec first as a docs-only PR, then staged PRs.
  The catalog is an app of its own in the Window Manager (wm); managing the marketplaces lives under
  Settings → Extensions.

### Session 2026-10-28 (operator, after the plan)

- Q: How fixed is the default marketplace? → A: Not at all. The configuration is **generic**: every
  marketplace is an equal row of name, address and optional credentials; HaexVault treats no address and no
  operator specially. The operator provides a public marketplace; the entry for it is only a **preset** (an
  ordinary row that can be changed and deleted, which a new vault brings along).
- Q: Several marketplaces? → A: As many as you like, used **at the same time**. Users may also host a
  marketplace themselves. The marketplace protocol is therefore a documented, open contract of this
  repository (FR-034), no longer just "the haex-space interface".
- Q: What happens when two marketplaces list the same extension? → A: Both entries appear, with their
  source; nothing is merged, so a self-hosted marketplace cannot cover the entry of another one.

### Session 2026-10-28 (plan)

- Q: Does the marketplace reliably deliver checksum and size? → A: Yes. The service
  `marketplace.haex.space` delivers `bundleHash` (SHA-256, hex) and `bundleSize` for every version and a
  short-lived signed address on another host for every download. The checksum therefore becomes mandatory
  (FR-013); without it nothing is installed.
- Q: Is there a size limit for `.xt`? → A: Yes, 64 MiB (spec 017, `limits::ARCHIVE_BYTES`). The download
  keeps the same limit (FR-014).

## Terms

- **Marketplace**: a catalog service that speaks the **marketplace protocol**
  ([contracts/marketplace-api.md](contracts/marketplace-api.md)) and offers extensions (spec 017) with
  description, publisher, category, versions and a download address. To HaexVault a marketplace is only a
  row of name, address and optional credentials; all are equal. Anyone can run one: the project (public),
  a company or the user themselves.
- **Preset**: the entry for the project's public marketplace that a new vault brings along. It is an
  ordinary row and can be changed, switched off and deleted.
- **Catalog**: the shared list of the extensions of all enabled marketplaces, at the same time.
- **Catalog entry**: an extension in the catalog with the marketplace's details (name, short description,
  icon, publisher, category, rating, number of downloads, available versions).
- **Source**: the marketplace a catalog entry comes from.
- **Credentials entry**: the entry in the password manager (spec 034) that an own marketplace points to.
- **Install preview**: the existing check and confirmation when installing an extension (spec 017 US1,
  `InstallDialog`): publisher, declared permissions, update or downgrade. It is the same for files and for
  the marketplace.

## Relation to existing specs

- **Spec 017** (extension host): This spec changes nothing about signature, permissions and install
  preview. An extension from the marketplace is an ordinary `.xt` bundle; whether it is trustworthy is
  decided by the signature in the bundle and the user's confirmation, **not** by the marketplace's
  "verified" claim. Installing from a file stays unchanged.
- **Spec 034** (password manager) and **spec 052** (choosing credentials): Choosing the credentials entry
  of a marketplace uses the same picker as the storage connections; the rules for the delete warning and
  shares from 034 and 052 apply accordingly.
- **Spec 024** (sync) and **spec 023** (settings): The marketplace list is vault data and reaches all own
  devices; there is no save button in the settings, changes apply at once (exception: the form of a
  marketplace, see FR-021).
- **Spec 021** (MCP server) and **specs 018/047** (agent tools): Agents and extensions can neither query
  the catalog nor install (see FR-030).
- **ADR-0004**: The catalog is a holzi function of its own, not a host function for extensions.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Find an extension in the catalog and install it (Priority: P1)

Anna opens the app "Erweiterungen entdecken" in HaexVault. She sees the shared catalog of her
marketplaces as tiles with icon, name, short description, publisher and rating. She filters by the category
"Produktivität", searches for "Kalender", opens the details of an extension and taps "Installieren".
HaexVault loads the install file and shows the same install preview as when installing from a file:
publisher, permissions, confirmation. After "Installieren" the extension appears among her apps, and the
tile shows "Installiert".

**Why this priority**: This is the core: without a catalog the user has to find `.xt` files themselves,
download them and install them by hand.

**Independent Test**: With a test marketplace (local server with two extensions, one signed `.xt`), open the
catalog, choose an extension, install it, confirm in the install preview; the extension runs as an app and
the tile shows "Installiert".

**Acceptance Scenarios**:

1. **Given** at least one enabled marketplace is reachable, **When** Anna opens the app, **Then** she sees
   the first page of the shared catalog: sources in their rank order, entries within each source by
   downloads descending, with categories to filter by.
2. **Given** the catalog is open, **When** Anna types into the search field, **Then** the list updates
   after a short pause (not after every letter) and the page jumps to page 1.
3. **Given** Anna chooses a category, **When** she also searches, **Then** both filters apply together;
   "Alle" lifts the category filter.
4. **Given** the catalog has more hits than fit on one page, **When** Anna pages, **Then** the next page
   loads without blocking the rest of the app.
5. **Given** Anna taps "Installieren" in the details, **When** the file is loaded, **Then** the install
   preview appears with publisher, permissions and the marketplace the file came from; only "Installieren"
   there installs.
6. **Given** Anna cancels the install preview, **When** she closes the dialog, **Then** nothing is
   installed and nothing is left on disk.
7. **Given** the installation succeeded, **When** Anna returns to the tile, **Then** it shows "Installiert"
   with the installed version.
8. **Given** Anna has two enabled marketplaces, **When** she opens the catalog, **Then** she sees the
   entries of both at the same time, every tile names its marketplace, and a "Quelle" filter narrows it
   down to one.
9. **Given** both marketplaces list an extension of the same name, **When** Anna looks at the catalog,
   **Then** both tiles appear with their source, and Anna chooses which one to install from.

---

### User Story 2 - Safe: only verified bytes are installed (Priority: P1)

The marketplace is a third party. HaexVault must not install blindly what it delivers. HaexVault compares
the checksum the marketplace gives for a version with the loaded file; if it does not match, or is missing,
it aborts. Independently of that, the install preview checks the signature in the bundle as always. If the
marketplace claims a publisher is "verified", that is only a display.

**Why this priority**: The marketplace operator (or an attacker on the network) must never bring code into
the user's vault without their knowledge.

**Independent Test**: Test marketplace with (a) a wrong or missing checksum, (b) an unsigned bundle,
(c) a bundle with a different publisher than displayed, (d) a far too large file: in all cases nothing is
installed and the user sees the reason.

**Acceptance Scenarios**:

1. **Given** the marketplace's checksum does not match the loaded file, **When** the download ends,
   **Then** HaexVault aborts, installs nothing and reports "Die geladene Datei stimmt nicht mit der Angabe
   des Marketplace überein"; if the checksum is missing, it reports "Der Marketplace liefert keine
   Prüfsumme".
2. **Given** the bundle is invalid or unsigned, **When** the install preview checks it, **Then** it shows
   the same error as when installing from a file (spec 017) and allows no installation.
3. **Given** the publisher key in the bundle differs from the publisher in the catalog, **When** the
   preview appears, **Then** it shows the key from the bundle as publisher and points out the difference
   from the catalog.
4. **Given** the marketplace delivers a file larger than 64 MiB (FR-014) or more bytes than stated,
   **When** the download runs, **Then** it aborts as soon as the limit is exceeded.
5. **Given** the marketplace answers with a redirect to another host, **When** HaexVault follows the
   redirect, **Then** it sends no marketplace credentials there.

---

### User Story 3 - Update and remove (Priority: P2)

Anna has installed "Kalender" in version 1.2.0; the catalog lists 1.3.0. The tile shows "Update
verfügbar". A tap opens the install preview in "Update" mode (data is kept, the permission changes are
shown). Via "Entfernen" in the details Anna starts the existing removal (spec 017), including the question
whether the extension's data is kept.

**Why this priority**: Without an update hint the user stays on old versions, but the first installation
(US1) already carries the feature.

**Independent Test**: Test marketplace with two versions of an already installed extension: the tile shows
"Update verfügbar", the update runs through the preview, afterwards "Installiert" with the new version;
"Entfernen" removes the extension after confirmation.

**Acceptance Scenarios**:

1. **Given** the installed version is older than the newest version in the catalog, **When** Anna opens
   the catalog, **Then** the tile shows "Update verfügbar" with both versions.
2. **Given** the installed version is equal or newer, **When** Anna looks at the tile, **Then** no update
   hint appears.
3. **Given** Anna taps "Aktualisieren", **When** the file is loaded, **Then** the preview shows "Update von
   A auf B" and which permissions are new; the extension's data stays.
4. **Given** the catalog lists several versions, **When** Anna chooses an older one in the details,
   **Then** the preview shows the downgrade confirmation as in spec 017.
5. **Given** an extension is installed, **When** Anna chooses "Entfernen" in the details, **Then** the
   existing removal runs with the question about the data.
6. **Given** the user opens the list of installed extensions (settings), **When** the catalog reported a
   newer version earlier, **Then** the row shows "Update verfügbar"; a tap opens the catalog entry.
7. **Given** Anna has installed an extension that is not on the catalog page currently shown (for example
   because of a search), **When** she opens the app or taps "Aktualisieren", **Then** the update is known
   anyway, and the list of installed extensions shows it.

---

### User Story 4 - Manage marketplaces (Priority: P2)

Under Settings → Extensions → Marketplaces Anna sees a list of her marketplaces, in a new vault with the
preset. All rows are equal: she switches each one on or off (applies at once), changes the order, adds a
new one with name and address, edits and deletes any of them. Whoever hosts a marketplace of their own
(company, club, home server) enters its address, and it works next to the others without HaexVault having
to know anything about it.

**Why this priority**: Important for companies and self-hosters, but the preset is enough for the first
use (US1).

**Independent Test**: Add a second marketplace, switch it off, switch it on, move it up, delete it; the
catalog and the order of the sources follow each time at once.

**Acceptance Scenarios**:

1. **Given** a new vault, **When** Anna opens the list, **Then** the preset is there as an ordinary row,
   enabled, without her having set anything up.
2. **Given** Anna adds a marketplace with name and address, **When** she confirms, **Then** HaexVault
   checks that the address is reachable and shows the result ("erreichbar, N Erweiterungen" or the cause
   of the error); she can save anyway.
3. **Given** a marketplace is switched off, **When** Anna opens the catalog, **Then** it contributes
   neither entries nor categories nor errors.
4. **Given** two marketplaces, **When** Anna moves one up, **Then** their arrangement in the catalog
   changes at once.
5. **Given** the preset, **When** Anna edits, switches off or deletes it, **Then** it works like with any
   other marketplace; HaexVault never re-creates it on its own or switches it on again.
6. **Given** Anna deletes a marketplace, **When** she confirms, **Then** it disappears from the list and
   from the catalog; already installed extensions stay untouched.
7. **Given** Anna added a marketplace on device A, **When** device B has synced, **Then** it is in the
   list there as well (credentials via the entry in the password manager).
8. **Given** Ben runs a marketplace of his own following the marketplace protocol at any address (also
   with a path part, also `http://` on the local network), **When** Anna enters the address, **Then** his
   extensions appear in the catalog next to those of the others, without any change to HaexVault.
9. **Given** Anna has no marketplace (vault created before this feature or all deleted), **When** she
   opens the catalog, **Then** the empty state explains this and offers "Öffentlichen Marketplace
   hinzufügen" (the preset) and "Eigenen Marketplace hinzufügen".

---

### User Story 5 - Private marketplace with credentials (Priority: P3)

A team runs a marketplace of its own behind a sign-in (bearer token or user name and password). When
adding it, Ben chooses an entry from the password manager in the form instead of a token (or creates a new
one there). The marketplace points to this entry; the list and the vault data hold no secret.

**Why this priority**: Only needed for closed marketplaces; a public marketplace needs no sign-in.

**Independent Test**: Test marketplace that answers 401 without a valid Authorization header: without
credentials the catalog shows "Anmeldung nötig" for this source, with the chosen entry it loads.

**Acceptance Scenarios**:

1. **Given** Ben chooses "Bearer" as auth type, **When** he opens the form, **Then** he can choose a
   fitting password entry (the password field is the token); for "Basic" it is user name and password.
2. **Given** Ben confirms the form, **When** HaexVault checks the connection, **Then** it sends the
   credentials and reports success or "Anmeldung abgelehnt".
3. **Given** the credentials entry was deleted in the password manager, **When** the catalog loads,
   **Then** the source shows "Zugangsdaten fehlen" with a jump to the form; the other sources keep loading.
4. **Given** the credentials entry has not been synced to this device yet, **When** the catalog loads,
   **Then** the source shows "wird synchronisiert" instead of an error (spec 052 edge case).
5. **Given** Ben wants to delete an entry in the password manager that a marketplace uses, **When** he
   deletes it, **Then** HaexVault warns and names the marketplace (spec 034 FR-034).
6. **Given** the address starts with `http://` and credentials are chosen, **When** Ben confirms,
   **Then** HaexVault refuses to save, pointing out that credentials only travel over `https://`.

---

### Edge Cases

- **Offline or marketplace not reachable**: The catalog shows an error line for the source with the cause
  and "Erneut versuchen"; other sources load. If all sources failed, there is an empty state with "Keine
  Verbindung", not "Keine Erweiterungen".
- **All marketplaces switched off**: Empty state with a pointer to the settings.
- **No marketplace set up**: Empty state with "Öffentlichen Marketplace hinzufügen" and "Eigenen
  Marketplace hinzufügen" (US4-9).
- **The same extension in two marketplaces**: Both entries appear with their source, nothing is merged
  (FR-012). The preview shows the publisher key from the bundle, so a planted entry can be recognised.
- **Marketplace delivers garbage** (no valid format, missing fields): The source shows "Antwort nicht
  verständlich"; an entry with missing required fields is left out, not the whole catalog.
- **Slow source**: A source that does not answer within a fixed deadline counts as failed and does not
  hold up the others.
- **Page and total with several sources**: Each source delivers the same page number with up to 20
  entries. The app appends the results in source rank order; `totalPages` is the largest page count of the
  sources, `total` the sum of their hits. A shared page can therefore contain more than 20 entries.
  Paging stays stable (no entry disappears while paging).
- **Catalog entry already installed, but another publisher**: As in spec 017, the install preview points
  out "gleicher Name, anderer Herausgeber".
- **No space / write error during download**: Abort without leftovers; error message with the cause.
- **Download is cancelled** (user closes the app or the connection drops): Nothing installed, no half
  bundle; tapping again starts over.
- **Marketplace changes the version between details and download**: The preview shows the version that was
  actually loaded.
- **Two installations at once** (two tiles tapped in quick succession): One preview runs; the second
  waits or is rejected with a hint, never two dialogs on top of each other.
- **Vault locked or closed**: Locked during the download: abort without installation.
- **Mobile (Android, iOS)**: Single-column tiles, touch targets; installing from a file stays possible
  next to it.
- **Windows**: Key and path handling as with installing from a file; the download uses no file path in the
  webview.

## Requirements _(mandatory)_

### Functional Requirements

**Catalog**

- **FR-001**: HaexVault MUST offer an app "Erweiterungen entdecken" in the wm that shows the catalog of
  all enabled marketplaces (tiles with icon, name, short description, publisher, category, rating, number
  of downloads).
- **FR-002**: The catalog MUST support free-text search, filtering by category and paging; filters apply
  together, a filter change jumps to page 1, input in the search field is debounced by 300 ms.
- **FR-003**: The catalog MUST arrange the sources in their configured rank order and, within each source,
  sort the entries by downloads descending by default.
- **FR-004**: A tile MUST show the state "Nicht installiert", "Installiert (Fassung)" or "Update
  verfügbar". The comparison with installed extensions uses the name of the extension (the details
  additionally use the publisher key); the install preview decides in the end, based on the publisher key
  and name from the bundle (spec 017).
- **FR-005**: The details of an extension MUST show description, publisher (with the marketplace's
  "verified" claim as a display), rating, versions, category, tags and source, and offer "Installieren",
  "Aktualisieren" and "Entfernen" from there.
- **FR-006**: Texts and icons from the marketplace MUST be shown as plain text or as an image, never as
  active content (no HTML, no scripts). Icons are loaded and embedded by the host without credentials. URLs
  on the configured marketplace origin are allowed; external URLs must be `https` and publicly routable.
  Private or reserved IP addresses and redirects to them are blocked; the webview itself never calls an
  address of a marketplace.
- **FR-007**: The marketplace catalog MUST be usable without any chosen installation (view only); a
  request to a marketplace sends neither vault nor device identifiers nor the user's name.

**Install, update, remove**

- **FR-010**: "Installieren" and "Aktualisieren" MUST load the install file in the host and hand the bytes
  straight to the install preview (spec 017); there is no second installation routine.
- **FR-011**: For a marketplace origin the install preview MUST additionally name the source, but
  otherwise show publisher, permissions, update/downgrade and the confirmations from spec 017 unchanged.
  Nothing is ever installed without the user's confirmation.
- **FR-012**: The catalog MUST show the entries of all enabled marketplaces at the same time. Every entry
  names its source. Entries of different sources are **never** merged, not even with the same extension
  identifier or the same name (a source must not cover the entry of another). A "Quelle" filter narrows
  down to one marketplace; the order of the marketplaces determines the arrangement.
- **FR-013**: HaexVault MUST check the checksum (SHA-256) that the marketplace states for the version
  against the loaded bytes before the preview and abort on mismatch. If the marketplace delivers no
  checksum, the installation MUST be refused. In addition the loaded size MUST equal the stated size. The
  signature in the bundle is checked by the preview independently of this (spec 017).
- **FR-014**: The download MUST keep the 64 MiB limit for an install file (spec 017) and abort as soon as it
  is exceeded, without loading the whole file. Nothing is left on disk when nothing is installed.
- **FR-015**: The download MUST show the user progress and be cancellable.
- **FR-016**: "Entfernen" in the catalog MUST use the existing removal from spec 017 (including the
  question about the data).
- **FR-017**: On opening and on request ("Aktualisieren" button) the catalog MUST look for updates for
  **all installed extensions** at all enabled sources, also for those that do not appear on the page
  shown; there are no queries in the background or at start-up. Known update candidates are kept per
  installed extension and source. A source that fails does not delete its previously known candidates; a
  successful answer updates or deletes only the candidates of that source. The list of installed
  extensions (settings) MUST show the compatible candidate with the highest version; for equal versions
  the marketplace rank decides. A tap opens exactly this catalog entry. The hint shows the last known
  state without triggering a query itself.

- **FR-018**: If a version states a minimum or maximum version of HaexVault, the catalog MUST mark a
  non-matching version as "Benötigt HaexVault ab/bis x" and block installing that version; a matching
  older version stays selectable. The requests name the platform and the version of HaexVault, nothing
  else about the user or device (FR-007).

**Managing marketplaces**

- **FR-020**: Settings → Extensions MUST show the marketplace list with name, address, on/off and rank
  order; on/off and order apply at once without a save button (spec 023).
- **FR-021**: The form of a marketplace (name, address, auth type, credentials entry) MUST test the
  connection on confirming (reachability, sign-in) and show the result before it saves; the user can also
  save deliberately after a failure.
- **FR-022**: A new vault MUST contain the preset (the project's public marketplace) as an ordinary row.
  If a vault has no marketplace, the catalog MUST offer "Öffentlichen Marketplace hinzufügen". After the
  initial setup HaexVault creates no row without the user's action and switches none back on.
- **FR-023**: Every marketplace MUST be addable, editable, switchable on/off, reorderable and deletable,
  the preset included. Deleting does not remove any installed extension.
- **FR-024**: Addresses MUST be `https://` or `http://` and may have a path part
  (`https://example.org/market`); `http://` is allowed (self-hosted marketplaces on the local network),
  the form points this out.
- **FR-025**: The marketplace list MUST be vault data and reach all own devices via spec 024 (including
  the on/off state and the order).

**Credentials**

- **FR-026**: Auth types are "Keine", "Bearer" and "Basic". For "Bearer" and "Basic" the marketplace MUST
  point to a credentials entry in the password manager (choice as in spec 052; newly entered values are
  stored as a visible entry there). The marketplace list never holds a token or password.
- **FR-027**: Credentials MUST ONLY be sent to `https://` addresses and only to the marketplace's host,
  never to a redirect to another host and never to the address an install file is loaded from if it lies
  on another host.
- **FR-028**: If the credentials are missing (entry deleted, not synced, incomplete), the source MUST say
  so, without disturbing other sources.
- **FR-029**: When an entry in the password manager that a marketplace uses is deleted, the password
  manager MUST warn and name the marketplace (spec 034 FR-034).

**Open protocol**

- **FR-034**: The marketplace protocol MUST be in this repository as a contract
  ([contracts/marketplace-api.md](contracts/marketplace-api.md)), so that anyone can run a compatible
  marketplace. HaexVault MUST NOT assume anything about a marketplace beyond this protocol: no fixed
  address, no special treatment by name or address (except the preset as an entry), no fields that only
  one operator delivers.
- **FR-035**: A marketplace that delivers additional fields MUST NOT disturb HaexVault (unknown fields are
  ignored); a marketplace without optional fields MUST remain usable.

**Boundaries and errors**

- **FR-030**: Neither extensions nor agents (specs 021, 018, 047) MUST query the catalog, manage
  marketplaces or trigger an installation. Installing happens only through a deliberate action of the
  user in the UI.
- **FR-031**: Every source MUST be allowed to fail on its own: an error line with the cause (not
  reachable, timeout, sign-in rejected, answer not understandable, credentials missing) and "Erneut
  versuchen", without disturbing the catalog of the other sources.
- **FR-032**: All texts MUST exist in German and English, the product name is "HaexVault"; the UI follows
  the style of specs 035/036 (toolbar, large heading, groups and rows).
- **FR-033**: All functions MUST work on Linux, Windows, macOS, Android and iOS; logic that seems
  platform-dependent (download, paths) MUST be testable on Linux.

### Key Entities

- **Marketplace**: name, address, on/off, rank order, auth type and a reference to a credentials entry
  (no secret). Vault data, synced.
- **Catalog entry**: transient, not stored; the marketplace's details plus source and local install state.
- **Version**: a version of an extension in the catalog with download address, checksum and size.
- **Credentials entry**: entry in the password manager (spec 034); the marketplace points to it.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user finds and installs an extension from a marketplace in under a minute from opening the
  app to the running extension (without looking for files).
- **SC-002**: In no case is an extension installed without the confirmation in the install preview; files
  with a wrong checksum, an invalid signature or over the size limit are rejected 100 % of the time
  (automated test with a test marketplace).
- **SC-003**: If one of several sources fails, the catalog shows the others within the same time as
  without the failure plus at most the fixed deadline for one source, and names the failed source.
- **SC-004**: Nowhere in the vault database, in logs or in sync data is there a token or password of a
  marketplace (automated test).
- **SC-005**: Credentials never leave the device to a host other than the marketplace's and never over
  `http://` (automated test with a redirect and an `http` address).
- **SC-006**: A new vault contains the preset without any setup; it can be deleted and does not come back
  on its own; a vault without a marketplace offers it with one tap, without entries being created twice.
- **SC-007**: A marketplace added on device A appears on device B after the sync (E2E with two devices).
- **SC-009**: Two test marketplaces at different addresses (one with a path part, one with sign-in)
  deliver entries into one catalog at the same time; without a line of code that knows either address
  (e2e, Rust test).
- **SC-008**: The flow catalog → install runs through in E2E on desktop and in the Android emulator.

## Assumptions

- The **project's public marketplace** is at `https://marketplace.haex.space` (operator, 2026-10-29);
  this address is the preset and appears in one single place in the code. The protocol is the one that
  haex-vault 2.x and `@haex-space/marketplace-sdk` 0.1.6 use: list with search, category, page and
  sorting, categories with counts, details and a download address for a version. From this spec on it is a
  contract of this repository (FR-034); the operator's service follows it. The list call lacks the
  publisher key (it is only in the details); missing optional fields are treated as "not delivered", except
  the checksum (FR-013).
- The response formats are recorded in a contract (plan, `contracts/`) and checked in the host against test
  data; whether the existing `@haex-space/marketplace-sdk` serves only as a type source or is rebuilt in
  Rust is decided by the plan.
- The fixed deadline per source is on the order of ten seconds; the plan fixes the value.
- There is no offline catalog and no cache beyond closing the app; ratings and reviews are shown but not
  submitted.
- There are no automatic updates and no background queries (see FR-017); a later spec can add that if the
  user wants it.
- Not in this spec: publishing extensions, paid extensions, DID sign-in, writing reviews, installation by
  agents or extensions.
- Developer mode (spec 017) and installing from a file stay unchanged next to it.
