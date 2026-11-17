# Feature Specification: CLI tools per vault

**Feature Branch**: `094-vault-cli-tools`

**Created**: 2026-11-16

**Status**: Draft

**Input**: User description: "Each vault gets its own installation of the CLI tools that the delegate backends run
(Claude Code `claude`, Codex `codex`), inside the vault folder, and its own HOME inside the vault folder. The user installs
a tool once before first use and updates it later, both from the UI. holzi must not depend on tools of the host and must
leave as few traces as possible when used on a foreign computer." (operator, 2026-11-16, while scoping the Abacus AI CLI
as a third delegate, which becomes spec 096 on top of this one.)

## Background

- **Today the delegates run the host's tools.** Spec 007 spawns `claude` and `codex` from the host `PATH`. holzi ships
  neither. The provider row's `base_url` holds the binary name or a path. A missing binary ends in "not installed or not on
  PATH" with an `npm install -g` hint. holzi never checks a version, and updating is the host owner's business.
- **Today a delegate run still touches the host.** The per-run config folder and working folder come from the operating
  system's temp folder, and `HOME` is not set, so everything a tool writes outside its config-folder variable (caches, logs,
  history, a second dotfile) lands in the real home folder of whoever is logged in.
- **Today records about vault activity can land in device-wide files.** holzi's file log (switched on in debug builds only)
  writes to `<AppLocalData>/logs/`, one file for the whole device, not into a vault's folder. Which lines about a vault, a tool
  or a delegate run reach it has not been checked.
- **What the operator wants.** On a computer that is not theirs, holzi must work without anything installed there and must
  leave nothing behind outside the vault's own folder. The rule behind it, stated by the operator on 2026-11-16:
  everything that concerns a vault lands in that vault's folder. The tools live in the vault folder (spec 084) next to everything else
  of the vault, which also lets them be covered by the encryption of vault subfolders later.

## Clarifications

### Session 2026-11-16

- Q: Where does a delegate process's home lie? → A: In `<vault folder>/home/`, a subfolder of its own, not the vault
  folder's root: the root keeps holding only SQLite and bookkeeping files (spec 084 FR-002) and the home can be encrypted
  later with the other subfolders (operator).
- Q: Which version does "update" install? → A: Only versions on holzi's tested-version list, always the newest
  one; the user cannot pick another version in the UI, and "go back" (FR-022) covers a new version that does not work
  (operator, clarified 2026-11-16; the backend command still accepts a listed version, used by tests and the quickstart).
- Q: Must isolation rest on environment variables? → A: No. A child process's environment is private to that child, but
  a tool may ignore it (measured: Claude Code's inbox socket ignores `HOME` and `TMPDIR`; Windows tools use account folders,
  not `HOME`), and a host may rely on variables its tools need (proxy, certificates). Isolation therefore has two layers:
  a portable base that changes as little as possible and passes through what a tool needs from the host, and an OS
  sandbox that enforces the boundary where the platform has one (operator, 2026-11-16).
- Q: Is the OS sandbox part of this spec? → A: No. This spec delivers the base layer behind a containment interface
  and shows the isolation level honestly ("best effort" until a sandbox exists); the sandbox for Linux and macOS is a
  spec of its own, Windows stays best effort (operator).
- Q: Run the tools in a container (Docker)? → A: No. It needs a program on the host that a foreign computer rarely has,
  leaves images and containers outside the vault folder, and does not fit the helper program and the approval socket
  (operator, after the trade-offs were laid out).
- Q: How many versions of a tool does a vault keep? → A: One in use, always the newest. The previous one is deleted only
  once the new one has proven itself, so there is a fallback until then, as far as that is feasible (operator,
  2026-11-16). "Proven" is decided in FR-022.
- Q: What does the model picker do with a delegate whose tool is not installed in the vault? → A: The entry cannot be
  chosen as a model and takes the user to that tool's install action in Settings (operator).
