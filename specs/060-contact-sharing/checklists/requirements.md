# Specification Quality Checklist: Kontakte teilen und der Kontakt „Ich“

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

- Die Textpräfixe `holzi:card:` und `npub` stehen in der Spec, weil sie bestehende, für den Nutzer
  sichtbare Formate benennen (Spec 051), keine neue Technik.
- Offen für den Plan, bewusst nicht in der Spec entschieden: die Höchstgröße für Link und Zwischenablage
  (FR-019), die genaue Form der festen Kennung des Ich-Kontakts (FR-001), ob Migrations-Inserts vom CRDT
  erfasst werden und welche Fallbacks das gegebenenfalls braucht (FR-001), das Textpräfix des neuen
  Formats (FR-018).
- Ebenfalls offen für den Plan: wie die bestätigten holzi-Adressen eines Kontakts gespeichert werden,
  wenn eine Weitergabe neue Geräte oder Relays bringt, ohne bisherige zu entfernen (FR-025; die
  unterschriebene Karte lässt sich nicht verändern), und ob die Aktion zum Teilen für Agenten in Spec
  020 als `destructive` ohne dauerhafte Erlaubnis abgebildet werden kann (FR-029, FR-030).
- Bewusst nicht Teil dieser Spec: das Auffrischen der Adressen eines Kontakts aus dem Kontakt selbst beim
  Verbinden (Spec 051 FR-012, Weg „Geräteliste“). `foreign_auth.rs` prüft die vorgelegte Liste nur und
  schreibt sie nicht in die Adressen des Kontakts; ein eigener Spec- oder Aufgabenpunkt.
- Plan-Vorgabe zu FR-025: Hinzugefügte Adressen dienen nur zum Anwählen. Die Generation der neuesten
  gespeicherten Karte bleibt der Schwellwert, nach dem eine vorgelegte Geräteliste als zu alt abgewiesen
  wird (`foreign_trust`), damit eine ältere Karte mit einem entfernten Gerät ihn nicht senkt. Gewählt
  (Betreiber, 2026-10-26): mehrere unveränderte Karten je Vault, die Adressen sind die Vereinigung beim
  Lesen. Primärschlüssel der Kartentabelle
  ist der Hash des unterschriebenen Inhalts (die kanonischen Bytes ohne Signatur, wie
  `haex_contacts_binaries.hash` den Hash als natürlichen Schlüssel nutzt); er enthält den Vault-Schlüssel
  schon, und dieselbe Karte ergibt auf zwei Geräten dieselbe Zeile. `vault_pubkey` bleibt als indizierte
  Spalte daneben, um die Karten einer Vault zu finden. Nicht `issued_at`: Karten, die beim Teilen
  frisch unterschrieben werden, können in derselben Sekunde mit verschiedener Auswahl entstehen.
- Plan-Vorgabe zu FR-025 und FR-025a (Betreiber, 2026-10-26): Alle Angaben, auch holzi-Adressen, lassen
  sich beim Einlesen ablehnen. Bei holzi-Adressen gilt die Wahl je Karte, weil die unterschriebene Karte
  unteilbar ist; eine Ablehnung einzelner Geräte oder Relays bräuchte eine Ausschlussliste und ist nicht
  Teil dieser Spec. Geprüft wird vor dem Speichern und bei jeder Verwendung: `format::from_bytes` prüft
  Format, Grenzen und Unterschrift schon für Text und gespeicherte Bytes; neu sind die Prüfung der
  Kennung (Hash des unterschriebenen Inhalts, von holzi berechnet, nicht übertragen) und dass
  `vault_pubkey` der Zeile der Vault in der verifizierten Karte entspricht, damit eine Karte nie unter
  einer fremden Vault geführt wird.
