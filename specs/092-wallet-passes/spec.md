# Feature Specification: Cards and tickets in the password manager (wallet passes)

**Feature Branch**: `092-wallet-passes`

**Created**: 2026-11-11

**Status**: Draft

**Input**: User description: "On Android and iOS there are wallets. What would we have to do so that the password manager
could fulfil these tasks as well?" (operator, 2026-11-11). Narrowed in the same conversation: the wallet function is a
second entry type next to the login entries of the password manager, with the same folders, tags, trash, history and
attachments, a link between entries, a pkpass import and barcode capture.

## Background

- **What the wallets of Android and iOS do for passes.** They hold loyalty cards, memberships, vouchers, event tickets,
  boarding passes and train tickets as a code (barcode or QR) plus a few fields, show the code large and bright at the
  counter or the gate, and surface the pass that matters now on the lock screen. Both are essentially a flat list; neither
  has user folders, and their search over contents is weak. They are also where payment cards, IDs and keys live, but those
  need vendor-attested hardware paths holzi cannot get (Not in scope).
- **The password manager already carries most of what a pass needs.** An entry (spec
  [`034`](../034-password-manager/spec.md)) has a title, note, address, icon, colour, expiry date, free fields, attachments,
  tags, a folder, a trash, a history and sync. What it lacks is everything that makes a card a card: one or more codes in a
  format, a date the pass starts, a kind, a way to show the code, and a way to belong to another entry (the ticket to the
  account it was bought with).
- **`haex_passwords_item_details` is shaped like a login.** Its columns are `username`, `password`, `otp_*` and
  `autofill_aliases`; a pass would leave seven of them empty. Six tables hang on `item_details(id)` by a foreign key (free
  fields, folder assignment, attachments, history, tag assignment, passkeys), and SQLite cannot point one foreign key at two
  tables. A separate table for passes would need a copy of each of them, and a second path through lists, trash, search,
  grants and sync.
- **The Bitwarden import already meets the problem.** It stores card, identity, secure note and SSH key as login entries
  with the tags `credit-card`, `identity` and `secure-note` and the data as free fields
  (`src-tauri/src/passwords/import/bitwarden.rs`). That was the right stop-gap for 034 and stays unchanged here; this spec
  builds the foundation that lets those types become real types later.
- **The list sorts by title only.** The password list sorts through `sortByTitle` (`src/components/passwords/List.vue`);
  the expiry date is shown on an entry and flagged when passed (034 FR-008), but nothing sorts or filters by it. Tickets
  need both.

## Relation to existing specs

- [`034-password-manager`](../034-password-manager/spec.md) and
  [`036-password-redesign`](../036-password-redesign/spec.md): This spec extends them. Everything said there about entries,
  folders, tags, trash, history, attachments, search, grants and sync holds for a pass unless this spec says otherwise.
  FR-036 of 034 lists the deviations from the data model of haex-vault 2.x as final; this spec adds to that list (FR-005),
  and FR-007 of 034 names the fields the search reads; this spec adds the folder names (FR-022). Both amendments go into
  034 with the plan.
- [`021-mcp-server`](../021-mcp-server/spec.md): The per-agent, tag-scoped grants cover passes (FR-025 to FR-027).
- [`049-external-bridge`](../049-external-bridge/spec.md) and
  [`065-browser-tools`](../065-browser-tools/spec.md): Autofill and `entry.fill` never touch a pass (FR-026).
- [`085-qr-link`](../085-qr-link/spec.md): Its camera work (the native Linux camera, the finding that the web camera
  freezes on WebKitGTK, the rule that a spike must count new frames) is the base for scanning a code (FR-034).
- [`013-vault-lifecycle-isolation`](../013-vault-lifecycle-isolation/spec.md): One process is one vault session and closing
  the vault ends the process. The Android share target and the shortcut start from a cold process and must wait for the
  unlock (FR-037, FR-038).
- [`084-vault-folders`](../084-vault-folders/spec.md): Temporary files of an import or a scan live in the vault's own folder
  and are removed when done (FR-035).
- [`058-attachment-viewer`](../058-attachment-viewer/spec.md): The viewer for PDF and images shows the ticket file attached
  to a pass.
- [`020-tab-navigation`](../020-tab-navigation/spec.md) and
  [`022-session-restore`](../022-session-restore/spec.md): A pass is a location in the tab like any entry and returns with
  the session without its code on screen (FR-040).
- [`043-android-build`](../043-android-build/spec.md): The Android parts build and run under its rules; iOS is not on the
  roadmap.

## Clarifications

### Session 2026-11-11 (operator)

- Q: What is in scope of "wallet"? → A: What the wallets do for passes: loyalty cards, memberships, vouchers and coupons,
  event tickets, boarding passes, train tickets. Not payment cards, IDs and keys (Not in scope).
- Q: Does a pass need its own table or its own view? → A: Neither alone. A pass is a second **type of entry** that shares
  the identity (`item_details`) with the login, so that folders, tags, trash, history, attachments, grants and sync work
  without a copy; its own fields sit in a 1:1 detail table.
- Q: Is that the same shape for the types that come later? → A: Yes. Payment card, identity, note and SSH key follow as
  further types in later specs, each costing a detail table and a view; the Bitwarden import then maps to them. This spec
  lays the pattern and does not build them.
