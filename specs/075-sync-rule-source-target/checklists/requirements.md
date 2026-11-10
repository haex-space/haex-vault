# Specification Quality Checklist: Name the two places of a sync rule "source" and "target" in code

**Purpose**: Validate specification completeness and quality before proceeding to planning

**Created**: 2026-11-01

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

- This is a rename spec, so the old and new identifiers are the subject itself and appear by name; that is
  the one place where "implementation details" are unavoidable.
- Settled with the operator on 2026-11-01: `source`/`target` on the rule, the inner field renamed, own spec
  before 074.
- Open: the new name `location` (Assumptions) and the migration for existing vaults (FR-004), against the
  "no migration paths" rule for holzi. Confirm at `/speckit-plan`.
