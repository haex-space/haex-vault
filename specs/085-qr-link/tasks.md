# Tasks: Link a vault by scanning a QR code

Spec [spec.md](./spec.md), plan [plan.md](./plan.md), research [research.md](./research.md), contracts in
[contracts/](./contracts/). Each PR below is made from its own worktree off `main`; no PR stacks on another. Tests live in
`*_tests.rs` or separate test files; files stay under 500 lines. Cargo only inside `nix develop` with
`scripts/with-nix-host-bridge.sh`, build only the relevant targets with `-j 4`. Task order inside a PR is execution order.

## Status (2026-11-08)

Built and in review: PR A #893 (T001 to T005), B #889 merged, B2 #898 (T013), C #895, D #897 (draft, stacked on A and C),
this docs PR (E). Open: T006 and T007 (the spike on real cameras; branch `085-a-spike` has the page), T039's manual camera
checks, and T041 (the full quickstart). Differences from the text of the tasks below:

- T003: the gate is not closed when the window is destroyed; the process ends with the window (spec 013), so a gate cannot
  outlive it.
- T013: `LinkCode::qr_svg` was deleted and `text::qr_svg` calls the contact card's renderer (research R11, outcome).
- T014: the vault name comes from `AppState::active_name()`; `LinkCodeInfo.qrSvg` is `null` above 1,200 bytes (T013).
- T016: no "omitted servers" line; with `qrSvg` null the view says to copy the link.
- T033: the camera pick holds for the open scan only.
- T040 (the fake-camera e2e) was dropped; T034 and T035 cover it. The old T041 and T042 are now T040 and T041.
- T027 was written but not run locally (the e2e rig does not run on this machine).

## PR A — spike and camera gate (R4, R5; blocks D)

Gate for the rest: if a desktop webview cannot open the camera, decide the native fallback of R4 for that OS before PR D.

- [x] T001 Add `src-tauri/src/camera_gate.rs`: `CameraGate` (one `AtomicBool`), `begin()`, `end()`, `answer(kind: PermissionKind) -> PermissionResponse` that returns `Allow` for `Camera` only while the gate is open, `Deny` for `Camera` while closed, and `Default` for every other kind (R5)
- [x] T002 [P] Tests in `src-tauri/src/camera_gate_tests.rs`: closed gate gives `Deny` for `Camera`; open gives `Allow`; `Microphone`, `DisplayCapture`, `Geolocation` give `Default` in both states; `end()` is idempotent; the gate is closed after the main window closed
- [x] T003 Add the commands `camera_scan_begin` and `camera_scan_end` in `src-tauri/src/camera_gate.rs`, register them in `src-tauri/src/lib.rs`, add them to the `main` window's capability in `src-tauri/capabilities/`, and close the gate when the main window is destroyed
- [x] T004 Register `on_permission_request` on the builder in `src-tauri/src/lib.rs` using `CameraGate::answer`; on Linux set the WebKitGTK `enable-media-stream` setting on the main window through `with_webview` (R4, Linux specifics)
- [x] T005 [P] Add `NSCameraUsageDescription` ("HaexVault uses the camera to scan the QR code of your other device.") next to the microphone text in `src-tauri/Info.plist`
- [ ] T006 Throwaway spike page `scripts/spike/link-camera.html` + a dev-only route that opens `getUserMedia`, draws frames and decodes with `jsqr` (installed locally for the spike, not committed: the dependency enters with T030); run it on Linux (dev shell and AppImage), Windows and macOS with a QR shown on a phone; record per OS in `specs/085-qr-link/research.md` (new section "R4 spike results"): `navigator.mediaDevices` present, prompt behavior, frames per second, decode success of 10 tries
- [ ] T007 For every OS where T006 failed, decide in `research.md` whether the native fallback (R4: `nokhwa` + `rqrr`, JPEG preview over an event) is built, and amend `plan.md` and the task list of PR D accordingly; remove the spike page and route; open PR A with checks (`cargo fmt --check`, `cargo clippy --all-targets`, camera gate tests, `pnpm lint`)

## PR B — the link text, the host side (US3; foundation of US1, US2, US4, US5)

T013 waits for the approval of the proposal in research.md R11.

