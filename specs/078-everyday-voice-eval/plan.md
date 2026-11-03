# Implementation Plan: Everyday voice agent evaluation

**Branch**: `078-everyday-voice-eval` | **Date**: 2026-11-03 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/078-everyday-voice-eval/spec.md`; the model runs of 2026-11-03
in [research.md](research.md) R1.

## Summary

A second measurement next to spec 032's: an everyday set of spoken requests, scored as whole chains against
a small fake world, with safety errors counted on their own and a phone verdict per model.

- **This delivery (US1 to US5)**: the set, the fixture world, a step-loop runner, scoring with safety
  errors, spoken twins, timing and speech fitness, and reference runs through a cloud key, an
  OpenAI-compatible server and the Claude subscription (`claude -p`).
- **Next delivery (US6, US7)**: the app reads recorded verdicts, gates the phone's local agent, and lends a
  PC's model to the phone. It waits for the new figures; lent models must pass the same gate (operator,
  2026-11-03, research R10).
- **Shape**: a new module `chat/eval/everyday/` reuses the chat's offer functions (`matched_offer`,
  `extend_offer`, `found_tools`), the real `find_actions` search and `system_prompt` with a fixed clock; it
  does not touch spec 032's runner, set or self-test.

## Technical Context

**Language/Version**: Rust (edition and MSRV of `src-tauri/Cargo.toml`)

**Primary Dependencies**: existing only: `serde`/`serde_json`, `tokio`, `async-trait`, the `ProviderAdapter`
adapters (`llm::llamacpp`, `llm::local`, `adapters::anthropic`, `adapters::openai_compatible`); the `claude`
CLI for the subscription reference (not a crate dependency).

**Storage**: embedded JSON (`include_str!`); reports under `src-tauri/target/eval/`, not versioned.

**Testing**: `cargo test --lib` for loading, scoring and planted safety errors (no model);
`tests/model_tool_eval.rs` `measure_everyday_voice` (`#[ignore]`) for a real model.

**Target Platform**: the measuring desktop (Linux, macOS, Windows); nothing ships to the app in this
delivery.

**Project Type**: desktop app (Tauri); this feature is a test-side measurement inside `src-tauri`.

**Performance Goals**: a full run of a 4B model on the reference desktop with GPU in at most 10 minutes
(SC-005; the 83-sentence set took 167 s).

**Constraints**: deterministic for local models (greedy, fixed clock, fixed world); no real data read and
no action run (FR-005); no key in reports or logs (FR-012); builds within 10 GB (the CUDA unit tests within 12 GB) on the
measuring machine (`-j 1`, `CMAKE_BUILD_PARALLEL_LEVEL=2`).

**Scale/Scope**: at least 80 requests (about 60 clean, 30 spoken), 11 categories, 2 languages; 15 haex-mail
tools and the existing core tools.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                                                                                                          | Result |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | Keys only from the environment; `claude-cli` uses the CLI's own login and copies no credentials; the world holds no secret values; reports are unversioned and key-free                        | Pass   |
| II No local absolute paths                              | Quickstart and contracts use repository-relative paths and placeholders (`<models>`, `<scratch>`); the CUDA recipe names no machine path                                                       | Pass   |
| III Device-independent identity                         | Not touched                                                                                                                                                                                    | Pass   |
| IV Pinned cross-repo references                         | haex-mail tools copied from `haex-space/haextension@39c499801157b8e71a68a198a27591ce1a581769:apps/haex-mail/haextension/manifest.json`, the reference stored in the file and checked by a test | Pass   |
| V External sources opt-in                               | No harness content inherited                                                                                                                                                                   | Pass   |
| VI Self-modifying instructions review-gated             | No skill, prompt or permission changes; the agent's instruction stays as it is (spec Assumptions)                                                                                              | Pass   |
| VII Relay never blocks local work                       | The measurement needs no relay; local models run offline                                                                                                                                       | Pass   |
| VIII No concealment instructions                        | Not touched                                                                                                                                                                                    | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch, PR per delivery                                                                                                                                                              | Pass   |
| Phase sequencing                                        | A measurement of an existing agent; the app behaviour (US6, US7) is deferred until its figures exist                                                                                           | Pass   |

No ADR: no decision touches a principle. Re-check after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/078-everyday-voice-eval/
├── plan.md              # This file
├── research.md          # Phase 0: R1–R10
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   └── everyday-format.md
├── eval-results.md      # Recorded verdicts (written by the implementation)
└── tasks.md             # Phase 2 ($speckit-tasks)
```

### Source Code (repository root)

```text
src-tauri/src/chat/eval/
├── mod.rs                      # + pub mod everyday;
└── everyday/
    ├── mod.rs                  # embedded set/world/mail tools, loading and validation
    ├── set.json                # the requests (contracts/everyday-format.md)
    ├── world.json              # the fixture world
    ├── mail_tools.json         # haex-mail tools at the pinned revision
    ├── world.rs                # tool handlers answering from the world
    ├── runner.rs               # step loop, offer per step, timing
    ├── scoring.rs              # expectations, chains, arg matchers, safety errors, speech fitness
    ├── report.rs               # rates, verdict, report JSON
    └── *_tests.rs              # one test file per module (repo convention)

src-tauri/tests/
├── model_tool_eval.rs          # + test measure_everyday_voice; providers openai and claude-cli
└── support/claude_cli.rs       # kept from the experiment, JSON fix (research R8)

src-tauri/src/chat/tools/
├── offer.rs, extension_offer.rs  # unchanged; already public
```

Removed from the worktree before the first commit (research R9): `llm/llamacpp/formats/lfm2.rs`,
`lfm2_tests.rs`, their registration in `formats/mod.rs`, the `{% generation %}` stripping in
`llm/llamacpp/template.rs`, and `HOLZI_LLAMACPP_CPU_OVERRIDE` in `llm/llamacpp/model.rs`.

**Structure Decision**: a sibling module of spec 032's eval inside `chat/eval/`, so it reuses the offer and
prompt functions through the crate and keeps spec 032's set, runner and in-app self-test unchanged.

## Complexity Tracking

No violations.
