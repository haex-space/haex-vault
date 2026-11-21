# Feature Specification: End-to-End Testing of the Real Application

**Feature Branch**: `016-e2e-testing`

**Created**: 2026-09-21

**Status**: Draft

**Input**: User description: "A developer can run automated tests that start the real, built HaexVault
application with its embedded interface, drive it the way a user does and observe what it does, on a
Linux development machine, without opening a window on the developer's desktop and without touching
the developer's own HaexVault data. The first scenarios check the vault-close promises of spec 013:
pressing the lock control ends the process within seconds, a reply that is still streaming is
cancelled, the window shows only a spinner while it closes, and a relaunch brings the app back.
Writing further scenarios must be short, failures must leave enough to diagnose them, and the tools
must come from the project's Nix development shell."

## Clarifications

### Session 2026-09-21

- Q: Should the default run also build and test a release-profile build so the relaunch scenario runs, or
  only the fast debug build? → A: The suite always builds and tests the debug build. A release build is
  tested only when one is built (a release), by pointing the suite at it; the relaunch scenario runs then
  and is reported as skipped in an ordinary run.

## User Scenarios & Testing _(mandatory)_

The users are the maintainers and contributors who write and run these tests. Each story below can
be delivered and used on its own.

### User Story 1 - Run the whole suite with one command, safely (Priority: P1)

A maintainer inside the development shell types one command. The suite prepares the application
under test, gives it a virtual screen, runs every scenario, prints a result per scenario and ends
with a status a script can use. It leaves nothing behind, shows nothing on the maintainer's screen and
does not touch the maintainer's own HaexVault data or a HaexVault they have open at the time.

**Why this priority**: Without a trustworthy way to run the real application, no scenario is worth
writing. A run that opens windows, leaves processes or damages real data would be avoided, and then
the whole feature is unused.

**Independent Test**: Run the command with one trivial scenario (start the application, see the
start screen, end it). Then check the machine: no window appeared, no process of the run survives, the
maintainer's own HaexVault data is byte-for-byte unchanged, and a HaexVault the maintainer had open kept
running.

**Acceptance Scenarios**:

1. **Given** the development shell and a checkout, **When** the maintainer runs the suite command,
   **Then** every scenario runs without a manual step, each gets a pass, fail or skipped result with
   its duration, and the exit status is failing if any scenario failed.
2. **Given** a run in progress, **When** the maintainer watches their desktop, **Then** no window of
   the application under test appears.
3. **Given** a run that is interrupted (Ctrl-C), that hits a time limit, or that has a failing
   scenario, **When** it ends, **Then** no process the run started (application, driver, virtual screen,
   stand-in provider) is left, and an immediate second run works.
4. **Given** the maintainer has their own HaexVault running with real data, **When** the suite runs and
   ends, **Then** that HaexVault is still running and its data is unchanged.

---

### User Story 2 - The close promises of spec 013 are checked automatically (Priority: P1)

The scenarios that were done by hand for spec 013 (tasks T048 to T050 and quickstart scenarios 1, 2
and 8) run on their own: locking while a reply streams, closing the window, what the window shows
while it closes, and what happens after a relaunch.

**Why this priority**: This is the reason the feature exists. These promises are only kept by the
whole application, and they are the kind that regresses without anyone noticing.

**Independent Test**: With a stand-in provider that streams without end, unlock a vault, start a
reply and press the lock control. The scenario passes only if the application process ends within the
promised time and the stand-in provider sees its connection close. Repeat with the window close
control. Then weaken the behavior on purpose (do not cancel the stream) and see the scenario fail.

**Acceptance Scenarios**:

1. **Given** an unlocked vault and a reply that is still streaming from a stand-in provider, **When**
   the lock control is pressed, **Then** the application process ends within 4 seconds, the stand-in
   provider records its connection closing within 1 second of the press, and no error appears.
2. **Given** the same state, **When** the window is closed instead, **Then** the same outcomes hold.
3. **Given** the page a closing application shows, **When** it is displayed, **Then** it contains no
   text at all and a single spinner, on a dark and on a light color scheme.
4. **Given** a build that relaunches on close, **When** the lock control is pressed, **Then** the old
   process is gone and a new process of the same application appears whose window shows the unlock
   screen, within 10 seconds.
5. **Given** an unlocked vault with nothing running, **When** the lock control is pressed twice in a
   row, **Then** the process ends once, no error is shown, and nothing needs to be pressed again.

---

### User Story 3 - A new scenario is short to write (Priority: P2)

A contributor who wants to check another behavior writes only what is specific to it. Starting a
fresh application with its own data, unlocking a vault, calling a backend command, clicking and
typing, waiting for a condition and taking a screenshot all come from a small set of ready helpers.
A stand-in provider that streams slowly, streams forever or fails is one call away.

