---
description: 'Task list for spec 044-file-browser'
---

# Tasks: Dateibrowser und Viewer

**Input**: Design documents from `/specs/044-file-browser/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md),
[contracts/agent-actions.md](./contracts/agent-actions.md), [contracts/media-server.md](./contracts/media-server.md),
[quickstart.md](./quickstart.md), design [`docs/plans/2026-10-07-file-browser-design.md`](../../docs/plans/2026-10-07-file-browser-design.md),
[ADR 0011](../../docs/adr/0011-native-catalog-actions.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic, and SC-005,
SC-006 and SC-007 need case corpora. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. No network services in `cargo test`:
`wiremock` for S3, `FakeStore` (`src-tauri/src/remote_storage/test_support.rs`) for the storage source.
Frontend logic that can run without a DOM lives in `src/lib/files/` and is checked by
`scripts/check-files-*.ts` (`node --test`).

**haex-vault references** use repository `https://github.com/haex-space/haex-vault` at revision
`fc4e84b61a050576ba42e0dc832d04064a8605a3` (constitution IV).

**Organization**: Grouped by user story in priority order (P1: US1, US2; P2: US3, US4, US5, US6; P3: US7).

**Shipping note** (plan.md, Lieferungen): PR A = Phase 1 (docs); PR B = Phase 2 + US1; PR C = US2;
PR D = US3; PR E = US4; PR F = US5 (needs US3 for transfers); PR G = US6 (storage permissions need
US5), split into G1 (native actions, permissions, all executors, `files.show`) and G2 (text from PDF
and office, images to models, the settings view of the permissions); PR H = US7. Each PR is reviewed and merged before the next one starts on top of `main`, in its own
worktree under `.worktrees/`.

**Every commit**: `cargo test` rewrites `src/types/bindings/InstanceInfo.ts` with trailing whitespace;
revert that file before committing unless the PR changes it on purpose. New or changed bindings are committed without trailing whitespace (`sed -i -E 's/[[:space:]]+$//' src/types/bindings/*.ts`): the CI strips it and compares, and `git diff --check` refuses it; doc comments on fields leave it.

**Every new Tauri command**: register it in `src-tauri/src/lib.rs` (`generate_handler!`). The list in
`src-tauri/src/vault_gate/invoke.rs` is not an allow list for normal use: it names the few commands that
still run after a vault close has started, and file commands must not be on it.

**Platforms in CI**: Linux runs the E2E scenarios (`pnpm test:e2e`); Windows, macOS and Android run the
self-checking `platform-probe` jobs of T007, because full E2E on those platforms is not built yet
(`scripts/e2e/PLATFORMS.md`).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US7)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 [P] Add a row for 044 to `plans/README.md` (priority P2, effort L, status "Spezifiziert und geplant 2026-10-07; Lieferungen PR A–H", gate: 038 in use; US6 needs the chat tool loop of 032). Add idea rows (status "Idee, Design-Entscheidungen 2026-10-07, noch nicht spezifiziert") for 045 Sync-Regeln (replaces 025), 046 Hintergrunddienst (Android, Desktop-Tray), and the rework of 027/029 (Space als Rahmen mit Einträgen). In the rows "Passwortmanager: Folgearbeiten" and "Passwortmanager-Redesign: Folgearbeiten" note that PDF-Vorschau and Vorschaubilder in Rust can reuse the viewer and `files/thumbnails.rs` from 044; add an idea row "E2E auf Windows, macOS und Android" (follow-up of `specs/033-multi-device-e2e/research.md` R9, after the model of haex-vault: reusable workflow for Windows/macOS, Android emulator with Maestro), which 044 does not build
- [x] T002 [P] ADR status: set `docs/adr/0011-native-catalog-actions.md` to `Status: accepted` (operator chose option A on 2026-10-07) and add one line at the end of `docs/adr/0006-actions-as-builtin-agent-tools.md`: "A catalog action may run in Rust instead of the frontend; see ADR 0011."
- [x] T003 [P] Align spec 017 in `specs/017-extension-host/research.md` (R19, files of the device): the path resolution, the list of holzi's own places (FR-049) and the watcher core move to `src-tauri/src/files/local/` with spec 044; extensions keep their behavior, and on Android and iOS `watch` keeps answering "nicht verfügbar" (8001) even though `notify` is now compiled for Android
- [x] T004 Run `pnpm format:check`, commit, push `044-file-browser`, open PR A against `main` (use `gh api -X POST …/pulls` if `gh pr create` hits the GraphQL bug) **Note**: PR A = haexmas/holzi#324; the first three docs commits had reached `main` through a push to the wrong upstream and were moved back into the PR, `main` now enforces its rules for admins too

---

## Phase 2: Foundational (PR B, start)

**Purpose**: Shared Rust core, the loopback media server and the app shell. Blocks every story.

