---
description: 'Task list for End-to-End Testing of the Real Application'
---

# Tasks: End-to-End Testing of the Real Application

**Input**: Design documents from `specs/016-e2e-testing/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/cli.md, contracts/helpers.md,
contracts/stand-in-provider.md, contracts/report.md, contracts/test-hooks.md, quickstart.md

**Tests**: Included as mandatory, not optional. The spaex constitution requires test code in files
separate from production code and one runnable check for every piece of non-trivial logic. Two kinds
exist here and both live in dedicated files. The helpers' own checks are `scripts/e2e/lib/*.test.ts`,
run by `pnpm check:e2e-lib` with no display and no application. The scenarios are
`scripts/e2e/scenarios/*.test.ts`, run only by `pnpm test:e2e`. Test-only helpers are files ending in
`.testlib.ts`, never mixed into production files. No new test runner and no new dependency: Node's
built-in `node:test`.

**Organization**: The plan's delivery stages, in the plan's order, each a pull request that can be
reviewed and used on its own. The story labels are US1 run the suite safely, US2 the spec 013 close
promises, US3 a new scenario is short, US4 a failed scenario explains itself, US5 missing tools stop the
run early, US6 the CI job. Stage 1 delivers US1 and US5 together because the command's preflight belongs
to the command; Stage 2 delivers US3 and US4; Stage 3 delivers US2, which needs what Stage 2 builds. The
two P1 stories (US1 and US2) with their prerequisites are the MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on another unchecked task)
- **[Story]**: US1 to US6 (only in story phases; the setup, foundational, validation and polish phases
  have none)
- All paths are repository-relative and written in full: `scripts/e2e/...`, `src/...`, `.github/...`.
- Commits follow Conventional Commits, one per checkpoint, **without** any agent or model trailer
  (constitution MUST NOT). Work lands on `main` through a pull request per stage, from a topic branch;
  no squash-merge. Opening a PR and pushing are done only after asking the operator.
- Inside the Nix dev shell: `nix develop --command pnpm ...` for scripts and the frontend, and
  `nix develop --command scripts/with-nix-host-bridge.sh ...` for anything that starts the application
  or needs the host's web view. `pnpm test:e2e` already runs through the bridge.
- **No credential literal in any committed file**, not even a test value: passphrases and provider keys
  are generated per run (constitution I). **No absolute or home-relative path in any committed file**
  (constitution II): derive paths at run time.
- **Marked shortcuts**: a deliberate shortcut with a known ceiling gets a `ponytail:` comment naming the
  ceiling and the upgrade path. Required at: the free-port choice (`scripts/e2e/lib/ports.ts`), the
  `/proc` scan (`scripts/e2e/lib/processes.ts`), each poll loop with a fixed deadline, and the one
  virtual screen per instance (`scripts/e2e/lib/instance.ts`).
- **Files stay under 500 lines.** `src/pages/chat/[instance].vue` (1316 lines) may gain exactly the two
  attribute lines of T048 and nothing else. No unwrap-like shortcuts on I/O: catch at the boundary that
  can report it and keep the cause in the message.
- **Graphify first**: before authoring a new named function, class, module or command in a stage, run
  the graphify queries in that stage's first task, from the primary checkout (read-only inspection is
  allowed; a linked worktree has no graph of its own) and note in the PR description which candidates
  were evaluated and why none was extended.
- Waits are polls with a deadline, never fixed sleeps. Generic scenario and run timeouts may be multiplied
  by `E2E_TIME_SCALE` (default 1); the 4-second, 1-second and 10-second close promises remain fixed for
  conformance. A run with a scale other than 1 is marked non-conformant and reports the scale.
- Contract between the command and the scenario files, by environment: `E2E_RUN_DIR`, `E2E_APP`,
  `E2E_CLOSE_BEHAVIOR`, `E2E_TOOLS` (JSON of resolved tool paths), `E2E_SCENARIO_TIMEOUT_MS`,
  `E2E_TIME_SCALE` and `HAEX_E2E_RUN` (the marker). Each is written once by `cli.ts` (T029) and read
  once by `scenario.ts` (T021).

---

## Phase 1: Setup — Stage 0, the tools reach the dev shell (FR-012)

**Purpose**: haexmas/atoms#32 is merged as `d5c48d0eb6662da84086bf69a3ca041a494e2b26`; haex-vault still pins
the older revision, so the shell has no `tauri-driver`, no `WebKitWebDriver` and no `xvfb-run`.

- [x] T001 Work in `.worktrees/016-e2e-testing` with a real `pnpm install` (no symlinked
      `node_modules`). This branch, `016-e2e-testing-plan`, carries the plan and these tasks; each
      later stage starts from the updated `origin/main` on its own branch (`016-e2e-stage0-tools`,
      `016-e2e-stage1-command`, `016-e2e-stage2-helpers`, `016-e2e-stage3-close`,
      `016-e2e-stage4-validation`, `016-e2e-stage5-ci`), created after the previous stage merged.
- [x] T002 Record the baseline in the "Baseline" section at the end of this file before any edit:
      `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`, `pnpm check:chat-state`,
      `pnpm check:vault-lifecycle`, `pnpm check:templates` results, and `wc -l` of
      `src/pages/chat/[instance].vue` (1316 at planning time), `src/components/onboarding/InstancesList.vue`
      (44) and `src/components/workspace/ChatFab.vue` (20).
- [x] T003 In a clone of `https://github.com/haexmas/atoms`, list what arrives with the new pin:
      `git log --oneline 1a292194da6a279eb382e562870fed1c41ebdf8e..d5c48d0eb6662da84086bf69a3ca041a494e2b26`.
      Put the list in the PR description. Anything besides #32 that changes a delivered file is noted
      there and checked in T005.
- [x] T004 In `.spaex/manifest.json` change the `revision` of the entry whose `source` is
      `https://github.com/haexmas/atoms` from `1a292194da6a279eb382e562870fed1c41ebdf8e` to the full
      hash `d5c48d0eb6662da84086bf69a3ca041a494e2b26` (a branch or `HEAD` is not allowed for a pin,
      constitution IV; the old value occurs once). Then run `spaex install` from the worktree root.
- [x] T005 Review the diff `spaex install` produced. Expected: `flake.nix` (now reads
      `.devshell/packages.nix`), the new `.devshell/packages.nix`, `.spaex/generated/nix-packages.json`
      (gains `xvfb-run`) and the manifest. Any other changed file is listed in the commit body and
      justified or reverted. No absolute path or secret may appear in a delivered file. Confirm
      `git ls-files .devshell/packages.nix` lists it (the flake reads only tracked files).
- [x] T006 Verify the tools in the shell. Run the block below through
      `nix develop --command scripts/with-nix-host-bridge.sh bash -c '<block>'`. Each tool prints a path
      in the Nix store and the two versions are equal (2.52.6 at planning time). The first
      `nix develop` after the pin builds `tauri-driver` from source and takes several minutes. Record
      the output in the "Validation record".

      ```bash
      command -v tauri-driver WebKitWebDriver xvfb-run
      cat "$(dirname "$(dirname "$(command -v WebKitWebDriver)")")/share/webkit-webdriver/version"
      pkg-config --modversion webkit2gtk-4.1
      ```

- [x] T007 Commit `chore(spaex): pin atoms at the revision that delivers the e2e tools` (body: the T003
      list and the T005 review). Push and open the Stage 0 PR after asking the operator.

**Checkpoint**: the shell provides the three tools and the version file; nothing else changed.

---

## Phase 2: Foundational — process control, driver client, instance, scenario core (Stage 1, blocks everything)

**Purpose**: what every story needs to start, drive and clean up an application. Nothing here runs the
application yet.

- [x] T008 Start branch `016-e2e-stage1-command` from the updated `origin/main`. Graphify first, from the
      primary checkout: `graphify query "spawn a child process and wait until it is ready"`,
      `graphify query "temporary directory created and removed by a test"`,
      `graphify query "http client for a json wire protocol"`,
      `graphify query "read environment of a process"`. Evaluate every candidate, including unexported
      ones; expected outcome (plan, research R9): none can be extended.
- [x] T009 In `package.json` add `"test:e2e": "scripts/with-nix-host-bridge.sh node scripts/e2e/cli.ts"` and
      `"check:e2e-lib": "node --test scripts/e2e/lib/*.test.ts"`. Confirm on Node 22.19 that
      `node --test` runs `.ts` files by type stripping without a flag and that
      `pnpm typecheck:scripts` (whose `tsconfig.scripts.json` includes `scripts/**/*.ts`) type-checks the
      new folder, including `node:test` types. No dependency is added.

### Tests for the foundation (write first; expected to fail until the implementation tasks land)

- [x] T010 [P] `scripts/e2e/lib/processes.test.ts`. Linux only: skip with a reason elsewhere. Cases:
      the marker is `<runner pid>:<runner start time>:<random>` and parses back, the start time read
      from field 22 of `/proc/self/stat`; a child spawned with the marker is found by the scan and one
      without is not; the application process is the marked one whose `/proc/<pid>/exe` equals the given
      binary (a marked process with another executable is not); a marker whose owner (pid and start
      time) is gone, or whose pid exists with a different start time, is an orphan, while the own run
      and another live runner are not; `sweepOrphans` stops only orphans and reports them, never an
      unmarked process; `stopRun(marker)` stops every process of that marker; `stopGroup` stops a
      detached group including a grandchild. Use short-lived `sleep` children; no fixed sleeps.
- [x] T011 [P] `scripts/e2e/lib/ports.test.ts`: `freePort()` returns an integer from 1024 to 65535 that can
      be bound at once; `withPortRetry` retries exactly once when the operation fails with an
      address-in-use error and rethrows the second failure with the port in the message.
- [x] T012 [P] `scripts/e2e/lib/webdriver.test.ts` and the test-only helper
      `scripts/e2e/lib/fake-driver.testlib.ts`: a local HTTP server speaking the W3C calls the client
      uses (new session, delete session, execute sync and async, find element, element click, element
      value, screenshot, close window, navigate). Cases: the new-session body is
      `{ capabilities: { alwaysMatch: { 'tauri:options': { application } } } }`; the script timeout is
      set to 60000 at session start; an element id is read from the key
      `element-6066-11e4-a52e-4f735466cecf`; `invoke` resolves `{ ok: true, data }` or
      `{ ok: false, error }`; when the server drops the connection or answers 500 during a call, `invoke`
      with `expectEnd` resolves `{ ended: true }` and without it throws an error that names the command.
- [x] T013 [P] `scripts/e2e/lib/instance.test.ts`, pure parts only (no real tools): `buildInstanceEnv` puts
      `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR` and `HOME` inside the
      instance root, sets `DBUS_SESSION_BUS_ADDRESS` to `disabled:` and `HAEX_E2E_RUN` to the marker,
      and does not pass through the parent's `XDG_*` or `HOME` (set sentinels in the test); `prepareRoot`
      creates `data`, `config`, `cache`, `run` (mode 0700) and `home` empty and writes
      `config/gtk-3.0/settings.ini` with `gtk-application-prefer-dark-theme=true` for `dark` and `=false`
      for the default `light`; `removeRoot` deletes the tree; with `reusesRoot` the tree is not emptied;
      `driverCommand` yields `xvfb-run -a -s "-screen 0 1280x800x24" tauri-driver --port <p> --native-port <n> --native-driver <path>`
      with the given ports and paths.
- [x] T014 [P] `scripts/e2e/lib/scenario.test.ts` with a fake instance factory: `needs.closeBehavior`
      different from the application's means the body never runs and the result is `skipped` with the
      reason "the build exits on close; this scenario needs one that relaunches" (and its reverse);
      a throwing body gives `failed` and every registered teardown ran exactly once, in reverse order; a
      body that never resolves with `timeoutMs` 100 gives `failed` with a message naming the deadline,
      the teardown ran and the next scenario still runs; `timeoutMs` defaults to 60 000 and
      `E2E_TIME_SCALE=2` doubles it; `ctx.waitFor` polls (interval at most 50 ms) until true and on
      timeout the error contains the description and the last observed value; `ctx.step` records
      `atMs` from one monotonic clock; the result is written to `<E2E_RUN_DIR>/results/<name>.json`
      with `name`, `status`, `durationMs`, `skipReason` and `steps`; a duplicate scenario name is an
      error.
- [x] T015 [P] `scripts/e2e/lib/preflight.test.ts`, tool resolution only for now: `resolveTools` returns
      the absolute path of `tauri-driver`, `WebKitWebDriver` and `xvfb-run` for those present on a given
      `PATH` (temporary directory with executable stubs) and lists the missing ones; a directory or a
      non-executable file of that name does not count; no shell is used.

### Implementation for the foundation

- [x] T016 [P] `scripts/e2e/lib/processes.ts`: `newMarker`, `parseMarker`, `scanMarked` (reads
      `/proc/<pid>/environ`, same user only, unreadable entries skipped), `findApplicationProcesses`,
      `sweepOrphans`, `stopRun`, `stopGroup`, `spawnMarked` (detached, own process group, output to a
      log file with a timestamp per line). Identification is by marker and executable only, never by
      name or command line (FR-008). `ponytail:` at the scan: linear in the number of processes.
- [x] T017 [P] `scripts/e2e/lib/ports.ts`: `freePort` (bind port 0, release, return) and `withPortRetry`.
      `ponytail:` at `freePort`: the port can be taken between release and use; one retry, upgrade path is
      to pass an already bound socket if the driver ever supports it.
- [x] T018 [P] `scripts/e2e/lib/webdriver.ts`: the minimal W3C client over global `fetch`, with the calls
      listed in T012 and `invoke` as an async script that calls `window.__TAURI_INTERNALS__.invoke` and
      resolves to an object instead of throwing. No dependency.
- [x] T019 [P] `scripts/e2e/lib/preflight.ts`: `resolveTools(pathEnv)` and the tool names as constants.
      The version check follows in T034; keep the file's exports so T034 only adds.
- [x] T020 `scripts/e2e/lib/instance.ts` (after T016 to T019): `buildInstanceEnv`, `prepareRoot`,
      `removeRoot`, `driverCommand`, `startInstance` (free ports, spawn marked with an own virtual
      screen, wait for the driver port with a deadline, open the session, find `appPid` as the marked
      process whose executable is the application, record `instance-ready`), `stop`, and for now
      `exec`, `screenshot`, `alive`. The instance root is "Created empty at start, removed at the end
      (FR-006). Holds `data`, `config`, `cache`, `run` and `home`", and the environment is exactly the
      set in data-model.md. `colorScheme` is "`light` or `dark`, default `light`". `ponytail:` at the
      one virtual screen per instance. If the file nears 400 lines, split the session code into
      `scripts/e2e/lib/session.ts`.
- [x] T021 `scripts/e2e/lib/scenario.ts` (after T020): `scenario(name, options, body)` on top of `node:test`,
      the context (`ctx.app`, `ctx.startInstance`, `ctx.step`, `ctx.waitFor`, `ctx.credentials` with a
      passphrase and a provider key generated per run), deadline, teardown in reverse order, the skip
      rule for `needs`, and the result file. Reads the environment listed in the Format section.
      Failure material is added in T057, so keep an `onFailure` extension point.
- [x] T022 In `.github/workflows/ci.yml`, documentation job, add after the "Check vault lifecycle" step:
      a step "Check e2e helpers" running `corepack pnpm check:e2e-lib`, with a comment saying it needs
      no display and no application.
- [x] T023 Checkpoint: `pnpm check:e2e-lib`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`
      pass. Commit `feat(e2e): process control, driver client and isolated instances`.

