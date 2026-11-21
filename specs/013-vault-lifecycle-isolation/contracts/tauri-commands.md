# Contract: Tauri command surface and the request gateway

The interfaces this feature changes are the command surface between the Vue frontend and the Rust
backend, plus one new backend-wide rule: every request passes one gateway. Wire structs keep
`#[serde(rename_all = "camelCase")]`; errors keep the `{ kind, ...fields }` shape.

This contract **supersedes** [001-frontend-onboarding tauri-commands.md](../../001-frontend-onboarding/contracts/tauri-commands.md)
for `open_instance`, `create_instance` and `close_instance`, and replaces spec 001 FR-022 (atomic
switch) with spec 013 FR-010. The supersession is recorded in an ADR (see the plan).

## The gateway (new, backend-wide)

`lib.rs` registers `invoke_handler(gate.wrap(tauri::generate_handler![...]))`. For every request:

| Gate phase | Command class           | Result                                                                       |
| ---------- | ----------------------- | ---------------------------------------------------------------------------- |
| `Idle`     | any                     | passed on; vault commands fail with `NoActiveInstance` as today              |
| `Active`   | any                     | passed on                                                                    |
| `Closing`  | app-scoped (allow-list) | passed on                                                                    |
| `Closing`  | anything else           | rejected at once with `{ kind: "VaultClosed" }`; the command body never runs |

**Allow-list (app-scoped commands, never touch the vault)**: `active_instance_name`,
`close_instance`, `list_instances`, `get_hardware_info`, `list_catalog`,
`catalog_recommend_tiers`, `list_stt_catalog`, `stt_recommend_tiers`. A command is app-scoped only
if it never calls `active_database`; tasks verify each entry. A new command is gated by default.

## `active_instance_name` (frontend reload recovery)

```rust
#[tauri::command]
pub fn active_instance_name(state: State<'_, AppState>) -> Result<Option<String>>;
```

Returns the name held by this process's active-instance slot without opening or reading the vault.
The landing page calls it during startup; when it returns a name, the frontend restores the
workspace route instead of presenting an unlock flow that would receive `VaultAlreadyActive`.

The wrapper never changes a response of a request that is already executing. See
[research.md](../research.md) R4 for how those are covered.

## `close_instance` (changed)

```rust
#[tauri::command]
pub async fn close_instance(app: AppHandle) -> Result<()>;
```

(The gate is reached through `app.state::<VaultGate>()` inside `start_close`, not injected as its own
parameter; `Result<T>` is the crate's own alias for `Result<T, HaexError>`.)

| Aspect         | Before                                                             | After                                                                 |
| -------------- | ------------------------------------------------------------------ | --------------------------------------------------------------------- |
| Can be refused | yes: `acquire_operation` fails while a turn, load or transfer runs | **never**                                                             |
| Idempotent     | only when nothing is active                                        | always: a second call returns `Ok(())` and changes nothing            |
| Cancels work   | no                                                                 | yes: turns, tools, preload, loads, downloads, voice, child processes  |
| Returns        | after the database was dropped                                     | **immediately** after phase 1 (below); phase 2 runs in the background |
| Ends           | leaves the process running                                         | ends the process (relaunch or exit, per `ClosePolicy`)                |
| `CloseFailed`  | when a subsystem still held a clone                                | removed; a stuck subsystem is answered by ending the process          |

**Phase 1 (synchronous, cannot fail)**: flip the gate to `Closing`; fire the cancellation token;
arm the outer forced end (the drain limit plus the grace period from now); call `abort_turn` and
fire the preload's own cancellation; navigate the webview to the closing page; emit
`instance-list-changed` `{ reason: "closed" }`; start phase 2; return `Ok(())`.

**Phase 2 (background task)**: let go of the loaded session, the tool registry and the approval
maps first, because a delegate adapter inside the session holds a database handle and the tracker
cannot empty while it lives; wait for the tracker up to about 1 s; abort registered tasks and end
the registered child processes; wait until about 3 s in total; take the database out of `AppState`
and drop it; clear the whisper cache (bounded); request the end of the process and arm the forced
end, which ends it about 0.5 s later if it is still alive. Ending the process does not wait for
anything that outlived the limit.

The outer forced end covers a phase 2 that never gets to run, for example because the async runtime
is blocked. Both forced ends share one claim on the gate, so the process is ended by force at most
once.

## `open_instance` (changed)

Arguments unchanged on the wire (`{ name, passphrase }`). Backend type of `passphrase` is
`Zeroizing<String>`.

| Situation                           | Result                                                                                                  |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Gate `Idle`, credentials valid      | opens, moves the gate to `Active`, returns `InstanceInfo`                                               |
| Gate `Active`                       | `{ kind: "VaultAlreadyActive" }`, nothing changes                                                       |
| Gate `Closing`                      | `{ kind: "VaultClosed" }`                                                                               |
| Another app process holds the vault | retried for up to about 3 s (that process may be closing), then `{ kind: "VaultAlreadyOpenElsewhere" }` |
| Wrong passphrase / no such file     | unchanged (`WrongPassphrase` / `NotFound`); the gate stays `Idle`                                       |

**Removed**: the atomic switch, the "already active with the same name" credential re-check and its
second SQLite connection. The passphrase is moved into the blocking open task and dropped there;
it is never cloned.

**Information note**: `VaultAlreadyOpenElsewhere` is reported when the file lock is taken, before
the passphrase is verified (this is already how the lock is ordered in haex-crdt). It reveals only
that a vault the list already shows is in use. The wrong-passphrase versus not-found rule of spec
001 FR-021 is unaffected.

## `create_instance` (changed)

Same gate rules as `open_instance` (`VaultAlreadyActive`, `VaultClosed`). Backend `passphrase` is
`Zeroizing<String>`; the clone in `create.rs` is removed. On success the gate moves to `Active`.

## `active_model_info` (already changed on this branch)

Takes no operation slot (FR-025). Listed here because it is the reason two overlapping reads no
longer reject each other.

## Unchanged commands that gain behavior through the gateway

All vault commands: after `Closing` they answer `VaultClosed` without running. Long-running ones
(`send_message`, `load_model`, `download_model_*`, `import_model_from_file`, voice, provider connect
and refresh) also observe the cancellation token while they run and return `VaultClosed` when it
fires.

## Error type additions

```rust
#[error("The vault is closed")]
VaultClosed,
#[error("A vault is already open in this app process")]
VaultAlreadyActive,
// exists: VaultAlreadyOpenElsewhere
```

`CloseFailed` is deleted. Both new variants are fieldless, and the TypeScript bindings are
regenerated (`pnpm generate:ts-types`; trailing whitespace is stripped as usual).

## Events

| Event                   | Change                                                                                  |
| ----------------------- | --------------------------------------------------------------------------------------- |
| `instance-list-changed` | still emitted on close with `reason: "closed"`; no consumer remains in the closing page |
| others                  | unchanged; none carry a vault epoch (not needed, one vault per process)                 |
