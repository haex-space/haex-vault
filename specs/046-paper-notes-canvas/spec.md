# Feature Specification: Paper Notes to Editable Canvas

**Feature Branch**: `feat/paper-notes-canvas`

**Created**: 2026-10-08

**Status**: Überarbeitet 2026-10-21 (Erkennung über Spec 055)

**Input**: User description: "Schüler fotografieren handschriftliche Notizen auf normalem Papier. Haex-notes erstellt daraus ein aufgeräumtes, bearbeitbares Canvas mit korrigiertem Text in einer auswählbaren Handschrift, erhaltenen Zeichnungen und PDF-Export. Das Original bleibt erhalten; eine persönliche Handschrift-Erkennung ist ausdrücklich nicht Bestandteil des ersten Schritts."

## Clarifications

### Session 2026-10-16

- Q: Muss Handschrift erkannt werden? → A: Ja. Ziel sind handschriftliche Mitschriften, Tafelbilder und Arbeitsblätter mit handgeschriebenen Einträgen. Es muss nicht alles perfekt sein; der Schüler verbessert Falsches selbst.
- Q: Wie werden Tabellen übernommen? → A: Als Zeilen und Zellen, nicht als Bild. Sie lassen sich verschieben und verändern. Aus Texten, Bildern und Tabellen entsteht ein ordentlich neu zusammengesetztes Tafelbild.
- Q: Bleibt der Prüfteil (Markierungen, Vorschläge, Ausschnitte)? → A: Nein. Ein Korrekturschritt setzt Rechtschreibung und eindeutige Lesefehler direkt ein, ohne Markierung und ohne Zurücksetzen; Abkürzungen, Wortwahl und Zeilenumbrüche bleiben.
- Q: Wie gehen gedrehte Fotos? → A: Messenger entfernen die Drehinformation. Die Seite wird aufrecht ausgewertet, oder die Erweiterung bietet das Drehen an.

### Session 2026-10-21

- Q: Worüber läuft die Erkennung? → A: Über die Modellaufträge von Spec 055 (vault-sdk `models.runAsync`, ab 4.7.1). haex-notes beschreibt den Auftrag „Seitenerkennung“ mit eigener Anweisung und eigenem Format; holzi wählt das freigegebene Modell, fragt vor dem Versand außerhalb des Geräts und verwirft kaputte Ausgabe. Die feste Aufgabe aus Spec 047 und die Profile entfallen.
- Q: Was passiert mit einem Drehhinweis des Modells? → A: haex-notes dreht die Arbeitskopie um den gemeldeten Winkel und liest einmal neu; das Original bleibt unverändert. Zusätzlich kann der Schüler das Foto vor dem Lesen selbst drehen.
- Q: Welche Ausgabe gilt als unbrauchbar? → A: Was holzi verwirft (leer, Schleife, Schriftwechsel, falsches Format), liefert gar kein Ergebnis; die Seite enthält dann nur das Original. haex-notes übergeht zusätzlich einzelne Regionen ohne Inhalt.
- Q: Wie liest man eine schlecht erkannte Stelle neu? → A: Der Schüler zieht auf der Seite ein Rechteck über die Stelle und macht davon ein Nahfoto. Es läuft durch denselben Auftrag; das Ergebnis wird in das Rechteck eingepasst und ersetzt die erzeugten Elemente, die ganz darin liegen. Ein ganzes Blatt erneut zu lesen und alte und neue Regionen einander zuzuordnen, entfällt: auf einem neuen Foto stimmen Lage und Zuschnitt nicht mehr überein.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Foto in einen sauberen Entwurf umwandeln (Priority: P1)

Ein Schüler fotografiert seine handschriftlichen Unterrichtsnotizen und importiert das Bild in haex-notes. Die Anwendung erstellt daraus einen aufgeräumten Entwurf, in dem erkannter Text und visuelle Inhalte getrennt bearbeitet werden können.

**Why this priority**: Ohne einen schnellen Weg vom Papier zum digitalen Entwurf entsteht für Schüler kein ausreichender Mehrwert gegenüber einer normalen Scan-App.

**Independent Test**: Mit einem repräsentativen Foto einer handschriftlichen Notiz lässt sich ein neuer Canvas-Entwurf erzeugen, der Originalbild und aufbereitete Inhalte enthält.

**Acceptance Scenarios**:

