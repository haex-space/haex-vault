# Specification Quality Checklist: Dateisync zwischen eigenen Geräten

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-28
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

- The operator resolved FR-002 in favor of a user-chosen file-system folder on each device. The
  earlier alternative of a holzi-managed store is retained only in the clarification history; the
  requirements and assumptions below use the chosen folder model.
- iroh and iroh-blobs are named only in the Input line and the Assumptions, as an operator
  constraint. The FRs use the shared glossary (Dateiindex, Objekt, Konfliktkopie, Bereich,
  Änderungspaket) and the field names `created_by`/`modified_by` from the design doc, which are
  domain terms shared with specs 027 and 028, not an implementation choice.
- Operator decisions D1, D2, D8, D10, D11, D12, D13 and design §7 are recorded as Q → A in the
  Clarifications, dated by the design's decision table.
- The haex-vault 2.x (Legacy) reference in the Assumptions is pinned as repository + full SHA + path; it is a
  model to learn from, not a requirement.
- Plan material, deliberately left open: how objects are chunked and encrypted, whether a device
  keeps encrypted copies for serving or derives them from the local file (FR-020, FR-021, FR-047),
  the size of the free-space reserve and the settle time for files being written (FR-010, FR-045),
  the exact date/time format inside the conflict-copy name and the rule for which version keeps the
  name (FR-033, FR-034). The name pattern itself is fixed:
  `<Name> (Konflikt <Gerätename> <Datum Uhrzeit>).<Endung>`.
- Cross-spec alignment (coordinator resolutions): the Dateiindex including per-file keys may travel
  through the vault's own Relay-Postfach as ordinary vault data; only the Vault-Identität private
  key is direct-only (FR-013, FR-022, D30); folders are managed in „Föderation“ → Unteransicht „Ordner“; objects
  of own folders on backend A are specified in Spec 026, backend B in Spec 029 User Story 6;
  FR-048 uses the Space flows without an own Bereich; pausing (FR-044) stops local scanning, writing
  files to disk and object transfer; Spec 024's data sync continues, incoming Dateiindex changes are
  applied atomically with their Änderungspaket and materialized on disk after resume. No new FRs were added.
- Edit-versus-delete keeps the edited version (FR-030). This is stricter than "deleted files do not
  come back" and follows the zero-data-loss priority of D10; a deletion of an unchanged file never
  resurrects (FR-029).
- SC-001 and SC-009 depend on hardware and network; they are checked in a manual quickstart run on
  a local network and a laptop with SSD.

- 2026-09-28: the operator answered all clarification questions for this spec; the decision is
  recorded under Clarifications.
- 2026-09-28 (D26, D27, D30): terminology moved to the Geräteliste of spec 024 (Begriffe,
  FR-015, Beziehung zu Spec 024, Edge Cases, Assumptions); no rotation of the Vault-Identität; a
  device removed from the Geräteliste gets and serves no objects and cannot decrypt new Dateiindex
  entries; D8 and the D13 remark on the Vault key marked as superseded in the Clarifications.
