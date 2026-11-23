# Specification Quality Checklist: Umbenennung von holzi in haex-vault

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-16
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

- Die Spec nennt Namen von Kennungen (Crate, Typ, Schema, Protokollnamen), weil sie der Inhalt der
  Umbenennung sind, keine Umsetzungsentscheidung. Wie die Änderung technisch geschnitten wird,
  gehört in die Planung.
- Der einzige Marker (Datenordner der alten App `space.haex.vault` auf demselben Gerät) ist am
  2026-10-16 vom Betreiber beantwortet: Ordner von Hand löschen, keine Code-Logik.

## Nachweise zu den Erfolgskriterien

- [x] SC-001: `python3 scripts/ci/check-old-name.py` liefert 0 Treffer außerhalb der
  Ausnahmeliste (6957 Dateien geprüft, 277 erlaubte Treffer); T063 belegt das Scheitern eines
  Test-PRs, der den alten Namen wieder einführt.
- [ ] SC-002 teilweise belegt: [Main-CI-Lauf 45088891620](https://github.com/haex-space/haex-vault/actions/runs/45088891620)
  auf `558d1d6c` ist vollständig grün. Desktop- und Android-E2E sowie Android-Build und alle
  weiteren in diesem Main-Lauf konfigurierten Prüfungen bestanden. `.github/workflows/ci.yml`
  konfiguriert keinen iOS-Build, den SC-002 ausdrücklich verlangt.
- [x] SC-003: T067/T068 und PR #1099 belegen das Öffnen der alten Vault-Kopie und den Abgleich
  der Eintragszahl.
- [x] SC-004: T035 und die Mehrgeräte-Szenarien im Main-E2E belegen die Synchronisation; die
  vollständigen Desktop- und Android-E2E-Shards liefen in Main-CI-Lauf 45088891620 grün.
- [x] SC-005: T011 bis T014 belegen die eindeutige Kennzeichnung alter App-Verweise.
- [x] SC-006: T088 belegt Weiterleitung, Archivierung und Verweis des Legacy-Repos.
- [x] SC-007: T071 bis T078 belegen die aktualisierten Geschwister-Repos und deren Prüfung;
  atoms hat keine CI.
