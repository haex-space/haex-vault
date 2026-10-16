# Feature Specification: Erweiterungen nutzen holzis Modelle

**Feature Branch**: `055-extension-model-access`

**Created**: 2026-10-16

**Status**: Draft

**Input**: User description: "Erweiterungen nutzen holzis Modelle: Eine Erweiterung (zuerst haex-notes) darf mit ausdrücklicher Berechtigung Anfragen an die Sprach- und Bildmodelle von holzi stellen, statt eigene Zugänge mitzubringen. Der Nutzer legt pro Erweiterung fest, welche Modelle erlaubt sind und mit welchem Aufwand (Effort) gesprochen werden darf; ohne Freigabe geht nichts. Anfragen können Text und Bilder enthalten und liefern ein Ergebnis in einem vorab vereinbarten, geprüften Format; freier Modelltext gilt nicht als Erfolg. Ein Auftrag kann aus mehreren Schritten mit unterschiedlichen Modellen bestehen. Offensichtlich kaputte Modellausgabe wird technisch verworfen. Lokal zuerst; sonst ein anderes Gerät, ein Server oder die Cloud, mit sichtbarer Einwilligung vor dem ersten Versand. Qualität geht vor Offline. Das Handy nimmt nur das Foto auf. Dazu gehört ein allgemeiner OpenAI-kompatibler Anbieter, und lokale Modelle müssen Bilder annehmen können. Erstes Anwendungsbeispiel ist die Digitalisierung handschriftlicher Mitschriften und Arbeitsblätter in haex-notes (Spec 046); Spec 047 wird dadurch verallgemeinert."

## User Scenarios & Testing

### User Story 1 - Eine Erweiterung stellt eine Anfrage an ein Modell (Priority: P1)

Eine Erweiterung schickt holzi eine Anfrage mit Text und/oder Bildern. Sie nennt, was sie braucht (zum Beispiel „eine Seite lesen“ oder „einen Text korrigieren“), und in welchem Format sie das Ergebnis erwartet. holzi führt die Anfrage mit einem Modell aus, das der Nutzer für diese Erweiterung freigegeben hat, und liefert das geprüfte Ergebnis zurück, zusammen mit der Angabe, was tatsächlich verwendet wurde.

**Why this priority**: Ohne diese Schnittstelle müsste jede Erweiterung eigene Zugänge, Schlüssel und Modellformate mitbringen. Das ist der Kern von allem Weiteren.

**Independent Test**: Eine Testerweiterung mit Freigabe sendet ein Bild und einen Auftrag mit Ergebnisformat. Sie erhält ein formatgerechtes Ergebnis, ohne dass ein externer Dienst aufgerufen wird (Testmodell).

**Acceptance Scenarios**:

1. **Given** eine Erweiterung mit Freigabe für ein Modell, **When** sie eine Anfrage stellt, **Then** erhält sie ein Ergebnis im vereinbarten Format und die Angabe, welches Modell, welcher Aufwand und welcher Ort verwendet wurden.
2. **Given** eine Erweiterung ohne Freigabe, **When** sie eine Anfrage stellt, **Then** wird nichts verarbeitet und sie erhält einen klaren Fehler.
3. **Given** eine beliebige Anfrage, **When** sie ausgeführt wird, **Then** sieht die Erweiterung zu keinem Zeitpunkt Zugangsdaten, Schlüssel oder Adressen interner Dienste.

---

### User Story 2 - Der Nutzer bestimmt pro Erweiterung Modelle und Aufwand (Priority: P1)

In den Einstellungen der Erweiterung sieht der Nutzer die Modelle, die holzi kennt, und legt fest, welche die Erweiterung nutzen darf und mit welchem höchsten Aufwand (Effort). Ohne Freigabe hat die Erweiterung keinen Zugriff. Fragt eine Erweiterung zum ersten Mal an, fragt holzi den Nutzer, statt stillschweigend zu verarbeiten.