- Q: Which data go in the typed fields and which in the free fields? → A: What the pass is (kind of pass, start, codes, link
  to its parent) is typed so it can be sorted, queried and shown as a code; anything free (seat, gate, platform, notes of
  the issuer) stays in the existing free fields.
- Q: How do several vouchers or tickets of one account stay apart? → A: **One entry is one pass.** A pass has its own
  fields, expiry, attachments, history and trash. It is linked to the entry it belongs to (the account, the card, the
  outbound trip); it is not copied into it. The account shows its linked passes as a list.
- Q: Is a loyalty card a field of the account or an entry of its own? → A: An entry of its own (a pass), linked to the
  account. One rule for everything that has a code, and the card is found as a card.
- Q: How many parents may a pass have? → A: One, any entry (login or pass). No cycles.
- Q: Must the vault be unlocked to see a pass? → A: Yes; that is what the vault is for. Unlock by biometrics is a separate
  planned feature and will make it quick. There is no restricted "cards only" access without a full unlock.
- Q: Where does a new pass go? → A: A pass that is ticket-like **and** carries a date is proposed for the folder `Tickets`
  (or `Tickets/<provider>`) with the tag `Ticket`. Everything else (loyalty card, membership, voucher, generic, a pass of
  unknown origin) is created at the root. The user can change both at every import; no rule moves an existing entry.
- Q: Is there a flat "all tickets" view ordered by date? → A: The folder and the tag give the set; a sort by date and a
  filter that hides expired passes give the order (FR-021).

### Session 2026-11-11 (clarify)

- Q: What is the UI name of the type? → A: "Karte/Ticket" (English "Card/Ticket"), internal `pass`.
- Q: Does the search read folder names? → A: Yes. The folder names join title, user name, address and tag names as a field
  the search reads (FR-022); this changes 034 FR-007 for all entries, not only passes.
- Q: Is the pkpass signature checked? → A: Not by default, but the import dialog MUST show the option to check it
  (FR-030). The result is information for the user and does not decide the import.
- Q: Is sorting by date needed? → A: Yes, definitely, for tickets: it is a requirement of the pass type, not a later
  improvement (FR-021).
- Q: Are linked passes hidden from the root list? → A: No; every pass is listed where its folder is.
- Q: How is the start of a ticket stored and shown? → A: As a point in time (UTC) with the time zone of the place, shown in
  the time of the place; when that zone differs from the device's, both times are shown (FR-002).
- Q: What may the built-in agent see of a pass? → A: Id, title, kind, dates, tags and folder name, never a code, unless
  the user restricts it; every tag restriction that exists for logins applies to passes unchanged, with no new setting. The
  plan found that the built-in agent's own search has no tag restriction for logins today; passes follow that (FR-027a).
- Q: In which order does the date sort list entries, and what can the user filter by? → A: Upcoming first, the nearest
  one on top; passes that are over follow, the most recent first; entries without a date last. The user filters by type
  (login or pass) and, within passes, by the kind of pass (ticket, loyalty card, voucher, …), and the filter works together
  with the search, so that "only tickets, by the next event" is two choices (FR-010, FR-021).
- Q: Can an entry change its type after it is created? → A: No. The type is fixed at creation; a user who needs another
  type creates a new entry (and links it where it belongs) and deletes the old one (FR-001).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Entries have a type, and nothing existing changes (Priority: P1)

The password manager learns that an entry is a login or a pass. Every existing entry becomes a login and behaves exactly as
before. Every part that handles entries from outside (lists, search, autofill, the external bridge, the built-in agent, MCP
callers, import) knows the type and treats a pass as a pass.

**Why this priority**: Everything else rides on it, and getting it wrong breaks the password manager that works today. It
is also the part that must be right once, so that every later type costs only a detail table and a view.

**Independent Test**: Open a vault with entries from before this spec, check every login field, tag, folder, history stand
and passkey unchanged, create one pass through the service, and check that lists show its type, that credential autofill
and the credential tools of an agent do not return it, and that the login entries still behave as before.

**Acceptance Scenarios**:

1. **Given** a vault from before this spec, **When** holzi opens it, **Then** every entry is a login and every field, tag,
   folder, attachment, history stand and passkey is unchanged.
2. **Given** a pass in the vault, **When** autofill, `entry.fill` or the external bridge look for credentials for a page,
   **Then** the pass is never offered, filled or counted as a duplicate of a login.
3. **Given** a pass in the vault, **When** an agent or extension lists entries, **Then** every row carries its type, a
   credential search returns only logins unless the caller asks for passes, and no row carries a code.
4. **Given** a device that does not know the type, **When** a pass reaches it by sync, **Then** it is not shown or edited
   as an empty login and its data is not overwritten (FR-042).

---

### User Story 2 - Create a pass and show its code (Priority: P1)

The user creates a pass (a ticket, a loyalty card, a voucher) with a title, a kind of pass, one or more codes, a start and an
end date and free fields, and opens it at the counter: the code is rendered large on a white ground at full brightness and
the screen stays on.

**Why this priority**: This is the use. Without a code that can be shown and scanned there is no wallet.

