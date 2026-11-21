# Quickstart: validating End-to-End Testing

**Feature**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

A run guide for using the suite and for proving that it does what the spec says. The command's shape is
in [contracts/cli.md](contracts/cli.md), what a scenario can call in
[contracts/helpers.md](contracts/helpers.md); neither is repeated here. Until the stages are built, the
commands below describe the intended use.

## Prerequisites

- A worktree of this repository, entered through `nix develop`, with a real `pnpm install` (do not
  symlink `node_modules`).
- HaexVault's atoms pin at or after `d5c48d0eb6662da84086bf69a3ca041a494e2b26` (Stage 0), so the shell
  carries `tauri-driver`, `WebKitWebDriver` and `xvfb-run`. `command -v` for each should print a path
  inside the Nix store.
- The host's `webkit2gtk-4.1`, the same prerequisite as for `pnpm tauri:dev`. The driver and it must be
  the same version; the suite checks and says so if not.
- Nothing else: no display, no internet, no credentials.

## Everyday use

| Command                                        | What it does                                                                         |
| ---------------------------------------------- | ------------------------------------------------------------------------------------ |
| `pnpm test:e2e`                                | Preflight, build the debug application, run every scenario, print the summary.       |
| `pnpm test:e2e --grep lock`                    | Only scenarios whose name contains `lock`.                                           |
| `pnpm test:e2e --app <path to a built binary>` | Skip the build and test that binary, for example a release build made for a release. |
| `pnpm check:e2e-lib`                           | The helpers' own fast checks. No display, no application. Also run by CI.            |

In an ordinary run (debug build) the relaunch scenario is listed as skipped with its reason. To run it,
build the same release artifact used by T067 with `pnpm tauri build --no-bundle`, then pass
`<target>/release/haex` with `--app` (`<target>` is `CARGO_TARGET_DIR` when set, otherwise
`src-tauri/target`).

## Expected result of the first full run

- Every scenario prints one line; the close scenarios each finish in under 30 seconds.
- The relaunch scenario is skipped with the reason that the build exits on close.
- Exit status 0. No window appeared on your desktop. Read the run marker from the printed run directory's
  `report.json`, then verify that no process from this run remains:
  `marker=$(node --input-type=module -e 'import { readFileSync } from "node:fs"; console.log(JSON.parse(readFileSync(process.argv[1], "utf8")).runMarker)' <run-directory>/report.json); ! grep -l "HAEX_E2E_RUN=$marker" /proc/*/environ 2>/dev/null`.
  This ignores unrelated `tauri-driver` processes.

## Proving the success criteria

Each recipe names the criterion it proves. Record the outcome in the task notes.

### SC-001 and SC-007: one command, quick close scenarios

Run `pnpm test:e2e --app <a debug build already built>` and read the durations in the summary. All close
scenarios should be under 30 seconds and the run under 5 minutes.

### SC-002: nothing left behind, nothing touched

1. Start your own HaexVault (for example `pnpm tauri:dev`) and note the data directory's checksum listing.
2. Run the suite 20 times, in 10 of them sending `kill -9` to the runner at a random moment (a shell
   loop with `sleep $((RANDOM % 25))` before the kill).
3. After each run: no marked process is left (the next run's sweep prints nothing to remove, except right
   after a killed run, where it prints what it removed), no window appeared, your HaexVault is still running,
   and its data listing is unchanged.

### SC-003: each promise, broken on purpose

In a scratch change, disable one promise at a time, rebuild, run, and expect the covering scenario to
fail; restore and expect it to pass.

| Break                                            | Scenario that must fail                                |
| ------------------------------------------------ | ------------------------------------------------------ |
| The reply is not cancelled when the vault closes | `lock-while-streaming`, `window-close-while-streaming` |
| The page is not replaced by the closing page     | `closing-page`                                         |
| The process does not end                         | `lock-while-streaming`, `lock-twice`                   |
| A release build does not relaunch                | `relaunch-after-lock`                                  |

### SC-004: a failure explains itself

Seed three failures: an application that never starts (point `--app` at a script that sleeps), a process
that does not end after the lock control, a provider connection that stays open. For each, open the
material the summary names: the timeline should name the failing step without a second run.

### SC-005: a new scenario is short

Give a contributor who has not seen the suite `scripts/e2e/README.md` and ask for the scenario "create
and unlock a vault, call one backend command, check its result". It should take 40 lines or fewer using
only the helpers, and run in the suite.

### SC-006: an early, clear stop

- Hide a tool: `PATH` without `xvfb-run`. Expect exit status 2 within 10 seconds, a message naming the
  tool and the remedy, and no process started.
- A driver of another version: set up a `PATH` where `WebKitWebDriver` resolves to a build with a
  different version file. Expect exit status 2, both versions printed, and no application started.

## CI (Stage 5, optional)

The job's steps are the same command on `ubuntu-24.04` with the distribution's `webkit2gtk-driver` and
`xvfb`. To prove it: a change that breaks a scenario makes the job fail and uploads the run directory;
a clean change passes. The helper checks in the documentation job run without any of this.
