# Tasks: ADR-Korpus bereinigen

**Input**: Design documents from `/specs/094-adr-curation/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md

**Tests**: Keine Produkttests angefordert. Die Dokumentationsänderung wird mit statischen Link- und Diff-Prüfungen kontrolliert.

## Phase 1: Setup

- [x] T001 Prüfe, dass `docs/adr/` vor der Bereinigung genau die 26 im Inventar in `specs/094-adr-curation/research.md` aufgeführten Dateien enthält und ergänze pro ADR die Zähler für eingehende Verweise.

## Phase 2: Foundational

- [x] T002 Bestätige für jeden Inventareintrag in `specs/094-adr-curation/research.md` die Spec-Abdeckung, einzigartige Begründung, Verfassungsregel und Disposition anhand der Kriterien in `specs/094-adr-curation/data-model.md`.

## Phase 3: User Story 1 — Eine klare Quelle für Produktverhalten (Priority: P1) — MVP

**Goal**: Vollständig redundante ADRs werden in Specs konsolidiert oder entfernt, ohne eigenständige Architekturentscheidungen zu verlieren.

**Independent Test**: Für jede entfernte ADR sind Entscheidung, noch relevante Begründung und Folgen in einer verbleibenden Spec auffindbar; keine Verweise zeigen auf eine fehlende Datei.

- [x] T003 [US1] Vergleiche alle noch nicht übernommenen Folgen aus ADR 0022 mit R5 in `specs/063-mailto-extensions/research.md`; ergänze dort die Prüfung neuer Windows-Schemata und die erneute Prüfung der Bundler-Annahme bei einem Upstream-Upgrade, falls sie nicht schon enthalten sind.
- [x] T004 [US1] Aktualisiere T026 in `specs/063-mailto-extensions/tasks.md`: erhalte den historischen Grund für ADR 0022, verweise auf `specs/063-mailto-extensions/research.md` R5 und entferne den Verweis auf den ADR-Dateipfad.
- [x] T005 [US1] Entferne die Datei zu ADR 0022, nachdem `rg` im versionierten Repository keine weiteren Verweise auf ihren Pfad oder ADR-Inhalt außerhalb von `specs/094-adr-curation/` findet.
- [x] T006 [US1] Aktualisiere die Disposition und Begründung für ADR 0022 in `specs/094-adr-curation/research.md` auf „entfernt, konsolidiert in Spec 063 R5“ und bestätige für alle übrigen ADRs, dass sie im Korpus verbleiben.

## Phase 4: User Story 2 — Technische Verträge bleiben auffindbar (Priority: P1)

**Goal**: Eigenständige Formatverträge und ihre Verweise bleiben von der ADR-Bereinigung unberührt.

**Independent Test**: HXEF-Formatdokument, Testvektoren und die Verweise aus Specs existieren nach der Bereinigung weiter.

- [x] T007 [US2] Prüfe, dass `docs/formats/encrypted-folder-v1.md` und seine Testvektoren unverändert vorhanden sind und dass `specs/048-encrypted-folders/` weiterhin auf den Vertrag verweist; notiere das Ergebnis in `specs/094-adr-curation/research.md`.

## Phase 5: Polish & Cross-Cutting Concerns

- [x] T008 Suche in `README.md`, `docs/`, `plans/` und `specs/` nach Verweisen auf gelöschte ADR-Pfade und repariere alle Treffer; bestätige, dass jeder verbliebene ADR-Link zu einer vorhandenen Datei führt.
- [x] T009 Führe die Prüfungen aus `specs/094-adr-curation/quickstart.md` aus und aktualisiere dort nur dann die Inventaraussagen, wenn der tatsächliche Stand davon abweicht.

## Dependencies & Execution Order

```text
T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009
```

Alle Tasks sind sequenziell, weil die Entscheidung über Entfernung von der bestätigten Inventur, dem Inhalte-Transfer und der Linkbereinigung abhängt. Kein paralleler Task-Split ist vorgesehen.

## Implementation Strategy

MVP ist User Story 1: ADR 0022 konsolidieren, löschen und Verweise bereinigen. User Story 2 bestätigt separat, dass der Formatvertrag HXEF vollständig erhalten bleibt. Danach folgen die repository-weiten Link- und Diff-Prüfungen.
