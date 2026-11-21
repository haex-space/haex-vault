# Research: End-to-End Testing of the Real Application

**Feature**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

Every decision below has the evidence it rests on. "Spike" means the throwaway branch `spike/e2e-rig`
(not merged, not a design to copy). Experiments E1 to E4 were run on the maintainer's machine on
2026-09-21 against a debug build of the application, under a virtual display.

## Experiments referred to below

| Id  | Setup                                                                                                                                        | Result                                                                                                                                                                                                                                           |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| E1  | Closing page in the embedded build, the desktop's session bus reachable, `GTK_THEME` unset, `Adwaita:dark`, `Adwaita:light`                  | Always dark: `prefers-color-scheme: dark` matched and the page background was the dark value in all three runs                                                                                                                                   |
| E2  | Same, inside a private `dbus-run-session`                                                                                                    | Still dark. The application asked for `org.freedesktop.portal.Desktop`, D-Bus activation started the host's portal service and its desktop backend, and three backend processes outlived the runs (parent pid 1) until they were stopped by hand |
| E3  | Session bus disabled (`DBUS_SESSION_BUS_ADDRESS=disabled:`), private runtime directory, GTK settings file in the instance's config directory | No setting: light. `gtk-application-prefer-dark-theme=false`: light. `=true`: dark (`prefers-color-scheme: dark` matched, background changed). No portal process started                                                                         |
| E4  | E3 plus a private `HOME`, lock scenario with a stand-in provider streaming                                                                   | The provider's connection closed 1 ms after the press, the process was gone after 555 ms, the run left no process behind                                                                                                                         |

## R1 Runner: `node:test` and a thin scenario wrapper

**Decision**: Scenarios are TypeScript files run by Node's built-in test runner (`node --test`), the way
the existing `scripts/check-*.ts` files are run directly by Node 22.19 or later with type stripping. A
small `scenario()` wrapper in `scripts/e2e/lib/scenario.ts` gives each scenario its isolated instance,
its own deadline, a timeline, failure artifacts and the skip rule for the close behavior. A command
file, `scripts/e2e/cli.ts`, does the work around the tests: preflight, build, orphan sweep, running,
summary, cleanup.

**Rationale**: No new dependency, no new framework: the project has no TypeScript test framework
today, and the constitution asks for the smallest thing built on what is adopted. `node:test` already
gives file discovery, name filters, skip with a reason and an exit status. The parts it cannot do (a
per-scenario deadline that first captures a screenshot, a run-wide cleanup on Ctrl-C, a report with step
times) sit in the wrapper and the command file, which the feature needs anyway.

**Alternatives considered**:

- _Playwright or WebdriverIO, as in the sibling project's rig_: pulls in a large dependency tree and a
  browser-automation model built for browsers, not for a WebKitGTK webview behind `tauri-driver`. The
  sibling rig also relies on Docker, which is out of scope here.
- _`selenium-webdriver` from npm_: a maintained client, but a new dependency for what the spike did in
  about 60 lines of `fetch`. The W3C calls needed are: new session, delete session, execute script
  (sync and async), find element, element click, element value, screenshot, window close, navigate.
- _Vitest_: not adopted anywhere in this repository; would add a second runner next to the scripts.
- _A hand-written runner_: more control, but reinvents discovery, filtering and reporting.

## R2 Driving the application: `tauri-driver` with `WebKitWebDriver`

**Decision**: The suite talks W3C WebDriver to `tauri-driver` (2.0.6), which starts the real binary
through `WebKitWebDriver`. Backend commands are called from the page with
`window.__TAURI_INTERNALS__.invoke` through an async script, which resolves to a result object instead
of throwing, so the runner can tell an error from an application that ended.

**Rationale**: It is the only route that runs the built binary with its embedded interface (FR-002) and
was shown to work in the spike: session start about 3 seconds, real clicks and typing, screenshots,
backend calls.

**Evidence and limits**: Pressing Enter through the driver did not submit the unlock form; clicking the
button did (helpers offer `click`, no key-based submit). A call that closes the application, such as
`close_instance` from the page, never answers because the page is replaced; the session then reports
"session gone". The helper turns that into the outcome `ended` when the scenario declared it expected.

## R3 Tools: from the dev shell, checked before anything starts

