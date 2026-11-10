# Implementation Plan: A harness that lets small local models do everyday tasks

**Branch**: `082-agent-harness` | **Date**: 2026-11-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/082-agent-harness/spec.md`; the failure analysis and the routing
measurement in [research.md](research.md) and `analysis/`.

## Summary

The work comes in two halves.

1. **Measure as the app runs.** The everyday set builds each step's offer with the chat's own function and plays the
   app's permission dialog in Auto mode, with a user who agrees to what was asked for and declines everything else.
   Every model then gets a new baseline.
2. **Improve the harness.**
   - **Routing**: a short routing call by the loaded model names the request's domain (90 % on the everyday set for
     Qwen3.5 4B, 92 % for Gemma 4 E4B, about 0.1 s).
   - **The offer**: the first step offers that domain's tools, its lookups always included, and no window tools
     unless they are needed.
   - **Guard**: holzi turns away ids nobody found and unknown tools, and every error answer says what to do next.

## Technical Context

**Language/Version**: Rust (edition and MSRV of `src-tauri/Cargo.toml`); TypeScript for the action declarations

**Primary Dependencies**: existing only (the adapters' `ProviderAdapter` for the routing call, `serde_json`)

**Storage**: none; the domain declarations are code and exported JSON (`chat/eval/tools.json`)

**Testing**:

- `cargo test --lib` for the domains, the routing prompt and parsing, the offer, the guard, the played dialog and
  the counting.
- The offer of every everyday request without a model (spec US2's independent test).
- The expected chains of every request against the guard (research R6).
- The everyday set with real models (`measure_everyday_voice`): baselines and each delivery step.
- `pnpm check:agent-actions` and the export of the tools for the TypeScript part.

**Target Platform**: desktop and Android (the same harness; phones without the command tool)

**Project Type**: desktop and mobile app (Tauri)

**Performance Goals**: the routing call adds at most 0.2 s to a turn on the reference laptop (measured 63 to 172 ms)

**Constraints**:

- Builds in the 10 GB memory scope with `-j 1` (lib tests with `--config 'profile.dev.package.holzi.debug=0'`).
- The CUDA build for the measurements per spec 078's quickstart.
- The everyday set's 106 requests are the yardstick: nothing is tuned on them (research R3).

**Scale/Scope**: five new modules, changes to the offer, the tool round's call planning, the instruction, the
everyday runner and report, and the action declarations.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                                                           | Result |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | No secrets involved                                                                                                                             | Pass   |
| II No local absolute paths                              | The analysis scripts take paths as arguments; the stored outputs contain none                                                                   | Pass   |
| III Device-independent identity                         | Not touched                                                                                                                                     | Pass   |
| IV Pinned cross-repo references                         | The mail extension stays pinned as in spec 078                                                                                                  | Pass   |
| V External sources opt-in                               | No harness content                                                                                                                              | Pass   |
| VI Self-modifying instructions review-gated             | The tool instruction for models changes in this PR, reviewed with it; no agent instruction file changes                                         | Pass   |
| VII Relay never blocks local work                       | Not touched                                                                                                                                     | Pass   |
| VIII No concealment instructions                        | The routing prompt and the instruction hide nothing from the user                                                                               | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch, one PR                                                                                                                        | Pass   |
| spaex: 500 LoC per file                                 | New modules for new logic; `tool_round.rs` (499) and `scoring.rs` (491) split in this change (research R11); `chat/send.rs` (538) one call only | Pass   |
| spaex: graphify first                                   | `graphify query` before each new function                                                                                                       | Pass   |
| spaex: tests in separate files                          | `*_tests.rs` next to each module                                                                                                                | Pass   |
| spaex: ponytail comments                                | The domain size limit (12), the id-name rule and the routing prompt's form are rules from one measurement; marked with `ponytail:`              | Pass   |

Checked against `.specify/memory/constitution.md` and `.spaex/constitution.md`. Re-check after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/082-agent-harness/
├── plan.md
├── research.md          # R1–R11
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── domains.md       # the domain declaration and the routing call
│   └── everyday-report.md
├── analysis/            # failure analysis, offer probe, routing measurement, development set
└── tasks.md             # $speckit-tasks
```

### Source Code (repository root)

```text
src/lib/actions/agentTools.ts            # AGENT_DOMAINS
scripts/export-eval-tools.ts             # exports the domains with the tools
src-tauri/src/chat/tools/
├── domains.rs           # new: domains from actions and extensions
├── route.rs             # new: the routing call and its parsing
├── offer.rs             # the routed first offer (R5); shared with the measurement
├── prompt.rs            # the instruction's new sentence
└── *_tests.rs
src-tauri/src/chat/turn/
├── guard.rs             # new: ids nobody found, unknown tools, closing sentences
├── plan_calls.rs        # moved out of tool_round.rs
└── tool_round.rs        # calls guard.rs
src-tauri/src/chat/send.rs               # the first offer through the routed builder
src/lib/actions/runner.ts                # closing sentence per error code
src-tauri/src/chat/eval/everyday/
├── consent.rs           # new: the played dialog and the four endings of a call
├── runner.rs            # the app's offer builder, the platform, the dialog
├── scoring.rs           # outcomes from consent.rs
└── report.rs            # the new counts (contract)
src-tauri/tests/model_tool_eval.rs       # platform profile, routing on/off for the baseline
```

**Structure Decision**: the harness stays in `chat/tools` and `chat/turn`; the measurement shares the chat's offer
and permission decision instead of copying them.

## Delivery order

1. **Measurement** (US1): shared offer builder, platform, played dialog, counting, report. Then the baselines of all
   models (research R10).
2. **Guard** (US4, US5): ids nobody found, unknown tools, closing sentences. Measure.
3. **Domains and routing** (US2): declarations, routing call, routed offer, instruction (US3). Measure.
4. **Hosted models** (US6): routing on and off; decide per measurement (research R3).
5. **Results**: `specs/082-agent-harness/results.md` with every model before and after each step.

Steps 2 and 3 are measured separately so each one's effect is visible.

## Complexity Tracking

No violations.
