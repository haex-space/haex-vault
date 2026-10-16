# Feature Specification: Paper Notes to Editable Canvas

**Feature Branch**: `feat/paper-notes-canvas`

**Created**: 2026-10-08

**Status**: Draft

**Input**: User description: "Schüler fotografieren handschriftliche Notizen auf normalem Papier. Haex-notes erstellt daraus ein aufgeräumtes, bearbeitbares Canvas mit korrigiertem Text in einer auswählbaren Handschrift, erhaltenen Zeichnungen und PDF-Export. Das Original bleibt erhalten; eine persönliche Handschrift-Erkennung ist ausdrücklich nicht Bestandteil des ersten Schritts."

## Änderungen nach Messung und Entscheidung (2026-10-16)

Diese Punkte gelten vor dem übrigen Text dieser Spec; wo sie widersprechen, haben sie Vorrang. Der Rest wird mit dem Plan angepasst.

- **Handschrift ist Pflicht.** Ziel sind handschriftliche Mitschriften und Tafelbilder, ebenso Arbeitsblätter mit handgeschriebenen Einträgen. Es muss nicht alles perfekt erkannt werden; der Schüler verbessert Falsches selbst.
- **Ergebnis aus Texten, Bildern und Tabellen.** Tabellen werden als Zeilen und Zellen erkannt, nicht als Bild, und lassen sich verschieben und verändern. Aus den erkannten Teilen entsteht ein ordentlich neu zusammengesetztes Tafelbild: Texte änderbar und erweiterbar, Bilder vergrößerbar, verkleinerbar und frei positionierbar.
- **Kein Prüfteil.** User Story 2 (Markierungen bei unsicheren Wörtern, Vorschläge, Ausschnitt des Originals als Referenz) entfällt. Ein Korrekturschritt setzt Rechtschreibung und eindeutige Lesefehler direkt ein, ohne Markierung und ohne Zurücksetzen. Er ändert nichts darüber hinaus: Abkürzungen, Wortwahl und Zeilenumbrüche bleiben.
- **Original bleibt** als ausblendbare Ebene erhalten.
- **Erkennung über Spec 055.** Das Foto geht über die Modellschnittstelle für Erweiterungen (Spec 055) an ein lokales Modell, ein Modell auf einem Server oder in der Cloud, je nach Freigabe des Nutzers. Das Handy nimmt nur das Foto auf. Die feste Aufgabe `paper-note-recognition` aus Spec 047 geht im Auftragsformat „Seitenerkennung“ von Spec 055 auf.
- **Gedrehte Fotos.** Fotos kommen oft seitlich oder kopfüber (Messenger entfernen die Drehinformation). Die Seite wird aufrecht ausgewertet, oder die Erweiterung bietet dem Schüler das Drehen an.
- **Unbrauchbare Erkennung** (Schleifen, leer, Schriftwechsel) wird technisch verworfen; der betroffene Bereich bleibt leer oder bleibt als Bild stehen.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Foto in einen sauberen Entwurf umwandeln (Priority: P1)

Ein Schüler fotografiert seine handschriftlichen Unterrichtsnotizen und importiert das Bild in haex-notes. Die Anwendung erstellt daraus einen aufgeräumten Entwurf, in dem erkannter Text und visuelle Inhalte getrennt bearbeitet werden können.

**Why this priority**: Ohne einen schnellen Weg vom Papier zum digitalen Entwurf entsteht für Schüler kein ausreichender Mehrwert gegenüber einer normalen Scan-App.

**Independent Test**: Mit einem repräsentativen Foto einer handschriftlichen Notiz lässt sich ein neuer Canvas-Entwurf erzeugen, der Originalbild und aufbereitete Inhalte enthält.

**Acceptance Scenarios**:

1. **Given** ein unterstütztes Foto einer einzelnen Notizseite, **When** der Schüler es direkt mit der Runtime-Kamera aufnimmt oder eine Bilddatei auswählt und die Aufbereitung startet, **Then** wird eine neue Canvas-Seite mit dem Originalbild und einem aufbereiteten Entwurf angelegt.
2. **Given** ein Foto mit Text, Zeichnungen, Tabellen und einer durchgestrichenen Textstelle, **When** die Aufbereitung abgeschlossen ist, **Then** werden Text, Tabellen als Zeilen und Zellen sowie visuelle Inhalte als getrennt bearbeitbare Bereiche dargestellt.
3. **Given** ein Foto mit schlechter Perspektive oder ungleichmäßiger Beleuchtung, **When** die Aufbereitung startet, **Then** wird es soweit möglich entzerrt und lesbarer gemacht, ein neuer Aufnahmeversuch vorgeschlagen und das unveränderte Original trotzdem erhalten.

### User Story 2 - Entfällt

