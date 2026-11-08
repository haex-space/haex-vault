# Feature Specification: Vault folders

**Feature Branch**: `084-vault-folders`

**Created**: 2026-11-06

**Status**: Done 2026-11-08 (built in PRs A–D2 and E; the manual quickstart M1–M6 is the
operator's). Clarified 2026-11-06: automatic move at start, per-vault settings, folder root holds only SQLite
files, private mode comes later)

**Input**: User description: "Every vault gets its own folder. All files of a vault, SQLite files, the agent browser
profile, filter lists and whatever comes later, go into it. That bundles them and lets us put encryption on top later."
(operator, 2026-11-06, while building the DNS settings of spec 065, where a vault named `filters` turned out to share a
folder with the tracker lists.)

## Background

- **Today the files of one vault are scattered, each place keyed by the vault's name.** The database is
  `<AppLocalData>/instances/<name>.db`, with sidecar files beside it (`.lock`, `-wal`, `-shm`, `.vault-id`, the
  `.db.pending` marker during creation, `.trash` as a name the list skips, used by nothing today). The agent browser profile is
  `<AppLocalData>/agent-browser/<name>/`, the files of the chat are `<AppLocalData>/chat-files/<name>/`. Every module
  that adds files of its own invents another folder next to these.
- **Device-wide data lives in the same places.** The tracker filter lists and their settings are
  `<AppLocalData>/agent-browser/filters/`, the DNS choice is `<AppLocalData>/agent-browser/dns.json`. `filters` is a valid
  vault name, so a vault of that name and the tracker lists share one folder: "clear the browser profile" of that vault
  removes the lists and the user's choice with them. Two kinds of data that belong to different owners are told apart by
  nothing but a name.
- **Removing a vault's files is not one step.** There is no "delete vault" function yet; only the clean-up of a vault whose
  creation was interrupted removes a vault's files, and it must find each module's folder separately. A delete function
  that comes later would have the same problem: a folder that is forgotten stays on the device.
- **What the user wants.** One folder per vault that holds everything of the vault. Device-wide data stays out of it.
  The folder's root holds only the SQLite files; everything else lies in subfolders, so those can be encrypted later
  with keys the opened vault provides, while the vault itself opens as it does today.

## Clarifications

### Session 2026-11-06

- Q: Should the tracker and DNS choices belong to each vault or each device? → A: The tracker choice is per vault
  (operator): it may differ from vault to vault. The DNS choice is per device (operator): each device may use a provider
  that works on its network.
- Q: How do the choices reach the user's other devices? → A: Files in a vault folder never sync; the vault's synced
  tables do (ADR 0001). The choices therefore move into the vault's `preferences` table (see US3).
- Q: Move existing vaults automatically or after a confirmation? → A: Automatically at start (operator).
- Q: Is the vault encrypted as a whole? → A: No. The vault's SQLite files stay as they are; only subfolders are
  encrypted, later. Their keys come from the opened vault (operator).
- Q: What about a "private mode" for computers the user does not trust? → A: Later, on the roadmap (see "Later"); not
  part of this spec (operator).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Existing vaults move into their own folders (Priority: P1)

After the update the user starts holzi and finds every vault as before: same list, same data, same unlock. Behind the
scenes each vault now has its own folder with its database, and nothing of it is left in the old places.

**Why this priority**: Every other story builds on the layout, and a failure here costs the user a vault.

**Independent Test**: Start a build with the old layout on a data folder with three vaults (one with an agent browser
profile and chat files, one named `filters`, one open in another holzi process) and check the result by file listing and
by opening each.

**Acceptance Scenarios**:

1. **Given** vaults in the old layout, **When** holzi starts, **Then** each vault lies in `vaults/<name>/`, appears in
   the list, opens with its passphrase and has the same data as before.
2. **Given** a vault with an agent browser profile and chat files, **When** it has moved, **Then** both lie in
   subfolders of its folder and are found by the features that use them.
