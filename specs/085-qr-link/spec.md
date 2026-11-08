# Feature Specification: Link a vault by scanning a QR code

**Feature Branch**: `085-qr-link`

**Created**: 2026-11-08

**Status**: Draft

**Input**: User description: "In the login / open-vault screen we have the option 'Mit einer Vault verknüpfen'. I want to make it
more user-friendly, so that a user can take over all necessary information from their old vault simply by scanning a QR code."
(operator, 2026-11-08)

## Background

- **What exists (spec 024, user story 5, FR-023 to FR-025).** A main device shows a link code, valid once and for ten minutes,
  as a QR code and as text (`LinkDeviceView.vue`). On a fresh installation the landing page offers "Mit einer Vault
  verknüpfen" (`LinkSheet.vue`). Today the user types the 26-character code, a device name, a name for the new vault, a
  passphrase twice, and, when the main device uses its own Nostr-Relays or iroh-Relays, those servers too, because a code cannot
  tell a new installation about them.
- **The QR code is shown but cannot be read.** The QR code carries only the 26 characters of the code. The sheet has no scanner,
  so the user reads a code off one screen and types it into another. Servers have to be re-entered by hand, which most users
  cannot do from memory.
- **What the user wants.** Point the new device at the old one and be left with only the choices that must be made on the new
  device: its name and its passphrase.
- **What stays as it is.** The code is still one-time and ten minutes, the proof of the code, the confirmation on the main
  device, the snapshot transfer and the discarding on abort (spec 024, FR-023 to FR-025) do not change. The passphrase never
  leaves the new device and is never part of the QR code.
- **Related.** Contacts already scan QR codes on phones with the official barcode plugin (spec 051, research R13); this feature
  reuses that scanner on phones and adds a camera scanner on desktops.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Scan the main device's QR code on a phone (Priority: P1)

A user installs HaexVault on a new phone, opens "Mit einer Vault verknüpfen", taps "QR-Code scannen", points the camera at the
code on their main device and sees the form filled in. They type a name for this device, choose a passphrase and start the
link.

**Why this priority**: This is the whole point of the feature, on the device where a camera is always at hand.

**Independent Test**: Show a link code on a main device, scan it on a phone installation, check that the form holds the code,
the vault name and the main device's servers, enter device name and passphrase, and finish the link.

**Acceptance Scenarios**:

1. **Given** the sheet is open on a phone, **When** the user taps "QR-Code scannen" and scans a valid link QR code, **Then**
   the camera closes, the code, the vault name and the servers of the main device are in the form, and only device name and
   passphrase are left empty.
2. **Given** the camera permission has not been granted yet, **When** the user taps "QR-Code scannen", **Then** the system
   asks for it, and after a grant the camera opens.
3. **Given** the user denies the permission or leaves the camera, **When** the scanner closes, **Then** the form is as before
   and typing the code still works; a denial shows a toast that says how to allow it.