**Independent Test**: Create a pass with one code of each format, open it, show the code large, and read each rendered code
back with a decoder: it yields the stored value.

**Acceptance Scenarios**:

1. **Given** the toolbar, **When** the user chooses "New", **Then** it offers a login, a pass ("Karte/Ticket") and a folder.
2. **Given** a pass with a code, **When** the user opens it, **Then** the code is drawn on the page without a network
   request, and "Show large" fills the screen with the code on a white ground, raises the brightness and keeps the screen on
   until the user leaves it, when the platform allows.
3. **Given** a pass with several codes (a group ticket, outbound and return), **When** the user opens it, **Then** each
   code is shown under its label and can be shown large on its own.
4. **Given** a code whose value does not fit its format (an EAN-13 with a wrong check digit), **When** the user enters it by
   hand, **Then** it is refused with the reason; **When** such a code arrives by import or sync, **Then** it is kept,
   flagged on the pass, shown as text and repairable.
5. **Given** the dark theme, **When** a code is shown, **Then** it is still dark on white, so a scanner reads it.
6. **Given** an expiry date in the past, **When** the list or the pass is shown, **Then** the pass is marked expired (034
   FR-008).
7. **Given** a concert in London at 20:00 local time and a device in Berlin, **When** the pass is shown, **Then** it shows
   "20:00" for London and "21:00" for the device, both labelled; **When** the device is in London, **Then** only one time
   is shown.

---

### User Story 3 - A pass belongs to an account or another pass (Priority: P1)

The user links a ticket to the account it was bought with, a voucher to the loyalty card it belongs to, or a return trip to
the outbound trip. The account or card then lists everything linked to it, with date and state, and each pass says where it
belongs.

**Why this priority**: Tickets and vouchers come in numbers; without a way to see which belong together the list becomes a
pile, which is the objection that led to one-entry-per-pass.

**Independent Test**: Link two vouchers with the same free fields and different values to one loyalty card; open the card and
see both as separate rows; trash the card and see both vouchers survive with "(in trash)".

**Acceptance Scenarios**:

1. **Given** a pass, **When** the user picks "Belongs to" and chooses an entry, **Then** the pass shows "Belongs to: …" at the
   top and the chosen entry shows the pass in its "Linked" section with title, date and valid or expired.
2. **Given** a pass that belongs to a pass that belongs to the first, **When** the user tries to close the loop, **Then**
   it is refused (FR-013).
3. **Given** a linked parent, **When** it is moved to the trash, **Then** the passes stay, show "(in trash)" at the top, and
   are back to normal when the parent is restored.
4. **Given** a linked parent, **When** the user deletes it from the trash for good, **Then** the passes stay and are
   no longer linked.
5. **Given** two devices that link the entries in opposite directions at the same time, **When** they sync, **Then** the
   loop is detected, shown, and repairable without hanging the list.
6. **Given** two linked vouchers with the same field names, **When** the user opens the card, **Then** nothing from either
   is merged or shown under the other.

---

### User Story 4 - Keep tickets in order and find them again (Priority: P2)

Tickets sit in a folder (`Tickets`, optionally one per provider), carry a tag, sort by date, and expired ones can be hidden.
A loyalty card is not in that folder; it is an entry at the root or wherever the user keeps it, found by the name of the
provider.

**Why this priority**: The wallets are weak here; this is where holzi is better. It matters once the user has more than a
handful of passes.

**Independent Test**: With twenty tickets of three providers and five loyalty cards, sort by date, hide the expired, filter by
type, and find one ticket by a word of its title.

**Acceptance Scenarios**:

1. **Given** a list of entries, **When** the user filters by type, **Then** only passes (or only logins) remain;
   **When** the user then filters by the kind "ticket", **Then** only tickets remain.
2. **Given** passes with dates, **When** the user sorts by date, **Then** the upcoming ones come first with the nearest on
   top, the passes that are over follow with the most recent first, and passes without a date come last; **When** the user
   hides expired passes, **Then** only valid ones remain.
3. **Given** a filter on tickets and a search word, **When** the user sorts by date, **Then** only the tickets that match the
   word are listed, the next event first.
4. **Given** a ticket titled "ICE Berlin → München, 14.11.", **When** the user searches "ice münchen", **Then** it is found
   (034 FR-007).
5. **Given** a ticket in the folder `Tickets/Deutsche Bahn` whose title does not name the provider, **When** the user
   searches "bahn", **Then** it is found through the folder name; **When** the folder is renamed, **Then** the ticket is
   found under the new name at once.
6. **Given** a loyalty card imported without a date, **When** the dialog proposes a place, **Then** it proposes the root, not
   `Tickets`.

---

### User Story 5 - See what changed on a ticket and keep the file (Priority: P2)

The organiser moves the concert; the user changes the date and the time. The history shows the old and the new date. A PDF or
a photo of the ticket is attached and can be opened later.

**Why this priority**: History and attachments exist already; this makes them hold for passes.

**Independent Test**: Change the start of a pass and read the history stand as a changed field; attach a PDF and open it;
restore an old stand.

**Acceptance Scenarios**:

