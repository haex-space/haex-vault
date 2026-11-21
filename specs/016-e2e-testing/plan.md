# Implementation Plan: End-to-End Testing of the Real Application

**Branch**: `016-e2e-testing` (spec merged in #113; this plan is on `016-e2e-testing-plan`) | **Date**: 2026-09-21 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/016-e2e-testing/spec.md`, clarified on 2026-09-21 (the suite always builds and tests the debug build; a release build is tested only when one is given by path).

## Summary

A developer runs one command from the Nix dev shell, `pnpm test:e2e`. It checks the tools, removes leftovers of an earlier killed run, builds the debug application with its embedded interface (or takes one by path), and runs each scenario against a fresh, isolated instance: its own data, its own virtual screen, its own ports and a stand-in model provider. The first scenarios check what spec 013 promises: the lock control and the window close end the process within seconds and cancel a streaming reply, the closing page shows only a spinner, a lock pressed twice ends the process once, and (with a release build) a relaunch produces a new process.

The approach is the one the spike proved: `tauri-driver` and the WebKit driver talk W3C WebDriver to the real binary under Xvfb; backend commands are called from the page; a stand-in provider on loopback plays a running reply through the existing Anthropic-kind provider settings. Everything is TypeScript run directly by Node with its built-in test runner, so nothing is added to `package.json` dependencies. The application changes by three `data-testid` attributes. Two things the spike did not have are decided here, both from experiments (research R5, R8): the instance is cut off from the desktop's session bus, because otherwise the result depends on the maintainer's desktop settings and D-Bus activation leaves host services behind, and every process is identified by an environment marker rather than by name or path.

## Technical Context

**Language/Version**: TypeScript run by Node.js 22.19 or later with type stripping (`erasableSyntaxOnly`, as `tsconfig.scripts.json` already requires). The application is Rust with Tauri 2.11.5 and a Nuxt 4 / Vue 3 interface; only three attributes change there.

**Primary Dependencies**: None new in `package.json`. Node built-ins: `node:test`, `node:http`, `node:child_process`, `node:fs`, `node:net`, global `fetch`. Tools from the Nix dev shell: `tauri-driver` 2.0.6, `WebKitWebDriver` (same version as the host's `webkit2gtk-4.1`, currently 2.52.6), `xvfb-run`. Build: the existing `pnpm tauri` and the host bridge script `scripts/with-nix-host-bridge.sh`.

**Storage**: Files only. Per instance a private directory tree, removed at the end. Per run a directory `<target>/e2e/<run-id>/` for the report and the material kept for failures; version control already ignores `src-tauri/target/`.

**Testing**: Scenarios are `scripts/e2e/scenarios/*.test.ts` run by `node --test`. The helpers' own logic (stand-in provider, process marker and sweep, version comparison, report, deadlines) has fast checks in `scripts/e2e/lib/*.test.ts`, run by `pnpm check:e2e-lib` with no display and no application, and wired into the existing documentation job of CI.

**Target Platform**: Linux development machines (the maintainer's is CachyOS with the Nix dev shell), and separably `ubuntu-24.04` in CI. Windows, macOS and Android are out of scope.

**Project Type**: Desktop application (Tauri). The feature is tooling that lives in `scripts/`, plus three attributes in the interface.

**Performance Goals**: The five close scenarios plus the smoke scenario finish in under 5 minutes without the build (SC-001); each close scenario in under 30 seconds (SC-007); the preflight stops a bad run in under 10 seconds (SC-006). The spike measured a session start of about 3 seconds.

**Constraints**: No window on the desktop (FR-003); no read or write of the maintainer's data, home, runtime directory or session bus (FR-006, research R5); no network beyond loopback (FR-007); exact process identification (FR-008); no fixed sleeps; files under 500 lines; tests in dedicated files.

**Scale/Scope**: About 7 scenarios and 10 small library files. Scenarios run one after another, each with its own instance.

## Constitution Check

_GATE: passed before research; re-checked after design._ Sources: the project constitution (principles I to VIII, workflow) and the spaex behavior harness (`.spaex/constitution.md`).

| Rule                                                            | Status   | How                                                                                                                                                                                                             |
| --------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I No secrets in git                                             | Pass     | The vault passphrase and the provider key are generated for each run; no credential literal, not even a fixture, is committed.                                                                                  |
| II No local absolute paths                                      | Pass     | Paths come from `import.meta`, the repository root, the target directory and a temporary directory at run time. Docs and CI use relative paths and placeholders.                                                |
| III Device-independent identity                                 | N/A      |                                                                                                                                                                                                                 |
| IV Cross-repo references pin immutable revisions                | Pass     | haex-vault's atoms pin moves to the full commit `d5c48d0eb6662da84086bf69a3ca041a494e2b26` (Stage 0). `tauri-driver` is pinned by version and hash in the molecule and by `--locked --version` in CI.           |
| V External sources opt-in                                       | Pass     | No new source. The atoms repository is already in the allowlist; only its pinned revision moves, in a reviewed change.                                                                                          |
| VI Self-modifying instructions review-gated                     | Pass     | No skill, permission or constitution change. The regenerated `flake.nix` and generated files arrive through a pull request.                                                                                     |
| VII Relay unavailability never blocks local work                | Pass     | The suite needs nothing from the network at run time.                                                                                                                                                           |
| VIII No concealment in agent output                             | Pass     |                                                                                                                                                                                                                 |
| Tests in dedicated files                                        | Pass     | Scenarios and helper checks are `*.test.ts`; helpers are never in a test file and tests are never in a helper file.                                                                                             |
| 500-line boundary                                               | Tracked  | New files stay small. `src/pages/chat/[instance].vue` (1316 lines) gains two attribute lines: see Complexity Tracking.                                                                                          |
| Graphify before authoring named artifacts                       | Pass     | Consulted (research R9): the Rust `sse_body()` and wiremock fixtures and the `scripts/lib` harnesses were evaluated; none can be extended, none is duplicated.                                                  |
| `ponytail:` comments on deliberate shortcuts                    | Planned  | At the free-port choice, the `/proc` scan, the poll loops with fixed deadlines and the single-screen-per-instance choice.                                                                                       |
| One runnable check for non-trivial logic, with adopted tooling  | Pass     | `node:test` is already Node's; the helper checks are `lib/*.test.ts`.                                                                                                                                           |
| No `unwrap`/`expect` on I/O, do not discard original errors     | Pass     | Applied as its TypeScript equivalent: I/O failures are caught at the boundary that can report them, and the cause is kept in the message.                                                                       |
| ADR for decisions that materially affect a Core Principle       | N/A      | Nothing here changes a principle.                                                                                                                                                                               |
| Conventional Commits, no model references in commits or files   | Planned  | Applied when committing; no agent reference in any file, comment or trailer.                                                                                                                                    |
| Pull request into `main`, no squash                             | Planned  | Stages land as pull requests from topic branches.                                                                                                                                                               |
| Do not depend on wall-clock timing in tests (Rust testing rule) | Excepted | The elapsed time is what these scenarios check. Waits are polls with a deadline, never fixed sleeps; the 4 s, 1 s and 10 s conformance limits stay fixed, while only generic timeouts may use `E2E_TIME_SCALE`. |

Post-design re-check: no new violation. The one addition over the first pass is the isolation of the session bus and runtime directory (research R5), which strengthens FR-006 and adds no rule conflict.

## Project Structure

### Documentation (this feature)

```text
specs/016-e2e-testing/
├── spec.md
├── plan.md              # This file
├── research.md          # Decisions and the experiments behind them
├── data-model.md        # Entities the suite handles at run time
├── quickstart.md        # How to run and how to prove the success criteria
├── contracts/
│   ├── cli.md           # The command, its options, exit statuses
│   ├── helpers.md       # What a scenario can call
│   ├── stand-in-provider.md
│   ├── report.md        # Report, timeline and kept material
│   └── test-hooks.md    # The controls a scenario may reach and how
├── checklists/requirements.md
└── tasks.md             # Created by /speckit-tasks
```

### Source Code (repository root)

```text
scripts/e2e/
├── cli.ts                          # pnpm test:e2e: options, preflight, sweep, build, run, summary, cleanup
├── README.md                       # Contributor guide: run, write a scenario, hooks (SC-005)
├── lib/
│   ├── preflight.ts                # Tools and driver/web-view version check
│   ├── build.ts                    # Build the debug app or take --app; close behavior
│   ├── processes.ts                # Run marker, /proc scan, sweep, stop process groups
│   ├── ports.ts                    # A free port
│   ├── webdriver.ts                # Minimal W3C client over fetch
│   ├── instance.ts                 # Isolated instance: directories, environment, screen, driver, session
│   ├── provider.ts                 # Stand-in provider
│   ├── page.ts                     # Interaction helpers: invoke, click, type, press, navigate, close window
│   ├── flows.ts                    # createAndUnlock, openChat, connectProvider, startReply
│   ├── scenario.ts                 # scenario(): context, deadline, timeline, skip rule, failure hook
│   ├── artifacts.ts                # Material kept for a failed scenario
│   ├── framebuffer.ts              # Reads the virtual screen's XWD image (relaunch check)
│   ├── report.ts                   # report.json and the printed summary
│   ├── close-promises.ts           # The numbers from spec 013: 4 s, 1 s, 10 s
│   ├── fake-driver.testlib.ts      # Test-only fake of the WebDriver server
│   └── *.test.ts                   # Fast checks of the above, no display, no application
└── scenarios/
    ├── smoke-start.test.ts
    ├── create-and-unlock.test.ts   # The SC-005 example, kept as the documentation's model
    ├── lock-while-streaming.test.ts
    ├── window-close-while-streaming.test.ts
    ├── closing-page.test.ts        # Light and dark
    ├── relaunch-after-lock.test.ts # Needs a relaunching build; skipped otherwise
    └── lock-twice.test.ts

package.json                        # + "test:e2e", "check:e2e-lib"
.github/workflows/ci.yml            # + helper checks in the documentation job; + optional e2e job (Stage 5)
.spaex/manifest.json, flake.nix, .devshell/packages.nix, .spaex/generated/nix-packages.json
                                    # Stage 0: pin bump and the files spaex install delivers

src/components/onboarding/InstancesList.vue      # + data-testid, data-instance-name
src/components/workspace/ChatFab.vue             # + data-testid
src/pages/chat/[instance].vue                    # + data-testid on the two lock buttons
```

**Structure Decision**: Tooling lives in `scripts/e2e/`, beside the existing `scripts/check-*.ts` and `scripts/lib/`, and is type-checked by the existing `tsconfig.scripts.json`, which already includes `scripts/**/*.ts`. Library files are split by reason to change (tools, process control, driver protocol, instance, provider, scenario shell, report), each well under 500 lines. Scenarios are one file per behavior so a failing scenario is one name in the report.

## Delivery Stages

Each stage is a pull request that can be reviewed and used on its own. Task numbering and detail come with `/speckit-tasks`.

| Stage | Delivers                                                                                                                                                                                                                                                                                                                             | Stories and requirements                                   |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| 0     | Move haex-vault's atoms pin to `d5c48d0eb6662da84086bf69a3ca041a494e2b26`, run `spaex install`, commit the delivered `flake.nix`, `.devshell/packages.nix` and generated files. Check `tauri-driver`, `WebKitWebDriver` and `xvfb-run` are on `PATH` in the shell.                                                                   | FR-012                                                     |
| 1     | The command: options, preflight with the version check, orphan sweep, build or `--app`, cleanup on every ending, exit statuses, summary. The process marker, ports, W3C client, isolated instance and the core of the scenario wrapper (deadline, teardown, skip rule). One smoke scenario. Helper checks and `check:e2e-lib` in CI. | US1, US5; FR-001 to FR-005, FR-008 to FR-011               |
| 2     | The interaction helpers and flows; the stand-in provider; the three hooks; `create-and-unlock` as the model scenario; `scripts/e2e/README.md`; failure material, the timeline and the report's step times.                                                                                                                           | US3, US4; FR-006, FR-007, FR-013 to FR-015, FR-019, FR-020 |
| 3     | The close scenarios: lock while streaming, window close, closing page (light and dark), lock twice; then the relaunch scenario, which starts with the two checks the plan leaves open (a release build can be driven; the relaunch window can be observed).                                                                          | US2; FR-016 to FR-018                                      |
| 4     | Proof of the success criteria: 20 runs with 10 killed (SC-002), each promise broken on purpose (SC-003), three seeded failures (SC-004), a contributor writes the SC-005 scenario, a missing tool and a version mismatch (SC-006).                                                                                                   | SC-001 to SC-007                                           |
| 5     | The optional CI job on Ubuntu, publishing the run directory on failure. Separable; nothing above waits for it.                                                                                                                                                                                                                       | US6; FR-021                                                |

Stage 3's relaunch scenario can land in a later pull request than the other four if its two open checks take time; the other scenarios do not depend on it.

## Risks

| Risk                                                                            | Effect                                      | Mitigation                                                                                                                                                                                         |
| ------------------------------------------------------------------------------- | ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A release-profile binary cannot be driven (webview automation)                  | The relaunch scenario cannot run at all     | First check of the relaunch scenario (T067). **Resolved 2026-09-22: it can** — research R11.                                                                                                       |
| The relaunched window cannot be observed from outside (framebuffer route fails) | FR-018's last clause is met only indirectly | Research R11 (T068). **Resolved 2026-09-22: it can** — `-fbdir` observes the relaunch directly; all four steps ship, no indirection needed.                                                        |
| Timing assertions flake on a loaded machine                                     | The suite is switched off                   | Polls with deadlines; conformance checks keep the 4 s, 1 s and 10 s promises fixed, while diagnostic runs may scale generic timeouts and report themselves non-conformant with the scale (FR-020). |
| Debug build time in CI                                                          | A slow job                                  | Separate, optional job; caches shared with the Rust job; Stage 5 is separable.                                                                                                                     |
| Driver and webview drift after a system update                                  | Failures that look like application bugs    | The preflight compares versions on every run and prints both (FR-011, SC-006).                                                                                                                     |
| Free-port race                                                                  | A driver fails to start                     | One retry with a new port; the failure names the port (`ponytail:` at the code).                                                                                                                   |

## Complexity Tracking

| Item                                                                                      | Why needed                                                                                                                                                  | Simpler alternative rejected because                                                                                                                                                                                                       |
| ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Two attribute lines added to `src/pages/chat/[instance].vue`, which is far over 500 lines | The lock control is only in this file and FR-014 needs a hook that does not depend on text or language. Attributes only; no behavior or appearance changes. | Finding the button by its icon class or its accessible name depends on generated markup or on the interface language. Extracting a lock component would shrink the file but is a refactor of a file whose split belongs to its own change. |
| Wall-clock assertions (4 s, 1 s, 10 s)                                                    | The elapsed time is the promise under test.                                                                                                                 | A test that does not measure time cannot show "ends within seconds". Polls with fixed conformance deadlines keep the promise measurable; generic diagnostic timeouts may still be scaled.                                                  |
| One virtual screen and one driver per instance, started for each scenario                 | Key Entities and FR-010 ask for isolated instances; cost is well under a second each.                                                                       | A shared screen couples scenarios and blocks running them in parallel later.                                                                                                                                                               |

## Spec alignment items

The spec is merged; these were differences found while planning. Folded in via `/speckit-analyze`
(T089, 2026-09-24):

1. **Resolved by a spec change.** FR-006 listed only data, configuration and cache; the runtime
   directory, the home directory and the session bus are isolated too (research R5, experiments E1 to
   E4). FR-006 now names all of them.
2. **Resolved, no spec change needed.** FR-018 asked that the relaunched window be shown to display the
   unlock screen. T067/T068 (research R11) resolved this fully: a release binary can be driven and the
   relaunch is observed directly via the virtual screen's framebuffer, all four steps ship — the spec's
   wording already matched what got built once the open risk closed.
3. **No spec change needed.** User Story 2, scenario 3 says the closing page shows no text and one
   spinner on a dark and a light scheme — a claim about what the page shows, not about when it is
   observed. It cannot be judged during a real close, which lasts milliseconds; the scenario loads the
   page directly and checks exactly that claim (research R10).
4. **Resolved by a spec change.** User Story 2, scenarios 1 and 2 say "no error appears". Added as an
   Assumptions entry: checked from the last page sample before the session ends plus the stronger facts
   (process ended, provider connection closed), not by continuous observation.
