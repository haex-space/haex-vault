---
description: 'Task list for spec 064-external-drives'
---

# Tasks: Externe Laufwerke im Dateibrowser

**Input**: Design documents from `/specs/064-external-drives/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/commands.md](./contracts/commands.md),
[contracts/ui.md](./contracts/ui.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires tests with every behaviour change.

- **Rust**: tests go into a `*_tests.rs` file next to the module, never inline `#[cfg(test)] mod tests`
  (the module gets `#[cfg(test)] #[path = "x_tests.rs"] mod tests;` like `drives.rs` today).
- **Frontend**: pure parts are checked in `scripts/check-files-*.ts` (`node --test`, no Vue).
- **Behaviour in the app**: the E2E scenario `scripts/e2e/scenarios/files-drives.test.ts`.

**Organization**: Grouped by user story in priority order: US1 (P1), US2 and US3 (P2), US4 (P3). Session
restore (FR-025) belongs to US1 but needs US3 for its locked variant, so it is its own phase after US4.
macOS and Windows backends are platform phases that serve every story.

**Shipping note** (plan.md "Slicing"):

| PR  | Content                                   |
| --- | ----------------------------------------- |
| A   | Phase 1, Phase 2 and US1 (Linux, Android) |
| B   | US2                                       |
| C   | US3 and US4                               |
| D   | Phase 7 (session restore)                 |
| E   | macOS                                     |
| F   | Windows                                   |
| —   | Phase 10 goes with the last PR            |

Each PR is reviewed and merged before the next starts on `main`. E and F only need A–C.

**Rules for every task**:

- **Before writing code**:
  - Before authoring a named function, component or module, query graphify (`graphify query` on
    `graphify-out/`) and extend a candidate if one fits.
  - Read a file before changing it, and keep hand-written files under 500 lines.
- **Rust**:
  - Never block the async executor: D-Bus, `diskutil`, Win32 and file system calls go into
    `spawn_blocking` or the monitor thread.
  - `unsafe` only in small blocks with a `// SAFETY:` comment and a test.
  - New `#[ts(export)]` types carry the `Drive` prefix (only `Drive.ts` exists today). After
    `cargo test`, run `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` like the CI.
- **Builds**: Cargo builds can drive this machine out of memory. Build only the affected targets with
  `-j 4`, and ask the operator before a full `cargo build` or `cargo test` of the whole crate.
- **Code with `cfg` branches**: clippy it locally for `aarch64-linux-android` too.
- **Passwords**: never in argv, logs (`tracing` fields included), `Debug` output, error messages or
  frontend state. Wrap them in a type without `Debug`/`Display`, or use `zeroize` if it is already a
  dependency.
- **UI text**:
  - Every new text goes into `src/i18n/locales/de.json` and `en.json` under `files.drives.*`.
    vue-i18n refuses `<…>` in messages, so write `{name}`.
  - `UiButton` in a form needs `type="button"`.
  - A missing UI building block goes into haex-ui, not a local copy.
- **Before every push**:
  - `pnpm exec prettier --write` on touched Markdown/JSON
  - `pnpm typecheck`, `pnpm typecheck:scripts` and all `pnpm check:*` (composables run in the Node
    sandbox checks)
  - `pnpm lint:rust`
- **Commits**: no agent attribution in commits or PR text.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US4)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Commit design, spec, plan, research, data model, contracts, quickstart, checklist and this file in `specs/064-external-drives/` and `.specify/feature.json` (`"feature_directory": "specs/064-external-drives"`)
- [x] T002 [P] Add a row to the table in `plans/README.md` after the 056–058 rows: "064: Externe Laufwerke im Dateibrowser (`specs/064-external-drives/`)", priority P2, effort L, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-27; Lieferungen PR A–F", gate "044 im Einsatz; Linux mit udisks2, macOS über diskutil, Windows ohne BitLocker-Passwort (FR-019)"
- [x] T003 [P] In `specs/044-file-browser/spec.md`:
  - under FR-002 add the note "Erweitert durch Spec 064: Laufwerke mit Zustand (eingehängt, nicht eingehängt, gesperrt), Einhängen, Entsperren, Auswerfen."
  - at the edge case "Der offene Ordner verschwindet" add "Umgesetzt mit Spec 064 (research R10)."

---

## Phase 2: Foundational (PR A, blocks all user stories)

**Purpose**: The new `Drive` model, the backend seam and the split of `volumes()`/`drives()`, with no
visible change yet. Agents and peers keep seeing only mounted drives.

- [x] T004 Add platform dependencies in `src-tauri/Cargo.toml` (research R1, R5, R6) and commit `Cargo.lock`. Each version must match what `Cargo.lock` already holds, so no second copy appears:
  - `zbus = "5"` under `[target.'cfg(all(target_os = "linux", not(target_os = "android")))'.dependencies]`. The default features include `async-io` and `blocking-api`, which the blocking proxies need. Compare `cargo tree -e features -i zbus --target x86_64-unknown-linux-gnu` before and after: the feature set must not change (no `tokio`), so nothing changes for `notify-rust`. If the existing build lacks `blocking-api`, adding it is fine; adding `tokio` is not.
  - `plist = "1"` under `cfg(target_os = "macos")`.
  - `windows-sys` under `cfg(windows)` with the features of research R6, at the newest version already in the lock.
  - Add a comment above each block naming spec 064 and the research id, like the other target blocks.
  - Done in PR A: only `zbus = "5"` (feature set unchanged, checked with `cargo tree`); `plist` and `windows-sys` come with PR E and PR F, where they are used.
- [x] T005 Turn `src-tauri/src/files/local/drives.rs` into the module directory `src-tauri/src/files/local/drives/`:
  - `mod.rs` keeps `space()` and the public API.
  - Move `parse_mountinfo`, `unescape`, `is_user_drive`, `VIRTUAL_TYPES` and `Mount` unchanged to `mountinfo.rs`.
  - Move `is_volume_id` and the Android path listing to `android.rs`.
  - Move the existing tests from `drives_tests.rs` to `mountinfo_tests.rs` and `android_tests.rs`, with `#[path]` attributes.
  - No behaviour change: `cargo test -j 4 --lib files::local::drives` is green.
