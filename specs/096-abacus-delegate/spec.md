# Feature Specification: Abacus AI CLI delegate

**Feature Branch**: `096-abacus-delegate`

**Created**: 2026-11-22

**Status**: Draft

**Input**: User description: "Add the Abacus AI CLI (`abacusai`) as a third CLI delegate vendor next to Claude Code and Codex
(spec 007), on the per-vault tool manager from spec 094: installed per vault, run with the per-vault home, never from the host,
never updated silently, only tested versions. The tool's own login is interactive, so how a vault user connects without
leaving credentials outside the vault, and where the credential lives, is the central open question. Whether the vendor publishes
integrity information, whether the tool has a headless mode usable as a delegate backend, its auto-update switch, its
config, log and cache locations and its platforms are to be researched. Operator rules: no host CLIs, no reliance on
environment variables for isolation, never silent, no backward compatibility, new identifiers use the `haex` prefix. Out of
scope: encryption of vault subfolders, mobile, changes to Claude Code and Codex, the OS sandbox. Only the operator has a real
Abacus account, so everything that needs one is an operator task." (operator, 2026-11-22)

## Background

- **Two delegates exist, on a tool manager made for more.** Spec 007 lets a user answer chat requests with a coding-assistant
  subscription they already have (Claude Code, Codex), under the user's approval posture and without host-level state.
  Spec 094 moved their programs and their home folder into the vault's folder and made the vendor list "data plus an
  adapter" (094 FR-015, SC-007), so that a third vendor needs no change to install, update, removal or paths.
- **What the operator wants.** Abacus AI as the third delegate, the same "bring your existing subscription" path as the
  other two, connected from the app's own UI: no command line for the user, no variable the user has to set, nothing
  depending on the host.
