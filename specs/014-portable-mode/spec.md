# Feature Specification: Portable Mode with Protected Data

**Feature Branch**: `014-portable-mode`

**Created**: 2026-09-21

**Status**: Draft

**Input**: User description: "A user wants to use their vault on a computer they do not own, for
example in an internet cafe, without leaving readable or usable data behind. HaexVault gets a portable
mode with two ways to use it. The first is started from removable storage such as a USB stick: it
keeps everything it stores on that storage and nothing on the computer. The second is a single
file that the user runs on the computer without installing anything system-wide and without
administrator rights: everything it stores goes into one protected container on the computer.
In both, the stored data is protected, so that someone who finds the stick or uses the computer
afterwards cannot read or use it, for example a user's own local model. Normal installations stay
as they are and get no extra encryption. The protection must work without administrator rights or
drivers on the computer and must be honest about what it cannot protect against."

## Clarifications

### Session 2026-09-21

- Q: Who is this for and what is the goal? → A: A user who wants to work in their vault on a
  computer they do not own and leave no readable or usable data of theirs behind.
- Q: How does a user get this behavior? → A: In one of two ways: by running the app from removable
  storage in portable mode, or by running a single file on the computer that needs no system-wide
  installation and no administrator rights. A normal installation stays unchanged and gets no extra
  encryption.
- Q: Is removable storage required? → A: No. With the single file, all data the app stores lives
  in one protected container on the computer's own disk instead of on a stick. Removable storage
  remains the second way.
- Q: What does it protect against? → A: The next person to use the computer, and loss or
  inspection of the storage. It does not protect against a compromised or hostile computer, such as
  a keylogger, screen capture or inspection of memory while the app runs.
- Q: What about files that agents create or change on the computer on the user's request? → A: Those
  are deliberate and remain possible. They are outside the guarantee.
- Q: Should the protection rely on an encrypted volume from the operating system? → A: No. The
  protection must work without administrator rights or drivers, so the app provides it itself.
- Q: What if data cannot be protected without writing readable data to a disk? → A: On removable
  storage, the user still gets a clearly worded weaker mode, where readable data exists only on the
  portable storage while the app runs, together with the advice to encrypt the storage with the
  operating system. On the computer's own disk there is no weaker mode: readable data of the user
  is never written there, and the app refuses to store data there instead. The spec is updated
  after the feasibility check described in the Assumptions.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Use HaexVault from a stick and leave nothing on the computer (Priority: P1)

A user plugs a USB stick into a computer they do not own and starts HaexVault from it in portable
mode. They unlock their vault, chat, install or import a model, close the vault and remove the
stick. Everything the app stored during that time, meaning the vault, models, settings, temporary
files and the interface's stored state, lives on the stick. Nothing of it remains on the computer.

**Why this priority**: This is the first slice of the feature and needs no protection layer to be
useful. Without a reliable, complete portable location there is nothing to protect, and no
protection helps if the app quietly writes to the computer as well.

**Independent Test**: Run a full session from a stick while watching every file the app creates or
changes on the computer, then remove the stick. Fully testable on its own and delivers "no app data
left on the computer".

**Acceptance Scenarios**:

1. **Given** the app runs in portable mode from a stick, **When** the user creates or unlocks a
   vault, chats, installs or imports a model and uses tools, **Then** the app creates or changes no
   file outside the portable location.
2. **Given** the user closes the vault and the app relaunches (feature 013), **When** the new app
   process starts, **Then** it is still in portable mode with the same portable location.
3. **Given** the portable location is missing, read-only or unusable when the app starts, **When**
   the app cannot keep its data there, **Then** it refuses to start with a clear message and writes
   nothing to the computer.
4. **Given** the computer also has a normal installation of the app, **When** the portable app
   runs, **Then** it neither reads nor changes the data of the normal installation, and the normal
   installation does not see the portable data.
5. **Given** the app needs somewhere to put temporary or cached data, **When** it does, **Then**
   that data goes to the portable location and not to the computer's temporary or cache locations.

---

### User Story 2 - The stored data is protected if it is lost or inspected (Priority: P2)

