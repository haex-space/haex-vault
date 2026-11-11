# Specification Quality Checklist: The reversible way first

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-11
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

- Product terms (Auto/Plan mode, routed turn, area, permission dialog) are user-facing or spec-082 concepts of holzi,
  not implementation details.
- SC-001 replaces spec 087 SC-007 by operator decision (2026-11-11).
- Model names in SC-001/SC-002/SC-004 are the measured subjects carried over from spec 087, not technology choices.
- SC-007 names a reference machine because the check costs one extra local model call.