Ein separater Prüfteil mit Unsicherheitsmarkierungen, Vorschlagslisten und Referenz-Crops gehört nicht zur ersten Ausbaustufe. Der Korrekturschritt wendet ausschließlich Rechtschreibung und eindeutige Lesefehler direkt an; der Schüler kann das Ergebnis danach wie jeden anderen Canvas-Inhalt bearbeiten.

### User Story 3 - Den Entwurf als Canvas weiterbearbeiten (Priority: P1)

Ein Schüler bearbeitet den erzeugten Entwurf wie eine digitale Notizseite. Er kann Text ändern, Inhalte verschieben und Zeichnungen oder Diagramme unabhängig voneinander anpassen.

**Why this priority**: Das Ziel ist kein schreibgeschütztes Scan-Ergebnis, sondern ein digitales Abbild, das nach der Digitalisierung weiterverwendet werden kann.

**Independent Test**: Nach der Aufbereitung lassen sich mindestens ein Textbereich und ein visueller Bereich unabhängig voneinander verändern, speichern und wieder öffnen.

**Acceptance Scenarios**:

1. **Given** ein aufbereiteter Textbereich, **When** der Schüler Wörter ändert oder ergänzt, **Then** bleibt die Änderung auf diesen Textbereich beschränkt und das Original bleibt unverändert.
2. **Given** eine Zeichnung oder ein Diagramm, **When** der Schüler es auswählt, **Then** kann er es verschieben und in der Größe verändern.
3. **Given** ein Entwurf mit mehreren Ebenen, **When** der Schüler zwischen Original, bereinigter Ansicht und Vergleichsansicht wechselt, **Then** werden die entsprechenden Inhalte ein- oder ausgeblendet, ohne Daten zu verlieren.
4. **Given** ein gespeicherter Entwurf, **When** der Schüler ihn später erneut öffnet, **Then** sind Text, visuelle Inhalte, Bearbeitungen und Ebenenzustände weiterhin vorhanden.

### User Story 4 - Eine lesbare Version exportieren (Priority: P2)

Ein Schüler exportiert den fertig bearbeiteten Canvas als PDF, um ihn abzugeben, zu teilen oder zu archivieren.

**Why this priority**: Der Export macht das Ergebnis außerhalb von haex-notes nutzbar, ist aber erst sinnvoll, wenn Import, Erkennung und Bearbeitung funktionieren.

**Independent Test**: Ein fertig bearbeiteter Entwurf kann als PDF exportiert und in einem üblichen PDF-Betrachter geöffnet werden.

**Acceptance Scenarios**:

1. **Given** ein fertig bearbeiteter Canvas, **When** der Schüler PDF-Export auswählt, **Then** enthält die PDF die sichtbaren bereinigten Text- und Bildelemente in ihrer aktuellen Anordnung.
2. **Given** ein Canvas mit ausgeblendeter Originalebene, **When** der Schüler exportiert, **Then** wird das Originalbild nicht in die bereinigte PDF aufgenommen.
3. **Given** ein Canvas, bei dem die Originalebene sichtbar ist, **When** der Schüler exportiert, **Then** wird die PDF entsprechend der gewählten Sichtbarkeit erstellt.

### Edge Cases

