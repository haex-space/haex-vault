# Research: Link a vault by scanning a QR code

## R1 The link text

- **Decision**: `haex-vault://link?v=1&c=<CODE>&n=<vault name>&nr=<url>…&ir=<url>…&off=<url>…`. Percent-encoded query
  with repeated keys. `c` is the 26 Base32 characters without dashes. Only the servers the user **added** (`nr`, `ir`) and the
  servers **switched off** (`off`, built-in or added) are listed; the built-in servers are implied. Full grammar in
  [contracts/link-text.md](./contracts/link-text.md).
- **Why**: the new build knows the same built-in servers, so listing them wastes QR modules; "added + switched off" is exactly
  what `ServerConfig` and `LinkJoinStartArgs.servers` already model (`nostrRelays`, `irohRelays`, `disabled`), so the parsed text
  maps onto the join arguments with no translation. A repeated-key query is the form the `url` crate already parses.
- **Alternatives**: JSON or postcard in the QR (longer or opaque, no benefit; the text must stay copyable and readable);
  a short code looked up on a server (needs a server, which the link flow deliberately avoids); listing all effective servers
  (larger and wrong if the built-in list changes between versions).

## R2 One parser, in Rust

- **Decision**: `sync/link/text.rs` owns build and parse; the frontend calls `link_text_parse` (works without an open vault, like
  `link_join_start`) for every input: scan, paste, typing a full code. TypeScript never parses the text.
- **Why**: spec FR-015 and the one-implementation rule; the parser reuses `LinkCode::parse` and `servers::validate`, so a text
  can never fill the form with something `link_join_start` would refuse differently.
- **Behavior**: a plain code (with dashes, spaces, lower case) parses as a text without servers (FR-003, SC-005); a newer `v`
  returns `UnsupportedVersion`; any other text returns `NotALinkCode`; a good code with a bad server list returns the code and
  a count of dropped servers (spec edge case). Unknown query keys of the same version are ignored (forward-compatible within a
  version), unknown versions are refused.

## R3 Phones: the existing plugin

- **Decision**: `tauri-plugin-barcode-scanner` through a new `useQrScan` composable that holds the call now in
  `ShareImport.vue` (`scan({ formats: [Format.QRCode] })`); the plugin asks for the camera permission itself, at the tap.
- **Why**: already in the app and capabilities (`mobile.json`), Android and iOS, no WebView camera rights (spec 043 keeps those
  closed).
- **Alternatives**: the desktop path on phones (rejected: needs `WebChromeClient` camera grants that spec 043 leaves closed).

## R4 Desktop: webview camera + `jsqr`, native fallback

- **Decision**: open the camera with `navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" } })` into a
  `<video>`, draw each ~100 ms a downscaled frame (longest side 640 px) to an `OffscreenCanvas` / `<canvas>`, decode with `jsqr`,
  stop at the first text that `link_text_parse` accepts. Load the decoder with a dynamic import so it is not in the start bundle.
- **Why `jsqr`**: pure JavaScript, no WASM, so `script-src 'self'` stays as it is; small (about 50 KB), MIT, offline, a single
  function. WebKit (macOS, Linux) has no `BarcodeDetector`, and WebView2 has it only behind a flag, so a built-in API cannot be the
  one implementation.
- **Alternatives**: `zxing-wasm` (more robust on blurry codes, but WASM needs `'wasm-unsafe-eval'` in `script-src`, a CSP
  loosening for the whole app); `BarcodeDetector` (not portable); decoding in Rust with `rqrr` on frames sent over IPC (works,
  but ships every frame through IPC and the permission problem is not solved by it).
- **Fallback if a webview cannot open the camera (decided by the spike, PR A)**: capture in Rust with `nokhwa` (V4L2,
  Media Foundation, AVFoundation) and decode with `rqrr`; show the preview as JPEG data URLs on an event. Same `useQrScan`
  interface, so PR C and the sheet do not change. Costs: two Rust crates and the OS camera permission text; chosen only per
  OS where the webview fails.
- **Linux specifics to check in the spike**: WebKitGTK exposes `navigator.mediaDevices` only when its `enable-media-stream`
  setting is on; wry's permission handler already routes `UserMediaPermissionRequest`, but the setting is not set by wry.
  It is set through `with_webview` on the main window (Linux only). The runtime also needs GStreamer with the PipeWire/V4L2
  plugins; the dev shell, the AppImage and the Nix package must be checked (quickstart M3).