The user loses the stick, or someone else plugs it into another computer. Without the passphrase
they cannot read or use what is on it: not the user's own local models, not downloaded or imported
files, not settings, and not the names of what the user stores. The user needs no administrator
rights or extra software on the computer to get this protection. The same protection applies to the
container of the single-file form in User Story 3.

**Why this priority**: It closes the gap that a plain portable location leaves open. The user's
own model is often their most valuable asset, and a stick is easy to lose. It follows the portable
location, because protecting data that is scattered over the computer would achieve nothing. It is
also what makes User Story 3 possible at all.

**Independent Test**: Use portable mode with recognizable marker content in every data category,
close the vault, and scan the raw contents of the stick from outside the app for the markers.
Testable on its own once the portable location exists.

**Acceptance Scenarios**:

1. **Given** the vault is closed, **When** someone examines the storage without the passphrase,
   **Then** no model, downloaded or imported file, chat content or setting is readable or usable.
2. **Given** the vault is closed, **When** someone lists the files on the storage, **Then** names
   that reveal what the user stores, such as model names, are not readable.
3. **Given** the user unlocks the vault on another computer, **When** they use a protected model,
   **Then** it works as before, without administrator rights or extra software.
4. **Given** a wrong passphrase, **When** the user tries to unlock, **Then** nothing protected is
   exposed, not even in part.
5. **Given** a session has ended, **When** the process is gone, **Then** the keys that protected
   the data are no longer held anywhere, and they follow the same secret-handling rules as the
   passphrase (feature 013).

---

### User Story 3 - Run a single file on the computer without installing anything (Priority: P2)

A user is on a computer they do not own and has no stick. They download or copy one file, run it,
and can use HaexVault at once. They install nothing system-wide, are not asked for administrator
rights, and change no setting of the computer. On the first start they choose where a protected
container is created, or accept a default in their own user area. From then on everything the app
stores goes into that container and nowhere else. When the session ends, the container is
unreadable without the passphrase. The user can delete it, and the app offers to delete it at the
end of the session, so that none of their data remains.

**Why this priority**: It serves the user who has no stick at all, and it makes the feature usable
in the most common shared-computer situation. It ranks with the protection story because the
container is only acceptable on the computer's own disk if it is protected.

**Independent Test**: On a computer where the tester has no administrator rights, run the single
file, create a container, work with a vault and a local model, end the session, and compare the
computer before and after and inspect the container from outside the app. Testable once the
protection exists.

**Acceptance Scenarios**:

1. **Given** the user has no administrator rights, **When** they run the single file, **Then** the
   app starts without an elevation prompt and makes no system-wide change: no installation, no
   startup entry, no shortcut and no file association.
2. **Given** the first start, **When** the user creates or chooses the protected container,
   **Then** everything the app stores from then on goes into that container, and the app persists
   nothing else on the computer.
3. **Given** the session has ended, **When** someone examines the computer without the passphrase,
   **Then** the only things of the app they find are the single file and the container, and the
   container's content, including vault names, is unreadable.
4. **Given** the container's name, **When** someone sees it in a file list, **Then** it does not
   reveal that it belongs to HaexVault or what it holds, unless the user chose such a name.
5. **Given** the user ends the session, **When** the app offers to delete the container and the
   user accepts, **Then** the container is removed and no data of the user remains in the app's
   own locations.
6. **Given** the user runs the single file again on a later day, **When** they unlock their
   container, **Then** their data is there as they left it.
7. **Given** the computer does not have enough free space for the container, **When** the user
   creates it, **Then** the app says so before creating anything.
8. **Given** protection of the data cannot be provided on the computer's own disk, **When** the
   user starts the single file, **Then** the app refuses to store data there and says why. It never
   stores readable data on the disk.

---

### User Story 4 - The user knows exactly what is and is not protected (Priority: P3)

The user opens portable mode for the first time and is told in plain language what it protects
against and what it cannot: the next person to use the computer and loss of the stick or container
are covered; a hostile computer with a keylogger, screen capture or memory inspection is not; and
the computer's own records, such as paging files, hibernation, crash reports, recently used lists,
download and security-check records, and logs of connected devices, may still show that something
was used. For the single file, the statement also says that the file itself and the container's
existence, size and dates remain visible, and that deleting a file does not guarantee that it can
never be recovered from every kind of disk.

**Why this priority**: A user in a hurry on a foreign computer decides based on what they believe
is protected. An overstated promise is a security risk in itself.