- Das Foto ist unscharf, stark geneigt oder teilweise verdeckt. Die Anwendung muss die Qualität verständlich anzeigen, ein neues Foto vorschlagen und trotzdem eine best-effort-Aufbereitung ermöglichen, ohne das Original zu verwerfen.
- Einzelne Wörter oder Symbole sind nicht lesbar. Der betroffene Bereich wird verworfen oder bleibt als Originalbild erhalten; es wird kein Prüfhinweis erzeugt.
- Durchgestrichener Text kann nicht sicher von einer gültigen Ergänzung unterschieden werden. Die Anwendung verwirft die unbrauchbare Erkennung oder belässt den betroffenen Bereich als Bild.
- Zeichnungen, Tabellen und Text werden falsch klassifiziert. Der Schüler muss die betroffenen Bereiche weiterhin als bearbeitbare Inhalte korrigieren können.
- Die Verarbeitung schlägt fehl oder ist vorübergehend nicht verfügbar. Das Originalbild muss trotzdem als Canvas-Inhalt gespeichert werden können.
- Eine verwendete Handschrift-Darstellung ist nicht verfügbar. Der Text muss mit einer neutralen Fallback-Darstellung lesbar bleiben.
- Bilder werden einzeln verarbeitet; ein Batch- oder Mehrseitenimport ist nicht Bestandteil der ersten Ausbaustufe. Ein Fehler bei Dekodierung oder Aufbereitung darf nicht zum Verlust bereits gespeicherter Seiten führen.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Das System MUST den Import eines Fotos einer handschriftlichen Notizseite ermöglichen und das unveränderte Original dauerhaft dem Entwurf zuordnen.
- **FR-001a**: Das System MUST den Import über einen Dateiauswahldialog ermöglichen. Eine verfügbare Gerätekamera KANN zusätzlich genutzt werden; Drag-and-drop ist optional und kein MVP-Bestandteil.
- **FR-002**: haex-notes MUST eine lokale Arbeitskopie hinsichtlich Zuschnitt, Perspektive und Lesbarkeit aufbereiten, ohne das Originalbild zu überschreiben. Bei erkannten Qualitätsproblemen MUSS die Anwendung einen neuen Aufnahmeversuch vorschlagen und die best-effort-Verarbeitung dennoch ermöglichen.
- **FR-003**: Das System MUST erkannte Inhalte nach ihrer Rolle unterscheiden können, mindestens zwischen Text und visuellen Inhalten wie Zeichnungen oder Diagrammen.
- **FR-004**: Das System MUST erkannten Text mit seiner Position auf der Seite in einen bearbeitbaren Entwurf übernehmen.
- **FR-005**: Das System MUST unbrauchbare Erkennung (etwa Schleifen, leere Ergebnisse oder unbrauchbare Schriftwechsel) für den betroffenen Bereich verwerfen; der Bereich bleibt leer oder als Originalbild erhalten.
- **FR-006**: Das System MUST Rechtschreibung und eindeutige Lesefehler direkt korrigieren, ohne einen separaten Prüfhinweis oder eine Vorschlagsliste zu erzeugen. Abkürzungen, Wortwahl und Zeilenumbrüche dürfen dabei nicht verändert werden.
- **FR-007**: Das System MUST die unveränderte Originaldarstellung als ausblendbare Ebene erhalten, auch wenn die Interpretation eines Bereichs verworfen wird.
- **FR-008**: Das System MUST die Auswahl einer handschriftlichen Darstellung für bereinigten Text ermöglichen. Die Darstellung dient der Präsentation; der zugrunde liegende Inhalt bleibt bearbeitbarer Text.
- **FR-009**: Das System MUST erkannte Zeichnungen, Diagramme und vergleichbare visuelle Inhalte als eigenständige Objekte im Canvas speichern, die der Schüler verschieben und in ihrer Größe verändern kann.
- **FR-010**: Das System MUST eine Ansicht ermöglichen, in der das Originalbild, der bereinigte Entwurf und ein direkter Vergleich ein- oder ausgeblendet werden können.
- **FR-011**: Das System MUST Tabellen als bearbeitbare Zeilen und Zellen speichern können, statt sie nur als Bild zu behandeln.
- **FR-012**: Das System MUST den Entwurf mit allen Text-, Bild-, Tabellen- und Sichtbarkeitsinformationen speichern und wiederherstellen können.
- **FR-013**: Das System MUST den sichtbaren Canvas-Zustand als PDF exportieren können.
- **FR-014**: Das System MUST ohne persönliche Schreibproben oder ein persönliches Handschriftprofil funktionieren. Eine individuelle Handschriftanpassung ist nicht Bestandteil dieser ersten Ausbaustufe.
- **FR-015**: Das System MUST vor einer Verarbeitung außerhalb des eigenen Geräts verständlich anzeigen, wohin Notizbilder und erkannte Inhalte übertragen werden, und eine solche Übertragung darf nicht stillschweigend erfolgen.
- **FR-016**: Das System MUST bei nicht verfügbarer oder fehlgeschlagener automatischer Verarbeitung den manuellen Zugriff auf das Originalbild und das Erstellen eines bearbeitbaren Canvas ermöglichen.
- **FR-017**: Die automatische Aufbereitung MUST standardmäßig über den konfigurierten Holzi-Agent-Task laufen. haex-notes darf für den Standardpfad weder einen konkreten LLM-Anbieter noch ein eigenes Provider-SDK voraussetzen.
- **FR-018**: Der Holzi-Agent-Task MUST ein strukturiertes, schema-validiertes Ergebnis mit Text-, Bild- und Tabellenregionen, Quellpositionen und Verarbeitungsmetadaten liefern. Freier Chat-Text darf nicht direkt als Canvas-Dokument interpretiert werden.
- **FR-019**: Die Auswahl von Modell und Harness MUST außerhalb des Canvas-Datenmodells über Holzi-Agent-Einstellungen erfolgen. Die Seite darf nur die verwendete Profil-/Modellkennung und den Verarbeitungsstatus als Provenienz speichern, niemals Zugangsdaten.

### Key Entities