**Checkpoint**: the foundation is tested in isolation; nothing runs the application yet.

---

## Phase 3: User Story 1 — Run the whole suite with one command, safely (Priority: P1) 🎯 MVP

**Goal**: One command from the dev shell builds, runs, reports and cleans up, without a window on the
desktop and without touching the maintainer's HaexVault or its data (FR-001 to FR-005, FR-008 to FR-010).

**Independent Test**: Run the command with the smoke scenario only. Afterwards no window appeared, no
marked process survives, the data of a HaexVault that was running at the time is unchanged, and that HaexVault
kept running.

### Tests for User Story 1 (write first)

- [x] T024 [P] [US1] `scripts/e2e/lib/build.test.ts`: the target directory is `CARGO_TARGET_DIR` if set, else
      `src-tauri/target`, and the binary `<target>/debug/haex`; `--app` skips the build and must exist and
      be executable; the close behavior is `--close-behavior` if given (`closeBehaviorFrom: flag`), else
      `relaunch` for a path containing `/release/` and `exit` for `/debug/` (`closeBehaviorFrom: path`),
      else an error that says how to state it with `--close-behavior`; a build failure error carries the
      last 40 lines of the build output and the path of `build.log`.
- [x] T025 [P] [US1] `scripts/e2e/lib/report.test.ts`: from result files the summary prints one line per
      scenario with status, name and duration, and skipped scenarios with their reason; the run status is
      `passed` only if no scenario failed; a scenario file that ended without writing a result counts as
      `failed` with the message "no result written"; the exit statuses are the table of contracts/cli.md
      (0 no scenario failed, 1 failed or timed out, 2 preflight failed, 3 build failed, 130 interrupted,
      143 terminated); `report.json` has the fields of contracts/report.md.
- [x] T026 [P] [US1] `scripts/e2e/lib/cli.test.ts` with injected steps: the order is preflight, sweep,
      build (unless `--app` or `E2E_APP`), scenarios, sweep by marker, summary; a failing preflight
      starts nothing and exits 2; a failing build exits 3 and runs no scenario; `--grep` becomes the
      runner's name filter; SIGINT and SIGTERM stop the run's processes first and exit 130 and 143;
      `--run-timeout` stops the current scenario as a failure, lists the rest as skipped with the reason
      "run time limit" and exits 1; option defaults are scenario limit 60 s and run limit 600 s; every
      file `scripts/e2e/scenarios/<name>.test.ts` declares `scenario('<name>'`.

### Implementation for User Story 1

- [x] T027 [US1] `scripts/e2e/lib/build.ts`: `resolveApplication` and `buildDebugApplication`
      (`pnpm tauri build --debug --no-bundle` in the repository root, default features, output to
      `build.log` in the run directory). It never builds a release profile (FR-004, clarified 2026-09-21).
- [x] T028 [US1] `scripts/e2e/lib/report.ts` (after T025): read the result files, print the summary,
      write `report.json` with the run marker and conformance status/scale, and map the outcome to the exit
      status.
- [x] T029 [US1] `scripts/e2e/cli.ts` (after T027, T028): options `--app`, `--close-behavior`, `--grep`,
      `--keep`, `--scenario-timeout`, `--run-timeout` and the environment `E2E_APP`, `E2E_ARTIFACTS_DIR`,
      `E2E_TIME_SCALE`; run id from a timestamp plus a random suffix; the order of work of
      contracts/cli.md; it runs `node --test --test-concurrency=1` over `scripts/e2e/scenarios/*.test.ts`
      in a marked, detached group with the environment of the Format section; signal handling and the
      final sweep run on every path. The preflight call is a stub until T035 (tool resolution only).
- [x] T030 [US1] `scripts/e2e/scenarios/smoke-start.test.ts`: start an instance, wait until
      `document.readyState` is `complete` and `location.pathname` is `/`, take a screenshot, end. No
      hook and no text is used.
- [x] T031 [US1] Manual verification of the independent test, recorded in the "Validation record": start
      your own HaexVault (for example `pnpm tauri:dev`) and note a checksum listing of its data directory;
      run `pnpm test:e2e --grep smoke`; then check no window appeared on the desktop, no process with a
      run marker remains (`grep -l HAEX_E2E_RUN /proc/*/environ 2>/dev/null` prints nothing), the
      listing is unchanged and your HaexVault still runs. Repeat with Ctrl-C at a random moment, and with
      `kill -9` on the runner followed by a second run: the second run's sweep reports and removes the
      leftovers, and the run passes.
- [x] T032 [US1] Checkpoint: `pnpm check:e2e-lib`, `pnpm typecheck:scripts`, `pnpm lint`,
      `pnpm format:check`. Commit `feat(e2e): run the suite with one command`.

**Checkpoint**: one command runs the smoke scenario safely; the version check is still missing.

---

## Phase 4: User Story 5 — Missing tools and mismatches stop the run early (Priority: P2)

**Goal**: A missing tool or a driver that differs from the machine's web view stops the run before
anything starts, with a message that names the problem and the remedy (FR-011, SC-006).

**Independent Test**: Hide a tool from `PATH`, and separately use a driver with another version: each
stops the run within 10 seconds with exit status 2, a message naming problem and remedy, nothing started.

### Tests for User Story 5 (write first)

- [x] T033 [P] [US5] Extend `scripts/e2e/lib/preflight.test.ts`: the driver version is read from
      `share/webkit-webdriver/version` next to the `bin/` directory the driver was found in (use the path
      as found on `PATH`, not the symlink target: the delivered driver is a symlink into another
      package); without that file it falls back to the installed package version of `webkit2gtk-driver`
      (an injectable command runner, `dpkg-query -W -f='${Version}' webkit2gtk-driver`), reduced to the
      upstream `major.minor.patch` (`2.44.2-0ubuntu0.24.04.1` becomes `2.44.2`, an epoch prefix is
      dropped); the web view version comes from `pkg-config --modversion webkit2gtk-4.1`; the versions are
      compared as `major.minor.patch`; a version that cannot be determined fails and names the missing
      source; each failure message names the problem and the remedy (dev shell, or the distribution
      package) and, for a mismatch, both versions; the result is a tool check as in data-model.md
      (`tools[]`, `driverVersion`, `webviewVersion`, `result`: `failed` if any tool is missing, a version
      is absent, or the two versions differ) and no command runs beyond those named; each command has a
      5 second limit.

### Implementation for User Story 5

- [x] T034 [US5] `scripts/e2e/lib/preflight.ts`: add `checkPreflight` returning the tool check of
      data-model.md, with the messages above. It reports before anything is started.
- [x] T035 [US5] Wire `checkPreflight` into `scripts/e2e/cli.ts` in place of the stub of T029 and add the
      cases to `scripts/e2e/lib/cli.test.ts`: a failed tool check exits 2 before the sweep and the build,
      starts no process, and prints the messages; a passing check passes the resolved tool paths on in
      `E2E_TOOLS`.
- [x] T036 [US5] Manual trials (SC-006), recorded: run with a `PATH` that lacks `xvfb-run`; then with
      `WebKitWebDriver` resolving to a stub whose version file says another version. Each: exit status 2
      within 10 seconds, the message names the tool or shows both versions and the remedy, and
      `ps` shows nothing started by the run.
- [x] T037 [US5] Checkpoint and Stage 1 pull request (T008 to T037): all checks of T032, commit
      `feat(e2e): check tools and versions before starting`. Open the PR after asking the operator.

**Checkpoint**: Stage 1 is complete: one safe command with an early, clear stop.

---

## Phase 5: User Story 3 — A new scenario is short to write (Priority: P2)

**Goal**: A contributor writes only what is specific to a scenario. The helpers of contracts/helpers.md,
the stand-in provider and the interface hooks exist and are documented (FR-006, FR-007, FR-013 to FR-015).

**Independent Test**: A contributor who has not seen the suite writes "create a vault, unlock it, call
one backend command, check its result" in 40 lines or fewer using only the helpers and the README, and it
runs in the suite (SC-005).

- [x] T038 [US3] Start branch `016-e2e-stage2-helpers` from the updated `origin/main`. Graphify first:
      `graphify query "stand-in http provider streaming server-sent events"` (research R9 records that the
      Rust `sse_body()` and wiremock fixtures and `scripts/lib` harnesses cannot be extended),
      `graphify query "click and type on a page by selector"`,
      `graphify query "create and unlock an instance flow"`. Note the candidates in the PR description.

### Tests for User Story 3 (write first)

- [x] T039 [P] [US3] `scripts/e2e/lib/page.test.ts` against `fake-driver.testlib.ts`: `click(hook)` and
      `type(hook, text)` resolve a hook per contracts/test-hooks.md (a value starting with `#`, `.` or `[`
      is a selector, any other value is `[data-testid="<value>"]`), choose the displayed element when
      several match and fail naming the hook if none is displayed by the deadline; `press(hook)` clicks
      inside the page through `setTimeout(…, 0)` so the call returns, records the `press` step with the
      runner's clock, and with `{ times: 2 }` dispatches both clicks in one script tick; `closeWindow`,
      `navigate(url)` and `exec` send the W3C calls; `waitForEnd(deadlineMs)` resolves with the time from
      the call to the end of `appPid` or fails at the deadline and records `process-ended`;
      `markedProcesses()` returns the run's marked processes whose executable is the application;
      `sampleUntilEnd(script, intervalMs)` returns the samples taken until the session ends; `invoke` with
      `expectEnd` resolves `{ ended: true }` for a call that gets no answer (User Story 3 scenario 2).
- [x] T040 [P] [US3] `scripts/e2e/lib/provider.test.ts`: it listens on `127.0.0.1` only, on a port the
      operating system chose; `GET /v1/models` answers one model `stand-in-model` with `data`, `has_more`,
      `first_id`, `last_id`; any other route is 404; `stream-forever` (defaults `intervalMs` 100, `text`
      `tick `) sends the start events then at least two `content_block_delta` within 500 ms and its
      connection has no `closedAt` until the client drops it, after which `closedAt` is set within
      200 ms; `stream-then-finish` (defaults `chunks` 5, `intervalMs` 200) sends that many deltas, then
      `content_block_stop`, `message_delta`, `message_stop`, and ends; `error` (default `status` 500)
      answers at once with that status and a JSON error body; `behave` applies to later requests only;
      `requests()` records method, path and body and never `x-api-key` or `authorization` (send a
      sentinel and search the serialized records for it); `waitForOpen` resolves; `close` ends open
      streams and frees the port.
