# Feature Specification: Abacus AI CLI delegate

**Feature Branch**: `096-abacus-delegate`

**Created**: 2026-11-22

**Status**: Draft

**Input**: User description: "Add the Abacus AI CLI (`abacusai`) as a third CLI delegate vendor next to Claude Code and Codex
(spec 007), on the per-vault tool manager from spec 094: installed per vault, run with the per-vault home, never from the host,
never updated silently, only tested versions. Login is only interactive, so how a vault user signs in without leaving
credentials outside the vault, and where the credential lives, is the central open question. Whether the vendor publishes
integrity information, whether the tool has a headless mode usable as a delegate backend, its auto-update switch, its
config, log and cache locations and its platforms are to be researched. Operator rules: no host CLIs, no reliance on
environment variables for isolation, never silent, no backward compatibility, new identifiers use the `haex` prefix. Out of
scope: encryption of vault subfolders, mobile, changes to Claude Code and Codex, the OS sandbox. Only the operator has a real
Abacus login, so every real login or run is an operator task." (operator, 2026-11-22)

## Background

- **Two delegates exist, on a tool manager made for more.** Spec 007 lets a user answer chat requests with a coding-assistant
  subscription they already have (Claude Code, Codex), under the user's approval posture and without host-level state.
  Spec 094 moved their programs and their home folder into the vault's folder and made the vendor list "data plus an
  adapter" (094 FR-015, SC-007), so that a third vendor needs no change to install, update, removal or paths.
- **What the operator wants.** Abacus AI as the third delegate, with the account's login and not an API key: the same
  "bring your existing subscription" path as the other two.
- **What is known about the tool** (measured 2026-11-16 on the `abacusai` 2.10 line, not yet checked against the vendor's
  own documentation): `auth login` works only interactively (a terminal menu whose first entry is e-mail); no token-setup
  command and no device-code flow were found; the tool honours `HOME`; its npm installer downloads
  `https://static.abacus.ai/cli/releases/<version>/abacusai-<platform>.tar.gz`.
- **What is not known** and decides what this spec can promise (listed as research in Open risks): whether the vendor
  publishes checksums or signatures, whether the tool has a non-interactive mode a delegate can drive, whether it can
  pause one tool call at a time for the user's approval, where it keeps its credential, config, history, logs and caches,
  how it renews a login, how to switch off its own update, and which operating systems and architectures it exists for.

## Clarifications

### Session 2026-11-22

- Q: How does the user sign in to Abacus, given that its only login is interactive? → A: The app runs the tool's login in a
  terminal view inside the app (vault home as home, a per-run folder for the login file), takes the login into the vault
  database and removes the file; the user needs no terminal of their own (operator).
- Q: What does the Abacus backend do if the tool cannot pause single tool calls for approval? → A: It is not offered until it
  can; no weaker mode is offered in its place (operator).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Install the Abacus tool into the vault and sign in (Priority: P1)

A user with an Abacus account, on a computer with no Abacus CLI and no Node, opens their vault, picks the Abacus backend,
installs its tool into the vault, signs in, and gets an Abacus-answered reply.

**Why this priority**: Nothing else works without an installed tool and a login the vault can keep. It is the whole path
the operator asked for.

**Independent Test**: Operator task (real account): on a machine whose `PATH` has no `abacusai`, `node` or `npm`, install the
tool from Settings, sign in from the app, send one request, confirm a reply labelled as answered by Abacus.

**Acceptance Scenarios**:

1. **Given** a vault without the Abacus tool, **When** the user selects the Abacus backend, **Then** it is shown as "not
   installed in this vault" with the install action next to the other tools' (094 US1), and no request is sent.
2. **Given** an installed tool and no login, **When** the user chooses to sign in, **Then** they complete the tool's login in a
   terminal view inside the app, without a terminal of their own and without anything on the host, and the backend shows
   as connected.