- **Notizimport**: Ein vom Schüler hinzugefügtes Originalfoto einschließlich Aufnahme- und Seiteninformationen.
- **Originalebene**: Die unveränderte Referenzdarstellung des importierten Fotos, die nicht durch automatische Korrekturen überschrieben wird.
- **Inhaltsbereich**: Ein räumlich abgegrenzter erkannter Bereich mit Rolle, Position und optionaler Verbindung zum Originalausschnitt.
- **Textbereich**: Ein bearbeitbarer Textinhalt mit Erkennungsergebnis und verwendeter Handschrift-Darstellung.
- **Visuelles Objekt**: Eine Zeichnung, ein Diagramm, eine Tabelle oder ein anderer nicht als Text behandelter Bereich, der unabhängig im Canvas bearbeitet werden kann.
- **Canvas-Entwurf**: Die zusammengesetzte, speicherbare Seite aus Original, Textbereichen, visuellen Objekten und Sichtbarkeitseinstellungen.
- **Recognition-Profil**: Eine in Holzi konfigurierte Auswahl aus Harness, Modell, Verarbeitungsort und optionaler Fallback-Regel für strukturierte Papiernotiz-Aufbereitung.

### Out of Scope for the First Release

- Persönliches Training oder lokales Abrufen einer individuellen Handschrift des Schülers.
- Automatische Umwandlung von Zeichnungen in vollständig editierbare Vektorgrafiken.
- Vollautomatische inhaltliche Verbesserung über Rechtschreibung und eindeutige Lesefehler hinaus; insbesondere darf die Anwendung keine Inhalte erfinden, löschen oder umformulieren.
- Zwangsbindung an einen bestimmten Cloud-Anbieter oder ein bestimmtes KI-Modell.
- Eine direkte LLM-/OCR-Integration in haex-notes neben dem Holzi-Agent-Task.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Für jede repräsentative Einzelbild-Fixture wird auf der festgelegten Referenzumgebung innerhalb von drei Minuten entweder ein erster Canvas-Entwurf oder eine verständliche Qualitäts-/Verarbeitungswarnung angezeigt.
- **SC-002**: Alle in den annotierten Referenz-Fixtures als unbrauchbar gekennzeichneten Regionen werden verworfen oder als Originalbild erhalten; es wird kein leerer oder erfundener Inhalt daraus erzeugt.
- **SC-003**: Der Quickstart-Durchlauf ermöglicht das Bearbeiten eines erzeugten Text- und Bildbereichs, ohne das Originalbild zu verändern.
- **SC-004**: Alle in einer annotierten Referenz-Fixture erwarteten Text- und visuellen Objekte bleiben nach Speichern, erneutem Öffnen und PDF-Export mit definierter Positionstoleranz und erwarteter Sichtbarkeit erhalten.
- **SC-005**: Automatische Änderungen beschränken sich auf Rechtschreibung und eindeutige Lesefehler; Abkürzungen, Wortwahl und Zeilenumbrüche bleiben in allen annotierten Referenz-Fixtures erhalten.
- **SC-006**: Bei einem Ausfall der automatischen Verarbeitung bleibt das Originalbild für 100 % der erfolgreich importierten Testseiten zugänglich und der Nutzer kann den Entwurf manuell weiterbearbeiten.
- **SC-007**: Eine spätere Pilotbewertung kann die wahrgenommene Übersichtlichkeit und Bearbeitbarkeit erfassen; sie ist kein technisches Release-Gate dieser Ausbaustufe.

## Assumptions

- Die erste Version erhält Bilder über den Dateiauswahldialog der plattformunabhängigen holzi-Runtime; eine verfügbare Kamera kann zusätzlich unterstützt werden. Eine zusätzliche Scanner- oder Tafel-App ist nicht erforderlich.
- Der Schüler bearbeitet automatisch erzeugte Inhalte selbst. Die Anwendung stellt einen korrigierbaren Entwurf bereit, keine verbindliche Transkription.
- Bilder werden einzeln verarbeitet und jeweils einer Notizseite zugeordnet; Batch- und Mehrseitenimport sind nicht Teil der ersten Ausbaustufe.
- Die Auswahl und Lizenzierung der verfügbaren Handschrift-Darstellungen wird vor der Veröffentlichung separat geprüft.
- Die Entscheidung, ob die automatische Verarbeitung lokal oder über einen externen Dienst erfolgt, bleibt eine spätere Architekturentscheidung; der Nutzer muss den Verarbeitungsort jedoch nachvollziehen können.
- Holzi stellt einen strukturierten Agent-Task bereit, über den haex-notes die Aufbereitung anfordert. Das Profil dieses Tasks wählt der Nutzer in Holzi; haex-notes zeigt die daraus resultierende Verarbeitungsart und Provenienz an.
- Für Bildverarbeitung wird ein Vision-/OCR-fähiges Modell vorausgesetzt. Ein reines Textmodell wie das derzeitige Qwen3-Profil kann den Import nicht allein verarbeiten.
- Der aktuelle haex-notes-Canvas wird als Zielumgebung verwendet, ohne dadurch eine bestimmte Erkennungs- oder KI-Technologie festzuschreiben.
