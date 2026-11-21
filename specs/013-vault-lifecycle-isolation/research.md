# Research: Vault Lifecycle Isolation

**Feature**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md) | **Date**: 2026-09-21

Every claim below was checked against the code or the pinned dependency sources of this branch
(Tauri 2.11.5, tokio-util 0.7.19, zeroize 1.9.0, haex-crdt at the pinned revision). Items that could
not be verified from source are marked **Open** and carry a manual check in
[quickstart.md](quickstart.md).

## R1 — What the code does today (baseline)

- `AppState.active_instance` is read raw in only four places: `state_utils.rs`
  (`active_database`), `instances/open.rs`, `instances/create.rs`, `instances/close.rs`. Every
  other command reaches the vault through `active_database` (11 files). Encapsulating the field is
  therefore mechanical.
- `close_instance` takes `acquire_operation()` first, so it fails with "operation still in
  progress" while a turn, load or transfer runs. It never calls `abort_turn`, and it resets only the
  loaded session, the whisper cache and the vault generation. `tool_registry`, the approval maps,
  `tool_cancellation` and `current_generation` are left as they are.
- `open_instance`/`create_instance` switch vaults atomically (spec 001 FR-022). When the requested
  vault is already active, `open_instance` mounts a **second read-only connection** only to check
  the passphrase (`open.rs`, around line 91).
