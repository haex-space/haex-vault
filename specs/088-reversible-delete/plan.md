# Implementation Plan: The reversible way first

**Branch**: `088-reversible-delete` | **Date**: 2026-11-11 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/088-reversible-delete/spec.md`

## Summary

When a local model proposes a final action (declared effect `destructive`, risk class `Risky`), holzi asks the model
one short question before the permission dialog: did the user ask for exactly this, and which offered changing tool
of the same area does it reversibly. If the user did not ask for it and a tool is named, the call is turned away with
an answer naming that tool; otherwise the dialog asks as today. The everyday measurement runs the same check and
reports it. Measured on the chain set first, then on the everyday set for the local models.

## Technical Context

**Language/Version**: Rust (edition 2021, MSRV per `src-tauri/Cargo.toml`)

**Primary Dependencies**: the chat turn (`chat/turn/plan_calls.rs`, `turn/guard.rs`, `turn/mod.rs`), routing
(`chat/tools/route.rs`: `RouteCall`, `route_timeout`), domains (`chat/tools/domains.rs`: `domain_of`), the
permission gate (`chat/tools/permission.rs`), the everyday measurement (`chat/eval/everyday/`), llama.cpp for local
models (CUDA build per spec 078/082 quickstart)

**Storage**: none; runs under `specs/088-reversible-delete/runs/`

**Testing**: lib tests in `*_tests.rs` (`--config 'profile.dev.package.holzi.debug=0'`, `-j 1`); a fake adapter
answers the check in unit tests; the chain set and the everyday set as model measurements

**Target Platform**: desktop (Linux reference laptop, RTX A2000 8 GB) and the phone profile in the measurement

**Project Type**: desktop/mobile app (Tauri)

**Performance Goals**: one extra model call only for a checked final action, at most 3 s (SC-007); none otherwise

**Constraints**: 10 GB cargo scope with `-j 1`; wording tuned on the chain set only; the everyday set and scoring
unchanged; no area word in model-facing text (087 FR-019); hosted models do not route and are not checked

**Scale/Scope**: chain set 69 requests (six ordinary deletes, two for good), everyday set 106, four local models,
Thunderbird name run

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                       | Result |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | No secrets involved                                                                         | Pass   |
| II No local absolute paths                              | The eval takes paths from env; reports contain none                                        | Pass   |
| III Device-independent identity                         | Not touched                                                                                 | Pass   |
| IV Pinned cross-repo references                         | haex-mail stays the pinned fixture; nothing changes in it                                   | Pass   |
| V External sources opt-in                               | No harness content                                                                          | Pass   |
| VI Self-modifying instructions review-gated             | The check question and the answer change in reviewed PRs; no agent instruction file changes | Pass   |
| VII Relay never blocks local work                       | Not touched                                                                                 | Pass   |
| VIII No concealment instructions                        | The answer tells the model not to claim what did not run; nothing is hidden from the user   | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch, one PR for the spec, one for the code                                     | Pass   |
| spaex: 500 LoC per file                                 | New `chat/tools/reversible.rs`; `eval/everyday/runner.rs` (493) split first (R7)            | Pass   |
| spaex: graphify first                                   | `graphify query` before each new function                                                   | Pass   |
| spaex: tests in separate files                          | `reversible_tests.rs`, `plan_call_tests.rs` next to the modules                             | Pass   |

Checked against `.specify/memory/constitution.md` and `.spaex/constitution.md`. Re-check after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/088-reversible-delete/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── check-question.md
│   └── report-checks.md
├── checklists/requirements.md
├── tasks.md
└── results.md            # written while measuring
```

### Source Code

```text
src-tauri/src/chat/
├── tools/
│   ├── reversible.rs          # NEW: candidates, question, parse, check(), answer text
│   └── reversible_tests.rs    # NEW
├── turn/
│   ├── mod.rs                 # TurnRunner gains the set of tools checked this turn
│   ├── plan_calls.rs          # runs the check after the guard, before decide_call
│   ├── plan_calls_tests.rs
│   ├── guard.rs               # Failure::NotFinal and its next step
│   └── guard_tests.rs
└── eval/everyday/
    ├── runner.rs              # per-call planning moved out (493 lines)
    ├── plan_call.rs           # NEW: guard, check, played dialog for one call
    ├── plan_call_tests.rs     # NEW
    ├── report.rs              # checks per request, turnedToReversible per run
    └── report_tests.rs
```

**Structure Decision**: the shared logic sits in `chat/tools/reversible.rs` next to `route.rs`, whose request shape
and `RouteCall` it reuses; the app (`plan_calls.rs`) and the measurement (`plan_call.rs`) call the same function, as
spec 087 requires.

## Delivery order

1. Split `eval/everyday/runner.rs` (no behaviour change; all everyday tests green).
2. `reversible.rs` with unit tests (candidates, question, parse, answer).
3. App: `plan_calls.rs` + `turn/mod.rs` + `guard.rs`.
4. Measurement: `plan_call.rs` + report fields.
5. Chain set (Qwen3.5 4B, Gemma 4 E4B, also Thunderbird); tune wording there only; keep or drop.
6. If kept: everyday set for the four local models; results.md; spec 087 SC-007 noted as replaced.

## Complexity Tracking

No violations.