3. **Given** a connected Abacus backend, **When** the user sends a request, **Then** it runs with the vault's own copy and the
   reply is labelled with the backend that answered it (007 FR-005).
4. **Given** the user cancels the login half way, **When** it ends, **Then** nothing is stored, nothing is left in the vault's
   home, and the backend still shows "not connected".

---

### User Story 2 - The login belongs to the vault, not to the device (Priority: P1)

A user signs in once and expects the vault to carry the login to every device that holds it, without a second login there and
without a login file lying around on any of them.

**Why this priority**: It is spec 007's second promise (FR-002, FR-003) and the point where Abacus differs from the others:
its only login is interactive and its login file is the tool's own.

**Independent Test**: Sign in on device A, open the same vault on device B (after sync), send a request without any login
step; then search the vault's folders and the host for the login's file or token.

**Acceptance Scenarios**:

1. **Given** a vault with an Abacus login made on device A, **When** it is opened on device B, **Then** the user installs the
   tool there (094 FR-018) and needs no further login to get a reply.
2. **Given** a stored login, **When** no request or login is running, **Then** no file with the login's content exists in the
   vault's home or anywhere on the host; the stored form is encrypted (007 FR-001).
3. **Given** the tool renews the login during a run, **When** the run ends, **Then** the renewed login replaces the stored one,
   so the next run, here or on another device, still works without a new login.
4. **Given** the login has expired or was revoked, **When** the user selects the backend, **Then** they are led back into the
   sign-in rather than shown a raw failure (007 FR-014).
5. **Given** a connected backend, **When** the user disconnects it, **Then** the stored login is removed and requests made
   afterwards are refused with the reason "not connected"; a response already running is not disturbed (007 FR-013).

---

### User Story 3 - Abacus acts only within the user's approval posture (Priority: P1)

A user lets Abacus work on a request and expects the same control over its actions as with the other delegates: sensitive
actions wait for them or are blocked, depending on their posture.

**Why this priority**: Without it the Abacus delegate would be a way around the approval rules of spec 003 and 007.

**Independent Test**: With the posture that blocks sensitive actions, ask Abacus to do one; confirm it does not happen and the
conversation shows why. With the posture that asks, confirm the action waits until the user decides.

**Acceptance Scenarios**:

1. **Given** the posture blocks sensitive actions, **When** Abacus attempts one, **Then** it is not performed (007 FR-007).
2. **Given** the posture asks per action, **When** Abacus attempts one, **Then** that action pauses until the user decides
   (007 FR-006).
3. **Given** the Abacus tool cannot be held to the posture (it cannot pause one tool call at a time), **When** the user looks
   for the Abacus backend, **Then** it is not offered, and Settings says why (FR-010).
4. **Given** a request that needs context, **When** it runs, **Then** the context is passed with the request; Abacus does not
   read the host's project files or its own host configuration (007 FR-009, FR-010).

---

### User Story 4 - Leave no trace on the host (Priority: P2)

A user on a borrowed computer installs, signs in to and uses Abacus, then removes the tool, and expects the host to look as
before, as with the other two tools.

**Why this priority**: It is the reason for spec 094. A third tool that writes to the real home folder, a temp folder or a
device-wide log breaks that promise, and what Abacus writes is not yet known.

**Independent Test**: File listings of the host's home, temp and config folders before and after install, sign-in, request,
stop, update and removal; a text search of every device-wide file HaexVault writes for the vault's name, the tool's name and
the request's text.

**Acceptance Scenarios**:

1. **Given** the full series above, **When** it ends, **Then** no new or changed file lies outside the vault's folder, apart
   from residuals that were measured, listed in this spec's Assumptions and accepted (094 FR-012, FR-021).
2. **Given** a host `abacusai` earlier on `PATH` and a configured host home, **When** a request runs, **Then** the vault's copy
   runs and the host's is never started (094 FR-003).
