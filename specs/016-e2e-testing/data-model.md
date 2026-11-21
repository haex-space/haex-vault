# Data Model: End-to-End Testing of the Real Application

**Feature**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

The suite has no database. These are the values it holds while it runs and the files it writes. They
are the spec's Key Entities made concrete; the shapes that leave the process are in
[contracts/](contracts/).

## Application under test

| Field               | Type                                | Rule                                                                                                                                                                                                |
| ------------------- | ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `path`              | absolute path, resolved at run time | Must exist and be executable. Never written to a committed file.                                                                                                                                    |
| `source`            | `built` or `given`                  | `built` when the suite ran the debug build, `given` when `--app` was used.                                                                                                                          |
| `closeBehavior`     | `exit` or `relaunch`                | From `--close-behavior`, else from the path (`/debug/` is `exit`, `/release/` is `relaunch`), else the preflight fails. An explicit value that conflicts with `/debug/` or `/release/` is rejected. |
| `closeBehaviorFrom` | `flag` or `path`                    | Reported so a wrong value can be traced.                                                                                                                                                            |

## Tool check

| Field            | Type                           | Rule                                                                                                                                                                                                 |
| ---------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tools[]`        | `{ name, path?, ok, remedy? }` | Names: `tauri-driver`, `WebKitWebDriver`, `xvfb-run`, and the tool that gives the web view version. Missing means `ok: false` with a `remedy` that names the dev shell and the distribution package. |
| `driverVersion`  | `major.minor.patch` or absent  | Read from the file next to the driver, else from the installed package. Absent fails the check.                                                                                                      |
| `webviewVersion` | `major.minor.patch` or absent  | `pkg-config --modversion webkit2gtk-4.1`. Absent fails the check.                                                                                                                                    |
| `result`         | `ok` or `failed`               | `failed` if any tool is missing, a version is absent, or the two versions differ. Failing is a failing run (FR-001).                                                                                 |

## Scenario

| Field        | Type                                               | Rule                                                                                                      |
| ------------ | -------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `name`       | string, unique, the file's base name               | Shown in the report and used by the name filter.                                                          |
| `needs`      | optional `{ closeBehavior: 'relaunch' \| 'exit' }` | If the application's close behavior differs, the scenario is skipped with a reason (FR-017).              |
| `timeoutMs`  | number, default 60 000                             | Multiplied by `E2E_TIME_SCALE`. Reaching it fails the scenario after capturing failure material (FR-005). |
| `status`     | `passed`, `failed` or `skipped`                    | Skipped never counts as passed and is listed with `skipReason`.                                           |
| `steps[]`    | `{ name, atMs, detail? }`                          | `atMs` is measured from the scenario's start by one clock; key names below.                               |
| `durationMs` | number                                             | Start of the scenario to the end of its teardown.                                                         |

State: `pending` to `running` to `passed` or `failed`; `pending` to `skipped`. A scenario never returns
from a final state.

Key step names (FR-020): `instance-ready`, `unlocked`, `reply-streaming`, `press`, `provider-closed`,
`process-ended`, `relaunch-seen`.

## Isolated instance

| Field                 | Type                                | Rule                                                                                                                                                                                                                                                                                                                       |
| --------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `root`                | directory under the run's directory | Created empty at start, and removed by its owning teardown (FR-006). Holds `data`, `config`, `cache`, `run` and `home`; a root reused by a fresh instance stays available until that fresh instance has verified it.                                                                                                       |
| `env`                 | map                                 | `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR`, `HOME` inside `root`; `DBUS_SESSION_BUS_ADDRESS=disabled:`; `GDK_BACKEND=x11`; `HAEX_E2E_RUN`. Not passed on from the parent: `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, `XDG_STATE_HOME`, the session and desktop variables and `GTK_THEME`.    |
| `colorScheme`         | `light` or `dark`, default `light`  | Written as `gtk-3.0/settings.ini` in `config` before the start.                                                                                                                                                                                                                                                            |
| `ports`               | `{ driver, native }`                | Free when chosen; one retry with new ports if the driver does not listen.                                                                                                                                                                                                                                                  |
| `marker`              | `HAEX_E2E_RUN` value                | `<runner pid>:<runner start time>:<random>`. Identical for every process the run starts.                                                                                                                                                                                                                                   |
| `driverPid`, `appPid` | numbers                             | `appPid` is the marked process whose executable is the application under test, found after the session starts. It is the pid "the process ended" refers to.                                                                                                                                                                |
| `sessionId`           | string or absent                    | Absent before the session starts and after it ends.                                                                                                                                                                                                                                                                        |
| `reusesRoot`          | optional path                       | Lets a fresh instance start over an earlier instance's data (relaunch check, research R11). The earlier instance leaves the root in place when it stops; the fresh instance owns it through verification and its teardown removes it. If the fresh instance never takes ownership, the run-level cleanup removes the root. |

State: `created` to `starting` to `ready` to `ended` (the application ended by itself) or `stopped`
(the suite stopped it). A start that does not reach `ready` within its deadline is `stopped` and fails
the scenario. Teardown leaves the instance `ended` or `stopped`; ordinary teardown removes its root,
while teardown of an instance whose root is being reused transfers cleanup to the fresh owner described
above.

## Stand-in provider

| Field           | Type                                                                   | Rule                                                                                          |
| --------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `baseUrl`       | `http://127.0.0.1:<port>`                                              | Loopback only, port chosen by the operating system.                                           |
| `behavior`      | `stream-forever`, `stream-then-finish` or `error`, with its parameters | May be changed between requests.                                                              |
| `connections[]` | `{ id, openedAt, closedAt?, method, path }`                            | `closedAt` is set when the socket closes. Times come from the runner's clock.                 |
| `requests[]`    | `{ id, at, method, path, body? }`                                      | Credentials are never stored; only what a maintainer needs to see what the application asked. |

## Run

| Field          | Type                                                                    | Rule                                                                       |
| -------------- | ----------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `runId`        | timestamp plus random suffix                                            | Names the run directory.                                                   |
| `application`  | Application under test                                                  | Named in the report (FR-004).                                              |
| `toolCheck`    | Tool check                                                              |                                                                            |
| `scenarios[]`  | Scenario results                                                        | In execution order.                                                        |
| `artifactsDir` | path                                                                    | `<target>/e2e/<runId>/`; a passing scenario leaves nothing in it.          |
| `status`       | `passed`, `failed`, `preflight-failed`, `build-failed` or `interrupted` | `passed` only if no scenario failed and the preflight and build succeeded. |

## Files a run writes

```text
<target>/e2e/<run-id>/
├── report.json                   # always
├── build.log                     # only if the suite built the application
└── <scenario>/                   # only for a failed or timed-out scenario
    ├── timeline.json
    ├── screenshot.png            # if the application was still there
    ├── page-state.txt            # address, focus, open dialogs, visible test ids and text; the reason if not taken
    ├── logcat.txt                # phone scenarios only: the app's log lines, then the device's warnings
    ├── driver.log                # tauri-driver, the webview driver and the application, interleaved
    └── provider.json             # connections and requests
```

See [contracts/report.md](contracts/report.md) for the fields.