- [x] T005 Prepare `.worktrees/044-b-core` from `main` after PR A: run `df -h` first and delete `target/` of merged worktrees if space is short; real `pnpm install`; `cargo check --manifest-path src-tauri/Cargo.toml` **Note**: worktree `.worktrees/044-b-core`, branch `044-b-core` (`--no-track`); `target/` copied from `review-pr-316`
- [x] T006 [P] Before the first new name, run bounded `graphify query … --budget 1200` for "extension filesystem resolve denied places watch", "passwords attachment preview thumbnail downscale", "action bridge register_agent_actions ActionTool", "wm app registry routes location"; note the candidates checked in the PR description (warn and continue if graphify fails) **Note**: run 2026-10-08; the graph is from 2026-09-16 and knows none of the asked areas (`extensions/fs`, the action catalog, the wm registry), so it returned nothing usable; the candidates come from the code research of plan.md (research R1, R3, R13)
- [x] T007 **Initial platform probe, before any media code** (research R4, Local Network Access risk): a cargo feature `platform-probe` (never in release bundles) that, when `HOLZI_PROBE=1` is set, starts the app before unlock, verifies the probe command's lifecycle and loopback reachability with a minimal internal `/__probe` health route, then reports `{ ok, steps }` through one JSON line (desktop: stdout and exit code; Android: `log::info!` with tag `holzi-probe`) and exits. This first probe has no media-server, playback, or fixture dependency and is the executable gate for T014–T016. CI in `.github/workflows/ci.yml`: a job `platform-probe` with matrix `windows-latest`, `macos-latest` (build with `--features platform-probe`, run with a 120 s timeout, fail on `ok: false` or timeout) and a job `platform-probe-android` that reuses the debug APK of `android-build` (built with the feature), starts an x86_64 emulator with `reactivecircus/android-emulator-runner` (KVM enabled, as in haex-vault `.github/workflows/build.yml` job `e2e-tests-android`), installs, `adb shell am start` with the probe extra, and reads the result from `adb logcat -s holzi-probe`. Record the first green run in research.md R4; if a platform fails, stop and escalate to the operator before T014. Note the cost of macOS runner minutes in the PR. After T014–T016 and T040 provide the media server and fixtures, T041 extends this probe with playback, seeking, and viewer checks. **Note**: PR #328 merged 2026-10-08: Windows (WebView2/Edge 153) and macOS green; Android emulator (WebView 124) fails with `TypeError: Failed to fetch`; the diagnosis (page context, policy violations, chromium logcat) runs in the follow-up PR; T014 waits for it (operator, 2026-10-08) **Resolved 2026-10-08** (PR #332): Android passes with `network_security_config.xml` (cleartext only to `127.0.0.1` in release, all in debug); `usesCleartextTraffic` in the manifest alone was not enough for the web view; the probe now reports connections the server saw, a self-connect from Rust and other ways to reach the loopback
- [x] T008 Dependencies in `src-tauri/Cargo.toml`: add `walkdir = "2.5"`, `image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp", "gif"] }`, `fast_image_resize = { version = "6.1", features = ["image"] }`; move `notify` and `notify-debouncer-full` from `cfg(not(any(android, ios)))` to `cfg(not(target_os = "ios"))`; `cargo check` for the host and `--target aarch64-linux-android` **Note**: the Android check runs in CI (`android-lint`); `image` 0.25.10 is the version mistralrs already pulls in
- [x] T009 [P] Tests first in `src-tauri/src/files/local/resolve_tests.rs` and `src-tauri/src/files/local/places_tests.rs`: move the cases of `src-tauri/src/extensions/fs/resolve_tests.rs`; add: a symlink into a denied place resolves to the denied place; `..` out of a known place; on Windows and macOS a denied path in different letter case is still denied (`#[cfg]`-gated cases); known places exist only if the directory exists
- [x] T010 Move the pure file core out of `src-tauri/src/extensions/fs/`: `resolve.rs` → `src-tauri/src/files/local/resolve.rs` (error type `FilesError`, the extension adapter maps it to `BridgeError`); the denied list and known places of `FsEnvironment` (`mod.rs:81-107`) → `src-tauri/src/files/local/places.rs` (`OwnPlaces::from_app(&AppHandle)`, `is_own(&Path) -> bool`, `known() -> Vec<Place>`); `extensions/fs` uses both; all extension tests stay green (`cargo test extensions::fs`) **Note**: `src-tauri/src/files/` already holds spec 043 (`picked`, `commands`); the core sits beside it as `files::local` and `files::FilesError` (new code `invalidPath` for the resolve errors); `OwnPlaces::touches` folds letter case on Windows and macOS, which extensions now get too
- [x] T011 Move the watcher core of `src-tauri/src/extensions/fs/watch.rs` into `src-tauri/src/files/local/watch.rs` (debounced `notify` watcher for one directory, non-recursive option, callback with created/removed/changed paths, `#[cfg(not(target_os = "ios"))]`); the extension adapter keeps its API and keeps answering 8001 on Android and iOS (it checks `PlatformCapabilities.folder_watch`, unchanged) **Note**: `files::local::watch::watch_folder` (recursive or not, `FolderWatch` ends on drop); the extension keeps its desktop-only `Watches` on top of it
- [x] T012 Types in `src-tauri/src/files/mod.rs`: `SourceRef { Device, Storage { storage_id } }`, `Entry` (fields of data-model.md), `FilesError` with the codes of contracts/tauri-commands.md (`exists`, `intoItself`, `noSpace`, `holziOwned`, `noAccess`, `notFound`, `binary`, `broken`, `tooLarge`, `blocked`, `notGranted`, `unsupported`), all `ts-rs` exported to `src/types/bindings/`; `Caller` is `crate::passwords::access::Caller` (no second enum) **Note**: in `src-tauri/src/files/mod.rs` beside spec 043; `FilesError` got the code `invalidPath`
- [x] T013 [P] Pure access rules, tests first in `src-tauri/src/files/access_tests.rs`, then `src-tauri/src/files/access.rs`: `check(caller, target_kind, real_path, own_places, grants) -> Result<AccessView, FilesError>` where `User` gets everything and own places as read-only (`holzi_owned`), `BuiltinAgent`/`ExternalAgent` get `blocked` for own places and `notGranted` without the `device` grant; storage grants `read`/`readWrite`/`denied`/absent (absent → `Ask`); writing to a storage needs `readWrite`. Corpus for SC-005: direct path, `..`, symlink, letter case (Windows/macOS), a search hit inside an own place **Note**: `check` returns `Allowed`, `ReadOnly` (user in an own place), `Ask` (storage without grant) or `Refused`; plus `visible_to` (lists, search hits) and `storage_listed`
- [x] T014 Media server, tests first in `src-tauri/src/files/media_server_tests.rs`: copy the range-parser cases of haex-vault `src-tauri/src/remote_storage/streaming/protocol.rs:300-350`; unknown token → 404, `/` → 404; `release` and `release_tab` → 404 afterwards; `OPTIONS` → 204 with the CORS headers of contracts/media-server.md; `HEAD` without body; a 1 GiB sparse temp file read through one `GET` keeps the server's buffer at ≤ 256 KiB (count the largest chunk handed to the writer) **Note**: tests in `files/media/{range,http,server}_tests.rs`; the flat-memory test uses a 64 MiB sparse file (the chunk bound does not depend on the size); a range whose end lies past the file is clamped (RFC 7233), where haex-vault answers 416
- [x] T015 Implement `src-tauri/src/files/streaming.rs` (`StreamingSource { size, read_range, content_type }`, `LocalFileSource`) and `src-tauri/src/files/media_server.rs` after haex-vault `src-tauri/src/media_server/mod.rs` (hand-written HTTP/1.1 on `TcpStream`, `127.0.0.1:0`, UUIDv4 tokens with `tab_id`), plus `release`, `release_tab`, `OPTIONS`, CORS headers, no `Content-Encoding`; started in `setup()` and stopped by `gate.token()`; kept in `AppState` **Note**: split into `files/media/{range,http,server}.rs`; the server is a Tauri managed state (`app.manage`), not a field of `AppState`; the accept loop stops with `gate.token()`
- [x] T016 CSP and Android: in `src-tauri/tauri.conf.json` add `media-src 'self' http://127.0.0.1:*` and `http://127.0.0.1:*` to `img-src` and `connect-src` (both `csp` and `devCsp`); add `src-tauri/gen/android/app/src/main/res/xml/network_security_config.xml` (cleartext only for `127.0.0.1`) and `android:networkSecurityConfig` in `src-tauri/gen/android/app/src/main/AndroidManifest.xml`; the `platform-probe` jobs of T007 must stay green **Done**: the Android `network_security_config` came with T007; the CSP entries with the media server
- [x] T017 `FilesService` skeleton in `src-tauri/src/files/service.rs` (`FilesService::new(state)`, every method takes `&Caller` first) and `src-tauri/src/files/browser_commands.rs` (`files/commands.rs` belongs to spec 043); register every command of this PR in `lib.rs`; commands fix `Caller::User` **Note**: no `FilesService` struct yet: `FilesState` (managed, own places, known places, thumbnail cache, watches) plus `files::access::check` in every command of `browser_commands.rs`; the service takes shape with agents (US6) and storages (US5)
- [x] T018 [P] App shell: `system.files` in `src/lib/wm/apps.ts` (`icon: 'lucide:folder'`, `multiInstance: true`, `tabTitle: 'location'`); `src/lib/files/registry.ts` with the locations of data-model.md (`/device`, `/storage/:id`, query `p`, `open`, `q`, `t`, `s`, `d`); routes in `src/components/wm/appRoutes.ts`; `src/components/apps/FilesApp.vue` (toolbar, sidebar via `useSidebarFrame`, `<WmRouterView/>`); i18n keys `wm.apps.files`, `wm.files.*`, `files.*` in `src/i18n/locales/{de,en}.json` **Note**: one mounted root `FilesApp` with children `""`, `device`, `storage/:id` (like the chat), `tabTitle: "app"`
- [x] T019 [P] `scripts/check-files-registry.ts` (locations parse and round-trip, unknown query keys dropped) and `"check:files": "node --test scripts/check-files-*.ts"` in `package.json`

