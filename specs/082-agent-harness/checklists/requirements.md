# Specification Quality Checklist: A harness that lets small local models do everyday tasks

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

- The three scope questions (what "no safety error" means, the goal, the phone model) were decided by the operator
  before writing (spec Decisions), so the spec has no open markers.
- The spec names holzi's own concepts (tools, the tool search, the window tools, the permission dialog, the
  everyday set): they are the product's vocabulary, not an implementation choice. How domains are recognised
  (FR-006) is left to the plan.
- The background figures come from the reports of spec 078's runs on 2026-11-04 and a probe of the first step's
  offer for all 106 requests; the plan's research records the method.