1. **Given** a pass, **When** the user changes its start, an end or a code, **Then** a history stand is kept and names the
   changed fields; the values of codes are shown only on request (034 FR-017).
2. **Given** history stands written before this spec, **When** the history is opened, **Then** they are read as logins as
   before.
3. **Given** a pass with a PDF attached, **When** the user opens the attachment, **Then** the viewer of spec 058 shows it.
4. **Given** an old stand of a pass, **When** the user restores it, **Then** the fields come back; the link to a parent and
   the folder are left as they are (FR-023).

---

### User Story 6 - Import a pass from a pkpass file (Priority: P2)

The user imports a `.pkpass` (from a mail, a download, a train company). A pass is created with its code, dates, fields and
logo; the dialog proposes the place by the rule and offers to link it to an existing entry.

**Why this priority**: Typing a code is the slow path; the file is how most tickets arrive.

**Independent Test**: Import one file of each pass style (boarding pass, event ticket, store card, coupon, generic); check
type, dates, codes and fields, and that anything not stored is named in the report.

**Acceptance Scenarios**:

1. **Given** a pkpass of an event ticket with a date, **When** the user imports it, **Then** the dialog shows the pass, the
   proposed folder `Tickets` (or `Tickets/<provider>`) and the tag `Ticket`, and the user can change either before saving.
2. **Given** a pkpass of a store card, **When** the user imports it, **Then** the dialog proposes the root.
3. **Given** the dialog, **When** the user chooses "Belongs to", **Then** it lists entries, puts a probable match (same
   provider name or address) first, and links the new pass to the chosen one; it never links or merges by itself.
4. **Given** the same pass imported twice, **When** the second import is read, **Then** the dialog says it exists and asks.
5. **Given** a pass with fields or images holzi does not keep, **When** it is imported, **Then** the report names them.
6. **Given** the import dialog, **When** it shows a pass, **Then** it offers "Check signature", off by default; **When** the
   user turns it on, **Then** the result (issuer, valid, invalid or not checkable) is shown before saving and does not stop
   the import.
7. **Given** a file that is not a readable pass, **When** it is imported, **Then** it is refused with the reason and the
   others of the same selection are still imported.

---

### User Story 7 - Capture a code by camera, photo or PDF (Priority: P2)

The user points the camera at a barcode on a card or a printout, or picks a photo or a PDF, and holzi reads the code into a
new pass, or adds it to an existing one.

**Why this priority**: Most loyalty cards and many tickets have no file, only a code on paper or a screen.

**Independent Test**: Scan a printed code of each of the five formats by camera, then read the same codes from a photo and
from a PDF; check format and value of the result.

**Acceptance Scenarios**:

1. **Given** the camera, **When** the user holds a code in view, **Then** the format and value are read, shown, and only
   saved after the user confirms.
2. **Given** a photo or a PDF with more than one code, **When** it is read, **Then** the user chooses which codes to keep.
3. **Given** a device without a usable camera, **When** the user wants to add a code, **Then** photo, PDF and typing remain.
4. **Given** a code read without any information on the kind of pass, **When** the user saves it, **Then** the dialog asks
   for the kind of pass and proposes the root.

---

### User Story 8 - Share a ticket to holzi on Android, open it from the home screen (Priority: P3)

On Android the user shares a pkpass, a PDF or an image from the mail or the browser to holzi, and gets the import dialog.
The user can pin a pass to the home screen and open it from there.

**Why this priority**: It makes the wallet quick, but needs native Android code and, to be quick, biometric unlock, which is
a separate feature.

**Independent Test**: On the emulator or a phone, share a pkpass to holzi with the vault locked and with it unlocked; pin a
pass and open it from the launcher.

**Acceptance Scenarios**:

1. **Given** holzi is closed and the vault locked, **When** the user shares a pkpass to it, **Then** holzi asks to unlock
   and then shows the import dialog with the shared file; the file is not lost between the two steps.
2. **Given** a pinned pass, **When** the user opens it from the launcher, **Then** holzi asks to unlock and then opens
   that pass, not the list.
3. **Given** the launcher, **When** a pass is pinned, **Then** the launcher holds a label the user chose (a neutral default,
   not the title) and the pass's identifier, and never a code.

---

### Edge Cases

- A pass with no title, no code and no date is valid, as an entry without a title is (034 FR-001); the list shows the
  placeholder.
- A pass whose end date is earlier than its start date: allowed (data arrives that way); shown as is, expired by the end date.
- A start whose time zone the source does not name (a pkpass gives only an offset, a scan gives nothing): the offset is kept
  as the zone; for a typed start the user's own zone is the default and can be changed. A start with no time is a date and
  carries no zone.
- A device whose zone changes (travel, daylight saving time): the point in time is unchanged; only the device's time shown
  next to the time of the place moves.
- A kind of pass written by a newer holzi that this one does not know: shown as "other" with its data; never refused.
- A pass whose parent id points at an entry not in the vault (yet): shows "(not available)" until the entry arrives; never an
  error and never an empty page.
- The parent is also a pass that is expired: the child is unaffected; its state is its own.
- A parent with very many linked passes (hundreds of tickets on one account): the list stays usable and sorted by date,
  in the order of FR-021; hide-expired applies here too.