- [x] T006 In `src-tauri/src/files/local/drives/mod.rs` replace `Drive` with the model of `data-model.md`:
  - `id`, `key`, `label`, `path`, `kind`, `state`, `release`, `encrypted`, `size`, `total`, `free`
  - new enums `DriveKind`, `DriveState`, `DriveRelease`, all `#[derive(Serialize, TS)]`, camelCase, exported to `src/types/bindings/`
  - the pure functions `sort_drives(&mut [Drive])` (system, internal, removable; then label case-insensitive with missing labels last; then id) and `display_name(&Drive) -> String` (label, else the last path segment, else id)
  - a `debug_assert`-free invariant checker `fn invariants_hold(&Drive) -> bool` used by tests (data-model.md "Invarianten")
- [x] T007 [P] Tests in `src-tauri/src/files/local/drives/mod_tests.rs`:
  - sort order across kinds and labels
  - `display_name` fallbacks
  - `invariants_hold` accepts each valid combination and rejects each broken rule of data-model.md
- [x] T008 Create `src-tauri/src/files/local/drives/backend.rs`:
  - `trait DriveBackend: Send + Sync { fn list(&self) -> Result<Vec<Drive>, DriveError>; fn mount(&self, id: &str) -> Result<String, DriveError>; fn unlock(&self, id: &str, secret: &DriveSecret) -> Result<String /* mounted path */, DriveError>; fn release(&self, id: &str) -> Result<bool /* ejected */, DriveError>; fn unlock_mode(&self) -> UnlockMode /* Passphrase | SystemDialog | None */; fn open_system_unlock(&self, id: &str) -> Result<(), DriveError>; }`
  - `DriveError` maps onto the new `FilesErrorCode`s of `contracts/commands.md`.
  - `DriveSecret` is a newtype around `String` without `Debug`/`Display`/`Serialize`.
  - `fn backend() -> Arc<dyn DriveBackend>` selects per platform: Linux udisks (falls back to mountinfo when udisks2 is unreachable), macOS, Windows, Android, else a mountinfo-free `/` backend as today.
  - A `#[cfg(test)] pub struct FakeBackend` with a scripted list and recorded calls.
- [x] T009 [P] Create `src-tauri/src/files/local/drives/fallback.rs`, a `DriveBackend` for "no udisks2", over `mountinfo` (FR-024):
  - every mount that passes `is_user_drive` becomes `state: mounted`
  - `kind: system` for `/`, else `internal`
  - `release: none`, `key: None`
  - `mount`/`unlock`/`release` return `driveUnsupported`
  - Tests in `fallback_tests.rs` use the `MOUNTINFO` fixture moved in T005.
- [x] T010 In `src-tauri/src/files/local/drives/mod.rs` add `pub fn volumes() -> Vec<Drive>` (backend list, sorted; a failing list logs once at `warn` and returns the fallback list) and redefine `pub fn drives() -> Vec<Drive>` as `volumes()` filtered to `state == mounted` (research R11)
- [x] T011 Update the consumers of the old `Drive` fields:
  - `src-tauri/src/files/agent/exec.rs` (`files.sources`, around line 170): `name` becomes `display_name(drive)`.
  - `src-tauri/src/files/peer/serve/ops.rs` (`roots` and `drive_entries`): `WireRoot.name` becomes `display_name`, `path` the unwrapped `path` of the mounted drive.
  - `src-tauri/src/files/browser_commands/navigate.rs` `files_sources`: uses `volumes()`.
  - Leave the callers of `space()` (`transfer/sync_copy.rs`, `browser_commands/change.rs`, `agent/exec_write.rs`) untouched. Only their import path may change to `drives::space`.
- [x] T012 [P] Add tests next to the touched modules:
  - In `src-tauri/src/files/agent/exec_tests.rs` (read it first), with a `FakeBackend` (inject through a `#[cfg(test)]` seam in `drives::backend`), `files.sources` lists only mounted drives.
  - In the peer `ops` tests, `roots` contains no unmounted or locked drive.
  - Done as a pure test instead: `mounted_only` in `mod_tests.rs`. Injecting a fake backend into `drives()` is not thread-safe, because the callers run it in `spawn_blocking`. `files.sources`, `roots` and `drive_entries` all call `drives()`, which is `mounted_only(volumes())`.
- [x] T013 Add the new codes to `FilesErrorCode` in `src-tauri/src/files/mod.rs`, each with a doc comment as in the table of `contracts/commands.md`: `DriveCancelled`, `DriveBusy`, `DriveGone`, `DriveFailed`, `UnlockFailed`, `DriveNoFilesystem`, `DriveUnsupported`, `NoVault`, `UrlTaken`. Then map `DriveError` into `FilesError`.
- [x] T014 Frontend consumers of `Drive`:
  - `src/components/files/Sidebar.vue`: `:key="drive.id"`, `data-testid` per `contracts/ui.md`, label via the helper of T015.
  - `src/components/apps/FilesApp.vue:62`: the start folder is the first drive with `state === 'mounted'`.
  - `pnpm typecheck` is clean. No visual change besides the order of the list.
- [x] T015 [P] Create `src/lib/files/drives.ts`, pure with relative imports only:
  - `driveLabel(drive, t)`: the label, else `t('files.drives.unnamed', { size })` with `formatFileSize`
  - `driveOf(path, drives)`: the mounted drive with the longest `path` prefix on a separator boundary, Windows drive letters case-insensitive
  - `remap(p, droot, root)`: `root + p.slice(droot.length)`, `null` when `p` does not start with `droot`
  - `isCurrentDrive(drive, current, drives)`
