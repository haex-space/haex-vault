# Research: Diagnostic log

Every decision names what was read in the code and what was rejected. File references are repository-relative.

## R1: One logger, one swappable sink

**Facts.** `tauri-plugin-log` 2.10 builds its `fern` dispatch once, when the plugin is created, from fixed targets. Its
`Folder`/`LogDir` target takes one directory for the life of the process and its rotation is private to the plugin. It also has
`TargetKind::Dispatch(fern::Dispatch)` for a target of our own, and the plugin owns the process-wide `log` facade used by backend macros. Today the plugin is created only `if cfg!(debug_assertions)` (`src-tauri/src/lib.rs`, level fixed at `Info`).

**Decision.** Create the plugin in every build, with one `TargetKind::Dispatch` whose output is our own `log::Log` sink
(`diagnostic_log/sink.rs`). The sink holds `RwLock<Option<Writer>>`: with no writer it drops the record, with one it formats the
line and writes it. In debug builds a `Stdout` target stays beside it, so the developer still sees the lines in the terminal.

**Why.** It is the only way to point the log at a different folder per vault without a second logger, and a swappable sink is needed to route backend records into the currently open vault.

**Rejected.**

- `TargetKind::Folder` per vault: the folder is fixed when the plugin is built, before any vault is known.
- Our own `log::set_logger` without the plugin: this would replace the existing process logger integration unnecessarily.
- A second plugin instance per vault: a process has one `log` logger; a second `set_logger` fails.

## R2: The level is the global max level, with module filters

**Facts.** `log::set_max_level` is what the `debug!` macros check first, so a filtered-out call costs one comparison. `fern`
filters at the dispatch level before the target, so the dispatch itself must be built at `Debug` or `Trace`, or a raised level
would not reach the sink.

**Decision.** The dispatch is built permissive (`Trace`). The level is applied twice: `log::set_max_level` is `Info` in
"Standard" and `Debug` in "Ausführlich" (changed at run time by `level.rs`), and a per-module filter keeps chatty
third-party crates at `Warn` at both levels (FR-012). The list starts with what the code already damps and the dev log showed:
`iroh`, `tracing::span`, `zbus`; the plan's tasks measure a "Ausführlich" session and add `hyper`, `h2`, `rustls`, `reqwest`
or `nostr_sdk` only if they flood.

**Why.** It satisfies FR-002 (at once, no restart) and the performance goal (no formatting in "Standard"), and the damping
stays out of the user's control as the spec says.

**Rejected.** Filtering only inside the sink: every `debug!` would format its arguments in "Standard" for nothing.

## R3: Where the log begins and ends

**Facts.** `instances/session_services.rs::start` is called by `create.rs`, `open.rs` and `import.rs` after the vault is
published; its doc comment says it exists so the three do not diverge. `close.rs::finish_close` drains the session, takes the
database and then lets the process end. The vault's folder is known from `VaultPaths` (`instances/vault_paths.rs`), whose
header says everything beyond the SQLite files goes into subfolders.

**Decision.** `diagnostic_log::begin(app, state)` is the first call in `session_services::start`: it resolves the vault's
`logs/` folder, opens `haex.log` for appending, reads the level (R4) and attaches the writer. `diagnostic_log::end()` runs at the
end of `finish_close` after the drain, so the lines of the close are in the log; it flushes and detaches. Lines before
`begin` (start of the app, the vault list, the passphrase, the migrations and the first-time setup inside the database open)
are dropped by the sink; nothing is buffered (FR-008). The migrations are the one place where a user might have wanted a line
(spec edge case "the unlock itself"); the spec accepts it.

**Rejected.**

- Attaching inside `open_existing_database` to also catch the migrations: it is a different function per way of getting a
  vault (create has its own) and would put a hook in a function that tests call directly.
- Buffering until `begin`: excluded by the spec's decision of 2026-11-21.

## R4: The level lives in a `_no_sync` table; the end is a time and a run

**Facts.** A device preference (`PrefScope::Device`, as `privacy/screen_capture.rs` uses) is a CRDT-tracked row: it replicates to
the other devices, which never read it, and every change of it is a change the sync has to carry. A switch of the log level is
made exactly when something is being diagnosed, possibly the sync itself, so it would add sync changes to the very thing under
observation. The repository has the other precedent for a setting of one device only: `haex_file_device_access_no_sync`
(`identity/migrations_file_shares.rs`, one row with `id = 1`, written with a plain `INSERT … ON CONFLICT` in
`files/share/device_access.rs`), and about forty `_no_sync` tables in all. A `_no_sync` table is not tracked, so no change of
`HAEX_TRIGGER_VERSION` is needed for it. The latest migration of this tree when this was written was `0066_chat_record_relays`; it became `0068_diagnostic_log` (0067 was taken meanwhile).