- A pkpass with localised strings: the strings of the UI language are used when present, otherwise the base ones; which is a
  plan detail, but the choice never drops a field.
- A pkpass with a code in a format holzi does not draw: stored as text with its original format name, flagged, and the
  report names it.
- A pkpass over the attachment limit (034 FR-020) or an image over the limit: the pass is created, the image is not kept and
  the report says so.
- A code read by camera that is a payment or identity QR (a URL, a token): holzi stores what it read as text; it does not
  open it, call it or interpret it.
- The shared file on Android is large or the vault takes long to unlock: the file stays in the app's own temporary space
  until the dialog ends or is cancelled and is then removed.
- A second window or device edits the pass while the user edits it: the conflict dialog of 034 applies.
- A pass is selected in a mass action (034 FR-012): the count named includes passes; moving and tagging work, nothing
  special.

## Requirements _(mandatory)_

### Functional Requirements

**Type and data**

- **FR-001**: The system MUST give every entry a type, `login` or `pass`. Every entry that exists before this spec becomes a
  `login`; every behaviour of a login stays as specified in 034 and 036. The type is fixed when the entry is created; no
  function converts an entry from one type to the other.
- **FR-002**: A pass MUST have the fields an entry has (title, note, address, icon, colour, expiry date, tags, folder,
  free fields, attachments, history; all may be empty) and in addition a **kind of pass** (ticket, loyalty card, voucher,
  membership, boarding pass, other), an optional **start** and optional **parent** (FR-013). The start is either a date
  alone, or a point in time (stored as UTC) together with the **time zone of the place** where it happens. A pass is
  shown and sorted by that point in time. Wherever the start is shown (list, pass, "Linked" section, history), it MUST be
  shown in the time of the place; when the time zone of the place differs from the device's current one, it MUST also be
  shown in the device's time, both labelled. A date alone has no time and no zone and is shown as a date. The end of a
  pass is the existing expiry date (034 FR-008), a date without time; no second end field exists.
- **FR-003**: A pass MUST hold any number of **codes**, each with a format (QR, Code 128, EAN-13, PDF417, Aztec), a value, an
  optional label and a position. A code entered by hand MUST be validated against its format (including check digits)
  and refused with a reason if it does not fit; a code that arrives by import or sync MUST be kept, shown as text, flagged
  and repairable, as an invalid TOTP secret is (034 FR-003).
- **FR-004**: A kind of pass the system does not know (written by a newer holzi, or imported) MUST be kept and shown as
  "other".
- **FR-005**: The data model MUST be: `haex_passwords_item_details` stays the one table for the identity and the shared
  fields and gets the type column; a new table `haex_passwords_pass_details` holds one row per pass (item id as key, kind of
  pass, start as a point in time plus its time zone or as a date, parent) and a new table `haex_passwords_pass_barcodes` holds the codes (item id, format, value, label,
  position). Everything that hangs on the item id today (free fields, folder assignment, attachments, history, tag
  assignment) MUST serve both types without a copy. The foreign key of a detail row to its entry MUST cascade on delete;
  the parent MUST be cleared (not cascade) when the parent entry is deleted. Both new tables follow 034 FR-037 (CRDT, no
  UNIQUE constraints, delete markers). This adds to the list of deviations in 034 FR-036; the amendment goes into 034 with
  the plan.
- **FR-006**: The foundation MUST be built so that a further type is a detail table, a view and a mapping in the import,
  not a change to the satellites, the lists, the trash, the history format, the grants or the sync.
- **FR-007**: The login-specific columns of `item_details` MUST stay; for a pass they are empty. Moving them out is not part
  of this spec.

**Showing a pass**

- **FR-008**: The view of a pass MUST draw each code itself, without any network request, and MUST offer "Show large": the
  code full screen on a white ground with the label, raised screen brightness and the screen kept on, all restored when the
  user leaves the view. Where a platform does not allow brightness or keeping the screen on, the code is still shown large
  and the rest is skipped without an error.
- **FR-009**: A code MUST be drawn dark on white in every theme, with the quiet zone the format requires.
- **FR-010**: Lists MUST show the type (an icon and, for a pass, its kind), the start and the expired state, and MUST offer a
  filter by type (login or pass) and, for passes, by kind (ticket, loyalty card, voucher, membership, boarding pass, other).
  The filter combines with the search and with the sort (FR-021), and the chosen filter and sort stay as the user left them
  while the vault is open.
- **FR-011**: The "New" action (034 FR-009: today login or folder) MUST offer the pass as a third item.
- **FR-012**: The editor of a pass MUST offer its typed fields, its codes (add, change, reorder, remove, copy the value of a
  code as text), the free fields and the attachments, in the style of 034 and with each value a user can copy (034 FR-006).

**Linking**

- **FR-013**: A pass MAY belong to **one** other entry, a login or a pass. The system MUST refuse a link that makes a pass
  belong to itself or to one of its descendants. A loop that arrives by sync (two devices linking in opposite directions)
  MUST be detected, shown on the passes concerned, and be repairable by the user, and MUST NOT hang or hide the list.