**Why this priority**: Der Nutzer behält die Kontrolle darüber, welche Erweiterung welche Modelle, auch bezahlte, und wie intensiv anspricht.

**Independent Test**: Mit zwei Modellen wird nur eines freigegeben. Anfragen an das freigegebene gelingen, Anfragen an das andere werden abgelehnt. Nach dem Widerruf scheitert auch die nächste Anfrage an das erste.

**Acceptance Scenarios**:

1. **Given** eine Erweiterung ohne Freigaben, **When** sie zum ersten Mal anfragt, **Then** fragt holzi den Nutzer, welche Modelle und welchen Aufwand er erlaubt.
2. **Given** eine Freigabe mit höchstem Aufwand „mittel“, **When** die Erweiterung „hoch“ verlangt, **Then** wird der Aufwand auf „mittel“ begrenzt und die Herkunftsangabe nennt den tatsächlichen Aufwand.
3. **Given** ein widerrufenes Modell, **When** die Erweiterung es bei der nächsten Anfrage nennt, **Then** wird die Anfrage abgelehnt; eine bereits laufende Anfrage darf zu Ende laufen oder abgebrochen werden.
4. **Given** mehrere freigegebene Modelle, **When** die Erweiterung nur nennt, was sie braucht, **Then** wählt holzi das nach der Rangfolge des Nutzers passende Modell.

---

### User Story 3 - Ergebnis im vereinbarten Format, mehrere Schritte, kaputte Ausgabe verworfen (Priority: P1)

Jeder Auftrag hat ein vorab vereinbartes Ergebnisformat. Ein Auftrag kann aus mehreren Schritten bestehen, die verschiedene Modelle nutzen, zum Beispiel erst ein Bildmodell, das eine Seite liest, dann ein Textmodell, das Rechtschreibung und Lesefehler korrigiert. Ausgabe, die nicht zum Format passt oder offensichtlich kaputt ist, wird nicht als Ergebnis geliefert.

**Why this priority**: Erweiterungen bauen auf dem Ergebnis auf. Freier Modelltext oder Endlosschleifen würden Schülern Unsinn in ihre Notizen schreiben.

**Independent Test**: Ein Testmodell liefert nacheinander gültige, formatwidrige, leere, sich wiederholende und in eine andere Schrift wechselnde Ausgabe. Nur die gültige wird als Ergebnis geliefert, in allen anderen Fällen erhält die Erweiterung einen strukturierten Fehler.

**Acceptance Scenarios**:

1. **Given** ein Auftrag mit zwei Schritten, **When** Schritt 1 ein Ergebnis liefert, **Then** erhält Schritt 2 dieses Ergebnis als Eingabe und die Herkunftsangabe nennt für jeden Schritt das verwendete Modell.
2. **Given** eine Ausgabe, die nicht zum Format passt, **When** holzi sie prüft, **Then** wird sie verworfen und die Erweiterung erhält einen Fehler, der „unbrauchbar“ von „nicht verfügbar“ unterscheidet.
3. **Given** eine Ausgabe aus einer Schleife (dieselbe Wendung vielfach wiederholt), einem Wechsel in eine ganz andere Schrift oder aus leerem Text, **When** holzi sie prüft, **Then** wird sie verworfen.
4. **Given** ein Korrekturschritt, **When** er einen Text bearbeitet, **Then** ändert er nur Rechtschreibung und eindeutige Lesefehler; Abkürzungen, Wortwahl und Zeilenumbrüche des Schreibers bleiben erhalten.
5. **Given** ein verworfenes Ergebnis, **When** die Erweiterung es bemerkt, **Then** zeigt der Nutzer in holzi keine Markierung am Text; die Erweiterung entscheidet selbst, was sie anzeigt (etwa den Bereich leer lassen).

---

### User Story 4 - Auftragsdaten verlassen das Gerät nur mit Einwilligung (Priority: P1)