- [x] T010 [US3] Add `src-tauri/src/sync/link/text.rs` with `LinkText { code, vault_name: Option<String>, servers: Option<SyncServers>, dropped_servers: u32 }` (ts-rs export to `src/types/bindings/`), `build(code, vault_name, &ServerConfig) -> String` and `parse(&str) -> Result<LinkText, LinkTextError::{NotALinkCode, UnsupportedVersion}>` per [contracts/link-text.md](./contracts/link-text.md): keys `v`, `c`, `n`, `nr`, `ir`, `off`; exactly one numeric `v` is required and only version 1 is supported; `c` exactly 1, 26 Base32 characters, no dashes, parsed with `LinkCode::parse`; `n` 0..1, at most 64 characters, percent-encoded UTF-8 (longer is shortened at a character boundary on build, refused-to-fill but not fatal on parse); `nr` 0..10 (`ws://` or `wss://`, checked with `is_nostr_relay`); `ir` 0..10 (`http://` or `https://`, checked with `is_iroh_relay`); `off` 0..40; each URL at most 256 bytes, no whitespace; unknown keys ignored within `v=1`; maximum parsed length 49,152 UTF-8 bytes, longer is `NotALinkCode`; a plain code (no scheme) parses as `c` alone; a good code with bad servers returns the code and counts them in `dropped_servers`
- [x] T011 [P] [US3] Tests in `src-tauri/src/sync/link/text_tests.rs`: build/parse round trip for no servers, added servers, switched-off built-in servers, switched-off added servers; plain code with spaces, dashes and lower case; `v=2` and `v=0` are `UnsupportedVersion`; missing, duplicated, or non-numeric `v` is `NotALinkCode`; unknown key ignored; a bad `nr` dropped and counted; 11 `nr` entries keep 10 and count one; a 65-character name is shortened on build at a character boundary (also with a multi-byte character at the cut); build and parse a maximum-size valid link with the maximum server counts and URL byte lengths; empty text and a 49,153-byte text are `NotALinkCode`; the built text contains none of passphrase, key or device secret (a field-name audit over the builder's inputs, SC-006)
- [x] T012 [US3] Size of the QR (R7) in `text.rs`: `QR_MAX_TEXT_BYTES = 1_200` and `fits_qr(&str) -> bool`; `build` never shortens or drops a server; tests: the usual text (name, one added server, one switched-off server) fits; 10 + 10 added servers of 256 characters do not; the built text is the same with and without the limit and identical on a second build
- [x] T013 [US3] (after R11 is approved) `src-tauri/src/sync/link/code.rs`: `qr_svg(text: &str) -> Option<String>` replaces `qr_svg(&self)`: `None` when `fits_qr` is false or `qrcode` cannot encode it (no `expect`), else error correction level M and `min_dimensions(320, 320)`; update `code_tests.rs`: the usual text gives `Some`, a 1,201-byte text gives `None`, the SVG equals a render of the same text with the same settings; the decode check is T035 (no `rqrr`, no new Rust crate)
- [x] T014 [US3] `src-tauri/src/sync/link/status.rs`: add `text: String` to `LinkCodeInfo` and make `qr_svg` an `Option<String>` (ts-rs regenerates `src/types/bindings/LinkCodeInfo.ts`, `qrSvg: string | null`); `link_code_create` in `src-tauri/src/sync/commands.rs` passes `AppState::active_name()` (`src-tauri/src/state.rs:137`) into `create_code` in `src-tauri/src/sync/link/host_task.rs`, which also reads `servers::read`, calls `text::build` and fills text and QR; the main-device-only check and the 10-minute single use stay unchanged (FR-012, spec 024 FR-023)
- [x] T015 [US3] Add the command `link_text_parse` in `src-tauri/src/sync/commands.rs` (works with no vault open, registered next to `link_join_start`), map `LinkTextError` to error kinds `NotALinkCode` and `UnsupportedVersion` in the shared error type, and register it in `src-tauri/src/lib.rs`
- [x] T016 [P] [US3] `src/composables/useDeviceLink.ts`: add `parseTextAsync(text): Promise<LinkText>`; `src/components/settings/LinkDeviceView.vue`: add a "Copy link" button (icon turns into a check mark, no success line) that copies `code.text`, one standing hint "Show this code only to your own device" and, when `qrSvg` is `null`, instead of the QR one standing line "Too many servers for a QR code: copy the link and send it to the other device"
- [x] T017 [P] [US3] Add texts to `src/i18n/locales/*.json` (every shipped language, German first): copy-link button, hint, too-many-servers line, error kinds `notALinkCode`, `unsupportedVersion` ("Update HaexVault to read this code"); wording rules: "Nostr-Relay", "iroh-Relay", "Server", never a bare "Relay"; product name "HaexVault" (FR-014)
- [x] T018 [US3] Integration test in `src-tauri/tests/sync_link.rs`: host with one added Nostr-Relay, one added iroh-Relay and one built-in server switched off creates a code; `link_text_parse` of `LinkCodeInfo.text` returns the same servers and the vault name; the link then completes with those servers (the existing join fixture)
- [x] T019 PR B checks: `cargo fmt --check`, `cargo clippy --all-targets`, the link and sync test targets (`-j 4`), `pnpm typecheck`, `pnpm lint`, prettier on touched Markdown/JSON; open PR B

## PR C — prefill, paste and phone scan (US4, US1, US5)

T023 and T024 wait for the approval of the proposal in research.md R11.

- [x] T020 [US4] `src/components/onboarding/LinkSheet.vue`: add `applyLinkText(raw)` that calls `parseTextAsync` and fills `code` (always, display form), and the vault name when empty or still equal to the prior untouched suggestion; track whether a name was suggested and clear/replace it on the next apply only if the user did not edit it. Track which server values were imported by the prior link and which values the user edited; remove untouched imported values on replacement, preserve manual values/edits, then apply new link servers (a plain code has no imported servers); `deviceName`, `passphrase` and `passphraseConfirm` are never touched; call it on paste and on blur of the code field when the input starts with `haex-vault://` or is longer than a code; errors only as `toast.error` with a fixed `id` (`NotALinkCode`, `UnsupportedVersion` have own texts); when `droppedServers > 0` one `toast.error` that the servers were not taken over (spec edge case)
- [x] T021 [US4] Test (Node check via `scripts/lib/script-setup-sandbox.ts`, with a `vue-sonner` import double) in `src/components/onboarding/linkSheetApply.test.ts`: link text fills code, empty vault name and imported servers; a typed vault name is kept; a second link replaces an untouched suggested name but keeps a user-edited name, replaces imported servers, and preserves manual server edits, device name and passphrases; replacing a full link with a plain code clears untouched imported servers but preserves manual settings; `UnsupportedVersion` and `NotALinkCode` toast and fill nothing; a plain code fills only the code
- [x] T022 [US5] `LinkSheet.vue`: when servers came with the current text, put both `SettingsServerList` blocks in a collapsible closed by default with one summary line "N servers taken over from your other device" (N = imported Nostr-Relay + iroh-Relay + disabled entries); when none came show them as today; switching off, adding and removing keep working and are sent in `joinStartAsync` as now; test in `linkSheetApply.test.ts` that the summary counts imported entries and the arguments of `joinStartAsync` equal the imported servers plus the user's changes
- [x] T023 [US1] (after R11 is approved) Add `src/composables/useQrScan.ts`: `scanAsync(): Promise<string | null>` with the phone path moved from `ShareImport.vue` (`scan({ formats: [Format.QRCode] })` through a dynamic import; `null` on cancel) and an injectable platform/camera seam so the desktop path of PR D plugs in without changing callers; error names mapped to toasts per R6
- [x] T024 [US1] (after R11 is approved) `ShareImport.vue`: replace its inline plugin call with `useQrScan` (behavior unchanged); keep its existing e2e/test expectations green
- [x] T025 [US1] `LinkSheet.vue`: "Scan QR code" button (text label, keyboard reachable, `data-testid="link-scan"`) that calls `useQrScan().scanAsync()` and then `applyLinkText`; shown on phones; the desktop path comes with T036
- [x] T026 [P] [US1] Texts for the scan button, denied-permission toast ("Allow the camera for HaexVault in the system settings"), "not a link code" toast and the servers summary in `src/i18n/locales/*.json` (same wording rules as T017)
- [x] T027 [US4] E2E scenario `scripts/e2e/scenarios/link-qr.test.ts` (spec 016 rig, errors read with `scripts/e2e/lib/toasts.ts`): host shows a code, the test reads `LinkCodeInfo.text`, pastes it into the new instance's code field, checks code, vault name and collapsed servers, enters device name and passphrase, links; second case: pasting a plain code leaves the servers block open; third case: a `v=2` text shows the update toast and leaves the form empty
- [x] T028 PR C checks: `pnpm typecheck`, `pnpm lint`, node checks, the `link-qr` scenario locally if the rig runs (otherwise via `workflow_dispatch`), prettier; open PR C. Manual M1 and M6 (quickstart) on a phone before merge

## PR D — desktop webcam scanner (US2)

Depends on PR A's result (T007) and PR C. If T007 chose the native fallback for an OS, T031/T032 use that path for it behind the same `useQrScan` interface.

- [x] T030 [US2] `src/components/onboarding/LinkScanner.vue`: preview `<video>` in the sheet, frame loop every ~100 ms on a downscaled frame (longest side 640 px) drawn to a canvas, decode with `jsqr` (added to `package.json` here) loaded by dynamic import, stop at the first text that `parseTextAsync` accepts and emit it; when a decoded QR is not accepted, stop the preview and show the corresponding `UnsupportedVersion` or `NotALinkCode` toast once (do not toast repeatedly for every frame); a frame with no decoded QR is ignored, with a standing hint after 15 s "Hold the code in the middle of the picture" (not a toast); camera picker shown only with more than one camera
- [x] T031 [US2] Desktop path in `src/composables/useQrScan.ts`: `camera_scan_begin` before `getUserMedia({ video: { facingMode: "environment" } })`, `camera_scan_end` after stop; stop all tracks and call `end` on success, close, unmount, `visibilitychange`/window blur and any error; retry without constraints on `OverconstrainedError`; no frame is stored, uploaded or logged (FR-007)
- [x] T032 [US2] Error mapping in `useQrScan`: `NotFoundError` → "no camera found", `NotReadableError` → "camera is in use by another app", `NotAllowedError` → denied with where to allow it for the OS (Linux, Windows, macOS text variants); every case `toast.error` with a fixed `id`, paste and typing keep working
- [x] T033 [US2] Camera choice (R6): after the first grant `enumerateDevices`; the picked `deviceId` is held in the scanner for the open scan only and not stored (the landing page has no open vault and no device-local store exists); the default camera is used at the next scan
- [x] T034 [P] [US2] Test with an injected fake camera and clock in `src/composables/useQrScan.test.ts`: success stops tracks and ends the gate; each camera error name gives its toast and ends the gate; close, unmount and blur end the gate and stop tracks (assert the track `stop` calls and the `camera_scan_end` call, no timers slept on); both happen in the same tick as the trigger, which is the bound of SC-004
- [x] T035 [P] [US2] Decode tests in `src/components/onboarding/linkScannerDecode.test.ts`: render the host's `qr_svg` of a typical text (about 150 bytes) and of a worst-case 1,200-byte text to an image in the test and decode it with the scanner's decoder function; both give the exact text (SC-002 groundwork); a plain-code QR as an older main device shows it and gives the plain code (SC-005); exercise `LinkScanner`'s decoded-text handler with `v=2` and contact-share QR codes and verify the preview stops and exactly one appropriate toast appears for each
- [x] T036 [US2] Show the scan button on desktops (remove the phone-only condition of T025), open `LinkScanner` in the sheet, close it on success or cancel; the sheet's close watcher also ends the scan (FR-016: the scanner exists only inside the open link sheet)
- [x] T037 [P] [US2] Texts for the preview, camera picker, hold-steady hint and the three denial variants in `src/i18n/locales/*.json`
- [x] T038 [US2] Accessibility of the scanner: labels and live-region text for the preview and the states, focus moves into the preview on open and back to the scan button on close, the scan button and the camera picker reachable by keyboard; check by tab order with no camera present
- [ ] T039 PR D checks as T028; manual M2, M3, M4, M5, M7 of the quickstart on Windows, Linux and macOS with real webcams (record results in the PR); open PR D
  - Status 2026-11-08: automated checks are covered by PR D; no real-camera check has run, so M2–M5 and M7 remain open

## PR E — end to end and documentation

- [x] T040 [P] Update `specs/024-own-device-sync/contracts/tauri-commands.md` (`link_code_create` result, `link_text_parse`) and `specs/024-own-device-sync/research.md` R11 with a pointer to this spec; update `CONTEXT.md` (link text, camera gate)
- [ ] T041 Run the full quickstart (automated and M1–M7), write the results into `specs/085-qr-link/quickstart.md`, mark which manual checks remain; prettier on all touched Markdown and JSON; open PR E

## Dependencies and order

- PR A (spike, gate) and PR B (link text) are independent and can run in parallel in two worktrees.
- PR C needs PR B (`link_text_parse`, `LinkCodeInfo.text`).
- PR D needs PR A's decision (T007) and PR C (`useQrScan`, the scan button).
- PR E needs PR C and PR D.
- Within stories: US3 (host text) before US4 (paste) before US1 (phone scan) and US5 (collapsed servers); US2 (desktop) last.

## Parallel examples

- PR B: T011 (tests) beside T010 once the signatures exist; T016 and T017 beside T014 and T015.
- PR D: T034, T035 and T037 beside T030 to T033.

## Implementation strategy

- **MVP**: PR B + PR C (host shows the full text, paste and phone scan prefill the form). This already removes the typing of the
  servers and the code on phones, and the paste path works on every desktop.
- Then PR A's result decides how PR D is built; PR D completes the desktop webcam (US2). PR E closes the spec.
- Manual checks that stay open after the last PR are listed in `quickstart.md` as in earlier specs (M1–M7).
