# Android runs the same core; platform code lives in one plugin crate

Status: accepted
Date: 2026-10-07

## Decision

holzi on Android (spec 043) is the same Rust core and the same Nuxt interface as on
the desktop. There is no second implementation of any rule for Android.

- **One plugin crate for platform code.** Everything only Android has (screen
  capture protection, system insets and keyboard height, runtime permissions, the
  device name, network change events) lives in the local Tauri plugin crate
  `src-tauri/plugins/haex-android/`, with its Kotlin, manifest entries, keep rules
  and Gradle dependencies. On the desktop the crate compiles and does nothing.
- **The Android project is committed.** `src-tauri/gen/android` comes from
  `tauri android init` and is checked in. Changes to it are limited to the
  activity's `configChanges` and the release signing configuration, so a newer
  Tauri template can be regenerated and compared.
- **Certificates are checked against the system store.** `rustls-platform-verifier`
  is initialised at startup from the Android context (`ndk-context`, `jni`). `lettre`
  calls an API of that crate that does not exist on Android; holzi uses a patched
  fork pinned by commit until the fix is released upstream. Bundled root lists
  (`webpki-roots`) are not used: they are not the roots the phone trusts.
- **Closing the vault ends the app.** ADR-0003 (one vault session per app process)
  holds unchanged. Where the desktop release build restarts to show the vault
  picker again, Android ends the activity and the process; the next start shows the
  picker. `tauri::process::restart` is never called on Android.
- **Local models use the desktop loader.** On Android holzi runs local models with
  the same loader as on the desktop (mistralrs, GGUF, CPU) and speech recognition
  with the same Whisper code. This replaces the part of ADR-0002 that planned a
  native MLC backend for mobile; the model family and the phone presets of ADR-0002
  (Qwen3-1.7B, Qwen3-0.6B for low memory, Q4_K_M) stay.

## Rationale

The core already compiles for `aarch64-linux-android` almost unchanged (a
`cargo check` on 2026-10-07 failed only in `lettre` and two desktop-only calls).
A second backend or a second file layer for Android would be a second
implementation of rules that exist once today (one loader, one tokenizer, one
tool-call format; one place that reads chosen files). Keeping Kotlin in a plugin
crate keeps it out of the generated template, which Tauri overwrites on
regeneration.

## Consequences

- New Android-only Rust dependencies: `jni` 0.22 and `ndk-context` 0.1. New Gradle
  dependency in the plugin crate: `org.rustls:rustls-platform-verifier` at the
  version of `rustls-platform-verifier-android` in `Cargo.lock`.
- `[patch.crates-io]` carries `lettre` from `haexmas/lettre` at a fixed commit until
  upstream releases the fix; then the patch is removed.
- Local models run on the phone's CPU only. Whether the phone preset answers within
  15 seconds (spec 043 SC-009) is measured in stage 5 and recorded here; if it does
  not, Qwen3-0.6B becomes the phone suggestion. A native backend would be a new
  decision with its own ADR.

## Related

- ADR-0002 (local model profiles): the MLC part is superseded by this ADR.
- ADR-0003 (one vault session per app process).
- Spec 043, research R1, R2, R4, R11, R17.