**Decision**: `tauri-driver`, `WebKitWebDriver` and `xvfb-run` come from the Nix dev shell through the
atoms molecules (haexmas/atoms#32, merged as `d5c48d0eb6662da84086bf69a3ca041a494e2b26`). haex-vault's pin of
the atoms repository is still the older revision and has to be moved, with `spaex install`, before the
tools exist in the shell (Stage 0 of the plan). The webview library itself stays the host's.

The preflight (`scripts/e2e/lib/preflight.ts`) resolves each tool from `PATH` and never assumes Nix, so
the same code serves a CI runner with distribution packages (R15).

**Version check**: The driver's version is read from `share/webkit-webdriver/version` next to the
`bin/` directory the driver was found in (the file the atoms change delivers, because the driver has no
`--version` option). Where that file does not exist (a CI runner), it falls back to the installed
package version of `webkit2gtk-driver`, reduced to the upstream `major.minor.patch`. The webview's
version is `pkg-config --modversion webkit2gtk-4.1`. Both are compared as `major.minor.patch`. If a
version cannot be determined the preflight fails and says which source was missing, rather than passing
unchecked.

**Rationale**: A driver that differs from the webview fails in ways that look like application bugs
(FR-011, SC-006). Failing early costs seconds.

**Alternatives considered**: putting `webkitgtk_4_1` itself into the shell was rejected in the atoms
change: its libraries would be found ahead of the host's through `LD_LIBRARY_PATH` and
`PKG_CONFIG_PATH` and the application would link a different WebKit than the one that ships.

## R4 The application under test and its close behavior

**Decision**: Without options the suite runs `pnpm tauri build --debug --no-bundle` (default features,
the profile a developer already builds, with the interface embedded) and tests
`<target>/debug/haex`, where `<target>` is `CARGO_TARGET_DIR` if set, else `src-tauri/target`. It never
builds a release profile itself. `--app <path>` skips the build and tests that binary (FR-004, clarified
2026-09-21).

The close behavior of a binary is taken from its path (`/release/` means relaunch, `/debug/` means
exit) and can be given explicitly with `--close-behavior exit|relaunch`. An explicit value that conflicts
with `/release/` or `/debug/` is rejected during preflight. A path with neither segment and no explicit
value fails the preflight and says how to state it. The report names the binary, the source of the value
and the value.

**Rationale**: The policy is a compile-time choice in `vault_gate::close_policy()` (debug: exit,
release: relaunch), and exposing it from the application would be an application change the spec puts
out of scope. Rejecting a conflicting declaration prevents a path-derived release or debug build from
being silently classified as the other behavior. For a path with neither segment, the report keeps the
operator-supplied declaration visible so a relaunch failure can be diagnosed against that declaration.

**Consequences**: Only the relaunch scenario needs a relaunching build. The other close scenarios track
the pid of the process they started, so "the process ended" means that pid, in both kinds of build.

**Open**: That a release-profile binary can be driven by `tauri-driver` at all (the driver needs the
webview's automation mode, which Tauri enables from an environment variable rather than by profile) has
not been run. It is the first check of Stage 3 for the relaunch scenario (risk in the plan).

## R5 Isolation: what "its own data" has to include

**Decision**: Each instance gets a private directory tree, created empty and removed at the end, and the
application is started with:

- `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` inside it (the vault, its settings and the shared
  models directory all resolve below `XDG_DATA_HOME`, so no model can be seen or changed and nothing is
  downloaded);
- `XDG_RUNTIME_DIR` and `HOME` inside it, so nothing written by GTK, WebKit or a library lands in the
  maintainer's home or runtime directory;
- `DBUS_SESSION_BUS_ADDRESS=disabled:`, so the application never reaches the maintainer's desktop
  session (E2, E3).
- `GDK_BACKEND=x11`, with `DISPLAY`, `WAYLAND_DISPLAY` and `XAUTHORITY` not passed on, so GTK can only
  use the instance's own virtual screen and never a Wayland session (FR-003). The application's
  environment was read from `/proc` during a run to confirm it (`DISPLAY=:99`, no `WAYLAND_DISPLAY`).

**Rationale**: E1 and E2 show a real leak the spec did not name: the application asks the desktop's
portal for its colour scheme, so a test's result depended on the maintainer's desktop settings, and a
private bus (the obvious fix) starts the host's portal services, which outlive the run. Disabling the bus
removes both. E4 shows the application still works and the lock scenario still passes. GLib treats
`disabled:` as "no session bus", on any distribution, so the same setting serves CI.

**Alternatives considered**: a private `dbus-run-session` (E2: activates and leaks host services, and
Nix's build of it needs an explicit configuration file); leaving the bus alone (test results depend on
the desktop); a container (out of scope, heavy).

**Spec alignment**: FR-006 names data, configuration and cache. The runtime directory, home and session
bus belong in that list. This is recorded as a spec-alignment item in the plan, to be folded in at
`/speckit-analyze`, not changed here.

## R6 A virtual screen per instance

**Decision**: Each instance is started as `xvfb-run -a -s "-screen 0 1280x800x24" tauri-driver …`. The
driver, the webview driver and the application inherit `DISPLAY` from the wrapper; the screen ends with
the wrapped command. `-a` picks a free server number.

**Rationale**: It keeps the Key Entities' "own virtual screen" true, costs well under a second per
instance, and needs no tool the shell does not already carry: `xvfb-run` is the delivered package and the
X server it wraps is not on `PATH` by itself, so starting it directly is not an option.

**Alternatives considered**: one screen for the whole run (simpler, but shares state between scenarios,
which FR-010 asks to avoid, and blocks parallel runs later).

## R7 Colour scheme for the closing-page checks

**Decision**: An instance has a `colorScheme` option, `light` (default) or `dark`. The runner writes
`gtk-3.0/settings.ini` with `gtk-application-prefer-dark-theme` into the instance's config directory. The
scenario asserts that `matchMedia('(prefers-color-scheme: dark)')` matches the requested scheme before it
judges the page, so a scheme that failed to apply is a clear failure, not a wrong pass.

**Evidence**: E3. This only works because the session bus is disabled (R5); with the desktop's bus the
setting is ignored (E1).

## R8 Identifying and cleaning up exactly the processes a run started

**Decision**: Every process the suite starts carries an environment marker,
`HAEX_E2E_RUN=<runner pid>:<runner start time>:<random>`. The environment is inherited by everything
below it: the spike's isolation variables reached the application through `tauri-driver` and the webview
driver the same way. That the relaunched application inherits it too is expected (a restart starts the
same binary without clearing the environment) and is the first thing the relaunch scenario checks.

- Identify: scan `/proc/<pid>/environ` for the marker (same user only). A run's processes are those with
  its own marker. The application is the marked process whose `/proc/<pid>/exe` is the binary under test.
  No name, path or command-line match is used on its own, so a maintainer's HaexVault, even the same binary,
  is never matched (FR-008).
- Start: processes with a marker whose owner (pid and start time) is gone are leftovers of a killed run.
  They are stopped and reported before the new run starts (FR-009).
- End, every path: on success, failure, deadline, SIGINT and SIGTERM the command file stops the run's
  process groups (each instance is started in its own group) and then sweeps by marker for anything that
  escaped, such as a relaunched application.

**Rationale**: The spike lost time to two identification mistakes: `pgrep -f` matched the `xvfb-run`
wrapper because its command line contained the binary path, and a leftover driver from a killed run held
a pipe and hung the next run. A marker in the environment cannot be produced by accident.

**Ceilings** (`ponytail:` comments in the code): the scan is linear in the number of processes; the free
port is chosen by binding and releasing, which leaves a small window before the driver binds it (one
retry).

**Alternatives considered**: `pkill -f` and path matching (the spike's failure); control groups or
`systemd-run` (absent in many CI containers and outside the Nix shell); `PR_SET_PDEATHSIG` (not
available from Node); a container (heavy, out of scope).

## R9 The stand-in provider

**Decision**: A loopback HTTP server started on port 0 in the runner's own process
(`scripts/e2e/lib/provider.ts`). It answers `GET /v1/models` with one model and `POST /v1/messages` per
the chosen behavior: `stream-forever`, `stream-then-finish` and `error`. It records, per connection, when
it opened and when it closed (the socket's close event), and per request its method, path and body, with
credentials removed. The application is pointed at it by adding an API-key provider of the Anthropic
kind with the server's base address, which needs no application change (spike).

**Rationale**: Runs in the same process and clock as the assertions, so "closed within 1 second of the
press" compares two timestamps taken by one clock. Loopback only, so nothing is reachable from outside
(FR-007).

**Candidates considered before authoring** (graphify): the Rust `sse_body()` helpers and the
`wiremock` servers in the adapter tests (`src-tauri/src/adapters/anthropic_stream_tests.rs`) are Rust
test fixtures for in-process adapter tests and cannot serve a Node runner, but they define the event
shapes the adapter parses, which the contract copies. `scripts/lib/chat-state-harness.ts` and
`scripts/lib/script-setup-sandbox.ts` build interface doubles for the replay checks and have no process
or network parts. None is reusable; nothing is duplicated.

## R10 Observing the close scenarios

- **Lock**: the press is a click on the control found by hook, scheduled with `setTimeout(…, 0)` inside
  the page so the WebDriver call still returns (spike). The press time is taken by the runner just before
  the call. The scenario then polls until the pid is gone and reads the provider's record.
- **Window close**: the WebDriver "close window" command. Spike: the provider connection closed within
  about 50 ms and the process ended. **Confirmed later (spec 016 Stage 4, T077 follow-up, 2026-09-23),
  with probes on the application side, that this command never reaches Tauri's `WindowEvent::
CloseRequested` at all** — it tears the window down some other way, so `window-close-while-streaming`'s
  pass rests on how fast that external teardown happens to be, not on the application's own cancellation.
  An `xdotool`-based alternative was tried and has the identical defect (`xdotool windowclose` calls
  `XDestroyWindow` directly; `xdotool windowquit`'s `_NET_CLOSE_WINDOW` needs a window manager, which
  Xvfb does not run here) — confirmed against xdotool's own upstream source. Accepted as a known
  limitation of this scenario rather than pursued further (a window manager or a custom native
  ClientMessage sender could fix it, at a cost judged not worth it for one diagnostic scenario);
  `lock-while-streaming` is what actually verifies the cancellation-on-close promise.
- **Closing page**: the closing page lasts only milliseconds in a real close (spike: the session was gone
  11 ms after the press), so it cannot be judged during one. The scenario navigates the page to
  `tauri://localhost/closing.html` and judges what it shows: no text, exactly one `.ring`, and a
  background equal to the page's own `--page` colour for the requested scheme.
- **"No error appears"**: the last page sample before the session ends contains no element with
  `role="alert"` (the interface's error pattern, as in the unlock sheet). This is best effort by nature,
  because the page is replaced; the stronger evidence is that the process ended and the provider
  connection closed.
- **Lock twice**: both clicks are dispatched in one script tick, then the runner checks the original pid
  ended once and no more than one marked process of the binary exists for 5 seconds afterwards.

## R11 The relaunch, seen from outside

**Decision**: The relaunch scenario needs a relaunching build (R4). After the press it checks, without
the driver session (which ends with the process):

1. the original pid is gone within 4 seconds;
2. a new marked process of the same binary, with a different pid, exists within 10 seconds;
3. that process has painted a window: the virtual screen is started with a framebuffer file
   (`-fbdir`, an XWD image the X server keeps current) and the runner reads it, so no extra tool is
   needed, and checks that the screen is no longer blank;
4. the unlock screen: after stopping the relaunched process, a fresh driver session over the same data
   shows the instance list with the instance not unlocked (`instance-entry` present, no active vault).
   A relaunch carries no state except what is on disk, so a fresh start over the same data is what the
   relaunched window shows.

**Rationale**: FR-018 says the relaunch must be observed from outside, because the driver cannot follow
it (spike). The driver cannot attach to a running application, so what the new window shows cannot be read
directly.

**Resolved (Stage 3, T067/T068, 2026-09-22, see tasks.md's validation record for the full runs)**: both
checks passed. **T067**: `tauri-driver` can drive a release-profile binary — a real `pnpm test:e2e --app
<release path>` smoke run passed with no leftover process. **T068**: the relaunch is observable from
outside as designed — after a lock press, the original pid ended in about 70 ms, a new marked process of
the same binary appeared in about 80 ms, and the `-fbdir` framebuffer went non-zero (chat open) → almost
blank (the instant after relaunch) → non-zero again about 2 s later (once the new window painted),
confirming steps 1 to 3. Step 4 (a fresh session over the same data shows the instance list, not the
vault) was already covered by ordinary CRDT/adoption behavior and needed no new mechanism. All four steps
of the relaunch scenario (`relaunch-after-lock.test.ts`) ship as designed; the clause "its window showing
the unlock screen" is met, not just approximated. No difference from FR-018 remained to report.

## R12 Reaching controls: hooks

**Decision**: Use what already exists and add only what is missing, as `data-testid` attributes with
kebab-case names. Existing stable hooks: `#unlock-passphrase`, `[form="unlock-form"]`,
`#create-name`, `#create-passphrase`, `#create-passphrase-confirm`, `[form="create-form"]`, and
the closing page's `.ring`. Missing: the instance entry, the open-chat button and the lock control (two
buttons, one in the sidebar and one in the header, of which one is displayed at a given width). Details
in [contracts/test-hooks.md](contracts/test-hooks.md).

**Rationale**: FR-014: no dependence on displayed text or language. Adding an attribute changes neither
behavior nor appearance.

**Cost**: The lock buttons are in `src/pages/chat/[instance].vue`, 1316 lines, far past the 500-line
boundary. Two attribute lines are added (see Complexity Tracking in the plan).

## R13 Timing assertions

**Decision**: The numbers are the promises of spec 013 and are kept in one file,
`scripts/e2e/lib/close-promises.ts`: process ends within 4 seconds (the drain ladder is 1 s cooperative,
3 s total, 0.5 s grace), provider connection closes within 1 second, relaunch within 10 seconds. These
three conformance deadlines are fixed. Waits are polls with a deadline, never fixed sleeps. `E2E_TIME_SCALE`
(default 1) may multiply generic scenario and run timeouts for loaded-machine diagnostics; such a run is
marked non-conformant and reports its scale separately.

**Rationale**: The elapsed time is the subject of the test, so a wall-clock assertion cannot be avoided,
but a fixed sleep can. Generic diagnostic timeouts may be scaled on a loaded machine, while the fixed
conformance deadlines still prove the spec-013 promises. The constitution's rule against depending on
wall-clock timing is written for Rust unit tests; it is recorded in the plan's Complexity Tracking so the
exception is visible.

## R14 Diagnostics and the report

**Decision**: A run has a directory, `<target>/e2e/<run-id>/` (already ignored by version control, and
overridable with `E2E_ARTIFACTS_DIR`). Per scenario the wrapper writes a timeline with timestamps. On
failure, before teardown, it also keeps a screenshot (skipped, with a note, if the application already
ended), the driver and application output, and the provider's request log. A passing scenario's
directory is deleted at the end, so a passing run keeps only `report.json` and the printed summary
(FR-019, FR-020). Details in [contracts/report.md](contracts/report.md).

## R15 Continuous integration (separable)

**Decision**: A separate job in `.github/workflows/ci.yml` on `ubuntu-24.04`: the existing build
packages plus `webkit2gtk-driver` and `xvfb`; `tauri-driver` 2.0.6 installed with
`cargo install --locked --version 2.0.6` and cached; then `pnpm test:e2e`. It uploads the run directory
when the run fails. Actions are pinned by commit as the other jobs are. The helper unit checks
(`pnpm check:e2e-lib`, no display and no application) run in the existing documentation job, so the
suite's own logic is checked on every change even before the full job exists.

**Rationale**: Ubuntu's `webkit2gtk-driver` and `libwebkit2gtk-4.1-0` come from the same repository and
carry the same version, which the preflight compares (R3). The job reuses the build the Rust job already
caches for its dependencies.

**Risk**: a debug build with the default features is slow on an uncached runner (the Rust job's default
leg took about 20 minutes uncached). It is a separate job, so it does not delay the other checks, and
User Story 6 is separable by the spec.

## R16 Constitution gates that shaped the design

- Test code in dedicated files: scenarios are `*.test.ts` files, helper checks are `lib/*.test.ts`, the
  helpers themselves are never mixed with tests.
- No secrets in git: the vault passphrase and the provider key are generated per run, so no literal
  credential is committed even as a fixture.
- No local absolute paths: every path is derived at run time.
- Pins: `tauri-driver` is pinned by version and hash in the atoms molecule; the atoms revision is a full
  commit hash in haex-vault's manifest.
- Graphify consulted before naming new artifacts (R9); no candidate to extend.

## Not researched, left to tasks

- The exact wording of the preflight messages and of the summary.
- Whether `--grep` maps to the runner's name filter directly (expected) or needs a wrapper.
- Whether the two lock buttons can share one hook value or need two (a helper that clicks the displayed
  one works either way).