- Q: How is a damaged installation detected? → A: By the program's file size, recorded at install and compared at each
  start; no hash is read at start (operator).
- Q: Is the measurement of device-wide files part of this spec? → A: Yes, as a task: a series is run and every
  device-wide file holzi writes is searched; hits outside the files this feature touches become follow-ups (operator).
- Q: Does an install ask for confirmation before it downloads? → A: No. The install starts at once; the download size and
  the space it takes in the vault are shown as text next to the action, and it can be cancelled at any time (operator).
- Q: What happens to a delegate request that is running when the vault is closed? → A: It is stopped and ends as
  cancelled; it is never confirmed (FR-022) and its run folder is removed. Nothing runs from the vault folder after the
  vault is closed (operator).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Install a tool into the vault and use it (Priority: P1)

A user on a computer with no Claude Code, no Codex and no Node opens their vault, picks a delegate backend and is offered
to install its tool into this vault. After the install they connect their subscription and get a delegate-answered reply.

**Why this priority**: Without an installed tool no delegate works at all once the host `PATH` is no longer used. Every
other story builds on it.

**Independent Test**: On a machine whose `PATH` has no `claude`, `codex`, `node` or `npm`, install the Codex tool from the
UI, connect, send a request, and confirm a delegate-answered reply.

**Acceptance Scenarios**:

1. **Given** a vault with no tool installed, **When** the user selects a delegate backend, **Then** the backend is shown as
   "not installed in this vault" with an install action (in the model picker the entry cannot be chosen as a model and
   takes the user to that action in Settings), and no request is sent.
2. **Given** a tool that is not installed, **When** the user sees its install action, **Then** the download size and the
   space the tool takes in the vault are shown next to it. **Given** the user starts an install, **When** it runs, **Then**
   they see progress, can cancel it, and when it succeeds the installed version is shown.
3. **Given** a tool is installed in this vault, **When** the user connects and sends a request, **Then** the request runs
   with the vault's own copy, never a copy found elsewhere.
4. **Given** the same user has two vaults, **When** they install the tool in one, **Then** the other vault still shows it as
   not installed.

---

### User Story 2 - The host's own tools and folders play no part (Priority: P1)

A user on a borrowed computer, which has its own `claude` of another version and a configured `~/.claude`, works with a
delegate and afterwards expects the host to look as before.

**Why this priority**: This is the reason for the feature. A tool that is still taken from the host, or a file left in the
real home folder, breaks the promise.

**Independent Test**: List the files of the host's home folder and temp folder, install a tool, connect, run a request, stop
a request, update, remove; list again. Repeat with a different `claude` first on `PATH`.

**Acceptance Scenarios**:

1. **Given** a host `claude` or `codex` earlier on `PATH` than anything else, **When** a delegate request runs, **Then** the
   vault's copy runs and the host copy is not started.
2. **Given** a delegate request, a connect flow, an install, an update or a removal, **When** it ends, **Then** no new or
   changed file lies outside the vault's folder.
3. **Given** a vault without an installed tool, **When** a host copy exists, **Then** the backend still shows "not
   installed in this vault"; the host copy is never used as a fallback.
4. **Given** a tool that writes outside its own config variable, **When** it runs, **Then** those writes land in the vault's
   own home folder.

---

### User Story 3 - Update on request, never silently (Priority: P2)

A user sees which version of each tool their vault has, is told when a newer one exists, and updates when they decide to.

**Why this priority**: A tool that never updates ages and eventually stops working against its service; but the user, not
the app, decides when it changes.

**Independent Test**: With an older version installed, trigger the update, confirm the new version is shown and runs, and
confirm nothing changed without the user's action.

**Acceptance Scenarios**:

1. **Given** an installed tool, **When** the user opens the tool's settings, **Then** the installed version is shown.
2. **Given** a newer version is available, **When** the user opens the settings, **Then** an update action is offered; no
   update happens without that action.