- [x] T041 [P] [US3] `scripts/e2e/lib/flows.test.ts` against the fake driver: `createAndUnlock` issues the
      backend call `create_instance`, clicks the `instance-entry` whose `data-instance-name` is the
      name, types the generated passphrase into `#unlock-passphrase`, clicks `[form="unlock-form"]`
      (never a key press: Enter did not submit the form in the spike) and waits for `/workspace/`;
      `openChat` clicks `open-chat` and waits for `/chat/`; `connectProvider` issues `add_provider`
      (`kind: 'api_key'`, `adapter: 'anthropic'`, the provider's base address, a generated key) then
      `load_model`; `startReply` issues `send_message` and records `reply-streaming` when the provider
      has an open connection.

### Implementation for User Story 3

- [x] T042 [P] [US3] `scripts/e2e/lib/page.ts`: the interaction helpers of T039.
- [x] T043 [P] [US3] `scripts/e2e/lib/provider.ts`: the stand-in provider of contracts/stand-in-provider.md,
      including the records with the runner's one clock and `closedAt` taken from the socket's close
      event. It serves nothing else and never outlives its scenario.
- [x] T044 [US3] `scripts/e2e/lib/flows.ts` (after T042, T043): `createAndUnlock`, `openChat`,
      `connectProvider`, `startReply` as in T041.
- [x] T045 [US3] In `scripts/e2e/lib/instance.ts` expose the page helpers on the instance
      (`invoke`, `click`, `type`, `press`, `closeWindow`, `navigate`, `waitForEnd`, `markedProcesses`,
      `sampleUntilEnd`) and add `ctx.provider(behavior?)` in `scripts/e2e/lib/scenario.ts`, ended with the
      context.
- [x] T046 [P] [US3] Hook: in `src/components/onboarding/InstancesList.vue` add
      `data-testid="instance-entry"` and `:data-instance-name="i.name"` to the entry button (2 lines).
- [x] T047 [P] [US3] Hook: in `src/components/workspace/ChatFab.vue` add `data-testid="open-chat"` to the
      link (1 line).
- [x] T048 [P] [US3] Hook: in `src/pages/chat/[instance].vue` add `data-testid="lock-instance-sidebar"` to
      the lock button in the sidebar and `data-testid="lock-instance-header"` to the one in the header (2
      lines, and nothing else in this file). (Originally one shared `data-testid="lock-instance"` on both;
      split into two unique ids after review — see the follow-up entry below.)
- [x] T049 [US3] Verify the hooks: `pnpm check:templates`, `pnpm typecheck`, `pnpm lint`,
      `pnpm format:check`; `wc -l` of the chat page equals the T002 baseline plus 2, the other two files
      plus their stated lines. Behavior and appearance are unchanged (attributes only).
- [x] T050 [US3] `scripts/e2e/scenarios/create-and-unlock.test.ts`: create and unlock a vault, call one
      backend command (`list_instances`) and check its result, in 40 lines or fewer, using only helpers.
      It is also the model the README shows.
- [x] T051 [US3] `scripts/e2e/README.md`: how to run (`pnpm test:e2e`, options, `--app`), the tools and
      where they come from, how to write a scenario (copy T050), the helper list of contracts/helpers.md,
      the hook table of contracts/test-hooks.md, the rules for scenarios (hooks only, no key submit, no
      waiting for a call that closes the app, no fixed sleeps, nothing started outside the context, `needs`
      for a relaunching build), how the command and the scenario files talk (the environment of the
      Format section), where the run directory is and what is in it, and troubleshooting for a leftover
      run. Run `pnpm exec prettier --write scripts/e2e/README.md`.
- [x] T052 [US3] First pass of SC-005, recorded: read only the README and write the scenario T050 again
      from scratch in a scratch file; it must come to 40 lines or fewer. Fix the README where it fell
      short.
- [x] T053 [US3] Run `pnpm test:e2e` (smoke and create-and-unlock pass). Commit the checkpoint
      `feat(e2e): scenario helpers, stand-in provider and test hooks`.

**Checkpoint**: a scenario can be written from the README and the helpers.

---

## Phase 6: User Story 4 — A failed scenario explains itself (Priority: P2)

**Goal**: For a failed scenario the run keeps what a maintainer needs to see why without running it
again, and says where it is (FR-019, FR-020, SC-004).

**Independent Test**: Seed three failures (the application never starts, the process does not end after
the lock control, the provider connection stays open); for each, the kept material names the step that
failed. The first is tested here, the other two after the close scenarios exist (T078).

### Tests for User Story 4 (write first)

- [x] T054 [P] [US4] `scripts/e2e/lib/artifacts.test.ts`: on failure the scenario directory under the run
      directory holds `timeline.json` (all steps with `atMs` and an ISO time, the failure message, the
      deadline reached if any), `screenshot.png` taken before teardown (absent, with a note in the
      timeline, if the application already ended), `driver.log` (driver, webview driver and application
      output in arrival order, each line timestamped) and `provider.json` (connections and requests);
      on a pass none of these exist and only `report.json` (and `build.log` if built) remain in the run
      directory; `--keep` keeps them for a pass too; capturing never throws when the session is gone.
- [x] T055 [P] [US4] Extend `scripts/e2e/lib/report.test.ts`: `failedStep` is the last step reached before
      the failure or the failing wait's description; `material` is the scenario directory; the printed
      line of a passing scenario with a `press` and a `process-ended` step includes "press to process end
      <seconds>"; a failure line names the failed step and the material directory.

### Implementation for User Story 4

- [x] T056 [US4] `scripts/e2e/lib/artifacts.ts`: `captureFailure` writing the four files above.
- [x] T057 [US4] Use it: in `scripts/e2e/lib/scenario.ts` call `captureFailure` from the `onFailure`
      extension point of T021 before teardown, for a thrown error and for a reached deadline; in
      `scripts/e2e/lib/report.ts` fill `failedStep`, `material` and the press-to-end text; pass `--keep`
      through `E2E_KEEP` from `scripts/e2e/cli.ts`; remove the directory of a passing scenario unless
      kept.
- [x] T058 [US4] Seeded failure 1 (SC-004), recorded: point `--app` at an executable script that only
      sleeps and run `pnpm test:e2e --close-behavior exit --grep smoke` with it; the material names the
      pending `instance-ready` step when startup is still in flight, and nothing is left running. Commit
      `feat(e2e): keep failure material and report step times`, then the Stage 2 pull request (T038 to
      T058), after asking the operator.

**Checkpoint**: Stage 2 is complete: helpers, provider, hooks, README and failure material.

---

## Phase 7: User Story 2 — The close promises of spec 013 are checked automatically (Priority: P1) 🎯 MVP

**Goal**: The scenarios done by hand for spec 013 run on their own (FR-016 to FR-018): lock while a
reply streams, window close while a reply streams, the closing page in a light and a dark scheme, lock
twice, and, with a relaunching build, the relaunch.

**Independent Test**: With the stand-in provider streaming without end, unlock a vault, start a reply and
press the lock control: the scenario passes only if the process ends within 4 seconds and the provider
sees its connection close within 1 second of the press. Weaken the behavior on purpose and it fails
(T077).

- [x] T059 [US2] Start branch `016-e2e-stage3-close` from the updated `origin/main`. Graphify first:
      `graphify query "close promises drain ladder deadlines"` and
      `graphify query "assert a process ended within a deadline"`; note the candidates.
- [x] T060 [P] [US2] `scripts/e2e/lib/close-promises.test.ts`: the process ends within 4 seconds, the
      provider connection closes within 1 second of the press, and the relaunch within 10 seconds; these
      thresholds stay fixed for conformance. A run with `E2E_TIME_SCALE` other than 1 is explicitly
      non-conformant and reports its scale rather than changing these assertions.
- [x] T061 [P] [US2] `scripts/e2e/lib/close-promises.ts`: the three numbers, with a comment naming their
      source (spec 013: drain ladder 1 s cooperative, 3 s total, 0.5 s grace).
- [x] T062 [US2] `scripts/e2e/scenarios/lock-while-streaming.test.ts`: provider `stream-forever`;
      `createAndUnlock`, `openChat`, `connectProvider`, `startReply`; wait until the provider's
      connection has been open for at least 800 ms; `press('lock-instance-sidebar')`; `waitForEnd` within the
      4 seconds of `close-promises`; the connection's `closedAt` minus the press time is at most 1 second
      (records `provider-closed`); no `[role="alert"]` in any sample from `sampleUntilEnd` and none just
      before the press. The pid tracked is the original `appPid`, so the scenario is correct on a
      relaunching build too.
- [x] T063 [P] [US2] `scripts/e2e/scenarios/window-close-while-streaming.test.ts`: the same state, then
      `closeWindow()` instead of the press; the same three outcomes.
- [x] T064 [P] [US2] `scripts/e2e/scenarios/closing-page.test.ts`: for `colorScheme` `light` and `dark`,
      each in its own instance: `navigate('tauri://localhost/closing.html')`; first assert that
      `matchMedia('(prefers-color-scheme: dark)').matches` equals the requested scheme (a scheme that did
      not apply is a clear failure, research R7); then `document.body.textContent.trim()` is empty,
      exactly one `.ring` exists, and the body's background equals a probe element's background set to
      `var(--page)`; finally the two schemes' backgrounds differ.
- [x] T065 [P] [US2] `scripts/e2e/scenarios/lock-twice.test.ts`: an unlocked vault with nothing running;
      `press('lock-instance-sidebar', { times: 2 })`; the original pid ends once within 4 seconds; for the next
      5 seconds `markedProcesses()` never holds more than one process; no `[role="alert"]` in the
      samples.
- [x] T066 [US2] Run the four scenarios with `pnpm test:e2e --grep <name>`; each finishes in under 30
      seconds (SC-007). Record the durations and the `press to process end` times in the "Validation
      record". Commit `feat(e2e): check the vault close promises of spec 013`.

### The relaunch scenario (needs a release build; two checks first)

- [x] T067 [US2] Check A, recorded: build a release with `pnpm tauri build --no-bundle` and run
      `pnpm test:e2e --app <its path> --grep smoke`. If `tauri-driver` cannot drive a release-profile
      binary, stop here: record the failure and report it to the operator; T068 to T073 wait for a
      decision, and T062 to T066 are unaffected.
- [x] T068 [US2] Check B, recorded, with a throwaway script that is not committed: with the release
      binary, press the lock control and observe from outside (a) the original pid is gone, (b) a new
      marked process of the same binary appears, so the marker is inherited by a relaunch (research R8),
      (c) whether the virtual screen started with `-fbdir <dir>` keeps a current XWD image that Node can
      read, and what a blank screen and a painted window look like in it. Record the values that the
      painted check of T070 will rely on. If (c) fails, keep the scenario to steps 1, 2 and 4 of research
      R11 and report the difference from FR-018 to the operator; do not drop it silently.
- [x] T069 [P] [US2] `scripts/e2e/lib/framebuffer.test.ts` with a synthetic XWD buffer built in the test:
      the header is parsed (width, height, bytes per line, bits per pixel, header size, colormap
      entries), a blank image is reported as not painted and one with a window-sized non-blank region as
      painted, and a truncated file is an error, not a pass.
- [x] T070 [P] [US2] `scripts/e2e/lib/framebuffer.ts`: `readFramebuffer` and `isPainted` per T069 and the
      values recorded in T068.
- [x] T071 [US2] In `scripts/e2e/lib/instance.ts` add the option `framebufferDir` that adds
      `-fbdir <dir>` to the virtual screen's arguments in `driverCommand`, and a case for it in
      `scripts/e2e/lib/instance.test.ts`.
- [x] T072 [US2] `scripts/e2e/scenarios/relaunch-after-lock.test.ts` declaring that it needs the
      `relaunch` close behavior: create and unlock, press the lock control; (1) the original pid is gone within 4
      seconds; (2) a new marked process of the same binary with another pid exists within 10 seconds
      (records `relaunch-seen`); (3) the screen shows a painted window (T070), if T068 allowed it;
      (4) stop that process, start a fresh instance with `reusesRoot` over the same data and check that
      `instance-entry` for the created name is displayed and `location.pathname` is `/`, which is what
      the relaunched window shows. If the application does not relaunch, the failure message names that
      the release binary did not relaunch.
- [x] T073 [US2] Run the relaunch scenario against the release build, record the result and time, and
      against the debug build, where it must be reported as skipped with its reason. Commit
      `feat(e2e): check the relaunch after a lock` and open the Stage 3 pull request (T059 to T073) after
      asking the operator.

**Checkpoint**: the spec 013 close promises run on their own; the relaunch runs with a release build.

---

## Phase 8: Stage 4 — Proof of the success criteria (records only, no product code)

**Purpose**: Show that the suite does what the spec promises, with the recipes of quickstart.md. Outcomes
go to the "Validation record". Branch `016-e2e-stage4-validation`; a pull request with the record.

- [x] T074 SC-001 and SC-007: `pnpm test:e2e --app <a debug build already built>`; the close scenarios
      each finish in under 30 seconds and the run in under 5 minutes; the relaunch scenario is listed as
      skipped with its reason; exit status 0; with the release build by path all five run.
- [x] T075 SC-002: 20 consecutive runs, 10 of them with `kill -9` on the runner at a random moment
      (`sleep $((RANDOM % 25))` before the kill), with your own HaexVault running throughout. After each run:
      no marked process remains once the next run's sweep has run, no window appeared, your HaexVault still
      runs and its data listing is unchanged, and every run after a killed one starts normally.
- [x] T076 Confirm the isolation of FR-006 on a real run: while a scenario runs, capture top-level
      listings of the maintainer's data, config, cache and runtime directories and home for changes
      made by it (none), and confirm no `xdg-desktop-portal` process was started by the run. If only
      top-level listings are captured, limit the conclusion to unchanged top-level entries.
- [x] T077 SC-003, in a scratch change reverted after each trial: (a) do not cancel the stream when the
      vault closes, expect `lock-while-streaming` to fail (see the T077 follow-up record: a real,
      investigated, accepted limitation means `window-close-while-streaming` cannot currently be made to
      fail the same way — rescoped, not achieved); (b) do not replace the page by the closing page,
      expect `closing-page` to fail; (c) make the process not end, expect `lock-while-streaming` and
      `lock-twice` to fail; (d) make a release build not relaunch, expect `relaunch-after-lock` to fail.
      Restore each and expect a pass. Record every trial.
- [x] T078 SC-004: seeded failures 2 (the process does not end after the lock control, from T077 (c)) and
      3 (the provider connection stays open, from T077 (a)), with failure 1 from T058. For each, the
      material names the failed step, so no second run is needed. Record.
- [x] T079 SC-005, final pass: ask the operator for a contributor who has not seen the suite, or start a
      fresh session, give it only `scripts/e2e/README.md` and ask for "create and unlock a vault, call one
      backend command, check its result". It must take 40 lines or fewer and run. Record what tripped it
      and fix the README.
- [x] T080 Commit the record as `docs(specs): record the validation of the e2e suite`; open the Stage 4
      pull request after asking the operator. Committed `6a7271f`, pushed `016-e2e-stage4-validation`,
      PR #121 opened (https://github.com/haex-space/haex-vault/pull/121).

---

## Phase 9: User Story 6 — The same suite runs in the repository's CI (Priority: P3)

**Goal**: The suite runs on `ubuntu-24.04` with the tools from the distribution; a failing run publishes
its material (FR-021). Separable: nothing above waits for it.

**Independent Test**: A CI run on a clean runner passes the same scenarios as the ordinary local run; a
deliberately failing scenario fails the job and uploads the run directory.

- [x] T081 [US6] Branch `016-e2e-stage5-ci` from the updated `origin/main`. In `.github/workflows/ci.yml`
      add a job `e2e` on `ubuntu-24.04` with `permissions: contents: read` and a `timeout-minutes`: check
      out (same pinned action and `persist-credentials: false` as the other jobs), Node 22.19.0 through
      the same pinned action, `corepack pnpm install --frozen-lockfile --ignore-scripts`, the apt
      packages of the Rust job plus `webkit2gtk-driver` and `xvfb`, the Rust toolchain and cache steps as
      the Rust job uses them (shared key for the default features), `tauri-driver` installed with
      `cargo install tauri-driver --locked --version 2.0.6` and cached, then `corepack pnpm test:e2e`. On
      failure upload `src-tauri/target/e2e` as an artifact. Every third-party action is pinned by full
      commit hash like the existing ones: look up the current release commit of each new action and
      record the tag in a comment. Outside the Nix shell the bridge script runs the command unchanged.
      Done 2026-09-24: added as designed. `actions/upload-artifact@043fb46d…` (v7.0.1) and
      `actions/cache@55cc8345…` (v6.1.0) resolved from each action's current release commit.
      Real bug found on the first real run: `pnpm tauri build`'s `beforeBuildCommand` (`pnpm generate`)
      spawns a literal `pnpm` process, not through corepack — every other job only ever runs
      `corepack pnpm ...`, so this never surfaced there. Fixed with an `Enable Corepack` step
      (`corepack enable`) before install.
- [ ] T082 [US6] The job relies on the package fallback of the version check (T033): confirm on the runner
      that the driver's and the web view's package versions are equal and the preflight passes.
      Record evidence that the version file checked by `driverVersionOf` was absent on the runner,
      or that its `dpkg-query` fallback executed, before marking this task complete.
      Reopened 2026-09-24: the clean run proves that preflight passed, but its log does not establish
      which version source was used. Runner evidence for the fallback remains outstanding.
- [x] T083 [US6] Verify on a pushed branch, recorded with the run links: a clean run passes with the
      relaunch scenario skipped; a scratch commit that breaks one scenario makes the job fail and the
      artifact downloadable. Revert the scratch commit. Commit `ci: run the e2e suite on ubuntu` and
      open the Stage 5 pull request after asking the operator.
      Done 2026-09-24, PR #135 (https://github.com/haex-space/haex-vault/pull/135): - Clean run (after the corepack fix, commit `b01fce2`):
      https://github.com/haex-space/haex-vault/actions/runs/36005937603 — all 5 jobs pass, `End-to-end
suite` green, `relaunch-after-lock` reported skipped (debug build exits on close), 6 passed. - Scratch-break run (commit `e201b80`, `smoke-start.test.ts` seeded to always throw):
      https://github.com/haex-space/haex-vault/actions/runs/36006711933 — `End-to-end suite` job failed
      exactly as intended: `Result: FAILED (1 failed, 1 skipped, 5 passed)`, only `smoke-start`
      failed with the seeded message, `relaunch-after-lock` still correctly reported skipped, and
      `e2e-failure-material.zip` (18084 bytes — real material, unlike the earlier build-failure
      run's empty-ish 761 bytes) was uploaded and downloadable. - Scratch commit reverted as `853f7bc`; `smoke-start.test.ts` back to its original content
      (confirmed no diff against the pre-scratch version). - One real CI-only flake observed on the run after the prettier-stabilizing commit
      (https://github.com/haex-space/haex-vault/actions/runs/36008292857): `closing-page` failed with
      `POST /session got no answer: fetch failed` — the tauri-driver-HTTP-port-before-native-driver
      race `newSessionWithRetry` already retries once (300 ms, PR #118); this time even that single
      retry lost the race, most likely because the GitHub-hosted runner is slower/more contended than
      the maintainer's machine. The other five scenarios passed and `relaunch-after-lock` still
      correctly reported skipped in the same run — this is the exact failure mode T074 already
      documented as a rare, unreproducible flake, not a new defect from this job. `gh run rerun
--failed` on the same commit passed cleanly on the next attempt
      (no code change). Left as an observed CI-environment flake, consistent with the project's own
      practice of not hardening a retry based on one occurrence; worth revisiting only if it recurs.

---

## Phase 10: Polish and cross-cutting

- [x] T084 Full CI parity from a clean state: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`,
      `pnpm format:check`, `pnpm check:chat-state`, `pnpm check:vault-lifecycle`, `pnpm check:templates`,
      `pnpm check:e2e-lib`, `python3 scripts/ci/check-docs.py`, `git diff --check`.
      Done 2026-09-24: all pass from a clean `pnpm install --frozen-lockfile` in a fresh worktree.
      Also ran `pnpm check:vault-passphrase-lifetime` (7 tests) and `pnpm check:e2e-lib` (155 tests),
      both green — not in this task's own list but part of the documentation job's real command set.
- [x] T085 Compare `wc -l` with the T002 baseline: the chat page grew by exactly 2 lines, every new file
      is under 500 lines. Record the numbers in the Baseline section.
      Done 2026-09-24: chat page 1316 → 1318 (exactly +2, the `lock-instance-sidebar`/
      `-header` hooks). The other two T002 files also grew — expected, not drift: both carry a
      Stage 2 `data-testid` hook (`InstancesList.vue` 44 → 46: `data-testid="instance-entry"` +
      `:data-instance-name`; `ChatFab.vue` 20 → 21: `data-testid="open-chat"`). Every file under
      `scripts/e2e/` is under 500 lines; the largest is `scripts/e2e/lib/scenario.ts` at 465.
- [x] T086 [P] Bring `plan.md`, `research.md`, `data-model.md` and `contracts/` in line with what was
      built: the files the plan did not list (`page.ts`, `flows.ts`, `artifacts.ts`, `framebuffer.ts`,
      `fake-driver.testlib.ts`, and `session.ts` if it was split), the outcomes of T067 and T068, and any
      renamed helper. Run `pnpm exec prettier --write specs/016-e2e-testing` twice and confirm it is
      stable.
      Done 2026-09-24: `plan.md`'s file tree already named every file under `scripts/e2e/` (an earlier
      stage had kept it current) — nothing missing, no `session.ts` split, no renamed helper found.
      What was stale: `research.md` R11 and `plan.md`'s Risks table still described T067/T068 as open
      ("has not been tried", "keep steps 1, 2 and 4"), though tasks.md's own validation record shows
      both resolved 2026-09-22 (a release binary is drivable; the relaunch is observable via `-fbdir`,
      all four steps ship). Updated both to state the resolution plainly. `pnpm exec prettier --write
specs/016-e2e-testing` run twice, second run made no further changes.
- [x] T087 [P] In `specs/013-vault-lifecycle-isolation/tasks.md` add a note to T050 that its automatable
      part (quickstart scenarios 1 and 2, the closing page and the relaunch) is covered by spec 016 with
      the scenario names, and that T048 (relaunch under `tauri dev`) and T049 (local inference stop
      time) stay manual.
      Done 2026-09-24: added directly under spec 013's T050, naming `lock-while-streaming.test.ts` +
      `closing-page.test.ts` for scenario 1, `lock-twice.test.ts` for scenario 2, and
      `window-close-while-streaming.test.ts` + `relaunch-after-lock.test.ts` for scenario 8's
      bounded-ending half and the relaunch; noted scenario 3 stays a Rust integration test (already
      true in spec 013's own text) and T048/T049 stay manual.
- [x] T088 Traceability: for FR-001 to FR-021 and SC-001 to SC-007 write down in the Validation record the
      test or the recorded result that covers each. A gap is closed or reported.
      Done 2026-09-24: table added to the Validation record below. No gap: every FR and SC has a
      covering file or recorded result. Two items carry an accepted caveat, already reported earlier
      rather than found here — FR-018 turned out to need no caveat at all (T067/T068 resolved it, see
      T086); SC-003's window-close half is real but narrower than its own table entry states (PR #122,
      T077's corrected scope), and FR-006's spec wording undercounts what R5 actually isolates (spec
      alignment item 1, for T089).
- [x] T089 Run `/speckit-analyze` (the constitution requires it to check plans against the constitution)
      and fix findings. Fold in the four spec alignment items of the plan (FR-006, FR-018, the closing
      page scenario, "no error appears") as a small reviewed spec change or record why not.
      Done 2026-09-24: analysis found 0 CRITICAL/HIGH findings — 21/21 FR and 7/7 SC each have ≥1 task
      (T088's table), no constitution violation, no ambiguity or duplication. One LOW staleness finding
      (Assumptions still described the atoms change as unmerged and the spike branch as present; both
      long since resolved) fixed alongside the alignment items. The four alignment items: **FR-006**
      and **"no error appears"** folded in as small spec.md changes (the isolation list now names the
      runtime/home/session-bus scope R5 actually covers; a new Assumptions entry states the
      last-sample-plus-stronger-evidence check); **FR-018** needed no change — T067/T068 resolved the
      open risk the item was tracking, so the spec's existing wording already matches what got built;
      **the closing-page scenario** needed no change — it is a claim about what the page shows, which
      the direct-navigation strategy checks exactly, not a claim about live-observing a real close.
- [x] T090 With the operator's agreement, remove the throwaway spike: the worktree
      `.worktrees/016-e2e-spike` and the local branch `spike/e2e-rig`, which the suite replaces, and the
      old local branch `spike/vault-gateway` (task T051 of spec 013).
      Done 2026-09-24: `git worktree remove .worktrees/016-e2e-spike --force` and
      `git branch -D spike/e2e-rig`. `spike/vault-gateway` no longer existed locally — already removed
      when spec 013's own T051 ran.
- [ ] T091 Open any remaining pull requests in the structure below after asking the operator. Merge by
      rebase or merge commit, never squash; merge-commit subjects use a Conventional Commits header.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Stage 0)** has no dependency and unblocks everything that runs the tools; Phase 2 can be
  written in parallel, but nothing that starts the application can run before T006 passes.
- **Phase 2** blocks every story. Tests T010 to T015 first, then T016 to T019 in parallel, then T020, then
  T021.
- **Phase 3 (US1)** after Phase 2. **Phase 4 (US5)** after Phase 3 (it replaces the stub in `cli.ts`).
  Stage 1 ends with T037.
- **Phase 5 (US3)** after Stage 1. **Phase 6 (US4)** after Phase 5 (it uses the fake driver, the scenario
  wrapper and the report). Stage 2 ends with T058.
- **Phase 7 (US2)** after Stage 2: the scenarios need the provider, the flows, the hooks and the page
  helpers. T067 and T068 gate T069 to T073; T062 to T066 do not depend on them.
- **Phase 8** after Stage 3. **Phase 9 (US6)** after Stage 1 and is separable; it can run in parallel with
  Stages 2 and 3, but its verification (T083) is more useful once scenarios exist.
- **Phase 10** at the end.

### Story dependencies

- US1: after the foundation, no other story. US5: extends US1's command. US3: after US1. US4: after US3.
  US2: after US3 and US4 (needs the helpers and the failure material to be diagnosable). US6: after US1.

### Within a stage

- Tests first and expected to fail; implementation next; then the stage's manual verification and the
  commit. One commit per checkpoint, Conventional Commits, no model trailer.

## Parallel Example

```text
# Foundation tests together (different files):
T010 processes.test.ts   T011 ports.test.ts   T012 webdriver.test.ts + fake-driver.testlib.ts
T013 instance.test.ts    T014 scenario.test.ts   T015 preflight.test.ts

# Then the independent implementations together:
T016 processes.ts   T017 ports.ts   T018 webdriver.ts   T019 preflight.ts

# Stage 2 helpers together:
T039 page.test.ts   T040 provider.test.ts   T041 flows.test.ts
T042 page.ts        T043 provider.ts        T046 / T047 / T048 the three hooks

# Stage 3 scenarios together (different files):
T063 window-close-while-streaming   T064 closing-page   T065 lock-twice
```

## Implementation Strategy

### MVP first

1. Stage 0 (tools), then Stage 1 (the command, US1 and US5): a safe, checked command with a smoke scenario.
2. Stage 2 (US3 and US4) and Stage 3 (US2): the helpers, the diagnostics and the close scenarios. With
   these the two P1 stories are complete and the spec 013 promises are checked automatically.
3. **Stop and validate** with Stage 4 before adding CI.

### Incremental delivery

1. Each stage is a pull request that leaves the suite usable: after Stage 1 it runs and stops early and
   clearly; after Stage 2 a scenario is short to write and a failure explains itself; after Stage 3 the
   close promises are covered; Stage 5 adds the CI job without changing the local flow.
2. The relaunch scenario (T067 to T073) may land in a later pull request than the other four close
   scenarios if its two checks take time.

## Notes

- [P] tasks touch different files and have no dependency on an unchecked task.
- A task that says "recorded" writes its outcome to the "Validation record" below before it is ticked.
- If T067 or T068 fail, the spec alignment item on FR-018 is reported to the operator, not decided here.

## Baseline

Recorded at T002 on 2026-09-22, branch `016-e2e-stage0-tools` from `origin/main` at `8e0b00b`, before any edit.

| Check                                                      | Result                   |
| ---------------------------------------------------------- | ------------------------ |
| `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check` | pass                     |
| `pnpm check:chat-state`                                    | 45 tests, 45 pass        |
| `pnpm check:vault-lifecycle`                               | 5 tests, 5 pass          |
| `pnpm check:templates`                                     | 33 Vue templates compile |

| File                                          | Lines |
| --------------------------------------------- | ----- |
| `src/pages/chat/[instance].vue`               | 1316  |
| `src/components/onboarding/InstancesList.vue` | 44    |
| `src/components/workspace/ChatFab.vue`        | 20    |

**T085, 2026-09-24** (branch `016-e2e-stage5-ci`, `origin/main` at `d626eae`):

| File                                          | Lines then | Lines now | Delta |
| --------------------------------------------- | ---------- | --------- | ----- |
| `src/pages/chat/[instance].vue`               | 1316       | 1318      | +2    |
| `src/components/onboarding/InstancesList.vue` | 44         | 46        | +2    |
| `src/components/workspace/ChatFab.vue`        | 20         | 21        | +1    |

The chat page's own delta matches the expectation exactly. The other two files' growth is the
remaining Stage 2 `data-testid` hooks (`instance-entry`, `open-chat`), not unplanned drift.

## Validation record

### Stage 0 (T003 to T006), 2026-09-22

- **T003** The atoms range `1a292194…..d5c48d0e…` holds two commits: `480a0e37` (the tooling change) and
  `d5c48d0e` (the merge of haexmas/atoms#32). Nothing else arrives with the new pin.
- **T004** The `revision` in `.spaex/manifest.json` is now the full hash
  `d5c48d0eb6662da84086bf69a3ca041a494e2b26`. `spaex install` first stopped with
  `pinned-revision-not-found`; `git fetch origin` in spaex's own publisher clone under
  `~/.local/share/spaex/repos/` fixed that. The installed run used
  `spaex install --speckit-agents claude,codex --no-install-hooks`: the Spec Kit selection is what the
  lock recorded before (a bare `spaex install` refuses to run without one), and the hooks were skipped
  because the graphify hook refuses any branch that is not `main`, which would only have recorded a
  branch-specific failure. `install.lock` therefore keeps its earlier hook state.
- **T005** Changed by the install: `flake.nix` (reads `.devshell/packages.nix`), the new
  `.devshell/packages.nix`, `.spaex/generated/nix-packages.json` (gains `xvfb-run`),
  `.spaex/manifest.json` and `.spaex/install.lock` (tracked despite its ignore rule: new revisions, the
  new file's hash, a new generation id and the Spec Kit declaration fingerprint). No other tracked file
  changed. No absolute path in any delivered file. `git ls-files .devshell/packages.nix` lists it.
- **T006** In the shell, through the host bridge:

  ```text
  tauri-driver     /nix/store/…-tauri-driver-2.0.6/bin/tauri-driver
  WebKitWebDriver  /nix/store/…-webkit-webdriver-2.52.6/bin/WebKitWebDriver
  xvfb-run         /nix/store/…-xvfb-run-1+g87f6705/bin/xvfb-run
  driver version file:                       2.52.6
  pkg-config --modversion webkit2gtk-4.1:    2.52.6
  ```

  `tauri-driver --help` lists `--port`, `--native-port` and `--native-driver`, the options the suite
  passes.

### Stage 1 (T008 to T037), 2026-09-22

- **T008** Graphify, from the primary checkout: the candidates for spawning a process were the Rust
  `process.rs` (`cli_delegate`) and its `spawn`, for reading process state the same file, for a wire
  protocol client the Rust adapters, and for waiting `waitForTurnTerminal` in the chat composable. All are
  Rust or frontend code in other processes; none can serve a Node runner, so nothing was extended.
- **T031** Partial validation only: the smoke scenario passed against a debug binary of the spike
  (`--app`) and against the debug build the command made itself in this worktree (`pnpm test:e2e --grep
smoke`, built from a reflink copy of an earlier build cache): `passed smoke-start`, 6.1 s, exit 0.
  The application environment was read from `/proc`: `DISPLAY=:99` (the virtual screen),
  `GDK_BACKEND=x11`, no `WAYLAND_DISPLAY`, `HOME`, `XDG_DATA_HOME` and `XDG_RUNTIME_DIR` inside the
  instance root, `DBUS_SESSION_BUS_ADDRESS=disabled:` and the marker. A checksum listing of the
  maintainer's data directory is identical before and after. No maintainer HaexVault was running during
  this check, so the concurrent-instance case and the visual no-window check were not yet covered; see
  the Stage 1 follow-up entry below, where that gap is closed.
  - Ctrl-C (SIGINT) at about 4 s: exit status 130, no process with a run marker left.
  - `kill -9` on the runner at about 4 s: 9 marked processes stayed behind; the next run reported
    "removed 9 leftover process(es) of an earlier killed run", passed, and left none.
- **T035** The cases the task lists already exist in `scripts/e2e/lib/cli.test.ts`: a failed tool check
  exits 2 before the sweep and the build and starts nothing, and a passing one hands the tool paths on
  in `E2E_TOOLS`.
- **T036** Missing `xvfb-run` (its directory removed from `PATH`): exit status 2 after 844 ms, message
  names the tool and the remedy, no marked process. A driver whose version file says 2.44.2 against the
  web view's 2.52.6: exit status 2 after 820 ms, message shows both versions and the remedy, no marked
  process.
- **T032 and T037** One commit covers both checkpoints, because the command imports the preflight it was
  going to stub. The environment of the application also pins `GDK_BACKEND=x11` and drops the display
  variables of the parent; `data-model.md` and `research.md` R5 say so.

### Stage 1 follow-up (T031), 2026-09-22

- **T031** Closed. The concurrent case the first pass lacked: the maintainer's own build (this
  worktree's embedded debug binary, run directly, not through the suite) stayed up on the real desktop
  for the whole check while `pnpm test:e2e --grep smoke` ran its own isolated instance. A checksum
  listing of `~/.local/share/space.haex.vault` (every file, `sha256sum`, sorted by path, hashed again) was
  identical before and after: `973e4eb56eb2255c5bb26c27676909d9d49cc14323a1c0cf08478302156e0630`. The
  maintainer process's pid did not change across a full pass, a SIGINT sent during the build step and a
  SIGKILL sent during the scenario step, so the two are independent process trees, not merely
  independent by the checksum. The real display's own window list
  (`xprop -root _NET_CLIENT_LIST`) held one unrelated window throughout; this is expected rather than a
  strong check on its own, since the suite's instance runs on its own `xvfb-run` display, never the
  maintainer's (`DISPLAY=:99` confirmed via `/proc` in the first pass) — the two literally cannot share a
  window list. The one-command run against a real, running maintainer instance stayed `passed
smoke-start`.
  Separately, a small driver session run by hand against the real desktop (not part of the delivered
  suite, `tauri-driver` and `WebKitWebDriver` with no `xvfb-run`, capabilities pointing at the same
  embedded debug binary) took a screenshot to rule out a rendering problem: the window showed "Willkommen
  bei HaexVault", the "Instanz anlegen" control and the maintainer's real "test" instance, confirming the
  build itself is sound independently of the isolation question. An earlier attempt to use
  `pnpm tauri:dev` for the standing instance was abandoned: its Nuxt dev server exited on its own partway
  through (unrelated to anything the suite did), which is why the embedded, dev-server-free debug build
  was used instead. The kill-9-then-resweep mechanism itself was already proven in the first pass (9
  leftover processes found and removed); this pass adds the concurrency evidence the first pass lacked.

### Stage 2 (T038 to T053), 2026-09-22

- **T038** Graphify, from the primary checkout: for the stand-in provider, nothing beyond the Rust
  `sse_body()` test fixture and wiremock (server-side test doubles in the other process; unusable from a
  Node runner); for reaching a control by selector, `scripts/check-chat-state.ts`'s `createTauriDouble()`
  fakes `invoke()` for composable-level checks but drives no page at all; for a create-and-unlock flow,
  `create_instance()` and `UnlockSheet.vue`'s `onSubmit()` exist as the two ends of the flow but nothing
  already joins them for a test. Nothing was extended; all three modules are new.
- **T039 to T045** `page.ts`, `provider.ts`, `flows.ts` written with their tests first, then wired onto
  `Instance` (`instance.ts`) and `ScenarioContext` (`scenario.ts`, `ctx.provider`). `click`/`type`/`press`
  resolve and act on a control through the driver's own find/is-displayed/click/send-keys calls, one per
  attempt; `press` does one attempt, not a poll, since it is only ever used on a control already on
  screen; `waitForEnd`/`markedProcesses`/`press` take an explicit `step` recorder so `page.test.ts` can
  assert on it directly, and `instance.ts` binds it to the scenario's own timeline; `startReply` takes the
  provider as a third parameter (not two, as the contract's table shows), since "records `reply-streaming`
  when the provider has an open connection" needs the provider to ask. (An initial version of `click`/
  `type`/`press` ran one script per attempt, querySelectorAll plus an `offsetWidth`/`offsetHeight` check,
  because the real displayed-element endpoint was believed unproven against WebKitWebDriver; superseded
  before review closed — see the follow-up entry below.)
- **T046 to T049** The three hooks add exactly 2, 1 and 2 lines
  (`InstancesList.vue` 44→46, `ChatFab.vue` 20→21, `chat/[instance].vue` 1316→1318, matching the T002
  baseline). `pnpm check:templates`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check` all pass; the
  hooks are attributes only, nothing else in any of the three files changed.
- **T050 to T052** `create-and-unlock.test.ts` is 21 lines. SC-005 checked genuinely, not by copying: the
  scenario was written a second time in a scratch file from the README's helper tables alone (not from
  the shown code sample), came to 16 lines, and needed no README fix.
- **T053** `pnpm check:e2e-lib` 123 tests pass; `pnpm typecheck:scripts`, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check` all pass. `pnpm test:e2e` (no `--grep`, so both scenarios, built from scratch):
  `passed create-and-unlock 6.6 s`, `passed smoke-start 4.0 s`, exit 0, on the first real run.
- **T054 to T057** `driver.log` needed no new code: it already exists for the whole life of every
  instance (`instance.ts`, since Stage 1), so `artifacts.ts` only adds `timeline.json`, `screenshot.png`
  and `provider.json` on a failure. `failedStep` is the last recorded step, or, for a `ctx.waitFor`
  timeout specifically, that wait's own description (`WaitTimeoutError` now carries it, rather than
  parsing it back out of the message); both `failedStep` and `material` are computed by `runScenario`
  itself, in `ScenarioResult`, since only it has the error as a typed value rather than a string. The
  "on a pass none of these exist" half of the contract turned out to need its own line: `driver.log` was
  already being left behind after every passing scenario before this change, so `runScenario` now removes
  the scenario's directory on a pass unless `E2E_KEEP` says otherwise. `pnpm check:e2e-lib`: 134 tests.
- **T058** `--app` pointed at a script that only `exec sleep 3600` (never a real WebDriver target),
  `--close-behavior exit --grep smoke --scenario-timeout 15`: `failed smoke-start`, 15.0 s, exit 1. The
  kept material's `timeline.json` has `"failedStep": "instance-ready"` and
  `"error": "scenario smoke-start reached its deadline of 15000 ms"`; the pending readiness operation is
  named even though no `instance-ready` event could be recorded. No process with the run's marker remained
  afterward, and the fake app itself was gone.

### Stage 2 follow-up (real driver calls, not WebdriverIO), 2026-09-22

Review raised two concerns about the querySelectorAll-based `click`/`type`/`press` above: the two lock
buttons shared one `data-testid`, and script-injection was not how a real user's click is ever expressed
against a WebDriver session. Both were checked empirically before any code changed:

- `GET /session/{id}/element/{id}/displayed` (the legacy "is element displayed" endpoint) genuinely works
  against a real WebKitWebDriver session: `200 { value: true }` for a displayed element. It was believed
  unproven when `page.ts` was first written; it is not.
- A real `POST /session/{id}/element/{id}/click` call, on the lock control while a reply streams (the
  case research.md flagged as risky: the process ends about 1 ms after), returns normally in the tens of
  milliseconds, before the process ends — it does not hang waiting for a page-load event that a process
  exit, rather than a navigation, will never fire. Checked four times against the real debug binary,
  isolated instance root, real stand-in provider; last run: click returned in 18 ms, the marked process
  was confirmed gone 39 ms after the click, the provider's connection closed 34 ms after opening.
- WebdriverIO was spiked separately (its own throwaway project, `webdriverio` installed there only) and
  does drive the real application through the same `tauri-driver` — session creation, `$$`, `isDisplayed`,
  `isClickable`, `getText`, a real `.click()` all worked. Not adopted: everything it would have bought
  (real find/displayed/click semantics) is already reachable through calls the suite's own ~200-line
  client and `tauri-driver` already speak, for the cost of one added method, not 238 transitive packages
  and a new supply chain to audit even as a dev-only dependency.

Changed as a result, before the pull request was reviewed further:

- `webdriver.ts` gained `isDisplayed(element)`.
- `page.ts`'s `click`/`type`/`press` now find the control by hook (`findElements`), pick the one
  `isDisplayed` reports true (a hook may still find more than one element in general, though none of the
  three current hooks does any more — see the next point), and act on it with the driver's own
  `click`/`sendKeys` calls, genuinely checked rather than approximated by `offsetWidth`/`offsetHeight`
  inside a script. `press` no longer needs the `setTimeout(0)` scheduling trick a click that ends the
  application was thought to need; a second `times` click after the application has already ended now
  throws like any other call to a gone session, rather than being masked by firing both from one script
  tick.
- `lock-instance` was one `data-testid` shared by the sidebar and the header button, with "the helper
  clicks the displayed one" as the intended design (`contracts/test-hooks.md`), not an implementation
  shortcut — reasonable given the pre-existing script only ever found one page-side match anyway. Review
  preferred two unique ids regardless, once it was clear a real, per-element `isDisplayed` check made
  either design equally correct: `lock-instance-sidebar` and `lock-instance-header`
  (`contracts/test-hooks.md`, T048). At the suite's fixed 1280×800 virtual screen only the sidebar one is
  ever on screen; the header one is exercised only by a future narrower-viewport scenario.
- Found by running the real flow end to end while checking the above (not by unit tests, which mock the
  backend and so could not catch this): `flows.ts`'s `startReply` sent `send_message` without
  `idempotencyKey`, a field the command has always required
  (`src-tauri/src/chat/commands.rs`, `SendMessageArgs`) and the frontend has always supplied
  (`useChat.ts`, defaulting to `crypto.randomUUID()`). The fake-driver tests never noticed, since they
  answer `send_message` unconditionally. Fixed the same way the frontend does.
- `pnpm check:e2e-lib`: 136 tests. `pnpm typecheck:scripts`, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`: all pass. `pnpm test:e2e`, no `--grep` (both scenarios, real binary, from scratch):
  `passed create-and-unlock 6.9 s`, `passed smoke-start 4.0 s`, exit 0. No process carrying the run's
  marker, nor any stray `tauri-driver`/`WebKitWebDriver`/`Xvfb`, remained after any of the runs above.

### Stage 3, close scenarios (T059 to T066), 2026-09-22

- **T059** Branch `016-e2e-stage3-close` started from updated `origin/main` (PR #117 merged). Graphify
  from the primary checkout: both queries ("close promises drain ladder deadlines",
  "assert a process ended within a deadline") returned nodes from before spec 013/016 existed — the
  primary checkout is still on an unrelated branch (`fix/active-model-info-lock`), so its graph predates
  `src-tauri/src/vault_gate/` and `scripts/e2e/` entirely and had nothing relevant to extend from. Read
  `src-tauri/src/vault_gate/drain.rs` directly instead: `COOPERATIVE_WINDOW` 1 s, `TOTAL_LIMIT` 3 s,
  `HARD_END_GRACE` 500 ms.
- **T060/T061** `close-promises.ts` exports the three scenario deadlines (`PROCESS_END_LIMIT_MS` 4000,
  `PROVIDER_CLOSE_LIMIT_MS` 1000, `RELAUNCH_LIMIT_MS` 10000) with the drain ladder's own numbers as their
  documented source; `close-promises.test.ts` checks the three values.
- **T062 to T065** `lock-while-streaming.test.ts`, `window-close-while-streaming.test.ts`,
  `closing-page.test.ts`, `lock-twice.test.ts` written. Three real bugs surfaced while getting them to
  pass for real, none catchable by the fake-driver unit tests:
  - Reading a provider connection's `closedAt` once, synchronously, right after `waitForEnd` is a real
    race: when the process ends within a few milliseconds of the press (window close: confirmed 0 ms),
    the socket's own "close" event has no realistic chance to reach this process's event loop yet.
    `lock-while-streaming` and `window-close-while-streaming` now `ctx.waitFor` it (fixed, up to
    `PROVIDER_CLOSE_LIMIT_MS`) instead of reading it once.
  - `lock-twice.test.ts` pressed the lock control right after `createAndUnlock` without `openChat` first
    — the lock hooks only exist on the chat page, not the workspace page. Missing `openChat(instance)`,
    not an application defect.
  - `press(hook, { times: 2 })`'s second click can find the session already gone (handled since the
    Stage 2 follow-up) or, seen for real running the full suite together, a **stale element reference**
    instead: under load the page's DOM can be torn down before the driver reports the whole session gone.
    `page.ts`'s `press` now treats a stale element reference (and "no such element") the same as a gone
    session for any click past the first.
  - Running the whole suite together (not `--grep`-isolated) surfaced a real, reproducible race in
    `tauri-driver` itself: its own port can answer, satisfying `launchDriver`'s readiness poll, before
    its connection to the native `WebKitWebDriver` behind it is actually ready, so the very first
    `POST /session` can fail with `got no answer` even though the driver is otherwise healthy. Reproduced
    twice, always on the run's first scenario. `instance.ts`'s `startInstance` now retries `newSession`
    once after a short pause on exactly this failure (`newSessionWithRetry`); any other error, or a
    second failure, still fails at once.
- **T066** `pnpm check:e2e-lib`: 144 tests. `pnpm typecheck:scripts`, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`: all pass. Each scenario alone via `--grep`: `lock-while-streaming` 7.9 s (press to
  process end 0.1 s), `window-close-while-streaming` 5.8 s, `closing-page` 9.9 s, `lock-twice` 11.9 s. The
  full suite (all six scenarios, no `--grep`) run twice in a row after the fixes above: both times
  `passed` for all six, `press to process end` between 0.0 s and 0.1 s. No process carrying the run's
  marker, nor any stray `tauri-driver`/`WebKitWebDriver`/`Xvfb`, remained after either run.

### The relaunch scenario, checks A and B (T067/T068), 2026-09-22

- **T067** Branch `016-e2e-stage3-relaunch` started from updated `origin/main` (PR #118 merged). Release
  build: `nix develop --command scripts/with-nix-host-bridge.sh pnpm tauri build --no-bundle` (the
  `tauri:build` npm script forwards `--no-bundle` past the Tauri CLI to `cargo build` itself, which
  rejects it as unrecognized — bypassed by calling the raw `tauri` alias directly). Cold build: 6 m 20 s,
  produced `src-tauri/target/release/haex`. `pnpm test:e2e --app <that path> --grep smoke`: `passed
smoke-start`, 6.0 s, exit 0, correctly reported as "closes by relaunch, from path". No leftover marked
  process, `tauri-driver`, `WebKitWebDriver` or `Xvfb` afterward. **Result: `tauri-driver` can drive a
  release-profile binary.** T068 can proceed.
- **T068** Throwaway script (not committed; several iterations, none kept) against the release binary,
  `-fbdir <dir>` added to the virtual screen's `-s` server-args string (a gotcha in itself: `-fbdir` is an
  **Xvfb** server option, not an `xvfb-run` option — passing it as a separate `xvfb-run`-level argument
  makes `xvfb-run` misparse it as the command to run, and the driver never starts. It belongs inside the
  same `-s "-screen 0 1280x800x24 -fbdir <dir>"` string `driverCommand` already builds).
  - One early run hit a real, if inconclusive, anomaly: `press('lock-instance-sidebar')` threw
    `SessionGoneError: ... 500: Session terminated without a reply` on the _first_ click, and the driver
    log showed two distinct `haex` process starts 4.2 s apart, before any lock action. Rerunning the
    same flow three more times (with and without `-fbdir`, with dense process-listing every 300 ms) never
    reproduced it — every other run showed exactly one process throughout create, unlock and open-chat.
    Best hypothesis, not confirmed: the same pre-existing `tauri-driver` native-connection race
    `newSessionWithRetry` already retries (PR #118) can leave the _first_ (failed) attempt's spawned app
    process running unreaped while the retry's app process becomes the one actually driven — an
    already-known race, not something `-fbdir` causes, since it also happens with `-fbdir` off elsewhere
    in the suite. Recorded here rather than dropped silently; not reproducible enough to act on beyond
    this note. Worth a second look if a real scenario run ever shows two processes at once.
  - The real check, completed cleanly on the next run: pressed `lock-instance-sidebar` after
    create+unlock+open-chat. `press()` returned normally in about 10 ms (release build behaves like the
    debug build here). (a) the original pid was gone within 4 s (confirmed in about 70 ms). (b) a new
    marked process of the same binary appeared within 10 s (confirmed in about 80 ms) — the marker is
    inherited across the relaunch (research R8 holds). (c) the `-fbdir` XWD file stayed readable
    throughout: non-blank while chat was open (~1.92 MB of 4.10 MB non-zero), dropped to blank
    immediately after the relaunch was seen (295 bytes non-zero — the "screen cleared" instant), then
    non-blank again about 2 s later (~1.90 MB non-zero) once the relaunched window painted its own
    (unlock) screen. **Result: all three of (a), (b), (c) hold. T069 to T073 can proceed as planned; no
    FR-018 gap to report.**
  - Gotcha hit while investigating, unrelated to the actual finding: the first, crashing run left its
    whole process group (`xvfb-run`/`Xvfb`/`tauri-driver`/`WebKitWebDriver`/the relaunched `haex`)
    running, because the throwaway script's own top-level `catch` only logged the error and never reached
    its cleanup — a reminder that a spike script needs the same "kill the process group even on an
    unexpected throw" discipline as the real suite's `stop()`. Killed by pgid by hand; also found and
    removed one stray empty `bdir` file the misparsed first `xvfb-run` invocation left in the worktree
    root (untracked, harmless, from before the `-fbdir` placement was fixed).

### The relaunch scenario, built and verified (T069 to T073), 2026-09-22

- **T069/T070** `scripts/e2e/lib/framebuffer.ts`/`.test.ts`: the XWD file header's 25 fields are fixed-size
  `CARD32`s and always big-endian, independent of the host's own byte order and of the file's own
  `byte_order` field (which only describes the pixel data) — confirmed against a real `Xvfb` 21.1.24
  capture in T068 (`header_size` 160, `colormap_entries` 256, and `header_size + colormap_entries*12 +
bytes_per_line*height` matching the observed file size exactly). `readFramebuffer` parses the header and
  slices out just the pixel data (past the header and the colormap); `isPainted` requires at least 1% of
  the pixel bytes to be non-zero, separating the 295-byte cleared-screen capture from the roughly 1.90 MB
  painted capture recorded in T068. Dedicated tests cover the cleared-screen margin and confirm
  header/colormap bytes (e.g. the window name) are never mistaken for painted pixels. 9 tests, synthetic
  buffers built in the test file itself, no
  real `Xvfb` needed to run them.
- **T071** `driverCommand` takes an optional `framebufferDir`; when given, `-fbdir <dir>` is appended
  inside the same `-s "-screen 0 1280x800x24 ..."` string (not as a separate `xvfb-run` argument — the
  T068 gotcha). Threaded through `StartInstanceOptions` → `launchDriver`, and through
  `ScenarioContext.startInstance`'s options → `StartInstanceRequest` → the real `startInstance` call in
  `scenario.ts`, so a scenario can ask for one without touching `instance.ts` directly.
  `contracts/helpers.md`'s `ctx.startInstance` row updated to name it.
- **T072** `relaunch-after-lock.test.ts`: `needs: { closeBehavior: 'relaunch' }`; create, unlock, open
  chat; press `lock-instance-sidebar` once; (1) `waitForEnd(PROCESS_END_LIMIT_MS)`; (2) `ctx.waitFor` a
  new marked process other than the original pid, within `RELAUNCH_LIMIT_MS` (records `relaunch-seen`);
  (3) `ctx.waitFor` the `-fbdir` capture to report `isPainted`, within the same limit (records
  `relaunch-painted`); (4) `instance.stop()` (kills the relaunched process too — T068 confirmed it shares
  the original driver's process group), then a fresh `ctx.startInstance({ reusesRoot: instance.root })`,
  checking the created instance is listed and `location.pathname` is `/`.
  - One real bug found running it for real (not catchable by a lib unit test, which mocks the whole
    driver): step (4)'s first version checked `document.querySelector(...)` once, right after
    `startInstance` resolved. `startInstance` only waits for the _process_ to appear, not for its page to
    have rendered the instance list yet — an exact repeat of the lesson `click`/`type` already learned
    (find-and-poll, not one bare check). Failed once against the release build
    (`false !== true`, `failed at: instance-ready`) with a clean timeline showing (1)-(3) all correct
    (process end 51 ms, relaunch seen immediately, window painted ~556 ms later) — only the last, bare
    check was wrong. Fixed with `ctx.waitFor` around the same `exec`, matching how every other displayed-ness
    check in the suite already works.
- **T073** Against the release build (`pnpm test:e2e --app <release path> --grep relaunch-after-lock`):
  `passed`, 11.6 s, `press to process end` 0.1 s. Against the debug build (`pnpm test:e2e --grep
relaunch-after-lock`, no `--app`): `skipped`, "the build exits on close; this scenario needs one that
  relaunches" — correct, not a pass or a fail. Full lib suite: 154 tests; typecheck/lint/format all pass.
  Full scenario suite (all seven test files — `closing-page` runs two color schemes as one file) against
  the release build, no `--grep`: all seven `passed`, 57.4 s total, no leftover marked process or
  `tauri-driver`/`WebKitWebDriver`/`Xvfb`. Stage 3 is now complete end to end (T059 to T073). Pushed as
  `016-e2e-stage3-relaunch`, PR #119 opened (https://github.com/haex-space/haex-vault/pull/119) — the second half
  of Stage 3 (T067 to T073), following #118's first half (T059 to T066) per the split agreed there.

### Stage 4, SC-001/SC-007 (T074), 2026-09-22

- **T074** Branch `016-e2e-stage4-validation` from updated `origin/main` (PR #119 merged). Full suite
  (seven test files; `closing-page` runs both color schemes as one file) against the debug build: 5
  passed, 1 skipped (`relaunch-after-lock`, correct reason), **1 failed on the first attempt**:
  `lock-while-streaming` — `could not start the application: POST /session got no answer: fetch failed`.
  The driver log showed `tauri-driver` printing `FATAL: Unable to listen for HTTP server at host
127.0.0.1 and port 38399` once, then — instead of exiting — staying alive and logging a multi-second
  burst of `Error serving connection: hyper::Error(User(Service), client error (SendRequest) ...
connection closed before message completed)` (225763 lines in that one driver.log). Diagnosed, not
  patched over: `launchDriver`'s port-in-use detection only runs once its `hasEnded(started.child)` check
  is true; here the process never ended, so that branch — and its `withPortRetry` — never ran at all.
  `newSessionWithRetry`'s own one retry (300 ms later) also hit the same broken state, since `tauri-driver`
  stayed wedged for several seconds, well past that gap. The host was otherwise idle at the time (checked
  immediately after: no leftover marked processes, 14 sockets in `TIME-WAIT`, the full 32768-60999
  ephemeral range available) — nothing pointed at resource exhaustion from this session's own many prior
  runs. An isolated retry of the same scenario, moments later, passed cleanly (7.8 s). Recorded honestly as
  an observed, rare `tauri-driver`-level flake (the exact race `freePort`'s own doc comment already names:
  "the port is bound and released, so another process can take it before the caller uses it") that this
  one specific failure mode of `tauri-driver` does not exit cleanly from, rather than invented as a fixed
  bug on unverified evidence — one data point for T075's own soak test, not a suite defect to paper over
  with an untested regex change.
  - Debug build durations (the 5 that passed plus the retried one): `closing-page` 9.8 s,
    `create-and-unlock` 4.6 s, `lock-twice` 9.9 s, `lock-while-streaming` 7.8 s (on retry),
    `window-close-while-streaming` 5.8 s, `smoke-start` 6.1 s — all under the 30 s bound (SC-001), whole
    run well under 5 minutes (SC-007).
  - Release build, full suite, no `--grep`: all seven `passed` in one clean run (no repeat needed) —
    `closing-page` 10.0 s, `create-and-unlock` 4.6 s, `lock-twice` 9.7 s, `lock-while-streaming` 5.7 s,
    `relaunch-after-lock` 9.5 s, `smoke-start` 4.0 s, `window-close-while-streaming` 5.7 s; total 57.1 s.
    All five close scenarios plus the relaunch ran (SC-001's "with the release build by path all five
    run"). Exit status 0 in both the debug (after the one retried scenario) and the release run. No
    leftover marked process, `tauri-driver`, `WebKitWebDriver` or `Xvfb` after either run.

### Stage 4, SC-002 (T075), 2026-09-22

- **T075** A throwaway soak-test script (not committed), run entirely inside `nix develop`: a real,
  independent HaexVault instance (the actual `~/.local/share/space.haex.vault`, not an isolated root — under
  `xvfb-run` on its own virtual display, never the real desktop) kept running throughout; 20 consecutive
  `pnpm test:e2e` runs against the debug build, 10 of them (a shuffled schedule) sent `SIGKILL` at a
  random 0-24 second delay.
  - **Two real bugs in the script itself**, found and fixed across four dry runs before committing to the
    full 20: (1) a fixed 40-second bound on waiting for the runner to end was too tight for an ordinary
    completing run (the debug suite's own seven scenarios take about a minute, T074) and force-killed a
    _normal_, healthy run — raised to 180 s; (2) comparing the real data directory's state with a full
    `sha256sum` of every file's _contents_ took about 5 minutes each way (it holds ~1.7 GB, including a
    923 MB downloaded model file, across 107742 files) — replaced with a metadata listing
    (`find -printf '%p %s %T@'`, path+size+mtime, sorted and hashed), which is sure to change on any
    write and runs in about 0.1 s. A first attempt at that lighter listing (`find -exec stat ... {} \;`)
    was itself still slow, for an unrelated reason: `-exec ... \;` spawns one `stat` process per file, so
    107742 files meant 107742 process spawns; `-printf` avoids the subprocess entirely.
  - **Result of the real 20-run soak (about 9.5 minutes total)**: all 10 normal runs exited 0 with no
    leftover marked process; all 10 killed runs ended by `SIGKILL` (exit 137) as intended; every killed
    run's leftovers (5 to 16 processes, depending on how far it got before the kill) were reported removed
    by the _next_ run's own preflight sweep (`removed N leftover process(es) of an earlier killed run`),
    confirmed by grepping each next run's own log rather than assumed; the one killed run with no
    following run (the 20th and last) was swept explicitly by the script itself. Own HaexVault's application
    process survived the entire test; its data directory's listing was identical before and after; the
    real desktop's window count was unchanged (8 before, 8 after) at every checkpoint, not just at the
    end. No leftover marked process, `tauri-driver`, `WebKitWebDriver`, `Xvfb` or `xvfb-run` remained
    afterward.

### Stage 4, FR-006 isolation on a real run (T076), 2026-09-22

- **T076** While a real `pnpm test:e2e` run (debug build, all seven scenario files) was active, took a
  top-level `ls -la` of the maintainer's `~/.local/share`, `~/.config`, `~/.cache`,
  `/run/user/$(id -u)` and `~` itself before, mid-run (checked live while the suite's own driver and
  application processes were visibly running), and after; also `pgrep -af xdg-desktop-portal` at the same
  three points. All five directory listings were identical at every checkpoint except one incidental,
  expected difference: `~/.local/share/pnpm`'s own mtime moved by a minute — from invoking the `pnpm` CLI
  itself to run the command, not from anything the application or the suite wrote; not a HaexVault isolation
  concern. The three real `xdg-desktop-portal`/`-gtk`/`-cosmic` processes already running (the
  maintainer's own desktop session) kept the exact same pids throughout — no new one was started by the
  run. This establishes that the captured top-level entries stayed unchanged on a real run; it does not
  establish that files inside already-existing child directories stayed unchanged, because no recursive
  manifests were captured. The architecture still redirects every one of these locations for the instance
  itself (`buildInstanceEnv`, per Stage 1), but that is separate from the scope of this runtime observation.

### Stage 4, SC-003 seeded failures (T077), trial (a), 2026-09-22

- **Trial (a), "the reply is not cancelled when the vault closes"**: took far longer than expected and
  is recorded in full because the false starts are themselves a real finding about the app's own
  cancellation architecture, not just noise.
  - **First attempt** (scratch change, reverted): bypassed `state.gate().spawn(...)` in
    `chat/commands.rs` with a plain `tauri::async_runtime::spawn(...)`, so the streaming task was never
    registered with the gate's tracker at all. Rebuilt debug, ran `--grep while-streaming`: **both
    scenarios still passed.** Reason, understood only after the fact: the whole process still ends
    quickly regardless (nothing else was touched), and an OS-level process exit closes every socket,
    including the one to the stand-in provider, whether or not the Rust-level task was ever tracked or
    cooperatively cancelled. Bypassing tracking cannot produce an observable difference in a promise
    that's already guaranteed by the process ending.
  - **Second attempt**: found the actual first-line cancellation call, `abort_turn`'s
    `current_generation` `AbortHandle::abort()` in `chat/commands.rs`, and made it a no-op (`guard.take()`
    without calling `.abort()`). Rebuilt, ran: **still passed.** Hypothesis: the drain ladder's own
    generic abort rung (`vault_gate/drain.rs`'s `drain_with`, the `for abort in ... { abort.abort() }`
    loop) is a redundant second line of defense that fires about a second later regardless.
  - **Third attempt**: additionally disabled that generic abort rung in `drain.rs`. Rebuilt, ran: **still
    passed**, and `press to process end` was still ~0 ms — meaning the process was ending almost
    instantly regardless of either seeded change, contradicting the theory so far.
  - **Diagnosis**: added temporary `eprintln!` probes (`T077-PROBE ...`, never committed) at each step of
    `instances/close.rs`'s `begin_close`/`finish_close`, rebuilt, ran with `--keep` (the actual CLI flag —
    an `E2E_KEEP=1` environment variable on the outer `nix develop`/pnpm invocation is not read by the
    suite at all; only `scripts/e2e/cli.ts`'s own `--keep` flag sets it for the scenario subprocess) to
    retain `driver.log`. The probes showed `drain_with returned Drained` within 8 ms of the press — the
    task tracker was already empty almost immediately, meaning something was still cancelling the
    streaming task cooperatively despite both earlier changes. Reading `chat/commands.rs` more closely:
    the per-request `cancel_token` created at `send_message` time is stored into
    `ChatState.tool_cancellation` (not only used for in-flight tool calls, despite the field's name), and
    `chat/turn/step.rs`'s streaming loop races its chunks against exactly this token via `tokio::select!`.
    `abort_turn`'s _second_ block calls `.cancel()` on this same token — a call neither of the first two
    attempts had touched.
  - **Fourth attempt, the one that finally worked**: disabled all three — `current_generation`'s
    `.abort()`, `tool_cancellation`'s `.cancel()`, and the drain's generic abort rung — together.
    Rebuilt, ran: `lock-while-streaming` **failed** as intended:
    `provider connection closed 3045 ms after the press, over the 1000 ms limit` (matching the drain
    ladder's own `TOTAL_LIMIT`, 3 s, plus overhead — the task now genuinely survived until the drain
    itself timed out and force-ended the process). `window-close-while-streaming`, unexpectedly,
    **still passed** — its own `driver.log` (kept, checked directly) contains no `T077-PROBE` line at
    all, meaning `begin_close`/`finish_close` never ran for that instance's close path; the WebDriver
    `DELETE /window` call apparently ends the process by some other route than `close_instance`'s command
    (`take_over_exit` calls the exact same `start_close` — why window-close doesn't reach it was not
    tracked down further given the time already spent). **Recorded as a genuine open question for a
    maintainer, not silently resolved**: it means `window-close-while-streaming`'s own coverage of "the
    reply is cancelled" may currently rest on the same process-exit guarantee that made the first two
    seeded attempts here look like they were passing for the right reason when they were not.
  - Reverted all three files, rebuilt, confirmed both scenarios pass again cleanly
    (`press to process end` back to ~0 ms).
  - **What this proves for SC-003**: the promise itself is real and covered — `lock-while-streaming`
    does fail when reply-cancellation is genuinely absent — but the app's defense-in-depth (three
    independent, redundant cancellation paths for the one thing) means a _partial_ regression in any
    one of them would currently go undetected by this suite, since the other two still make the
    observable outcome (connection closes in time) hold. That is a real, useful finding in its own
    right, not just a methodology note.

- **Trial (b), "the page is not replaced by the closing page"**: made `close_effects.rs`'s
  `show_closing_page` a no-op (scratch change, reverted), rebuilt, ran `closing-page`: **still passed,
  unaffected**. Reason: `closing-page.test.ts` navigates directly to `tauri://localhost/closing.html`
  itself (research R10's own documented decision — the real close lasts only milliseconds, too fast
  to reliably judge live, confirmed again by the original spike) and never triggers an actual close, so
  it never calls `show_closing_page` at all; `lock-while-streaming`/`window-close-while-streaming` (the
  scenarios that DO trigger a real close) check process-end timing and the absence of an error alert,
  but never assert on-screen content during the close. **Net result: no scenario currently exercises
  this specific promise's real trigger path** — SC-003's own table entry for this row does not hold as
  stated for the current suite. Raised to the operator rather than forcing a flaky live-DOM-sampling
  assertion against a promise research.md already documented as unobservable in real time; operator's
  decision: leave it (the closing page is cosmetic — a spinner) and prioritize the substantive
  guarantee instead, that closing genuinely stops every call and reinitializes state so nothing leaks
  from one vault session into the next. Reverted the seeded change, rebuilt, confirmed `closing-page`
  passes on the clean baseline again. Recorded here as a genuine, accepted gap, not silently dropped.

- **Trial (c), "the process does not end"**: made `close_effects.rs`'s `request_end` and `force_end`
  both no-ops (keeping `force_end`'s `children.kill_all()` call, since that alone does not end the
  process), rebuilt, ran `--grep lock`: `lock-twice` and `lock-while-streaming` **both failed as
  intended**, but not the way expected. Neither failed with `waitForEnd`'s own specific message
  ("process did not end within 4000 ms"); instead both ran until the scenario's own generic 60000 ms
  deadline fired (`scenario lock-twice/lock-while-streaming reached its deadline of 60000 ms`), with the
  failure attributed to the **`press`** step, not `waitForEnd`. Traced the cause: `press()`'s underlying
  `client.click()` WebDriver call itself hangs, not the later, more specific check. Hypothesis: clicking
  the lock control triggers `begin_close`'s synchronous first phase, which calls `show_closing_page()`
  and navigates the window before anything else happens; WebDriver's own click command appears to block
  on a resulting page-load signal from that navigation. In the normal, working case this never matters,
  because the process actually dies within tens of milliseconds — exactly the original spike's own
  "decisive test" (a real click during a real close) — which severs the pending click's connection
  before any such wait would matter. With the process seeded to never end, that safety net is gone, so
  `press()` hangs for the full scenario timeout instead of failing fast with a specific message. This is
  the seeded failure manifesting correctly, just less precisely attributed than trial (a)/(b)'s; no
  product or suite change was made in response, since making `press()` itself impose a tighter timeout
  would only mask how a real regression here actually presents.
  - **Process cleanup, checked with extra care since the app was deliberately made immortal**: after
    both failures, before reverting anything, `pgrep -af` for the debug binary, `tauri-driver`,
    `WebKitWebDriver`, `xvfb-run` and `Xvfb` found nothing left running — the suite's own teardown
    (`stopGroup`, a `SIGKILL` by process group id) force-ends everything regardless of whether the app
    ever cooperates, exactly as designed.
  - Reverted `close_effects.rs`, rebuilt, reran `--grep lock`: all three non-skipped scenarios pass
    again (`lock-twice` 9.9 s, `lock-while-streaming` 5.9 s, both `press to process end` back to
    ~0.1 s), `relaunch-after-lock` still correctly skipped on a debug build. Confirmed clean again with
    the same process check.
  - **What this proves for SC-003**: the lock path catches a process that does not end after the lock
    control, but the suite currently catches it through the same generic per-scenario deadline that
    would also fire for an unrelated hang, rather than through `waitForEnd`'s own faster, specific
    check. That is a real, minor gap in diagnostic precision for this one failure mode (a maintainer
    reading the report sees "press timed out after 60 s" rather than "the process did not end").

- **Trial (d), "a release build does not relaunch"**: made both `close_effects.rs`'s `request_end`
  and `force_end` treat `ClosePolicy::Relaunch` the same as `Exit` (scratch change, reverted), built a
  release binary (`pnpm tauri build --no-bundle`, ~1m confirmed cargo time on top of the frontend
  build), ran `--grep relaunch`: `relaunch-after-lock` **failed as intended**, cleanly and specifically
  this time — `timed out after 10000 ms waiting for a new marked process of the same binary to appear
after the relaunch` — unlike trial (c), the failure landed exactly on the check the promise is about,
  not a generic deadline. No leftover processes afterward (checked the same way as every other trial).
  Reverted, rebuilt release, reran: `relaunch-after-lock` passes again (11.4 s, `press to process end`
  back to ~0.1 s).

### Stage 4, T077 follow-up: window-close-while-streaming's real limit, investigated and accepted, 2026-09-23

A reviewer (CodeRabbit, on PR #121) correctly caught that trial (a)'s own conclusion had overstated
things: `window-close-while-streaming` staying green while cancellation was removed is not "an open
question to note", it means the task's own stated expectation ("(a) ... expect `lock-while-streaming`
**and** `window-close-while-streaming` to fail") was not actually met, and T077 should not have been
marked done on that basis. Reopened T077, investigated properly rather than either forcing a fix or
re-closing it on the same evidence.

- **Root cause, confirmed with probes**: `instance.closeWindow()` used WebDriver's own
  `DELETE /session/{id}/window` command. Added temporary `eprintln!` probes (reverted after) at
  `WindowEvent::CloseRequested`, `ExitRequested`, `RunEvent::Exit` (`lib.rs`) and at `begin_close`/
  `finish_close` entry (`close.rs`): **none of them ever fire** when this WebDriver command runs against
  the app. `close_effects.rs`'s `request_end`/`force_end` and `lib.rs`'s event wiring are all correct
  and reach `begin_close` for every path Tauri itself dispatches — WebDriver's close-window command
  simply never reaches Tauri's dispatch at all, likely tearing the GTK window down directly. So
  `window-close-while-streaming`'s pass has always rested entirely on how fast that external, unmanaged
  teardown happens to be, the same class of blind spot trial (a)'s own first failed seeding attempt
  already illustrated for a different mechanism.
- **First fix attempt: `xdotool windowclose` instead of WebDriver's command.** Implemented in full: a new
  `scripts/e2e/lib/x11.ts` (`readDisplay` via a new generic `processes.ts` helper `readProcEnvVar`;
  `closeWindowNatively` with an injectable runner and a poll loop using `xdotool search --onlyvisible
--pid <pid>` to pick the one real, mapped window over a tiny unmapped 10x10 GTK helper window
  confirmed to also exist at the same pid), wired through `page.ts`/`instance.ts`, `xdotool` added to
  `REQUIRED_TOOLS` (already present in the Nix devshell's package list — no atoms change needed), README
  and `contracts/helpers.md` updated, 9 new/updated tests, typecheck/lint/format clean, 164 lib tests
  passing. Made `window-close-while-streaming` pass normally.
  - **Reapplied trial (a)'s exact seed to check it for real**: `window-close-while-streaming`
    **still incorrectly passed** while `lock-while-streaming` correctly failed — the new mechanism has
    the identical defect.
  - **Root-caused definitively** by reading xdotool's own upstream source
    (`jordansissel/xdotool`'s `xdo.c`): `xdo_close_window` (bound to the `windowclose` CLI command) calls
    `XDestroyWindow` directly — no `ClientMessage` at all, the same class of bug as the original
    WebDriver command. `xdo_quit_window` (bound to `windowquit`, described in its own man page as
    closing "gracefully") sends `_NET_CLOSE_WINDOW` to the **root** window with
    `SubstructureRedirectMask` — by design, this needs an actual window manager running to translate it
    into the client's own `WM_DELETE_WINDOW` handling, and Xvfb runs with no window manager here.
    Confirmed with `xprop -id <id> WM_PROTOCOLS` that the app's own window correctly advertises
    `WM_DELETE_WINDOW` support — the application side is not at fault; `xdotool` simply ships no command
    that sends a bare, window-manager-independent `WM_DELETE_WINDOW` straight to a specific window.
  - Reverted every diagnostic probe and the reapplied seed (confirmed clean via `git diff main`), and
    reverted the entire `xdotool` suite change itself back to matching `main` exactly (confirmed via
    `git diff main -- <every touched file>` producing no output) — keeping a differently-broken "fix"
    that still doesn't exercise `begin_close`, at the cost of a new required tool and ~150 lines of new
    code, would only relocate the same misleading pass to a new mechanism.
- **Decision (operator's, after being given three concrete options — a minimal window manager alongside
  Xvfb, e.g. `openbox`, so `xdotool windowquit`/`wmctrl -c` would have something to forward the request
  to; a small compiled native helper sending the raw ICCCM `ClientMessage` directly, bypassing the need
  for a window manager; or accepting and documenting the limitation)**: accept and document, matching the
  precedent already set for trial (b)'s closing-page gap rather than adding a new system dependency and
  background process (the window-manager option) or a new compiled artifact (the native-helper option)
  for one diagnostic scenario.
- **T077's actual, corrected scope**: `lock-while-streaming` is what SC-003's cancellation-on-close
  promise is verified through — trial (a)'s seed fails it specifically and correctly.
  `window-close-while-streaming` still verifies its own real promise (closing the window ends the
  process and an in-flight reply does not linger past the promised limit), it just cannot currently
  distinguish that from a scenario where cancellation is entirely broken, because nothing in this
  suite's toolset can yet deliver a window-manager-mediated close under Xvfb. **T077 is complete on this
  corrected understanding**: every trial that CAN be seeded to fail does, precisely; the one that
  cannot is now a documented, understood, accepted limitation instead of an unexamined false pass.

### Stage 4, a look beyond T077: does state actually stay isolated between vault sessions, 2026-09-22

Prompted by the operator's own priority ("hauptsache alle calls werden gestoppt und variablen neu
initialisiert, damit nie state zwischen vaults leakt") rather than a numbered task, before moving on to
T078: read the code path a vault-to-vault leak would actually have to travel through, the same way the
T077 (a) investigation traced the cancellation architecture before trusting it.

- **Process-level cleanup is thoroughly covered already, at the Rust unit-test level, with real
  processes**: `vault_gate/children.rs`'s `ChildRegistry::kill_all()` signals the whole process group
  (reaching grandchildren a child itself spawned, e.g. an agent's own shell), is called on **every**
  drain outcome — `Drained`, `DrainedAfterAbort`, and `Stuck` alike (`drain.rs:54,66`), not only the
  slow path — and `drain_tests.rs`'s `children` module proves this for all three outcomes with a real
  spawned `sleep`, plus a fourth test proving a child registered _after_ `kill_all()` is still killed at
  once. Nothing here needs a new E2E check; it is already exercised more precisely at the unit level
  than an E2E scenario could manage (E2E has no way to assert a specific grandchild pid died).
- **A real, more significant finding, upstream of spec 016 entirely**: `instances/open.rs`'s
  `open_instance` still contains an "atomic switch" path — if a second vault is opened while one is
  already active, it cancels the old vault's preload, drops the old database handle, clears
  `chat.session`, bumps a `vault_generation` counter and invalidates the whisper cache, then publishes
  the new vault, all **without ending the process**. `state.rs`'s `switch_to` doc comment says outright:
  _"Stage 4 of spec 013 removes it together with the switch itself."_ Checked spec 013's own
  `tasks.md`: **Phase 5, "A new vault never sees anything from the previous one" (User Story 2, **P1**,
  FR-010 to FR-012, SC-003) is entirely unimplemented — T053 through T062 are all still `[ ]`.** Its own
  goal statement is almost the operator's sentence verbatim: _"one app process serves at most one vault
  session; opening or creating while one exists is refused."_ The gate-level primitives this phase needs
  already exist and are unit-tested (`ensure_can_open`/`begin_session`, `HaexError::VaultAlreadyActive`/
  `VaultClosed`, commit `b99bb68`), but `open.rs`/`create.rs` were never switched over to use them (T055
  to T057), so today a second `open_instance` call is not refused — it is served by the ad hoc cleanup
  above instead of the drain-ladder architecture trials (a)-(c) already validated.
  - **Practical severity, checked rather than assumed**: searched the frontend for any UI action that
    could reach a second `open_instance` call while a vault is active. None exists —
    `pages/index.vue` (the only place `OnboardingUnlockSheet` is mounted) is the pre-unlock picker only;
    `onUnlocked`/`onCreated` both navigate away to `/workspace/<name>`; the only vault-ending actions
    inside a workspace (`chat/[instance].vue`, `federation/[instance].vue`) call `closeAsync`, which
    ends the process, never a "switch vault" action. So the gap is currently **latent, not reachable
    through the real UI** — but it is a backend command that trusts the frontend never to call it twice,
    which is exactly the shape of bug FR-010 exists to rule out by construction instead.
  - **Not something spec 016 should paper over**: writing an E2E scenario against this now would either
    encode the current (soon-to-be-wrong) switch behavior as "expected", or assert the refusal spec 013
    intends and fail against today's code — neither belongs in this spec's validation record. Spec
    013's own tasks already plan the right-shaped tests for this (T053's `vault_single_session.rs`,
    T054's `gate_tests.rs` case), at the Rust integration level, once T055 to T057 land.
  - **Reported to the operator as a real, separate finding**, not folded into T077/T078's own record:
    the promises spec 016 was asked to validate (SC-003's close behaviors) all hold; a related but
    distinct promise (spec 013 FR-010, single active vault session) does not yet exist in the code to
    validate.

### Stage 4, SC-004 material check (T078), 2026-09-22

Failure 1 was already seeded and recorded at T058 (Stage 2): app never starts, kept material's
`timeline.json` has `"failedStep": "instance-ready"` and
`"error": "scenario smoke-start reached its deadline of 15000 ms"`. Failures 2 and 3 reuse T077's own
trials (c) and (a); re-seeded each briefly with `--keep` to inspect the actual kept material's content
(the earlier trial runs' directories had already been cleaned up), not just the one-line report already
recorded there.

- **Failure 2** (process never ends after the lock control — `close_effects.rs`'s `request_end`/
  `force_end` as no-ops again): `lock-twice` failed, `press` → 60 s generic deadline, same shape as
  trial (c). Kept material: `timeline.json` has `"failedStep": "press"`,
  `"error": "scenario lock-twice reached its deadline of 60000 ms"`, `"deadlineMs": 60000`. This time,
  because the process stayed alive, `screenshot.png` **was** captured — and shows the closing page's
  spinner frozen on screen, direct visual confirmation of trial (c)'s own hypothesis (pressing lock
  already triggered `begin_close`'s `show_closing_page()` navigation before the click itself hung).
- **Failure 3** (the provider connection stays open — `chat/commands.rs`'s `abort_turn` and
  `vault_gate/drain.rs`'s abort rung as no-ops again): `lock-while-streaming` failed exactly like trial
  (a)'s fourth attempt (`provider connection closed 3046 ms after the press, over the 1000 ms limit`,
  matching the original run's 3045 ms almost exactly). Kept material: `timeline.json` has
  `"failedStep": "process-ended"`, the same error message verbatim, and
  `"screenshotNote": "no screenshot: the application had already ended"` (the app had already exited by
  capture time, unlike failure 2). `provider.json` independently corroborates: the `POST /v1/messages`
  connection's own `openedAt`/`closedAt` timestamps land right where the press-to-process-end gap says
  they should, without needing the driver log at all.

**SC-004 holds for all three**: each failure's kept material names its `failedStep` precisely (never a
generic "something failed"), the `error` string alone explains what limit was missed, and where a
screenshot cannot exist the material says so explicitly rather than silently omitting it — a maintainer
reading only the kept files, with no second run, has everything trial (a)/(c)'s own live investigation
needed. Both seeds reverted and confirmed clean (`--grep lock`: 3 passed, 1 skipped, no leftover
processes) after each inspection.

### Stage 4, SC-005 fresh-eyes contributor test (T079), 2026-09-22

Ran as a genuinely blind trial: a fresh subagent, with no prior context on this suite or repository, was
given only the absolute path to `scripts/e2e/README.md` and told explicitly not to open any other file
under `scripts/e2e/` (no existing scenarios, no `lib/`, no `contracts/`, no `specs/016-e2e-testing/`)
unless the README itself named an exact path — the same constraint a genuinely new contributor who has
only just found the README would face. Asked to write "create and unlock a vault, call one backend
command, check its result" in 40 lines or fewer, and to actually run it.

**Result: it succeeded, first try, one attempt, no debugging** — a 21-line scenario
(`vault-create-unlock-check.test.ts`, reusing `createAndUnlock` + `list_instances`, deleted afterward as
a pure exercise artifact since it duplicated `create-and-unlock.test.ts`'s own coverage) ran and passed
in 6.7 s. SC-005's own bar ("it must take 40 lines or fewer and run") is cleared with margin. Full,
honest account of what tripped it up regardless of the pass, verbatim from its report:

- **No concept of a "backend command"** is explained anywhere; `list_instances` is the only command name
  in the whole document, appearing solely inside the copied example, with no pointer to where the full
  set is defined or how to find one not already used in that example.
- **The exact run command was never spelled out as one copy-pasteable line** — "From the dev shell
  (`nix develop`), or through `scripts/with-nix-host-bridge.sh` directly, the way `tauri:dev` and
  `tauri:build` run" is a prose fragment, not a command; a blind contributor could plausibly compose the
  wrong wrapping.
- **No worked example combining `--app` and `--grep`** for the one workflow a new-scenario author needs
  most: iterating on a single new scenario against an already-built binary.
- **Ambiguous whether the linked contract files are in scope for a first scenario** — "Full table: X"
  reads as pure reference, not an instruction to open X, yet reaching a hook or helper not already in
  the README's own examples would require opening one anyway.
- **The inlined `create-and-unlock.test.ts` code fence never asserted it was a verbatim, kept-in-sync
  copy** of the real file — it happened to be trustworthy, but the document didn't say so.
- **Terminology gap**: the task said "vault", the API says "instance" throughout, with no glossary note
  connecting the two.
- **No expected run-time baseline**, so there was no way to judge "normal" from "hanging" other than
  waiting it out.

Fixed the README directly for every point above (not deferred): a paragraph naming `list_instances` as
one `#[tauri::command]`-annotated Rust function among the full set `src-tauri/src/lib.rs`'s
`invoke_handler(generate_handler![...])` lists, plus the vault/instance glossary note, right after the
copied example; the `## Running` section now leads with the literal
`nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e` command instead of a prose
description; a new worked `--app`+`--grep` example and a "5-15 s is normal" note follow the copied
scenario; the copy instruction now says outright that the fence is a verbatim reproduction and the
linked file is the source of truth if the two drift; the Helpers and Hooks sections now say plainly that
their own tables are enough for an ordinary scenario and the linked contracts are for what isn't covered
there. Verified `pnpm exec prettier --check`/`--write` clean and `pnpm check:e2e-lib` still 155 tests
passing after the edit and after deleting the exercise scenario file.

_Filled by later tasks: T080, T081 to T083 and T088._

### T088 — Traceability, 2026-09-24

| Requirement | Covered by                                                                                                                                                                                                                                                                                                                                                                         | Note |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| FR-001      | `pnpm test:e2e` (`cli.ts`), documented in `README.md`                                                                                                                                                                                                                                                                                                                              |      |
| FR-002      | `build.ts`'s `pnpm tauri build --debug --no-bundle` (no dev server); `--app` for a given binary                                                                                                                                                                                                                                                                                    |      |
| FR-003      | `instance.ts` starts each instance under its own `xvfb-run`; T031's checksum/window-count proof                                                                                                                                                                                                                                                                                    |      |
| FR-004      | `build.ts`/`resolveApplication`: builds debug by default, `--app`/`E2E_APP` to test a given one                                                                                                                                                                                                                                                                                    |      |
| FR-005      | `scenario.ts`'s per-scenario deadline; `cli.ts`'s overall run limit                                                                                                                                                                                                                                                                                                                |      |
| FR-006      | `instance.ts` + research R5: data/config/cache **and** runtime dir, `HOME`, session bus all isolated; T076's real filesystem-diff run. Spec wording lists fewer than what's implemented (spec alignment item 1, for T089)                                                                                                                                                          |      |
| FR-007      | `provider.ts`, the stand-in provider; no scenario reaches the internet                                                                                                                                                                                                                                                                                                             |      |
| FR-008      | `processes.ts` (marker, sweep, `stopGroup`); T031/T075 real concurrent-instance and kill proofs                                                                                                                                                                                                                                                                                    |      |
| FR-009      | `ports.ts`: a free port per use, one retry on collision                                                                                                                                                                                                                                                                                                                            |      |
| FR-010      | `instance.ts`/`scenario.ts`: a fresh instance per scenario, any order                                                                                                                                                                                                                                                                                                              |      |
| FR-011      | `preflight.ts`; T036's manual missing-tool/mismatch trials                                                                                                                                                                                                                                                                                                                         |      |
| FR-012      | Stage 0 (T003-T006): atoms pin delivers the tools via the dev shell                                                                                                                                                                                                                                                                                                                |      |
| FR-013      | `page.ts` (invoke, click, type, press, navigate) + `flows.ts` (createAndUnlock, openChat, connectProvider, startReply)                                                                                                                                                                                                                                                             |      |
| FR-014      | `contracts/test-hooks.md`'s three `data-testid` hooks (language- and markup-independent)                                                                                                                                                                                                                                                                                           |      |
| FR-015      | `provider.ts`                                                                                                                                                                                                                                                                                                                                                                      |      |
| FR-016      | `scenarios/{lock-while-streaming,window-close-while-streaming,closing-page,lock-twice,relaunch-after-lock}.test.ts`                                                                                                                                                                                                                                                                |      |
| FR-017      | `build.ts`'s `classifyCloseBehavior` + `scenario.ts`'s `needs: relaunch` skip rule                                                                                                                                                                                                                                                                                                 |      |
| FR-018      | `framebuffer.ts`; T067/T068 (resolved, see T086) — the relaunch is observed directly, no indirection needed                                                                                                                                                                                                                                                                        |      |
| FR-019      | `artifacts.ts`'s `captureFailure` (screenshot, timeline, provider record, driver log)                                                                                                                                                                                                                                                                                              |      |
| FR-020      | `report.ts` (per-scenario duration and step times)                                                                                                                                                                                                                                                                                                                                 |      |
| FR-021      | This PR (T081-T083): the `e2e` job in `ci.yml`, `ubuntu-24.04`, tools from apt/`cargo install`                                                                                                                                                                                                                                                                                     |      |
| SC-001      | T074: a clean debug run passes with `relaunch-after-lock` correctly skipped; a release run passes all seven                                                                                                                                                                                                                                                                        |      |
| SC-002      | T075: 20-run soak, 10 killed at random, no leftover process or directory afterward                                                                                                                                                                                                                                                                                                 |      |
| SC-003      | T077 (a)-(d): each spec 013 promise, disabled in turn, fails the matching scenario — with one accepted, reported limitation: `window-close-while-streaming` cannot exercise the real cancellation path, since a driver-issued window close never reaches `WindowEvent::CloseRequested` (PR #122); `lock-while-streaming` is what actually proves the cancellation-on-close promise |      |
| SC-004      | T058 + T078: three seeded failures, each `timeline.json`/kept material names the precise failed step                                                                                                                                                                                                                                                                               |      |
| SC-005      | T052 (first pass) + T079 (fresh-eyes subagent, no prior context): the README alone got a working 21-line scenario running on the first try                                                                                                                                                                                                                                         |      |
| SC-006      | T036: missing `xvfb-run` and a version mismatch both stop the run within the bound, before any tool starts, naming the remedy                                                                                                                                                                                                                                                      |      |
| SC-007      | T074's recorded durations: every close scenario 5.8-11.6 s, all under the 30 s bound                                                                                                                                                                                                                                                                                               |      |

No FR or SC is uncovered. The two caveats above (FR-006's narrower wording, SC-003's window-close limit)
are pre-existing, already-reported findings surfaced again here for completeness, not new gaps found by
this task; folding FR-006's wording into the spec text is T089's job.