- **FR-014**: A pass with a parent MUST show "Belongs to: …" at its top, linked to the parent. An entry with linked passes
  MUST show a "Linked" section listing its direct children, with title, start or expiry date and valid or expired,
  in the order of FR-021. The section is a query over the parent link and has no storage of its own.
- **FR-015**: Moving a parent to the trash MUST NOT move, change or delete its linked passes; they show "(in trash)" at the
  top and return to normal on restoring. Deleting a parent for good MUST clear the link of its passes and leave them
  otherwise untouched. A parent that is not in the vault (not yet synced) MUST show "(not available)".
- **FR-016**: Linking MUST never copy or merge fields: each pass keeps its own free fields, codes, attachments and history, so
  two linked passes with the same field names do not collide.
- **FR-017**: The editor MUST offer a picker to set and to clear the parent, and the view of a parent MUST offer "Add linked
  pass" that opens a new pass with the parent preset. The existing `{REF:…}` placeholders (036) resolve values of other entries
  and are not this relation; they are unchanged.

**Organising and finding**

- **FR-018**: The rules of 034 for folders, tags, trash, bulk actions and restore MUST hold for passes without change
  (034 FR-009 to FR-012, FR-015, FR-016).
- **FR-019**: When a pass is created from a file or a scan, the dialog MUST propose a folder and a tag by this rule, and the
  user MUST be able to change both before saving: a pass that is **ticket-like** (kind ticket or boarding pass, or an
  event or transit pass in a file) **and** carries a date is proposed for the folder `Tickets` (or `Tickets/<provider>`
  when the provider is known) and the tag `Ticket`; every other pass is proposed for the root with no tag. A folder or
  tag that does not exist is created when the user confirms the proposal; an existing one of the same name (case
  ignored) is reused, not duplicated. No rule moves an entry that exists.
- **FR-020**: An import MUST set a speaking title (for a ticket: what it is for, where, and when, for example "ICE Berlin →
  München, 14.11.") and, where the file gives a provider address, set the address; the provider's name MUST at least
  appear in the title or the note, so that the entry is found by the existing search.
- **FR-021**: The list MUST be sortable by title (as today) and by date, and MUST offer a filter that hides expired entries.
  The date of an entry is its start, otherwise its end. The order by date is: entries that are still to come, the nearest
  first; then entries that are over, the most recent first; then entries without a date. This is new for all entries;
  logins without an expiry date sort last by date. It is required for the pass type and ships with it, not after it.
- **FR-022**: Search (034 FR-007) MUST find passes by title, user name, address, tag names **and folder names** as it finds
  logins. The folder names are those of the entry's whole folder path (`Tickets` and `Deutsche Bahn` for an entry in
  `Tickets/Deutsche Bahn`); an entry at the root or in the trash has none, and a renamed or moved folder takes effect at
  once. This changes the search for all entries, and the amendment of 034 FR-007 goes with the plan. The values of codes,
  free fields and notes are not searched.

**History and attachments**

- **FR-023**: Every change to a pass MUST keep a history stand (034 FR-017). The format of a stand becomes version 2, with a
  block for the pass data (type, kind of pass, start, codes); a reader MUST read version 1 stands as logins. A stand does not
  hold the parent link or the folder (restoring never moves or re-links, as it does not move folders today). The history
  names the changed fields; the values of codes are shown only on request, like the free fields of a login.
- **FR-024**: Attachments of a pass (a PDF, a photo of the ticket) MUST work as in 034 FR-019 to FR-020, with the limit of
  25 MiB, and open in the viewer of spec 058.

**Access from outside**

- **FR-025**: The values of codes MUST be secrets in the sense of 034: they MUST NOT appear in a list, in the header data
  of an entry, in a title of a window, in a log, in an error or in the history list of a tab (034 FR-026, FR-040), and the
  built-in agent MUST NOT read them (034 FR-027). They are returned only by a single query for one entry. The header data
  of a pass carries its type, kind, dates, state and the id of its parent, never a code.
- **FR-026**: Browser autofill and `entry.fill` (spec 065), the external bridge (spec 049) and the credential matching of any
  caller MUST consider only logins. A pass is never offered, filled, matched or counted as a duplicate; passkeys belong to
  logins only.
- **FR-027**: Grants (034 FR-025 to FR-029, spec 021) MUST cover passes by tag as they cover logins. Search results for
  agents and extensions MUST carry the type and accept a filter by type; the credential tools MUST return only logins
  unless the caller asks for passes. Creating or changing a pass from outside is not offered in this spec; the user's own
  interface is the one writer (FR-031 of 034).
- **FR-027a**: The built-in agent MUST see of a pass the same narrowed header as of a login (id, title, tags, folder name)
  plus its kind and dates, and never a code (FR-025); without a restriction by the user it may read all of that for every
  pass. Wherever the user can restrict logins by tag today (external agents, extensions and the external bridge through their
  grants, and the browser fill of the built-in agent through its profile), the same restriction MUST apply to passes
  unchanged: a pass outside the scope is not there for that caller (034 FR-029). The search of the built-in agent is not
  limited by tags for logins today (rule Z2 of `specs/034-password-manager/contracts/access.md`: it sees only the narrowed header and never a secret); passes follow
  that rule and this spec does not change it. This spec adds no setting of its own for passes.