**Independent Test**: Start portable mode for the first time and read the statement and the
documentation. Verify that each category above is named. Testable on its own.

**Acceptance Scenarios**:

1. **Given** the user starts portable mode for the first time, **When** the app opens, **Then** a
   plain-language statement of what is and is not protected is available before the user unlocks a
   vault.
2. **Given** the statement, **When** the user reads it, **Then** it names the next user of the
   computer and loss of the storage as covered, and a compromised computer and the computer's own
   records as not covered.
3. **Given** an agent creates or changes a file on the computer on the user's request, **When** it
   does, **Then** the app does not block or silently redirect it, and the statement has told the
   user that this leaves a trace.
4. **Given** the single file, **When** the user reads the statement, **Then** it also names the
   visible file, container and dates and the limits of deleting files.

---

### User Story 5 - Portable mode says so when the computer cannot run it (Priority: P3)

Some computers block running programs from removable media or from user folders, or lack a
component the app needs. Where the app can run at all, it recognizes what is missing, says so in
plain language and leaves no data on the computer.

**Why this priority**: The failure is likely on locked-down shared computers. A silent failure or a
fallback that writes to the computer would defeat the purpose.

**Independent Test**: Start portable mode on a computer without a required component or with the
portable location unusable and observe the message and the computer's file system. Testable on its
own.

**Acceptance Scenarios**:

1. **Given** a required component is missing where the app can still start, **When** the user
   starts portable mode, **Then** the app names the missing component and stops without writing
   data to the computer.
2. **Given** the computer blocks running the app, **When** the user tries, **Then** nothing of the
   app's data is left on the computer.

---

### User Story 6 - Normal installations do not change (Priority: P3)

A user who installs HaexVault normally on their own computer notices no difference: same data
location, no additional encryption, same speed.

**Why this priority**: Portable mode must not cost the majority of users anything.

**Independent Test**: Run the existing behavior of a normal installation before and after the
feature and compare. Testable on its own.

**Acceptance Scenarios**:

1. **Given** a normal installation, **When** the user uses the app, **Then** data stays in the
   normal per-user location and nothing is encrypted in addition.
2. **Given** a normal installation, **When** the app starts, **Then** no portable-mode behavior
   such as the first-start statement appears.

---

### Edge Cases

- The stick is removed while the app runs: the session ends safely, the vault is not damaged beyond
  work in flight, and the app writes nothing to the computer as a fallback.
- The stick or the disk is full while a model is installed: the install fails cleanly with a clear
  message and no partial file remains.
- The app crashes in portable mode: the protected data stays protected, and the computer keeps only
  what its own operating system records, which the statement describes.
- The stick is slow: loading a large model takes longer and the app shows progress, as it does for
  slow disks today.
- Two portable app processes start from the same stick or the same container: the rule for opening
  one vault twice from feature 013 applies.
- The vault's name is needed to list vaults before unlocking on a stick: it may be readable there,
  and the statement tells users to choose a neutral name if that matters. In the single-file form
  the names are inside the protected container.
- A helper program that the app starts for the user wants to write temporary data: it is given the
  portable location, or the action is refused. It never writes to the computer's temporary
  location on its own.
- The user unlocks the vault on a computer with a keylogger: outside the guarantee, and named in
  the statement.
- The container is copied to another place or damaged: without the passphrase a copy is as
  unreadable as the original, and a damaged container fails to unlock without exposing anything.
- The computer's security software removes or blocks the single file: no data of the user is
  affected, and the container stays intact.
- The user leaves the container on the computer at the end of the session: it stays protected, and
  the statement says that its existence, size and dates remain visible.
- The user runs the single file on a computer where a normal installation of the app exists: the
  two do not see each other's data.

## Requirements _(mandatory)_

### Functional Requirements

**Portable location**

- **FR-001**: The user MUST be able to run the app in portable mode in two ways: from removable
  storage, or as a single file on the computer. Whether a run is portable, and where its portable
  location is, MUST be determined when the app starts and MUST be kept when the app relaunches
  after a close (feature 013).
- **FR-002**: In portable mode, everything the app stores MUST live in the portable location: vault
  files, model files, downloaded and imported files, settings and preferences, temporary and cached
  data, diagnostic output and the interface's stored state.