3. **Given** a new Abacus version, **When** it is considered for the tested-version list, **Then** it enters the list only
   after the same measurement showed no residue beyond those listed (094 FR-021).

---

### User Story 5 - Update, remove and repair like the other tools (Priority: P2)

A user sees Abacus next to Claude Code and Codex in Settings and uses the same actions: see the version, update when they
decide, go back, remove, reinstall.

**Why this priority**: It proves the tool manager was made for more than two vendors, and the user should not have to learn a
fourth way to manage a tool.

**Independent Test**: Run the update, go-back, removal and "reinstall" scenarios of spec 094 against Abacus with a test double
of its download.

**Acceptance Scenarios**:

1. **Given** an installed Abacus tool, **When** the user opens its settings, **Then** version, isolation level, update, go-back
   and removal behave as in spec 094 (US3, US4).
2. **Given** the user removes the Abacus tool, **When** it is confirmed, **Then** its files and its stored login are gone
   together and the user installs and signs in again to come back (094 US4).
3. **Given** a copied vault, a damaged installation or a download that does not match its integrity information, **When** the
   user selects the backend, **Then** the states and messages of spec 094 US5 and FR-006 apply unchanged.

---

### Edge Cases

- The vendor changes the login menu or the login file's form in a new version: the tested-version list is what protects the
  user; a version outside it is shown as untested (094 FR-009), and a login that cannot be completed ends with a specific
  reason, not a hang.
- The login needs a browser or a second device (for example a link opened elsewhere): the app tells the user what to do, and
  nothing is opened on the host that leaves state there beyond the vault's folder (the host browser's own state is the
  host's and is not touched).
- HaexVault is killed during a login or a run: the next start finds no login file in the vault's home; a per-run folder that
  held one is cleared (094 FR-007, FR-011), and the stored login is either the old or the new one, never a half-written one.
- The tool renews the login in a run that is then stopped or crashes: the renewed login is still kept if it was written
  completely, otherwise the stored one stays; the user is never left with neither.
- Two devices run the same vault's Abacus login at once and both renew it: the later complete write wins and the other device
  is led back into the sign-in if its copy was invalidated; no silent retry loops.
- The account has no CLI access or its subscription lapsed: a specific explanation, never a silent switch to another backend
  (007 FR-008).
- The vendor publishes the program for fewer platforms than the other two tools: on an unsupported device the backend says
  "not available for this device" with the reason and offers nothing to install; mobile stays unavailable (094 FR-016).
- The vendor publishes no integrity information for a version: the install for it does not proceed (094 FR-006) until the
  operator has decided what replaces it.
- The user is offline: install and login say so with a specific reason; a request fails with "network", not "not connected".

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The Abacus AI CLI MUST be a vendor of the tool manager of spec 094, added as a vendor definition plus an adapter,
  with no change to the install, update, removal or path logic (094 FR-015, SC-007). The tool lives in
  `<vault folder>/cli-tools/abacusai/<version>/`; the vault's home is `<vault folder>/home/`; both are resolved only by the
  vault path function (094 FR-001, FR-002).
- **FR-002**: The user MUST be able to install, update, see the version of, go back and remove the Abacus tool for the current
  vault from the UI, with the behaviour of spec 094 (FR-004, FR-007 to FR-010, FR-013, FR-014, FR-022); installing or updating
  MUST NOT happen without the user's action, and the tool's own update MUST be switched off.
- **FR-003**: Each tested Abacus version in the list MUST carry integrity information for every platform it is offered for, and
  HaexVault MUST refuse a download that does not match it, before anything of it is run (094 FR-006). Where the vendor
  publishes a checksum or signature, the entry MUST have been taken from or checked against it; where it publishes none, that
  MUST be recorded as an open risk and the install for that version MUST NOT proceed until the operator has decided.
- **FR-004**: HaexVault MUST offer the Abacus tool only on operating systems and architectures the vendor publishes it for; on
  others the backend MUST say "not available for this device" with the reason. On Android and iOS the backend stays
  unavailable (094 FR-016).
