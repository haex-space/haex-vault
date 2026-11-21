# Implementation Plan: Diagnostic log

**Branch**: `spec/diagnostic-log` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/098-diagnostic-log/spec.md`

## Summary

The log becomes a feature of every build. A custom log sink, attached to the logger at the unlock of a vault and detached at
its close, writes the backend's lines and the interface's messages into `logs/` in the vault's own folder. The level
("Standard" or "Ausführlich") is kept in a table of the vault that is not synced, read at the unlock; "Ausführlich" ends after 24 hours or at the
next start of the app. The interface forwards its warnings, errors and uncaught errors through the log plugin's command.
Before the unlock nothing is written, kept or buffered. The approach and its alternatives are in [research.md](research.md).

## Technical Context

**Language/Version**: Rust 2021 (the `haex` crate, version 3.0.0), TypeScript with Vue 3 / Nuxt 4 for the interface.

**Primary Dependencies**: `tauri-plugin-log` 2.10 (already in `Cargo.toml`, with `fern` 0.7), `log` 0.4, `tauri-plugin-opener`
2.5 (already present). Interface messages use the session-scoped `diagnostic_log_ui_message` app command. New on Android: a share call in the
`tauri-plugin-haex-android` plugin with a `FileProvider`.

**Storage**: Plain-text files `logs/haex.log` and its rotated files in the vault's folder (`vaults/<name>/`, spec 084). The
level and its end time are one row of a new `_no_sync` table of the vault (`haex_diagnostic_log_no_sync`, one migration).

**Testing**: `cargo test` (unit tests in `*_tests.rs` beside the module, integration tests in `src-tauri/tests/`), the existing
pre-rename vault fixture for the secrets test, the interface's `scripts/check-*.ts` checks and `pnpm typecheck`, Android on the
emulator by hand (no Kotlin unit tests exist in the plugin).

**Target Platform**: Linux, macOS, Windows (desktop) and Android. iOS is not a target of the repository.

**Project Type**: desktop and mobile app (Tauri 2 with a Nuxt interface).

**Performance Goals**: no measurable cost in "Standard" for a `debug!` call that is filtered out (the global max level stays at
`Info`); a message of the interface costs no IPC call when it is filtered out in the interface.

**Constraints**: nothing is written before the unlock (FR-008); no global log (FR-010); no secret in any line (FR-009); the
log must not block or fail the app when the disk is full (FR-016); the sink must not hold a lock while it writes to disk longer
than one write.

**Scale/Scope**: "Standard" keeps about 1 MiB (4 files of 256 KiB), "Ausführlich" about 20 MiB (5 files of 4 MiB), per vault.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Spec Kit constitution (`.specify/memory/constitution.md`, v1.4.0) and the spaex constitution (`.spaex/constitution.md`):

| Principle                                                  | Result                                                                                                                                                            |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No secrets in git                                       | Pass. The feature touches no key material in the repository. The runtime log is the opposite concern and is covered by FR-009 and SC-003.                         |
| II. No local absolute paths in versioned config            | Pass. Every path is built from the vault's folder at run time; docs and tests use relative paths and temporary folders.                                           |
| III, IV, V. Identity, pinned revisions, opt-in sources     | Pass. Not touched. The one new npm dependency is pinned.                                                                                                          |
| VI. Self-modifying instructions are review-gated           | Pass. No instruction file is changed.                                                                                                                             |
| VII. Relay unavailability never blocks local work          | Pass. The log is local disk only.                                                                                                                                 |
| VIII. No concealment instructions                          | Pass.                                                                                                                                                             |
| ADR for principle-affecting decisions                      | The decision "a plain-text log lies beside the encrypted vault" is lasting, so an ADR (`docs/adr/`) is planned in the tasks, though it changes no Core Principle. |
| Tests in separate files (spaex)                            | Pass. Unit tests in `*_tests.rs`, integration tests in `src-tauri/tests/`.                                                                                        |
| 500 LoC per file (spaex)                                   | Pass by design: the sink, the level, the rotation and the commands are separate modules (see the structure).                                                      |
| Imports at the top of the file                             | Pass; applies to the new modules and the plugin.                                                                                                                  |
| `holzi-ui-feedback`: no success message inside a component | Pass. The setting shows no "Gespeichert"; a failure goes to the toast like the neighbouring settings.                                                             |
| New identifiers use `haex`                                 | Pass: preference keys `diagnostics.*`, events `haex:diagnostic-log-…`, module `diagnostic_log`.                                                                   |
| No backward compatibility                                  | Pass. The old global dev log file simply stops being written.                                                                                                     |

No violation, so the Complexity Tracking table stays empty. After the Phase 1 design the check holds unchanged; the one
point to watch in review is FR-009 (no secret in any line), covered by the test in the plan's last phase.

## Project Structure

### Documentation (this feature)

```text
specs/098-diagnostic-log/
├── plan.md              # This file
├── research.md          # Phase 0: decisions and alternatives
├── data-model.md        # Phase 1: level, its table, file layout, line format
├── quickstart.md        # Phase 1: how to see the feature work by hand
├── contracts/
│   ├── commands.md      # the Tauri commands of the setting
│   ├── events.md        # the events the interface listens to
│   └── log-format.md    # the file layout and the shape of a line
└── tasks.md             # Phase 2 ($speckit-tasks, not created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── diagnostic_log/
│   ├── mod.rs              # public surface: begin/end for a vault, level get/set, the plugin's target
│   ├── sink.rs             # the log::Log sink behind a swappable writer; drops when not attached
│   ├── rotation.rs         # size-based rotation of the files, injectable limits
│   ├── level.rs            # the two levels, their module filters, the global max level
│   ├── expiry.rs           # the row of the table, end time, run id, the timer for the end
│   ├── commands.rs         # level get/set, reveal folder, share (Android)
│   ├── sink_tests.rs
│   ├── rotation_tests.rs
│   ├── level_tests.rs
│   ├── expiry_tests.rs
│   └── commands_tests.rs
├── identity/migrations_diagnostic_log.rs   # the `_no_sync` table (registered in `migrations.rs`, test in `migrations_tests.rs`)
├── lib.rs                  # the plugin is built for every build, with the Dispatch target; commands registered
├── instances/session_services.rs   # begin the log at the start of the session services (create, open, import)
├── instances/close.rs      # end and flush at the end of the close
└── instances/vault_paths.rs        # `logs_dir(name)` beside the other subfolders