1. **Given** ein unterstütztes Foto einer einzelnen Notizseite, **When** der Schüler es direkt mit der Runtime-Kamera aufnimmt oder eine Bilddatei auswählt und die Aufbereitung startet, **Then** wird eine neue Canvas-Seite mit dem Originalbild und einem aufbereiteten Entwurf angelegt.
2. **Given** ein Foto mit Text, Zeichnungen, Tabellen und einer durchgestrichenen Textstelle, **When** die Aufbereitung abgeschlossen ist, **Then** werden Text, Tabellen als Zeilen und Zellen sowie visuelle Inhalte als getrennt bearbeitbare Bereiche dargestellt.
3. **Given** ein Foto mit schlechter Perspektive oder ungleichmäßiger Beleuchtung, **When** die Aufbereitung startet, **Then** wird es soweit möglich entzerrt und lesbarer gemacht, ein neuer Aufnahmeversuch vorgeschlagen und das unveränderte Original trotzdem erhalten.
4. **Given** ein seitlich oder kopfüber fotografiertes Blatt, **When** die Aufbereitung läuft, **Then** wird die Seite aufrecht ausgewertet und der Entwurf aufrecht angelegt; das Original bleibt, wie es aufgenommen wurde.
5. **Given** der Nutzer hat in holzi kein passendes Modell freigegeben oder die Übertragung abgelehnt, **When** die Aufbereitung startet, **Then** entsteht eine Seite mit dem Original, und haex-notes sagt verständlich, warum nichts erkannt wurde.

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
5. **Given** eine Stelle des Entwurfs, die schlecht oder gar nicht erkannt wurde, **When** der Schüler ein Rechteck darüber zieht und ein Nahfoto der Stelle lesen lässt, **Then** ersetzt das Ergebnis die erzeugten Elemente, die ganz im Rechteck liegen, eigene Ergänzungen und das Original bleiben, und ein Rückgängig stellt den vorherigen Stand her.

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
- Einzelne Wörter oder Symbole sind nicht lesbar. Der Schüler korrigiert sie im Entwurf; es wird kein Prüfhinweis erzeugt. Ist die ganze Ausgabe unbrauchbar, enthält die Seite nur das Original.
- Durchgestrichener Text kann nicht sicher von einer gültigen Ergänzung unterschieden werden. Das Modell soll nichts erfinden; der Schüler entfernt Überflüssiges selbst.
- Das Foto liegt seitlich oder kopfüber. Die Seite wird aufrecht ausgewertet (Drehhinweis des Modells oder Drehen durch den Schüler); das Original bleibt unverändert.
- Der Nutzer hat in holzi kein Modell mit Bildern freigegeben, lehnt die Übertragung ab oder bricht ab. Das Original bleibt als Seite, und der Schüler kann manuell weiterarbeiten.
- Zeichnungen, Tabellen und Text werden falsch klassifiziert. Der Schüler muss die betroffenen Bereiche weiterhin als bearbeitbare Inhalte korrigieren können.
- Die Verarbeitung schlägt fehl oder ist vorübergehend nicht verfügbar. Das Originalbild muss trotzdem als Canvas-Inhalt gespeichert werden können.
- Das Nahfoto zeigt mehr oder etwas anderes als die Stelle unter dem Rechteck. Das Ergebnis wird trotzdem in das Rechteck eingepasst; der Schüler nimmt es zurück oder verschiebt die Elemente.
- Das Lesen eines Nahfotos schlägt fehl oder wird abgebrochen. Die Seite bleibt unverändert, und das Nahfoto wird nicht gespeichert.
- Eine verwendete Handschrift-Darstellung ist nicht verfügbar. Der Text muss mit einer neutralen Fallback-Darstellung lesbar bleiben.
- Bilder werden einzeln verarbeitet; ein Batch- oder Mehrseitenimport ist nicht Bestandteil der ersten Ausbaustufe. Ein Fehler bei Dekodierung oder Aufbereitung darf nicht zum Verlust bereits gespeicherter Seiten führen.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Das System MUST den Import eines Fotos einer handschriftlichen Notizseite ermöglichen und das unveränderte Original dauerhaft dem Entwurf zuordnen.
- **FR-001a**: Das System MUST den Import über einen Dateiauswahldialog ermöglichen. Eine verfügbare Gerätekamera KANN zusätzlich genutzt werden; Drag-and-drop ist optional und kein MVP-Bestandteil.
- **FR-002**: haex-notes MUST eine lokale Arbeitskopie hinsichtlich Zuschnitt, Perspektive und Lesbarkeit aufbereiten, ohne das Originalbild zu überschreiben. Bei erkannten Qualitätsproblemen MUSS die Anwendung einen neuen Aufnahmeversuch vorschlagen und die best-effort-Verarbeitung dennoch ermöglichen.
- **FR-003**: Das System MUST erkannte Inhalte nach ihrer Rolle unterscheiden können, mindestens zwischen Text und visuellen Inhalten wie Zeichnungen oder Diagrammen.
- **FR-004**: Das System MUST erkannten Text mit seiner Position auf der Seite in einen bearbeitbaren Entwurf übernehmen.
- **FR-005**: Das System MUST unbrauchbare Erkennung verwerfen: Eine Ausgabe, die holzi als leer, Schleife, Schriftwechsel oder formatwidrig verwirft, erzeugt keinen Inhalt, und die Seite behält nur das Original. Einzelne Regionen ohne Inhalt werden übergangen.
- **FR-006**: Das System MUST Rechtschreibung und eindeutige Lesefehler direkt korrigieren, ohne einen separaten Prüfhinweis oder eine Vorschlagsliste zu erzeugen. Abkürzungen, Wortwahl und Zeilenumbrüche dürfen dabei nicht verändert werden.
- **FR-007**: Das System MUST die unveränderte Originaldarstellung als ausblendbare Ebene erhalten, auch wenn die Interpretation eines Bereichs verworfen wird.
- **FR-008**: Das System MUST die Auswahl einer handschriftlichen Darstellung für bereinigten Text ermöglichen. Die Darstellung dient der Präsentation; der zugrunde liegende Inhalt bleibt bearbeitbarer Text.
- **FR-009**: Das System MUST erkannte Zeichnungen, Diagramme und vergleichbare visuelle Inhalte als eigenständige Objekte im Canvas speichern, die der Schüler verschieben und in ihrer Größe verändern kann.
- **FR-010**: Das System MUST eine Ansicht ermöglichen, in der das Originalbild, der bereinigte Entwurf und ein direkter Vergleich ein- oder ausgeblendet werden können.
- **FR-011**: Das System MUST Tabellen als bearbeitbare Zeilen und Zellen speichern können, statt sie nur als Bild zu behandeln.
- **FR-012**: Das System MUST den Entwurf mit allen Text-, Bild-, Tabellen- und Sichtbarkeitsinformationen speichern und wiederherstellen können.
- **FR-013**: Das System MUST den sichtbaren Canvas-Zustand als PDF exportieren können.
- **FR-014**: Das System MUST ohne persönliche Schreibproben oder ein persönliches Handschriftprofil funktionieren. Eine individuelle Handschriftanpassung ist nicht Bestandteil dieser ersten Ausbaustufe.
- **FR-015**: Eine Verarbeitung außerhalb des eigenen Geräts MUST NOT stillschweigend erfolgen. Die Einwilligung mit Anzeige des Ziels fragt holzi vor dem ersten Versand ab (Spec 055); haex-notes zeigt nach der Erkennung an, welches Modell an welchem Ort gelesen hat.
- **FR-016**: Das System MUST bei nicht verfügbarer oder fehlgeschlagener automatischer Verarbeitung den manuellen Zugriff auf das Originalbild und das Erstellen eines bearbeitbaren Canvas ermöglichen.
- **FR-017**: Die automatische Aufbereitung MUST als Modellauftrag über holzi laufen (Spec 055). haex-notes darf weder einen konkreten Anbieter noch ein Modell noch Zugangsdaten voraussetzen.
- **FR-018**: Der Auftrag „Seitenerkennung“ MUST aus zwei Schritten bestehen: Lesen der Seite in ein Format mit Regionen (Art Text, Bild oder Tabelle; Position; Text; Zeilen und Zellen; Drehhinweis) und Korrektur der gelesenen Texte. haex-notes übernimmt nur ein Ergebnis, das holzi gegen dieses Format geprüft hat; freier Modelltext wird nie als Seite gedeutet.
- **FR-019**: Modellwahl, Aufwand und Einwilligung MUST in holzi liegen (Spec 055). Die Seite speichert als Herkunft nur Modellname, Ort und Zeitpunkt der Erkennung, niemals Adresse oder Zugangsdaten.
- **FR-020**: Das System MUST ein seitlich oder kopfüber aufgenommenes Foto aufrecht auswerten. Meldet das Modell einen Drehwinkel, dreht haex-notes die Arbeitskopie und liest einmal neu; der Schüler kann das Foto vor dem Lesen auch selbst drehen. Das Original wird nie gedreht gespeichert.
- **FR-021**: Das System MUST auf einer Seite aus einem Foto das Nachlesen einer Stelle über ein Nahfoto ermöglichen: Der Schüler wählt die Stelle als Rechteck auf der Seite, das Nahfoto läuft durch denselben Auftrag „Seitenerkennung“, und das Ergebnis ersetzt die erzeugten Elemente, die ganz im Rechteck liegen. Eigene Ergänzungen und das Original bleiben unverändert. Das Ersetzen ist ein Undo-Schritt; ein Fehler oder Abbruch ändert die Seite nicht.

