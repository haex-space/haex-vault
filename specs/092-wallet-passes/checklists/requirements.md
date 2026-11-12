# Specification Quality Checklist: Cards and tickets in the password manager (wallet passes)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-11
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs). The table and column names in FR-005 are deliberate: spec
      034 fixes its data model by name (FR-036) and this spec extends that list; nothing names a language, framework or
      library.
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders. The Background and the user stories are plain; the Requirements follow the
      register of spec 034.
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain. One open question is listed in its own section; it is a fact to check in the
      code, not a decision, and does not block the plan.
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded (Not in scope names what the wallets do that holzi cannot, and what is left to later specs)
- [x] Dependencies and assumptions identified (Relation to existing specs, Assumptions)

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Open Question 1 (behaviour of an old device) is a fact to check in the code, not a decision; the plan resolves it.
  The other six open questions were resolved by the operator on 2026-11-11 and are in the Clarifications.
- FR-042 and the amendments of 034 FR-036 (FR-005) and 034 FR-007 (FR-022, folder names in the search) are the places where
  this spec reaches into an existing spec; the plan carries the amendments.
- The operator's wording "field-name collisions are shown" for the link did not survive the decision of one entry per pass:
  linking never copies fields (FR-016), so there is nothing to collide; duplicates of the same pass are caught instead
  (FR-031).
