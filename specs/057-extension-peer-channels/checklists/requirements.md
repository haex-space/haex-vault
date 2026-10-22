# Specification Quality Checklist: Kanäle zwischen Erweiterungen auf verschiedenen Vaults

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-22
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

- The two operator decisions of 2026-10-22 are recorded under Clarifications and applied: invitations
  are accepted only in holzi's own invitation prompt (FR-014 to FR-016, US1, SC-006), and the first
  version reaches a contact's devices through the addresses on the stored business card, reporting
  "unreachable" with a hint to share a current card, and switches to knocking once spec 056 has it
  (FR-011, FR-012, FR-018, SC-001, Assumptions). No marker remains.
- The permission name `peers`/`connect` (FR-001) and the limits 4 KiB, 64 KiB, 1,000 bytes, 128 bits,
  5 minutes, 5 seconds and 30 seconds are named on purpose: they are the agreed contract with the
  consuming extension (haex-doom), not implementation choices. How they are carried (transport,
  framing, codes) is left to the plan.
- Defaults taken instead of markers are listed under Assumptions: devices of the own vault count as
  confirmed, the permission is vault-wide and may come from the manifest, the inviter learns
  "unreachable" without a reason when the invitee does not list it as a contact or blocked it, the
  last frame closing ends the extension's channels as with the shell, knocking comes from spec 056,
  and admitting non-contacts for public lobbies needs a new ADR beyond ADR-0015.