holzi nutzt zuerst ein lokales Modell, wenn eines passt, und die Auftragsdaten bleiben auf dem Gerät. Sollen Text oder Bilder an ein anderes Gerät, einen Server oder einen Cloud-Anbieter gehen, sieht der Nutzer vor dem ersten Versand an dieses Ziel, welche Daten übertragen werden und wohin, und willigt ein oder lehnt ab.

**Why this priority**: Schülernotizen können personenbezogene oder schulische Inhalte enthalten. Qualität geht vor Offline, aber nie ohne Wissen des Nutzers.

**Independent Test**: Eine Anfrage mit Text oder Bild an ein Cloud-Modell ohne Einwilligung sendet nichts. Eine Anfrage an ein lokales Modell erzeugt keine Netzwerkanfrage nach außen.

**Acceptance Scenarios**:

1. **Given** ein lokales und ein Cloud-Modell sind freigegeben und das lokale genügt, **When** eine Anfrage mit Bild kommt, **Then** läuft sie lokal und kein Bildbyte verlässt das Gerät.
2. **Given** nur ein Ziel außerhalb des Geräts ist freigegeben, **When** zum ersten Mal Text oder ein Bild dorthin soll, **Then** zeigt holzi die Datenübertragung und das Ziel und fragt um Einwilligung, bevor etwas gesendet wird.
3. **Given** eine für die Sitzung erteilte Einwilligung, **When** spätere Anfragen derselben Erweiterung dasselbe Ziel nutzen, **Then** wird in dieser Sitzung nicht erneut gefragt; nach einem Neustart von holzi fragt es wieder.
4. **Given** eine dauerhaft erteilte Einwilligung, **When** spätere Anfragen dasselbe Ziel nutzen, **Then** wird nicht mehr gefragt, bis der Nutzer sie in den Einstellungen widerruft.
5. **Given** ein lokales Modell ist nicht verfügbar, **When** ein freigegebenes Ziel außerhalb des Geräts existiert, **Then** wechselt holzi nicht stillschweigend dorthin, sondern nur zu einem Ziel mit erteilter Einwilligung, sonst mit Rückfrage.

---

### User Story 5 - Ein Modell auf einem anderen Gerät, auf einem Server oder in der Cloud nutzen (Priority: P2)

Der Nutzer kann holzi um einen allgemeinen Anbieter nach verbreitetem Schnittstellenstandard („OpenAI-kompatibel“) ergänzen, indem er eine Adresse und einen Zugangsschlüssel einträgt. So lassen sich ein eigener Rechner mit Grafikkarte, ein Server (etwa ein Schul-Server mit lokalem Modell) oder ein Cloud-Dienst mit dieser Schnittstelle nutzen. Das Handy nimmt nur das Foto auf; die Analyse läuft dort, wo der Nutzer sie erlaubt hat.

**Why this priority**: Auf dem Handy ist ein lokales Bildmodell heute unbefriedigend. Schüler brauchen einen Weg, ihre Fotos auf einem leistungsfähigeren Ziel auswerten zu lassen.

**Independent Test**: Ein Testserver mit dieser Schnittstelle wird als Anbieter eingetragen. Er erscheint wie andere Modelle in der Freigabeliste, und eine freigegebene Erweiterung erhält darüber ein Ergebnis.

**Acceptance Scenarios**:

1. **Given** ein eingetragener Anbieter mit Adresse und Schlüssel, **When** der Nutzer ihn speichert, **Then** zeigt holzi dessen Modelle in der Freigabeliste, mit der Angabe, ob sie Bilder annehmen (erkannt oder vom Nutzer angegeben).
2. **Given** ein Foto auf dem Handy und kein erlaubtes Ziel erreichbar, **When** die Analyse gestartet wird, **Then** erhält die Erweiterung einen klaren Fehler und das Foto bleibt unverändert erhalten.
3. **Given** ein falscher Schlüssel oder eine nicht erreichbare Adresse, **When** eine Anfrage ankommt, **Then** erhält die Erweiterung einen Fehler, der die Ursache nennt, ohne den Schlüssel preiszugeben.