### Key Entities

- **Notizimport**: Ein vom Schüler hinzugefügtes Originalfoto einschließlich Aufnahme- und Seiteninformationen.
- **Originalebene**: Die unveränderte Referenzdarstellung des importierten Fotos, die nicht durch automatische Korrekturen überschrieben wird.
- **Inhaltsbereich**: Ein räumlich abgegrenzter erkannter Bereich mit Rolle, Position und optionaler Verbindung zum Originalausschnitt.
- **Textbereich**: Ein bearbeitbarer Textinhalt mit Erkennungsergebnis und verwendeter Handschrift-Darstellung.
- **Visuelles Objekt**: Eine Zeichnung, ein Diagramm, eine Tabelle oder ein anderer nicht als Text behandelter Bereich, der unabhängig im Canvas bearbeitet werden kann.
- **Canvas-Entwurf**: Die zusammengesetzte, speicherbare Seite aus Original, Textbereichen, visuellen Objekten und Sichtbarkeitseinstellungen.
- **Tabelle**: Ein Bereich aus Zeilen und Zellen mit Text, auch leeren Zellen, der als Ganzes verschoben und in seinen Zellen bearbeitet wird.
- **Seitenerkennungsauftrag**: Der Modellauftrag, den haex-notes an holzi gibt (Anweisung und Format je Schritt, das Foto); er existiert nur während der Erkennung.
- **Nahaufnahme**: Ein zusätzliches Foto einer Stelle der Seite mit dem Rechteck, das es ersetzt hat, und der Herkunft seiner Erkennung.
- **Herkunft**: Modellname, Ort (dieses Gerät, eigenes Netz, Cloud) und Zeitpunkt der Erkennung einer Seite.

