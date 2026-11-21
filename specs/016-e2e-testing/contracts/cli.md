# Contract: the command

**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md)

## Invocation

```text
pnpm test:e2e [options]
```

The package script runs `scripts/e2e/cli.ts` through `scripts/with-nix-host-bridge.sh`, the way
`tauri:dev` and `tauri:build` do, so the host's web view and graphics paths are set up. Outside the
Nix shell the bridge script runs the command unchanged.

## Options

| Option                            | Meaning                                                                                                          |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `--app <path>`                    | Test this built application and skip the build (FR-004). Typically a release build made for a release.           |
| `--close-behavior exit\|relaunch` | State what the given application does on close. Needed only when the path has neither `/debug/` nor `/release/`. |
| `--grep <text>`                   | Run only scenarios whose name contains the text.                                                                 |
| `--keep`                          | Keep the run directory's material for passing scenarios too.                                                     |
| `--scenario-timeout <seconds>`    | Per-scenario limit. Default 60.                                                                                  |
| `--run-timeout <seconds>`         | Whole-run limit. Default 1800 (600 until the multi-device scenarios of spec 033)                                 |

Environment: `E2E_APP` (same as `--app`), `E2E_ARTIFACTS_DIR` (run directory root),
`E2E_TIME_SCALE` (multiplies generic scenario and run timeouts, default 1; close-promise conformance
deadlines remain fixed, and a scaled run is marked non-conformant with its scale in the report).

## Order of work

1. Preflight (tools, driver and web view versions). Stops here on failure, having started nothing.
2. Sweep: stop and report processes with a marker whose owner is gone (leftovers of a killed run).
3. Build the debug application, unless `--app` or `E2E_APP` is given. Stops here on failure.
4. Run the scenarios one after another, each in its own isolated instance.
5. Sweep by this run's marker, print the summary, write `report.json`, exit.

Steps 4 and 5 also run when the run is interrupted or reaches its limit.

## Exit status

| Status | Meaning                                                                                     |
| ------ | ------------------------------------------------------------------------------------------- |
| 0      | No scenario failed. Skipped scenarios are listed with their reason and do not fail the run. |
| 1      | At least one scenario failed or timed out.                                                  |
| 2      | The preflight failed (FR-001, FR-011).                                                      |
| 3      | The build failed.                                                                           |
| 130    | Interrupted (Ctrl-C). Everything the run started was stopped first.                         |
| 143    | Terminated (SIGTERM). Same.                                                                 |

A run that hits `--run-timeout` stops the current scenario as a timed-out failure, skips the rest with
the reason "run time limit", and exits 1.

## Output

One line per scenario as it ends: status, name, duration, and for a failure the step that failed and the
directory holding the kept material. Then a summary: the application and its close behavior, counts of
passed, failed and skipped, each skip with its reason, and the run directory. The same information is in
`report.json` ([report.md](report.md)).

The application's own warnings on a virtual screen (for example that hardware acceleration is off) are
expected and are not shown as failures.

## Preflight messages

A message names the problem and the remedy, in one place:

- Missing tool: the tool's name, that the dev shell provides it once haex-vault's atoms pin is at or after the
  revision that delivers it, and the distribution package for a machine without Nix.
- Version mismatch: both versions and the fix (enter the shell again after updating the pin, or install
  the matching driver package).
- Undeterminable version: which source was missing.
- Close behavior unknown: how to state it with `--close-behavior`.