---

### User Story 6 - Lokale Modelle verstehen Bilder (Priority: P2)

Der Nutzer kann ein lokales Modell wählen, das Bilder annimmt. Ein reines Textmodell wird für Bildaufträge vor dem Senden abgelehnt.

**Why this priority**: Eine vollständig lokale Auswertung von Fotos ist das bevorzugte Ziel, heute können lokale Modelle in holzi nur Text.

**Independent Test**: Mit einem lokalen bildfähigen Testmodell läuft ein Bildauftrag ohne Netzwerkzugriff durch. Mit einem reinen Textmodell wird derselbe Auftrag vor jeder Verarbeitung abgelehnt.

**Acceptance Scenarios**:

1. **Given** ein lokales bildfähiges Modell, **When** eine Erweiterung ein Bild sendet, **Then** liest es das Bild und liefert ein formatgerechtes Ergebnis.
2. **Given** ein reines Textmodell, **When** ein Bildauftrag darauf zielt, **Then** wird er abgelehnt, bevor Bilddaten übertragen werden, und die Erweiterung erfährt den Grund.

---

### User Story 7 - Seitenerkennung als erstes Auftragsformat (Priority: P2)

Das erste Auftragsformat dient haex-notes: Aus dem Foto einer Seite entsteht ein Ergebnis mit Regionen. Jede Region hat eine Art (Text, Bild, Tabelle), eine Position auf der Seite und eine Lesereihenfolge. Textregionen enthalten den Text, Tabellenregionen Zeilen und Zellen.

**Why this priority**: Ohne ein konkretes, nützliches Format bliebe die Schnittstelle abstrakt. Dieses Format löst den Fall der Schüler: Mitschriften und Arbeitsblätter digitalisieren.

**Independent Test**: Ein Testmodell liefert für ein Seitenfoto ein Ergebnis mit je einer Text-, Bild- und Tabellenregion. Es wird gegen das Format geprüft, eine Tabelle mit fehlender Zelle wird verworfen.

**Acceptance Scenarios**:

1. **Given** ein Seitenfoto, **When** der Auftrag läuft, **Then** liefert das Ergebnis Regionen mit Art, Position und Lesereihenfolge.
2. **Given** eine Tabelle auf der Seite, **When** sie erkannt wird, **Then** enthält das Ergebnis Zeilen und Zellen, auch leere Zellen, und nicht nur ein Bild der Tabelle.
3. **Given** ein auf der Seite liegendes Foto, **When** der Auftrag läuft, **Then** wird die Seite aufrecht ausgewertet, oder die Erweiterung erhält die Angabe, dass sie gedreht werden muss.

### Edge Cases

- Das gewählte Modell ist nicht verfügbar, etwa weil das lokale Modell nicht installiert ist. Es gibt keinen stillen Wechsel zu einem Ziel ohne Einwilligung.
- Das Modell liefert gültigen Aufbau, aber offensichtlich erfundene Inhalte auf unleserlichem Material. holzi erkennt das nicht in jedem Fall; es verwirft nur Schleifen, Schriftwechsel, leere und formatwidrige Ausgabe. Alles andere liefert die Erweiterung.
- Die Anfrage dauert zu lange oder wird abgebrochen. Die Erweiterung erhält einen eindeutigen Status und kann ihr Original weiter verwenden.
- Zwei Erweiterungen fragen gleichzeitig an. Jede wird nur nach ihren eigenen Freigaben behandelt, Ergebnisse vermischen sich nie.
- Der Nutzer entfernt eine Freigabe, während ein Auftrag mit mehreren Schritten läuft. Der nächste Schritt wird nicht mehr ausgeführt, die Erweiterung erhält einen Fehler.
- Ein Schritt braucht ein Modell, das der Nutzer nicht freigegeben hat, obwohl andere Schritte freigegebene Modelle haben. Der Auftrag scheitert vor dem ersten Schritt, nicht mittendrin.
- Das Bild ist sehr groß. Die Anfrage wird abgelehnt oder verkleinert, mit klarer Meldung und ohne Verlust des Originals.
- Ein Zugangsschlüssel oder eine Serveradresse darf nie in Fehlermeldungen, Ergebnissen oder dem Speicher der Erweiterung auftauchen.