**Checkpoint**: The app opens as an empty shell; the media server answers 404; extension tests green.

---

## Phase 3: User Story 1 - Dateien durchsuchen und ansehen (Priority: P1) 🎯 MVP (PR B)

**Goal**: Browse the device, list/grid with thumbnails, breadcrumbs, open text and images, info view, live updates, restore per tab.

**Independent Test**: spec.md US1; quickstart.md §1.

- [x] T020 [P] [US1] Tests first in `src-tauri/src/files/local/ops_tests.rs`: `list` returns name, kind, size, mtime, mime (`extensions/mime.rs::for_path`), `hidden` (dot files; Windows attribute under `cfg(windows)`), `symlink`, `no_access` (chmod 000 dir under `cfg(unix)`), `holzi_owned`; `stat` of a missing path → `notFound`
- [x] T021 [US1] Implement `src-tauri/src/files/local/ops.rs` (`list`, `stat`) and `files_list`, `files_stat` in `src-tauri/src/files/browser_commands.rs` (blocking thread)
- [x] T022 [P] [US1] Drives and places in `src-tauri/src/files/local/places.rs`: Linux/Android roots from `/proc/self/mountinfo` skipping the virtual types of research R6 (and `/` always), macOS `/Volumes/*`, Windows drive letters (`GetLogicalDrives`); Android `/storage/emulated/0` plus `/storage/<XXXX-XXXX>`; free/total space where known (`statvfs`/`GetDiskFreeSpaceExW`, a small own function, reused by T046); tests for the mountinfo parser in `places_tests.rs` (octal escapes, fstype after `-`); command `files_sources` (storages come in US5) **Note**: Windows has no free space yet (`space` returns `None`); T046 adds it
- [x] T023 [P] [US1] Pure browser state in `src/lib/files/state.ts`: navigation history (back/forward/up), sort by name/size/modified/type asc/desc (folders first), hidden filter, selection (single, ctrl, shift range); tests in `scripts/check-files-state.ts` **Note**: back and forward come from the tab router; selection follows with US3
- [x] T024 [US1] Components in `src/components/files/`: `Toolbar.vue` (back, forward, up, `ShadcnBreadcrumb`, view toggle, sort menu, hidden toggle, refresh), `Sidebar.vue` (known places, drives), `List.vue` (`useVirtualList` from `@vueuse/core`, columns name/size/modified/type), `Grid.vue` (virtual rows of tiles); lock icon for `no_access`, link marker for `symlink`, read-only badge for `holzi_owned`; empty and loading states **Note**: `Toolbar.vue`, `Sidebar.vue`, `FolderView.vue` (list and grid in one virtual list), `EntryIcon.vue`
- [x] T025 [US1] Device preferences `files.view`, `files.sort`, `files.hidden` through `usePreferences` with device scope (`src/composables/useFilesPrefs.ts`, scope from `useDevice().currentDeviceInfoAsync()`); add their validation to `src-tauri/src/storage/preferences_commands.rs` `validate_value`
- [x] T026 [US1] Live updates: `files_watch(path, channel)`/`files_unwatch` in `browser_commands.rs` using `files::local::watch` (desktop and Android); the frontend watches only the open folder, reloads on events (debounced) and on tab activation/window focus (`src/composables/useFilesFolder.ts`)
- [x] T027 [P] [US1] Thumbnails, tests first in `src-tauri/src/files/thumbnails_tests.rs`: cache key changes with size/mtime; a JPEG with EXIF orientation 6 comes out rotated; a corrupt file writes `.broken` and is not decoded again; a 30 000 × 30 000 header is refused by `Limits`; longest edge 320 px
- [x] T028 [US1] Implement `src-tauri/src/files/thumbnails.rs` (`image` + `fast_image_resize`, cache `<AppCache>/files-thumbnails/`, JPEG quality 80) and `files_thumbnail` returning `tauri::ipc::Response` bytes
- [x] T029 [US1] Share the thumbnail queue: move `src/lib/passwords/thumbnails.ts` to `src/lib/images/thumbnailQueue.ts` (unchanged behavior, passwords import the new path, `check:passwords` green); `src/composables/useFilesThumbnails.ts` requests only visible entries and revokes object URLs when the tab closes **Note**: now `src/lib/images/thumbnailCache.ts`
- [x] T030 [P] [US1] Viewer dispatch in `src/lib/files/viewerKind.ts` (MIME/extension → `text | pdf | image | video | audio | info`) with tests in `scripts/check-files-viewer.ts`
- [x] T031 [P] [US1] Text: tests in `src-tauri/src/files/local/text_tests.rs` (≤ 5 MB returned whole, larger cut at 5 MB with `truncated`, NUL bytes in the first 8 KiB → `binary`, invalid UTF-8 replaced), then `src-tauri/src/files/local/text.rs` and `files_read_text`
- [x] T032 [US1] `files_open(source, path, tabId)` in `browser_commands.rs`: kind from T030's table mirrored in Rust (`files/kind.rs`), registers a `LocalFileSource` with the media server for `image|video|audio|pdf`, returns `{ kind, url?, entry }`; `files_release`, `files_release_tab`; the tab-close path in `src/stores/windowManager.ts` calls `files_release_tab` for `system.files` tabs **Note**: `FilesApp.vue` releases its tab on unmount (`files_release_tab`) instead of a branch in the window-manager store; the viewer releases each URL when it closes
- [ ] T033 [US1] Viewer components in `src/components/files/viewer/`: `ViewerFrame.vue` (overlay inside the tab, close, prev/next within the folder), `TextViewer.vue`, `ImageViewer.vue` (photoswipe like `src/components/passwords/AttachmentLightbox.vue`, zoom), `InfoView.vue` (name, type, size, mtime, "Mit System-App öffnen" → `files_open_system` via the opener plugin) **Partly done**: `Viewer.vue` shows text, images (bytes over IPC up to 50 MB, `files_read_image`, click to zoom) and the info view with "open with system app" (desktop); video, audio and PDF show the info view until T014 ff.; images now load through the media server (`files_open`, no size limit, `files_read_image` is gone); photoswipe is still open
- [ ] T034 [US1] Session restore: the tab location carries `p` and `open`; on restore `FilesApp` re-opens the file through `files_open` (share URLs are never stored); check in `scripts/check-files-registry.ts` **Partly done**: `p` and `open` live in the tab location (`lib/files/registry.ts`), so a restored tab opens its folder and file again; the E2E proof is T035
- [ ] T035 [US1] E2E scenario `scripts/e2e/scenarios/files-basic.test.ts` part 1: temp folder with sub folders, `notiz.txt`, `foto.png`; list and grid, thumbnail present, open text and image, `touch` a file → appears, two tabs restored after restart; a generated folder with 1 000 files is listed in under 1 s and one with 50 000 files scrolls to the end with no frame longer than 100 ms (measured with `requestAnimationFrame` deltas through the page, SC-001, FR-008)
- [ ] T036 [US1] Run `cargo fmt --check`, `pnpm lint:rust`, `cargo test files:: extensions::fs`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm check:files`, `pnpm check:passwords`; revert `InstanceInfo.ts`; commit, open PR B

**Checkpoint**: US1 complete and shippable (MVP).

---

## Phase 4: User Story 2 - Video und Audio abspielen (Priority: P1) (PR C)

**Goal**: Video, audio and PDF stream through the media server with seeking; flat memory.

**Independent Test**: spec.md US2; quickstart.md §2.

- [x] T037 [P] [US2] `src/components/files/viewer/MediaViewer.vue`: `<video>`/`<audio>` with native controls and the share URL; the element's `error` event switches to `InfoView` **Note**: `src/components/files/MediaViewer.vue` (flat beside `Viewer.vue`, as the other file components); no autoplay; an `error` shows the info view with a notice
- [x] T038 [P] [US2] PDF: add `pdfjs-dist` (`^6.4`) to `package.json`; `src/components/files/viewer/PdfViewer.vue` after haex-vault `src/components/haex/system/files/PdfViewer.vue` (page navigation, zoom), loaded lazily, worker via `pdfjs-dist/build/pdf.worker.min.mjs?url`, `rangeChunkSize` 1 MiB; copy `cmaps/` and `standard_fonts/` into `public/pdfjs/` at build time (Nuxt/Vite config) and pass `cMapUrl`, `standardFontDataUrl`; no `'wasm-unsafe-eval'` **Note**: `src/components/files/PdfViewer.vue` shows one page at a time (haex-vault draws every page) with `disableAutoFetch` and `disableStream` (without it pdf.js keeps reading the whole file), so a large PDF loads only the parts a page needs; the canvas is capped at 2²⁴ pixels; JBIG2 and JPEG 2000 images decode with pdf.js's JavaScript fallbacks from `wasm/` (`wasmUrl`), published like the CMaps; loading and drawing live in `src/lib/files/pdf.ts`, shared with the probe (T041); `isEvalSupported` no longer exists in pdf.js 6, `useWasm: false` instead; CMaps and fonts via `nitro.publicAssets`, fetched by the page (`useWorkerFetch: false`), so the release CSP gained `'self'` in `connect-src`
- [x] T039 [US2] Range logging: `tracing::debug!` per request in `media_server.rs` (method, token prefix, range), used by the E2E check **Note**: `log::debug!` (holzi logs through `log`, not `tracing`); the E2E does not read it, see T040
- [x] T040 [US2] Fixtures: commit small media files under `src-tauri/tests/fixtures/files/` (an MP4 with H.264/AAC under 1 MB that is at least 10 s long, an MP3 under 200 KB, a two-page PDF, a PNG; licence-free, generated once and checked in, no ffmpeg in CI). E2E `scripts/e2e/scenarios/files-basic.test.ts` part 2 with them: the MP3 plays; the MP4 plays, seek to the middle, the log shows a `Range` request with a non-zero start; the PDF opens and page 2 renders; close the tab, the URL answers 404 **Note**: own scenario `files-media.test.ts` instead of part 2 of `files-basic`; the MP4 is 88 KB and loads whole, so a seek proves nothing about ranges: the scenario fetches a range from the page instead (206, `Content-Range` readable) and checks the seek separately; the PNG comes from `solidPng` as in `files-basic`; the Linux E2E job installs the GStreamer plugins for H.264/AAC/MP3
- [x] T041 [US2] Extend the probe of T007 with the PDF viewer component and the video/audio viewer components (not only raw elements) and keep `platform-probe` and `platform-probe-android` green on Windows, macOS and Android (SC-008); Linux is covered by T040. SC-002 and SC-003 (4 GB video, memory growth) stay a manual check on Linux per quickstart §2, recorded in the PR **Note**: the probe serves the fixtures built in (`include_bytes!`) through the real media server and checks `video-play`, `video-seek`, `audio-play`, `media-range` and `pdf-render` (through `lib/files/pdf.ts`, as the viewer); the media steps run after a green loopback step
- [ ] T042 [US2] Checks as T036; commit, open PR C

---

## Phase 5: User Story 3 - Dateien verwalten (Priority: P2) (PR D)

**Goal**: Create, rename, copy, move, delete, conflicts, transfers without half files, drop from the OS.

**Independent Test**: spec.md US3; quickstart.md §3.

- [x] T043 [US3] Dependency: `trash = "5.2"` under `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]` in `src-tauri/Cargo.toml` (it compiles neither for Android nor, presumably, for iOS); the delete path uses the same `cfg`
- [x] T044 [P] [US3] Tests first in `src-tauri/src/files/local/edit_tests.rs`: create folder (name exists → `exists`), rename (exists → `exists`, invalid characters), changing anything in an own place → `holziOwned` **Note**: a bad name is its own code `invalidName` (empty, `.`/`..`, over 255 bytes, characters Windows refuses, a trailing dot or space; refused on every platform, as a folder may later go to Windows or a storage); renaming and moving use `resolve_entry` (the folder resolved, the name kept), so a link is renamed itself and not its target
- [x] T045 [US3] Implement `src-tauri/src/files/local/edit.rs` and `files_create_folder`, `files_rename`
- [x] T046 [P] [US3] Transfer tests first in `src-tauri/src/files/transfer/local_tests.rs`: copy writes `.<name>.holzi-part-<uuid>` and renames at the end; cancel mid-file leaves no part file; conflict `replace`/`keepBoth` (name "x (2).txt")/`skip` and "for all"; copy a folder into itself or a sub folder → `intoItself` before start; move on the same filesystem is a `rename`; move across filesystems is copy + delete; not enough free space → `noSpace` before start; delete on desktop goes to the trash (`cfg(not(target_os = "android"))`), on Android permanent **Note**: a folder onto a folder of the same name merges (as Explorer and Nautilus do), only file names ask; "replace" renames the finished part file over the old one, so a cancel keeps the old file; copying into the folder the entries are in keeps both without asking, moving there does nothing; links are copied and deleted as links; the trash itself is not unit tested (it would write into the real trash), the E2E deletes through it on Linux
- [x] T047 [US3] `src-tauri/src/files/transfer/mod.rs` (`TransferManager`: ids, state machine of data-model.md, events over `tauri::ipc::Channel<TransferEvent>`, `gate.spawn`, per-transfer `CancellationToken` child of `gate.token()`, conflict wait) and `src-tauri/src/files/transfer/local.rs`; commands `files_transfer_start`, `files_transfer_answer`, `files_transfer_cancel`, `files_transfer_retry` **Note**: the progress type is `TransferProgress` (a `Progress` binding exists for the password import); progress goes out at most every 100 ms; retry runs the whole job again with the same channel
- [x] T048 [US3] Vault close while a transfer runs: test in `transfer/local_tests.rs` that cancelling the gate token removes the part file
- [x] T049 [P] [US3] Pure clipboard and selection actions in `src/lib/files/clipboard.ts` (cut/copy/paste within holzi, paste target rules) with tests in `scripts/check-files-clipboard.ts` **Note**: selection like Google Files/Gmail, so touch and desktop behave alike: a plain click opens while nothing is selected and toggles while something is; Ctrl/Cmd toggles, Shift selects a range; the context menu acts on the selection when it holds the entry
- [x] T050 [US3] UI: context menu (`ShadcnContextMenu`, model in `src/lib/files/menus.ts`), new folder and rename dialogs, delete confirmation (always on Android and storages, with count), conflict dialog (`ShadcnAlertDialog`, "für alle übernehmen"), `TransferBar.vue` (progress, cancel, retry), drag within holzi (HTML5, pattern of `src/lib/passwords/dnd.ts`); mutations disabled for `holzi_owned` **Note**: one context menu around the whole list (the entry under the pointer comes from `data-path`), plus an action bar while entries are selected or the clipboard is filled (touch without a long press); the conflict dialog opens in the tab that started the transfer, other tabs show a button; i18n group `files.actions` (`files.menu` is the toolbar label); no keyboard shortcuts yet
- [x] T051 [US3] Drop from the OS: `onDragDropEvent` in `src/components/files/DropTarget.vue` (pattern `src/components/passwords/Attachments.vue:108-140`, position check per `devicePixelRatio`) → `files_import_dropped` (a copy transfer) **Note**: a drop from the system is not reachable by WebDriver; still to check by hand (quickstart §3)
- [x] T052 [US3] E2E `files-basic.test.ts` part 3: create, rename (clash refused), copy with conflict "keep both", cancel a large copy (no part file), copy into itself refused, delete (desktop: gone from the folder) **Note**: own scenario `files-manage.test.ts`; "cancel a large copy" is cancelled at its conflict question (deterministic; a large file is too fast or too slow to push through `adb` for a mid-file cancel, which `transfer/local_tests.rs` covers); a move by dragging inside holzi is part of it (`dragTo`)
- [ ] T053 [US3] Checks as T036; commit, open PR D

---

## Phase 6: User Story 4 - Suchen und filtern (Priority: P2) (PR E)

**Goal**: Live fuzzy search from the current folder with type/size/date filters.

**Independent Test**: spec.md US4; quickstart.md §4.

- [x] T054 [US4] Dependency: `frizbee = "0.13"` in `src-tauri/Cargo.toml`
- [x] T055 [P] [US4] Tests first in `src-tauri/src/files/search_tests.rs`: `noitz` finds `notiz.txt` (one typo), `max_typos` 1 up to 5 characters else 2; a new search cancels the old one (token); a symlink loop is not followed; directories without access are skipped silently; the walk does not leave the starting device (test the decision function with fake `dev` values); agent limits produce `truncated: true` at 500 hits; filters by type/size/date **Note**: no own `dev` decision function: `walkdir`'s `same_file_system(true)` decides (Unix `st_dev`, Windows the volume serial), a copy would only test itself; a further test checks that the first hit goes out while the walk still runs (SC-004)
- [x] T056 [US4] Implement `src-tauri/src/files/search.rs` (`walkdir` with `same_file_system(true)`, `follow_links(false)`, hidden entries per preference, own places omitted for agent callers, top-K by score, hits batched over `Channel<SearchEvent>` every 100 ms) and `files_search_start`, `files_search_cancel` **Note**: a folder's files before its sub folders (sorted walk), so hits close to the start come first; the first hit goes out at once, then at most every 100 ms; the window gets at most 1 000 hits (`truncated`), agents 500 or 30 s; own places are marked for the user and left out with `hide_own` (agents, US6); the event type is `FilesSearchEvent` (hits, done)
- [x] T057 [P] [US4] Pure filters for the open folder in `src/lib/files/filters.ts` (same categories as Rust: image, video, audio, document, text) with tests in `scripts/check-files-filters.ts`
- [x] T058 [US4] UI: search field in `Toolbar.vue` (debounced 200 ms, cancels the previous search), filter popover, results list with relative path and "im Ordner zeigen"; location query `q`, `t`, `s`, `d` **Note**: the search field and the filter sit in a row of their own below the toolbar (the path bar fills the toolbar, above all on phones); the filter is a dropdown (types as checkboxes, size and time as presets: under 1 MB, 1 to 100 MB, over 100 MB; 24 hours, 7, 30, 365 days) and stays when the folder changes, the search ends then
- [x] T059 [US4] E2E `files-basic.test.ts` part 4: typo search, type filter, symlink loop terminates; a generated tree of 10 000 files: first hit in the current folder within 1 s, search done within 10 s (SC-004) **Note**: own scenario `files-search.test.ts`; times measured in the page from the input event (first hit, end); the link loop is a test of `search_tests.rs` (making a link needs a platform's own tools in the scenario); `DeviceFiles.tree` makes the 10 000 files in one go (a shell loop on the phone)
- [ ] T060 [US4] Checks as T036; commit, open PR E

---

## Phase 7: User Story 5 - Einen S3-Speicher durchsuchen (Priority: P2) (PR F)

**Goal**: Storages from 038 as sources: browse, stream, upload, download.

**Independent Test**: spec.md US5; quickstart.md §5.

- [x] T061 [P] [US5] `wiremock` tests first in `src-tauri/src/remote_storage/s3_tests.rs` (or a new `s3_ext_tests.rs` if the file passes 500 lines): `head` returns size; `get_range` sends `Range: bytes=N-M`; `get_to_writer` streams without buffering the body; `list_dir` sends `delimiter=/` and parses `CommonPrefixes` across pages; multipart create/upload part/complete and `abort`; `copy` sends `x-amz-copy-source`; DNS pinning and "no redirects" unchanged **Note** (PR F1, F2): `head`, `get_range`, `list_dir`, `copy` in `remote_storage/s3_ext_tests.rs`; multipart comes with the transfers (F2). `copy` is a PUT with a signed `x-amz-copy-source`, `get_range` signs its `range` ; F2: multipart (create, part, complete with the ETags, abort) in `s3_ext_tests.rs`
- [x] T062 [US5] Extend `RemoteStore` in `src-tauri/src/remote_storage/mod.rs` and implement in `src-tauri/src/remote_storage/s3.rs` plus new `s3_multipart.rs`/`s3_list.rs` (keep each file < 500 lines); extend `FakeStore` in `remote_storage/test_support.rs` with the same methods; confirm rusty-s3 0.10 supports each action (research R5) and record deviations **Note** (PR F1, F2): rusty-s3 0.10.2 has `HeadObject`, `GetObject`, `ListObjectsV2` with delimiter and the multipart actions, but **no `CopyObject`** (research R5 assumed it): its `PutObject` signs extra headers, so the copy is built on that; `s3.rs` stays under 500 lines (474); multipart in F2 ; F2: multipart in `s3_multipart.rs`, `list_dir` and `copy` moved to `s3_ext.rs` (`s3.rs` 452 lines)
- [x] T063 [P] [US5] Tests first in `src-tauri/src/files/storage_source_tests.rs` against `FakeStore`: prefixes appear as folders, create folder writes `name/`, rename a file is copy + delete, rename a folder copies every object under the prefix, stat via `head`, delete of one object and of a folder (every object under the prefix, then the `name/` marker), a failed delete in the middle reports which keys are left; credentials never appear in an error (FR-038)
- [x] T064 [US5] `src-tauri/src/files/storage_source.rs` (access via `StorageService::access_of`, `Caller::Internal { feature: "storage" }` stays inside `remote_storage`; list, stat, create folder, rename, delete as tested in T063), `S3StreamingSource` in `streaming.rs` (`get_range` per request), `files_sources` lists storages, `files_open`/`files_thumbnail` for storages (thumbnail: download to a temp file up to 50 MB, else `tooLarge`), delete transfers on storages without trash and only after the confirmation of T050 **Note**: the window sees storage paths like device paths (`/Fotos/a.jpg` ↔ key `Fotos/a.jpg`, `/Fotos` ↔ prefix `Fotos/`), only `storage_source.rs` knows keys; `read_text` and the thumbnail cache were split so storages reuse them (`text_from`, `thumbnails::cached` before any download); new error codes `credentials` and `unreachable`; delete runs as a transfer (`TransferManager` got a second kind of work); copy and move to or from a storage, search there and "open with system app" answer `unsupported` until F2
- [x] T065 [US5] Transfers across sources in `src-tauri/src/files/transfer/s3.rs`: upload < 8 MiB as one `put`, larger as multipart with 8 MiB parts, `abort` on cancel or failure; download streamed into the part file; same connection → server-side `copy`; retries 1 s/2 s/4 s on network errors then `failed`; tests in `transfer/s3_tests.rs` with `FakeStore` (`fail_once`) **Note**: in `transfer/remote.rs` (+ `remote_plan.rs`, files < 500 lines), on the blocking pool like local transfers with the same hooks; each provider call through `Handle::block_on`; a bucket of another storage streams through holzi (one part in memory); a folder onto a folder merges, files ask; tests in `transfer/remote_tests.rs`; E2E: up in `files-storage`, down in `files-storage-down`
- [x] T066 [US5] Search on storages in `search.rs`: prefix walk via `list_dir` with progress events; tests in `search_tests.rs` **Note**: `search_storage` walks folder by folder through `StorageFiles::list` (breadth first), same matcher, filters and limits as the device; `FilesSearchEvent::Progress { dirs }`, shown in the hits header; tests in `search_tests.rs` (module `storage`)
- [x] T067 [US5] UI: storages in `Sidebar.vue`, route `/storage/:id`, refresh button and reload on tab activation, error view for `AccessDenied`/credentials with a link to the storage settings (`wm.app.open` of settings at the storage location of 038) **Note**: no storage is watched; it loads again when its tab comes to the front (and on window focus); the path bar shows the storage's name for `/`; dropping from the system into a storage waits for F2
- [x] T068 [US5] Manual quickstart §5 against RustFS (or an E2E scene if the rig runs RustFS, as `scripts/e2e/scenarios/extension-storage.test.ts` does); record results in the PR **Note**: an E2E scenario `files-storage.test.ts` against the rig's RustFS instead of the manual check: list, new folder (marker), rename a folder (every object), text and a film (play, seek) from the storage, delete after the question; the bucket is checked directly
- [ ] T069 [US5] Checks as T036 plus `cargo test remote_storage::`; commit, open PR F

---

## Phase 8: User Story 6 - Ein Agent sucht und liest Dateien (Priority: P2) (PR G)

**Goal**: `files.*` actions in Rust (ADR 0011), agent permissions, readable formats, images to models, `files.show`.

**Independent Test**: spec.md US6; quickstart.md §6.

- [x] T070 [US6] Dependencies: `pdf-extract = "0.12"`, `calamine = "0.36"`, `zip = "8"`, `quick-xml = "0.41"` in `src-tauri/Cargo.toml`; `cargo check` host and Android **Note** (PR G2): plus `lopdf = "0.42"` (the one pdf-extract uses) to spell out text operators, see T079; `zip` without default features (`deflate` only)
- [x] T071 [P] [US6] ADR 0011 in the catalog: `runner?: 'native'` on `ActionDefinition` in `src/lib/actions/types.ts`, passed through `toAgentActionDef` (`src/lib/actions/agentTools.ts`) to `set_agent_actions`; in `src-tauri/src/chat/action_commands.rs` native definitions become `NativeActionTool` (new `src-tauri/src/chat/tools/native_action.rs`, source `action`, risk class from `effect` like `ActionTool`), executors looked up by id in a registry filled at startup; a native id without executor or an executor without definition is a registration error; tests in `action_commands_tests.rs` **Note** (PR G1): the ADR was written with the plan; G1 adds `runner` to `ActionDefinition`/`AgentActionDef` and `NativeActionTool` with `NativeExecutors` in `ChatState`, filled in `setup()`; errors take the shape of window actions (`{error: {code, message, field?}}`); tests in `native_action_tests.rs` and `action_commands_tests.rs`
- [x] T072 [P] [US6] `scripts/check-agent-actions.ts`: stub native `read` actions in the harness instead of `catalogRunner`; `files.*` are not in `CORE_AGENT_TOOLS`; update the eval tool snapshot `src-tauri/src/chat/eval/tools.json` **Note** (PR G1): native actions are left out of the runner check; a new check compares the native ids of the catalog with `FILE_ACTION_IDS` in `files/agent/exec.rs`
- [x] T073 [US6] Catalog `src/lib/actions/filesActions.ts` with the actions of contracts/agent-actions.md (descriptions written for the model, JSON schemas, `runner: 'native'` except `files.show`); scopes `files.read`, `files.write` in `src/lib/actions/scopes.ts`; titles `actions.files.*` and `actions.scopes.files*` in `src/i18n/locales/{de,en}.json` **Note** (PR G1): `files.copy` and `files.move` take `toSource` for a target on another source; `source` also takes `storage:<name>` (contract updated)
- [x] T074 [P] [US6] Migration `0029_agent_file_permissions` in `src-tauri/src/identity/migrations_agent_files.rs` (data-model.md), registered in `migrations.rs`, `HOLZI_TRIGGER_VERSION` raised; test in `migrations_agent_files_tests.rs` **Note** (PR G1): no UNIQUE constraint, like every synced table (it would halt the sync); the derived id keeps one row per agent, kind and target (data-model.md updated)
- [x] T075 [US6] `src-tauri/src/files/permissions.rs` (read/write rows, deterministic ids, evaluation through `files::access`); `remote_storage::store::remove_storage` also deletes `kind='storage'` rows of that storage; tests in `permissions_tests.rs` **Note** (PR G1): rows without a known kind or status (a newer version) are skipped
- [x] T076 [US6] Permission prompt bridge in `src-tauri/src/files/agent/prompt.rs` (pattern `chat/action_bridge.rs`): event `files-agent-permission-request`, answer via `files_agent_permission_answer`, 60 s timeout or no window → deny for this call (not stored); answers `read`/`readWrite`/`deny` are stored; frontend dialog `src/components/files/AgentPermissionDialog.vue` mounted at app level; tests for timeout and stored answers **Note** (PR G1): `files/agent/prompt.rs`, the dialog is `components/files/AgentPermissionDialog.vue` on the workspace page; questions wait in line, and a second call for the same storage reads the grants again before it asks
- [x] T077 [US6] Settings view for agent file permissions (FR-031b): category or row in the settings app (`src/lib/settings/registry.ts`, `src/components/settings/`) listing per agent "Dateien des Geräts" and each storage with its status, revoke and change; commands `files_agent_permissions`, `files_agent_permission_set` **Note** (PR G2): location "Agents → Dateien" (`agents.files`, `components/settings/AgentFilesSetting.vue`) for the built-in agent; the change runs through the guardrail action `settings.agents.files.set`; a decision can be changed, not taken back to "ask on first access"
- [x] T078 [P] [US6] Text extraction, tests first in `src-tauri/src/files/extract/pdf_tests.rs`, `office_tests.rs` with small fixtures committed under `src-tauri/tests/fixtures/files/` (a text PDF, an image-only PDF, a password-less encrypted PDF, docx, odt, xlsx with two sheets, ods): text found, sheet names present, scanned PDF → hint, panicking input caught, 50 MB input limit, 200 000 character output limit with `truncated` **Note** (PR G2): fixtures made with Ghostscript and LibreOffice (`tests/fixtures/files/README.md`); the panic is tested through `guarded` with a closure, since no small input makes pdf-extract panic reliably
- [x] T079 [US6] Implement `src-tauri/src/files/extract/{pdf,office}.rs` (`pdf-extract` in `catch_unwind` on a blocking thread, scanned heuristic of research R8; `calamine::open_workbook_auto`; docx `word/document.xml` and odt `content.xml` with `zip` + `quick-xml`) **Note** (PR G2): pdf-extract 0.12 knows only `Tj`, `TJ` and `T*`; Ghostscript writes every line after the first with `'`, so those (and `"`) are spelled out with lopdf before the text is read, or such lines would be missing; scanned = fewer than 16 visible characters per page; xls and xlsb come with calamine as well
- [x] T080 [P] [US6] Images in tool results, tests first: `ToolResult.images` in `src-tauri/src/chat/tools/mod.rs` (default empty); in `src-tauri/src/adapters/request.rs` a tool result with images becomes a content array of text and `image` blocks when `capabilities.accepted_attachment_kinds` contains images, else the text plus a hint; persisted message keeps text plus the placeholder only (`chat/turn/tool_round.rs`); local adapter drops images with the hint; tests in `request_tests.rs` and the tool-round tests **Note** (PR G2): the decision lives in `chat/turn/images.rs`; a model that is not known to accept images gets the note, the history keeps "[Image <name>: shown to the model.]" or the note; native executors answer `NativeReply { result, images }`; tests in `request_tests.rs` and `tests/chat_tool_loop_images.rs`
- [x] T081 [US6] Image for agents in `src-tauri/src/files/extract/image.rs`: decode with orientation, longest edge 1568 px, re-encode JPEG until ≤ 5 MB; tests in `image_tests.rs` **Note** (PR G2): decode and orientation shared with the thumbnails (`thumbnails::upright_within`); JPEG qualities 85 → 35 until the image fits
- [x] T082 [US6] Executors in `src-tauri/src/files/agent/exec.rs` for `files.sources`, `list`, `stat`, `search`, `read`, `folder.create`, `copy`, `rename`, `move`, `delete` with `Caller::BuiltinAgent` (contracts/agent-actions.md: own places blocked and omitted, storages without grant omitted, `onConflict`, limits, `files_blocked`/`files_not_granted` errors); tests in `exec_tests.rs` including the SC-005 corpus end to end; also the command `files_agent_check(source, path)` for the `files.show` handler (same checks as the executors, no content); a test that an error from a storage reaches the agent without credentials (FR-038) **Open point (found in T013)**: storages without a grant never appear for an agent (FR-031a), so an agent cannot learn their ids; to ask for one the user names it ("liste den Speicher Fotos auf"), so `source` must also accept `storage:<name>` (exact name, unique), resolved in Rust; update contracts/agent-actions.md **Note** (PR G1): `files/agent/{env,exec,exec_write}.rs`; an unknown storage name, a denied storage and a question without a window answer alike (`files_not_granted`, SC-005); a folder holding one of holzi's own places is refused as a whole, also for copying; `files.read` gives text of text files and a note for images and documents until PR G2
- [x] T083 [US6] `files.show`: frontend handler in `src/stores/filesActionHandlers.ts` calls `files_agent_check` then `wm.openApp('system.files', at)` with `p` and `open`; registered with the other action handlers **Note** (PR G1): `files_agent_check` returns `FilesAgentShow` with the folder to open and the file for the viewer
- [x] T084 [US6] E2E or manual: if the E2E rig has a scripted model for chat scenarios (see `scripts/e2e/lib`), add `scripts/e2e/scenarios/files-agent.test.ts` (search, read PDF, read vault file → blocked, storage prompt); otherwise run quickstart §6 by hand with an Anthropic and a local model and record it in the PR **Note** (PR G1): the stand-in provider plays a scripted model (`kind: 'script'`, tool calls then a text); `files-agent.test.ts` finds the file actions with `find_actions`, searches a device folder and reads a text file, all in Rust; reading PDFs and images and the storage question follow in G2; G2: the scenario also reads the text of a PDF
- [x] T085 [US6] Checks as T036 plus `pnpm check:agent-actions`, `cargo test chat::`; commit, open PR G **Note**: PR G1 #359, PR G2

