# Quickstart: validating Vault Lifecycle Isolation

**Feature**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

A run guide for proving the feature end to end. Contracts are in [contracts/](contracts/) and the
runtime entities in [data-model.md](data-model.md); they are not repeated here.

## Prerequisites

- The worktree for branch `013-vault-lifecycle-isolation`, entered through `nix develop`, with a real
  `pnpm install` (do not symlink `node_modules`).
- For Rust: the host bridge wrapper, `nix develop --command scripts/with-nix-host-bridge.sh cargo ...`.
- For the manual scenarios: a scratch vault (two if possible), one small local model or a connected
  provider, and a shell to start the app twice.

## Automated checks (CI parity)

| Command                                                            | Proves                                                 |
| ------------------------------------------------------------------ | ------------------------------------------------------ |
| `cargo test --manifest-path src-tauri/Cargo.toml`                  | Gate, drain ladder, gateway, close protocol, secrets   |
| `cargo fmt --check` and `pnpm lint:rust`                           | Formatting and clippy in both feature sets             |
| `pnpm check:chat-state`                                            | Frontend replay cases (closing, passphrase lifetime)   |
| `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm check:templates` | Types and templates                                    |
| `pnpm lint`, `pnpm format:check`                                   | Lint and Prettier (covers this `specs/` folder)        |
| `git checkout -- src/types/bindings/` before committing            | Drops the trailing-whitespace churn `cargo test` makes |

## Scenarios

Each scenario names the spec items it covers. Steps are observable actions, not code.

### 1. Close while a reply streams and a download runs (US1, FR-001..FR-003, SC-001, SC-002)

1. Unlock a vault, start a long reply, start a model download.
2. Press the lock control.
3. Expect: the window shows only a spinner within a moment; the reply and the download stop; no
   further text appears; within about 4 seconds the app process has ended and (release build) the
   unlock screen is back. No error is shown and nothing needs to be pressed twice.
4. While the reply runs, have the agent start a long command through the shell tool, for example
   `sleep 300`. After the close, no such process remains: `pgrep -f "sleep 300"` prints nothing.
5. After the relaunch, open the vault: the interrupted reply is not shown as complete, and no
   partial model is listed as installed (FR-008).

### 2. Close cannot be refused (FR-002)

Repeat scenario 1 and press the control repeatedly, then close the window during the grace period.
Expect one consistent ending and no "operation in progress" error.

### 3. Work that ignores cancellation (US1 scenario 3, SC-002)

Covered by an integration test with a task that never stops and a long blocking call. Expect the
drain outcome `Stuck` after about 3 seconds and the process ending anyway. Measure the real local
inference stop time once (research R5, **Open**) and record it in the task notes.

### 4. A new vault sees nothing of the old one (US2, SC-003)

1. In vault A, create a chat titled with a marker, type an unsent draft, choose a non-default model
   and an effort option, and trigger a visible error.
2. Close A, unlock vault B in the relaunched app.
3. Expect none of the markers anywhere: lists, titles, draft, selected model, effort, error text.
4. With A active, call `open_instance` for B from the developer tools. Expect `VaultAlreadyActive`
   and no change (FR-010).

### 5. Passphrase hygiene (US3, SC-004, SC-005)

- The Rust tests assert that `{:?}` and `{:#?}` of both argument types never contain the value, and
  that the passphrase type erases on drop.
- Manual: in a debug build with logging, unlock with a distinctive test passphrase, fail once, close.
  Search the terminal output and any log file for the passphrase. Expect zero hits.
- The replay test asserts the form field is cleared after a successful unlock and after the sheet is
  dismissed, kept after a failed attempt, and never present in any store.

### 6. Two app processes, two vaults (US4, FR-026, SC-006, SC-007, SC-010)