**Import and capture**

- **FR-028**: The system MUST import `.pkpass` files (a ZIP with `pass.json` and images): the kind of pass from the pass
  style, the provider from the organisation name, the start from the relevant date (the file gives a point in time with an
  offset from UTC; that offset is the time zone of the place when the file names no zone, and the user can change it
  in the dialog), the end from the expiration date (as a date), the
  codes from the barcodes with their formats, the fields and back fields as free fields, and the logo as the icon. Anything
  in the file that holzi does not keep MUST be named in the report; nothing is lost silently (034, Clarifications).
- **FR-029**: An import MUST never use the network: the update service address in a pass is not called, and no pass is
  updated by its issuer.
- **FR-030**: The signature of a pass is not checked by default. The import dialog MUST show an option "Check signature" (off
  unless the user turns it on). When it is on, the dialog MUST show the result before saving (the issuer the signature
  names, valid, invalid or not checkable, with the reason), and the check MUST work without the network. The result is
  information: it does not stop the import, and a pass is never treated as trusted because it is signed. A pass imported
  without the check is shown as imported from a file. The data of a pass MUST be read as untrusted input (size limits on
  the ZIP and its entries, no path traversal, no execution).
- **FR-031**: The import dialog MUST show the pass (kind, dates, codes by label, fields) before saving, the proposal of
  FR-019, and a choice "Belongs to" (an entry picker that puts a probable match by provider name or address first, and
  never links by itself) or none. A pass the vault already has (by the identifier the file carries) MUST be detected and
  the user asked (034, import). Several files MAY be imported in one go; each is its own unit, one failing does not stop
  the others, and the report names each.
- **FR-032**: The report of an import MUST name for each pass the entry, the place and what was not kept, and MUST NOT
  contain a code or any secret; it can be saved as a text file (034, import).
- **FR-033**: A file that cannot be read as a pass MUST be refused with the reason and leave nothing behind.
- **FR-034**: The system MUST read a code from the camera, from a photo and from a PDF (formats of FR-003) into a new pass or
  into an existing one, show format and value, let the user pick when several are found, and save only after the user
  confirms. The camera path builds on spec 085; where no camera is usable, photo, PDF and typing MUST remain. A value read
  is stored as text and is never opened, called or interpreted.
- **FR-035**: A shared, imported or scanned file MUST be held only in memory or in a folder of the vault folder named for its purpose (spec 084 FR-002), or in the app's cache directory before a vault is open and MUST be removed when the dialog ends or is cancelled; nothing is sent anywhere.

**Android**

- **FR-036**: Every function of this spec MUST work on every platform the password manager runs on (desktop and Android);
  where the operating system limits a part (brightness, keeping the screen on, the camera) the rest MUST still work and the
  logic MUST be testable on Linux. iOS is not part of it.
- **FR-037**: On Android, holzi MUST be offered as a share target for `.pkpass`, PDF and image files. A shared file MUST reach
  the import dialog after the vault is unlocked, also when holzi was not running (a cold process, spec 013), and MUST NOT
  be lost between the start and the unlock.
- **FR-038**: The user MUST be able to pin a pass to the launcher; opening it MUST ask to unlock, then open that pass. The
  launcher entry MUST hold a label the user chose (a neutral default, never the title) and the pass's identifier, and MUST
  NOT hold a code.
- **FR-039**: Unlock by biometrics is not part of this spec. The share target and the shortcut work with the unlock that
  exists; their speed goals are only checked once biometric unlock exists.

**Everything around it**

- **FR-040**: A pass MUST be a location in the tab (spec 020) and return with the session without a code on screen or any
  revealed value (spec 022, 034 FR-039); a code shown large MUST end when the user leaves it or the tab is closed.
- **FR-041**: The password manager MUST update live when a pass changes in another window, by an agent or by sync, without
  overwriting an edit in progress (034 FR-038).
- **FR-042**: A device that does not know the type of an entry MUST NOT show it as an empty login, MUST NOT let the user edit
  it and MUST NOT write over its data; it shows a notice that the entry needs a newer holzi. The sync version
  gate of spec 024 already keeps builds with different migrations from syncing (plan, research R1); this requirement is the
  second layer behind it.
- **FR-043**: Every text MUST exist in German and English; the interface MUST work at 360 px without horizontal scrolling and
  follow the style of 034 FR-039 and FR-041; the icon of each type and kind of pass is one of the icons the password manager already uses for entries (a fixed set of
  `lucide:` names; the icon sets of spec 053 serve the file browser only and are not used here).

### Key Entities

- **Entry**: the common record of the password manager (034), now with a **type**, `login` or `pass`.
- **Pass**: an entry of type `pass`: a card or ticket with a kind of pass, optional start (a point in time with the time zone
  of its place, or a date), optional parent and any number of codes; its end is the expiry date of the entry.
- **Kind of pass**: ticket, loyalty card, voucher, membership, boarding pass, other. It steers the proposal of a place (FR-019)
  and the icon; it does not change what a pass can hold.