3. **Given** a delegate request is running, **When** the user updates, **Then** the running request is not disturbed and the
   new version applies to requests started afterwards.
4. **Given** a failed or interrupted update, **When** it ends, **Then** the previous version still works unchanged.
5. **Given** a tool that has its own self-update, **When** it runs, **Then** it does not update itself.
6. **Given** a new version that fails its start check during an update, **When** the update ends, **Then** the new
   version is discarded, the previous version is untouched and still in use, and the user is told why.
7. **Given** an update was installed and no run with it has ended normally yet, **When** the user opens the tool's
   settings, **Then** the previous version is still on disk and an action to go back to it is offered.
8. **Given** the same state, **When** the first delegate request or connect flow with the new version ends normally,
   **Then** the previous version is deleted and the go-back action disappears.
9. **Given** the same state, **When** the user chooses to go back, **Then** the new version is deleted and the previous
   one is in use again.

---

### User Story 4 - Remove a tool, remove a vault (Priority: P2)

A user removes a tool they no longer use, or removes a whole vault, and expects nothing of it to remain on the device.

**Why this priority**: Disk cost is real (measured 2026-11-16: the Claude Code program is 294 MB, the Codex package
about 500 MB unpacked, per version, times vaults) and leftovers are traces.

**Independent Test**: Install two tools in a vault, remove one, then remove the vault; list the vault's folder and the
device's folders.

**Acceptance Scenarios**:

1. **Given** an installed tool, **When** the user removes it, **Then** its files are gone, and its connection to the
   subscription is unaffected.
2. **Given** a vault with tools and a home folder, **When** the vault's files are removed, **Then** the tools and the home
   folder go with the vault's folder, and nothing of them is left.

---

### User Story 5 - A copied vault says what is missing (Priority: P3)

A user copies a vault to another computer or another operating system, or a download was cut off, and expects a clear state
instead of a crash.

**Why this priority**: It is a state the user can reach through ordinary use, but it can be told apart and fixed by a
reinstall.

**Independent Test**: Copy a vault folder with an installed tool onto a machine with another operating system or CPU
architecture, and break an installed tool's file; open both in the UI.

**Acceptance Scenarios**:

1. **Given** an installation made for another operating system or architecture, **When** the vault is opened, **Then** the tool
   is shown as "does not fit this device — reinstall" and is not started.
2. **Given** a damaged or incomplete installation (a file missing, or the program's file size differing from the one
   recorded at install), **When** the user selects the backend, **Then** it is shown as needing a reinstall and is not
   started.
3. **Given** a vault arriving on a new device through sync, **When** it is opened, **Then** tools are shown as not installed,
   because files of a vault folder never sync.

---

### Edge Cases

- A download is cut off, the network is gone, or the vendor's server answers with an error: the install ends with a specific
  reason, and no half-installed version looks usable (FR-007).
- The download does not match its integrity information: it is not installed, not run, and the user is told why (FR-006).
- The app is killed during an install or update: the next start finds either the complete previous version or the complete
  new one, never a mixture, and leftovers of the interrupted attempt are cleared.
- Two installs of the same tool start at once in one vault (two windows): one runs, the other waits or is refused; the result is
  never a mixture.
- An update or removal is requested while a request or a connect flow uses the tool: the running use is not disturbed
  (FR-010).
- Several vaults each hold their own copy: this is wanted; each vault shows its own state and nothing is shared between
  them outside the vault folders.
- The vault folder is on a path where running downloaded programs is not allowed (for example a mount without execute
  permission): the tool is shown as not runnable with that reason, and holzi does not copy it elsewhere on the host to
  work around it.
- The tool's version is older than the oldest entry of the tested-version list: the backend says so and offers the update;
  a version newer than the list (a vault from a newer holzi) is shown as untested and still started.
- The user is offline when they open the update: the update offers the listed versions without any network need; only the
  download itself needs the network.
