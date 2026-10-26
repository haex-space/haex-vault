# Feature Specification: Paper Notes to Editable Canvas

**Feature Branch**: `feat/paper-notes-canvas`

**Created**: 2026-10-08

**Status**: Überarbeitet 2026-10-21 (Erkennung über Spec 055; Nahfoto einer Stelle)

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

### Session 2026-10-26

- Q: Was ist das Original einer Fotoseite im Heft? → A: Ein Anhang der Seite, keine eigene Ebene und keine zweite Ansicht. Die Seite zeigt nur die digitale Fassung; das Original ist immer erreichbar, aber nirgends als zweite Seite oder als Ansicht aufgeführt. Ziel ist ein vollständiger digitaler Schulhefter je Fach, in dem keine Seite doppelt steht.
- Q: Was wird aus „Original / Entwurf / Vergleich“? → A: Der Schalter entfällt. Ein Klick auf den Anhang öffnet das Original in einem Seitenfenster neben der Seite, sodass der Vergleich möglich bleibt.
- Q: Wie öffnet man einen Anhang? → A: Im Seitenfenster neben der Seite oder im Vollbild. Das Vollbild zeigt nur den Anhang, damit ein Schüler mit einem Übungsblatt üben kann, ohne die Antworten (Seite, Original) zu sehen.
- Q: Was kann ein Anhang sein? → A: Jede Datei (Foto, PDF, Arbeitsblatt), die der Schüler an die Seite hängt; das Original des Fotos ist der erste Anhang und gegen Löschen geschützt.
- Q: Was heißt „Seitenränder anpassen“? → A: Der Rand der Notizseite selbst: wo Linien oder Karos aufhören und wo Text und Zeichnungen stehen dürfen, je Seite nachträglich einstellbar; er gilt für Bildschirm und PDF. Der Rand im PDF-Export bleibt zusätzlich.
- Q: Woher kommt das Papier der erkannten Seite? → A: Der Schüler wählt Vorlage und Papierfarbe nachträglich im Seitenmenü (haextension#138). Ob die Erkennung die Vorlage des Fotos (kariert, liniert, blanko) vorschlägt, ist eine spätere Ergänzung des Auftragsformats.
- Q: Formeln? → A: Formeln (Mathematik, Chemie, Physik) werden als LaTeX erkannt und gesetzt, nicht als Handschrift oder Bild übernommen. Auf den Hefterseiten stehen sie als eigene Zeile, in Tabellenzellen (Reaktionsgleichungen) und mitten im Satz.
- Q: Wie weit geht das in der ersten Fassung? → A: Eine Formelzeile und eine Tabellenzelle, die nur eine Formel enthält, werden gesetzt. Eine Formel mitten im Satz bleibt Text; das Modell schreibt sie dort in Unicode (H₃O⁺, SO₄²⁻), weil das Mischen von Text und gesetzter Formel in einem umbrochenen Textblock eine eigene Aufgabe ist. Strukturformeln und Skizzen (Lewis-Formeln) bleiben Bilder.
- Q: Womit wird gesetzt? → A: Mit MathJax (SVG-Ausgabe, mit der Chemie-Erweiterung `\ce{…}` für Reaktionsgleichungen), geladen erst, wenn eine Seite eine Formel braucht. Das gesetzte SVG wird mit der Quelle gespeichert.

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
3. **Given** ein Entwurf mit mehreren Ebenen, **When** der Schüler eine Ebene ein- oder ausblendet, **Then** werden die entsprechenden Inhalte ein- oder ausgeblendet, ohne Daten zu verlieren. Das Original ist keine dieser Ebenen, sondern ein Anhang (User Story 5).
4. **Given** ein gespeicherter Entwurf, **When** der Schüler ihn später erneut öffnet, **Then** sind Text, visuelle Inhalte, Bearbeitungen und Ebenenzustände weiterhin vorhanden.
5. **Given** eine Stelle des Entwurfs, die schlecht oder gar nicht erkannt wurde, **When** der Schüler ein Rechteck darüber zieht und ein Nahfoto der Stelle lesen lässt, **Then** ersetzt das Ergebnis die erzeugten Elemente, die ganz im Rechteck liegen, eigene Ergänzungen und der Anhang „Original“ bleiben unverändert, und ein Rückgängig stellt den vorherigen Stand her.

### User Story 4 - Eine lesbare Version exportieren (Priority: P2)

Ein Schüler exportiert den fertig bearbeiteten Canvas als PDF, um ihn abzugeben, zu teilen oder zu archivieren.

**Why this priority**: Der Export macht das Ergebnis außerhalb von haex-notes nutzbar, ist aber erst sinnvoll, wenn Import, Erkennung und Bearbeitung funktionieren.

**Independent Test**: Ein fertig bearbeiteter Entwurf kann als PDF exportiert und in einem üblichen PDF-Betrachter geöffnet werden.

**Acceptance Scenarios**:

1. **Given** ein fertig bearbeiteter Canvas, **When** der Schüler PDF-Export auswählt, **Then** enthält die PDF die sichtbaren bereinigten Text- und Bildelemente in ihrer aktuellen Anordnung.
2. **Given** ein Canvas mit einem Anhang „Original“, **When** der Schüler exportiert, **Then** wird der Anhang nicht in die bereinigte PDF aufgenommen.
3. **Given** ein Canvas mit Papier und Seitenrand, **When** der Schüler exportiert, **Then** zeigt die PDF den sichtbaren Seitenzustand einschließlich Papier und Seitenrand.

### User Story 5 - Das Original als Anhang der Seite (Priority: P1)

Ein Schüler digitalisiert den Schulhefter eines Fachs Seite für Seite. Jede Seite zeigt nur die digitale Fassung; das Foto des Originals hängt an der Seite und ist jederzeit abrufbar, ohne dass es im Heft als zweite Seite oder als zweite Ansicht auftaucht. An dieselbe Seite lassen sich weitere Dateien hängen, etwa das gescannte Arbeitsblatt als PDF.

**Why this priority**: Das Ziel ist der komplette Hefter eines Fachs in digitaler Form. Steht jede Seite zweimal da, einmal als Original und einmal digital, ist er unübersichtlich und doppelt so lang.

**Independent Test**: Nach dem Import eines Fotos zeigt die Seite nur den Entwurf, die Seitenliste hat keinen zweiten Eintrag, und der Anhang „Original“ lässt sich öffnen, neben der Seite ansehen und als Datei sichern. Eine zweite Datei lässt sich anhängen, nach dem Speichern und Öffnen wiederfinden und entfernen; das Original lässt sich nicht entfernen.

**Acceptance Scenarios**:

1. **Given** ein importiertes Foto, **When** die Seite entsteht, **Then** zeigt sie nur die digitale Fassung, und das unveränderte Foto liegt als Anhang „Original“ an der Seite.
2. **Given** eine Seite mit Anhängen, **When** der Schüler den Anhang öffnet, **Then** erscheint er in einem Seitenfenster neben der Seite (mit Zoom), und die Seite bleibt bearbeitbar.
   2a. **Given** ein Übungsblatt mit Fragen als Anhang, dessen Antworten auf der Seite oder im Original stehen, **When** der Schüler den Anhang im Vollbild öffnet, **Then** sieht er nur das Blatt, weder die Seite noch das Original, bis er das Vollbild schließt.
3. **Given** ein Anhang, **When** der Schüler ihn sichert, **Then** erhält er die Datei unverändert.
4. **Given** eine Seite, **When** der Schüler eine Datei anhängt, **Then** erscheint sie in der Liste der Anhänge, bleibt nach Speichern und erneutem Öffnen erhalten und lässt sich wieder entfernen.
5. **Given** der Anhang „Original“, **When** der Schüler versucht, ihn zu entfernen, **Then** entfernt ihn nur, wer die Seite selbst löscht; ein Entfernen der Seite aus dem Papierkorb bringt ihn mit zurück.
6. **Given** eine Seite, deren Erkennung fehlschlug oder abgebrochen wurde, **When** sie entsteht, **Then** steht das Foto sichtbar auf der Seite, damit etwas zu bearbeiten ist, und liegt zusätzlich als Anhang „Original“ an der Seite.
7. **Given** eine geteilte Seite, **When** der Empfänger sie öffnet, **Then** gehen ihre Anhänge mit, oder er sieht einen Platzhalter mit Name und Größe, wenn die Datei bei ihm fehlt.
8. **Given** der PDF-Export, **When** der Schüler exportiert, **Then** enthält die PDF die Seite wie sie zu sehen ist und keinen Anhang.

### User Story 6 - Papier und Seitenrand nachträglich ändern (Priority: P2)

Ein Schüler ändert nach dem Import die Vorlage (blanko, kariert, liniert, Lineatur, ...), die Papierfarbe und den Rand der Seite, weil das Original auf kariertem Papier stand und die Seite weiß entstanden ist.

**Independent Test**: Vorlage, Papierfarbe und Seitenrand einer Seite lassen sich ändern, rückgängig machen, speichern, wieder öffnen und im PDF wiederfinden.

**Acceptance Scenarios**:

1. **Given** eine Seite, **When** der Schüler die Vorlage oder die Papierfarbe wählt, **Then** zeigt die Seite sie sofort, die Wahl lässt sich rückgängig machen, und Inhalte der Seite bleiben unberührt.
2. **Given** eine Seite, **When** der Schüler den Seitenrand ändert, **Then** ändern sich die Fläche, auf der Linien und Karos gezeichnet werden, und der Rand, bis zu dem Inhalte stehen dürfen; vorhandene Inhalte bleiben, wo sie stehen.
3. **Given** eine Seite mit eigenem Rand, **When** der Schüler als PDF exportiert, **Then** zeigt die PDF denselben Rand wie die Seite, zuzüglich dem Rand der Exporteinstellung.

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

- **FR-001**: Das System MUST den Import eines Fotos einer handschriftlichen Notizseite ermöglichen und das unveränderte Original dauerhaft als Anhang der Seite ablegen (FR-022).
- **FR-001a**: Das System MUST den Import über einen Dateiauswahldialog ermöglichen. Eine verfügbare Gerätekamera KANN zusätzlich genutzt werden; Drag-and-drop ist optional und kein MVP-Bestandteil.
- **FR-002**: haex-notes MUST eine lokale Arbeitskopie hinsichtlich Zuschnitt, Perspektive und Lesbarkeit aufbereiten, ohne das Originalbild zu überschreiben. Bei erkannten Qualitätsproblemen MUSS die Anwendung einen neuen Aufnahmeversuch vorschlagen und die best-effort-Verarbeitung dennoch ermöglichen.
- **FR-003**: Das System MUST erkannte Inhalte nach ihrer Rolle unterscheiden können, mindestens zwischen Text und visuellen Inhalten wie Zeichnungen oder Diagrammen.
- **FR-004**: Das System MUST erkannten Text mit seiner Position auf der Seite in einen bearbeitbaren Entwurf übernehmen.
- **FR-005**: Das System MUST unbrauchbare Erkennung verwerfen: Eine Ausgabe, die holzi als leer, Schleife, Schriftwechsel oder formatwidrig verwirft, erzeugt keinen Inhalt, und die Seite zeigt nur das Foto (FR-025). Einzelne Regionen ohne Inhalt werden übergangen.
- **FR-006**: Das System MUST Rechtschreibung und eindeutige Lesefehler direkt korrigieren, ohne einen separaten Prüfhinweis oder eine Vorschlagsliste zu erzeugen. Abkürzungen, Wortwahl und Zeilenumbrüche dürfen dabei nicht verändert werden.
- **FR-007**: Das System MUST die unveränderte Originaldarstellung als Anhang der Seite erhalten, auch wenn die Interpretation eines Bereichs verworfen wird. Sie ist keine Ebene der Seite (FR-022).
- **FR-008**: Das System MUST die Auswahl einer handschriftlichen Darstellung für bereinigten Text ermöglichen. Die Darstellung dient der Präsentation; der zugrunde liegende Inhalt bleibt bearbeitbarer Text.
- **FR-009**: Das System MUST erkannte Zeichnungen, Diagramme und vergleichbare visuelle Inhalte als eigenständige Objekte im Canvas speichern, die der Schüler verschieben und in ihrer Größe verändern kann.
- **FR-010**: Das System MUST das Original neben der Seite öffnen können, damit Entwurf und Original verglichen werden können (FR-023). Der Schalter zwischen Original, Entwurf und Vergleich entfällt.
- **FR-011**: Das System MUST Tabellen als bearbeitbare Zeilen und Zellen speichern können, statt sie nur als Bild zu behandeln.
- **FR-012**: Das System MUST den Entwurf mit allen Text-, Bild-, Tabellen- und Sichtbarkeitsinformationen speichern und wiederherstellen können.
- **FR-013**: Das System MUST den sichtbaren Canvas-Zustand als PDF exportieren können.
- **FR-014**: Das System MUST ohne persönliche Schreibproben oder ein persönliches Handschriftprofil funktionieren. Eine individuelle Handschriftanpassung ist nicht Bestandteil dieser ersten Ausbaustufe.
- **FR-015**: Eine Verarbeitung außerhalb des eigenen Geräts MUST NOT stillschweigend erfolgen. Die Einwilligung mit Anzeige des Ziels fragt holzi vor dem ersten Versand ab (Spec 055); haex-notes zeigt nach der Erkennung an, welches Modell an welchem Ort gelesen hat.
- **FR-016**: Das System MUST bei nicht verfügbarer oder fehlgeschlagener automatischer Verarbeitung den manuellen Zugriff auf das Originalbild und das Erstellen eines bearbeitbaren Canvas ermöglichen; das Foto steht dann sichtbar auf der Seite (FR-025).
- **FR-017**: Die automatische Aufbereitung MUST als Modellauftrag über holzi laufen (Spec 055). haex-notes darf weder einen konkreten Anbieter noch ein Modell noch Zugangsdaten voraussetzen.
- **FR-018**: Der Auftrag „Seitenerkennung“ MUST aus zwei Schritten bestehen: Lesen der Seite in ein Format mit Regionen (Art Text, Bild oder Tabelle; Position; Text; Zeilen und Zellen; Drehhinweis) und Korrektur der gelesenen Texte. haex-notes übernimmt nur ein Ergebnis, das holzi gegen dieses Format geprüft hat; freier Modelltext wird nie als Seite gedeutet.
- **FR-019**: Modellwahl, Aufwand und Einwilligung MUST in holzi liegen (Spec 055). Die Seite speichert als Herkunft nur Modellname, Ort und Zeitpunkt der Erkennung, niemals Adresse oder Zugangsdaten.
- **FR-020**: Das System MUST ein seitlich oder kopfüber aufgenommenes Foto aufrecht auswerten. Meldet das Modell einen Drehwinkel, dreht haex-notes die Arbeitskopie und liest einmal neu; der Schüler kann das Foto vor dem Lesen auch selbst drehen. Das Original wird nie gedreht gespeichert.
- **FR-021**: Das System MUST auf einer Seite aus einem Foto das Nachlesen einer Stelle über ein Nahfoto ermöglichen: Der Schüler wählt die Stelle als Rechteck auf der Seite, das Nahfoto läuft durch denselben Auftrag „Seitenerkennung“, und das Ergebnis ersetzt die erzeugten Elemente, die ganz im Rechteck liegen. Eigene Ergänzungen und das Original bleiben unverändert. Das Ersetzen ist ein Undo-Schritt; ein Fehler oder Abbruch ändert die Seite nicht.
- **FR-022**: Jede Seite MUST eine Liste von Anhängen haben. Ein Anhang ist eine Datei mit Namen, Typ und Größe; das Original eines importierten Fotos ist der Anhang der Art „Original“ und liegt unverändert, ungedreht und in voller Auflösung dort. Anhänge liegen nicht als Ebene oder Element auf der Seite, tauchen nicht in der Seitenliste auf und zählen nicht als Seite.
- **FR-023**: Der Schüler MUST einen Anhang auf zwei Arten öffnen können: in einem Seitenfenster neben der Seite (die Seite bleibt bearbeitbar) oder im Vollbild. Im Vollbild ist nur der Anhang zu sehen, nie die Seite, ihr Entwurf oder das Original; ein Schließen führt zur Seite zurück. Bilder haben Zoom und Verschieben; andere Dateien zeigen Name, Typ und Größe und lassen sich sichern. Im Vollbild lässt sich ein Anhang öffnen, ohne dass der Schüler dabei die Seite oder ein anderes Original ins Bild bekommt (Übungsblatt mit Fragen, dessen Antworten auf der Seite stehen).
- **FR-024**: Der Schüler MUST Dateien an eine Seite hängen, einen Anhang als Datei sichern und einen Anhang entfernen können. Der Anhang „Original“ ist gegen Entfernen geschützt; er geht nur mit der Seite. Anhänge sind Teil dessen, was mit der Seite gespeichert, geteilt und aus dem Papierkorb wiederhergestellt wird.
- **FR-025**: Entsteht eine Seite ohne Entwurf (Erkennung fehlgeschlagen, abgelehnt oder abgebrochen), MUST das Foto als gewöhnliches Bild sichtbar auf der Seite stehen, damit der Schüler etwas hat, auf dem er arbeiten kann; es bleibt zusätzlich der Anhang „Original“. Ein Nahfoto über die ganze Seite liest das Blatt dann neu (FR-021).
- **FR-026**: Das PDF MUST nur die Seite zeigen, wie sie zu sehen ist; Anhänge gehören nicht hinein. Die Option „Original einbeziehen“ des Exports entfällt, weil das Original keine Ebene mehr ist.
- **FR-027**: Der Schüler MUST Vorlage (blanko, kariert 5 mm und 10 mm, liniert, Punktraster, Lineaturen, Notenlinien, Millimeterpapier) und Papierfarbe einer Seite nachträglich ändern können. Die Wahl ist ein Schritt, den Rückgängig zurücknimmt, wird mit der Seite gespeichert und im PDF gezeigt.
- **FR-028**: Der Schüler MUST den Rand einer Seite (oben, unten, links, rechts) nachträglich ändern können. Der Rand bestimmt, wo Linien und Karos der Vorlage gezeichnet werden und wo neue Inhalte stehen dürfen; vorhandene Inhalte bleiben, wo sie stehen. Er wird mit der Seite gespeichert, ist rückgängig zu machen und gilt im PDF.
- **FR-029**: Das System MUST Formeln der Mathematik, Chemie und Physik, die auf einer Seite als eigene Zeile oder allein in einer Tabellenzelle stehen, als LaTeX erkennen und als gesetzte Formel übernehmen (`latex`-Element mit editierbarer Quelle; Chemie in der Schreibweise `\ce{…}`). Eine Formel mitten im Satz bleibt Text in Unicode; Strukturformeln und Skizzen bleiben Bilder. Die gesetzte Formel steht in der Tintenfarbe der Seite, nicht in Handschrift.
- **FR-030**: Das System MUST eine Formel, deren LaTeX-Quelle sich nicht setzen lässt, nicht verlieren und nicht als Fehler zeigen: Die Quelle bleibt als gewöhnlicher Text stehen, den der Schüler bearbeiten kann. Das Erkennungsformat zählt Formeln als eigene Art von Region; bei einer Tabelle steht eine Formelzelle als `$…$` in der Zelle.
- **FR-031**: Der Schüler MUST eine Formel bearbeiten können: Ein Doppelklick auf eine gesetzte Formel öffnet ihre LaTeX-Quelle mit einer Vorschau, die beim Tippen folgt; eine Quelle, die sich nicht setzen lässt, wird nicht übernommen und zeigt den Grund. Das Übernehmen ist ein Schritt, den Rückgängig zurücknimmt. Eine Tabellenzelle mit `$…$` verhält sich ebenso.
- **FR-032**: Der Schüler MUST eine Formel von Hand einfügen können: ein Knopf in der Werkzeugleiste legt eine Formel an der Seite ab und öffnet ihre Quelle. Verschieben und Skalieren gelten wie bei Text und Bild.
- **FR-033**: Gesetzte Formeln MUST im PDF so stehen wie auf der Seite, nach Speichern und Öffnen erhalten bleiben und beim Teilen mitgehen (das gesetzte SVG liegt im Seiten-JSON).

### Key Entities

- **Notizimport**: Ein vom Schüler hinzugefügtes Originalfoto einschließlich Aufnahme- und Seiteninformationen.
- **Anhang**: Eine Datei, die an einer Seite hängt. Das Original ist der Anhang der Art „Original“: die unveränderte Referenzdarstellung des importierten Fotos, die nicht durch automatische Korrekturen überschrieben wird und nicht entfernt werden kann.
- **Inhaltsbereich**: Ein räumlich abgegrenzter erkannter Bereich mit Rolle, Position und optionaler Verbindung zum Originalausschnitt.
- **Textbereich**: Ein bearbeitbarer Textinhalt mit Erkennungsergebnis und verwendeter Handschrift-Darstellung.
- **Visuelles Objekt**: Eine Zeichnung, ein Diagramm, eine Tabelle oder ein anderer nicht als Text behandelter Bereich, der unabhängig im Canvas bearbeitet werden kann.
- **Canvas-Entwurf**: Die zusammengesetzte, speicherbare Seite aus Textbereichen, visuellen Objekten, Papier und Seitenrand und Sichtbarkeitseinstellungen; Anhänge hängen an ihr, gehören aber nicht zu ihrem Inhalt.
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
- **SC-006**: Bei einem Ausfall der automatischen Verarbeitung bleibt das Originalbild für 100 % der erfolgreich importierten Testseiten als Anhang zugänglich und der Nutzer kann den Entwurf manuell weiterbearbeiten.
- **SC-007**: Eine spätere Pilotbewertung kann die wahrgenommene Übersichtlichkeit und Bearbeitbarkeit erfassen; sie ist kein technisches Release-Gate dieser Ausbaustufe.
- **SC-008**: In einem Heft aus N importierten Fotos hat die Seitenliste genau N Einträge und jede Seite zeigt höchstens eine Fassung des Inhalts; das Original jeder Seite ist in höchstens zwei Bedienschritten geöffnet (Anhänge, Anhang), im Seitenfenster oder im Vollbild.
- **SC-009**: Auf den annotierten Referenz-Fixtures mit Formeln wird jede Formel, die als eigene Zeile oder allein in einer Zelle steht, entweder als gesetzte Formel übernommen oder als Text erhalten; keine geht verloren (FR-030). Die Genauigkeit der LaTeX-Quelle hängt vom Modell ab und ist kein Release-Gate; sie wird je Modell gemessen und im PR festgehalten.

## Assumptions

- Die erste Version erhält Bilder über den Dateiauswahldialog der plattformunabhängigen holzi-Runtime; eine verfügbare Kamera kann zusätzlich unterstützt werden. Eine zusätzliche Scanner- oder Tafel-App ist nicht erforderlich.
- Der Schüler bearbeitet automatisch erzeugte Inhalte selbst. Die Anwendung stellt einen korrigierbaren Entwurf bereit, keine verbindliche Transkription.
- Bilder werden einzeln verarbeitet und jeweils einer Notizseite zugeordnet; Batch- und Mehrseitenimport sind nicht Teil der ersten Ausbaustufe.
- Die Auswahl und Lizenzierung der verfügbaren Handschrift-Darstellungen wird vor der Veröffentlichung separat geprüft.
- Wo die Erkennung läuft, entscheidet der Nutzer in holzi: lokal (ein Bildmodell über Ollama auf demselben Gerät, Spec 055 research R8), auf einem Server im eigenen Netz oder in der Cloud. Qualität geht vor Offline.
- Für die Erkennung braucht es ein Modell, das Bilder annimmt. Gemessen: `qwen3-vl:4b-instruct` über Ollama liest eine Seite auf einer 8-GB-Grafikkarte in 12–18 s; ein reines Textmodell kann nur den Korrekturschritt übernehmen.
- haex-notes nutzt vault-sdk ab 4.7.1 (`models.runAsync`; die Wiederholung nach der Antwort des Nutzers auf eine Frage von holzi).
- Der aktuelle haex-notes-Canvas wird als Zielumgebung verwendet, ohne dadurch eine bestimmte Erkennungs- oder KI-Technologie festzuschreiben.
