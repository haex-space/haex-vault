# Quickstart: validating the QR link

## Automated

1. Rust: `nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 link::text` and the same with `camera_gate` – round trip
   of build and parse, plain codes, unknown version, unknown keys, bad servers dropped with a count, QR size limit (no QR above
   1,200 bytes), no secret fields in the output.
2. Frontend: `nix develop --command pnpm typecheck` and the composable test with a fake camera (permission denied, no camera,
   busy, success, stop on every exit).
3. E2E (spec 016 rig): `link-qr` scenario – host shows a code, the test reads `LinkCodeInfo.text`, pastes it into the new
   instance's code field, checks the form (code, vault name, collapsed servers), fills device name and passphrase, links.
   Also: a decode test that renders the host's SVG to an image and runs it through the same decoder as the scanner.

## Manual (real cameras)

| #   | Check                                                                           | Expect                                                                      |
| --- | ------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| M1  | Phone: scan the QR of a main device                                             | form prefilled; only device name and passphrase left                        |
| M2  | Windows desktop with webcam: scan from a phone screen                           | same; camera light off within 1 s after the scan                            |
| M3  | Linux desktop with webcam (dev shell, AppImage): scan                           | same; if `navigator.mediaDevices` is missing the spike failed (R4 fallback) |
| M4  | macOS desktop with webcam: scan                                                 | system prompt once, then same                                               |
| M5  | Desktop without camera / permission denied / camera busy                        | one toast naming the case; paste and typing work                            |
| M6  | Older main device (plain code QR)                                               | scan fills the code; servers block stays open                               |
| M7  | Close the sheet while scanning                                                  | camera off at once                                                          |
| M8  | Time a user from the scan to the started link (device name and passphrase only) | at most 30 s (SC-001)                                                       |
| M9  | Scan a QR code shown on a phone screen 10 times per OS                          | at least 9 succeed on the first try (SC-002)                                |

## Results (2026-11-08)

Automated, all green: `link::text` (19 unit tests), `camera_gate` (6), `sync_link` (7, one new), `check:settings` with
`check-link-form` (6) and `check-camera-scan` (13, including the decoder reading the QR code of a usual text and of one at the
1,200-byte limit), typecheck, lint, clippy `-D warnings`. The `link-qr` e2e scenario was written but not run locally.

Linux camera (part of M3): the webview shows one frozen picture, so the camera is read natively there (research "R4 spike results"). `link_camera::capture` has a hardware test (`cargo test link_camera::capture -- --ignored`): 45 or more distinct JPEGs in 3 s, passed on the Integrated Camera of the author's laptop. Decoding a real QR code in front of the camera was not tried. Manual checks still open (no QR code has been scanned by a camera yet): M1 (phone scan), M2 (Windows), M3 (Linux),
M4 (macOS), M5 (no camera, denied, busy), M6 (older main device), M7 (close while scanning), M8 (30 s), M9 (9 of 10 scans). The
spike page for M2 to M4 is on branch `085-a-spike`.