- [x] T016 [P] Create `scripts/check-files-drives.ts` with `node --test` cases for T015:
  - `driveOf`: prefix on a boundary (`/mnt/a` is not the drive of `/mnt/ab`), the longest prefix wins, root `/`, Windows `e:\` vs `E:\x`
  - `remap`: same root, new root, not a prefix → `null`
  - `driveLabel`: fallback

**Checkpoint**: `cargo test -j 4 --lib files::` (scoped) and `pnpm check:files` are green. The sidebar
looks as before. Agent and peer output are unchanged.

---

## Phase 3: User Story 1 - Eingesteckten Stick sehen und öffnen (Priority: P1) 🎯 MVP

**Goal**: The sidebar lists all drives with state, live. A click mounts and opens. Pulling a drive
removes it, and a tab on it jumps to the nearest existing folder (spec US1 scenarios 1–7 and 9, FR-001
to FR-007, FR-023, FR-024).

**Independent Test**: quickstart Linux steps 1, 2 and 8. The E2E scenario `files-drives` covers
"plug in", "mount", "pull".

### Linux backend (udisks2)

- [x] T017 [US1] Create `src-tauri/src/files/local/drives/udisks_classify.rs`, pure (research R2, R3):
  - Input: `struct UdisksObject { path, block: Option<BlockProps>, filesystem: Option<FsProps>, encrypted: Option<EncProps>, drive: Option<DriveProps> }` with exactly the properties of research R1/R2 (`IdLabel`, `IdUUID`, `IdUsage`, `HintIgnore`, `HintSystem`, `CryptoBackingDevice`, `Size`, `Drive`, `MountPoints` as byte strings, `CleartextDevice`, `Removable`, `Ejectable`, `CanPowerOff`).
  - `fn classify(objects: &[UdisksObject]) -> Vec<Drive>` applies rules 1–6 of R2:
    - `kind`: `system` for the block mounted at `/`, `removable` when its drive is `Removable`, else `internal`.
    - `release`:
      - `none` for `system`, for any non-mounted drive and for mount paths not passing `is_user_drive`
      - `eject` for removable
      - `unmount` for internal
    - `key`: lowercase `IdUUID` of the crypto backing block when there is one, else of the filesystem block.
    - `label`: of the filesystem block. A locked crypto block has only its own `IdLabel`, often empty.
    - `encrypted`: whether a crypto backing block exists.
    - `state`: `locked` for a crypto block with `CleartextDevice == "/"`.
    - Mount points: decode the first one as a NUL-terminated byte string.
- [x] T018 [P] [US1] Tests in `src-tauri/src/files/local/drives/udisks_classify_tests.rs`, built from a hand-written fixture that mirrors this machine (`lsblk` in the session of 2026-10-27):
  - NVMe with ESP `/boot/efi`, `/recovery`, LUKS `cryptdata` with LVM inside and root `/`, and plain dm-crypt swap
  - a USB stick with two partitions (label "Linux Mint …" iso9660, "writable" ext4)
  - an unmounted removable vfat `FOTOS`
  - a locked LUKS stick, the same stick unlocked with ext4 `BACKUP`, an internal unmounted NTFS partition with `HintSystem = true`, and an object with `HintIgnore = true`
  - Assert, for each case:
    - exactly which drives appear (no ESP, recovery, swap, LVM cleartext; `HintSystem` does not hide)
    - kinds, states, `release`, `key` (crypto UUID for both locked and unlocked `BACKUP`), labels
    - the order after `sort_drives`
    - that every result passes `invariants_hold`
- [x] T019 [US1] Create `src-tauri/src/files/local/drives/udisks.rs` (Linux only):
  - zbus 5 blocking proxies, hand-written with `#[zbus::proxy]` and `blocking_name`, for `org.freedesktop.DBus.ObjectManager` (`GetManagedObjects`) and `org.freedesktop.UDisks2.{Block,Filesystem,Encrypted,Drive}`, with the methods of research R1.
  - `UdisksBackend::connect()` opens the system bus once; `list()` converts `GetManagedObjects` into `UdisksObject`s and calls `classify`.
  - `mount(id)` calls `Filesystem.Mount({})` and returns the path.
  - Map `org.freedesktop.UDisks2.Error.*` names to `DriveError`: `NotAuthorizedDismissed`/`Cancelled` → `driveCancelled`, `DeviceBusy` → `driveBusy`, `UnknownMethod`/missing object → `driveGone`, anything else → `driveFailed` with the D-Bus message.
  - `unlock_mode()` = `Passphrase`.
  - `unlock`/`release` stay `driveUnsupported` until US3/US2.
  - Keep the file under 500 lines by putting the proxies in `udisks_proxy.rs`.
  - Done with zbus's generic `blocking::Proxy::call` and `blocking::fdo::ObjectManagerProxy`. No hand-written proxies, so no `udisks_proxy.rs`.
- [x] T020 [P] [US1] Tests in `src-tauri/src/files/local/drives/udisks_tests.rs`:
  - the error-name mapping as a pure function `map_error(name: &str, message: &str) -> DriveError`
  - the variant → `UdisksObject` conversion from a `HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>` built in the test (no bus)
- [x] T021 [P] [US1] Android backend in `src-tauri/src/files/local/drives/android.rs`:
  - today's `/storage/emulated/0` + `is_volume_id` listing as a `DriveBackend`
  - `kind: removable` for card/USB ids, `internal` for `emulated/0`
  - `state: mounted`, `release: none`, `key: Some(id lowercase)`
  - `unlock_mode: None`; everything else `driveUnsupported` (FR-023)
  - Extend `android_tests.rs`.

### Monitor and commands

- [x] T022 [US1] Create `src-tauri/src/files/local/drives/monitor.rs` (research R4, R8):
  - `DriveMonitor` holds `Mutex<HashMap<u64, Channel<DrivesChanged>>>`, the last list and a stop flag.
  - `subscribe(channel) -> u64` sends the current list at once and starts the thread on the first subscriber. The thread is a `std::thread` named `haex-drives`, calls `backend.list()` every 2 s, and sends `DrivesChanged { drives }` when `changed(prev, next)` says so.
  - `unsubscribe(id)` stops the thread after the last one; `unsubscribe_all()`.
  - A failed send (closed channel) drops that subscriber.
  - `pub fn changed(prev: &[Drive], next: &[Drive]) -> bool` ignores `free` only (data-model.md "DriveDiff").
  - Make the interval a constructor argument so tests can use a short one.
