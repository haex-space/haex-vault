# Specification Quality Checklist: The header bar belongs to the app

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-11
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs) — control catalog and message names are product-level; the plan picks components and numbers
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders — repo specs name existing specs and actions; kept to that level
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded (pilots: Settings, file browser; tabs: model stays, strip out)
- [x] Dependencies and assumptions identified (vault SDK change, tab follow-up spec)

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Known accepted gap (Decision 2, FR-029): no visible way to switch tabs by hand until the follow-up spec.
- Spec number 090: 088 is taken (PR #943), 089 is reserved for the topics-over-sources spec.
- SC-008 is a by-hand check by the operator.
