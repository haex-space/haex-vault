# Feature Specification: Diagnostic log

**Feature Branch**: `spec/diagnostic-log`

**Created**: 2026-11-21

**Status**: Draft

**Input**: User description: "ich hätte eh gerne die möglichkeit, mehr logging zeitweise zu aktivieren. als nutzer mit einem
problem würde ich gerne ein flag oder log level hochsetzen wollen, damit infos geloggt werden, mit denen dann weitere
analysen besser möglich sind" (operator, 2026-11-21), with the decisions of the same session below.

## Background

When something goes wrong, the user often sees one short sentence and nothing else. On 2026-11-21 an old vault file could
not be imported: the app said "Die Datei ist beschädigt". The real cause (a column that held bytes where text was expected)
was written to a log, but only a development build writes that log at all. A release build of haex-vault writes none, so a
user with a problem has nothing to send and the developer has nothing to read.

The log that exists in development is also fixed: one level, one global place, backend only. What the window does when
something fails in the interface never reaches it.

This spec makes the log a feature of the app: it exists in every build, the user can raise its level for a while when
something is wrong, backend and interface write into the same log, and the log lies with the vault it belongs to. The log
begins when the vault has been unlocked; before that nothing is logged.

## Decisions

Decided by the operator on 2026-11-21:

- **The switch is in the app**, in the settings, not only a start parameter. A user with a problem does not start the app
  from a terminal.
- **It takes effect at once**, without a restart, for the backend and the interface together.
- **Interface and backend share one log.** Messages of the interface (warnings, errors, uncaught errors, rejected promises,
  errors of the view layer) are written to the same log, marked as coming from the interface. Only the text and the stack
  of a message are kept, never the objects it was called with.
- **"Ausführlich" does not stay on.** It falls back to "Standard" after 24 hours or at the next start of the app, whichever
  comes first.
- **The log lies in the folder of the vault**, not in a global folder: the folders below the vaults folder of the app's data
  directory. Every vault has its own log.
- **Nothing secret is ever logged**, not even in "Ausführlich": no passwords, no keys, no content of a vault.
- **Logging begins when the vault has been unlocked** and not before. Lines from before (start of the app, the list of vaults,
  the unlock itself) are not kept and not written, and there is no global log. A vault that does not open therefore leaves no
  log; the operator accepts that.
- **Android gets "Protokoll teilen"** through the system's share sheet, since there is no folder to open there.
- **Writing the extra lines is a later step per area** (import, sync, the steps after the unlock). This spec delivers the switch, the
  log in every build and the interface messages; the lines that make a given area readable come with that area.

## Clarifications

### Session 2026-11-21

- Q: How does the user raise the level for a vault that does not open? → A: Not needed. Logging begins when the vault has
  been unlocked and not before, so there is no log, no level setting and no buffer for a vault that does not open.

## User Scenarios & Testing

### User Story 1 - A failure leaves a trace the user can pass on (Priority: P1)

The user imports a file or runs another step in an open vault and the app says only that it failed. The user opens the settings, finds
"Protokoll", sets it to "Ausführlich", repeats the step, and then opens the folder of the vault, where the log lies. The log
names what the app tried and why it failed. The user sends that file to whoever helps.

**Why this priority**: it is the reason for the feature. Without a log in a release build, nothing else here has a use.

**Independent Test**: in a release build, make a known step fail (a wrong file for the import), switch to "Ausführlich", repeat
it, open the log in the vault's folder and find the cause.

**Acceptance Scenarios**:

1. **Given** a release build with "Standard", **When** a step fails with a warning or an error, **Then** the log in the folder
   of the open vault holds a line with the time, the level, where it came from and what happened.
2. **Given** "Standard", **When** the user sets "Ausführlich", **Then** the next steps already write the more detailed lines,
   without a restart.
3. **Given** "Ausführlich" is on, **When** 24 hours have passed or the app starts again, **Then** the level is "Standard" again
   and the settings show it.
4. **Given** the settings, **When** the user looks at "Protokoll", **Then** they see the current level, and while "Ausführlich"
   is on, when it ends.

---

### User Story 2 - Errors of the interface are in the same log (Priority: P2)