- [x] T023 [P] [US1] Tests in `src-tauri/src/files/local/drives/monitor_tests.rs` with `FakeBackend` and a test channel:
  - first message at subscribe
  - a change → one message
  - a change of `free` only → no message
  - last unsubscribe stops polling (the fake's call count stops growing)
  - a dropped subscriber is removed
- [x] T024 [US1] Create `src-tauri/src/files/browser_commands/drives.rs`:
  - `files_drives_watch(channel) -> u64`, `files_drives_unwatch(id)`
  - `files_drive_open(id) -> DriveOpened`. For now only `opened`: when mounted return `path`, when unmounted `mount` in `spawn_blocking`; `driveGone` when the id is not in the backend's current list.
  - The types `DriveOpened` (all variants of `contracts/commands.md`, unused ones included so the binding is final) and `DrivesChanged` as `#[ts(export)]`.
  - Manage `DriveMonitor` as Tauri state (where `FilesState` is created).
  - Register the commands in `src-tauri/src/lib.rs` next to `files_sources`, and call `drive_monitor.unsubscribe_all()` in `on_page_load` `Started` next to `files.unwatch_all()`.
- [x] T025 [P] [US1] Tests in `src-tauri/src/files/browser_commands/drives_tests.rs`: `files_drive_open` on mounted / unmounted (fake records `mount`) / unknown id (`driveGone`), and a fake whose `mount` fails with `driveGone` (stick pulled while mounting, spec edge case), through the inner function that takes the backend (the command is a thin wrapper)

### Frontend

- [x] T026 [US1] Add to `src/composables/useFiles.ts`: `drivesWatchAsync(onChange)` (a `Channel<DrivesChanged>`, like `watchAsync`), `drivesUnwatchAsync(id)`, `driveOpenAsync(id)`.
- [x] T027 [US1] Create `src/composables/useFilesDrives.ts`, a module-level singleton:
  - `drives: Ref<Drive[]>`, `start()` (subscribes once), `stop()` (unsubscribes), `busy: Ref<Set<string>>`
  - Seed `drives` from `files_sources` when the first message has not arrived yet.
  - It must not touch DOM globals, so the Node sandbox checks keep working.
- [x] T028 [US1] Wire `src/components/apps/FilesApp.vue`:
  - `useFilesDrives().start()` where `openFrames` goes from 0 to 1 (`onMounted`, around line 136), `stop()` where it returns to 0 (`onBeforeUnmount`, around line 144)
  - pass `drives` to the sidebar instead of `sources.drives`
  - Keep the change small (research R14, spec 062 will move the panel part).
- [x] T029 [US1] Create `src/composables/useFilesDriveActions.ts` with `openDrive(drive)`:
  - marks `busy`, calls `driveOpenAsync`, on `opened` emits the path to navigate
  - errors → toasts of `contracts/ui.md` (`driveCancelled`, `driveFailed`, `driveGone`)
  - `needsPassword`/`systemDialog` are left for US3.
- [x] T030 [US1] `src/components/files/Sidebar.vue` per `contracts/ui.md`:
  - icon by kind and state (`lucide:usb` for removable, `lucide:hard-drive` otherwise, `lucide:lock` for locked); unmounted rows dimmed
  - spinner while `busy`; free space only when mounted
  - highlight via `isCurrentDrive`
  - a click on a mounted drive emits `pick(path)` as today, on others it calls `openDrive` and then `pick`
  - Keep `data-testid="files-drive-…"`.
- [x] T031 [US1] Nearest existing folder (research R10, spec 044 edge case, US1 scenario 7):
  - In `src/composables/useFilesFolder.ts`, when `files_list` fails with `notFound` for a `device` source, walk up with `parentPath` (from `src/lib/files/state.ts`) using `statAsync` until a folder exists, replace the location with it (no new history entry), and show the toast "{folder} gibt es nicht mehr – zeige {target}". When nothing exists up to the root, use the start folder.
  - Put the walk as a pure async function `nearestExisting(path, exists)` in `src/lib/files/drives.ts` and test it in `scripts/check-files-drives.ts`.
  - Done in its own composable `useFilesNearestFolder.ts`, wired into `Panel.vue` (spec 062 moved the folder into the panel). `useFilesFolder.ts` is unchanged.
- [x] T032 [P] [US1] Texts in `src/i18n/locales/de.json` and `en.json` under `files.drives`: `unnamed`, `mount`, `mounting`, `cancelled`, `failed`, `gone`, `gone_jump`. Check that `files.drives` does not collide with `files.sidebar.drives` (a string): it is a new object at `files.drives`, which is free.

### E2E (Linux)

- [x] T033 [US1] CI spike (research R12) in `.github/workflows/ci.yml` job `e2e`:
  - add `udisks2 cryptsetup` to the `apt-get install` list
  - add a step that writes `/etc/polkit-1/rules.d/49-haex-e2e.rules` with sudo: a JavaScript rule returning `polkit.Result.YES` for `action.id.indexOf("org.freedesktop.udisks2.") == 0 && subject.user == "runner"`
  - restart `polkit` and `udisks2`
  - run `udisksctl status` as a smoke check
  - Push the spike alone first and check the job log. If udisks2 or dm-crypt does not work on the runner, record that in `research.md` R12, drop T034–T035 and the E2E tasks of later phases, and rely on quickstart.
  - Done as `scripts/e2e/udisks-ci-smoke.sh`, which also mounts a vfat and a LUKS2 image. It passed in the CI run of #622; result in `research.md` R12.
- [x] T034 [US1] Create `scripts/e2e/lib/udisks.ts`:
  - `udisksAvailable()`: `udisksctl status` succeeds
  - `makeVfatImage(label)`: `truncate` in `$RUNNER_TEMP` or `os.tmpdir()`, `mkfs.vfat -n` via `sudo`
  - `makeLuksImage(label, password)`: `cryptsetup luksFormat --type luks2 -q` with the password on stdin, then open, `mkfs.ext4 -L`, close
  - `attach(img) → /dev/loopN` (`udisksctl loop-setup -f … --no-user-interaction`), `detach(dev)`
  - cleanup in `after()`
  - Pattern of the docker skip in `scripts/e2e/lib/rustfs.ts`: scenarios skip when `udisksAvailable()` is false.
- [x] T035 [US1] Create `scripts/e2e/scenarios/files-drives.test.ts` (style of `files-basic.test.ts`; read it first). US1 part:
  - open the file browser, attach a vfat image labelled `E2EFOTOS`: within the SC-001 limit (3 s plus the runner tolerance constant the other timed scenarios use; T074 adds the exact measurement) the sidebar shows `files-drive-<drive-id>-E2EFOTOS` with the USB/removable icon
  - click it: it mounts and the folder view shows the image's test file
  - detach it while a tab shows it: the entry disappears and the tab jumps to an existing parent folder with the toast
- [ ] T036 [US1] `cargo test -j 4 --lib files::`, `pnpm check:files`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm test:e2e --grep files-drives`, clippy for `x86_64-unknown-linux-gnu` and `aarch64-linux-android`, then quickstart Linux steps 1, 2, 8 by hand
  - The checks and `files-drives` passed in the CI run of #622 (`files-drives` 8.3 s, not skipped). Open: quickstart Linux 1, 2, 8 by hand.

**Checkpoint**: US1 works on Linux and Android (MVP, PR A).

---

## Phase 4: User Story 2 - Laufwerk sicher auswerfen (Priority: P2)

**Goal**: Eject and unmount release haex's own handles first, ask about running transfers and report
the outcome (FR-008 to FR-012, US2 scenarios 1–6).

**Independent Test**: quickstart Linux step 3, and the E2E "eject" case.

- [x] T037 [US2] `src-tauri/src/files/state.rs`: add `root: PathBuf` to `Watching::Local` (both branches of `files_watch` in `browser_commands/navigate.rs` know the resolved `real` path) and `pub fn unwatch_under(&self, prefix: &Path) -> usize` with `retain`. Test it in `state_tests.rs`.
- [x] T038 [P] [US2] `src-tauri/src/files/media/server.rs`: add `root: Option<PathBuf>` to `Grant` (set by `files_open` for local sources, `None` otherwise) and `pub fn release_under(&self, prefix: &Path) -> usize`. Test it in the server's `*_tests.rs` (read first): grants under the prefix are gone (404 afterwards), others stay.
  - Done as a new `register_file(tab, root, source)` next to an unchanged `register`: passwords and the platform probe register sources without a file of this device.
- [x] T039 [P] [US2] `src-tauri/src/files/search_manager.rs`: keep `(root, token)` per search and `pub fn cancel_under(&self, prefix: &Path) -> usize`; test.
- [x] T040 [P] [US2] `src-tauri/src/files/transfer/mod.rs`:
  - `pub fn touching(&self, prefix: &Path) -> Vec<String>` (ids of running transfers whose `Job.sources`/`target` or `RemoteJob` side `Side::Device` paths start with `prefix`)
  - `pub fn cancel_touching(&self, prefix: &Path) -> usize` while keeping every matching job
    trackable and awaiting its worker before `unwatch_under`, `release_under`, `cancel_under`,
    `end_under` and `backend.release`; `TransferManager::cancel` must not remove a job until its
    completion can be observed
  - Test it in `transfer` tests (local copy into the prefix, remote job from a device path, storage job ignored).
  - A running job that the window cancels now stays known until its worker has ended (a watch channel per job), and a cancelled job that ends as failed is forgotten too.
- [x] T041 [US2] Extension watches: in `src-tauri/src/extensions/fs/watch.rs`, call the existing `end_unless` with a predicate "root not under prefix" through a small `pub fn end_under(prefix)` wrapper; test in its `*_tests.rs`.
  - `end_under` filters the watches itself instead of going through `end_unless`: `end_unless` keeps the watches of a dialog choice, which must end before a release as well.
- [x] T042 [US2] `src-tauri/src/files/local/drives/udisks.rs` `release(id)`:
  - `Filesystem.Unmount({})`
  - when `encrypted`, `Encrypted.Lock({})` on the backing crypto block (FR-011)
  - for removable drives `Drive.PowerOff({})` when `CanPowerOff`, else `Drive.Eject({})` when `Ejectable`; returns whether one of them ran
  - `DeviceBusy` → `driveBusy`
  - Add the mapping cases to `udisks_tests.rs`.
  - The steps are a pure `release_steps` in `udisks_classify.rs`, tested there. A stick with another mounted partition is only unmounted (switching it off would pull the other one). When switching off fails after the unmount, the result is `ejected: false` instead of an error.
- [x] T043 [US2] `files_drive_release(id, cancel_transfers) -> DriveReleased` in `src-tauri/src/files/browser_commands/drives.rs` (`contracts/commands.md` order):
  - `driveGone` when the id is unknown
  - `driveUnsupported` when `release == none`
  - `transfersRunning { count }` when `touching` is non-empty and `cancel_transfers` is false
  - else `cancel_touching`, then `unwatch_under`, `release_under`, `cancel_under`, `end_under`, then `backend.release` in `spawn_blocking`
  - Register it in `lib.rs`. Tests in `drives_tests.rs` with fakes for the order and for each outcome.
  - The four releases sit behind a small `Holders` trait, so the tests check the order with fakes.
- [x] T044 [US2] Frontend:
  - `useFiles.ts` `driveReleaseAsync(id, cancelTransfers)`
  - `useFilesDriveActions.ts` `releaseDrive(drive)`: on `transfersRunning` show the confirm dialog of `contracts/ui.md` and repeat with `true`; toasts for `released` (ejected or not), `driveBusy`, `driveCancelled`, `driveFailed`
  - `Sidebar.vue`: the button for `release != 'none'` (`lucide:arrow-up-from-line`, title "Auswerfen"/"Aushängen", `data-testid="files-drive-eject-…"`), not shown while busy
  - texts in `de.json`/`en.json`
- [x] T045 [US2] Extend `scripts/e2e/scenarios/files-drives.test.ts` with the US2 part:
  - mount the vfat image, open a text file of it in the viewer, eject: success toast, the drive is no longer mounted (`udisksctl info` shows no mount point)
  - start a copy of a large file onto it and eject: the confirm dialog appears; cancel keeps it mounted
  - the root `/` has no eject button
  - The running transfer is a copy that waits at a taken name, not a large file (a large copy onto a loop image ends before the click). The page clicks the button itself because the open conflict dialog keeps pointer clicks out. Its own scenario `files-drives-eject` in the same file.
- [ ] T046 [US2] Checks as in T036 for the touched modules, then quickstart Linux step 3 by hand (including `less` holding the drive → "wird noch … benutzt")

**Checkpoint**: US1 and US2 work (PR B).

---

## Phase 5: User Story 3 - Verschlüsseltes Laufwerk entsperren (Priority: P2)

**Goal**: A locked drive opens through the unlock dialog with a typed password (FR-013 to FR-015,
FR-017 to FR-020, US3 scenarios 1–5). The password manager comes in US4, but `files_drive_open`
already looks for saved entries; without any it always shows the dialog.

**Independent Test**: quickstart Linux step 4, and the E2E "LUKS" case.

- [ ] T047 [US3] `src-tauri/src/files/local/drives/udisks.rs` `unlock(id, secret)`:
  - `Encrypted.Unlock(secret, {})` → cleartext object, then read it again
  - with no `Filesystem` interface → `Encrypted.Lock` again and `driveNoFilesystem` (FR-020)
  - else `Filesystem.Mount({})` → path; if mounting fails after the unlock, lock the cleartext
    device again and return the mount error
  - `Error.Failed` during unlock → `unlockFailed` with the message
  - The secret is passed as `&str` borrowed from `DriveSecret` only for the call.
  - Test the decision logic (cleartext without filesystem, error mapping) in `udisks_tests.rs` with the pure helpers.
- [ ] T048 [US3] Create `src-tauri/src/files/local/drives/unlock.rs` for the lookup half (US4 adds remembering):
  - `pub fn entry_url(key: &str) -> String` (`haex:drive:` + lowercase key)
  - `pub fn matching_entries(passwords: &PasswordsService, key) -> Result<Vec<DriveEntry>, DriveError>`: `list_headers(&Caller::User, &[])` filtered on the exact URL
  - `pub fn read_password(passwords, item_id) -> Result<DriveSecret, DriveError>` via `read_secret_item(&Caller::User, &[], id)`
  - No open vault (`state.database()` fails with `NoActiveInstance`) → `NoVault`.
  - `DriveEntry` (`itemId`, `title`, `username`, `url`) is `#[ts(export)]`.
- [ ] T049 [P] [US3] Tests in `src-tauri/src/files/local/drives/unlock_tests.rs` against a test vault (use the passwords `test_support.rs`; read it first):
  - `entry_url` lowercases
  - 0, 1 and 2 matching entries; an entry with the URL in another case or with a suffix does not match
  - an entry in the trash does not match
  - `read_password` returns the password
  - The password never appears in the `Debug` of anything returned (assert on `format!("{:?}", …)` of `DriveEntry` lists).
- [ ] T050 [US3] Extend `files_drive_open` for `state == locked` per `contracts/commands.md` steps 3–4:
  - `unlock_mode == SystemDialog` → `open_system_unlock`, `systemDialog`
  - `Passphrase`, depending on the matches:
    - exactly one → read and unlock (returns `opened`, or on `unlockFailed` → `needsPassword { reason: savedFailed, message }`)
    - none → `needsPassword { reason: none, candidates: [] }`
    - several → `reason: several` with the matches
  - Without a vault → `needsPassword { reason: none }`; the dialog learns `noVault` from the candidates call. This is a safeguard (FR-018, research R7): test it in `drives_tests.rs` with a state that has no database, not in the E2E.
  - Add `files_drive_unlock(id, secret, remember)` with `remember` accepted but only `no` implemented yet (`yes` → `driveUnsupported` until T055), and `files_drive_unlock_candidates(query)` (entries with a password, titles/usernames/URLs only, pattern `remote_storage::credentials::candidates`).
  - Register them in `lib.rs`. Tests in `drives_tests.rs` with the fake backend and a test vault: each branch, and that a typed secret reaches the fake's `unlock` unchanged.
- [ ] T051 [US3] Frontend:
  - `useFiles.ts` wrappers `driveUnlockAsync`, `driveUnlockCandidatesAsync`
  - `useFilesDriveActions.ts` handles `needsPassword` (opens the dialog) and `systemDialog` (toast "Entsperren im Dialog des Systems")
  - Create `src/components/files/DriveUnlockDialog.vue` per `contracts/ui.md`:
    - password field (`type=password`, cleared after every attempt and on close), `unlockFailed` shown under the field
    - the "Aus Passwortmanager wählen" list via `driveUnlockCandidatesAsync`, reusing `src/components/settings/storage/CredentialsEntryPicker.vue` if its props fit (read it; if it is storage-specific, extract the generic list into haex-ui or `components/ui` first, as the repo rule for missing building blocks says)
    - the hint line for `savedFailed`/`several`
    - the "merken" checkbox rendered but disabled until US4
  - Texts in `de.json`/`en.json`.
- [ ] T052 [US3] Extend `scripts/e2e/scenarios/files-drives.test.ts` with the US3 part:
  - attach the LUKS image `E2EBACKUP` (password from a constant in the scenario): the sidebar shows a locked drive
  - click → dialog; a wrong password → error, dialog stays
  - the right password → the folder view shows the test file
  - eject → the crypto device is locked again (`udisksctl info -b /dev/loopN` shows `CleartextDevice: '/'`)
- [ ] T053 [US3] FR-017 check: add a test in `drives_tests.rs` that runs `files_drive_unlock` with a typed secret through a capturing `tracing` subscriber (pattern: grep `tracing_subscriber` in existing `*_tests.rs`) and asserts the secret is not in any log line; then the checks as in T036 and quickstart Linux steps 4 and 6 by hand

**Checkpoint**: US1–US3 work on Linux.

---

## Phase 6: User Story 4 - Passwort aus dem Passwortmanager nehmen und merken (Priority: P3)

**Goal**: Saved passwords open a drive with one click, on every own device. The dialog can pick an
entry and remember (FR-013, FR-016, US4 scenarios 1–8).

**Independent Test**: quickstart Linux step 5.

- [ ] T054 [US4] `src-tauri/src/files/local/drives/unlock.rs`, the remembering half:
  - `remember_typed(passwords, key, label, secret)`: `create_item(&Caller::User, &[], ItemInput { title: "Laufwerk: {label}", password, url: entry_url(key), tags: vec!["haex-drive"], ..Default::default() }, None)`. The title prefix comes from a constant shared with the frontend text, or is passed in from the frontend in its language: read how `remote_storage/credentials.rs` builds "S3: …" and do the same.
  - `link_entry(passwords, item_id, key, replace: bool)`:
    - reads the header
    - URL already `entry_url(key)` → nothing
    - URL empty → `update_item` with `Patch::Set(url)` and the header's `updated_at`, and `set_tags` add `haex-drive`
    - other URL → `UrlTaken` unless `replace`
- [ ] T055 [US4] Implement `remember` in `files_drive_unlock`:
  - remember only after a successful `backend.unlock` (FR-016)
  - `typed` + `yes` → `remember_typed`
  - `entry` + `yes` → `link_entry(replace = false)`. A `UrlTaken` comes back **after** the drive is open, as `opened` with `rememberFailed: 'urlTaken'` (`contracts/commands.md`), so the UI can ask without unlocking twice
  - add `files_drive_link_entry(id, itemId)`: `link_entry(replace = true)` for the drive's `key`; register it in `lib.rs`
  - A failing remember never closes the drive.
- [ ] T056 [P] [US4] Tests in `unlock_tests.rs` and `drives_tests.rs`:
  - typed + yes creates exactly one normal entry (no owner) with title, URL and tag
  - a failed unlock creates nothing
  - entry without URL gets the URL and the tag
  - entry with another URL → `urlTaken` and unchanged; with replace → replaced
  - entry already linked → unchanged
  - the next `files_drive_open` on the same key unlocks without a dialog (one match)
- [ ] T057 [US4] `DriveUnlockDialog.vue`:
  - enable the checkbox; its text "Im Passwortmanager merken" when typing, "Mit diesem Laufwerk verknüpfen" for a picked entry
  - on `rememberFailed: 'urlTaken'` ask "Der Eintrag hat schon die Adresse {url}. Durch die Laufwerks-Adresse ersetzen?" and on yes call `files_drive_link_entry(id, itemId)` (`contracts/commands.md`, added in T055)
  - preselect the matches for `reason: several`
  - grey out the picker and checkbox with the hint on `noVault` (FR-018)
  - Texts in `de.json`/`en.json`.
- [ ] T058 [US4] Extend `scripts/e2e/scenarios/files-drives.test.ts` with the US4 part:
  - unlock `E2EBACKUP` with "merken"
  - the password manager (open it through the app, as in the password manager scenarios; read one first) lists "Laufwerk: E2EBACKUP" with tag `haex-drive`
  - eject and click again: it opens without a dialog
  - change the entry's password to a wrong one, eject and click: the dialog shows the `savedFailed` hint
- [ ] T059 [US4] Checks as in T036, quickstart Linux step 5. The second-device part of SC-003 is manual (two instances with sync, as in the sync scenarios).

**Checkpoint**: US1–US4 work on Linux (PR C).

---

## Phase 7: User Story 1 (continued) - Sitzung auf einem nicht eingehängten Laufwerk (FR-025, PR D)

**Goal**: A restored tab whose drive is plugged in but not mounted, or locked, offers "Einhängen" or
"Entsperren" and then opens its folder, even at a new mount point (US1 scenario 8, FR-025).

**Independent Test**: quickstart Linux step 7.

- [ ] T060 [US1] `src/lib/files/registry.ts`:
  - add `drive` and `droot` to `FilesPlace` and to `KEYS`, both optional
  - `filesLocation` writes them only for the `device` source and only together
  - `parseFilesPlace` reads them and drops them when one of the two is missing or `p` does not start with `droot`
  - Extend `scripts/check-files-registry.ts`: round trip, only-one-key dropped, wrong prefix dropped, storage source ignores them. Also check that `src/lib/wm/session.ts` keeps them (it keeps any string keys); add a case to `scripts/check-wm-session.ts`.
- [ ] T061 [US1] Set the keys where device locations are built, using `driveOf(path, drives)` from `useFilesDrives` (only when the drive has a `key`):
  - `go()` in `src/components/apps/FilesApp.vue` (around line 152)
  - `onSidebarPick` (around line 288)
  - `src/stores/filesActionHandlers.ts` (around line 25)
  - If spec 062's `Panel.vue` has landed by then, apply the same in its location writer instead (research R14).
- [ ] T062 [US1] Create `src/composables/useFilesDriveRestore.ts`:
  - Given the place and the drive list: when the folder load failed with `notFound` and the place has `drive`, find the drive with that `key`.
    - Present and not mounted or locked → expose `notice: { drive, state }` and stop the nearest-folder walk of T031 (T031 must ask this composable first).
    - Absent → let T031's walk run.
  - `resolve()` runs `openDrive`/the unlock flow, then navigates to `remap(p, droot, newPath)` with `replace` and refreshes `droot`.
- [ ] T063 [US1] Create `src/components/files/DriveNotice.vue` per `contracts/ui.md` (text, one button, busy state, `data-testid="files-drive-notice"`) and show it in the folder area instead of the error. Texts in `de.json`/`en.json`.
- [ ] T064 [P] [US1] Extend `scripts/e2e/scenarios/files-restore.test.ts` (the existing restore scenario; read it first) or `files-drives.test.ts`:
  - open a subfolder on the mounted vfat image, restart with session restore while the image is attached but unmounted: the notice with "Einhängen" appears; the click opens the same subfolder
  - repeat with the LUKS image locked: "Entsperren" and the saved password from US4 open the subfolder without a dialog
- [ ] T065 [US1] `pnpm check:files`, `pnpm check:wm-state`, `pnpm typecheck`, `pnpm typecheck:scripts`, E2E as above, quickstart Linux step 7

**Checkpoint**: The spec is complete on Linux (PR D).

---

## Phase 8: Platform macOS (PR E)

**Purpose**: The macOS backend for all stories (research R5). Requires a Mac or the macOS CI runner;
the plist keys are UNVERIFIED until T066.

- [ ] T066 Spike on a Mac (or a `macos-latest` workflow run with `workflow_dispatch`):
  - capture the real outputs of `diskutil list -plist`, `diskutil info -plist <id>` for the boot volume, the EFI partition, the APFS Preboot/Recovery/VM/Update volumes, an external USB stick and a disk image (`hdiutil create -size 64m -fs APFS -encryption AES-256 -stdinpass`, attached), and `diskutil apfs list -plist` with that image locked
  - store them as fixtures in `src-tauri/src/files/local/drives/fixtures/macos/*.plist`
  - record in `research.md` R5 which keys exist (confirm or correct the UNVERIFIED list)
  - Do not commit any personal disk names or serials; anonymise them.
- [ ] T067 Create `src-tauri/src/files/local/drives/macos_plist.rs`, pure: parse the fixtures' structure with `plist` into `Drive`s (`kind` by `Internal`/`RemovableMediaOrExternalDevice`/root, `locked` from the APFS list, `key` = `VolumeUUID`/`APFSVolumeUUID` lowercase). Apply the visibility rule of research R5 (no system roles, no EFI, `Data` not on its own, unmounted only when removable or locked). Missing keys mean "unknown", not an error. Tests in `macos_plist_tests.rs` over the fixtures, including that none of EFI, Preboot, Recovery, VM, Update appears (SC-006).
- [ ] T068 Create `src-tauri/src/files/local/drives/macos.rs`:
  - fingerprint gate: names in `/dev` starting with `disk` plus the entries of `/Volumes`; `list()` reruns `diskutil` only when it changed, else it returns the cached list with fresh `free` values via `space()`
  - `mount` → `diskutil mount <id>`
  - `release` → `diskutil unmount` or `eject`
  - `unlock` → `diskutil apfs unlockVolume <id> -stdinpassphrase` with the secret written to stdin and stdin closed; never `-passphrase` (FR-017)
  - map the exit status and stderr to `DriveError`
  - Test the argument lists (a pure `fn command_for(action) -> Vec<&str>` asserting `-stdinpassphrase` and no secret in the args) in `macos_tests.rs`.
- [ ] T069 Build and clippy for `aarch64-apple-darwin` on the macOS CI job (`pnpm lint:rust` equivalent there), then quickstart macOS by hand

---

## Phase 9: Platform Windows (PR F)

**Purpose**: The Windows backend (research R6). Requires a Windows machine or the Windows CI runner for
the spikes.

- [ ] T070 Spikes on Windows, results into `research.md` R6:
  - (a) whether `CM_Request_Device_EjectW` must target the disk devinst or its parent for a USB stick and a USB HDD
  - (b) which error `GetVolumeInformationW` returns for a locked BitLocker To Go volume without admin rights (so it can show as `locked`)
  - (c) whether `ShellExecuteExW` with verb `unlock-bde` on `X:\` opens the unlock dialog, else `bdeunlock.exe X:`
- [ ] T071 Create `src-tauri/src/files/local/drives/windows.rs`:
  - list volumes with drive letters via `FindFirstVolumeW`/`FindNextVolumeW`/`GetVolumePathNamesForVolumeNameW`, never `GetLogicalDrives`
  - label and file system via `GetVolumeInformationW`; `locked` per T070 (b)
  - `kind` via `IOCTL_STORAGE_QUERY_PROPERTY` on a handle opened with access 0; the Windows directory's volume is `system`
  - `key` = lowercase volume serial as hex (only used for the password URL, which BitLocker cannot use anyway, and for FR-025, which has no unmounted state here)
  - `release: eject` for removable, `none` otherwise
  - `release` per T070 (a) with a veto buffer; a veto → `driveBusy` with the veto name
  - `unlock_mode: SystemDialog`, `open_system_unlock` per T070 (c)
  - Each `unsafe` call in a small wrapper with a `// SAFETY:` comment.
- [ ] T072 [P] Tests in `windows_tests.rs` for the pure parts: multi-string parsing of `GetVolumePathNamesForVolumeNameW` output, bus type → kind, veto type → error. Run them on the Windows CI job.
- [ ] T073 Clippy and tests on the Windows CI job, then quickstart Windows by hand

---

## Phase 10: Polish & Cross-Cutting

- [ ] T074 [P] SC-001 check: in `files-drives.test.ts` measure the time from `udisksctl loop-setup` to the sidebar entry and assert ≤ 3 s, plus the runner tolerance the other timed scenarios use (grep for their tolerance constant first)
- [ ] T075 [P] Update `docs/plans/2026-10-27-external-drives-design.md` with the deviations decided in planning:
  - the visibility rule follows gvfs, not `HintSystem`
  - Windows: no unmount for internal drives and no letterless volumes; BitLocker uses the system dialog
  - macOS lists through `diskutil` behind a fingerprint gate
  - the nearest-folder jump is implemented here
- [ ] T076 Set the status in `specs/064-external-drives/spec.md` to "Implemented" (or "Implemented, manual … open"), update the row in `plans/README.md`, and tick the checklist.
- [ ] T077 Final run as in CI:
  - `pnpm lint:all`, `pnpm typecheck`, `pnpm typecheck:scripts`, all `pnpm check:*`
  - `cargo test -j 4` of the touched modules
  - `pnpm test:e2e --grep files-`
  - Review the diff size per PR against the constitution's thresholds.

---

## Dependencies & Execution Order

- **Overall**: Phase 1 → Phase 2 → US1 (Phase 3, PR A) → US2 (PR B) and US3 (part of PR C) in either
  order → US4 (needs US3) → Phase 7 (needs US1, and US4 for its locked case) → Phase 10.
  - Phases 8 and 9 need Phases 2–6, not each other, and not Phase 7.
- **Inside Phase 2**:
  - T004 → T005 → T006 → T007, T008 → T009, T010 → T011 → T012
  - T013 any time after T008
  - T014 after T006; T015 → T016
- **Inside US1**:
  - T017 → T018, T019 → T020
  - T021 independent
  - T022 → T023, T024 → T025
  - T026 → T027 → T028 → T029 → T030
  - T031 after T026; T032 any time
  - T033 → T034 → T035; T036 last
- **Inside US2**: T037–T041 in parallel, then T042, T043, T044, T045, T046.
- **Inside US3**: T047 and T048 in parallel; T049 after T048; T050 after both; T051 after T050;
  T052, T053 last.
- **Inside US4**: T054 → T055 → T056; T057 after T055; T058, T059 last.
- **Inside Phase 7**: T060 → T061 → T062 → T063 → T064 → T065.

## Parallel Examples

- **Phase 2**: T007 (model tests), T009 (fallback backend), T015/T016 (frontend pure helpers) touch
  different files.
- **US1**: T018 (classification tests) while T019 (D-Bus proxies) is written; T021 (Android) and T023
  (monitor tests) independently; T032 (texts) any time.
- **US2**: T037 (watches), T038 (media), T039 (search), T040 (transfers), T041 (extension watches)
  are five different modules.
- **Platforms**: Phase 8 (macOS) and Phase 9 (Windows) by different people or runners in parallel.

## Implementation Strategy

- **MVP**: Phases 1–3 (PR A).
  - The sidebar shows all drives live on Linux and Android, mounts on click, and jumps to an
    existing folder when a drive goes away.
  - Agents and peers are unchanged.
- **Then**:
  - PR B (eject), PR C (unlock and password manager), PR D (session)
  - macOS (PR E) and Windows (PR F) independently.
  - Each PR keeps the app releasable. Until PR B no eject button exists; until PR C a locked drive
    shows the lock and a click says "nicht unterstützt" through `driveUnsupported`.
- **Manual-only checks**: quickstart Linux steps 6 (process list) and 8 (udisks2 stopped), SC-003 on a
  second device, macOS and Windows by hand, and Android with a real OTG stick.
