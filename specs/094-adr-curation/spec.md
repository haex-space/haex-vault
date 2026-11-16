# Feature Specification: ADR-Korpus bereinigen

**Feature Branch**: `094-adr-curation`

**Created**: 2026-11-16

**Status**: Draft

**Input**: User request: „dann los“ — alle ADRs darauf prüfen, ob ihre Entscheidungen samt nötiger Begründung bereits in Specs festgehalten sind, und redundante ADR-Inhalte konsolidieren oder entfernen.

## Scope

Die Bereinigung prüft alle 26 vorhandenen ADR-Dateien. Sie verschiebt keine normative Anforderung aus einer Spec zurück in eine ADR. Jede ADR wird einzeln bewertet: eigenständige Entscheidung und Begründung, vollständige Abdeckung in Specs, eingehende Verweise sowie Anforderungen aus den Projektverfassungen. Wo eine Verfassungsregel die ADR-Aufzeichnung verlangt, bleibt mindestens ein knapper Entscheidungsvermerk mit Verweis auf die normative Spec erhalten; doppelte Anforderungen und Details werden aus ihm entfernt.

## User Scenarios & Testing

### User Story 1 - Eine klare Quelle für Produktverhalten (Priority: P1)

Als Maintainer möchte ich Anforderungen und Verhalten in den Specs finden, ohne dieselbe normative Aussage in einer ADR doppelt pflegen zu müssen.

**Why this priority**: Doppelte Entscheidungsquellen können auseinanderlaufen und machen Änderungen schwerer auffindbar.

**Independent Test**: Für jede zur Entfernung vorgeschlagene ADR lässt sich die Entscheidung, ihre nötigen Folgen und ihre Begründung in einer benannten Spec oder einem Vertrag finden.

**Acceptance Scenarios**:

1. **Given** eine der vorhandenen ADRs, **When** das Korpus geprüft wird, **Then** dokumentiert das Inventar, welche Spec die Entscheidung trägt, welche Verweise darauf zeigen und ob die ADR eigenständige Information enthält.
2. **Given** eine ADR, die vollständig durch eine Spec abgedeckt ist und keine separate Aufzeichnungspflicht erfüllt, **When** sie entfernt wird, **Then** bleiben Entscheidung und nötige Begründung in der Spec auffindbar.
3. **Given** eine ADR mit eigenständiger, mehrere Specs verbindender Entscheidung oder einer vom Projektprinzip geforderten Aufzeichnung, **When** das Korpus bereinigt wird, **Then** bleibt die Entscheidung erhalten und ihr Inhalt wird auf nötige Begründung und Entscheidungsverweis gekürzt.
4. **Given** eine entfernte ADR mit Verweisen aus Specs, Plänen oder Tasks, **When** die Bereinigung abgeschlossen ist, **Then** zeigen diese Verweise auf die übernommene Quelle oder entfallen nachvollziehbar.

### User Story 2 - Technische Verträge bleiben auffindbar (Priority: P1)

Als Maintainer möchte ich eigenständige technische Verträge weiterhin an einer stabilen Stelle finden.

**Why this priority**: Ein Formatvertrag wie HXEF enthält konkrete Regeln und Testvektoren, die nicht allein durch das Entfernen einer ADR verschwinden dürfen.

**Independent Test**: Der verschlüsselte Ordnervertrag und alle Specs, die ihn benötigen, bleiben erreichbar und widerspruchsfrei.

**Acceptance Scenarios**:

1. **Given** ein technischer Vertrag, der nicht bloß eine ADR-Begründung ist, **When** die ADR-Liste gekürzt wird, **Then** bleibt der Vertrag bestehen und seine Links bleiben gültig.

## Edge Cases

- Eine ADR kann trotz fehlender direkter Links noch eine projektweite Entscheidung begründen.
- Eine abgeschlossene Spec kann in aktiven Specs als übergreifende Grundlage zitiert werden.
- Eine ADR kann Teile einer Entscheidung dokumentieren, während eine spätere Spec andere Teile ersetzt.
- Dateinamen können doppelte ADR-Nummern enthalten; Verweise müssen auf die richtige Entscheidung zeigen.
- Eine Spec kann eine ADR nur als Kontext nennen, ohne deren Entscheidung oder Begründung vollständig aufzunehmen.

## Requirements

### Functional Requirements

- **FR-001**: Alle vorhandenen ADR-Dateien MUST einzeln nach Inhalt, Verweisen, Spec-Abdeckung und Aufzeichnungspflicht beurteilt werden.
- **FR-002**: Eine ADR MUST nur entfernt werden, wenn eine benannte verbleibende Quelle die Entscheidung und alle weiterhin nötigen Folgen oder Begründungen vollständig abdeckt und keine Verfassungsregel eine ADR-Aufzeichnung verlangt.
- **FR-003**: Entscheidungen, die ein Kernprinzip betreffen und laut Projektverfassung eine ADR verlangen, MUST als auffindbarer Entscheidungsvermerk erhalten bleiben; ihre normativen Verhaltensdetails MUST in Specs stehen.
- **FR-004**: Verweise auf entfernte oder gekürzte ADRs MUST auf die verbleibende Quelle aktualisiert oder nachvollziehbar präzisiert werden.
- **FR-005**: Eigenständige technische Verträge und ihre Testvektoren MUST erhalten bleiben, auch wenn eine verknüpfte ADR entfernt oder gekürzt wird.
- **FR-006**: Das Ergebnis MUST eine Inventarliste aller geprüften ADR-Dateien mit Entscheidung und Begründung enthalten.

### Key Entities

- **ADR**: eine dokumentierte Architekturentscheidung mit Kontext, Entscheidung, Folgen und gegebenenfalls Alternativen.
- **Normative Quelle**: eine Spec oder ein Vertrag, der Anforderungen und technische Verhaltensregeln verbindlich festlegt.
- **Verweis**: ein Link oder eine inhaltliche Referenz aus einem anderen Projektartefakt auf eine ADR.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Alle 26 vorhandenen ADR-Dateien sind im Inventar einzeln beurteilt.
- **SC-002**: Jede entfernte oder gekürzte ADR hat eine verbleibende Quelle für ihre weiterhin geltenden Entscheidungen und notwendigen Begründungen.
- **SC-003**: Die Bereinigung hinterlässt keine Verweise auf gelöschte ADR-Dateien und keine unklaren doppelten Entscheidungstexte.
- **SC-004**: Keine ADR wird allein deshalb entfernt, weil sie eine Spec verlinkt; die Entscheidung muss in der Spec tatsächlich abgedeckt sein.
- **SC-005**: Ein Maintainer kann für jede geprüfte ADR anhand der Inventarliste erkennen, ob und warum sie entfernt, gekürzt oder vollständig beibehalten wird.

## Assumptions

- Specs bleiben die normative Quelle für Feature-Verhalten.
- ADRs bleiben für übergreifende Entscheidungen, relevante Begründungen und die in `.spaex/constitution.md` verlangten Aufzeichnungen verfügbar.
- ADR 0022 ist ein erster möglicher Redundanzfall: Spec 063 enthält die Windows-Entscheidung, die Umsetzung und die Spike-Ergebnisse; das Inventar muss noch prüfen, ob einzelne Folgen in Spec 063 ergänzt werden müssen.
- ADR 0013 und `docs/formats/encrypted-folder-v1.md` werden getrennt bewertet: Formatvertrag und Testvektoren bleiben auch dann erhalten, wenn die ADR gekürzt wird.
- `docs/formats/encrypted-folder-v1.md` ist ein eigenständiger technischer Vertrag und steht nicht zur Löschung.