Something in the window breaks: a button does nothing, a list stays empty. The error appears in the interface only. With this
feature it is also in the log, next to the lines of the backend from the same moment, so the sequence can be read in one place.

**Why this priority**: many failures begin in the interface and end in the backend, or the other way round. One log shows the
chain; two would not.

**Independent Test**: cause an error in the interface and an error in the backend one after the other and read both, in order,
in the same log.

**Acceptance Scenarios**:

1. **Given** the app is running, **When** the interface reports a warning, an error, an uncaught error, a rejected promise or an
   error of the view layer, **Then** the log holds a line marked as coming from the interface, with its text and its stack.
2. **Given** "Standard", **When** the interface writes informational or debug messages, **Then** they are not in the log.
3. **Given** "Ausführlich", **When** the interface writes informational or debug messages, **Then** they are in the log.
4. **Given** a message of the interface called with an object that holds the content of an entry, **When** it is logged,
   **Then** only its text and stack are in the log, not the object.

---

### User Story 3 - The user finds and passes on the log (Priority: P3)

The user wants to send the log. On a computer the settings offer a button that opens the folder with the log. On a phone
there is no folder to open, so the settings offer "Protokoll teilen", which hands the log to the system's share sheet.

**Why this priority**: the log helps only when it reaches someone. The user can also find the file without the button, so it
is a convenience, not a blocker.

**Independent Test**: press the button on a computer and find the log in the folder it opens; press "Protokoll teilen" on a
phone and see the log offered to the share sheet.

**Acceptance Scenarios**:

1. **Given** the settings on a computer, **When** the user presses "Ordner mit dem Protokoll öffnen", **Then** the file manager
   shows the folder of the open vault with its log.
2. **Given** the settings on a phone, **When** the user presses "Protokoll teilen", **Then** the system's share sheet offers the
   log of the open vault as a file, and the user chooses where it goes.
3. **Given** the share sheet is closed without a choice, **When** the user looks at the vault, **Then** nothing has changed and
   nothing was sent.

---

### Edge Cases

- A line is produced before a vault has been unlocked (start of the app, the list of vaults, the unlock itself): it is dropped.
  Nothing is buffered and no global log exists.
- The user chooses a vault that does not open (damaged file, wrong passphrase): no log is written and the settings are out of
  reach, as before this spec. This is deliberate; the failure is shown in the window only.
- The interface reports an error on a screen before a vault is open (the vault list): it is dropped like any line before the
  unlock.
- Two vaults are opened one after the other: each log holds only the lines from the time that vault was the open one, and the
  level of one does not change the level of the other.
- The vault is deleted: its log goes with its folder.
- The disk is full or the folder cannot be written: the app keeps working, and the settings say that the log cannot be
  written.
- "Ausführlich" is on and the log reaches its limit: the oldest lines go first; a session of normal use still keeps its
  beginning (see SC-004).
- The clock jumps while "Ausführlich" is on: the end is computed from the time of the switch and cannot be extended by changing
  the clock backwards.
- The interface produces thousands of messages a second (a loop): the log notes how many were dropped instead of filling up the
  file and hiding the lines around it.
- A third-party library logs very much (the sync network): its lines stay damped, so the lines of haex-vault survive the limit.
- The vault folder is copied or shared with someone: the log goes with it; the settings say that it does, because the user may
  not expect it.
- A message already contains a secret before it reaches the log (text composed by the app): FR-009 holds for all text, and
  the test uses known secret strings.
- The extension frames of a vault (third-party pages) write to their own console: their output is not part of this log.

## Requirements

### Functional Requirements

- **FR-001**: The settings MUST offer a section "Protokoll" with two levels, "Standard" and "Ausführlich", and show the
  current one.
- **FR-002**: A change of the level MUST take effect at once, for the backend and the interface, without a restart.
- **FR-003**: "Ausführlich" MUST end by itself after 24 hours or at the next start of the app, whichever comes first, and the
  settings MUST show when it ends while it is on.
- **FR-004**: The log MUST be written in every build of the app, at least at "Standard".
- **FR-005**: The interface's warnings, errors, uncaught errors, rejected promises and errors of the view layer MUST be written
  to the same log, marked as coming from the interface. Informational and debug messages of the interface MUST be written only
  in "Ausführlich".