**Why this priority**: The first five scenarios prove the approach; the value grows with every
scenario added, and that only happens if adding one is cheap.

**Independent Test**: A contributor who has not seen the suite writes the scenario "create a vault,
unlock it, call one backend command, check its result" using only the helpers and the documentation,
and it runs in the suite.

**Acceptance Scenarios**:

1. **Given** the helper set, **When** a scenario needs an application, **Then** one call gives a
   running instance with empty data locations of its own, and the instance and everything it started
   is removed when the scenario ends, whatever its result.
2. **Given** a scenario that presses a control the application closes over, **When** the reply never
   comes because the page was replaced, **Then** the helper reports that the application ended rather
   than failing with a confusing timeout, if the scenario declared that outcome as expected.
3. **Given** the stand-in provider, **When** a scenario asks for its behavior (stream forever, stream
   slowly then finish, answer with an error) and later asks what it saw, **Then** it gets every
   connection with when it opened and when it closed.

---

### User Story 4 - A failed scenario explains itself (Priority: P2)

When a scenario fails, the run keeps what a maintainer needs to see why without running it again:
what the window looked like, what the application printed, what the stand-in provider was asked, and a
timeline of the steps with timestamps. The summary says where these are.

**Why this priority**: Tests that fail without a reason are switched off. The scenarios are timing
sensitive, so a failure that cannot be reproduced at once is common.

**Independent Test**: Seed three failures (the application never starts, the process does not end
after the lock control, the provider connection stays open) and check that for each one the kept
material names the step that failed.

**Acceptance Scenarios**:

1. **Given** a failing scenario, **When** the run ends, **Then** the summary names a location that
   holds a screenshot from the moment of failure, the application's output, the stand-in provider's
   request log and the timeline.
2. **Given** a passing run, **When** it ends, **Then** it leaves no bulky material behind, only the
   summary.

---

### User Story 5 - Missing tools and mismatches stop the run early (Priority: P2)

The tools the suite needs come from the project's development shell. If one is missing, or the driver
that talks to the application's web view is not the same version as the web view the application uses
on this machine, the run stops before starting anything and says what is wrong and how to fix it.

**Why this priority**: A version mismatch fails in ways that look like application bugs. Catching it
in seconds saves an afternoon.

**Independent Test**: Start a run with a tool hidden from the path and with a deliberately different
driver version, and check that each stops the run within 10 seconds with a message naming the problem
and the remedy, and that nothing was started.

**Acceptance Scenarios**:

1. **Given** a required tool is not available, **When** the suite starts, **Then** it stops before
   starting any process and names the tool and how to get it.
2. **Given** the driver and the machine's web view differ in version, **When** the suite starts,
   **Then** it stops before starting the application and shows both versions.

---

### User Story 6 - The same suite runs in the repository's CI (Priority: P3)

The suite can also run in the repository's continuous integration on a stock Linux runner, with the
tools installed from the distribution. This is separable: everything above works without it.

**Why this priority**: Local runs catch a regression when someone remembers to run them; CI catches it
on every change. It is worth having, but the local flow must not wait for it.

**Independent Test**: A CI job runs the suite on a clean runner and reports the same results; a
failing scenario uploads its kept material.

**Acceptance Scenarios**:

1. **Given** a clean runner, **When** the job runs, **Then** the suite runs the same scenarios as the
   ordinary local run (against the debug build) and the job fails exactly when a scenario fails.
2. **Given** a failing scenario, **When** the job ends, **Then** the kept material is available as a
   downloadable result of the job.

---

### Edge Cases

- The maintainer has their own HaexVault (for example a development instance) running: the suite must
  neither attach to it, nor stop it, nor match it by name. It identifies its own processes exactly.
- A network port the suite would use is already taken, or a run killed earlier left a process behind
  that holds a resource: the next run must not hang; it picks free ports and reports or removes what
  it started before.
- The application never shows a window, crashes at start or hangs: the scenario fails at its time
  limit with the output, and the run continues with the next scenario.
- A scenario needs the build that relaunches on close, but only the build that exits is available (the
  normal case in an ordinary run, which uses the debug build), or the reverse: the scenario is reported
  as skipped with the reason, never as passed.
- The application's texts are German by default and may change: no scenario depends on a displayed
  text or on the interface language.
- Submitting a form with the keyboard did not work through the driver in the spike, clicking the
  button did: helpers offer the click, and a scenario does not depend on key handling by accident.
- The application renders without hardware acceleration on a virtual screen and warns about it: the
  warning is expected and is not a failure.