- **FR-005**: The user MUST be able to sign in to Abacus from the app, for the current vault, without opening a terminal of
  their own and without any tool or login state on the host (007 FR-001, FR-003). The app MUST run the tool's interactive
  login itself, in a terminal view inside the app, with the vault's home as its home and a per-run folder for the login
  file; when the login ends, the app takes the login from that folder into the vault database (FR-006) and removes the
  folder. What the user types and what the tool prints in that view MUST NOT be logged or kept after the login ends.
- **FR-006**: The resulting Abacus login MUST be stored encrypted in the vault's database as a delegate credential, portable
  to any device that holds the vault, usable there with no further login step (007 FR-001, FR-002). Where the tool needs its
  own login file to run, the file MUST exist only in a per-run folder inside the vault's home, created from the stored login
  when a run or version query starts, and removed when it ends; a folder that survived a crash MUST be cleared at the next
  start. No file with the login's content may remain anywhere else, on the host or in the vault's folder.
- **FR-007**: If the tool renews the login during a run, HaexVault MUST read the renewed login from the per-run folder at the
  end of the run and replace the stored one with it, completely or not at all; a failure to do so MUST be shown to the user,
  not swallowed.
- **FR-008**: The user MUST be able to disconnect the Abacus login at any time, which removes the stored login and does not
  disturb a response already running; an expired or revoked login MUST lead the user back to the sign-in with a specific
  explanation (007 FR-013, FR-014).
- **FR-009**: The Abacus backend MUST be selectable per request like the other delegates, MUST label the response with the
  backend that answered it, and MUST show a record of the tool use it performed (007 FR-004, FR-005).
- **FR-010**: Abacus tool use MUST follow the user's approval posture as spec 007 requires of the other delegates: blocked
  actions are not performed, and where the posture asks, each action pauses until the user decides (007 FR-006, FR-007). If
  the research shows the Abacus tool cannot be held to this, the Abacus backend MUST NOT be offered at all until it can; no
  weaker mode (no approvals, once per request, answers without tools) is offered in its place, and Settings states the
  reason.
- **FR-011**: An Abacus request MUST NOT be influenced by host-level configuration, instructions or settings of the tool, and
  MUST NOT depend on the tool finding context on the host; context is passed with the request (007 FR-009, FR-010).
- **FR-012**: The user MUST be able to stop an Abacus-answered response; stopping MUST end the tool's process and MUST NOT resume
  or retry (007 FR-011, FR-012).
- **FR-013**: Every Abacus process (a request, a login, a version query) MUST run through the containment interface of spec
  094 (FR-011, FR-020), with the isolation level shown in Settings (094 FR-021), and MUST NOT create or change any file
  outside the vault's folder (094 FR-012, FR-019). A version MUST enter the tested-version list only after the measurement of
  094 FR-021 was made for Abacus on each platform it is offered for; residue found MUST be listed under Assumptions and
  decided by the operator, not accepted by default.
- **FR-014**: When the Abacus backend is unavailable (not installed, not connected, login expired, account without CLI access,
  not available for this device), the user MUST get a specific, actionable explanation and MUST NOT be switched to another
  backend (007 FR-008).
- **FR-015**: Everything that needs a real Abacus account (the first login, a real request, the measurement series on a real
  login) MUST be listed as operator tasks in the quickstart with the steps; no automated test uses real credentials, and each
  step an automated test needs is covered by a test double of the Abacus tool and its download.

### Key Entities

- **Abacus tool**: The `abacusai` program, one version for one platform in one vault's `cli-tools/` area; a CLI tool and a
  tool installation as in spec 094.
- **Abacus vendor definition**: The data for the Abacus tool in the tool manager: where each tested version comes from, its
  platforms, its integrity information, and the home- and config-variables it needs.
