# End-to-end tests

Starts the real, built Holzi binary under a virtual screen, drives it through `tauri-driver` the way a
user does, and checks what it does — without opening a window on your desktop and without touching your
own Holzi data. Background and design decisions: [`specs/016-e2e-testing/`](../../specs/016-e2e-testing/).

## Running

```sh
nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e
```

(`pnpm test:e2e` alone works too, from inside a `nix develop` shell already wrapped the same way, the
way `tauri:dev` and `tauri:build` run.) This builds the debug application, starts a virtual screen per
scenario, runs every `scripts/e2e/scenarios/*.test.ts` file, prints a summary, and cleans up.

Common options:

| Option          | Meaning                                                                |
| --------------- | ---------------------------------------------------------------------- |
| `--app <path>`  | Test this already-built application instead of building one.           |
| `--grep <text>` | Run only scenarios whose name contains the text.                       |
| `--keep`        | Keep the run's material (screenshots, logs) for passing scenarios too. |

`--app` is for a release build: the ordinary run always builds and tests the debug profile (clarified in
`specs/016-e2e-testing/spec.md`); pointing `--app` at a release binary is the only way this suite tests
one. The full option and exit-status list is in
[`specs/016-e2e-testing/contracts/cli.md`](../../specs/016-e2e-testing/contracts/cli.md).

## The tools

`tauri-driver`, `WebKitWebDriver` and `xvfb-run` come from the Nix dev shell (holzi's atoms pin,
`.devshell/packages.nix`). A missing tool, or a driver whose WebKit version does not match the host's
`webkit2gtk-4.1`, stops the run before anything starts, with a message naming the tool or the versions
and the fix.

## Writing a scenario

Copy [`scenarios/create-and-unlock.test.ts`](scenarios/create-and-unlock.test.ts), reproduced verbatim
below so this section stays useful on its own — but the linked file is the source of truth if the two
ever drift:

```ts
import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'

scenario('create-and-unlock', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  const name = 'e2e-test'

  await createAndUnlock(instance, { name })

  const result = await instance.invoke('list_instances')
  if (!('ok' in result) || !result.ok) {
    throw new Error(`list_instances failed: ${JSON.stringify(result)}`)
  }
  const names = (result.data as Array<{ name: string }>).map((i) => i.name)
  assert.ok(
    names.includes(name),
    `expected "${name}" among ${JSON.stringify(names)}`,
  )
  ctx.step('checked')
})
```

The file name and the string passed to `scenario(...)` must match (`pnpm test:e2e` checks this before it
starts anything). `node --test scripts/e2e/lib/*.test.ts` (`pnpm check:e2e-lib`) does not run scenario
files; only `pnpm test:e2e` does, since a scenario needs the built application and the tools.

`list_instances` above is one _backend command_ among others — a Rust function marked
`#[tauri::command]` and listed in `invoke_handler(generate_handler![...])` in `src-tauri/src/lib.rs`;
that list names every command a scenario can call through `instance.invoke`. A "vault" is called an
_instance_ everywhere in this API (`ctx.startInstance`, `list_instances`, `createAndUnlock`) — there is
no separate "vault" type to look for.

To run only your new scenario against the application you already have built, without rebuilding:

```sh
nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --app <path to a built binary> --grep <your scenario's name>
```

A simple scenario like the one above typically finishes in 5-15 s; if it is still running well past
that, something is stuck rather than merely slow.

## Scenarios with several vaults and users

A scenario for the sync (`sync-*.test.ts`), for spaces or for anything else that needs more than one
vault or device asks the context for a _group_. Every device is an application process with data of its
own; all of them share one Nostr relay (`e2e_nostr_relay`, a binary of the main package behind the Cargo
feature `e2e`, built before the scenarios start). The whole of it is
[`scenarios/sync-two-users.test.ts`](scenarios/sync-two-users.test.ts), under 50 lines:

```ts
import { scenario } from '../lib/scenario.ts'
import { expectOnline } from '../lib/group-expect.ts'
import { addThread, expectThreads } from '../lib/sync-flows.ts'

scenario('sync-two-users', { timeoutMs: 480_000 }, async (ctx) => {
  const g = await ctx.group({
    users: { anna: ['laptop', 'phone'], ben: ['desktop', 'tablet'] },
  })
  const [laptop, phone] = [g.device('anna/laptop'), g.device('anna/phone')]
  await phone.pubkey() // read while it runs: a stopped device cannot be asked
  await phone.stop()
  await expectOnline(ctx, laptop, phone, false, 60_000) // laptop lists phone as not online
  await addThread(laptop, 'Annas Chat')
  await phone.start()
  await expectThreads(
    ctx,
    phone,
    ['Annas Chat'],
    "Anna's chat to reach her phone",
  )
})
```

(An excerpt: it leaves out Ben's part, which checks that his vault never holds Anna's chat.) The
second argument of `scenario` takes `timeoutMs`, the most the scenario may run; a group of four devices
needs a few minutes just to start, so give such a scenario 300 to 600 s. A user may have a single device.

Each user has one vault (`e2e-<user>`). The first device of a user creates it; the others are linked the
way a person does it, with a code from a main device (`{ name: 'desk', main: true }` links a second main
device). Devices are named `<user>/<device>`, and a device name is unique in the whole group. More devices
than `E2E_MAX_DEVICES` (default 6) are refused before anything starts, because each device is an
application, a virtual screen and a driver. Everything that was started ends with the scenario, in
reverse order, whether it passed, failed or timed out; a failure keeps screenshot, driver log and data of
every device in a folder of its own.

On a device (`g.device('anna/laptop')`):

| Call                                                | Effect                                                                                                        |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `device.page`                                       | The same surface as an instance: `click`, `type`, `invoke`, `exec`, `waitForDisplayed`, `screenshot`, …       |
| `device.stop()` / `.start()` / `.restart()`         | Close the application gracefully / open it again over the same data / both.                                   |
| `device.kill()`                                     | End it without its own shutdown; the data stays.                                                              |
| `device.lock()`                                     | Press the lock control and wait for the process to end; resolves with the time from the press to the end.     |
| `device.goOffline()` / `.goOnline()`                | Run without any server, and back; devices it knew can still reach it directly. Both restart (research R2).    |
| `device.setServers(servers)`                        | The server lists of the vault; they apply at the next opening.                                                |
| `device.deviceList()` / `.status()` / `.identity()` | The rows of `list_vault_devices`, `sync_status` and the public vault identity, through the interface.         |
| `device.copyVaultTo(name, { user? })`               | Copy the vault file of a stopped device to a new, stopped device; the copy of a main device is a main device. |

On the group: `g.device(address)`, `g.relay.stop()` / `g.relay.start()` (the relay goes away and comes back
on the same address), `g.link(host, name, { main? })` (a device linked later through a main device) and
`g.addDevice(user, name)` (a device without a vault that the scenario links itself, for instance through
the form on the start page).

Waiting (`lib/group-expect.ts`, `lib/sync-flows.ts`): `expectOnline`, `expectLastSeen`, `expectRole` and
`expectThreads` wait with a fixed deadline that `E2E_TIME_SCALE` does not stretch, because they wait for
a promise of the product (40 s to sync, 60 s to notice a device that is gone) and not for a slow machine.
Write, read and count the data with `addThread`, `addThreads` (many at once, in a loop on the page),
`threadTitles` and `threadCount`; the chat threads stand for the vault's data until other data kinds
exist. What a person does in the device view, the link form and the server lists is in
[`lib/sync-ui.ts`](lib/sync-ui.ts), by the hooks of
[`contracts/test-hooks.md`](../../specs/016-e2e-testing/contracts/test-hooks.md).

The helpers, all `async`, in the order of their arguments:

| Helper                                                    | From               | Meaning                                                                                                              |
| --------------------------------------------------------- | ------------------ | -------------------------------------------------------------------------------------------------------------------- |
| `expectOnline(ctx, observer, target, online, timeoutMs?)` | `lib/group-expect` | Wait until the device list of `observer` shows `target` online (`true`) or not online (`false`).                     |
| `expectLastSeen(ctx, observer, target, { withinMs })`     | `lib/group-expect` | Wait until `observer` shows `target` not online with a last-seen time that recent.                                   |
| `expectRole(ctx, device, role)`                           | `lib/group-expect` | Wait until the device reports `main`, `linked`, `awaiting_admission` or `removed` of itself.                         |
| `expectThreads(ctx, device, titles, description)`         | `lib/sync-flows`   | Wait until the device holds exactly these thread titles (any order); `description` names what is awaited on failure. |
| `addThread(device, title)` / `addThreads(device, titles)` | `lib/sync-flows`   | Make threads on the device (the second in a loop on the page, for thousands); works with no server reachable.        |
| `threadTitles(device)` / `threadCount(device)`            | `lib/sync-flows`   | The sorted titles / the number of threads the device holds now. No waiting.                                          |

