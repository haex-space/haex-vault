# Tasks: Link a vault by scanning a QR code

Spec [spec.md](./spec.md), plan [plan.md](./plan.md), research [research.md](./research.md), contracts in
[contracts/](./contracts/). Each PR below is made from its own worktree off `main`; no PR stacks on another. Tests live in
`*_tests.rs` or separate test files; files stay under 500 lines. Cargo only inside `nix develop` with
`scripts/with-nix-host-bridge.sh`, build only the relevant targets with `-j 4`. Task order inside a PR is execution order.

## PR A — spike and camera gate (R4, R5; blocks D)

Gate for the rest: if a desktop webview cannot open the camera, decide the native fallback of R4 for that OS before PR D.

- [ ] T001 Add `src-tauri/src/camera_gate.rs`: `CameraGate` (one `AtomicBool`), `begin()`, `end()`, `answer(kind: PermissionKind) -> PermissionResponse` that returns `Allow` for `Camera` only while the gate is open and `Default` for every other kind and while closed (R5)
- [ ] T002 [P] Tests in `src-tauri/src/camera_gate_tests.rs`: closed gate gives `Default` for `Camera`; open gives `Allow`; `Microphone`, `DisplayCapture`, `Geolocation` give `Default` in both states; `end()` is idempotent; the gate is closed after the main window closed
- [ ] T003 Add the commands `camera_scan_begin` and `camera_scan_end` in `src-tauri/src/camera_gate.rs`, register them in `src-tauri/src/lib.rs`, add them to the `main` window's capability in `src-tauri/capabilities/`, and close the gate when the main window is destroyed
- [ ] T004 Register `on_permission_request` on the builder in `src-tauri/src/lib.rs` using `CameraGate::answer`; on Linux set the WebKitGTK `enable-media-stream` setting on the main window through `with_webview` (R4, Linux specifics)
- [ ] T005 [P] Add `NSCameraUsageDescription` ("HaexVault uses the camera to scan the QR code of your other device.") next to the microphone text in `src-tauri/Info.plist`
- [ ] T006 Throwaway spike page `scripts/spike/link-camera.html` + a dev-only route that opens `getUserMedia`, draws frames and decodes with `jsqr` (add `jsqr` to `package.json` as a dependency); run it on Linux (dev shell and AppImage), Windows and macOS with a QR shown on a phone; record per OS in `specs/085-qr-link/research.md` (new section "R4 spike results"): `navigator.mediaDevices` present, prompt behavior, frames per second, decode success of 10 tries
- [ ] T007 For every OS where T006 failed, decide in `research.md` whether the native fallback (R4: `nokhwa` + `rqrr`, JPEG preview over an event) is built, and amend `plan.md` and the task list of PR D accordingly; remove the spike page and route; open PR A with checks (`cargo fmt --check`, `cargo clippy --all-targets`, camera gate tests, `pnpm lint`)

## PR B — the link text, the host side (US3; foundation of US1, US2, US4, US5)