- **What is known about the tool** (research 2026-11-22 on `abacusai` 2.10.23, from the vendor's documentation, its release
  files and runs in a throwaway home; details in `research.md`): the release archives carry a Sigstore signature made by
  the vendor's release workflow; the tool has a print mode and a JSON-lines mode in which it asks its host to answer
  each tool-call permission; its interactive `auth login` needs a real terminal and stores the login in an encrypted file
  whose key comes from the device's name and user, so the file cannot move between devices; without a stored login the
  tool accepts a credential from its environment (an API key, or a device code that the vendor's desktop app sets).
- **What is still not known** (research that needs a real Abacus account, listed in Open risks): whether an API key
  draws on the subscription's credits, what an authenticated run writes beyond the unauthenticated runs measured so far,
  and the exact messages of an approval round trip.

## Clarifications

### Session 2026-11-22

- Q: How does the user sign in to Abacus, given that its own login needs a terminal? → A: From the app's own UI, as with
  Claude Code and Codex; no terminal view, no command line, no variable the user sets, nothing that depends on the host
  (operator, 2026-11-22, replacing the first answer of the same day, a terminal view in the app).
- Q: Where does the credential live, and is it portable? → A: In the password manager, as an entry of its own, and is used
  from there; the entry travels with the vault to every device. This holds for all delegate providers, present and future
  (operator, 2026-11-22). This spec does it for Abacus; moving Claude Code and Codex is a change of its own (Assumptions).
- Q: What does the Abacus backend do if the tool cannot pause single tool calls for approval? → A: It is not offered until it
  can; no weaker mode is offered in its place (operator).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Install the Abacus tool into the vault and connect (Priority: P1)

A user with an Abacus account, on a computer with no Abacus CLI and no Node, opens their vault, picks the Abacus backend,
installs its tool into the vault, connects their account from the app's UI, and gets an Abacus-answered reply.

**Why this priority**: Nothing else works without an installed tool and a credential the vault can keep. It is the whole
path the operator asked for.

**Independent Test**: Operator task (real account): on a machine whose `PATH` has no `abacusai`, `node` or `npm`, install the
tool from Settings, connect from the app, send one request, confirm a reply labelled as answered by Abacus.

**Acceptance Scenarios**:

1. **Given** a vault without the Abacus tool, **When** the user selects the Abacus backend, **Then** it is shown as "not
   installed in this vault" with the install action next to the other tools' (094 US1), and no request is sent.
2. **Given** an installed tool and no credential, **When** the user chooses to connect, **Then** they complete it in the
   app's UI, without a terminal and without setting any variable, and the backend shows as connected.
3. **Given** a connected Abacus backend, **When** the user sends a request, **Then** it runs with the vault's own copy and the
   reply is labelled with the backend that answered it (007 FR-005).
4. **Given** the user cancels connecting half way, **When** it ends, **Then** nothing is stored, nothing is left in the
   vault's home, and the backend still shows "not connected".

---

### User Story 2 - The credential belongs to the vault, not to the device (Priority: P1)

A user connects once and expects the vault to carry the credential to every device that holds it, without connecting again
there and without a credential file lying around on any of them.

**Why this priority**: It is spec 007's second promise (FR-002, FR-003) and the point where Abacus differs from the others:
the tool's own login is stored in a file that only the device that wrote it can read.

**Independent Test**: Connect on device A, open the same vault on device B (after sync), send a request without any
connect step; then search the vault's folders and the host for the credential.

**Acceptance Scenarios**:

1. **Given** a vault with an Abacus credential saved on device A, **When** it is opened on device B, **Then** the user
   installs the tool there (094 FR-018) and needs nothing else to get a reply.
2. **Given** a saved credential, **When** no request is running, **Then** it exists only as the password manager's entry
   (encrypted at rest with the vault, ADR 0007); no file with its content exists in the vault's home or anywhere on the host.
3. **Given** the credential has expired or was revoked, **When** the user selects the backend, **Then** they are led back
   to connecting rather than shown a raw failure (007 FR-014).
4. **Given** a connected backend, **When** the user disconnects it, **Then** the entry is removed and requests made
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

A user on a borrowed computer installs, connects and uses Abacus, then removes the tool, and expects the host to look as
before, as with the other two tools.

**Why this priority**: It is the reason for spec 094. A third tool that writes to the real home folder, a temp folder or a
device-wide log breaks that promise, and what Abacus writes is not yet known.

**Independent Test**: File listings of the host's home, temp and config folders before and after install, connect, request,
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
2. **Given** the user removes the Abacus tool, **When** it is confirmed, **Then** its files and its credential entry are
   gone together and the user installs and connects again to come back (094 US4).
3. **Given** a copied vault, a damaged installation or a download that does not match its integrity information, **When** the
   user selects the backend, **Then** the states and messages of spec 094 US5 and FR-006 apply unchanged.

---

### Edge Cases

- The vendor changes how the tool accepts a credential or names its messages in a new version: the tested-version list is
  what protects the user; a version outside it is shown as untested (094 FR-009), and a request that fails for it ends with
  a specific reason, not a hang.
- Connecting needs something outside the app (for example a key the user creates on the vendor's website): the app tells
  the user what to do and where, and nothing is opened on the host that leaves state there beyond the vault's folder (the
  host browser's own state is the host's and is not touched).
- HaexVault is killed during a run: the next start finds no credential file anywhere, because none was ever written; the
  per-run folder is cleared (094 FR-007, FR-011).
- The same vault's credential is used on two devices at once: both runs work; nothing is written back by the tool.
- The account has no CLI access or its subscription lapsed: a specific explanation, never a silent switch to another backend
  (007 FR-008).
- The vendor publishes the program for fewer platforms than the other two tools: on an unsupported device the backend says
  "not available for this device" with the reason and offers nothing to install; mobile stays unavailable (094 FR-016).
- The vendor publishes no integrity information for a version: the install for it does not proceed (094 FR-006) until the
  operator has decided what replaces it.
- The user is offline: install and connect say so with a specific reason; a request fails with "network", not "not connected".

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The Abacus AI CLI MUST be a vendor of the tool manager of spec 094, added as a vendor definition plus an adapter,
  with no change to the manager's update, removal or path logic and, for install, only the two additions the vendor's release
  format needs: an x64 variant for CPUs without AVX2, and zip archives for Windows (094 FR-015; research §8). The tool lives in
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
- **FR-005**: The user MUST be able to connect their Abacus account from the app's own UI, for the current vault, as with
  Claude Code and Codex: no terminal, no command line, no variable the user sets, and no tool or login state on the host
  (007 FR-001, FR-003). The tool's own interactive login (which needs a real terminal and writes a device-bound file) MUST
  NOT be part of connecting. What the user enters MUST NOT be logged.
- **FR-006**: The credential MUST be stored in the password manager as an entry of its own, reached only through the
  password manager's service by a HaexVault function with a fixed grant (ADR 0007), and used from there. It is portable to
  every device that holds the vault, usable there with no further connect step (007 FR-002). HaexVault hands it to the
  tool's process for that run only; it MUST NOT be written to any file, in the vault's home or on the host, and no file of
  the tool that holds a login (such as its keystore) may remain after a run.
- **FR-007**: A credential the tool does not accept (expired, revoked, wrong) MUST end the request with a specific reason
  that leads the user back to connecting; HaexVault MUST NOT retry with another credential or another backend.
- **FR-008**: The user MUST be able to disconnect Abacus at any time, which removes the entry and does not disturb a
  response already running; requests made afterwards are refused with "not connected" (007 FR-013, FR-014).
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
- **FR-013**: Every Abacus process (a request or a version query) MUST run through the containment interface of spec
  094 (FR-011, FR-020), with the isolation level shown in Settings (094 FR-021), and MUST NOT create or change any file
  outside the vault's folder (094 FR-012, FR-019). A version MUST enter the tested-version list only after the measurement of
  094 FR-021 was made for Abacus on each platform it is offered for; residue found MUST be listed under Assumptions and
  decided by the operator, not accepted by default.
- **FR-014**: When the Abacus backend is unavailable (not installed, not connected, credential refused, account without CLI access,
  not available for this device), the user MUST get a specific, actionable explanation and MUST NOT be switched to another
  backend (007 FR-008).
- **FR-015**: Everything that needs a real Abacus account (connecting, a real request, the measurement series on a real
  credential) MUST be listed as operator tasks in the quickstart with the steps; no automated test uses real credentials, and each
  step an automated test needs is covered by a test double of the Abacus tool and its download.

### Key Entities

- **Abacus tool**: The `abacusai` program, one version for one platform in one vault's `cli-tools/` area; a CLI tool and a
  tool installation as in spec 094.
- **Abacus vendor definition**: The data for the Abacus tool in the tool manager: where each tested version comes from, its
  platforms, its integrity information, and the home- and config-variables it needs.
- **Abacus credential**: The delegate credential (spec 007) in the form the tool accepts without its interactive login; one
  entry of the password manager per vault; never a file.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: On a machine with no `abacusai`, `node` or `npm`, the operator goes from an opened vault to an Abacus-answered
  reply using only the app; the install takes under 5 minutes on a 50 Mbit/s connection, not counting the time the user
  needs to connect the account.
- **SC-002**: A vault with an Abacus credential saved on one device gives an Abacus-answered reply on a second device after
  install there, with no connect step, in 100% of a series of 10 alternating runs across the two devices.
- **SC-003**: After install, connect, request, stop, update, disconnect and removal, a file listing of the host's home, temp and
  config folders shows no new or changed file outside the vault's folder, apart from listed residuals.
- **SC-004**: When no run is active, a search of the vault's folder and of the host for the credential's text and for a
  keystore file of the tool finds nothing outside the password manager's entry, in 100% of states including after killing
  HaexVault during a run.
- **SC-005**: A download whose content was changed is not run in 100% of test cases, and the user sees the integrity reason.
- **SC-006**: Under the posture that blocks sensitive actions, 100% of sensitive actions Abacus attempts are blocked, or the
  backend is not offered with weaker control (FR-010).
- **SC-007**: Adding Abacus changes vendor data, one adapter, the places that name the delegate vendors, and the user-facing
  texts; a review of the change finds no edit in the update, removal or path logic of the tool manager, and its edits to
  install are limited to the platform variant and the zip archive of FR-001.
- **SC-008**: After the same series as SC-003, a text search of every device-wide file HaexVault writes (its log included, in a
  build with logging on) for the vault's name, the tool's name and the request's text finds nothing.

## Assumptions

- Builds on spec 007 (delegate adapters, approval bridge, isolation, credential in the vault) and spec 094 (tool manager,
  vault home, containment, tested-version list, integrity check). Their requirements apply unchanged; this spec only names how
  they apply to Abacus and what is new (the credential's place, FR-005 to FR-008).
- HaexVault has no users yet: no migration, no legacy path, no binary-path override.
- The credential's place is the password manager (operator, 2026-11-22). Claude Code and Codex keep the credential in their
  provider row as spec 007 built it; moving them to the same place is a change of its own, outside this spec ("changes to
  Claude Code and Codex" are out of scope), to be agreed with the operator. Until then HaexVault holds two places.
- The password manager keeps secrets as plain values in the vault database, protected by the vault's encryption at rest and
  by grants (ADR 0007); spec 007 FR-001's "not unencrypted" is read as that.
- The vault's subfolders are not encrypted yet (out of scope).
- The OS sandbox is a later spec; the base containment layer reports `best effort`.
- Whether the vendor's terms allow driving its CLI from a third-party product: the customer terms
  (`abacus.ai/terms`, read 2026-11-22) neither allow nor forbid automated access or wrappers of the CLI; they do not mention
  the CLI. It is recorded as an open question for the operator, not assumed either way (the decision for Claude Code,
  accepted at product level in spec 007, does not carry over).
- **Listed residuals** (measured 2026-11-22, unauthenticated runs only): the tool extracts a native library to a file in
  the system temp folder named by a content hash (`.bun-<uid>-<hash>.node`), which carries no vault name and no content, and
  stays after the run, and it writes its config store under the user's XDG config folder; both follow `TMPDIR` and
  `XDG_CONFIG_HOME`, which the base layer sets for the process, so they land in the vault's home. An authenticated run
  may add entries; each needs the operator's agreement.
- New identifiers (environment variables of HaexVault's own, schemas, event names) use the `haex` prefix; the tool's own
  variables keep their names.

## Open risks

Resolved by the research phase on 2026-11-22 (details in `research.md`): the vendor publishes a Sigstore signature in
every archive, so FR-003 is satisfiable; the tool has a JSON-lines mode with a host-answered `can_use_tool` permission
request, so FR-010 looks satisfiable (the message round trip still needs an authenticated run); its own update can be
switched off (`ABACUS_AUTO_UPDATE=off`); release files exist for Linux, macOS and Windows on x64 and arm64. Still open:

- **Which credential the tool accepts and what it costs.** An API key (`ABACUS_API_KEY`) and a device code
  (`CODELLM_DEVICE_CODE`, set by the vendor's desktop app) sign the tool in without its own login. The key is documented for
  scripts; the device code comes from vendor endpoints that are not documented. Whether a key draws on the subscription's
  credits, and whether a device code can be obtained without the tool's terminal, needs the operator's account (FR-015).
- **Authenticated traces.** What a run with a credential writes beyond the unauthenticated runs measured so far (a keystore
  file, sessions, caches), with `HOME`, `TMPDIR` and `XDG_CONFIG_HOME` moved (FR-006, FR-013).
- **The approval round trip.** The messages of a real `can_use_tool` exchange and what the tool does with a refusal
  (FR-010).
- **Terms.** The customer terms are silent on automated use of the CLI (Assumptions); the operator decides whether that
  is enough.
- **Operator tasks.** Everything needing the real account (FR-015): connecting, a real request, an approval round trip,
  the measurement series, a second device for SC-002.