4. **Given** a QR code that is not a HaexVault link (another app's code, a contact share), **When** the user scans it, **Then**
   nothing is filled in and a toast says that this is not a link code of a vault.

---

### User Story 2 - Scan the main device's QR code on a desktop with a webcam (Priority: P1)

A user installs HaexVault on a laptop, opens "Mit einer Vault verknüpfen", clicks "QR-Code scannen", holds their phone with the
code in front of the webcam and sees the same prefilled form.

**Why this priority**: The operator asked for it explicitly. A new computer is the common case for "I have a new device", and
every device and OS must work (a gap is a bug).

**Independent Test**: On each desktop OS holzi supports, with a webcam, scan a link QR code shown on another screen and check
that the form is filled as in story 1; then with no camera and with the permission denied, check the fallback.

**Acceptance Scenarios**:

1. **Given** the sheet is open on a desktop with a webcam, **When** the user clicks "QR-Code scannen", **Then** the system's
   camera permission is asked if needed, a live preview opens in the sheet, and a valid link QR code in view fills the form
   as in story 1 and closes the preview.
2. **Given** the preview is open, **When** the user closes it, closes the sheet, or the scan succeeds, **Then** the camera is
   released at once (the camera light goes off) and no frame is kept.
3. **Given** the desktop has no camera, the camera is in use elsewhere, or the permission is denied, **When** the user clicks
   "QR-Code scannen", **Then** a toast says which of these applies, and pasting the link or typing the code still works.
4. **Given** several cameras, **When** the preview opens, **Then** the user can pick another camera for this scan.

---

### User Story 3 - The main device shows a QR code that carries everything (Priority: P1)

On the main device the user opens "Gerät verknüpfen", and the QR code now carries more than the code: the Nostr-Relays and
iroh-Relays the vault's devices use (also those switched off, so the new device can show them as off) and the vault's name.
The text form shows the same content as a link the user can copy.

**Why this priority**: Without it, scanning would fill in the code only and the user would still have to enter the servers.

**Independent Test**: Open "Gerät verknüpfen" on a main device with one added Nostr-Relay, one added iroh-Relay and one built-in
server switched off; read the QR text and check that it holds all of them and the vault name, and that it holds no passphrase,
no key and no device secret.

**Acceptance Scenarios**:

1. **Given** a main device with its own servers, **When** it shows the code, **Then** the QR code and the copyable link carry
   the code, the servers and the vault name in one versioned link text starting with `haex-vault://link`.
2. **Given** the code expires or is used, **When** the main device shows a new one, **Then** the QR code changes with it; an
   expired code still scans but the link fails as it does today ("expired").
3. **Given** a main device that already is a linked device (not a main device), **When** the user looks for the code, **Then**
   it is not offered, as before (spec 024, FR-023).

---

### User Story 4 - Paste a link or type a code as before (Priority: P2)

A user without a camera, or with the QR code on the same device, copies the link on the main device (or sends it to the new
one) and pastes it into the code field; the form fills as in a scan. A user who only has the 26 characters types them, and
everything works as it does today.

**Why this priority**: Every device must work without a camera; this is the fallback of stories 1 and 2, and it keeps old
QR codes and old versions of the main device working.

**Independent Test**: Paste the new link, a plain code, and a code with spaces and dashes into the code field and check the
result of each.

**Acceptance Scenarios**:

1. **Given** the code field, **When** the user pastes a `haex-vault://link` text, **Then** the code field shows the code and
   the vault name and the servers are filled in.
2. **Given** a plain 26-character code (typed, pasted, or from a QR code of an older version), **When** it is entered, **Then**
   the code field takes it, and the servers block stays open as today.
3. **Given** a link text of a newer version than this build understands, **When** it is entered, **Then** a toast says that
   this build must be updated, and nothing is filled in.

---

### User Story 5 - Servers that came with the QR code are out of the way (Priority: P2)

When the QR code brought the servers, the form does not ask for them: the server blocks are collapsed to a short line
("Server von deinem anderen Gerät übernommen") that can be opened to look at, switch off or add to the list.

**Why this priority**: The form shrinks to what only the user can decide; the full list stays reachable for the rare user who
needs it.

**Independent Test**: Scan or paste a link with servers and check that the form shows device name, passphrase and the
collapsed line; open it and check the list and that a change is kept.

**Acceptance Scenarios**:

1. **Given** servers came with the link, **When** the form shows, **Then** the server blocks are collapsed and the sheet shows
   a short note of how many it took over.
2. **Given** the collapsed line is opened, **When** the user switches a server off or adds one, **Then** the change is sent
   with the link request as the existing form does it.
3. **Given** no servers came (plain code), **When** the form shows, **Then** it shows the server blocks as today.

### Edge Cases

- A QR code scanned twice, or scanned after the link was already started with another code: the second scan replaces the code
  and current link's imported server values; it replaces a prior scan-suggested vault name if the user has not edited it. Manual
  server settings and edits are preserved, as are the device name and passphrases.
- The user already typed or edited a vault name and then scans: the typed name is kept; a link name is only a suggestion while
  the field is empty or still contains the previous untouched suggestion.
- A full link is replaced with a plain code: prior imported server values that remain untouched are cleared, while manual server
  settings and edits are retained.
- A vault name from the QR code that this device already has as a vault: the field is filled, and a taken name is reported when the link
  starts, as it is when the user types the name.
- A link that carries a code but a malformed server list (not a URL, wrong scheme, too many entries): the code is taken, the
  broken servers are dropped, and a toast says that the servers were not taken over so the user can enter them.
- A very long vault name or many servers: the QR code stays readable; the main device limits the name to 64 characters, and a text too large for a readable QR code shows no QR code but the
  complete link to copy (see FR-004).
- The preview is open when the app goes to the background (phone) or the window loses focus (desktop): the camera is released
  and the preview closes.
- Screen readers: the scan button has a text label, and everything the scan does is also reachable by keyboard without the
  camera.
- Someone photographs the QR code: it works like the typed code, valid once and ten minutes (the same exposure as today's
  code); the sheet on the main device says the QR code should only be shown to the user's own device.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The link QR code and the copyable link text of a main device MUST be one versioned text `haex-vault://link?...`
  that carries the link code, the vault name, and the Nostr-Relays and iroh-Relays of the vault including those switched off.
  The version MUST be part of the text so a later change can be told apart.
- **FR-002**: The text MUST NOT contain the passphrase, any key, any device secret or anything else that is not needed to reach
  the main device; only the code is secret, with the validity of spec 024 (one use, ten minutes).
- **FR-003**: The new device MUST accept three inputs in the code field and the scanner: the new link text, a plain code as
  today (typed, pasted or from an older QR code), and nothing else; every other text is refused with a toast.
- **FR-004**: The main device MUST bound the vault name (64 characters) and the servers (the limits of the server settings),
  and MUST show a QR code only when the text is small enough to scan reliably from a phone screen; otherwise it MUST say so
  and offer the complete link to copy. It MUST never shorten the text or drop a server silently, because a missing server can
  make the main device unreachable.
- **FR-005**: On a phone the sheet MUST offer "QR-Code scannen" using the camera scanner already used for contacts; the
  system's camera permission MUST be asked at the moment of tapping, not at start.
- **FR-006**: On a desktop the sheet MUST offer "QR-Code scannen" with a live camera preview in the sheet; every desktop OS
  holzi supports MUST work, and where the webview cannot give camera access on one of them, that OS gets a working path
  (system picker, helper, or a documented bridge) instead of the button disappearing.
- **FR-007**: The camera MUST be released when the scan succeeds, the preview or sheet closes, the app or window loses
  focus, or an error occurs; no frame MUST be stored, uploaded or logged. The scan decodes on the device.
- **FR-008**: Where there is no camera, the camera is busy, or the permission is denied, the sheet MUST say so in a toast
  with the way out (allow it, close the other app, paste the link); pasting and typing MUST always work.
- **FR-009**: A successful scan or paste MUST fill code, vault name (only into an empty field or by replacing an untouched
  suggestion from the previous scan) and servers, and MUST leave
  device name and passphrase untouched. The user MUST still review everything and press the button; a scan alone MUST NOT
  start the link.
- **FR-010**: When servers came with the link, the server blocks MUST be collapsed to one line with the count and be openable
  to the full list; switching off or adding servers MUST work as in the current form. When none came, the blocks MUST show as
  today.
- **FR-011**: A link text of a newer version than the build understands MUST be refused with a toast that asks to update
  HaexVault; a text of an older or equal version MUST work.
- **FR-012**: Old behavior MUST be unchanged: code format, parsing of spaces and dashes, proof of the code, confirmation on
  the main device, discarding on abort, and the code lifetime (spec 024, FR-023 to FR-025).
- **FR-013**: All errors of scanning and reading a link MUST show as a toast; the only inline message is a wrong input shown
  under its field. A success of a scan is visible in the filled form and MUST NOT get an extra message.
- **FR-014**: User-visible texts MUST say "Nostr-Relay", "iroh-Relay" or "Server", never a bare "Relay", and the product name
  "HaexVault"; texts exist in every language holzi ships.
- **FR-015**: Reading the link text MUST be one implementation that the form, the paste handler and the scanner share, and
  the main device's producer MUST be the counterpart of that parser in one place (one rule, one implementation).
- **FR-016**: The camera scan MUST only be reachable while the link sheet is open, and MUST NOT run in any other part of the
  app.

### Key Entities

- **Link text**: the versioned `haex-vault://link?...` text. Attributes: version, link code, vault name (suggestion), list of
  Nostr-Relays, list of iroh-Relays, which servers are switched off. Shown as a QR code and as copyable text by the main
  device, read by the new device from a scan, a paste or typing.
- **Link code**: unchanged from spec 024 (16 random bytes, 26 characters, one use, ten minutes).
- **Scan source**: where the text came from: phone camera, desktop camera, clipboard, or typing. Behavior after reading is
  the same for all.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user with both devices at hand finishes the form after the scan in under 30 seconds (device name and
  passphrase only), measured on a phone and on a desktop with a webcam.
- **SC-002**: A scan of a link QR code shown on a phone screen at normal reading distance succeeds on the first try in at
  least 9 of 10 attempts on each supported desktop OS and on a phone.
- **SC-003**: A user needs to type nothing else than device name and passphrase in 100% of links where the main device uses
  its own servers; today they must type the servers.
- **SC-004**: The camera is off within one second after a scan succeeds, the preview closes, or the sheet closes, on every
  supported platform.
- **SC-005**: Every old input (plain code typed, pasted, or from an older QR code) still links successfully, and a link from an
  older build's main device is not refused.
- **SC-006**: In an audit of the link text, it contains no passphrase and no key, and its code is the only secret.

## Assumptions

- The camera scanner for phones from spec 051 can be reused as is; the only addition there is the scan of the new text.
- A desktop webcam scan needs a decoder that runs on the device without a network; the choice, and how camera access works
  on each desktop OS's webview, is a planning question (research), not decided here. FR-006 says what must be true.
- The vault name from the main device is a suggestion for the new vault's name; the user may change it. (Today the field is
  empty and the user chooses.)
- A link made by the main device of a version before this feature carries only the code; the new build still links with it.
- A main device older than this feature cannot produce the new text; the new device reads its plain code as today.
- Showing the QR code to a camera on the same device (one desktop, two windows) is not a case to support.
- The sheet on the main device keeps its current layout; only the QR payload, the copy-link action and a one-line hint change.
- iOS stays out of scope until it is on the roadmap (a phone scan on iOS works through the same plugin, but is not verified here).