- [ ] T010 [US3] Add `src-tauri/src/sync/link/text.rs` with `LinkText { code, vault_name: Option<String>, servers: Option<SyncServers>, dropped_servers: u32 }` (ts-rs export to `src/types/bindings/`), `build(code, vault_name, &ServerConfig) -> BuiltText { text, omitted_servers }` and `parse(&str) -> Result<LinkText, LinkTextError::{NotALinkCode, UnsupportedVersion}>` per [contracts/link-text.md](./contracts/link-text.md): keys `v`, `c`, `n`, `nr`, `ir`, `off`; `v` exactly 1 and this build supports 1; `c` exactly 1, 26 Base32 characters, no dashes, parsed with `LinkCode::parse`; `n` 0..1, "at most 64 characters, percent-encoded UTF-8" (longer is shortened at a character boundary on build, refused-to-fill but not fatal on parse); `nr` 0..10 (`ws://` or `wss://`, checked with `is_nostr_relay`); `ir` 0..10 (`http://` or `https://`, checked with `is_iroh_relay`); `off` 0..40; each URL "at most 256 characters, no whitespace"; unknown keys ignored within `v=1`; maximum parsed length 4,096 characters, longer is `NotALinkCode`; a plain code (no scheme) parses as `c` alone; a good code with bad servers returns the code and counts them in `dropped_servers`
- [ ] T011 [P] [US3] Tests in `src-tauri/src/sync/link/text_tests.rs`: build/parse round trip for no servers, added servers, switched-off built-in servers, switched-off added servers; plain code with spaces, dashes and lower case; `v=2` is `UnsupportedVersion`; missing or non-numeric `v` is `NotALinkCode`; unknown key ignored; a bad `nr` dropped and counted; 11 `nr` entries keep 10 and count one; a 65-character name is shortened on build at a character boundary (also with a multi-byte character at the cut); empty text and a 4,097-character text are `NotALinkCode`; the built text contains none of passphrase, key or device secret (a field-name audit over the builder's inputs, SC-006)
- [ ] T012 [US3] Size bound (R7) in `text::build`: if the text exceeds 1,200 bytes drop servers from the end (added `ir`, then `nr`, then `off`) until it fits and return their count as `omitted_servers`; test with 10 + 10 long URLs that the text is at most 1,200 bytes, `omitted_servers` is the count removed, and a second build of the same input is identical
- [ ] T013 [US3] `src-tauri/src/sync/link/code.rs`: `qr_svg` takes the text (not the bare code), error correction level M, `min_dimensions(320, 320)`; update `code_tests.rs` (the SVG is non-empty and decodes, in a test, to the link text via `rqrr` as a dev-dependency or, if not wanted, by comparing against a known render)
- [ ] T014 [US3] `src-tauri/src/sync/link/status.rs`: add `text: String` and `omitted_servers: u32` to `LinkCodeInfo` (ts-rs regenerates `src/types/bindings/LinkCodeInfo.ts`); `src-tauri/src/sync/link/host_task.rs::create_code` reads the vault name and `servers::read`, calls `text::build` and puts text and QR into `LinkCodeInfo`; the main-device-only check and the 10-minute single use stay unchanged (FR-012, spec 024 FR-023)
- [ ] T015 [US3] Add the command `link_text_parse` in `src-tauri/src/sync/commands.rs` (works with no vault open, registered next to `link_join_start`), map `LinkTextError` to error kinds `NotALinkCode` and `UnsupportedVersion` in the shared error type, and register it in `src-tauri/src/lib.rs`
- [ ] T016 [P] [US3] `src/composables/useDeviceLink.ts`: add `parseTextAsync(text): Promise<LinkText>`; `src/components/settings/LinkDeviceView.vue`: add a "Copy link" button (icon turns into a check mark, no success line) that copies `code.text`, one standing hint "Show this code only to your own device" and, when `omittedServers > 0`, one standing line saying that some servers are only in the copied link
- [ ] T017 [P] [US3] Add texts to `src/i18n/locales/*.json` (every shipped language, German first): copy-link button, hint, omitted-servers line, error kinds `notALinkCode`, `unsupportedVersion` ("Update HaexVault to read this code"); wording rules: "Nostr-Relay", "iroh-Relay", "Server", never a bare "Relay"; product name "HaexVault" (FR-014)
- [ ] T018 [US3] Integration test in `src-tauri/tests/sync_link.rs`: host with one added Nostr-Relay, one added iroh-Relay and one built-in server switched off creates a code; `link_text_parse` of `LinkCodeInfo.text` returns the same servers and the vault name; the link then completes with those servers (the existing join fixture)
- [ ] T019 PR B checks: `cargo fmt --check`, `cargo clippy --all-targets`, the link and sync test targets (`-j 4`), `pnpm typecheck`, `pnpm lint`, prettier on touched Markdown/JSON; open PR B

## PR C — prefill, paste and phone scan (US4, US1, US5)

- [ ] T020 [US4] `src/components/onboarding/LinkSheet.vue`: add `applyLinkText(raw)` that calls `parseTextAsync` and fills `code` (always, display form), `vaultName` (only when empty), and `nostrRelays`, `irohRelays`, `disabledRelays` from `servers` (replacing the sheet's lists); `deviceName`, `passphrase` and `passphraseConfirm` are never touched; call it on paste and on blur of the code field when the input starts with `haex-vault://` or is longer than a code; a plain code goes through the same call; errors only as `toast.error` with a fixed `id` (`NotALinkCode`, `UnsupportedVersion` have own texts); when `droppedServers > 0` one `toast.error` that the servers were not taken over (spec edge case)
- [ ] T021 [US4] Test (Node check via `scripts/lib/script-setup-sandbox.ts`, with a `vue-sonner` import double) in `src/components/onboarding/linkSheetApply.test.ts`: link text fills code, empty vault name and servers; a typed vault name is kept; a second apply replaces code, name-if-empty and servers but keeps device name and passphrases; `UnsupportedVersion` and `NotALinkCode` toast and fill nothing; a plain code fills only the code
- [ ] T022 [US5] `LinkSheet.vue`: when servers came with the text, put both `SettingsServerList` blocks in a collapsible closed by default with one summary line "N servers taken over from your other device" (N = nostr + iroh + disabled entries); when none came show them as today; switching off, adding and removing keep working and are sent in `joinStartAsync` as now; test in `linkSheetApply.test.ts` that the summary count and the arguments of `joinStartAsync` equal the parsed servers plus the user's changes
- [ ] T023 [US1] Add `src/composables/useQrScan.ts`: `scanAsync(): Promise<string | null>` with the phone path moved from `ShareImport.vue` (`scan({ formats: [Format.QRCode] })` through a dynamic import; `null` on cancel) and an injectable platform/camera seam so the desktop path of PR D plugs in without changing callers; error names mapped to toasts per R6
- [ ] T024 [US1] `ShareImport.vue`: replace its inline plugin call with `useQrScan` (behavior unchanged); keep its existing e2e/test expectations green
- [ ] T025 [US1] `LinkSheet.vue`: "Scan QR code" button (text label, keyboard reachable, `data-testid="link-scan"`) that calls `useQrScan().scanAsync()` and then `applyLinkText`; shown wherever a scanner exists, hidden nowhere without a replacement (desktop gets its path in PR D, until then the button shows only on phones)
- [ ] T026 [P] [US1] Texts for the scan button, denied-permission toast ("Allow the camera for HaexVault in the system settings"), "not a link code" toast and the servers summary in `src/i18n/locales/*.json` (same wording rules as T017)
- [ ] T027 [US4] E2E scenario `scripts/e2e/scenarios/link-qr.test.ts` (spec 016 rig, errors read with `scripts/e2e/lib/toasts.ts`): host shows a code, the test reads `LinkCodeInfo.text`, pastes it into the new instance's code field, checks code, vault name and collapsed servers, enters device name and passphrase, links; second case: pasting a plain code leaves the servers block open; third case: a `v=2` text shows the update toast and leaves the form empty
- [ ] T028 PR C checks: `pnpm typecheck`, `pnpm lint`, node checks, the `link-qr` scenario locally if the rig runs (otherwise via `workflow_dispatch`), prettier; open PR C. Manual M1 and M6 (quickstart) on a phone before merge

## PR D — desktop webcam scanner (US2)

Depends on PR A's result (T007) and PR C. If T007 chose the native fallback for an OS, T031/T032 use that path for it behind the same `useQrScan` interface.

- [ ] T030 [US2] `src/components/onboarding/LinkScanner.vue`: preview `<video>` in the sheet, frame loop every ~100 ms on a downscaled frame (longest side 640 px) drawn to a canvas, decode with `jsqr` loaded by dynamic import, stop at the first text that `parseTextAsync` accepts and emit it; a frame that decodes to something else is ignored, with a standing hint after 15 s "Hold the code in the middle of the picture" (not a toast); camera picker shown only with more than one camera
- [ ] T031 [US2] Desktop path in `src/composables/useQrScan.ts`: `camera_scan_begin` before `getUserMedia({ video: { facingMode: "environment" } })`, `camera_scan_end` after stop; stop all tracks and call `end` on success, close, unmount, `visibilitychange`/window blur and any error; retry without constraints on `OverconstrainedError`; no frame is stored, uploaded or logged (FR-007)
- [ ] T032 [US2] Error mapping in `useQrScan`: `NotFoundError` → "no camera found", `NotReadableError` → "camera is in use by another app", `NotAllowedError` → denied with where to allow it for the OS (Linux, Windows, macOS text variants); every case `toast.error` with a fixed `id`, paste and typing keep working
- [ ] T033 [US2] Camera choice (R6): after the first grant `enumerateDevices`; the chosen `deviceId` is stored as device-local preference `link.scan.cameraId` (applies on selection, no save button) and used next time; falls back to the default camera when the id is gone
- [ ] T034 [P] [US2] Test with an injected fake camera in `src/composables/useQrScan.test.ts`: success stops tracks and ends the gate; each error name gives its toast and ends the gate; close, unmount and blur end the gate and stop tracks (assert the track `stop` calls and the `camera_scan_end` call, no timers slept on); a frame decoding to a non-link text is ignored; the stored camera id is used and a missing id falls back
- [ ] T035 [P] [US2] Decode test `src/components/onboarding/linkScannerDecode.test.ts`: render the host's `qr_svg` of a typical text (about 150 bytes) and of a worst-case 1,200-byte text to an image in the test and decode it with the scanner's decoder function; both give the exact text (SC-002 groundwork)
- [ ] T036 [US2] Show the scan button on desktops (remove the phone-only condition of T025), open `LinkScanner` in the sheet, close it on success or cancel; the sheet's close watcher also ends the scan (FR-016: the scanner exists only inside the open link sheet)
- [ ] T037 [P] [US2] Texts for the preview, camera picker, hold-steady hint and the three denial variants in `src/i18n/locales/*.json`
- [ ] T038 PR D checks as T028; manual M2, M3, M4, M5, M7 of the quickstart on Windows, Linux and macOS with real webcams (record results in the PR); open PR D

## PR E — end to end and documentation

- [ ] T040 Extend `scripts/e2e/scenarios/link-qr.test.ts` with a fake-camera case for the desktop path if the rig can feed a video (Chromium-style fake device is not available in WebKitGTK; otherwise cover through T034/T035 and note why in the scenario file header)
- [ ] T041 [P] Update `specs/024-own-device-sync/contracts/tauri-commands.md` (`link_code_create` result, `link_text_parse`) and `specs/024-own-device-sync/research.md` R11 with a pointer to this spec; update `CONTEXT.md` (link text, camera gate)
- [ ] T042 Run the full quickstart (automated and M1–M7), write the results into `specs/085-qr-link/quickstart.md`, mark which manual checks remain; prettier on all touched Markdown and JSON; open PR E

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