1. Start the app twice. Unlock vault A in the first and vault B in the second.
2. Start a reply in B. Close A. Expect B unaffected and its reply completing.
3. In the second process, try to unlock A (if still open) or, before closing, in a third start: expect
   the "open in another HaexVault instance" message and no data change.
4. Create a vault in one process, focus the other: the list shows it without a restart.
5. Install the same model in both at the same time: exactly one valid installation results.
6. While one process downloads a model and another creates a vault, start a further process (or
   close a vault so the app relaunches). Expect the download and the creation to finish normally and
   no `.pending` or `.part` file to be removed while they run.

### 7. Reads never fail while other work runs (US5, FR-025, SC-008)

Start a reply, leave to Settings and return to the chat view, and repeat while a model loads.
Expect no error banner and a correct effort control.

### 8. Ending the process by other means (FR-007)

Close the window and quit from the system menu while a reply runs. Expect the same cancellation and
bounded ending as scenario 1, with exit (no relaunch).

## Manual checks still open from research

Both resolved by T048/T049 (2026-09-24); see the Validation record below and research.md R2/R5.

| Check                                                       | Where recorded               |
| ----------------------------------------------------------- | ---------------------------- |
| Relaunch under `tauri dev` (does the dev runner re-attach?) | research R2, policy function |
| Local inference stop time after cancel                      | research R5                  |

## Validation record

Filled in by T048, T049 and T050 (2026-09-24), against a real debug binary driven under `xvfb-run` +
`tauri-driver` (scenarios 1, 2, 3) or a real `pnpm tauri:dev` session (the `tauri dev` check). Every
run's material (steps, timings) lives in `tasks.md`'s own Validation record; this table is the
pass/fail summary the task asks for.

| #   | Scenario                                                   | Result                 | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| --- | ---------------------------------------------------------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Close while a reply streams and a download runs            | Pass                   | Process ended 62 ms after press; provider closed 26 ms after press; no partial model after reopening (`list_installed_models` empty). The shell-tool/`pgrep` sub-check was not re-walked live — it relies on `ChildRegistry`'s own real-process integration tests (`vault_gate/drain_tests.rs`), already proven with spawned processes.                                                                                                                                                                                                                                                                                     |
| 2   | Close cannot be refused                                    | Pass                   | Repeated press + a window-close ~300 ms into the drain: one clean ending (1 ms after press — nothing was still running to drain), no error, no second process. The process routinely ends before a genuinely mid-drain window opens with nothing else running, so this does not exercise true interleaving; scenario 1 above is the case where real work (a stream) is actually draining.                                                                                                                                                                                                                                   |
| 3   | Work that ignores cancellation / local inference stop time | Pass                   | The `Stuck`-after-3s drain path is covered by an existing integration test (not re-run live). The local-inference measurement (T049): a real Qwen3 0.6B (Q4_K_M) generation, cancelled mid-stream, ended the process in 50 ms — see research.md R5.                                                                                                                                                                                                                                                                                                                                                                         |
| 8   | Ending the process by other means                          | Pass (mixed evidence)  | `take_over_exit` (`instances/close.rs`) is called unconditionally with `ClosePolicy::Exit` from both `WindowEvent::CloseRequested` and `RunEvent::ExitRequested` (`lib.rs`), independent of build type — confirmed by reading the code, not by a live relaunch-build test: a driver-issued window close does not reach this path at all (it destroys the window directly; spec 016's T077 follow-up, PR #122, documents this as an accepted tooling limitation). The bounded-ending half is covered live by spec 016's own `window-close-while-streaming` e2e scenario (part of the baseline `pnpm test:e2e` run, passing). |
| —   | `tauri dev` relaunch (research R2)                         | Resolved (not "works") | Forcing a relaunch under `pnpm tauri:dev` ends the whole dev session rather than the dev runner re-attaching — reproduced twice. `close_policy()` stays `Exit` for debug builds. Full detail in research.md R2 and tasks.md's T048 entry.                                                                                                                                                                                                                                                                                                                                                                                   |