- A tool is removed that a stored delegate provider row still refers to: the row stays, shows "not installed in this vault".
- The first run with a new version fails because of the version itself (for example a changed protocol): it does not
  confirm, the previous version is still there, and the user can go back; holzi does not decide that for them (FR-022).

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Every vault MUST have its own area for CLI tools, `<vault folder>/cli-tools/<vendor>/<version>/`, and its own
  home folder, `<vault folder>/home/`. Both lie in the vault folder of spec 084 as subfolders of their own; the vault
  folder's root keeps holding only the SQLite files and the small bookkeeping files (spec 084 FR-002). The vault folder
  itself is never used as a home.
- **FR-002**: The paths of FR-001 MUST be resolved by the one function that resolves the vault's paths (spec 084 FR-003);
  no other module builds them.
- **FR-003**: holzi MUST NOT look for a delegate tool on the host: no `PATH` lookup, no fixed host location, no fallback to a
  host copy when the vault has none. A provider row no longer carries a binary name or path.
- **FR-004**: For every supported vendor the user MUST be able to install, update, see the installed version of, and remove
  the tool, for the current vault, from the UI. When a delegate backend is chosen or connected while its tool is missing,
  the install is offered there.
- **FR-005**: holzi MUST download and unpack the tools itself, with nothing on the host but holzi: no Node, no npm, no
  shell script. The download MUST show progress and be cancellable, and a failure MUST name its reason (offline, server
  error, not enough space, integrity).
- **FR-006**: holzi MUST verify what it downloaded against integrity information (checksum or signature) for that exact
  version before anything of it is run, and MUST refuse a mismatch. Each entry of the tested-version list (FR-009) carries
  that information, so the check does not depend on the vendor's server at install time; where the vendor itself publishes a
  checksum or signature, the entry's value MUST have been taken from or checked against it. If neither can be established
  for a vendor, the install for that vendor MUST NOT proceed until the operator has decided what replaces it (see Open
  risks).
- **FR-007**: An install or update MUST be complete or absent: it is prepared aside, verified, and only then made the
  installed version. An interrupted attempt MUST leave the previous state intact and be cleared at the next start.
- **FR-008**: holzi MUST NOT install or update a tool without the user's action, and MUST keep each tool's own
  self-update switched off.
- **FR-009**: holzi MUST keep, per vendor, a list of the versions it has tested with its adapter. An install and an update
  MUST only install a version of that list: from the UI always the newest one (the command accepts another listed version
  for tests and the quickstart; the UI offers none). "A newer version is
  available" means a newer entry in the list; holzi does not ask the vendor for its latest version. A tool installed in a
  version that is not on the list (for example a vault from a newer holzi) is shown with that fact and is still started.
- **FR-010**: An update or a removal MUST NOT disturb a delegate request or a connect flow that is already using the
  tool (a removal is refused while one runs); a changed version applies to what starts afterwards. Closing the vault is
  the exception: it stops every running delegate process of the vault, which ends as cancelled, and no process started
  from the vault folder outlives the open vault.
- **FR-011**: Every process holzi starts for a delegate (a request, a connect flow, a version query) MUST be contained so
  that it neither reads nor writes host state outside the vault folder, as far as the platform and the tool allow. The
  containment MUST change only what the tool needs relocated, MUST leave alone what the tool needs from the host (such as
  proxy and certificate settings, `PATH`, locale), and MUST NOT depend on the host having left any setting unused. The
  per-run config and working folders keep the fresh, per-run behaviour of spec 007 and MUST lie inside the vault's home
  folder, not in the host's temp folder, and be removed when the run ends.
- **FR-012**: Using, installing, updating or removing a delegate tool MUST NOT create or change any file outside the vault's
  folder (extends spec 007 FR-003 and SC-004 to the tools themselves).