- The frontend never resets a store. `lock()` (chat page) and `onLock()` (federation page) call
  `closeAsync()`, clear the active instance and navigate to `/`. (Since 2026-09-30 the lock lives in
  `useVaultLock`, called by the launcher's lock tile.)
- Only model-load events carry a vault generation.
- `haex-crdt::Database` is `Clone` over `Arc<DatabaseInner>` with one `Mutex<Connection>`. The
  `DatabaseConfig` (and its `SqlCipherKey`) is consumed by `Database::open`; no key is stored.
  `open_and_init_db` applies the key once with `pragma_update(None, "key", key)`.
- The log plugin is registered only under `cfg!(debug_assertions)` (`lib.rs`), so release builds
  write no log file.
- Local models load from a file path (`GgufModelBuilder::new(dir, [file])`, `llm/local/loader.rs`).

## R2 — Ending the process: relaunch, exit, and what a relaunch keeps

**Decision**: Ending the process uses `AppHandle::request_restart()` (default) or
`AppHandle::exit(0)` (policy `Exit`). Not `restart()`.

**Rationale** (Tauri 2.11.5 source):

- `restart()` on a non-main thread parks the calling thread forever (`sleep(Duration::MAX)`) while it
  waits for the exit event. Inside an async task that would burn a worker. `request_restart()` sets
  the same flag and returns.
- The relaunch is `process::restart(&env)`. It starts the current binary again with the same
  argument list (the first entry skipped), so the new process gets the **same arguments and the
  inherited environment**, which is what FR-005's "same configuration" needs. Under Linux with an
  AppImage, `current_binary` is the AppImage path, so the single file itself is relaunched (relevant
  for the follow-up feature 014).
- Both paths raise `RunEvent::ExitRequested` with `RESTART_EXIT_CODE` (`i32::MAX`) followed by
  `RunEvent::Exit`. The exit hook added for FR-007 must ignore requests that already carry that code
  or that arrive while the gate is already closing, or it would loop.

**Last resort** (operator, 2026-09-21): `AppHandle::exit` and `request_restart` only post the request
to the window event loop, and Tauri ends the process directly only when posting fails
(`app.rs`, `exit` and `request_restart`). If the event loop hangs, the process would stay alive with
the vault open. So about 0.5 s after the request, a plain thread ends the process forcibly:
`std::process::exit(0)` for `Exit` and `tauri::process::restart(&app.env())` for `Relaunch`
(`Manager::env` and `tauri::process::restart` are public API). The forced end skips destructors, so
SQLCipher does not overwrite its key blocks, but the operating system reclaims the memory; SQLite
recovers from its journal at the next open.

**Child processes** (PR review, 2026-09-21): a forced end also skips the `Drop`-based kills that stop
children today (`ChildLifecycle` in `cli_delegate/process.rs`, `kill_on_drop`). A child that ignored
the cancellation would then outlive the app and could keep running a tool after the close, against
FR-003 and the edge case that a running tool or child process is stopped with the session.
**Decision**: a `ChildRegistry` in the gate. Every child started for the vault (the tool shell in
`chat/tools/cli.rs`, delegated CLIs, MCP servers in `chat/tools/mcp.rs`) is its own process group and
is registered. The drain ladder calls `kill_all` when it aborts (a thread that waits for its child
cannot be aborted and would hold the drain to the limit) and on its other paths, and the forced end
calls it right before it ends the process. `kill_all` reuses what the code already does (`libc::kill(-pid, SIGKILL)` on
Unix, `taskkill /T /F` on Windows in `cli.rs`), so there is no new dependency. **Limit**: a
descendant that left its process group survives. **Upgrade path**: a Windows Job Object with
kill-on-close and `PR_SET_PDEATHSIG` on Linux, which would also cover a killed app; not adopted
because it needs platform-specific code and a new Windows dependency.

**Resolved** (T048, 2026-09-24): under `pnpm tauri:dev`, the dev runner **stops**. Forcing
`ClosePolicy::Relaunch` in a debug build (a scratch env-var check in `close_policy()`, removed
again after the test) and triggering it through the real UI (`xvfb-run` + `tauri-driver`, screen
captured via `Xvfb -fbdir` and read back as a converted PNG, input via `xdotool`) showed the same
outcome twice: the relaunched `target/debug/haex` process appears within milliseconds (a new pid,
a fresh GTK/MESA init line), but `cargo run`'s exit then makes the `tauri dev` watcher itself exit
(`XVFB-RUN-WRAPPER-EXIT-CODE: 0`, i.e. a clean, non-error exit) with no further Vite/Nitro output —
it does not re-attach to the new process or rebuild. The instance's on-disk data survived intact
across both runs (the relaunched app listed it as "Zuletzt verwendet"), so this is a dev-tooling
limitation, not a data-safety issue. Debug builds stay `Exit`, matching the pre-existing default; no
code change needed beyond confirming it.

**Alternatives considered**: `restart()` (blocks a thread); spawning our own child process (loses
Tauri's AppImage and macOS bundle handling); never relaunching (spec default is relaunch).

## R3 — The gateway: one place that every request passes

**Decision**: Wrap the generated invoke handler once in `lib.rs`:
`invoke_handler(gate.wrap(tauri::generate_handler![...]))`. `Builder::invoke_handler` takes any
`Fn(Invoke<R>) -> bool`, and `Invoke` exposes `message` (with the command name) and `resolver`. The
wrapper rejects with the typed error `VaultClosed` once the gate is closing, **except** for a small
allow-list of app-scoped commands. Default-deny means a command added later is gated without anyone
remembering to do so.

**Rationale**: It is the literal "gateway" the operator asked for, it is a single small function,
and it needs no change to the 50-odd commands. The spike proved a custom `CommandArg` extractor also
works, but an extractor must be added to every command and cannot cover a command that forgets it.

**Allow-list** (derived from which commands never call `active_database`): `close_instance`,
`list_instances`, `get_hardware_info`, `list_catalog`, `catalog_recommend_tiers`, `list_stt_catalog`,
`stt_recommend_tiers`. The list is confirmed command by command in tasks.

**Allow-list verdicts** (confirmed by reading each command and its helpers, task T017):

| Command                     | Reaches vault, chat or `AppState`?                              | Verdict                                                                                     |
| --------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `close_instance`            | Yes, by design: it ends the session                             | Allow: a close must always be accepted (FR-002)                                             |
| `list_instances`            | No: scans the instances folder with `AppHandle` only            | Allow                                                                                       |
| `get_hardware_info`         | No: hardware probe                                              | Allow                                                                                       |
| `list_catalog`              | No: compiled catalog plus hardware probe                        | Allow                                                                                       |
| `catalog_recommend_tiers`   | No: compiled catalog plus hardware probe                        | Allow                                                                                       |
| `list_stt_catalog`          | No: compiled STT catalog plus hardware probe                    | Allow                                                                                       |
| `stt_recommend_tiers`       | No: compiled STT catalog plus hardware probe                    | Allow                                                                                       |
| `list_installed_stt_models` | No: directory scan of the STT model folders                     | Deny (default): not needed on the unlock screen, onboarding runs inside a vault             |
| `download_stt_model`        | No vault data, but a long network download to the shared folder | Deny (default): FR-003 wants downloads cancelled on close; T043 makes it stop with the gate |

**Limit found**: the wrapper cannot see or replace the _response_ of a request that is already
executing (`InvokeResolver` cannot be wrapped from outside Tauri). See R4 for how FR-001 and FR-009
are met anyway.

**Alternatives considered**: a `Vault` extractor on every command (kept out of scope: large mechanical
change, cannot cover forgetful commands; the spike remains as reference); checking the gate inside
`active_database` only (covers commands that touch the DB, not the ones that do not, and is not a
single place).

## R4 — In-flight requests and what the user sees

**Decision**: On close, the interface is replaced at once. The backend calls `Webview::navigate` to a
static closing page that shows only a spinner and no text (the method exists in Tauri 2.11.5 on
`Webview` and `WebviewWindow`). The frontend does nothing: no overlay and no closing state (operator
decision 2026-09-21). The old content stays visible for the length of one call, which is accepted.

**Rationale**: Discarding the page discards the JavaScript context, so vault content disappears
(FR-009) and no response can be applied to any store or view (FR-001 from the user's side), whatever
the request was and whether or not its command was written with cancellation in mind. It also drops
the frontend's copies of decrypted state at T+0 instead of waiting for the process to end.

**Interpretation to confirm at the review gate**: FR-001 says every request still in flight "MUST end
with a vault closed outcome". The plan meets it in three layers: new requests get `VaultClosed` from
the wrapper (R3); long-running requests are cancelled through the session token and return
`VaultClosed` (R5); short requests that are already executing may finish inside the grace period but
their response reaches no page. If the reviewer wants a typed error even for those, the follow-up is
migrating commands to an extractor (R3 alternatives).

**Outcome (2026-09-21)**: the page lives at `public/closing.html` in the repository root, next to
`src/`. Nuxt 4 resolves its `public/` directory against the repository root, not against `srcDir`, so
`src/public/` is not used. Both places that matter were checked: `pnpm dev` serves
`http://localhost:3030/closing.html` as `text/html` (HTTP 200), and `pnpm generate`, the command
named in `beforeBuildCommand`, puts the file at `.output/public/closing.html`, which is the
`frontendDist` folder Tauri embeds. The `about:blank` fallback stays in the code for the case that
the navigation to the page fails.

## R5 — Tracking work and draining

**Decision**: One `TaskTracker` plus one `CancellationToken` per app process (managed state
`VaultGate`). Three things register with the tracker:

1. Session-scoped background tasks: the chat turn (`chat/commands.rs`), the model preload
   (`chat/default_model.rs`, which already has a token and a join handle), voice transcription and
   its cap watcher (`voice.rs`), the provider connect completion task (`providers/connect.rs`).
2. Blocking work: everything run through `spawn_blocking` for the vault.
3. Every `VaultDb` handle: `active_database` returns a guard that wraps the `Arc<Database>` **and** a
   tracker token. It is `Clone` (callers clone it into blocking closures today) and derefs to
   `Database`. The tracker is empty exactly when no request still uses the database, so the drain
   can then take the `Arc` out of `AppState` and drop it (the connection closes, SQLCipher
   overwrites its key blocks, the file lock is released).

**Drain ladder** (spike-proven, on the local branch `spike/vault-gateway`): fire the token; wait up to
1 s for cooperative stop; abort registered async tasks; wait until 3 s in total; then end the
process regardless. Cooperative cleanup that must run (killing a child's process group in
`chat/tools/cli.rs`) observes the token; delegated CLI processes already use `kill_on_drop`, and the `ChildRegistry` (R2) covers what those miss.

**Rationale**: It reuses the existing cancellation pieces (`abort_turn`, preload token, tool
cancellation token) instead of adding a second mechanism, and the tracker replaces the
`Arc::try_unwrap` "fail if anyone still holds a clone" check with "wait, then decide".

**What cannot be stopped in-process**: blocking threads (a long SQLite call, local inference on the
engine's own threads, audio capture threads). They are bounded by the 3 s limit, after which the
process ends. This is the reason the process ends on close at all.

**Residuals (recorded 2026-09-21, T042)**: the reader thread in
`adapters/cli_delegate/connect_claude.rs` (a plain `std::thread` reading the PTY of a
`claude setup-token` flow) is not converted: it has no gate to register with, holds no vault data
(only the URL and the token that flow produces) and ends with the PTY or the process. The
`claude setup-token` child itself is not in the `ChildRegistry` either; it ends when the process
does, because its terminal closes. `gate.run` drops the future of a long command at the close, so
a `spawn_blocking` closure such as the model file copy keeps running until the process ends, and
the staging file it leaves is removed by the next start-up cleanup.

**Resolved** (T049, 2026-09-24): yes, promptly. Verified live: downloaded the real catalog model
`qwen3-0.6b-instruct-q4_k_m` (~462 MB, CPU inference), loaded it, sent a prompt asking for a long
(2000+ word) story, confirmed generation was genuinely under way from the process's own CPU time
(`/proc/<pid>/stat`, not the DOM — a raw backend `send_message` call, like `startReply` already does
for the provider case, never runs through the frontend's own reactive plumbing, so nothing here
could assume the page would show it), let it run a few more seconds, then pressed lock. The process
ended in 50 ms — the cooperative path, nowhere near the 3 s/3.5 s limit. Dropping
`llm/local/stream.rs`'s reader task stops the engine immediately in practice; the hard limit exists
for a genuinely stuck case, not the ordinary one.

**Alternatives considered**: a full per-vault `VaultSession` object dropped on close (unneeded once
the process ends); epoch headers on every request (unneeded, one vault per process).

## R6 — One vault per process supersedes atomic switching

**Decision**: `open_instance` and `create_instance` refuse while a session exists
(`VaultAlreadyActive`) or is closing (`VaultClosed`). The atomic-switch branch, the re-unlock
validation branch and its second SQLite connection are deleted. `bump_vault_generation` and the
model-load generation filter stay as they are (harmless, out of scope to remove).

**Rationale**: Spec 001 FR-022 required the atomic switch; spec 013 FR-010 replaces it. The change is
recorded by an ADR (task) and by a supersession note in [contracts/](contracts/tauri-commands.md).
Removing the validation branch also removes the second connection and one clone of the passphrase.

**Frontend consequence**: `pages/index.vue` only shows the unlock list when no session exists, which
is already true after the process ends.

## R7 — Passphrase and key handling

**Findings**:

- The passphrase is the key: `SqlCipherKey::new(passphrase)` receives it verbatim, SQLCipher derives
  internally.
- SQLCipher keeps `key`, `hmac_key` and a `pass` copy in its context while the connection is open and
  overwrites those blocks when it frees them (`sqlcipher_free` calls `xoshiro_randomness` on the
  block), for every connection it closes.
- Rust-side copies today: `OpenInstanceArgs.passphrase` (which also derives `Debug`), two clones in
  `open.rs`, one in `create.rs`, the `SqlCipherKey(String)` inside `DatabaseConfig` (haex-crdt:
  `Clone`, no `Drop`), the SQL text built by `pragma_update`, and Tauri's JSON body.

**Decision (haex-vault side)**:

- `passphrase: Zeroizing<String>` in `OpenInstanceArgs` and `CreateInstanceArgs`. `zeroize` 1.9.0 has
  `serde::Deserialize for Zeroizing<Z>` behind its `serde` feature. It is already in `Cargo.lock`
  transitively; it becomes a **direct dependency** with `features = ["serde"]` (justified in the
  plan's Complexity Tracking). The `ts-rs` export keeps `string` through `#[ts(type = "string")]`.
- A hand-written `Debug` that prints `<redacted>`; a test proves neither `{:?}` nor `{:#?}` shows the
  value.
- No clones: the value is moved into the blocking closure that opens the database and dropped there.
- The validation connection disappears with R6.

**Decision (haex-crdt side, upstream)**: `SqlCipherKey` wraps `Zeroizing<String>` and no longer
derives `Clone` (or clones into another `Zeroizing`). haex-vault then pins the new revision by full commit
SHA (constitution: immutable references). Until it lands, the haex-vault-side work is complete and the
remaining exposure is one freed, unwiped `String` per open, bounded by the process ending.

**Residual (documented, not solvable in-process)**: the SQL text inside rusqlite/SQLite for
`PRAGMA key`, Tauri's IPC body, and the JavaScript strings in the unlock and create forms. The
process ending is the guarantee for these. `sqlite3_key` would avoid the SQL text but needs `unsafe`
on the raw handle; not adopted, recorded as a possible later step.

**Frontend**: `UnlockSheet.vue` already keeps the `ref` during and after a failed attempt and clears
it through `reset()` when the sheet closes. The plan keeps that behavior, adds an explicit clear on
success (a vault close discards the whole page, so it needs none), and never puts the value in a
store. Overwriting with random data
would add nothing: JavaScript strings are immutable, so assigning a new value only drops the reference
and the old string is freed unwiped. The password field is `UiInputPassword` from the external
haex-ui layer (pinned commit), a `v-model` component that does not expose its input element, so an
uncontrolled input that avoids a copy per keystroke would need a local component; not worth it here,
because the interface runtime cannot wipe either way and the process ending is the guarantee.

## R8 — Several app processes on one computer

- **Same vault twice**: `DatabaseLock` (`fs2` exclusive lock on `<db>.lock`) already gives
  `HaexError::VaultAlreadyOpenElsewhere`; the frontend has no branch for it. Add the kind to
  `useErrorString` and a localized message (de, en).
- **Vault still closing in another process**: the lock is released when the old process ends or its
  `Database` drops. `open_instance` retries acquiring for up to the close deadline (3 s) before
  returning `VaultAlreadyOpenElsewhere`. Polling with a `ponytail:` comment (ceiling: latency of one
  poll interval; upgrade: none needed).
- **Model publication**: `MODEL_PUBLICATION_LOCKS` is an in-process `tokio::Mutex` per slug
  (`models/paths.rs`, two call sites in `models/commands.rs`). Add a cross-process lock file per slug
  under `<models>/.locks/`, taken with `std::fs::File::try_lock` (stable since Rust 1.89; the crate
  builds on 1.95) and polled asynchronously so a close can interrupt the wait. No new crate. The
  guard becomes a small struct holding both locks. Tasks confirm that listing and cleanup ignore
  `.locks/`.
- **Startup cleanup** (found while clarifying, FR-026): `cleanup_orphans_on_startup`
  (`instances/startup.rs`, called from `setup` in `lib.rs`) runs in every app process. It deletes
  every `.pending` marker with its `.db`, and through `cleanup_staging_in_dir` every model staging
  leftover, without asking whether another running process owns them. With several processes, and
  with a relaunch after every close, a starting process could delete a vault another process is still
  creating (the creator goes on writing to an unlinked file and the vault vanishes) or a download
  another process has been writing for hours (the final publish then fails).
  **Decision** (operator, 2026-09-21): a presence lock. Each process opens
  `<app local data>/presence.lock` (read and write, because Windows refuses locks on append-only
  handles) and tries an exclusive `File::try_lock`. When it gets the lock it runs the cleanup **while
  it holds it**, then calls `unlock` and only then `lock_shared`, and keeps that shared lock for the
  rest of its life. A process that does not get the exclusive lock waits in `lock_shared` and skips
  the cleanup. The code never locks a handle that already holds a lock, which `std` leaves
  unspecified and possibly deadlocking (PR review finding). An atomic exclusive-to-shared downgrade
  is neither available nor needed: `std` has no such call, Windows `LockFileEx` has no conversion,
  and `flock(2)` documents its conversion as not guaranteed to be atomic. It is not needed because
  **no process starts work before it holds its shared lock**: whoever holds the exclusive lock, or
  sits between `unlock` and `lock_shared`, has started no work, and whoever has started work holds a
  shared lock, which makes every other exclusive attempt fail. The OS drops the lock when the process
  ends or crashes. The relaunch after a close overlaps the draining old process, so it skips the
  cleanup too, which is fine because nothing was orphaned. Half-created vaults are never listed
  (`list_instances` skips `.pending`) and staging files are not `.gguf`, so leftovers are harmless
  until the next start that is alone. **Alternatives**: a lock per leftover (precise, but downloads
  would have to hold a lock for hours or a new lock kind is needed); an age threshold (a guess that a
  long download or a suspended computer can still hit).
- **Vault list**: `instance-list-changed` is in-process. `pages/index.vue` already calls `syncAsync`
  on mount; add a refresh on window focus and when the unlock sheet opens.
- **Logs**: debug builds only, so FR-023 holds for release builds by construction. Debug builds may
  interleave; no vault content is logged (checked by the passphrase test, not by a promise).
- **Approval socket**: already in a per-run `TempDir`, no cross-process collision.

## R9 — Window close and quit

**Decision**: Handle `WindowEvent::CloseRequested` and `RunEvent::ExitRequested` (unless the code is
`RESTART_EXIT_CODE` or the gate is already closing) by preventing the default, running the close
protocol with policy `Exit`, and exiting when it finishes. This gives FR-007 the same guarantees as
an explicit close. The builder needs `.build(context)?.run(callback)` instead of `.run(context)`.

## R10 — Existing candidates (graphify)

Queries on the repository graph (snapshot of the primary checkout, used as is for a linked worktree):

| Need                         | Candidates found                                                                                           | Outcome                                                  |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| Close and cancel work        | `close_instance`, `ChatState`, `abort_turn`, preload handle, `AppState`                                    | Extend these; no parallel close path                     |
| Tracked background tasks     | `spawn_cap_watcher`, `spawn_turn` (test fixture), `cli_delegate/process.rs` (`kill_on_drop`)               | None is a tracker; `VaultGate` is new, reuses the tokens |
| Secret hygiene               | none (only the passphrase refs in the two sheets)                                                          | New: `Zeroizing`, redacted `Debug`                       |
| Cross-process file locking   | none in haex-vault (`fs2` only inside haex-crdt)                                                           | New small helper next to the publication lock            |
| Startup cleanup gate         | `instances/startup.rs` cleans unconditionally                                                              | New `instances/presence.rs`; the cleanup is its callback |
| Run a future until a token   | none reusable: `chat/tools/cli.rs` selects on its cancellation token inline                                | New `VaultGate::run`                                     |
| Retry until a lock is free   | `retry_or_bail`, `retry_backoff`, `RetryDecision` in `chat/turn/step.rs` retry provider streams, not locks | New `retry_while_locked` (Stage 5)                       |
| Close effects behind a trait | none                                                                                                       | New `CloseEffects` (Stage 3)                             |
| Handle with a drop guard     | only `ClaudeConnectSession`'s own `Drop`; `abort_turn` in `chat/commands.rs` is reused                     | New `VaultDb`                                            |

The four queries of task T016 ("run a future until a cancellation token fires", "retry an operation
until a file lock is free", "abstract the side effects of closing so tests can record them", "wrap a
shared handle with a drop guard counter") returned only broad results (breadth-first traversals from
generic terms over 4986 nodes). The candidates in the four rows above were confirmed by reading the
code, so no existing helper is paralleled.

## R11 — Test approach

- Rust: sibling `*_tests.rs` files declared from the module, plus integration tests under
  `src-tauri/tests/`. The Tauri mock runtime (`tauri` `test` feature as a dev-dependency, already on
  this branch) exercises the real IPC path for the wrapper and the close protocol. No sleeps in async
  tests: use paused time or explicit channels.
- Frontend: the replay harness `scripts/check-chat-state.ts`. Its header says the next new case must
  first extract `createChatState` into `scripts/lib/`; that extraction is a prerequisite task
  (Complexity Tracking).
- Manual (quickstart): relaunch under `tauri dev`, closing page placement, local inference stop time,
  two processes with two vaults.