- **FR-003**: In portable mode, the app MUST NOT write persistent data anywhere else on the
  computer than the portable location. If it cannot keep something there, it MUST refuse the
  operation or the start with a clear message and MUST NOT fall back to another location on the
  computer.
- **FR-004**: A portable run MUST NOT read or change the data of a normal installation on the same
  computer, and a normal installation MUST NOT see the portable data.
- **FR-005**: The portable location MUST be movable between computers by moving the storage, with
  the app and its data continuing to work. For the single-file form, the container MUST be movable
  or copyable as one item.
- **FR-006**: If the portable storage becomes unavailable while the app runs, the session MUST end
  safely without writing to the computer as a fallback.

**Single file on the computer**

- **FR-007**: The single-file form MUST run without installation, without administrator rights and
  without system-wide changes to the computer, such as installed programs, startup entries,
  shortcuts and file associations. A distribution form that installs system-wide, such as an
  installer package, does not satisfy this.
- **FR-008**: In the single-file form, everything the app persists, other than the file itself,
  MUST live in one protected container at a location the user chooses, with a default in the
  user's own area of the computer.
- **FR-009**: The container MUST NOT reveal what it holds. Its default name MUST NOT contain the
  product name, and its content, including vault names, MUST NOT be readable from outside the app.
- **FR-010**: The user MUST be able to delete the container, and the app MUST offer to delete it at
  the end of a session. After deletion no data of the user MUST remain in the app's own locations.
- **FR-011**: Before creating a container, the app MUST check that the computer has enough free
  space and say so if it does not.

**Protection**

- **FR-012**: In portable mode, data at rest in the portable location that belongs to the user's
  vaults, such as model files, downloaded and imported files, caches and settings, MUST be
  protected so that it cannot be read or used without the vault's passphrase.
- **FR-013**: The protection MUST NOT require administrator rights, drivers or additional software
  on the computer.
- **FR-014**: Names that reveal what the user stores, such as the names of models and imported
  files, MUST NOT be readable at rest.
- **FR-015**: Protected models MUST remain usable in normal operation: a user can chat with a
  protected local model and install or import further ones.
- **FR-016**: The keys protecting the data MUST exist only for the duration of the session. They
  MUST follow the same handling rules as the passphrase (feature 013): never written in readable
  form, never in logs, erased from memory as early as possible, and gone when the process ends.
- **FR-017**: A wrong passphrase MUST NOT expose any protected data, not even in part.
- **FR-018**: Readable data of the user MUST NEVER be written to the computer's own disk in
  portable mode. If protection cannot be provided for the single-file form, the app MUST refuse to
  store data on the computer. On removable storage, if protection cannot be provided for a
  category of data, the app MUST say so before the user relies on it, MUST NOT present that data as
  protected, and MAY offer the clearly worded weaker mode.

**Honesty about limits**

- **FR-019**: In portable mode, the app and its documentation MUST state what is protected and what
  is not: the next user of the computer and loss of the storage are covered; a compromised
  computer, and the computer's own records such as paging files, hibernation, crash reports,
  recently used lists, download and security-check records and logs of connected devices, are not.
  For the single-file form the statement MUST also say that the file itself and the container's
  existence, size and dates remain visible, and that deleting a file does not guarantee that it
  cannot be recovered from every kind of disk.
- **FR-020**: Actions that agents take on the computer's own files on the user's request MUST NOT
  be blocked or silently redirected by portable mode. They are outside the guarantee, and the
  statement MUST say so.
- **FR-021**: Where the app can start but a required component is missing, it MUST name what is
  missing in plain language and MUST leave no data on the computer.

**Normal installations**

- **FR-022**: A normal installation MUST behave exactly as before: same data location, no
  additional encryption and no portable-mode messages.

### Key Entities

- **Portable mode**: A way of running the app in which it keeps all its data in one portable
  location. It comes in two forms, removable storage and a single file on the computer. Chosen when
  the app starts and kept across relaunches.
- **Portable location**: The removable storage, or the protected container on the computer, that
  holds the app's data in portable mode.
- **Single file**: One file that runs without installation, without administrator rights and
  without system-wide changes.