3. **Given** the move was interrupted (power loss, killed process), **When** holzi starts again, **Then** it continues,
   no vault is lost or listed twice, and the result equals an uninterrupted move.
4. **Given** a vault that another holzi process of this device has open, **When** holzi starts, **Then** that vault is
   not moved, stays usable where it is and is moved at a later start.
5. **Given** a vault named `filters`, **When** holzi starts, **Then** it moves like any other and the tracker lists are
   untouched.

---

### User Story 2 - One folder holds everything of a vault (Priority: P1)

Every module that keeps files for a vault asks one place for the vault's folder and puts its files in a subfolder named
for its purpose. The folder's root holds only the vault's SQLite files.

**Why this priority**: This is the point of the change: one place to find, delete, and later encrypt a vault's files.

**Independent Test**: Create a vault, use the agent browser and the chat files, interrupt another creation, and check
that nothing of it is left; list the root of a vault's folder.

**Acceptance Scenarios**:

1. **Given** a vault, **When** the agent browser, the chat files or any other module keeps files for it, **Then** they
   lie in a subfolder of the vault's folder, never in the root and never in a folder outside it that is named after the
   vault.
2. **Given** a vault whose creation was interrupted, **When** holzi starts, **Then** its folder is gone, with everything in
   it, and nothing of it is left elsewhere on the device.
3. **Given** a vault named like a device-wide folder (`filters`, `models`, `agent-browser`), **When** it is created and
   used, **Then** nothing of it and nothing device-wide interferes with the other.
4. **Given** importing a vault file (spec 037, 043), **When** it succeeds, **Then** the vault has its own folder and the
   import still takes and produces a single database file.

---

### User Story 3 - Tracker and DNS choices belong to the vault and sync (Priority: P2)

The tracker blocking choice (switched off lists, own lists) and the DNS over HTTPS choice are kept in the vault, so each
vault has its own and the choices reach the user's other devices.

**Why this priority**: It follows from "per vault" and from the rule that only the vault's tables sync; it can ship after
the layout.

**Independent Test**: Set a choice in vault A, open vault B (another process), compare; set a choice on one device and
read it on a second one.

**Acceptance Scenarios**:

1. **Given** two vaults, **When** the user sets different tracker lists in each, **Then** each vault's browser uses its
   own.
2. **Given** the tracker choice is set on one device, **When** the other device has synced, **Then** it shows and uses the
   same choice (vault-wide).
3. **Given** the DNS choice is set on one device, **When** the other device has synced, **Then** the other device keeps
   its own DNS choice (per device: a network decides which provider works).
4. **Given** the files `filters/settings.json` and `dns.json` of the current layout, **When** a vault without a choice is
   opened for the first time, **Then** it takes the choice from them once and the files are removed.

---

### Edge Cases

- A vault whose folder name would collide with a leftover of the old layout (a file `<name>`): the move never overwrites
  anything; it stops for that vault and logs why.
- Not enough room or a read-only data folder: the vault stays in the old place and works; the move is tried again at the
  next start.
- The same vault name in the old and the new layout (a half moved vault): the move continues from the state the
  interrupted step left; a case it cannot tell apart is left untouched and reported, never resolved by guessing which
  database is the right one.
- Android and Windows: the same layout; a move is a rename within one volume, never a copy across volumes.
- A vault copy (spec 043 recognises one by its `.vault-id`) keeps working: the id file stays beside the database.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Every vault MUST have exactly one folder, `<AppLocalData>/vaults/<name>/`, named like the vault today
  (`^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$`).
- **FR-002**: The root of a vault's folder MUST hold only the vault's SQLite files (database, `-wal`, `-shm`) and the few
  small bookkeeping files that are needed before the vault opens (lock, vault id, creation marker). Every other file of
  the vault MUST lie in a subfolder named for its purpose (`agent-browser/`, `chat-files/`, further ones later). The database is `vault.db` in it, the other files carry its
  name as before (`vault.db-wal`, `vault.db.lock`, `vault.db.vault-id`, `vault.db.pending`).