### Out of Scope for the First Release

- Persönliches Training oder lokales Abrufen einer individuellen Handschrift des Schülers.
- Automatische Umwandlung von Zeichnungen in vollständig editierbare Vektorgrafiken.
- Vollautomatische inhaltliche Verbesserung über Rechtschreibung und eindeutige Lesefehler hinaus; insbesondere darf die Anwendung keine Inhalte erfinden, löschen oder umformulieren.
- Zwangsbindung an einen bestimmten Cloud-Anbieter oder ein bestimmtes KI-Modell.
- Eine direkte LLM-/OCR-Integration in haex-notes neben den Modellaufträgen von holzi.
- Der Betrieb eines Schul-Servers und die Auswahl des konkreten Modells (Einstellung in holzi, Spec 055).

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
- Wo die Erkennung läuft, entscheidet der Nutzer in holzi: lokal (ein Bildmodell über Ollama auf demselben Gerät, Spec 055 research R8), auf einem Server im eigenen Netz oder in der Cloud. Qualität geht vor Offline.
- Für die Erkennung braucht es ein Modell, das Bilder annimmt. Gemessen: `qwen3-vl:4b-instruct` über Ollama liest eine Seite auf einer 8-GB-Grafikkarte in 12–18 s; ein reines Textmodell kann nur den Korrekturschritt übernehmen.
- haex-notes nutzt vault-sdk ab 4.7.1 (`models.runAsync`; die Wiederholung nach der Antwort des Nutzers auf eine Frage von holzi).
- Der aktuelle haex-notes-Canvas wird als Zielumgebung verwendet, ohne dadurch eine bestimmte Erkennungs- oder KI-Technologie festzuschreiben.