**Decision.** One table `haex_diagnostic_log_no_sync` in a new migration (the next free number when it is written, file
`identity/migrations_diagnostic_log.rs`, registered in `haex_migration_source`): one row with `id = 1` and the columns `level`
(`standard` or `verbose`), `until_ms` (end time in Unix milliseconds, only with `verbose`) and `run_id` (random id of the app run
that switched, only with `verbose`); no row means `standard`. At `begin`: if `verbose` and the end time has passed or the run id
differs from this run's, the row is reset to `standard`. While `verbose` is on, a task (tracked as session work like the sync)
sleeps until the end time, then resets it and emits the event of the change (contracts/events.md). The end time is computed from the
system clock at the switch and capped at 24 hours from the monotonic instant of the switch (not app start), so moving the clock
backwards cannot extend it (spec edge case). A vault file copied to another device carries the row; its `run_id` is then never the new device's run, so
the copy starts at `standard`.

**Why.** It is local by construction, which is what the spec says ("does not sync to other devices"), it costs the sync nothing
and it follows a precedent for exactly this kind of setting.

**Rejected.**

- A device preference (the first draft of this plan): needs no migration, but the row replicates and every switch is a sync change.
  The saved migration was the only argument for it; the repository's own `_no_sync` precedent shows the cost is one small file.
- A plain file next to the log: it was needed only to read the level before the unlock, which the spec dropped.
- A global preference outside the vault: contradicts "one level per vault".

**Risk.** A new migration name can collide with a parallel branch (`open_existing_database` already carries a retry for renumbered
migrations). The task takes the next free number at the time it is written and the pull request is rebased on `main` before it
is pushed.

## R5: The interface forwards through the plugin's command

**Facts.** The interface has no `@tauri-apps/plugin-log` today (`package.json`), no `log:` permission in
`capabilities/default.json`, no global error handler, and 73 `console.warn`/`console.error` calls in 35 files. Both
capabilities files apply to the window `main` only, so extension frames get nothing.

**Decision.** A client plugin `src/plugins/diagnosticLog.client.ts` wraps `console.warn` and `console.error` (and `console.info`
and `console.debug` while "Ausführlich" is on), listens to `window.onerror` and `unhandledrejection`, and listens to
the Nuxt hooks `vue:error` and `app:error` for errors of the view layer (Nuxt itself sets `vueApp.config.errorHandler` in
`nuxt/dist/app/entry.js`, so the plugin must not replace it). Each message becomes a string (the text of the arguments, the stack of an `Error`); objects other
than errors are not serialised, they become `[object]` (FR-006). The plugin calls a dedicated `diagnostic_log_ui_message` command with the current opaque `sessionId`; the backend drops a
queued message unless its token matches the attached vault session. The original `console` call still runs. The
plugin takes the current level from the change event, which the backend sends at every `begin` (the plugin loads before the unlock and cannot ask then), and from one `diagnostic_log_get` at its start for a reload of the window, and drops `info`/`debug` without an IPC
call in "Standard". A token bucket (100 messages a second, burst 200) limits a loop; once a second the plugin writes one line with
the number of messages it dropped (FR-014). The scoped app command is available only to the main window.

**Why.** The session token prevents a message queued by the webview from crossing a vault switch; formatting and redaction stay
in one typed command.

**Rejected.**

- Serialising arguments to JSON: an object with an entry's fields would put them in the log (FR-009).
- `attachConsole` (backend to console): the opposite direction, not needed.

## R6: Files, rotation and size

**Facts.** Today's limit is 40 kB with `KeepOne` rotation, which rotated the sync lines away within seconds (comment in
`lib.rs`). The plugin's rotation is private (R1).

**Decision.** `logs/haex.log` plus `haex.log.1` to `haex.log.N` in the vault's folder. `rotation.rs` rotates when the current file
would pass the limit: the oldest is deleted, the others shift up, a new `haex.log` starts. Limits are constants of the module
and arguments of the type, so a test uses small ones: "Standard" 4 files of 256 KiB (about 1 MiB), "Ausführlich" 5 files of 4 MiB
(about 20 MiB). On a switch from "Ausführlich" to "Standard", immediately retain only the newest complete lines that fit
within four files of 256 KiB total, with each file at most 256 KiB; discard older lines first. An I/O error on a write sets a flag `unwritable`
(shown by the setting, FR-016), drops the line and tries again on the next one; it never panics and never blocks beyond the single
write. The folder is created at `begin`; a failure there leaves the sink detached and sets the same flag.

