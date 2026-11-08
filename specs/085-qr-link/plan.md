# Implementation Plan: Link a vault by scanning a QR code

**Branch**: `085-qr-link` | **Date**: 2026-11-08 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/085-qr-link/spec.md`

## Summary

The main device's QR code and copyable link become one versioned text, `haex-vault://link?…`, that carries the link code, the
vault name and the servers a new device cannot know (the Nostr-Relays and iroh-Relays the user added, and the servers switched
off). The new device reads that text in **one Rust parser** (a Tauri command that works without an open vault) whether it came
from a phone scan, a desktop webcam scan, a paste or typing, and prefills the existing link form. The phone scan reuses the
barcode plugin of spec 051. The desktop scan opens the camera in the webview (`getUserMedia`) and decodes frames with a small
pure-JS decoder; the camera permission is allowed by the host **only while the link sheet's scanner is open**. Nothing of the
link protocol (code, proof, confirmation, transfer) changes.

The one real risk is the desktop camera in each OS's webview, above all WebKitGTK on Linux. The plan therefore starts with a
spike on all three desktop OSes and defines a native fallback (R4) if a webview cannot do it.

## Technical Context

**Language/Version**: Rust 2021 (src-tauri), TypeScript / Vue 3 + Nuxt (src)

**Primary Dependencies**: existing: `qrcode` 0.14 (render), `url` (parse), `tauri-plugin-barcode-scanner` 2.5 (phones), `vue-sonner`;
new: one pure-JS QR decoder (`jsqr`, R4), no new Rust crate

**Storage**: none (the link text is never stored; nothing is stored)

**Testing**: `cargo test` for the text codec (`text_tests.rs`) and the camera gate; vitest/Node checks for the scanner
composable with a fake camera; e2e (spec 016 rig) for paste and prefill, and a decode test over generated QR images; manual
M1 to M6 for real cameras (quickstart)

**Target Platform**: Linux (WebKitGTK), Windows (WebView2), macOS (WKWebView) for the camera path; Android for the plugin path;
iOS not verified (not on the roadmap)

**Project Type**: desktop + mobile app (Tauri 2)

**Performance Goals**: first decoded frame within 1 s of the preview opening; camera off within 1 s of close (SC-004); decode
at about 10 frames per second on a downscaled frame without a visible stutter in the preview

**Constraints**: no network for decoding; `script-src 'self'` stays (no `wasm-unsafe-eval`); camera never opens outside the
link sheet; the passphrase and keys never appear in the text

**Scale/Scope**: one sheet, one scanner component, one composable, one Rust module for the text, one small Rust module for the
camera gate; about 600 net lines over five PRs

## Constitution Check

_GATE: passed before Phase 0, re-checked after Phase 1._

| Rule                                                                      | Status                                                                                                            |
| ------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Worktree on a topic branch, PR to `main`, Conventional Commits, no squash | Planned: `.worktrees/085-qr-link`, five PRs (below)                                                               |
| Graphify first; extend before authoring                                   | Done, see "Candidates evaluated"                                                                                  |
| Smallest implementation; no new dependency if avoidable                   | One JS dependency, justified in R4 (WebKit has no `BarcodeDetector`); no Rust crate                               |
| One implementation per rule                                               | The text is built and parsed in one Rust module; TS never parses it (R2); server checks reuse `servers::validate` |
| Tests next to behavior, in separate files, deterministic                  | `text_tests.rs`, `camera_gate_tests.rs`, composable test with a fake camera, no sleeps                            |
| Errors as toast, no inline success lines, one toast per error             | All scan and parse errors via `toast.error`; the filled form is the success (spec FR-013)                         |
| Settings apply on selection                                               | No new setting: the camera pick holds for the open scan only                                                      |
| Product name HaexVault, no bare "Relay", specs in English                 | Texts reviewed in tasks (FR-014)                                                                                  |
| No secrets in git, no local absolute paths                                | Fixtures use generated codes; no paths                                                                            |
| Every device and OS (memory: holzi-every-os)                              | Desktop camera on three OSes with a native fallback (R4); phones via plugin; iOS explicitly out                   |
| Encrypted storage and credentials rule                                    | Not touched: no credentials, no stored data                                                                       |
| ADR for principle-affecting decisions                                     | None needed: no Core Principle changes. The camera-permission gate (R5) is recorded in research                   |
| 500 LoC per file                                                          | New files stay far below                                                                                          |