- **FR-013**: Before a tool is started holzi MUST check that its installation fits this device's operating system and
  architecture and is complete (every file present, the program's size equal to the one recorded at install). If not, the
  backend MUST show "reinstall" and the tool MUST NOT be started. Damage that keeps the size (a changed byte) is not
  detected at start; a start that fails is reported as "not runnable" with the system's reason.
- **FR-014**: Removing a vault's files MUST remove its tools and home folder with it (spec 084 FR-007), and removing one
  tool MUST remove all its versions and leave other tools and the home folder in place.
- **FR-015**: The list of vendors MUST be data plus an adapter: a vendor definition names where the tool comes from, which
  platforms it exists for, how it is verified, and which home- and config-variables it needs; adding a vendor MUST NOT
  change the install, update, removal or path logic.
- **FR-016**: The tool manager MUST work on desktop only; on Android and iOS the delegate backends stay unavailable with the
  reason `platform` (spec 043 FR-016, FR-026), and nothing is offered to install.
- **FR-017**: A tool installed in one vault MUST NOT be visible to, or shared with, another vault; no device-wide download
  cache or install location is used, since either would be a trace outside the vault folders.
- **FR-018**: The installation MUST NOT sync to other devices; each device installs for itself (spec 084: files of a vault
  folder never sync).
- **FR-019**: Everything about a vault's tool and delegate activity MUST land in that vault's folder: the tools, the home
  folder, the per-run folders, and every record of what happened (a tool's own logs and history, holzi's log of an install,
  an update, a connect flow or a run). No device-wide file (holzi's own log included) may contain the vault's name, the
  tool a vault uses, or what a run did; a device-wide file may only carry what holds true for every vault alike. Records of a
  vault's tool activity live in a subfolder of the vault's folder, so that they too can be encrypted later.
- **FR-020**: Starting a tool MUST go through one containment interface that decides how a process is contained and
  reports the isolation level it reached, `best effort` or `enforced`. This spec supplies one implementation, the
  portable base layer, which always reports `best effort`; a later OS sandbox plugs into the same interface without
  changing the install, update, removal or run logic.
- **FR-021**: The Settings page MUST show each installed tool's isolation level, and a version MUST enter the
  tested-version list only after a containment measurement (the files it creates, writes or removes outside the vault
  folder, on the platforms holzi runs it on) showed no residue other than those listed under Assumptions.
- **FR-022**: Every newly started request MUST resolve to the highest complete version of its tool. A request already
  running MUST keep the version it selected at start. A second update MUST be refused while the current version is
  unconfirmed; the user must first confirm it with a normally ended run or revert it. An update MUST also be refused
  while a fallback is pending deletion for an active run. An install or update MUST pass a start check before it becomes
  usable: the vendor's program is run under the same
  containment as a real run, asks for
  its version, and must exit successfully and report the version it was installed as; otherwise the attempt is
  discarded and the previous version stays in use. After an update the previous version MUST stay on disk until the new
  version is confirmed, which happens when the first delegate request or connect flow that used it ends normally (a
  stop by the user, a missing login or a network error does not confirm). On confirmation the older versions MUST be
  deleted once no active run uses them; cleanup MUST be retried after those runs end and at the next vault open if a
  program still cannot be deleted. While the newest version is unconfirmed and an older one exists, the user MUST be able to go back to the
  older one, which deletes the newest. holzi MUST NOT switch to an older version by itself at run time.

### Key Entities

- **CLI tool**: A vendor's command-line program a delegate runs (Claude Code, Codex; later the Abacus AI CLI).
- **Tool installation**: One version of one tool for one operating system and architecture, in one vault's
  `cli-tools/` area; complete or absent; **confirmed** once a run with it has ended normally (FR-022).
- **Vault home**: The vault's own home folder, `<vault folder>/home/`, given to every delegate process, holding the per-run
  folders while a run lasts.
- **Vendor definition**: The data that describes a vendor: where each tested version comes from, which platforms it exists
  for, its integrity information, and the home- and config-variables the tool needs.
- **Tested-version list**: Per vendor, the versions holzi has tested with its adapter, newest first; it is what an install
  or update chooses from.
- **Isolation level**: What the containment of one tool's processes guarantees on this platform, `best effort` (the
  base layer redirects what it can and the residue is measured and listed) or `enforced` (an OS sandbox blocks it).
- **Integrity information**: What lets holzi tell the vendor's real file from a changed one (checksum, signature).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: On a machine with no `claude`, `codex`, `node` or `npm` on `PATH`, a user goes from an opened vault to a
  delegate-answered reply using only the app, and the install takes under 5 minutes on a 50 Mbit/s connection.
- **SC-002**: With a different host `claude` or `codex` first on `PATH` and a configured host home folder, 100% of delegate
  runs in a test series use the vault's copy and none starts the host's.
- **SC-003**: A file listing of the host's home, temp and config folders taken before and after install, connect, request,
  stop, update and removal shows no new or changed file outside the vault's folder, apart from the listed residuals
  (Assumptions).
- **SC-004**: Killing holzi at each step of an install and of an update, one run per step, always ends in either the
  complete previous or the complete new version after the next start, and never in a version that starts but is incomplete.
- **SC-005**: A download whose content was changed is not run in 100% of test cases, and the user sees the integrity reason.
- **SC-006**: After removing a vault, no file of its tools or home folder is left on the device.
- **SC-007**: A test double of a third vendor can be added by a vendor definition and an adapter alone, without a change to
  the manager's install, update, removal or path logic.
- **SC-008**: An update started while a delegate request runs lets that request finish normally in 100% of test cases.
- **SC-009**: After the same series, a text search of every device-wide file holzi writes (its log included, in a build with
  logging on) for the vault's name, the tool's name and the text of a request finds nothing, while the vault's own folder
  holds the records of the series.
- **SC-010**: In 100% of states where a tool is installed, the Settings page shows its isolation level, and with the
  base layer only it reads `best effort`.
- **SC-011**: In a test series of updates, exactly one version folder of the tool exists after an update that failed its
  start check, and exactly two between an update and the first normally ended run with the new version. If another run
  still uses the fallback when the new version is confirmed, its folder remains pending deletion until that run ends;
  after the next successful cleanup, exactly one version folder remains, in 100% of cases.

## Assumptions

- Builds on spec 084 (vault folders, one path function, files never sync, removal removes the folder) and spec 007
  (delegate adapters, isolation, approval bridge). The adapters keep their behaviour; only where the tool comes from and
  where the process's folders lie changes.
- holzi has no users yet, so there is no migration: delegate provider rows that exist only on the operator's devices are set
  up again, and no legacy `PATH` fallback or binary-path override is kept. An override can be added later if asked for.
- Each vault holding its own copy is wanted by the operator and its disk cost is accepted; no shared cache is used.
- The operator's rule, "everything that concerns a vault lands in that vault's folder", holds for the whole app. This spec
  applies it to tool and delegate activity (FR-012, FR-019). Other places that break it today (other features' log lines,
  device-wide files that name a vault) are not changed here; the plan lists those it finds, and each becomes a spec of its
  own unless it is a one-line fix inside the files this feature touches.
- **Listed residuals** (measured 2026-11-16, accepted for this spec, to become literal zero with the OS sandbox):
  Claude Code's per-session inbox socket in the system temp folder (`/tmp/cc-socks-<uid>/<pid>.sock`: removed at exit,
  its folder stays), and, only where the vault folder's path is too long for a Unix socket address, holzi's own approval
  socket in the system temp folder under a random name (removed at the end of the run). Neither carries a vault name or
  any content.
