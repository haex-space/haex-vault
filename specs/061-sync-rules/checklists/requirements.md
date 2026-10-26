# Specification Quality Checklist: Sync-Regeln

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-26
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

- Die Spec nennt `.holzi-trash` und die Prüfsumme als sichtbares Verhalten (Name des Papierkorb-Ordners,
  Kontrolle nach dem Kopieren); Formate und Module bleiben Sache des Plans.
- Geklärt 2026-10-26: verschlüsselte Ordner (048) liefern Größe, Änderungszeit und Klartext-Prüfsumme
  je Datei aus der Sidecar-Datei. Offen für den Plan: Eine Datei, die in einen verschlüsselten Ordner
  geschrieben wird, bekommt die Schreibzeit als Änderungszeit; ob eine Quellzeit mitgegeben werden
  soll, ist eine Planfrage (FR-016 verlangt es nur, wo der Ort es erlaubt).
- SC-007 (10 000 Dateien unter 5 Sekunden) ist ein Richtwert des Autors, nicht vom Betreiber vorgegeben;
  im Plan gegenprüfen.
