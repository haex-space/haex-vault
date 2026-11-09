# Implementation Plan: Small local models finish what they start

**Branch**: `087-agent-chains` | **Date**: 2026-11-06 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/087-agent-chains/spec.md`

## Summary

Spec 082 routes a request to its area; under the app's tool names Qwen3.5 4B passes 78 and Gemma 4 E4B 77 of the
everyday set's 106 requests, short of 80. The failures behind the routing sort into five levers (research R1): asking
or searching for tools although the area's tools are offered, deleting for good instead of into the trash, a day
without its end, a chain that stops after a successful search, and values in the wrong form. The plan first makes
the report say where each request was routed and adds a chain development set, takes a new baseline on the current
app, and then measures one lever at a time: a deterministic value turning before the guard, a clock line with the
day's bounds, holzi-side wording for final tools and the offer's order, and wording for looking up and finishing a
chain, each tuned on the chain set and kept by its everyday numbers. After the baseline the operator asked for
general levers instead of mail levers (spec Decision 7): the measuring world first answers every reading tool
truthfully (R12) and the chain set covers every area (R13), with a new baseline; requests that name only a person or
a thing are routed to every area that may hold them (R11).

## Technical Context

**Language/Version**: Rust (edition 2021, MSRV per `src-tauri/Cargo.toml`), TypeScript for the action exports

**Primary Dependencies**: holzi's chat turn (`chat/turn`), tool offer and routing (`chat/tools`), the everyday
measurement (`chat/eval/everyday`), llama.cpp for local models (CUDA build per spec 078/082 quickstart)

**Storage**: none new; reports and run files under `specs/087-agent-chains/runs/`

**Testing**: `cargo test` lib tests in `*_tests.rs` (with `--config 'profile.dev.package.holzi.debug=0'`), the
`chat_tool_loop_*` and `action_*` integration tests, the everyday and chain sets as model measurements

**Target Platform**: desktop (Linux reference laptop, RTX A2000 8 GB) and phone profile in the measurement

**Project Type**: desktop/mobile app (Tauri)

**Performance Goals**: no new model calls per turn; the clock line and tool wording add at most a few dozen prompt
tokens

**Constraints**: 10 GB cargo scope with `-j 1`; wording tuned on the chain set and the routing development set only;
haex-mail stays the pinned fixture; hosted models do not route

**Scale/Scope**: 106 everyday requests, about 55 chain-set requests from every area, 4 local and 3 hosted models

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                                | Result |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | No secrets involved                                                                                                  | Pass   |
| II No local absolute paths                              | Scripts and the eval take paths as arguments or env; reports contain none                                            | Pass   |
| III Device-independent identity                         | Not touched                                                                                                          | Pass   |
| IV Pinned cross-repo references                         | haex-mail stays pinned as in spec 078                                                                                | Pass   |
| V External sources opt-in                               | No harness content                                                                                                   | Pass   |
| VI Self-modifying instructions review-gated             | The model instruction and tool wording change in reviewed PRs; no agent instruction file changes                     | Pass   |
| VII Relay never blocks local work                       | Not touched                                                                                                          | Pass   |
| VIII No concealment instructions                        | The wording tells the model to act and to show; it hides nothing from the user, and the dialog shows the values used | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch per step group, each with a PR                                                                      | Pass   |
| spaex: 500 LoC per file                                 | New modules for value turning and the offer's order; `runner.rs` (473) only gains a call (R10)                       | Pass   |
| spaex: graphify first                                   | `graphify query` before each new function                                                                            | Pass   |
| spaex: tests in separate files                          | `*_tests.rs` next to each module                                                                                     | Pass   |

Checked against `.specify/memory/constitution.md` and `.spaex/constitution.md`. Re-check after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/087-agent-chains/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── report-routing.md     # per-request routing record in the everyday report
│   ├── chain-set.md          # the chain development set and HAEX_EVAL_SET
│   └── value-forms.md        # which values holzi turns and how
├── analysis/
│   └── chainset.json         # the chain development set (R3)
├── runs/                     # baseline/, chain/<step>/, steps/<step>/, final/
├── results.md
└── tasks.md                  # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/chat/
├── eval/everyday/
│   ├── report.rs             # per-request routed domains and first offer (R2)
│   ├── runner.rs             # hands the routing record to the report
│   └── mod.rs                # load a set from HAEX_EVAL_SET (R3)
├── turn/
│   ├── arg_forms.rs          # new: turn values into the schema's type (R5)
│   └── plan_calls.rs         # calls arg_forms before the guard
├── tools/
│   ├── prompt.rs             # clock line with the day's bounds (R4); instruction sentences (R6.1, R7.1)
│   ├── route.rs              # routing prompt: a person or a thing goes to every area that may hold it (R11.1)
│   ├── offer.rs              # final tools after reversible ones in a routed offer (R8.1)
│   ├── final_tools.rs        # new if needed: the sentence holzi adds to final tools (R8.2)
│   └── find_actions.rs       # already offered tools named in the result (R6.2)
src-tauri/tests/model_tool_eval.rs   # HAEX_EVAL_SET
```

## Delivery order

1. **Measurement** (US1): routing record per request (R2), chain set and `HAEX_EVAL_SET` (R3); after #903 and #904
   (a local model's context, spec 086) are on `main`, the baseline for every model on the everyday set and for the target models on the chain
   set.
2. **Value forms** (US4, R5): deterministic, measured once on the everyday set (target and phone models, hosted).
   **Then the world and the chain set across areas** (R12, R13), and a new baseline for every model on the everyday
   set and for the target models on the chain set; every later step is compared with it.
3. **The day's end** (R4): clock line; chain set, then everyday.
4. **Deleting into the trash** (US5, R8): sentence for final tools, then order; chain set, then everyday, also under
   another extension name (FR-011).
5. **Looking up first** (US2, R6): instruction, then the tool search's answer, then (only if needed) no tool search in
   a routed first step; chain set, then everyday. Then **a person or a thing, not an area** (R11): the routing prompt
   and holzi's domain descriptions tuned on the routing development sets of every area, and the sentence to look in
   the next offered area; chain set, then everyday.
6. **Finishing a chain** (US3, R7): instruction, then the next step after a successful result; chain set, then
   everyday.
7. **Final** (US6): all models of the baseline, the target models also under another extension name; `results.md`.

Each step is a commit group of its own with its run files; a step that makes a target model or a hosted model worse
than the baseline is dropped or reworked before the next one.

## Complexity Tracking

No constitution violations to justify.