### Candidates evaluated (graphify-first)

| Candidate                                                               | Result                                                                                                                                                          |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LinkCode::qr_svg` in `sync/link/code.rs`                               | **Extend** (approval in research R11): the QR is rendered from the link text instead of the bare code; same renderer                                            |
| `servers::validate`, `ServerConfig`, `MAX_SERVERS` in `sync/servers.rs` | **Reuse** to check servers read from a text and to bound what the host puts in                                                                                  |
| `LinkCode::parse`                                                       | **Reuse** for the code inside the text and for plain codes                                                                                                      |
| `scanAsync` in `ShareImport.vue`                                        | **Extend** (approval in research R11): its plugin call moves into a shared `useQrScan` composable; contacts can adopt the desktop path later (not in this spec) |
| `SettingsServerList`, `serverEntries`                                   | **Reuse** unchanged inside the collapsed block                                                                                                                  |
| `known_link` / `LINK_PREFIX` in `deep_link/mod.rs`                      | **Not extended**: it answers only to `haex-vault://contact/`; opening a link text from outside is later (R9)                                                    |
| Contact share text (`contacts/share`)                                   | Not a match: a different payload and a different trust model; only the scheme is shared                                                                         |

## Project Structure

### Documentation (this feature)

```text
specs/085-qr-link/
├── plan.md
├── research.md          # decisions R1–R10, incl. the desktop camera spike and fallback
├── data-model.md        # the link text and its parsed form
├── quickstart.md        # automated and manual validation
├── contracts/
│   ├── link-text.md     # grammar, limits, versioning
│   └── tauri-commands.md
└── tasks.md             # by /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── sync/link/
│   ├── text.rs                # NEW: LinkText build + parse (the one implementation)
│   ├── text_tests.rs          # NEW
│   ├── code.rs                # qr_svg renders the link text
│   └── host_task.rs           # create_code puts name + added/disabled servers into the text
├── sync/commands.rs           # NEW command link_text_parse; LinkCodeInfo gains `text`
├── camera_gate.rs             # NEW: allow Camera in the webview only while a scan is open
├── camera_gate_tests.rs       # NEW
└── lib.rs                     # on_permission_request, with_webview media setting (Linux), commands
src-tauri/Info.plist           # NSCameraUsageDescription
src/
├── composables/useQrScan.ts   # NEW: phone plugin path + desktop camera path, one result type
├── composables/useDeviceLink.ts   # linkTextParseAsync
├── components/onboarding/LinkSheet.vue    # scan button, prefill, collapsed servers
├── components/onboarding/LinkScanner.vue  # NEW: camera preview + frame loop (desktop)
├── components/settings/LinkDeviceView.vue # shows/copies the link text, hint
├── components/contacts/ShareImport.vue    # calls useQrScan for the phone path (same behavior)
└── i18n/locales/*.json
scripts/e2e/scenarios/link-qr.test.ts      # NEW
```

**Structure Decision**: the text codec lives with the rest of the link protocol in `sync/link/`; the camera gate is its own small
module because it belongs to the window, not to sync. The frontend adds one composable and one component and changes two.

### Delivery in five PRs

1. **A – Spike and gate** (R4, R5): `camera_gate`, Linux media setting, Info.plist, a throwaway page that opens the camera and
   decodes a QR on each desktop OS. Gate: if a webview fails, decide the fallback before B.
2. **B – Link text** (R1, R2, R7): `text.rs`, `link_text_parse`, `LinkCodeInfo.text`, host shows and copies it.
3. **C – Sheet prefill and phone scan** (R3, R8): paste/scan into the form, collapsed servers, `useQrScan` phone path.
4. **D – Desktop scanner** (R4, R6): `LinkScanner.vue`, camera choice, lifecycle, errors.
5. **E – E2E and docs**: scenarios, quickstart run, manual M1–M6 results.

## Complexity Tracking

No constitution violations. The one dependency (`jsqr`) is justified in R4.