- For Claude Code and Codex the only way to relocate their configuration and state is an environment variable of the
  tool (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`); neither has a flag for it (Codex can move only its logs and SQLite
  files with `-c`). The base layer therefore sets those variables for the child process alone and nowhere else.
- The OS sandbox (Linux namespaces set up by holzi itself rather than a host program, macOS Seatbelt) is a later spec;
  Windows has no comparable unprivileged mechanism and stays `best effort`. A container runtime is not used.
- The operator wants the debug log to be switchable at runtime in release builds, so that a user with a problem can switch it
  on and attach the log to a ticket (2026-11-16). That is a spec of its own, not part of this one. This spec only fixes the
  boundary it has to respect: FR-019 holds with the log on, so a log a user sends contains no vault-specific content unless it
  is a record kept in that vault's folder and handed over on purpose.
- The vault's subfolders can be encrypted later with keys from the opened vault (spec 084). This spec only keeps
  `cli-tools/` and `home/` as separate subfolders; running a tool needs a decrypted view of its folder while the vault is open,
  which the later encryption has to provide. Encryption itself is out of scope.
- The user's subscription credential stays in the vault database as in spec 007; this spec does not change how it is
  obtained or stored.
- A third vendor, the Abacus AI CLI, is spec 096. Known so far: its npm installer downloads
  `https://static.abacus.ai/cli/releases/<version>/abacusai-<platform>.tar.gz` and its tool honors `HOME`; this is a fact
  for spec 096, not a requirement here.

## Open risks

Resolved by the research phase on 2026-11-16 (details in `research.md`): both vendors publish checksums for their
programs (Claude Code: a GPG-signed manifest; Codex: `codex-package_SHA256SUMS`), so FR-006 is satisfiable for both; neither
tool's relocation is possible without an environment variable of the tool itself, and the writes outside the vault folder
that remain are the two listed residuals; each tool's own update can be switched off (`DISABLE_UPDATES=1`;
`-c check_for_update_on_startup=false`). Still open:

