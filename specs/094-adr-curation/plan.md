# Implementation Plan: ADR-Korpus bereinigen

**Branch**: `docs/adr-curation` | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/094-adr-curation/spec.md`

## Summary

Prüfe alle 26 ADR-Dateien samt eingehender Verweise. Nur ADRs, deren Entscheidung und nötige Begründung vollständig in Specs enthalten sind und die keine Verfassungsregel erfüllen, werden entfernt. Die Erstprüfung findet ADR 0022 als einzigen klaren Entfernungskandidaten: Spec 063 enthält die Windows-Ausnahme, Umsetzung und Spike-Ergebnisse bereits. Die restlichen ADRs bleiben vorerst als eigenständige, übergreifende Entscheidungen, Sicherheitsbegründungen oder wegen ihrer aktiven Spec-Verweise erhalten. Der HXEF-Formatvertrag samt Testvektoren bleibt bestehen.

## Technical Context

**Language/Version**: Markdown

**Primary Dependencies**: Git, lokale Markdown-Dateien

**Storage**: versionierte Markdown-Artefakte

**Testing**: keine Laufzeittests; statische Prüfung von Diff und internen Dateiverweisen

**Target Platform**: Git-Repository

**Project Type**: Dokumentationspflege

**Performance Goals**: nicht anwendbar

**Constraints**: Alle relevanten Inhalte und Links erhalten; Specs bleiben normativ; `.spaex/constitution.md` verlangt ADRs für Entscheidungen mit wesentlichem Einfluss auf Kernprinzipien; keine Änderungen an der Verfassung im Umfang dieser Arbeit.

**Scale/Scope**: 26 ADR-Dateien, davon 1 Entfernungskandidat; 25 beizubehalten; zusätzlich Spec 063 und alle eingehenden Verweise prüfen.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                          | Befund                                                                                                                                                                     |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Keine Geheimnisse in Git                                       | Erfüllt; nur vorhandene Textartefakte werden geprüft oder geändert.                                                                                                        |
| Keine lokalen absoluten Pfade in versionierten Konfigurationen | Erfüllt; Artefakte nutzen projekt-relative Links.                                                                                                                          |
| Geräteunabhängige Projektidentität                             | Nicht berührt.                                                                                                                                                             |
| Externe Referenzen auf unveränderliche Revisionen pinnen       | Keine externen Referenzen werden eingeführt.                                                                                                                               |
| Externe Quellen nur nach Projektfreigabe                       | Keine externen Quellen.                                                                                                                                                    |
| Selbständernde Anweisungen sind review-gated                   | Keine Anweisungen, Verfassungen oder Berechtigungen werden geändert.                                                                                                       |
| Relay-Ausfall blockiert lokale Arbeit nicht                    | Nicht berührt; alle Quellen sind lokal.                                                                                                                                    |
| ADR-Pflicht für Entscheidungen mit Kernprinzip-Einfluss        | Erfüllt; nur ADR 0022 wird zur Löschung vorgeschlagen, da die Mailto-Registrierungsgrenze keine Kernprinzip-Entscheidung ist. Prinzipienrelevante ADRs bleiben auffindbar. |
| Eigener Worktree, Topic-Branch und PR nach `main`              | Erfüllt; diese Arbeit liegt isoliert auf `docs/adr-curation`.                                                                                                              |
| Aktiver Speckit-Ablauf mit Review-Gates                        | Erfüllt; Spec wurde freigegeben, dieser Plan wird vor Tasks zur Prüfung vorgelegt.                                                                                         |

**Ergebnis**: GATE PASS. Nach Phase 1 erneut geprüft; keine Verfassungsänderung und kein Konflikt.

## Project Structure

### Documentation (this feature)

```text
specs/094-adr-curation/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
└── tasks.md                 # folgt nach Planfreigabe
```

### Source Documentation

```text
docs/adr/                    # Audit aller 26 Entscheidungseinträge
docs/formats/                # HXEF-Formatvertrag bleibt unangetastet
specs/063-mailto-extensions/ # normative Quelle für ADR 0022
```

**Structure Decision**: Keine neue Laufzeitstruktur. Das Inventar liegt bei der Feature-Spec; fachliche Entscheidung und Plattformgrenzen werden in Spec 063 verankert. Verweise aus abgeschlossenen Tasks bleiben als historische Spur erhalten, werden aber so angepasst, dass sie nicht auf eine entfernte Datei zeigen.

## Implementation Stages

1. **Inventar prüfen**: alle ADRs einzeln gegen Specs, Pläne, Aufgaben und Querverweise bewerten. Ergebnisse in `research.md` festhalten.
2. **Kandidaten konsolidieren**: falls der Vollabgleich ADR 0022 bestätigt, noch fehlende Konsequenzen in Spec 063 R5 ergänzen und den erledigten Task-Verweis aktualisieren.
3. **Redundanz entfernen**: ADR 0022 löschen, sofern Spec 063 danach alle bleibenden Entscheidungen und Gründe abdeckt. Bei jedem weiteren Kandidaten denselben Nachweis erbringen; andernfalls beibehalten.
4. **Links bereinigen**: alle Links auf gelöschte Dateien aktualisieren; Formatverträge und ADRs mit eigener Geltung erhalten.
5. **Statische Prüfung**: nach gelöschten Dateipfaden suchen, die Datei-Links auflösen und `git diff --check` ausführen.

## Complexity Tracking

Keine Abweichungen von der Constitution.