src-tauri/tests/
└── diagnostic_log_secrets.rs       # runs the pre-rename vault import in "Ausführlich" and searches the log for secrets

src-tauri/plugins/haex-android/                # `share_file` (ACTION_SEND with a FileProvider)

src/
├── plugins/diagnosticLog.client.ts          # forwards session-tagged warnings/errors, rate limit, level from the backend
├── lib/diagnosticLog.ts                     # pure helpers: text and stack of a message, the token bucket (tested in a node check)
├── components/settings/DiagnosticLogSetting.vue    # the section "Protokoll" in "Allgemein → Grundeinstellung"
├── lib/settings/registry.ts, lib/actions/settingsActions.ts   # the location and the catalog action (not callable by agents)
└── i18n/locales/{de,en}.json                # texts

scripts/check-diagnostic-log.ts              # node check of the helpers, in the style of the other check-* scripts
docs/adr/00NN-vault-folder-log.md            # the decision, written with the tasks
```

**Structure Decision**: one new backend module `diagnostic_log` with five small files and its own tests, hooked into the vault
session at the one place all three ways of getting a vault (create, open, import) already share, one small `_no_sync` table in the
shape of `haex_file_device_access_no_sync` (spec 056), and one interface plugin plus one settings component copied in shape from
the screen capture setting (spec 043 FR-011a), which is the same kind of device setting applied at the unlock.

## Complexity Tracking

No constitution violation to justify.