- **What reaches device-wide files today.** holzi's file log (`<AppLocalData>/logs/`, debug builds) was not read for vault
  names, tool names or request text, and the webview's own stores in the data folder (`storage`, `localstorage`,
  `WebKitCache`, `cookies`) were not checked for vault-specific content. FR-019 and SC-009 need a measured baseline, and
  a way to keep tool and delegate records out of the device-wide log (for example by routing them to a per-vault record).
- **Residual host traces measured so far.** Claude Code binds a per-session inbox socket even under `-p`, at a fixed place
  in the system temp folder (`/tmp/cc-socks-<uid>/<pid>.sock`) that neither `HOME` nor `TMPDIR` moves; it is removed at exit
  but its folder stays. Only its "bare mode" skips it, and bare mode cannot use a subscription login (spec 007 research §3).
  FR-012 therefore cannot be met literally for this tool on macOS and Linux before the OS sandbox exists; the
  operator accepted listing it as a residual (Assumptions) and showing the isolation level.
- **Where programs may be run from.** A vault folder on a mount without execute permission, or a folder an endpoint
  scanner blocks, can stop a tool from starting; the state must be reported clearly (see Edge Cases).
- **Are the vendors' terms satisfied** by holzi downloading their official program on the user's behalf into the user's own
  vault? The spec assumes the same position as spec 007's accepted risk for driving the raw CLIs, and does not resolve it.
- **Who keeps the tested-version list current, and how a version gets onto it.** The approval protocols of both tools are
  verified only against single versions (spec 007 research). Entering a version needs a repeatable check (the protocol
  smoke test of spec 007 research §1, plus the containment measurement of FR-021) and its recorded integrity information;
  a release of holzi is needed to ship a new entry. The maintainer script of `tasks.md` automates the checks; whether CI
  runs them against the real vendors is not decided.
- **Platforms not measured.** All measurements ran on Linux x86-64. macOS and Windows behaviour of the containment
  (temp folder, account folders, executable bit) and the downloaded programs there is assumed from vendor documentation
  and covered by the quickstart only.
- **A real turn.** The measurements stopped at the app server's handshake and an unauthenticated `-p` run; a turn that
  uses the tools' shell or patch tool may write elsewhere or need Codex's `rg` and `bwrap`. The quickstart covers it.
