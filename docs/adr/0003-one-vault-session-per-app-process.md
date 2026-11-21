# One app process serves at most one vault session

Status: accepted
Date: 2026-09-23

## Decision

One HaexVault process opens at most one vault for its whole lifetime. `open_instance` and
`create_instance` both check the vault gate first, before resolving any path: if a vault is already
active, they refuse with `VaultAlreadyActive`; if a close is under way, with `VaultClosed` (spec 013
FR-010 to FR-012). Neither ever switches the active vault out from under a running process. The only
way to work with a different vault is to close the current one (which ends the process, spec 013
US1) and unlock the other one in a fresh process.

This supersedes spec 001's FR-022 ("only one active instance per app process"), which the earlier
`open_instance` implementation satisfied by _switching_: dropping the previous handle, cancelling its
preload, clearing its chat session, and publishing the new one in its place, all without ending the
process. That switch is removed together with this decision.

## Rationale

A switch is a second, narrower copy of everything the real close protocol (spec 013 US1) already
does correctly and completely: cancel the running turn, stop tracked work, drop the database, wipe
the passphrase. Keeping both means every future addition to session-scoped state — a cache, a
loaded model, a background task — has to remember to be reset in _two_ places, and a switch that
misses one is a state leak between vaults that no test forces into the open, because the switch path
is exercised far less than the close path.

The gate already tracks exactly the state needed to decide this correctly (`VaultPhase`), and its
`Idle` → `Active` → `Closing` phases only ever move forward — there is no `Active` → `Active` step to
misuse for a switch. Refusing a second open is therefore not new machinery, only routing the
already-existing decision to the two commands that need it before they touch a path or a file.

## Consequences

- `open_instance`/`create_instance` refuse a second vault instead of switching to it; a maintainer
  who wants "switch vaults" support must build it as close-then-reopen (two processes), not as a new
  in-process code path — that would reintroduce the invariant this decision removes.
- `AppState::install` is the one place a vault handle is published, and it moves the gate `Idle` to
  `Active` under the same lock, so the two can never disagree (spec 013 T054, T057).
- `AppState::switch_to`, `active_database_named` and `is_active_named` are removed; nothing else used
  them.
- A second `open_instance`/`create_instance` call while one is active is unreachable through the
  real UI today (spec 013 T058: no frontend path re-enters the unlock screen without first locking),
  so this closes a latent gap rather than a user-visible bug — but a backend command that only the
  frontend's own restraint keeps single-session is exactly the shape of bug this decision rules out
  by construction, for whatever calls the command next (a future feature, a test, a scripting
  surface).

## Adding work later

- Session-scoped background work uses `gate.spawn` (or `gate.spawn_blocking`) and `gate.run`, so
  close tracks, cancels, and drains it.
- A new Tauri command is gated by default. Allow-list a command only if it never touches the vault.