## Requirements

### Functional Requirements

- **FR-001**: holzi MUST einer berechtigten, aktiven Erweiterung erlauben, Anfragen mit Text und/oder Bildern an Modelle von holzi zu stellen.
- **FR-002**: Eine Erweiterung ohne Freigabe MUST abgelehnt werden, bevor ein Modell angesprochen wird.
- **FR-003**: Der Nutzer MUST pro Erweiterung festlegen können, welche Modelle sie nutzen darf und mit welchem höchsten Aufwand; ohne ausdrückliche Freigabe MUST kein Modell nutzbar sein.
- **FR-004**: Fragt eine Erweiterung zum ersten Mal an, MUST holzi den Nutzer um Freigabe bitten, statt stillschweigend zu verarbeiten oder zu scheitern. Die Abfrage folgt den übrigen Berechtigungsabfragen von holzi: Erlauben gilt für die Sitzung oder dauerhaft.
- **FR-005**: Ein Widerruf MUST ab der nächsten Anfrage wirken.
- **FR-006**: Verlangt eine Erweiterung mehr Aufwand als erlaubt, MUST er auf das Erlaubte begrenzt werden; das Ergebnis MUST den tatsächlichen Aufwand nennen.
- **FR-007**: Nennt eine Erweiterung nur, was sie braucht, MUST holzi unter den freigegebenen Modellen nach der Rangfolge des Nutzers das passende wählen.
- **FR-008**: Jeder Auftrag MUST ein versioniertes Ergebnisformat haben; freier Modelltext MUST NICHT als Erfolg gelten.
- **FR-009**: Ein Auftrag MUST aus mehreren Schritten mit je eigenem Modell bestehen können; das Ergebnis eines Schritts MUST dem nächsten als Eingabe dienen.
- **FR-010**: holzi MUST Ausgabe, die nicht zum Format passt, leer ist, sich in Schleifen wiederholt oder in eine andere Schrift wechselt, verwerfen und der Erweiterung einen strukturierten Fehler liefern.
- **FR-011**: Der Fehler für unbrauchbare Ausgabe MUST von „Modell nicht verfügbar“, „nicht freigegeben“, „Zeitüberschreitung“ und „abgebrochen“ unterscheidbar sein.
- **FR-012**: holzi MUST für verworfene Ausgabe keine Markierung im Text des Nutzers erzeugen; die Darstellung bleibt der Erweiterung überlassen.
- **FR-013**: Ein Korrekturschritt MUST nur Rechtschreibung und eindeutige Lesefehler ändern und Abkürzungen, Wortwahl und Zeilenumbrüche erhalten.
- **FR-014**: holzi MUST lokale Modelle bevorzugen, wenn eines freigegeben und für den Auftrag geeignet ist; Bilddaten MUST dann das Gerät nicht verlassen.
- **FR-015**: Vor dem ersten Versand von Auftragsdaten (Text oder Bilder) an ein Ziel außerhalb des Geräts MUST holzi die Datenübertragung und das Ziel sichtbar machen und die Einwilligung des Nutzers einholen; ohne Einwilligung MUST nichts gesendet werden.
- **FR-016**: Die Einwilligung MUST wie jede andere Berechtigungsabfrage in holzi funktionieren: Sie gilt pro Erweiterung und Ziel und wird für die Sitzung oder dauerhaft erteilt. Eine dauerhafte Einwilligung MUST in den Einstellungen widerrufbar sein, eine Sitzungs-Einwilligung endet mit dem Neustart von holzi.
- **FR-017**: holzi MUST NICHT stillschweigend von einem lokalen Modell auf ein Ziel ohne erteilte Einwilligung wechseln.
- **FR-018**: Der Nutzer MUST einen allgemeinen Anbieter nach einem verbreiteten Schnittstellenstandard (OpenAI-kompatibel) mit Adresse und Zugangsschlüssel eintragen können; dessen Modelle MUST in der Freigabeliste erscheinen.
- **FR-019**: Zu jedem Modell MUST bekannt sein, ob es Bilder annimmt (erkannt oder vom Nutzer angegeben); Bildaufträge an Modelle ohne Bildfähigkeit MUST vor der Übertragung abgelehnt werden.
- **FR-020**: Lokale Modelle MUST Bilder annehmen können, wenn das Modell dazu fähig ist.
- **FR-021**: Zugangsdaten, Schlüssel und Adressen interner Dienste MUST der Erweiterung, Fehlermeldungen, Ergebnissen und synchronisierten Daten fernbleiben.
- **FR-022**: Der Nutzer MUST laufende Aufträge abbrechen können; ein Abbruch oder Fehler MUST die Originaldaten der Erweiterung unverändert lassen.
- **FR-023**: Das Ergebnis MUST die Herkunft nennen: Auftragsformat samt Version, je Schritt Modell, Aufwand und Ort (lokal, anderes Gerät, Server, Cloud).
- **FR-024**: Das Auftragsformat „Seitenerkennung“ MUST Regionen mit Art (Text, Bild, Tabelle), Position, Lesereihenfolge und Inhalt liefern; Tabellen MUST Zeilen und Zellen enthalten.
- **FR-025**: Ein Auftrag mit mehreren Schritten MUST vor dem ersten Schritt prüfen, dass für alle Schritte ein freigegebenes Modell vorhanden ist.