- **Abacus login**: The delegate credential (spec 007) obtained through the tool's interactive sign-in; stored encrypted in the
  vault database; materialised as the tool's own login file only inside a per-run folder.
- **Sign-in session**: One run of the login flow for one vault, from the user's start to a stored login or a cancel.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: On a machine with no `abacusai`, `node` or `npm`, the operator goes from an opened vault to an Abacus-answered
  reply using only the app; the install takes under 5 minutes on a 50 Mbit/s connection, not counting the time the user
  needs to complete the login.
- **SC-002**: A vault with an Abacus login made on one device gives an Abacus-answered reply on a second device after install
  there, with no login step, in 100% of a series of 10 alternating runs across the two devices (renewals included).
- **SC-003**: After install, sign-in, request, stop, update, disconnect and removal, a file listing of the host's home, temp and
  config folders shows no new or changed file outside the vault's folder, apart from listed residuals.
- **SC-004**: When no run and no login is active, a search of the vault's folder and of the host for the login's token or file
  finds nothing, in 100% of states including after killing HaexVault during a login and during a run.
- **SC-005**: A download whose content was changed is not run in 100% of test cases, and the user sees the integrity reason.
- **SC-006**: Under the posture that blocks sensitive actions, 100% of sensitive actions Abacus attempts are blocked, or the
  backend is not offered with weaker control (FR-010).
- **SC-007**: Adding Abacus changes only vendor data, one adapter and the user-facing texts; a review of the change finds no
  edit in the install, update, removal or path logic of the tool manager.
- **SC-008**: After the same series as SC-003, a text search of every device-wide file HaexVault writes (its log included, in a
  build with logging on) for the vault's name, the tool's name and the request's text finds nothing.

## Assumptions

- Builds on spec 007 (delegate adapters, approval bridge, isolation, credential in the vault) and spec 094 (tool manager,
  vault home, containment, tested-version list, integrity check). Their requirements apply unchanged; this spec only names how
  they apply to Abacus and what is new (the sign-in, the login's write-back).
- HaexVault has no users yet: no migration, no legacy path, no binary-path override.
- The vault's subfolders are not encrypted yet (out of scope); the login is protected as spec 007 requires by living only in
  the encrypted vault database outside runs.
- The OS sandbox is a later spec; the base containment layer reports `best effort`.
- Whether the vendor allows driving its CLI as part of a third-party product is checked in the research phase against its
  own terms, from the primary source; it is not assumed either way (the decision for Claude Code, accepted at product level in
  spec 007, does not carry over).
- **Listed residuals**: none known yet. The first measurement series on the real tool fills this list; each entry needs the
  operator's agreement.
- New identifiers (environment variables of HaexVault's own, schemas, event names) use the `haex` prefix; the tool's own
  variables keep their names.

## Open risks

To be resolved in the research phase from primary sources (the vendor's documentation and release files) and by the
operator's measurements; each result goes to `research.md`, and a result that contradicts a requirement above is reported to
the operator, not worked around:

- **Integrity information.** Does the vendor publish a checksum or signature per release and platform? If not, FR-003's
  consequence applies.
- **Sign-in.** What the interactive login stores and where; whether any non-interactive way to obtain the same login exists;
  whether the login has an expiry, a renewal and a refresh token that the tool rotates (FR-007).
- **Headless use and approvals.** Whether the tool has a non-interactive mode a delegate can drive, what its output looks like,
  and whether one tool call at a time can be held for approval (FR-010).
- **Where the tool writes.** Config, history, logs, caches, sockets and temp files, with and without `HOME` moved; which of
  them ignore `HOME` (as Claude Code's inbox socket does).
- **Its own update.** How to switch it off without reliance on a host setting.
- **Platforms.** Which operating systems and architectures the release files exist for, and the size of each.
- **Terms.** Whether the vendor's terms allow this use.
- **Operator tasks.** Everything needing the real account (FR-015): first login on a scratch home, a real request, the
  measurement series, a second device for SC-002.