---

## Phase 9: User Story 7 - Alle Dateien auf Android (Priority: P3) (PR H)

**Goal**: "Zugriff auf alle Dateien" with explanation and recheck.

**Independent Test**: spec.md US7; quickstart.md §7.

- [ ] T086 [P] [US7] Manifest `src-tauri/gen/android/app/src/main/AndroidManifest.xml`: `MANAGE_EXTERNAL_STORAGE`, `READ_EXTERNAL_STORAGE` and `WRITE_EXTERNAL_STORAGE` with `android:maxSdkVersion="29"`
- [ ] T087 [US7] Plugin `src-tauri/plugins/holzi-android`: Kotlin commands `allFilesAccessStatus` (`Environment.isExternalStorageManager()` on API 30+, runtime permission on 26–29) and `requestAllFilesAccess` (`ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION` with `package:` URI, fallback `ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION` on `ActivityNotFoundException`; runtime request on 26–29); `build.rs` `COMMANDS`, plugin permissions, Rust wrappers in `src/android.rs`; app capability allows them
- [ ] T088 [US7] Commands `files_all_access`, `files_request_all_access` (desktop: always granted, request is a no-op)
- [ ] T089 [US7] UI `src/components/files/AllFilesAccess.vue`: explanation and button instead of the list when not granted; recheck on window focus/visibility change
- [ ] T090 [US7] `pnpm check:android-versions`; extend `platform-probe-android` (T007): run once without the permission (probe reports `granted: false`), then `adb shell appops set <package> MANAGE_EXTERNAL_STORAGE allow`, restart, probe lists `/storage/emulated/0`, then `adb shell touch /storage/emulated/0/Download/probe.txt` and the probe sees the watch event; commit, open PR H