### Key Entities

- **Modellfreigabe**: Pro Erweiterung die erlaubten Modelle, deren Rangfolge und der höchste Aufwand.
- **Auftrag**: Eine Anfrage einer Erweiterung mit Eingabe (Text, Bilder), Ergebnisformat und einem oder mehreren Schritten.
- **Schritt**: Ein einzelner Modellaufruf eines Auftrags mit benötigter Fähigkeit (zum Beispiel Bild lesen, Text korrigieren).
- **Auftragsformat**: Versioniertes Eingabe- und Ergebnisformat; das erste ist „Seitenerkennung“.
- **Anbieter**: Eine Quelle für Modelle: lokal, ein Konto bei einem Cloud-Dienst, die Anbindung eines Befehlszeilen-Agenten oder ein allgemeiner OpenAI-kompatibler Dienst mit Adresse und Schlüssel.
- **Fähigkeit**: Aussage, ob ein Modell Bilder, strukturierte Ausgabe oder Werkzeuge kann.
- **Ziel**: Wo ein Auftrag verarbeitet wird: dieses Gerät, ein anderes Gerät, ein Server oder die Cloud.
- **Einwilligung**: Die Zustimmung des Nutzers pro Erweiterung und Ziel, Auftragsdaten dorthin zu senden; für die Sitzung oder dauerhaft.
- **Herkunft**: Nicht geheime Angaben, welches Format, Modell, Aufwand und Ziel tatsächlich verwendet wurden.

## Out of Scope

- Der Betrieb eines Schul-Servers: Konten für Schüler, Datenschutz bei Minderjährigen, Kapazität und Kosten.
- Das Canvas in haex-notes (Tafelbild, Neuanordnen, Bearbeiten, PDF-Export); das bleibt Spec 046.
- Die Wahl des konkreten Modells. Sie ist eine Einstellung und wird gemessen, nicht festgeschrieben.
- Ein allgemeiner Chat für Erweiterungen mit Gesprächsverlauf und freie Agenten mit beliebigen Werkzeugen.
- Persönliche Handschriftprofile und Training mit Schülerdaten.
- Das Weiterleiten von Aufträgen an ein anderes Gerät desselben Nutzers über den Tresor. Aufträge laufen auf einem eingetragenen Anbieter.

