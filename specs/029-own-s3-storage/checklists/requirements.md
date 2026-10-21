# Specification Quality Checklist: Eigener S3-Speicher für Spaces

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

- FR-008 is resolved: RustFS and AWS S3 are the verified and tested v1 providers.
  Cloudflare R2, Backblaze B2, Hetzner Object Storage and further providers remain
  outside v1 until they pass the suitability checks in FR-004.
- "No implementation details": S3, buckets and versioning are named because
  they are the feature itself (operator instruction).
  Wire formats, the provider admin APIs and the envelope format stay plan
  material.
- Decision taken without a marker: moving a space between backend A and B is in
  scope as P3 (User Story 8), justified in the Clarifications. Cost/quota display
  is an optional KANN requirement (FR-036).
- Decisions taken as assumptions rather than markers: default retention of old
  versions is 30 days (FR-011); tokens exist per capability type, not per member, as in
  the design; members with the delete capability get the read-write token
  because S3 does not separate writing from deleting.
- FR-017 and FR-033 (revised 2026-09-28, D30/D32): the admin's full provider
  credentials and the access tokens live in the Passwortmanager ([spec 034](../../034-password-manager/spec.md)) as ordinary vault data; they sync to all own devices, also through the
  vault's own mailbox, and are recoverable. They never reach members (except
  the matching access token in its envelope), spaces, data shares or logs.
- Cross-spec alignment (2026-09-28): the space mailbox stays on the admin's
  relay independent of backend B, and without a relay it syncs only directly;
  there is no default backend, the choice at space creation is mandatory (spec
  027 FR-003); settings live in „Föderation“ (space detail view under „Spaces“,
  own folders under „Ordner“); where saved storage connections are managed is
  left to the plan. New FR-040: an admin device removes unreferenced objects
  from backend B with the admin credentials.
- FR-015 refines the design's "inside the encrypted space data": the tokens
  travel with the space data but in per-recipient envelopes, so a read-only
  member cannot decrypt the read-write token.
- SC-002 (old token rejected within 5 minutes) depends on the provider's own
  revocation latency; the suitability check measures it with a test key
  (FR-004 g), so SC-002 holds for every provider marked „geeignet“. Review fix
  2026-09-28: FR-018 revokes at the provider before the new member list is
  published, FR-022 publishes anyway on failure and retries with a persistent
  warning.
- Aligned with the sibling spec 027 as written: 027 excludes dissolving a space
  and switching a space's backend. This spec lifts the switching exclusion (User
  Story 8, also from "nur direkte Übertragung" to B) and keeps dissolving out of
  scope. User Story 7 therefore covers giving up a bucket (after a move, or when
  the vault's own folders stop using it) and removing a storage connection;
  FR-034 also applies once a later spec introduces dissolving a space.

- 2026-09-28: the operator answered all clarification questions for this spec; the decision is
  recorded under Clarifications.
- D25 (2026-09-28): providers that fail any criterion of FR-004 cannot be
  connected for a space; the former fallback path via an admin device and
  short-lived provider-signed access is removed. The suitability check has two
  outcomes (FR-005). Members use their scoped token directly (FR-023), admin
  devices use the admin credentials (FR-024), garbage collection runs on an
  admin device (FR-040). User Story 5, FR-023 to FR-025 and SC-008 were
  rewritten for this.
