# Implementation Plan: Delegates reach the web only through HaexVault's agent browser

**Branch**: `102-delegate-web-tools` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/102-delegate-web-tools/spec.md`

## Summary

Every start of a subscription delegate gets its own web tools and browser switched off by the program itself, in every
autonomy mode and whether or not the MCP access is on; computer use and everything else stay. Claude Code starts with an
allow-list of its built-in tools without `WebFetch`/`WebSearch` and with `--no-chrome`; Codex with `web_search="disabled"`
and five browser features disabled; Abacus with deny rules for its bundled browser command in its settings file, in both
modes (its web search and page fetch run on its server and are a named residual). Where the program reports its state at start (Claude's init line, Codex' `config/read` and
`experimentalFeature/list`), HaexVault checks it and ends the request with a readable reason if the block is missing. One
module records the block per vendor and the program versions it was checked against; a test over the shipped catalog
fails for a tested version without that check. Details and measurements in [research.md](research.md).

## Technical Context

**Language/Version**: Rust 1.77.2 edition 2021 (`src-tauri`), TypeScript 5 / Vue 3 / Nuxt (one Settings line).

**Primary Dependencies**: none new; `serde_json`, `tokio`, `semver` and the existing delegate drivers.

**Storage**: N/A. No table; the block is arguments on each start, for Abacus a settings file in the run folder (as today).

**Testing**: `cargo test` with sibling `*_tests.rs` files (unit tests on the command builders and the confirmations, the
catalog test of R5); the existing unix stub delegates in `src-tauri/tests/` extended with init lines and app-server answers
with and without the block; Node checks for the Settings text keys. Real-program checks are manual
([quickstart.md](quickstart.md)); research R4 was measured with real Abacus runs on 2026-11-27.

**Target Platform**: desktop (Linux, macOS, Windows), where delegates run. Android/iOS: no delegates.

**Project Type**: desktop app (Tauri backend plus Nuxt frontend).

**Performance Goals**: the Codex confirmation adds two local requests to the app-server before the thread starts (no
network); Claude's is a read of a line that already arrives.

**Constraints**: the block never rests on environment variables or the instruction (spec FR-005); a failed confirmation
never falls back to another backend (FR-007); nothing besides the web tools and the browser changes (FR-010); files under
about 500 lines (`codex.rs` already carries a maintainability exception, so its new check goes into the new module);
cargo with the memory-limited flags of this machine, or CI only while the app runs.

**Scale/Scope**: three vendors, three command builders, two confirmations, one module, one Settings line, one test over
the catalog.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                       | Check                                                                                                       |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| I. No secrets in git                            | Pass. The Abacus run of R4 keeps the device code in the child's environment and scratchpad.                 |
| II. No local absolute paths in versioned config | Pass. No config file is added; the probes' paths stay in the scratchpad.                                    |
| III. Device-independent identity                | Pass. Not touched.                                                                                          |
| IV. Cross-repo references pin revisions         | Pass. No cross-repo content.                                                                                |
| V. External sources opt-in                      | Pass. No external source.                                                                                   |
| VI. Self-modifying instructions review-gated    | Pass. The delegate instruction (`DELEGATE_INSTRUCTION`) is unchanged (FR-009); all changes go through a PR. |
| VII. Relay unavailability never blocks local    | Pass. Not touched.                                                                                          |
| VIII. No concealment instructions               | Pass. A refused block is shown to the person (FR-007).                                                      |

Re-check after Phase 1: unchanged, pass.

## Project Structure

### Documentation (this feature)

```text
specs/102-delegate-web-tools/
├── plan.md              # This file
├── research.md          # Phase 0: switches per vendor, measured
├── data-model.md        # Phase 1: the web block record
├── quickstart.md        # Phase 1: real-program checks, incl. the check of a new version
├── contracts/
│   └── web-block.md     # Phase 1: arguments and confirmation per vendor
└── tasks.md             # Phase 2 ($speckit-tasks)
```

### Source Code (repository root)

```text
src-tauri/src/adapters/cli_delegate/
├── web_block.rs            # NEW: per-vendor block (arguments, names to confirm, checked versions), the Claude and
│                           #      Codex confirmations as pure functions
├── web_block_tests.rs      # NEW: arguments per vendor and mode, confirmations, catalog vs. checked versions
├── claude.rs               # build_command adds the block; the init line is confirmed before the first chunk
├── codex.rs                # build_command adds the block; after `initialize`, config/read + experimentalFeature/list
├── abacus.rs               # build_command adds the deny rules in both modes
├── abacus_models.rs        # the catalog run gets the same arguments (one builder)
├── mod.rs                  # `mod web_block;`
└── *_tests.rs              # existing builder tests learn the new arguments
src-tauri/tests/
└── cli_delegate_*_run.rs   # stub transcripts with and without the block (Claude init, Codex answers)
src/i18n/locales/{de,en}.json   # settings.autonomyMode.webBlock: the block and its residuals (FR-008)
src/components/…            # the Settings view of the autonomy modes shows the line (file found in tasks)
specs/096-abacus-delegate/research.md  # §16 pointer to spec 102 for the deny rules in "Ungated"
```

**Structure Decision**: one new module in `adapters/cli_delegate/` next to `own_tools.rs`, which already keeps per-vendor
tool knowledge as data plus pure functions; the drivers call it at the two places they already shape a start (the
command builder and the start-up read).

## Phases of delivery

- **PR A (P1, US1+US2)**: `web_block.rs`, the three builders, the Claude and Codex confirmations, stub tests, the Abacus
  deny rules (measured, research R4), the test of R6 for the notice.
- **Operator run**: the quickstart per vendor and mode.
- **PR B (P2, US3 + polish)**: the catalog test of R5 with the checked versions, the Settings line naming the residuals,
  spec 096 pointer.

## Complexity Tracking

No constitution violation.