- The application's data includes a models directory shared by all vaults: a scenario's isolated data
  must include it, so a scenario can neither see nor change the maintainer's real models, and no
  scenario downloads anything.
- A call made from the page during a close never gets an answer, because the page is replaced: the
  suite treats the application ending as the expected outcome for scenarios that say so.
- Two scenarios must not disturb each other: each gets its own instance, data and ports, so a failure
  in one leaves the next in a clean state.

## Requirements _(mandatory)_

### Functional Requirements

**Running the suite**

- **FR-001**: One documented command, run from the development shell, MUST run every scenario and end
  with a status that is failing if and only if a scenario or a preflight check failed. Missing required
  tools and driver/web-view version mismatches MUST fail the preflight before any scenario starts.
  Skipped scenarios MUST be listed with their reason and MUST NOT count as passed.
- **FR-002**: The suite MUST run the real application as built for a user, with its embedded
  interface. A development server or a browser stand-in for the interface is not sufficient.
- **FR-003**: A run MUST NOT open any window on the maintainer's desktop.
- **FR-004**: By default the suite MUST build the debug-profile application itself and test that. It
  MUST NOT build a release-profile application on its own. It MUST accept a path to an already built
  application, for example a release build made for a release, and then skip the build and test that
  one. The report MUST name the application used and which close behavior (relaunch or exit) it has.
- **FR-005**: The suite MUST enforce a time limit per scenario and per run. A scenario that reaches
  its limit fails with a diagnostic, its instance is removed, and the run continues.

**Isolation and cleanup**

- **FR-006**: Every scenario MUST start the application with data, configuration, cache, runtime and
  home locations of its own, empty at the start and removed at the end, including the models directory,
  and MUST NOT use the maintainer's desktop session bus. The maintainer's own HaexVault
  data MUST NOT be read or written.
- **FR-007**: A scenario MUST NOT depend on the internet. It uses local stand-ins for every external
  service and needs no credentials, and it downloads nothing.
- **FR-008**: However a run ends (all passed, a failure, a time limit or an interrupt), no process the
  run started MAY remain. The suite MUST identify its processes exactly and MUST NOT stop or attach to
  any other process, including another HaexVault.
- **FR-009**: The suite MUST choose network ports that are free at the moment of use, and a leftover
  from an earlier killed run MUST NOT make a later run hang.
- **FR-010**: Scenarios MUST be independent: a fresh instance for each, any order, and a failure in
  one leaves the next unaffected.

**Tools and checks**

- **FR-011**: Before starting anything, the suite MUST check that every required tool is present and
  that the driver is the same version as the web view the application uses on this machine, and MUST
  stop with a message naming the problem and the remedy if not.
- **FR-012**: The required tools MUST be provided by the project's development shell definition, so
  that a contributor needs no manual installation beyond the platform's own web view that the
  application already needs to build.

**Helpers for scenarios**

- **FR-013**: The suite MUST provide helpers to: start an isolated instance and end it; call a backend
  command and receive its result; report, for a call that gets no answer because the application
  ended, that the application ended; click and type into a control found by a stable hook; wait for a
  condition with a time limit and a message that names it; check whether the application's process
  is alive; and take a screenshot.
- **FR-014**: Every control a scenario needs MUST be reachable by a hook that does not depend on the
  displayed text or the interface language. Where the interface has none, a minimal hook MUST be
  added that changes neither behavior nor appearance.
- **FR-015**: The suite MUST provide a stand-in provider on the local machine that the application can
  be pointed at through its existing provider settings. It MUST offer at least these behaviors: stream
  without end, stream slowly and then finish, and answer with an error status. It MUST record every
  connection with its opening and closing time and every request it received.

**Close scenarios (spec 013)**

- **FR-016**: The suite MUST include a scenario for each of: locking while a reply streams, closing
  the window while a reply streams, the closing page, a relaunch after locking, and locking twice in a
  row. Their assertions are the acceptance scenarios of User Story 2.
- **FR-017**: A scenario that depends on the close behavior of the build (relaunch or exit) MUST
  declare it, and a run against a build with the other behavior MUST report it as skipped with the
  reason.
- **FR-018**: The relaunch scenario MUST observe the relaunch from outside the application (the old
  process gone, a new process of the same application present, its window showing the unlock screen),
  because the connection used to drive the application ends with its process.

**Diagnostics**

- **FR-019**: For every failed scenario the run MUST keep a screenshot from the moment of failure, the
  application's output, the stand-in provider's request log and a timeline of the scenario's steps with
  timestamps, and the summary MUST say where they are. A passing run MUST keep only its summary.
- **FR-020**: The report MUST record per scenario its duration and the time of its key steps (start of
  the instance, press to process end), so a slowdown shows before it becomes a failure.