## R5 Camera permission and its gate

- **Decision**: register `on_permission_request` on the app builder. For `PermissionKind::Camera` it answers `Allow` only while
  the camera gate is open and `Default` otherwise (which keeps the system's own behavior); every other kind stays `Default`.
  The sheet opens the gate with `camera_scan_begin` before `getUserMedia` and closes it with `camera_scan_end` when the preview
  closes (also on unmount and on window blur).
- **Why**: Tauri's handler receives the webview and the kind, not the origin, and extension frames run in the same webview
  (spec 017). The gate plus the missing `allow="camera"` on extension iframes (Permissions-Policy denies camera in cross-origin
  frames by default) keeps an extension from opening the camera; this is also spec FR-016. The microphone path of voice is
  native (`cpal`) and untouched.
- **OS parts**: macOS `NSCameraUsageDescription` in `Info.plist` (next to the microphone text; the app is ended without it);
  Windows WebView2 raises its own prompt when the handler answers `Default`, and `Allow` skips it, which is why the gate must be
  strict; the system's camera privacy switch for desktop apps can still deny, which surfaces as `NotAllowedError` (R6).
- **Alternatives**: allowing camera always (rejected: any frame in the window could open it).

## R6 Camera lifecycle and errors

- **Decision**: `useQrScan` owns the stream: stop all tracks on success, close, unmount, `visibilitychange`/window blur and any
  error; no frame leaves the composable. Error names map to toasts: `NotFoundError` (no camera), `NotReadableError` (busy),
  `NotAllowedError` (denied, with where to allow it per OS), `OverconstrainedError` (retry without constraints). Camera choice:
  `enumerateDevices` after the first grant; the picked `deviceId` is a device-local preference.
- **Why**: spec FR-007, FR-008, SC-004. The fake camera in the composable test (an injected `getUserMedia`) makes each branch
  testable without hardware or sleeps.

## R7 Size of the QR code

- **Decision**: the host puts in at most 10 added servers per kind (`MAX_SERVERS`, the settings' own limit), each at most 256
  characters, and a vault name of at most 64 characters (longer is shortened at a character boundary). That is at most about
  3.3 KB in the worst case and about 150 bytes in the usual one. The SVG is rendered with `min_dimensions(320, 320)` at error
  correction level M. If a text would exceed 1,200 bytes the host drops servers from the end and reports `omittedServers` in
  `LinkCodeInfo`; the view shows one standing hint line, and the copyable link stays complete.
- **Why**: a phone must read it from a laptop screen; above roughly 1.2 KB the modules get too small at normal distance (SC-002).
  The copyable link has no such limit, so nothing is lost for users who paste.
- **Alternatives**: ECC level L (denser, less tolerant of glare and a bad camera); refusing to show a code (a code is always
  better than none).

## R8 Host view

- **Decision**: `LinkDeviceView.vue` renders `LinkCodeInfo.qrSvg` as now, adds "Copy link" next to the existing code text, and
  one hint line ("Show this code only to your own device"). No new screen. `LinkCodeInfo` gains `text` and `omittedServers`.
- **Why**: keeps the main device's layout (spec assumption) and gives the paste path (story 4) its source.

## R9 Opening a link from outside (not in this spec)

- **Decision**: `haex-vault://link` opened by the system (a phone's own camera app) is **not** handled; `known_link` keeps
  answering only to `contact` and `mailto`.
- **Why**: a link on a fresh installation must open the landing page and the sheet, and with an open vault it must be refused;
  that is its own flow with its own states. The text format is chosen so it can be added later without a change (it already is
  a scheme the app registers).

## R10 Form behavior

- **Decision**: `LinkSheet.vue` gets one handler, `applyLinkText(raw)`, that calls `link_text_parse` and fills: code (always),
  vault name (only when empty), servers (replaces the sheet's `nostrRelays`, `irohRelays`, `disabledRelays`). The device name
  and the passphrases are never touched. When servers came, `SettingsServerList` sits in a collapsible with a one-line summary
  ("N servers taken over from your other device"); when none came it shows as today. The code field accepts a pasted link
  text and applies it on paste and on blur.
- **Why**: spec FR-009, FR-010; the existing reset, cancel and `canSubmit` logic stays as is.