---

## Phase 10: Polish & Cross-Cutting Concerns

- [ ] T091 [P] Every new Rust and Vue file under 500 lines (`wc -l`), split where needed
- [ ] T092 [P] Update `plans/README.md` row 044 status after each merged PR
- [ ] T093 Full quickstart run on Linux by hand (SC-002, SC-003 with a 4 GB video); Windows, macOS and Android are covered by the `platform-probe` jobs; note results in the last PR
- [ ] T094 [P] Spec bookkeeping: mark the checklist and spec status after PR H; open questions found during implementation go back to `/speckit-clarify`, not into code

---

## Dependencies & Execution Order

- **Phase 1** → **Phase 2** → stories. T007 (spike) gates T014–T016.
- **US1** (P1) needs Phase 2 only. **US2** needs US1 (viewer frame, `files_open`).
- **US3** needs US1. **US4** needs US1. **US5** needs US3 (transfers) and US4 (search on storages).
- **US6** needs US4 (search); its storage part needs US5. **US7** needs US1.
- Within a story: tests first, then Rust core, then commands, then UI, then E2E, then checks.

### Parallel Opportunities

- Phase 1: T001–T003 together.
- Phase 2: T009, T013, T018, T019 alongside the moves T010–T011; T014 after T007.
- US1: T020, T022, T023, T027, T030, T031 together.
- US3: T044, T046, T049 together. US4: T055, T057 together. US5: T061, T063 together.
- US6: T071, T072, T074, T078, T080 together; T082 after T075, T079, T081.

### Parallel Example: User Story 1

```text
T020 ops tests (Rust)      T022 drives/places (Rust)   T023 browser state (TS)
T027 thumbnail tests (Rust) T030 viewer dispatch (TS)  T031 text tests (Rust)
```

## Implementation Strategy

1. **MVP**: Phase 1, Phase 2, US1 → PR B: browse the device, thumbnails, text and images. Demo.
2. **P1 complete**: US2 → PR C: video, audio, PDF.
3. **P2**: US3 → US4 → US5 → US6, each its own PR, each demoable.
4. **P3**: US7 → PR H.