**Continuous integration (separable)**

- **FR-021**: The suite MUST be runnable on a stock Linux CI runner with the tools installed from the
  distribution, and a failing run MUST publish the kept material. Nothing in the local flow depends on
  this.

### Key Entities

- **Scenario**: One behavior to check, with its own instance, its expected outcome and a declared
  need, if any, for a particular close behavior of the build. It ends as passed, failed or skipped.
- **Application under test**: A built application with its embedded interface, identified by its path
  and by whether it relaunches or exits on close.
- **Isolated instance**: A running application with data, configuration and cache locations that exist
  only for one scenario, its own ports and its own virtual screen; removed with everything it started.
- **Stand-in provider**: A local server that plays an external model provider with a chosen behavior and
  records connections and requests.
- **Run report**: The per-scenario results with durations and key step times, the application used,
  the skipped scenarios with reasons, and the location of the kept material for failures.
- **Tool check**: The preflight that verifies tools and versions before anything starts.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: From a clean checkout inside the development shell, one command runs the close scenarios
  that apply to the debug build (four of the five; the relaunch scenario is reported as skipped with its
  reason) to the end with no manual step, in under 5 minutes not counting the build. Given a release
  build by path, the same command runs all five.
- **SC-002**: Across 20 consecutive runs, including 10 that are killed at random points, no process
  started by the suite is left behind, no window ever appears on the desktop, the data of a HaexVault
  running at the time is unchanged in every run, and every run after a killed one starts normally.
- **SC-003**: Breaking each spec 013 promise on purpose (the stream is not cancelled, the page is not
  replaced, the process does not end) makes the scenario that covers it fail in 100% of the trials,
  and restoring the behavior makes it pass again.
- **SC-004**: For each of three seeded failures (the application never starts, the process does not end
  after the lock control, the provider connection stays open) the kept material names the step that
  failed, so that a maintainer needs no second run to find it.
- **SC-005**: A contributor who has not seen the suite writes the scenario "create and unlock a vault,
  call one backend command, check the result" in 40 lines or fewer using only the helpers and the
  documentation.
- **SC-006**: A missing tool or a driver/web view version mismatch stops a run within 10 seconds, before
  the application starts, with a message that names the problem and the remedy.
- **SC-007**: Each close scenario completes within 30 seconds on the maintainer's machine, so the suite
  stays something a maintainer runs before pushing.

## Assumptions

- The target is a Linux development machine with the web view library the application already needs
  to build and the project's Nix development shell. Windows, macOS and Android are out of scope. Scenarios
  that run several application processes at once were out of scope here and have since been added by spec
  024 and extended by spec 033; each instance having its own data, ports and virtual screen made that
  possible.
- The behavior under test exists: spec 013 is merged (pull request 112). Its close policy is exit in a
  debug build and relaunch in a release build, so the relaunch scenario needs a release-profile build.
  The ordinary run uses the debug build, which keeps it fast enough to run before every push. The
  release build is tested when a release is built, by giving the suite its path (see FR-004). There is
  no release workflow in the repository yet, so wiring the suite into one is left to whoever adds it.
- A spike on the maintainer's machine (throwaway branch `spike/e2e-rig`, since removed — the suite
  replaces it) showed the approach works: the Tauri WebDriver bridge and the WebKit driver started the
  real debug binary on a virtual screen (session start about 3 seconds), backend commands could be
  called from the page, real clicks and typing worked, screenshots came back, and locking while a
  stand-in provider streamed closed its connection 1 ms later with the process gone (window close:
  about 50 ms). Details go into the plan's research.
- The driver must match the web view version, and only the driver binary may enter the development
  shell, never the web view library itself, because that would displace the host's library the
  application links. The tools and the check needed a change to the shared atoms (`haexmas/atoms`:
  `nix-devshell-base` 0.7.0 and `haex` 0.6.0), merged as `haexmas/atoms#32` and pinned in HaexVault's
  `.spaex/manifest.json`.
- The stand-in provider needs no change to the application: an API-key provider of the Anthropic kind
  accepts any base address, so a local server that lists one model and streams replies can play it.
- The application has no stable hooks for its controls today, and its texts are German. Adding
  minimal language-independent hooks is in scope; changing any other application behavior is not.
- Scenarios run one after another on one virtual screen. Running them in parallel is a later
  optimization.
- Continuous integration (User Story 6) is separable and may ship after the local suite.
- A real close lasts milliseconds, too fast to sample live: "no error appears" (User Story 2, scenarios
  1 and 2) is checked from the last page sample taken before the session ends, together with the
  stronger evidence that the process actually ended and the stand-in provider's connection actually
  closed — not by watching continuously while the window closes.