- **Protected container**: One item on the computer's disk that holds all data of the single-file
  form and cannot be read without the passphrase.
- **Protected data**: The user's data at rest in the portable location that cannot be read or used
  without the passphrase, such as models, downloaded and imported files, caches and settings.
- **Session key**: A key that protects the data during one vault session. It exists only in memory
  and only for that session.
- **Trace**: Anything left on the computer, or readable on the storage, that shows or reveals what
  the user did or holds.
- **Statement of limits**: The plain-language description of what portable mode does and does not
  protect against.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In a full portable session, covering creating or unlocking a vault, chatting with a
  local model, installing or importing a model, using a tool and closing the vault, monitoring the
  computer shows 0 files created or changed by the app outside the portable location.
- **SC-002**: After a session from a stick and removing the stick, a scan of the computer for files
  created by the app finds none, apart from records the operating system itself keeps and that the
  statement of limits names.
- **SC-003**: Scanning the raw contents of the storage or the container without the passphrase,
  after a session with a known marker in each data category, finds 0 markers, and lists 0 readable
  model or vault names.
- **SC-004**: A user can complete a chat with a protected local model from the storage or the
  container and close the vault without an error caused by the protection. Load time is limited by
  the storage speed, not by the protection.
- **SC-005**: In 100% of tested relaunches after a close, the new app process is still in portable
  mode with the same portable location.
- **SC-006**: In 100% of attempts to start with an unusable portable location, the user sees a
  plain-language message and 0 files are written to the computer.
- **SC-007**: A normal installation behaves the same before and after the feature in every existing
  test, and shows no portable-mode message.
- **SC-008**: The statement of limits is available before unlocking in portable mode and names each
  category of what is and is not covered.
- **SC-009**: Starting the single file on a computer without administrator rights shows 0
  elevation prompts, and a comparison of the computer before and after a session shows 0
  system-wide changes.
- **SC-010**: After a session of the single-file form, the only items of the app on the computer are
  the file and the container, and after the user deletes the container 0 data files of the user
  remain in the app's own locations.
- **SC-011**: In 100% of attempts where protection cannot be provided on the computer's own disk,
  0 bytes of readable user data are written there.

## Assumptions

- This feature builds on feature 013 (vault lifecycle isolation): one app process serves one vault
  session, closing ends the process, a relaunch keeps the process configuration, and secrets follow
  the handling rules there. For the single file, a relaunch starts the same file with the same
  container.
- The first supported platform for portable mode is Windows, because shared computers such as those
  in internet cafes mostly run it. Other platforms are decided in planning after checking what each
  operating system provides, such as the web view component, whether a single file needs a helper
  component, and warnings about unsigned programs.
- The single-file form is a file that runs without installing. The exact file kind per platform is
  decided in planning. An installer package does not qualify, because it changes the computer
  system-wide.
- The computer allows running programs from removable media or from the user's own area. Computers
  that forbid this are out of scope, and the app cannot help there.
- The protection is provided by the app itself, without mounting an encrypted volume, because that
  needs drivers and administrator rights that a user on a foreign computer usually lacks.
- Whether protected local models can be loaded without writing readable data to any disk is
  unverified today, because local models are currently loaded from a file path. A feasibility check
  gates the protection stories and the single-file form. If it fails, the weaker mode applies to
  removable storage only. The single-file form cannot be offered without protection, because
  readable data must never reach the computer's own disk.
- The relationship between the passphrase that unlocks the container and the vaults' passphrases,
  and whether the container grows or has a fixed size, are decided in planning.
- The threat model is the next person to use the computer and loss or inspection of the storage. A
  compromised computer is out of scope.
- The vault file is already encrypted with the passphrase. This feature adds protection for the
  other data in the portable location.
- Vault names may be readable on removable storage, because the app lists vaults before
  unlocking. Users can choose neutral names. In the single-file form they are inside the container.
- The computer's own records cannot be prevented by the app. Where reducing them is cheap, planning
  may do so, and the statement names the remainder.
- The container and the single file remain on the computer until the user deletes them. Their
  existence is a visible trace, which the statement names.
- How a vault gets onto the computer, for example by copying or syncing, is outside this feature.
- Files that agents create or change on the computer on the user's request are outside the
  guarantee.
- Normal installations receive no additional encryption in this feature.