- **Code**: one barcode or QR of a pass, with format, value, optional label and position. Its value is a secret.
- **Link (belongs to)**: the relation from a pass to one entry it belongs to; a query, not a table.
- **Pkpass file**: the ZIP an issuer provides; a source of one pass and the data of FR-028.
- **Import report**: as in 034, with the entry, place and what was not kept; never a secret.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user creates a pass with a title and one code by hand in under a minute, and from the list opens it and
  shows the code large in two taps.
- **SC-002**: For each of the five formats, a rendered code read back by a standard decoder yields the stored value in 100 %
  of the test values (a round trip in the automatic tests).
- **SC-003**: A vault with entries from before this spec opens with 0 visible changes: every login equals itself in fields,
  tags, folder, attachments, history and passkeys (100 % of the compared entries).
- **SC-004**: In the automatic tests, no list, header data, agent view, log or error contains the value of a code (0 finds
  over all query kinds).
- **SC-005**: Across the test set of credential lookups (autofill, `entry.fill`, bridge, credential tools), 0 passes are
  returned.
- **SC-006**: The sample files of all five pass styles import with the correct kind, dates, codes and fields in 100 % of
  the cases, and everything not kept is in the report (0 silent losses).
- **SC-007**: 100 % of attempts to link a pass to itself or a descendant are refused; a loop made by two devices is
  detected and repairable without a hang.
- **SC-008**: A changed start date is named in the history in 100 % of the cases, and 100 % of version 1 stands still read.
  A start in another time zone than the device's shows both times correctly in 100 % of the tested zones, including the
  days around a change to or from daylight saving time.
- **SC-009**: With twenty tickets of three providers and five loyalty cards, sorting by date, hiding expired passes and
  filtering by type give the expected list in 100 % of the checks, and a search for a word of the provider's folder
  finds every ticket in it.
- **SC-010**: Every function of the spec is usable at 360 px without horizontal scrolling.
- **SC-011**: The main flows (create, show large, link, import a pkpass, sort and filter) run as end-to-end tests against
  the built app on desktop; the Android flows (share target, shortcut, camera) run on the emulator where it can and are
  otherwise manual steps in the quickstart.

## Assumptions

- The vault is unlocked whenever a pass is shown; biometric unlock will make that quick but is a separate feature.
- A code on screen is not covered by a reveal step: the point of a pass is to be shown quickly, and the unlocked vault is
  the gate. The value is still a secret for everything that is not the user's own screen (FR-025).
- The five code formats of FR-003 cover what the wallets carry for the pass kinds in scope; others are kept as text.
- Folder and tag names such as `Tickets` and `Ticket` are created in the language of the interface when the user confirms
  the proposal and are ordinary user data afterwards (renamable, deletable).
- One parent per pass is enough; a pass that belongs to two accounts is not a case this spec serves.
- A pass is a child at most one level in the display (the "Linked" section lists direct children); deeper chains are
  stored and navigable by opening the parent.
- External callers read passes but do not create or change them in this version.
- The Bitwarden import stays as it is; the types card, identity, note and SSH key arrive in later specs on the foundation
  of this one, and the question of converting what was already imported is theirs.
- The user's own interface is the only writer of passes, so there is no tag-scope rule for writing a pass.

## Open Questions

Each has a default, so the plan can start; the operator can overrule any of them.

1. **Old devices.** Resolved by the plan (research R1): the sync gate of spec 024 (FR-029) already refuses to sync between
   builds whose migrations or trigger version differ, and an old build refuses to open a vault that holds a migration it does
   not know. FR-042 therefore asks for a second layer only: an entry of a type this build does not know is never shown as an
   empty login, and every later type ships with a migration so the gate stays the enforcement.

Resolved on 2026-11-11 (see Clarifications): UI name, search over folder names, signature check as an option, sort by date,
linked passes stay in the list, the type of an entry is fixed.

## Not in scope

- **Contactless payment.** Paying by phone needs tokenisation and certification with the card networks. A payment card as an
  entry type (number, holder, expiry, no payment) is a possible later type, not this.
- **Digital IDs, driver licences, car and hotel keys, transit on NFC.** They rest on keys attested by the vendor. An
  identity entry as stored data is a possible later type.
- **Codes that rotate** (the Deutschlandticket and some train tickets show an animated or time-based code). A static import of
  such a ticket is not valid at the gate; holzi does not pretend otherwise.
- **Updates pushed by the issuer** (gate changed, delay). They go through the vendor's push service; the pass is edited by
  hand or re-imported.
- **iOS.** Not on the roadmap (no Apple developer licence); the iOS parts of spec 085 and 070 are deferred the same way.
- **Biometric unlock** itself (separate feature).
- **Time- and place-based surfacing** (a ticket appears on the lock screen on the day, a pass appears at a location).
  A later addition; it would need local notifications, and for places a location permission.
- **Further entry types** (payment card, identity, note, SSH key) and converting the entries the Bitwarden import made.
  Later specs on the foundation of this one.
- **Moving the login columns out of `item_details`.** A separate clean-up.
- **Writing passes from outside** (agents, extensions, the bridge).
- **Sharing a pass with another person.** The password manager has one user per vault (034, Assumptions).
- **A web pass format other than pkpass** (the Google "Save to Wallet" link carries a signed token that few issuers
  publish as a file); added only if a source turns out to be common.
