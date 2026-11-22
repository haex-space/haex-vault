---
description: 'Task list for the diagnostic log (spec 098)'
---

# Tasks: Diagnostic log

**Input**: Design documents from `specs/098-diagnostic-log/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. The spec's success criteria (SC-002, SC-003, SC-005, SC-006) are tests, and the repository writes the test before the code. Every test task comes before the task that makes it pass.

**Organization**: Tasks are grouped by user story so each story can be built and tested alone. US1 is the MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no open dependency)
- **[Story]**: the user story of the spec (US1 to US3); setup, foundation and polish tasks carry none

## Notes for every task

- **Rust tests live in `*_tests.rs` beside the module**, declared with `#[cfg(test)] #[path = "x_tests.rs"] mod tests;`, never inline. Integration tests go to `src-tauri/tests/`.
- **Build and test inside the memory scope** (the machine runs out of memory otherwise): `env -u CC -u CXX -u AR CARGO_TARGET_DIR=<main checkout>/src-tauri/target CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CMAKE_BUILD_PARALLEL_LEVEL=2 NUM_JOBS=1 systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 cargo test <filter> -j 1`. Do not raise 10 GB without asking.
- **New `#[ts(export)]` types carry the prefix `DiagnosticLog`** (a name collision overwrites another binding silently); after `cargo test` run `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.
- **Every new text exists in `de` and `en`** (`src/i18n/locales/`); the group is `settings.diagnosticLog` (the existing `logs` group belongs to the extensions' log view).
- **No path of a person's machine in any committed file**; docs and tests use relative paths and temporary folders.
- **Imports at the top of the file**; files stay below 500 lines.
- **Commits**: Conventional Commits in English, no trailer naming an assistant.

---

## Phase 1: Setup

**Purpose**: the dependency, the module skeleton, the path of the log folder and the table of the level

- [x] T002 [P] Create the module `src-tauri/src/diagnostic_log/mod.rs` with the doc comment of the feature (spec 098, FR-007 to FR-010) and declare it as `pub mod diagnostic_log;` in `src-tauri/src/lib.rs`; the sub-modules `sink`, `rotation`, `level`, `expiry`, `commands` are added by the tasks that create them
- [x] T003 [P] Add `logs_dir(&self, name: &str) -> PathBuf` and the constant `LOGS` (`"logs"`) to `src-tauri/src/instances/vault_paths.rs`, next to `agent_browser_dir` and the other subfolders, and extend the header comment; test it in `src-tauri/src/instances/vault_paths_tests.rs` (the folder lies below the vault's folder and never in the vaults folder itself); then read every place that walks a vault's folder (`src-tauri/src/instances/layout_adopt.rs`, `layout_move.rs`, `cleanup.rs`, `list.rs`) and decide for each what it does with an unknown subfolder `logs/`: it must neither take the folder for a vault or an orphan nor lose it when the vault's folder is moved or adopted (FR-017: the log goes with the folder); write one test per place that has a branch for it, in its `*_tests.rs`, and list the decision of each place in the pull request note
- [x] T004 [P] Write the migration of the level table: `src-tauri/src/identity/migrations_diagnostic_log.rs` with the constant for `CREATE TABLE haex_diagnostic_log_no_sync (id INTEGER PRIMARY KEY NOT NULL, level TEXT NOT NULL, until_ms INTEGER, run_id TEXT)` (research R4, data-model.md; take the next free migration number after `0066_chat_record_relays` at the time of writing, check `git log origin/main -- src-tauri/src/identity` first), register it in `src-tauri/src/identity/migrations.rs` next to `0066_chat_record_relays` and add the table to the list of tables in `src-tauri/src/identity/migrations_tests.rs`; the table is not tracked by the sync, so `HAEX_TRIGGER_VERSION` stays as it is; test in a new `migrations_diagnostic_log_tests.rs` (the table exists after opening a new vault and a vault from before the migration, and a write to it creates no sync change)

---

## Phase 2: Foundation (blocks every story)

**Purpose**: a sink that writes into a vault's `logs/`, rotates, filters by level and is attached at the unlock and detached at the close. After this phase every build writes a log at "Standard"; the level setting itself is US1.

**⚠️ CRITICAL**: no user story starts before this phase is done.

- [x] T005 [P] Write `src-tauri/src/diagnostic_log/rotation_tests.rs` first: a file past the limit rotates to `haex.log.1` and a new `haex.log` starts; with N files kept the oldest is deleted; a rotation never splits a line; the limits are arguments, so the test uses 200 bytes; a write error returns an error value, it does not panic
- [x] T006 Implement `src-tauri/src/diagnostic_log/rotation.rs` (the type `RotatingFile` with `open(folder, limits)`, `write_line`, `set_limits`, `flush`) until T005 passes; the defaults are constants: "Standard" 4 files of 256 KiB, "Ausführlich" 5 files of 4 MiB (research R6)
- [x] T007 [P] Write `src-tauri/src/diagnostic_log/level_tests.rs` first: "Standard" gives max level `Info`, "Ausführlich" gives `Debug`; the module filter keeps `iroh`, `tracing::span` and `zbus` at `Warn` in both levels and lets `haex_lib::…` through at the level's own; applying a level changes `log::max_level()`
- [x] T008 Implement `src-tauri/src/diagnostic_log/level.rs` (enum `LogLevel { Standard, Verbose }`, `max_level()`, `allows(target, level)`, `apply()` setting `log::set_max_level`) until T007 passes; the damping list is one constant (research R2)
- [x] T009 [P] Write `src-tauri/src/diagnostic_log/sink_tests.rs` first: a record is dropped while no writer is attached (FR-008, nothing buffered: attach afterwards and the earlier line is not in the file); an attached sink writes `[date][time][LEVEL][origin] message`; a UI record written by the session-scoped command gets origin `ui` and keeps stack lines indented; a record the level does not allow is not written; detaching flushes and stops writing; a failed write sets `unwritable` and the next write is tried again; a message with a line break is written as one record
- [x] T010 Implement `src-tauri/src/diagnostic_log/sink.rs` (`struct DiagnosticSink` implementing `log::Log` behind `RwLock<Option<RotatingFile>>`, an `AtomicBool` for `unwritable`, `attach(folder, level)`, `detach()`, the line formatter) until T009 passes; one static `SINK` reachable from `mod.rs`
- [x] T011 Build the log plugin for every build in `src-tauri/src/lib.rs`: replace the block `if cfg!(debug_assertions) { app.handle().plugin(tauri_plugin_log::Builder::default()… }` with a builder that calls `clear_targets()`, builds at `LevelFilter::Trace`, adds `Target::new(TargetKind::Dispatch(…))` chaining the `SINK`, adds `TargetKind::Stdout` only `if cfg!(debug_assertions)`, and sets `log::set_max_level(Info)` before the vault exists; the global file of the dev build (`LogDir`) is no longer written (research R9, FR-010)
- [x] T012 Implement in `src-tauri/src/diagnostic_log/mod.rs` the function `begin(app, state)` (resolves `VaultPaths::logs_dir` of the active vault with `get_app_local_data` and `state.active_name()`, creates the folder, attaches the sink at "Standard"; a failure sets `unwritable` and leaves the sink detached, FR-016) and `end()` (flush and detach), plus the testable core `attach_folder(folder)` that takes a path; add a test in `src-tauri/src/diagnostic_log/sink_tests.rs` or a new `mod_tests.rs` that `attach_folder` on a temporary folder creates `logs/haex.log` and a second `attach_folder` on another folder moves the writing there without a line in the first (FR-007)
- [x] T013 Hook the log into the vault's session: call `crate::diagnostic_log::begin(app, state)` as the first line of `start` in `src-tauri/src/instances/session_services.rs` (create, open and import all pass there) and `crate::diagnostic_log::end()` after the drain in `finish_close` in `src-tauri/src/instances/close.rs`, before the process is told to end; add the one-line why-comments

**Checkpoint**: unlock a vault in a dev build and `vaults/<name>/logs/haex.log` appears with the app's warnings and errors; before the unlock there is no `logs/` folder.

---

## Phase 3: User Story 1 - A failure leaves a trace the user can pass on (Priority: P1) 🎯 MVP

**Goal**: the user can switch to "Ausführlich" in the settings, the switch works at once and ends by itself, and a failing step in an open vault leaves a line naming the cause.

**Independent Test**: with the dev build, switch to "Ausführlich", import a file whose read fails, and find the cause in `logs/haex.log`; set the end time into the past and unlock again: the level is "Standard".

### Tests for User Story 1 (write first, see them fail)

- [x] T014 [P] [US1] Write `src-tauri/src/diagnostic_log/expiry_tests.rs`: switching to `verbose` writes the row of `haex_diagnostic_log_no_sync` with `level = verbose`, `until_ms` (now plus 24 hours) and `run_id`; switching back writes `standard` with both cleared; no row reads as `standard`; at `begin` a `verbose` whose end has passed becomes `standard`; a `verbose` of another run id becomes `standard` (a new start of the app); a damaged row (unknown level, `verbose` without end or run) counts as `standard` and is rewritten; the deadline is capped at 24 hours from the monotonic instant of the switch (including a backward wall-clock change), not from app start
- [x] T015 [P] [US1] Write `src-tauri/src/diagnostic_log/commands_tests.rs` (use the helpers of `src-tauri/src/privacy/screen_capture_tests.rs` for an open test vault): fresh status is standard with no end and writable; verbose sets Debug and expires 24 hours after the switch; repeating a level is a no-op; begin/change events carry the session token; after creating verbose archives, switching to standard immediately retains only newest complete lines within four 256 KiB files total; expiry is scheduled when verbose is selected without restart and cancelled by standard; change events are emitted once per change and once at begin
- [x] T016 [P] [US1] Write `src-tauri/tests/diagnostic_log_failure_trace.rs` (SC-002): build a legacy-looking source vault with the helpers of `src-tauri/tests/common/legacy_haex_vault_fixture.rs`, make a read query fail (a blob in the `title` column), attach the sink to a temporary `logs/` folder, run the import into an open vault and assert that `haex.log` holds a `WARN` line from `passwords::import::legacy_haex_vault` with the reason `corrupt`
- [x] T017 [P] [US1] Write `src-tauri/tests/diagnostic_log_secrets.rs` (SC-003, FR-009): attach the sink at "Ausführlich", import `src-tauri/tests/fixtures/pre_rename_vault/pre_rename_vault.db` through the legacy reader and the import service (passphrase `pre-rename-test-passphrase`), write one known marker with `log::info!` first (a control so a green test cannot be empty), and assert that none of `pre-rename-mail-secret`, `pre-rename-router-secret`, `pre-rename-shop-secret`, `JBSWY3DPEHPK3PXP`, the note of the `Mail` entry, the value `4711` of its field `PIN`, the passphrase and the hex of the vault identity's secret key (read from `vault_identity_secret_no_sync`) appears in any file of `logs/`; run the open of the vault and the import in the test, the two flows SC-003 names

### Implementation for User Story 1

- [x] T018 [US1] Implement `src-tauri/src/diagnostic_log/expiry.rs` (the row of the table, `read(q)`, `switch(tx, level, run_id, now)`, the validation of data-model.md, `RUN_ID` as a `OnceLock<String>` set at the first use) until T014 passes; read with `q.query_row` and write with `tx.execute("INSERT … ON CONFLICT(id) DO UPDATE …")` as `files/share/device_access.rs` does for `haex_file_device_access_no_sync`, so a switch creates no sync change; keep a per-switch monotonic deadline for this run
- [x] T019 [US1] Extend `begin` in `src-tauri/src/diagnostic_log/mod.rs`: read the level with `expiry::read`, reset an ended or foreign `verbose`, apply the level (`level::apply`, the sink's limits), and assign a fresh opaque session token and emit `haex:diagnostic-log-changed` with status at the end of every `begin`; while `verbose` is on start a task that sleeps until its monotonic deadline, then resets the level, applies it and emits the event again; track it as session work and cancel it in `end`
- [x] T020 [US1] Implement `diagnostic_log_get`, `diagnostic_log_set` and `diagnostic_log_ui_message` in `commands.rs`; get returns the current session token; UI-message compares its supplied token with the attached session and drops stale tokens; set immediately reduces archives on verbose-to-standard to four 256 KiB files total, starts/replaces expiry on verbose, cancels expiry on standard, and emits status/events; register and test commands until T015 passes
- [x] T021 [P] [US1] Add the texts to `src/i18n/locales/de.json` and `src/i18n/locales/en.json` under `settings.diagnosticLog`: title "Protokoll", the levels "Standard" and "Ausführlich", "Endet am {time}", the note that the log lies in the vault's folder and goes with it when the folder is copied (FR-017), the note that more detail lines come with later steps, "Das Protokoll kann nicht geschrieben werden", the failure text; write both languages in the style of `settings.screenCapture`
- [x] T022 [US1] Add the catalog action `settings.diagnostics.logLevel.set` (`scope: 'guardrails'`, `effect: 'write'`, input `{ level: 'standard' | 'verbose' }`, result the status) in `src/lib/actions/settingsActions.ts` after the screen capture action, its handler in `src/stores/settingsActionHandlers.ts` calling `diagnostic_log_set`, and the location entry in `src/lib/settings/registry.ts` (title and description from T021, next to `settings.screenCapture.title`); run `pnpm check:settings` and `pnpm check:agent-actions` until they pass
- [x] T023 [US1] Create `src/components/settings/DiagnosticLogSetting.vue` (a segmented choice for the two levels, the end time while "Ausführlich" is on, the two notes of T021, the unwritable notice from the status and from the event; the shape of `src/components/settings/ScreenCaptureSetting.vue`: `useActionOrThrow`, `invoke('diagnostic_log_get')`, a failure goes to `toast.error`, no success text anywhere) and mount it in `src/components/settings/BasicView.vue` below the session restore setting

**Checkpoint**: US1 works alone. SC-001 (three steps, no restart), SC-002, SC-003 and SC-005 hold.

---

## Phase 4: User Story 2 - Errors of the interface are in the same log (Priority: P2)

**Goal**: warnings, errors, uncaught errors, rejected promises and view errors of the main window appear in the same log with the origin `ui`, as text and stack only.

**Independent Test**: in the dev tools console run `console.error(new Error('probe'))`; a `[ui]` line with the stack appears in `haex.log`; `console.debug('x')` appears only in "Ausführlich".

### Tests for User Story 2 (write first)

- [x] T024 [P] [US2] Write `scripts/check-diagnostic-log.ts` (node test, in the style of `scripts/check-platform-probe.ts`) for the pure helpers: the text of several arguments is joined with spaces; an `Error` gives its message and stack; a plain object or an array becomes `[object]` and none of its fields appear (FR-006); a long text is cut at 8 KiB with a marker; the token bucket lets 200 messages through in a burst and 100 a second afterwards and reports the count it dropped once per second (FR-014); `info` and `debug` are dropped in "Standard" and kept in "Ausführlich"
- [x] T025 [P] [US2] Add `"check:diagnostic-log": "node --test scripts/check-diagnostic-log.ts"` to `package.json` and a step "Check diagnostic log" to `.github/workflows/ci.yml` next to the other `check:*` steps (same `if: needs.changes.outputs.non_docs == 'true'`); the branch carrying a workflow change is pushed with the account that has the workflow scope

### Implementation for User Story 2

- [x] T026 [US2] Implement `src/lib/diagnosticLog.ts` (pure helpers of T024: `messageText(args)`, `createBucket(now)`, `allows(level, verbose)`; no DOM global, no import of the plugin) until T024 passes
- [x] T027 [US2] Create `src/plugins/diagnosticLog.client.ts` (a Nuxt client plugin): wrap `console.warn` and `console.error` always and `console.info` and `console.debug` while verbose is on, set `window.onerror` and `window.onunhandledrejection`, and register `nuxtApp.hook('vue:error', …)` and `nuxtApp.hook('app:error', …)` for errors of the view layer (never assign `vueApp.config.errorHandler`: Nuxt sets it itself in `entry.js` and a replacement would drop its error pipeline), send sanitized severity, text and current `session_id` through `diagnostic_log_ui_message`, call the original `console` method as before, guard against recursion, start at "Standard", take level/token from `haex:diagnostic-log-changed` and call `diagnostic_log_get` once at plugin start for a reload; if an event arrives while that read is pending, the event takes precedence over its older response, write the "dropped N messages" line from the bucket
- [x] T028 [P] [US2] Register `diagnostic_log_ui_message` as an app command available only to the main window; verify extension frames cannot call it
- [x] T029 [US2] Review every `console.warn`, `console.error`, `console.info` and `console.debug` call site (`rg "console\.(warn|error|info|debug)" src --glob '*.{ts,vue}'`) for an argument that can carry an entry's field, a passphrase or a key; change each such call to pass a fixed text and the error's message, and list what was changed in the pull request note (research R7, FR-009) — Reviewed: no call passes an entry's field, a passphrase or a key (they pass fixed texts and caught errors). Operator decision (2026-11-22, after first choosing strict `[object]`): an error object is logged with its cause. A caught backend error arrives as `{ kind, reason }`, so `messageText` reads the text fields `kind`, `code`, `reason` and `message` of an object and no other field; an entry-like object stays `[object]`. `useExtensionFrame` logs only an extension event's type, not its payload; no other call site changed. The two display fallbacks that showed `String(cause)` (`useFilesManage`, `useFilesDriveActions`) use `errString` now.
- [x] T030 [P] [US2] Add tests for backend/UI order within one second (SC-006), dropping a UI message from vault A after vault B attaches (FR-007), and a startup status response arriving after a verbose event without overwriting its level/token

**Checkpoint**: US1 and US2 work together; the interface's failures and the backend's lines read as one story.

---

## Phase 5: User Story 3 - The user finds and passes on the log (Priority: P3)

**Goal**: on a computer a button opens the folder with the log; on Android "Protokoll teilen" offers the log to the system's share sheet.

**Independent Test**: desktop: press the button and see the file manager at `logs/`; Android emulator: press "Protokoll teilen" and see the share sheet with the file.

### Tests for User Story 3 (write first)

- [x] T031 [P] [US3] Extend `src-tauri/src/diagnostic_log/commands_tests.rs`: `snapshot_log(folder, target)` joins `haex.log.N … haex.log.1 haex.log` into one file in time order and leaves the originals unchanged and the log writing; `diagnostic_log_reveal` and `diagnostic_log_share` return `unsupported` on the platform that does not have them; the status reports `can_reveal` on desktop and `can_share` on Android

### Implementation for User Story 3

- [x] T032 [US3] Implement in `src-tauri/src/diagnostic_log/commands.rs`: `diagnostic_log_reveal` (desktop, `tauri_plugin_opener::OpenerExt::reveal_item_in_dir` on `haex.log`; no frontend permission needed), `diagnostic_log_share` (Android: `snapshot_log` into the app cache folder, then `app.haex_android().share_file(path, "text/plain")`), `can_reveal` and `can_share` from `crate::platform::capabilities()`; register both in `src-tauri/src/lib.rs`
- [x] T033 [US3] Add the call `share_file(path, mime)` to the Android plugin: the Rust wrapper in `src-tauri/plugins/haex-android/src/lib.rs` and `android.rs` (the non-Android build in `other.rs` returns `unsupported`), the command in `src-tauri/plugins/haex-android/android/src/main/java/space/haex/vault/android/HaexAndroidPlugin.kt` (an `Intent.createChooser` with `ACTION_SEND`, `EXTRA_STREAM` of a `content://` URI, `FLAG_GRANT_READ_URI_PERMISSION`), a `FileProvider` entry in the plugin's `AndroidManifest.xml` and a `res/xml/` paths file that exposes only the app cache folder used for snapshots, and the permission file of the new command in the plugin's `permissions/` — Done without a permission file: the plugin's `COMMANDS` list is empty, the core calls the Kotlin side directly. The Kotlin compiles only in CI's Android build (not run locally).
- [x] T034 [US3] Add the two buttons to `src/components/settings/DiagnosticLogSetting.vue`: "Ordner mit dem Protokoll öffnen" when `can_reveal`, "Protokoll teilen" when `can_share`; a failure goes to `toast.error`; texts in `de` and `en` under `settings.diagnosticLog`
- [ ] T035 [US3] Check on Android: `cargo clippy` for the target `aarch64-linux-android` (the NDK and the target are installed locally, see the notes of the earlier Android work) and the manual share on the emulator (quickstart step 6); write the result in the pull request note, the plugin has no Kotlin tests — Clippy for `aarch64-linux-android` (no default features) is clean locally; the manual share on the emulator is open. The path question is settled in the source: Tauri 2.12 resolves `app_cache_dir()` on Android to `activity.cacheDir` (`PathPlugin.kt`, `getCacheDir`) and the app sets no `appDirectoriesOverride`, so the file the core writes is in the folder the Kotlin check compares with (both sides are canonicalised). The share sheet itself has not been opened on an emulator.

**Checkpoint**: all three stories work.

---

## Phase 6: Polish and cross-cutting concerns

- [x] T036 [P] Write the ADR `docs/adr/00NN-vault-folder-log.md` with the next free number at that time: a plain-text log beside an encrypted vault, nothing before the unlock, one level per vault and device, the alternatives of research R1, R3 and R4; link it from `specs/098-diagnostic-log/research.md` — ADR 0027 written, linked from `research.md`.
- [x] T037 Review every `log::trace!`, `log::debug!`, `log::info!`, `log::warn!` and `log::error!` call in `src-tauri/src` and every error type whose `Display` is logged (`rg "log::(trace|debug|info|warn|error)!" src-tauri/src`) for a value of the user (a secret, an entry's field, a path of the home folder); change each such line to a fixed text plus the cause that names no value, as `corrupt()` in `passwords/import/legacy_haex_vault/mod.rs` does; list the changes in the pull request note (research R7, FR-009) — 591 calls reviewed; the ones that print a path of a file or a variable of the user were three: `files/transfer/entries.rs` (a `trash::Error` prints its `Debug` with the paths, and the raw path) and `files/transfer/local.rs` (two raw paths); they now use `Redacted` and the variant or `io::ErrorKind`. Left as they are because the cause is the point: `HaexError` and `DriveError` messages are texts of our own code (the import line that started this spec), relay and server addresses, ids and counts. Not exhaustive for every `{error}`: a wrapped library error whose `Display` holds a value would still pass; the review read the types behind `{error}` in the files that touch paths, URLs and passwords.
- [ ] T038 Run one real "Ausführlich" session of at least 30 minutes with a vault that syncs, measure the size per minute and which crate floods, then set the size constants of `rotation.rs` and the damping list of `level.rs` and update the numbers in `research.md` and `data-model.md` (SC-004 must hold); add a test in `src-tauri/src/diagnostic_log/rotation_tests.rs` that writes 30 minutes of the measured rate into the "Ausführlich" limits and finds the first line of the session
- [ ] T039 Run the full local checks before the push: `cargo fmt --check`, `cargo clippy --lib --tests -D warnings` in the memory scope, `cargo clippy` for `aarch64-linux-android`, `pnpm lint`, `pnpm typecheck`, `pnpm typecheck:scripts`, all `pnpm check:*`, and `prettier --check .` from the worktree folder (from the main checkout it writes nothing); strip the trailing spaces of the bindings; fix what they find
- [ ] T040 Walk through [quickstart.md](quickstart.md) by hand in a dev build and write the result of each of the eight steps in the pull request note; start the end-to-end run for the settings change with `gh workflow run ci.yml --ref <branch>` (it does not run on pull requests)

---

## Dependencies and order

- **Phase 1** has no external dependency.
- **Phase 2** needs T002 and T003; inside it: T005 → T006, T007 → T008, T009 → T010 (each test first), T011 and T012 need T010 and T008, T013 needs T012. It blocks every story.
- **US1 (Phase 3)** needs Phase 2 and T004 (the table). T014 to T017 are written first; T018 needs T014, T019 needs T018, T020 needs T019 and T015, T022 and T023 need T020 and T021.
- **US2 (Phase 4)** needs Phase 2 and the status command of T020 (the plugin reads it); it does not need T023. T026 needs T024, T027 needs T026, T020 and T028.
- **US3 (Phase 5)** needs Phase 2 and T020 and T023 for the buttons; T033 is independent of the Rust command T032 until both meet in the share call.
- **Polish (Phase 6)**: T036 and T037 any time after Phase 3; T038 after Phase 3; T039 and T040 last.

## Parallel opportunities

- Phase 1: T002, T003 and T004.
- Phase 2: the three test-first pairs T005, T007 and T009 touch different files and run together; so do T006, T008 and T010 once their tests exist.
- US1: T014, T015, T016, T017 and T021 together.
- US2: T024, T025, T028 and T030 together; T029 beside the plugin work.
- US3: T031 beside T033 (a Rust test and a Kotlin change).

```text
# US1, tests first, in parallel:
T014 expiry_tests.rs   T015 commands_tests.rs   T016 diagnostic_log_failure_trace.rs   T017 diagnostic_log_secrets.rs   T021 i18n
```

## Implementation strategy

- **MVP**: Phase 1, Phase 2 and US1. It delivers the log in every build, the switch, the end by itself and the trace of a failing step; SC-001, SC-002, SC-003 and SC-005 hold. It can be a pull request of its own.
- **Then** US2 (the interface in the same log) and US3 (folder and share) as separate pull requests, each independently testable; US3's Android part can follow the desktop part.
- **Always**: one pull request per phase boundary, the checks of T039 before each push, and no `debug!` lines for import, sync or opening a vault in this spec (they come with those areas, spec Decisions).
