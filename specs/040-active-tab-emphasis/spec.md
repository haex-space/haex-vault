# Feature Specification: Active Tab Emphasis

**Feature Branch**: `feat/active-tab-emphasis`

**Created**: 2026-10-06

**Status**: Ready for planning

**Input**: User description: "Den aktiven Tab im Window besser hervorheben, damit sofort klar wird, welcher Tab gerade aktiv ist."

> **Note (spec 090):** The tab strip is suspended by spec 090 until spec 091 (switching tabs) is done.

## User Scenarios & Testing

### User Story 1 - Aktiven Tab sofort erkennen (Priority: P1)

Als Nutzer möchte ich den aktiven Tab eines Fensters auf den ersten Blick erkennen, damit ich beim Wechseln zwischen mehreren Tabs nicht den Überblick verliere.

**Why this priority**: Die zentrale Aufgabe der Änderung ist eine schnellere und eindeutigere Orientierung in Fenstern mit mehreren Tabs.

**Independent Test**: Ein Fenster mit mindestens zwei Tabs wird in heller und dunkler Darstellung geöffnet. Der aktive Tab ist ohne Interaktion eindeutig vom inaktiven Tab unterscheidbar; beim Wechsel bleibt genau der ausgewählte Tab hervorgehoben.

**Acceptance Scenarios**:

1. **Given** ein Fenster mit mindestens zwei Tabs, **When** ein Tab aktiv ist, **Then** hebt sich dieser Tab durch eine klar erkennbare visuelle Gestaltung von den inaktiven Tabs und der Titelleiste ab.
2. **Given** ein Fenster mit mehreren Tabs, **When** der Nutzer einen anderen Tab auswählt, **Then** wandert die Hervorhebung auf den ausgewählten Tab und der vorher aktive Tab verliert sie.
3. **Given** die Anwendung verwendet ein helles oder dunkles Farbschema, **When** mehrere Tabs sichtbar sind, **Then** bleibt die Hervorhebung in beiden Schemata eindeutig und der Text gut lesbar.
4. **Given** ein Fenster zeigt nur einen Tab oder befindet sich im kompakten Modus, **When** der Tab-Titel dargestellt wird, **Then** bleibt das bestehende platzsparende Layout erhalten.

### Edge Cases

- Tabs mit langen Titeln dürfen die Hervorhebung nicht aus dem sichtbaren Tab-Bereich herausdrängen.
- Die Schließen-, Scroll- und Neu-Tab-Steuerelemente bleiben bedienbar und werden nicht von der Hervorhebung überdeckt.
- Die vorhandene Tastatur- und ARIA-Auswahl des aktiven Tabs bleibt unverändert.
- Die Hervorhebung darf sich nicht allein auf Farbe stützen; Schriftgewicht und Akzentbalken bleiben auch ohne Farbwahrnehmung erkennbar.

## Requirements

### Functional Requirements

- **FR-001**: Das Fenster MUSS den aktiven Tab in einer Tab-Leiste visuell deutlich von inaktiven Tabs unterscheiden.
- **FR-002**: Die Hervorhebung MUSS den aktiven Tab gleichzeitig durch eine kontrastreiche Fläche, eine stärkere Textgewichtung und einen sichtbaren Akzent an der Tab-Kante kennzeichnen.
- **FR-003**: Beim Wechsel des aktiven Tabs MUSS die Hervorhebung ohne zusätzliche Nutzeraktion auf den neuen Tab wechseln.
- **FR-004**: Die Hervorhebung MUSS in hellen und dunklen Farbschemata ausreichend Kontrast für Tab-Titel und Symbole bieten.
- **FR-005**: Die Darstellung einzelner Tabs und kompakter Fenster DARF durch die Mehrfach-Tab-Hervorhebung nicht verändert werden.
- **FR-006**: Die bestehende Auswahl-, Schließ- und Tastaturbedienung der Tabs MUSS erhalten bleiben.
- **FR-007**: Ein per Tastatur fokussierter Tab MUSS einen sichtbaren Fokusindikator zeigen, der sich von der Hervorhebung des aktiven Tabs unterscheidet und nicht von der Tab-Leiste abgeschnitten wird.

## Success Criteria

### Measurable Outcomes

- **SC-001**: In einer visuellen Prüfung mit mindestens zwei sichtbaren Tabs identifizieren Nutzer den aktiven Tab innerhalb von zwei Sekunden ohne zusätzliche Interaktion.
- **SC-002**: Nach einem Tab-Wechsel ist genau ein Tab hervorgehoben und der zuvor aktive Tab zeigt keine aktive Hervorhebung mehr.
- **SC-003**: Die Hervorhebung besteht die Prüfung in hellem und dunklem Farbschema, ohne dass Tab-Titel oder Symbole ihre Lesbarkeit verlieren.
- **SC-004**: Die bestehenden Tab-Navigations- und Template-Prüfungen bleiben erfolgreich.

## Assumptions

- Die vorhandenen Theme-Farben und Utility-Klassen reichen für die Gestaltung aus; neue Farbwerte oder Abhängigkeiten sind nicht erforderlich.
- Die Änderung betrifft ausschließlich die sichtbare Mehrfach-Tab-Leiste im Window.
- Kompakte Fenster und Fenster mit genau einem Tab behalten ihr bestehendes Layout.