**Why.** About 20 MiB hold well over 30 minutes of "Ausführlich" at the rates seen in the dev log (about 1 KiB a second with iroh
damped), which is what SC-004 asks for; the numbers are constants to adjust after a measured session.

**Rejected.** A time-based rotation (daily files): the size is what the spec bounds.

## R7: Secrets never enter a line

**Facts.** The log is plain text beside an encrypted vault, so nothing downstream protects its content. The code has many
`log::` calls; the risk is a value in a message, not the macro.

**Decision.** Three measures. (1) The interface sends text and stack only (R5). (2) A review task lists every `log::` call at `warn`
and above and every error type whose `Display` could contain a value, and fixes the ones that do; the `corrupt()` helper of the
legacy import (which logs the SQL error text, no value) is the model. (3) `tests/diagnostic_log_secrets.rs` imports the pre-rename
vault fixture (known secrets: `pre-rename-mail-secret`, `pre-rename-router-secret`, `JBSWY3DPEHPK3PXP`, the passphrase) in
"Ausführlich" and fails if any appears in the log (SC-003). The later `debug!` lines of other areas must keep the test green; the
spec says they come in later steps.

**Rejected.** A redaction filter that searches lines for secret-looking text: it cannot know a user's secret and gives false
comfort.

## R8: Open the folder, share on Android

**Facts.** `tauri-plugin-opener` 2.5 is in `Cargo.toml` and is initialised in `lib.rs`; the capabilities list only
`opener:allow-open-url`. The Android plugin (`src-tauri/plugins/haex-android`) has only the inbound share
(`SharedFilesIntake.kt`, `ACTION_SEND` received); it has no `FileProvider` and no outbound share.

**Decision.** Desktop: a Rust command `diagnostic_log_reveal` calls `OpenerExt::reveal_item_in_dir` on `haex.log`, so no frontend
permission is added. Android: a new call `share_file(path, mime)` in the Android plugin starts a `Intent.createChooser` with
`ACTION_SEND` and a `content://` URI from a `FileProvider` declared in the plugin's manifest and limited to the app cache folder
used for diagnostic-log snapshots. The command `diagnostic_log_share` hands the path of a copy of the current log (a snapshot, so the file can keep
rotating) to it. Nothing is sent by the app; the share sheet's target is the user's choice (spec User Story 3).

**Risk.** There is no Kotlin test setup and the repository's Android emulator procedure is manual (memory: local emulator). The
task for it states the manual check and the Rust side is tested up to the plugin call.

**Rejected.** Sharing the log's text as `EXTRA_TEXT`: the binder limit of about 1 MiB cuts a verbose log. A `ShareSheet` npm plugin:
a new dependency for a call the Android plugin can make itself.

## R9: The dev build keeps its terminal output

**Decision.** In debug builds a `Stdout` target stays (filtered by the same level), so `pnpm tauri:dev` prints as before. The old
global dev file (`<app local data>/logs/<name>.log`) is not written any more: it contradicts FR-010 and the dev log of a vault now
lies in that vault's folder. Searched the repository: no script, E2E suite or doc reads that file (`rg HaexVault.log`).

## R10: The setting and its catalog action

**Facts.** Settings writes in this repository go through the action catalog (`settings.privacy.screenCapture.set`), which no
agent may call (a guardrail); the component is mounted in `BasicView.vue` ("Allgemein → Grundeinstellung"); texts exist in `de`
and `en`.

**Decision.** `DiagnosticLogSetting.vue` in `BasicView.vue` with the two levels as a segmented choice, the end time while "Ausführlich"
is on, a line that says the log lies in the vault's folder and goes with the folder when it is copied (FR-017), a line that says
more detail comes with later steps (spec Assumptions), the button to reveal the folder (desktop) or "Protokoll teilen" (Android), and
a notice when the log cannot be written. The catalog action is `settings.diagnostics.logLevel.set`, marked as not callable by an
agent. The component shows no success text (UI feedback rule); a failure goes to the toast as in
`ScreenCaptureSetting.vue`.

## Open questions

None. Two numbers (the module damping list of R2 and the sizes of R6) are constants the tasks tune after one measured session;
the spec fixes only the behaviour (SC-004).