## Success Criteria

### Measurable Outcomes

- **SC-001**: 100 % der Anfragen einer Erweiterung ohne Freigabe werden abgelehnt, bevor ein Modell angesprochen wird.
- **SC-002**: Ein Widerruf wirkt bei der nächsten Anfrage; in keinem Testfall wird danach noch verarbeitet.
- **SC-003**: Bei lokaler Verarbeitung verlässt in einem überwachten Test kein Bildbyte das Gerät.
- **SC-004**: Ohne Einwilligung startet keine Übertragung an ein Ziel außerhalb des Geräts; die Abfrage nennt das Ziel in 100 % der Fälle.
- **SC-005**: In einer Testreihe mit gültiger, formatwidriger, leerer, sich wiederholender und schriftwechselnder Ausgabe wird 100 % der unbrauchbaren Ausgabe verworfen und nie als Ergebnis geliefert.
- **SC-006**: 100 % der Bildaufträge an ein Modell ohne Bildfähigkeit werden vor der Übertragung abgelehnt.
- **SC-007**: Ein Nutzer trägt einen neuen Anbieter (Adresse, Schlüssel) in unter zwei Minuten ein und sieht danach dessen Modelle in der Freigabeliste.
- **SC-008**: Nach Abbruch oder Fehler ist das von der Erweiterung gespeicherte Original in 100 % der Fehlerfälle unverändert nutzbar.
- **SC-009**: Eine Testseite mit Text, Bild und Tabelle liefert ein formatgültiges Ergebnis; die Tabelle enthält alle Zeilen und Zellen.
- **SC-010**: Ein Wechsel des Modells für einen Auftrag erfordert weder Änderungen an der Erweiterung noch Datenmigrationen.

## Assumptions

- holzi hat bereits eine gemeinsame Verwaltung von Anbietern und Modellen (lokal, Konto bei einem Cloud-Dienst, Befehlszeilen-Agent); diese Spec ergänzt den Zugriff von Erweiterungen, den allgemeinen Anbieter und Bildfähigkeit für lokale Modelle.
- Das Auftragsformat „Seitenerkennung“ ersetzt das feste Format `paper-note-recognition` aus Spec 047; die dort gebaute Hostschnittstelle (Start, Abbruch, Ergebnisprüfung, Ereignis bei Abschluss) wird weiterverwendet und verallgemeinert.
- Einwilligung und erste Freigabe nutzen die Berechtigungsabfrage, die holzi für Erweiterungen schon hat: Der Nutzer erlaubt für die Sitzung oder dauerhaft und kann in den Einstellungen widerrufen. Ein Auftrag fragt nicht jedes Mal neu.
- Wählt die Erweiterung kein Modell, entscheidet holzi nach der Rangfolge, die der Nutzer pro Erweiterung festlegt.
- Verlangter Aufwand über dem Erlaubten wird begrenzt und nicht abgelehnt; das Ergebnis nennt den tatsächlichen Aufwand.
- Die Qualität einzelner Modelle bei Handschrift, Tabellen und Positionen wird gesondert gemessen (Testfotos liegen vor); die Wahl eines Modells ist daher keine Voraussetzung dieser Spec.
- Das Handy nimmt nur das Foto auf; es läuft dort kein lokales Bildmodell. Wo das Foto ausgewertet wird, bestimmen die freigegebenen Modelle und Ziele.
- Ein Schul-Server ist für holzi nur ein weiterer Anbieter mit Adresse und Schlüssel.
- Das Wiederholen verworfener Aufträge (Warteschlange, Wiederholversuche) gehört der Erweiterung; holzi liefert nur den Fehler.