- **FR-003**: One function MUST resolve the vault's folder and the paths in it. No module builds a path to a vault's
  files from the vault's name by itself (constitution: one implementation per rule).
- **FR-004**: At start, before the vault list is read, holzi MUST move every vault of the old layout into its folder:
  the database with its sidecar files, `agent-browser/<name>/` to `vaults/<name>/agent-browser/` and
  `chat-files/<name>/` to `vaults/<name>/chat-files/`. The move MUST be automatic and use renames within one volume.
- **FR-005**: The move MUST be restartable. After an interruption the next start completes it; at no moment may a vault be
  listed twice, be unlisted, or fail to open for the move. The move MUST NOT delete or overwrite data of a vault.
- **FR-006**: A vault that another process of this device holds open MUST NOT be moved while it is open; it is moved at a
  later start.
- **FR-007**: Removing a vault's files (today the clean-up of an interrupted creation; later a delete function) MUST remove
  its folder with everything in it. No file of a vault lies outside its folder once the move of FR-004 is done.
- **FR-008**: Device-wide data MUST NOT lie in a vault's folder or in a folder a vault name can address. The tracker list
  cache (downloaded public lists) MUST move to `<AppLocalData>/filter-lists/`; the models, the installation id and the
  deep link inbox stay where they are.
- **FR-009**: The tracker choice MUST be stored in the vault's `preferences` table as a vault-wide row, the DNS choice as
  a row of the device that set it (ADR 0001), so they sync and no file of them lies outside the vault. A vault that has
  no choice yet MUST take the choice of the legacy files once (US3 scenario 4); a vault that has one MUST NOT read them.
- **FR-010**: Importing a vault MUST still accept and produce one database file, and the vault list MUST show the same
  entries as before the move.
- **FR-011**: The layout MUST work the same on Linux, Windows, macOS and Android; a rule that holds on one platform only
  is a bug.

### Key Entities

- **Vault folder**: `<AppLocalData>/vaults/<name>/`; root holds the SQLite files and bookkeeping files, subfolders hold
  everything else.
- **Device-wide data**: models, installation id, deep link inbox, tracker list cache; outside `vaults/`.
- **Legacy layout**: `instances/<name>.db` and its sidecars, `agent-browser/<name>/`, `chat-files/<name>/`,
  `agent-browser/filters/`, `agent-browser/dns.json`.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: After the update every vault of a test data folder with at least five vaults opens with its data intact.
- **SC-002**: After the update no database file lies in `instances/` and no vault-named folder lies in `agent-browser/` or
  `chat-files/`.
- **SC-003**: Killing holzi at every step of the move, one run per step, always ends with all vaults moved after the next
  start.
- **SC-004**: The clean-up of an interrupted creation leaves no file of that vault on the device (checked by a file listing
  of the data folder).
- **SC-005**: Vaults named `filters`, `models` and `agent-browser` work next to the device-wide data without interfering.

## Assumptions

- A vault's name stays its identity on the device; renaming a vault is not part of this spec.
- The vault database is not encrypted as a whole beyond SQLCipher as today; later encryption applies to subfolders and
  takes its keys from the opened vault, in the way of spec 048 and ADR 0013. Nothing in this spec encrypts a folder.
- The chat files folder and the agent browser profile folder keep their own contents and formats; only their place
  changes.
- The vault's `preferences` table and its device rows exist as ADR 0001 describes.

## Later (not part of this spec)

- **Private mode**: the landing page offers a checkbox "this computer is trusted", default trusted. For a computer that is
  not trusted, everything the user would otherwise share device-wide, the language models included, goes into the vault's
  folder, and that folder's subfolders are encrypted (operator 2026-11-06). The layout of this spec is the precondition:
  one folder per vault, SQLite files in the root, everything else in subfolders that can be encrypted. Its own spec.
