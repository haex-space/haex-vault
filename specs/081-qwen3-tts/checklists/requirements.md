# Specification Quality Checklist: Qwen3-TTS as haex's natural voice

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-04
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- The spec names llama.cpp, GGUF and CUDA/Metal on purpose: running Qwen3-TTS inside the engine haex
  already ships (spec 070) is the operator's constraint, not a design choice left to the plan. The
  requirements themselves stay behavioural (FR-002 forbids a second engine; how the engine is called is
  for the plan).
- Defaults chosen without a clarification marker, open for the operator to override:
  Qwen3-TTS stays selectable on devices where it is slower than real time (US3); the default is the
  best-sounding model that keeps up (FR-012); the local chat model has priority on the graphics card
  (FR-009); long sentences are split into parts of at most 20 words and failed parts are generated again (FR-010, FR-010a, operator: never cut); Qwen3-TTS 0.6B and replacing sherpa-onnx for Pocket are follow-ups (Assumptions).
