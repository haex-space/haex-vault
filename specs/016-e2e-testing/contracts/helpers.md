# Contract: what a scenario can call

**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md)

Signatures show what exists and what it promises, not the code. The names are the intended shape and
may be adjusted while implementing, as long as the promises hold and the contributor guide
(`scripts/e2e/README.md`) is updated with them.

## Declaring a scenario

```ts
scenario(name: string, options: { needs?: { closeBehavior: 'exit' | 'relaunch' }; timeoutMs?: number },
         body: (ctx: ScenarioContext) => Promise<void>): void
```

- Each run of the body gets a fresh context; everything the context started is removed when the body
  ends, whatever its result (FR-013, User Story 3 scenario 1).
- If `needs.closeBehavior` differs from the application's, the body does not run and the scenario is
  reported as skipped with the reason (FR-017).
- If the body throws or reaches its deadline, the wrapper first captures the failure material and only
  then tears down ([report.md](report.md)).

## The context

| Member                                       | Promise                                                                                                                                                                                                                                                                                                                                                        |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ctx.app`                                    | The application under test: `path`, `closeBehavior`.                                                                                                                                                                                                                                                                                                           |
| `ctx.startInstance(opts?)`                   | Returns a running isolated instance with empty data of its own. Options: `colorScheme`, `reusesRoot`, `framebufferDir` (keeps the screen's current XWD image at `<framebufferDir>/Xvfb_screen0`, research R11; see `scripts/e2e/lib/framebuffer.ts`). The context ends it if the body does not.                                                                |
| `ctx.provider(behavior?)`                    | Starts a stand-in provider ([stand-in-provider.md](stand-in-provider.md)) and returns it. Ended with the context.                                                                                                                                                                                                                                              |
| `ctx.step(name, detail?)`                    | Adds a timeline entry with the time since the scenario's start.                                                                                                                                                                                                                                                                                                |
| `ctx.waitFor(description, predicate, opts?)` | Polls until the predicate is true or the deadline passes. On timeout the error names `description` and the last observed value, then what `opts.observe` returns (give it the values the predicate compared: a boolean alone says only `false`) and one line per page (the instance's, each group device's: address, focus, dialogs, toasts). No fixed sleeps. |
| `ctx.credentials()`                          | A passphrase and a provider key generated for this run.                                                                                                                                                                                                                                                                                                        |

## The instance

| Member                                   | Promise                                                                                                                                                                                                                                                    |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `instance.invoke(command, args?, opts?)` | Calls a backend command from the page. Resolves `{ ok: true, data }` or `{ ok: false, error }`. With `opts.expectEnd`, a call that gets no answer because the application ended resolves `{ ended: true }` instead of a timeout (User Story 3 scenario 2). |
| `instance.click(hook)`                   | Clicks the displayed control found by hook ([test-hooks.md](test-hooks.md)). If the control is not displayed after the deadline, fails naming the hook.                                                                                                    |
| `instance.type(hook, text)`              | Types into the control found by hook.                                                                                                                                                                                                                      |
| `instance.exec(script, args?)`           | Runs a script in the page. For assertions on what the page shows.                                                                                                                                                                                          |
| `instance.press(hook)`                   | Click by hook, scheduled inside the page so the call returns before the page is replaced; records the `press` step with the runner's clock.                                                                                                                |
| `instance.closeWindow()`                 | The WebDriver close-window command.                                                                                                                                                                                                                        |
| `instance.navigate(url)`                 | Loads a URL in the window (used for the closing page).                                                                                                                                                                                                     |
| `instance.screenshot(name)`              | Saves a PNG in the scenario's material and returns its path. Fails clearly if the application is gone.                                                                                                                                                     |
| `instance.alive()`                       | Whether the started application process (`appPid`) still exists.                                                                                                                                                                                           |
| `instance.waitForEnd(deadlineMs)`        | Resolves with the time from the call to the process's end, or fails at the deadline. Records `process-ended`.                                                                                                                                              |
| `instance.markedProcesses()`             | The run's marked processes whose executable is the application, with pids. For the relaunch and lock-twice checks.                                                                                                                                         |
| `instance.pid`, `instance.root`          | The application's original pid; the instance's directory.                                                                                                                                                                                                  |

## Convenience built on the above

| Member                                | Steps                                                                                                                                                                  |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `createAndUnlock(instance, { name })` | `create_instance` by backend call, click the instance entry by hook, type the passphrase, click the unlock button, wait for the workspace address. Records `unlocked`. |
| `openChat(instance)`                  | Click the open-chat hook, wait for the chat address.                                                                                                                   |
| `connectProvider(instance, provider)` | `add_provider` with the stand-in's base address and a generated key, then `load_model` for the stand-in's model. Returns the model id.                                 |
| `startReply(instance, text)`          | `send_message`; records `reply-streaming` when the provider has an open connection.                                                                                    |

## The close promises

`scripts/e2e/lib/close-promises.ts` exports the numbers a scenario asserts against: process end within
4 seconds, provider connection closed within 1 second of the press, relaunch within 10 seconds. They come
from spec 013 (drain ladder: 1 s cooperative, 3 s total, 0.5 s grace) and remain fixed for conformance.
`E2E_TIME_SCALE` may scale generic scenario and run timeouts only; a scaled run is non-conformant and
reports its scale.

## Rules for scenarios

- Reach controls by hook only, never by displayed text or the interface language.
- Do not depend on keyboard submit; click the button.
- Do not wait for the reply of a call that closes the application; declare `expectEnd`.
- Do not use a fixed sleep; use `waitFor`.
- Do not start a process outside the context, so the cleanup rules hold.
- A scenario that needs a relaunching build says so in `needs`.