What the group's relay does: `g.relay.stop()` and `g.relay.start()` take the Nostr relay away and bring it
back on the same address. The relay is how devices _find_ each other; a pair that is already connected
stays connected (the connection is direct), and a device that starts while the relay is away finds nobody
until it is back, and then finds the others on its own, usually within seconds. `device.goOffline()` is
not unreachable: on one machine a device without servers still dials the devices it knew at their last
address. To make a device unreachable while others keep running, and to see a device notice that another
one has gone, stop or kill that one. Both take a while: a device that was ended cleanly is listed as gone at
once, one that was killed or cut off within about 35 s, and the promise is 60 s.

Nothing arrives is a claim without an event to wait for. Wait until a device that _should_ get the thing
has it (it was sent by then), then check the other with `threadTitles`; if that is not possible, a fixed
wait of a few seconds with a comment saying so is the honest choice.

A scenario never names a platform: it imports none of `instance.ts`, `processes.ts`, `webdriver.ts`,
`build.ts`, `node:child_process`, `node:os` or `node:fs`, never calls `process.kill` and never writes
`xvfb`, `tauri-driver` or `/proc`. `pnpm check:e2e-lib` fails on it. Everything platform specific lives
in [`lib/platform/`](lib/platform/) behind the `DeviceHost` interface; what other platforms would need is
in [`PLATFORMS.md`](PLATFORMS.md).

## Helpers

The tables below are enough to write an ordinary scenario; open
[`contracts/helpers.md`](../../specs/016-e2e-testing/contracts/helpers.md) only for a promise not
covered here (edge cases, exact error shapes).