- **FR-006**: A message of the interface MUST be logged as its text and its stack only, never with the objects it was called with.
- **FR-007**: The log MUST lie in the folder of the vault it belongs to, below the vaults folder of the app's data directory.
  Every vault MUST have its own log, and a line MUST NOT be written into the log of another vault.
- **FR-008**: Logging MUST begin when the vault has been unlocked. Before that, the app MUST NOT write, keep or buffer any log line.
- **FR-009**: The log MUST NOT contain passwords, keys, passphrases or the content of a vault, at any level, in any line of the
  backend or the interface.
- **FR-010**: The app MUST NOT create a global log. When the vault is closed, its log MUST stop receiving lines.
- **FR-011**: The size of the log MUST be bounded, with a higher limit in "Ausführlich" than in "Standard", and the oldest lines
  MUST go first.
- **FR-012**: The lines of very chatty third-party libraries MUST stay damped at every level, so the lines of haex-vault are not
  rotated away by them.
- **FR-013**: Every line MUST show the time, the level and where it came from (a part of the backend, or the interface), as plain
  text a person can read without a tool.
- **FR-014**: Flooding from the interface MUST be limited, and the log MUST say how many messages were dropped.
- **FR-015**: On a computer, the settings MUST offer a button that opens the folder with the log. On a phone, the settings MUST
  offer "Protokoll teilen", which offers the log of the open vault to the system's share sheet as a file and sends nothing
  by itself.
- **FR-016**: When the log cannot be written, the app MUST keep working and the settings MUST say that the log cannot be written.
- **FR-017**: The settings MUST say that the log lies in the vault's folder and goes with the folder when it is copied.

### Key Entities

- **Log level**: "Standard" or "Ausführlich". It has a switch time and, for "Ausführlich", an end time. It is set and kept per
  vault and read when the vault is unlocked.
- **Log**: the plain-text lines of one vault, backend and interface in time order, bounded in size, in the folder of the vault.
- **Log line**: time, level, origin (a part of the backend, or the interface) and a message.

## Success Criteria

### Measurable Outcomes

- **SC-001**: A user finds the setting and raises the level in at most three steps from the settings page, in under 30 seconds,
  and needs no restart.
- **SC-002**: In a release build, a failed import or another failed step in an open vault leaves a line in the log in the vault's
  folder that names the cause, in 100 % of the cases covered by the test files.
- **SC-003**: In a test that opens a vault and imports a file in "Ausführlich", with known secret strings in the vault (a
  password, a note, a field, a key and the passphrase), none of those strings is found in the log; later steps that add lines
  for other areas extend the test with their flows.
- **SC-004**: After 30 minutes of normal use in "Ausführlich", the log still holds the first line of that session.
- **SC-005**: After the end time of "Ausführlich", or at the next start, the level is "Standard" in 100 % of the cases tested, and
  the settings say so.
- **SC-006**: An error of the interface and an error of the backend that follow each other within one second appear in the
  same log in that order.

## Assumptions

- The log is plain text on purpose: the user must be able to read and send it as a file. Because it lies beside an encrypted
  vault, it is the one place where care for secrets is the whole protection (FR-009).
- "Standard" keeps the lines of level information and above, as the development build does today. "Ausführlich" adds the detail
  level. No third level is offered.
- The limits of the size are chosen in the plan; the order of magnitude is some hundred kilobytes for "Standard" and some
  megabytes for "Ausführlich" (today 40 kB, which rotated sync lines away within seconds).
- The choice of the level is one per vault and per device: it is kept as a setting of that vault on this device and does not sync
  to other devices, each device decides for itself. It needs to be known only once the vault has been unlocked.
- Only the main window writes interface messages. The extension frames are third-party content (spec 057) and keep their own
  console.
- The lines that make a given area (import, sync, opening a vault) readable at "Ausführlich" are written with that area, in a
  later step. Until then the switch changes little for those areas, and this spec says so in the settings text.
- Reading the log inside the app is out of scope; the user opens the file or shares it.
- Closing and unlocking the same vault again during one run of the app keeps "Ausführlich" until its end time; only a start of
  the app or the end time ends it.
- Sending the log automatically to anyone is out of scope. The user decides whom to send it to.
- The vault's folder layout of spec 084 stays as it is; the log is one more file in it.