On the context (`ctx`, the argument to a scenario's body):

| Member                     | What it does                                                                                  |
| -------------------------- | --------------------------------------------------------------------------------------------- |
| `ctx.startInstance(opts?)` | A fresh, isolated running instance. Ended for you when the scenario ends.                     |
| `ctx.provider(behavior?)`  | A stand-in model provider (see below). Ended with the context.                                |
| `ctx.group(spec)`          | Users with devices, see [several vaults and users](#scenarios-with-several-vaults-and-users). |
| `ctx.step(name, detail?)`  | Adds a timeline entry, for the report and for diagnosing a failure without a second run.      |
| `ctx.waitFor(desc, pred)`  | Polls until `pred()` is truthy or the deadline passes. Use this, never a fixed sleep.         |
| `ctx.credentials()`        | A generated passphrase and provider key, so none is ever committed.                           |

On an instance:

| Member                                       | What it does                                                                                                                                                                     |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `instance.invoke(command, args?, opts?)`     | Calls a backend command. `{ ok: true, data }`, `{ ok: false, error }`, or with `opts.expectEnd`, `{ ended: true }` for a call that gets no answer because the application ended. |
| `instance.click(hook)` / `.type(hook, text)` | Reach a control by hook ([below](#hooks)); use the displayed one if several match.                                                                                               |
| `instance.press(hook)`                       | Click a control that may close or navigate away, without waiting for a reply. Resolves with the time the click was sent, for a deadline measured from the press.                 |
| `instance.exec(script, args?)`               | Runs a script on the page, for assertions on what it shows.                                                                                                                      |
| `instance.screenshot(name)`                  | Saves a PNG in the scenario's kept material.                                                                                                                                     |
| `instance.waitForEnd(deadlineMs)`            | Resolves with the time until the application process ends, or fails at the deadline.                                                                                             |
| `instance.markedProcesses()`                 | This run's processes running the application, for the relaunch and lock-twice checks.                                                                                            |

Convenience built on the above, in [`lib/flows.ts`](lib/flows.ts):

- `createAndUnlock(instance, { name })` — create a vault, unlock it, wait for the workspace.
- `openChat(instance)` — open the chat from the workspace.
- `connectProvider(instance, provider)` — point the instance at a stand-in provider and load its model.
- `startReply(instance, provider, text)` — send a message; waits until the provider sees it arrive.

For the settings (spec 023), [`lib/settings.ts`](lib/settings.ts) adds `openSettings`, `waitForLocation`
(where the settings are, from the title's `data-location`), `choose` (an entry of a settings select),
`runAction` (a catalog action through the page, e.g. `wm.window.setGeometry`), `resizeSettingsWindow` and
contrast measurements for the colour scheme checks.

## The stand-in provider

`ctx.provider(behavior?)` starts a local server that plays an external model provider of the Anthropic
kind — no application change, since the app already accepts any base URL for that kind of provider. Full
contract: [`contracts/stand-in-provider.md`](../../specs/016-e2e-testing/contracts/stand-in-provider.md).

```ts
const provider = await ctx.provider({ kind: 'stream-forever' }) // or 'stream-then-finish', 'error'
provider.behave({ kind: 'stream-then-finish', chunks: 3 }) // changes what the next request gets
await provider.waitForOpen() // resolves once a POST /v1/messages connection is open
provider.connections() // { id, openedAt, closedAt?, method, path }[]
```

`closedAt` is set from the socket's own close, so a reply the application cancelled shows up as a closed
connection, not as one that quietly finished.

## Hooks

Every control a scenario needs is reachable without depending on displayed text or the interface
language (the interface is German by default). Open
[`contracts/test-hooks.md`](../../specs/016-e2e-testing/contracts/test-hooks.md) for the full,
authoritative list of existing hooks before adding a new `data-testid`, so you don't duplicate one that
already exists.

```ts
instance.click('lock-instance') // [data-testid="lock-instance"]
instance.click('#unlock-passphrase') // a selector starting with # . [ is used as is
```

A hook without a selector character is a `data-testid`. Add one only when a scenario needs it and
nothing else reaches the control (a `data-testid` and, where useful, a data attribute carrying the
control's own data — never its displayed text).

## Rules for scenarios

- Reach controls by hook only, never by displayed text or the interface language.
- Do not rely on a keyboard submit; click the button (pressing Enter did not submit the unlock form).
- Do not wait for the reply of a call that closes the application; pass `expectEnd`.
- Do not use a fixed sleep; use `ctx.waitFor` or the polling helpers above.
- Do not start a process outside the context, so the cleanup rules hold.
- A scenario that needs a relaunching build declares it: `scenario(name, { needs: { closeBehavior: 'relaunch' } }, …)`.
  It is skipped, not failed, against a build that does not relaunch.

## How the command and scenario files talk

`pnpm test:e2e` sets the environment before it spawns `node --test` over the scenario files:
`E2E_RUN_DIR`, `E2E_APP`, `E2E_CLOSE_BEHAVIOR`, `E2E_TOOLS` (JSON), `E2E_SCENARIO_TIMEOUT_MS`,
`E2E_TIME_SCALE`, `E2E_KEEP`, `HOLZI_E2E_RUN` (the run's process marker). `scripts/e2e/lib/scenario.ts`
reads it once, in `readEnv()`. A scenario file is not meant to be run any other way.

`local-model-answer` runs a real model (Qwen3 0.6B, about 480 MB): `lib/local-model.ts` fetches it once,
pinned and checked by its SHA-256, into `~/.cache/holzi-e2e/models` (or `E2E_MODEL_CACHE`).

## The run directory

`<CARGO_TARGET_DIR or src-tauri/target>/e2e/<run-id>/` (moved with `E2E_ARTIFACTS_DIR`; already outside
version control). A passing run leaves only `report.json` and, if it built the application, `build.log`.
A failed or timed-out scenario also leaves its own directory: `timeline.json`, `screenshot.png` (unless
the application had already ended), `driver.log`, `provider.json`. `--keep` keeps a passing scenario's
directory too. Full shape: [`contracts/report.md`](../../specs/016-e2e-testing/contracts/report.md).

## Troubleshooting a leftover run

Every process the suite starts carries `HOLZI_E2E_RUN` in its environment, so a killed run's leftovers
are found and removed by the next one automatically — never by matching a process by name or path, so
your own Holzi is never touched. To check by hand:

```sh
grep -l HOLZI_E2E_RUN /proc/*/environ 2>/dev/null
```

Anything this prints beyond the current run is a leftover; the next `pnpm test:e2e` sweeps and reports it
before it builds or runs anything.
