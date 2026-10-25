# Feature Specification: Erweiterungen nutzen holzis Modelle

**Feature Branch**: `055-extension-model-access`

**Created**: 2026-10-16

**Status**: Draft

**Input**: User description: "Erweiterungen nutzen holzis Modelle: Eine Erweiterung (zuerst haex-notes) darf mit ausdrücklicher Berechtigung Anfragen an die Sprach- und Bildmodelle von holzi stellen, statt eigene Zugänge mitzubringen. Der Nutzer legt pro Erweiterung fest, welche Modelle erlaubt sind und mit welchem Aufwand (Effort) gesprochen werden darf; ohne Freigabe geht nichts. Anfragen können Text und Bilder enthalten und liefern ein Ergebnis in einem vorab vereinbarten, geprüften Format; freier Modelltext gilt nicht als Erfolg. Ein Auftrag kann aus mehreren Schritten mit unterschiedlichen Modellen bestehen. Offensichtlich kaputte Modellausgabe wird technisch verworfen. Lokal zuerst; sonst ein anderes Gerät, ein Server oder die Cloud, mit sichtbarer Einwilligung vor dem ersten Versand. Qualität geht vor Offline. Das Handy nimmt nur das Foto auf. Dazu gehört ein allgemeiner OpenAI-kompatibler Anbieter, und lokale Modelle müssen Bilder annehmen können. Erstes Anwendungsbeispiel ist die Digitalisierung handschriftlicher Mitschriften und Arbeitsblätter in haex-notes (Spec 046); Spec 047 wird dadurch verallgemeinert."

## Clarifications

### Session 2026-10-17

- Q: Wer legt fest, was ein Auftrag dem Modell sagt? → A: Die Erweiterung liefert eigene Anweisung und eigenes Ergebnisformat; holzi prüft nur Berechtigung, Format und Ausgabe (Option B).
- Q: Was passiert, wenn das bevorzugte Modell ausfällt oder Unbrauchbares liefert? → A: holzi versucht automatisch das nächste Modell der Rangfolge, aber nur mit schon erteilter Einwilligung für dessen Ziel (Option B).
- Q: Was darf holzi von den Inhalten der Aufträge aufbewahren? → A: Nichts, auch keine Metadaten (Option A).
- Q: Was gilt für Größe und Dauer eines Auftrags? → A: holzi legt Obergrenzen für Bildgröße, Bildzahl und Dauer fest; die Erweiterung darf sie unterschreiten, nicht überschreiten; der Nutzer kann die Dauer pro Erweiterung ändern (Option B).
- Q: Was darf eine Erweiterung über die Modelle in holzi erfahren? → A: Die Liste der für sie freigegebenen Modelle mit Namen und Fähigkeiten, ohne Adressen und Schlüssel (Option C).

### Session 2026-10-20

- Q: Dürfen Aufträge Werkzeuge nutzen, etwa Webabrufe über ein lokales Modell? → A: Ja, aber nicht in dieser Spec. Eine eigene Spec regelt das zusammen mit holzis späterer Websuche: Die Erweiterung muss jedes Werkzeug in ihrem Manifest erklären, der Nutzer gibt es frei, und zuerst gibt es nur Web-Werkzeuge. Bis dahin gilt FR-026.

### Session 2026-10-23

- Q: Welche Modelle aus holzi darf der Nutzer einer Erweiterung freigeben? → A: Jedes Modell, das holzi kennt, unabhängig von seiner Quelle: lokal installierte Modelle, Modelle über Modell-Server oder ein Konto bei einem Cloud-Dienst und verbundene Abos (Claude Code, Codex). Freigabe, Rangfolge und Aufwand gelten für alle gleich (US8).
- Q: Mit welchem Zugriff läuft ein Abo (Befehlszeilen-Agent) in einem Auftrag? → A: Mit dem vollen Zugriff, den der Agent in holzi hat; holzi schränkt ihn für Aufträge nicht zusätzlich ein. Die Freigabe macht sichtbar, dass die Erweiterung damit einen Agenten mit Datei- und Befehlszugriff anweist (FR-026, FR-032).
- Q: Was gilt im Autonomie-Modus „Standard“, wenn es im Auftrag keinen Chat zum Bestätigen gibt? → A: Zuerst „wie Gegated (permissiv)“; das geht nicht, weil dieser Modus jeden Aufruf verweigert, für den er keinen Chat-Verlauf zum Protokollieren hat, und ein Auftrag nichts protokollieren darf (FR-028). Ein Abo läuft im Auftrag deshalb mit vollem Zugriff, wie der Nutzer es wollte (Korrektur 2026-10-24, FR-032).
- Q: Wählt der Nutzer Modelle und Aufwand in der Erweiterung oder in holzi? → A: In holzi. Die Erweiterung bekommt kein eigenes Auswahlmenü, kann aber holzis Modell-Seite für sich öffnen und die Freigabe vor dem ersten Auftrag anstoßen (US9).
- Q: Erkennt holzi ein lokales Ollama selbst, samt Bildfähigkeit seiner Modelle? → A: Nicht in dieser Ergänzung; die wenigsten Nutzer haben ein lokales Ollama. Es kann später nachgeliefert werden.

## User Scenarios & Testing

### User Story 1 - Eine Erweiterung stellt eine Anfrage an ein Modell (Priority: P1)

Eine Erweiterung schickt holzi eine Anfrage mit Text und/oder Bildern. Sie liefert die Anweisung an das Modell, nennt, was sie braucht (zum Beispiel „ein Bild lesen“ oder „einen Text korrigieren“), und gibt das Format vor, in dem sie das Ergebnis erwartet. holzi führt die Anfrage mit einem Modell aus, das der Nutzer für diese Erweiterung freigegeben hat, und liefert das geprüfte Ergebnis zurück, zusammen mit der Angabe, was tatsächlich verwendet wurde.

**Why this priority**: Ohne diese Schnittstelle müsste jede Erweiterung eigene Zugänge, Schlüssel und Modellformate mitbringen. Das ist der Kern von allem Weiteren.

**Independent Test**: Eine Testerweiterung mit Freigabe sendet ein Bild, eine eigene Anweisung und ein Ergebnisformat. Sie erhält ein formatgerechtes Ergebnis, ohne dass ein externer Dienst aufgerufen wird (Testmodell).

**Acceptance Scenarios**:

1. **Given** eine Erweiterung mit Freigabe für ein Modell, **When** sie eine Anfrage stellt, **Then** erhält sie ein Ergebnis im vereinbarten Format und die Angabe, welches Modell, welcher Aufwand und welcher Ort verwendet wurden.
2. **Given** eine Erweiterung mit dem Berechtigungszustand `denied` (explizit abgelehnt), **When** sie eine Anfrage stellt, **Then** wird nichts verarbeitet und sie erhält einen klaren Fehler.
3. **Given** eine beliebige Anfrage, **When** sie ausgeführt wird, **Then** sieht die Erweiterung zu keinem Zeitpunkt Zugangsdaten, Schlüssel oder Adressen interner Dienste.

---

### User Story 2 - Der Nutzer bestimmt pro Erweiterung Modelle und Aufwand (Priority: P1)

In den Einstellungen der Erweiterung sieht der Nutzer die Modelle, die holzi kennt, und legt fest, welche die Erweiterung nutzen darf und mit welchem höchsten Aufwand (Effort). Der Berechtigungszustand `denied` sperrt den Zugriff. Steht die Berechtigung auf `ask`, fragt holzi bei der ersten Anfrage den Nutzer, statt stillschweigend zu verarbeiten.

**Why this priority**: Der Nutzer behält die Kontrolle darüber, welche Erweiterung welche Modelle, auch bezahlte, und wie intensiv anspricht.

**Independent Test**: Mit zwei Modellen wird nur eines freigegeben. Anfragen an das freigegebene gelingen, Anfragen an das andere werden abgelehnt. Nach dem Widerruf scheitert auch die nächste Anfrage an das erste.

**Acceptance Scenarios**:

1. **Given** eine Erweiterung ohne Freigaben, **When** sie zum ersten Mal anfragt, **Then** fragt holzi den Nutzer, welche Modelle und welchen Aufwand er erlaubt.
2. **Given** eine Freigabe mit höchstem Aufwand „mittel“, **When** die Erweiterung „hoch“ verlangt, **Then** wird der Aufwand auf „mittel“ begrenzt und die Herkunftsangabe nennt den tatsächlichen Aufwand.
3. **Given** ein widerrufenes Modell, **When** die Erweiterung es bei der nächsten Anfrage nennt, **Then** wird die Anfrage abgelehnt; eine bereits laufende Anfrage darf zu Ende laufen oder abgebrochen werden.
4. **Given** mehrere freigegebene Modelle, **When** die Erweiterung nur nennt, was sie braucht, **Then** wählt holzi das nach der Rangfolge des Nutzers passende Modell.
5. **Given** eine Erweiterung mit Freigaben, **When** sie die Liste ihrer Modelle abfragt, **Then** erhält sie nur die für sie freigegebenen Modelle mit Namen, Fähigkeiten und Ort, nie Adressen oder Schlüssel; bei fehlender Freigabe ist die Liste leer.

---

### User Story 3 - Ergebnis im vereinbarten Format, mehrere Schritte, kaputte Ausgabe verworfen (Priority: P1)

Jeder Auftrag hat ein Ergebnisformat, das die Erweiterung mit der Anfrage vorgibt. Ein Auftrag kann aus mehreren Schritten bestehen, die verschiedene Modelle nutzen, zum Beispiel erst ein Bildmodell, das eine Seite liest, dann ein Textmodell, das Rechtschreibung und Lesefehler korrigiert. Ausgabe, die nicht zum Format passt oder offensichtlich kaputt ist, wird nicht als Ergebnis geliefert.

**Why this priority**: Erweiterungen bauen auf dem Ergebnis auf. Freier Modelltext oder Endlosschleifen würden Schülern Unsinn in ihre Notizen schreiben.

**Independent Test**: Ein Testmodell liefert nacheinander gültige, formatwidrige, leere, sich wiederholende und in eine andere Schrift wechselnde Ausgabe. Nur die gültige wird als Ergebnis geliefert, in allen anderen Fällen erhält die Erweiterung einen strukturierten Fehler.

**Acceptance Scenarios**:

1. **Given** ein Auftrag mit zwei Schritten, **When** Schritt 1 ein Ergebnis liefert, **Then** erhält Schritt 2 dieses Ergebnis als Eingabe und die Herkunftsangabe nennt für jeden Schritt das verwendete Modell.
2. **Given** eine Ausgabe, die nicht zum Format passt, **When** holzi sie prüft, **Then** wird sie verworfen und die Erweiterung erhält einen Fehler, der „unbrauchbar“ von „nicht verfügbar“ unterscheidet.
3. **Given** eine Ausgabe aus einer Schleife (dieselbe Wendung vielfach wiederholt), einem Wechsel in eine ganz andere Schrift oder aus leerem Text, **When** holzi sie prüft, **Then** wird sie verworfen.
4. **Given** ein Korrekturschritt, dessen Anweisung festlegt, nur Rechtschreibung und eindeutige Lesefehler zu ändern, **When** er einen Text bearbeitet, **Then** liefert holzi die Ausgabe des Modells unverändert zurück und greift nicht in den Text ein; die Verantwortung für die Anweisung liegt bei der Erweiterung.
5. **Given** ein verworfenes Ergebnis, **When** die Erweiterung es bemerkt, **Then** zeigt der Nutzer in holzi keine Markierung am Text; die Erweiterung entscheidet selbst, was sie anzeigt (etwa den Bereich leer lassen).

---

### User Story 4 - Auftragsdaten verlassen das Gerät nur mit Einwilligung (Priority: P1)

holzi berücksichtigt zuerst geeignete lokale Modelle. Sind mehrere geeignete Modelle freigegeben, entscheidet die Rangfolge des Nutzers; ein lokales Modell ist vorzuziehen, wenn kein höher geranktes geeignetes Modell existiert. Wird ein höher geranktes Remote-Modell gewählt, bleiben die Auftragsdaten bis zur Einwilligung auf dem Gerät. Sollen Text oder Bilder an ein anderes Gerät, einen Server oder einen Cloud-Anbieter gehen, sieht der Nutzer vor dem ersten Versand an dieses Ziel, welche Daten übertragen werden und wohin, und willigt ein oder lehnt ab.

**Why this priority**: Schülernotizen können personenbezogene oder schulische Inhalte enthalten. Qualität geht vor Offline, aber nie ohne Wissen des Nutzers.

**Independent Test**: Eine Anfrage mit Text oder Bild an ein Cloud-Modell ohne Einwilligung sendet nichts. Eine Anfrage an ein lokales Modell erzeugt keine Netzwerkanfrage nach außen.

**Acceptance Scenarios**:

1. **Given** ein lokales und ein Cloud-Modell sind freigegeben und das lokale ist das höchstgerankte geeignete Modell, **When** eine Anfrage mit Bild kommt, **Then** läuft sie lokal und kein Bildbyte verlässt das Gerät.
2. **Given** nur ein Ziel außerhalb des Geräts ist freigegeben, **When** zum ersten Mal Text oder ein Bild dorthin soll, **Then** zeigt holzi die Datenübertragung und das Ziel und fragt um Einwilligung, bevor etwas gesendet wird.
3. **Given** eine für die Tresorsitzung erteilte Einwilligung, **When** spätere Anfragen derselben Erweiterung dasselbe Ziel nutzen, **Then** wird in dieser Tresorsitzung nicht erneut gefragt; nach ihrem Schließen fragt es bei der nächsten Tresorsitzung wieder.
4. **Given** eine dauerhaft erteilte Einwilligung, **When** spätere Anfragen dasselbe Ziel nutzen, **Then** wird nicht mehr gefragt, bis der Nutzer sie in den Einstellungen widerruft.
5. **Given** das bevorzugte Modell fällt aus oder liefert unbrauchbare Ausgabe, **When** das nächste Modell der Rangfolge ein Ziel außerhalb des Geräts hat, **Then** wechselt holzi nur dorthin, wenn dafür schon eine Einwilligung vorliegt; sonst endet der Auftrag mit einem Fehler und holzi fragt nicht zwischendurch.

---

### User Story 5 - Ein Modell auf einem anderen Gerät, auf einem Server oder in der Cloud nutzen (Priority: P2)

Der Nutzer kann holzi um einen allgemeinen Anbieter nach verbreitetem Schnittstellenstandard („OpenAI-kompatibel“) ergänzen, indem er eine Adresse und, falls der Dienst einen verlangt, einen Zugangsschlüssel einträgt. So lassen sich ein eigener Rechner mit Grafikkarte, ein Server (etwa ein Schul-Server mit lokalem Modell) oder ein Cloud-Dienst mit dieser Schnittstelle nutzen. Das Handy nimmt nur das Foto auf; die Analyse läuft dort, wo der Nutzer sie erlaubt hat.

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

### User Story 7 - Seitenerkennung als erstes Beispiel einer Erweiterung (Priority: P2)

haex-notes beschreibt als erste Erweiterung einen Auftrag „Seitenerkennung“ mit eigener Anweisung und eigenem Format: Aus dem Foto einer Seite entsteht ein Ergebnis mit Regionen. Jede Region hat eine Art (Text, Bild, Tabelle), eine Position auf der Seite und eine Lesereihenfolge. Textregionen enthalten den Text, Tabellenregionen Zeilen und Zellen.

**Why this priority**: Ohne ein konkretes, nützliches Format bliebe die Schnittstelle abstrakt. Dieses Format löst den Fall der Schüler: Mitschriften und Arbeitsblätter digitalisieren.

**Independent Test**: Eine Testerweiterung beschreibt dieses Format. Ein Testmodell liefert für ein Seitenfoto ein Ergebnis mit je einer Text-, Bild- und Tabellenregion. holzi prüft es gegen das Format der Erweiterung, eine Tabelle mit fehlender Zelle wird verworfen.

**Acceptance Scenarios**:

1. **Given** ein Seitenfoto, **When** der Auftrag läuft, **Then** liefert das Ergebnis Regionen mit Art, Position und Lesereihenfolge.
2. **Given** eine Tabelle auf der Seite, **When** sie erkannt wird, **Then** enthält das Ergebnis Zeilen und Zellen, auch leere Zellen, und nicht nur ein Bild der Tabelle.
3. **Given** ein auf der Seite liegendes Foto, **When** der Auftrag läuft, **Then** wird die Seite aufrecht ausgewertet, oder die Erweiterung erhält die Angabe, dass sie gedreht werden muss.

---

### User Story 8 - Jedes Modell aus holzi kann freigegeben werden (Priority: P2)

Der Nutzer kann einer Erweiterung jedes Modell freigeben, das holzi kennt: ein lokal installiertes Modell, ein Modell über einen Modell-Server oder ein Konto bei einem Cloud-Dienst und ein verbundenes Abo wie Claude Code oder Codex. Die Freigabeliste zeigt alle diese Modelle gleich, mit Fähigkeiten und Ort. Woher ein Modell kommt, spielt für Freigabe, Rangfolge, Aufwand, Einwilligung und Herkunft keine Rolle.

**Why this priority**: Wer holzi schon ein Abo oder ein lokales Modell gegeben hat, soll es nicht ein zweites Mal als Modell-Server einrichten müssen, damit eine Erweiterung es nutzen kann.

**Independent Test**: Mit je einem lokal installierten Modell, einem Modell eines Modell-Servers und einem verbundenen Abo (Testadapter) erscheinen alle drei in der Freigabeliste einer Testerweiterung. Ein Textauftrag läuft auf jedem, sobald es freigegeben ist; die Herkunft nennt Modell und Ort.

**Acceptance Scenarios**:

1. **Given** holzi kennt ein lokal installiertes Modell, ein Modell eines Modell-Servers und ein verbundenes Abo, **When** der Nutzer die Modelle einer Erweiterung öffnet, **Then** sieht er alle drei mit Fähigkeiten und Ort und kann jedes freigeben.
2. **Given** ein freigegebenes lokal installiertes Modell, das gerade nicht geladen ist, **When** ein Textauftrag kommt, **Then** lädt holzi es auf dem Gerät und führt den Auftrag aus, ohne dass Daten das Gerät verlassen.
3. **Given** ein laufender Chat-Zug mit einem lokalen Modell, **When** ein Auftrag ein lokales Modell braucht, **Then** wartet der Auftrag, bis der Chat-Zug fertig ist; der Chat wird weder unterbrochen noch verfälscht und lädt sein Modell beim nächsten Zug wieder.
4. **Given** ein freigegebenes Abo, **When** der Nutzer es freigibt, **Then** zeigt holzi, dass die Erweiterung damit einen Agenten mit Datei- und Befehlszugriff anweist, und vor dem ersten Versand fragt holzi um Einwilligung für das Ziel des Abos (US4).
5. **Given** ein Abo, dessen Verbindung getrennt wurde, **When** ein Auftrag es versucht, **Then** gilt es als nicht verfügbar und holzi versucht das nächste Modell der Rangfolge (FR-027).

---

### User Story 9 - Aus der Erweiterung zur Modellwahl in holzi (Priority: P2)

Modelle und Aufwand wählt der Nutzer immer in holzi, nie in einem Menü der Erweiterung. Die Erweiterung kann ihn aber dorthin bringen: Sie öffnet holzis Modell-Seite für sich selbst, etwa über einen Menüpunkt „Modelle in holzi festlegen“ oder aus einer Fehlermeldung heraus. Und sie kann die Freigabe anstoßen, bevor der erste Auftrag läuft, etwa beim ersten Öffnen; dann zeigt holzi dieselbe Frage wie beim ersten Auftrag.

**Why this priority**: Ein Hinweis wie „Gib in holzi ein Modell frei“ lässt den Nutzer suchen. Ein Weg direkt zur richtigen Seite spart das, ohne dass die Erweiterung die Auswahl bestimmt.

**Independent Test**: Eine Testerweiterung öffnet holzis Modell-Seite und landet bei ihrer eigenen. Sie stößt die Freigabe für einen Bildschritt an: holzi fragt mit den passenden Modellen, nach der Antwort erhält die Erweiterung ihre freigegebenen Modelle; ohne passendes Modell erhält sie `no_capable_model`.

**Acceptance Scenarios**:

1. **Given** eine aktive Erweiterung, **When** sie holzis Modell-Seite öffnet, **Then** zeigt holzi die Modelle genau dieser Erweiterung und nie die einer anderen; ist die Seite schon offen, holt holzi sie nach vorn, statt eine weitere zu öffnen.
2. **Given** eine Erweiterung ohne passende Freigabe und ein passendes Modell im Tresor, **When** sie die Freigabe für einen Bildschritt anstößt, **Then** fragt holzi wie beim ersten Auftrag (Modell, höchster Aufwand, Sitzung oder dauerhaft), und die Erweiterung erhält danach ihre freigegebenen Modelle.
3. **Given** eine passende Freigabe besteht schon, **When** die Erweiterung die Freigabe anstößt, **Then** fragt holzi nicht und liefert die freigegebenen Modelle.
4. **Given** kein Modell im Tresor passt, **When** die Erweiterung die Freigabe anstößt, **Then** erhält sie `no_capable_model` und kann dem Nutzer anbieten, holzis Modell-Seite zu öffnen.
5. **Given** die Freigabe wird angestoßen, **When** holzi fragt, **Then** wird nichts an ein Modell gesendet und keine Einwilligung für ein Ziel eingeholt; die kommt mit dem ersten Auftrag, der Daten sendet (FR-015).

### Edge Cases

- Das gewählte Modell ist nicht verfügbar, etwa weil das lokale Modell nicht installiert ist. holzi versucht das nächste Modell der Rangfolge, aber nie ein Ziel ohne erteilte Einwilligung.
- Alle Modelle der Rangfolge sind ausgefallen oder haben Unbrauchbares geliefert. Die Erweiterung erhält einen Fehler, der die Versuche nennt, und behält ihr Original.
- Das Modell liefert gültigen Aufbau, aber offensichtlich erfundene Inhalte auf unleserlichem Material. holzi erkennt das nicht in jedem Fall; es verwirft nur Schleifen, Schriftwechsel, leere und formatwidrige Ausgabe. Alles andere liefert die Erweiterung.
- Die Anfrage dauert zu lange oder wird abgebrochen. Die Erweiterung erhält einen eindeutigen Status und kann ihr Original weiter verwenden.
- Zwei Erweiterungen fragen gleichzeitig an. Jede wird nur nach ihren eigenen Freigaben behandelt, Ergebnisse vermischen sich nie.
- Der Nutzer entfernt eine Freigabe, während ein Auftrag mit mehreren Schritten läuft. Der nächste Schritt wird nicht mehr ausgeführt, die Erweiterung erhält einen Fehler.
- Ein Schritt braucht ein Modell, das der Nutzer nicht freigegeben hat, obwohl andere Schritte freigegebene Modelle haben. Der Auftrag scheitert vor dem ersten Schritt, nicht mittendrin.
- Das Bild oder die Zahl der Bilder liegt über der Obergrenze von holzi. Die Anfrage wird abgelehnt, mit klarer Meldung, welche Grenze überschritten wurde, und ohne Verlust des Originals. holzi verkleinert nichts stillschweigend.
- Ein Zugangsschlüssel oder eine Serveradresse darf nie in Fehlermeldungen, Ergebnissen oder dem Speicher der Erweiterung auftauchen.
- Ein Bildauftrag zielt auf ein lokal installiertes Modell, das nur Text kann. Es wird wie jedes Modell ohne Bildfähigkeit vor der Übertragung übergangen (FR-019).
- Der Chat belegt das lokale Modell länger als die Höchstdauer des Auftrags. Der Auftrag endet mit Zeitüberschreitung, der Chat läuft weiter.
- Ein Abo ändert in einem Auftrag Dateien oder führt Befehle aus. holzi verhindert das nicht (Clarifications 2026-10-23); die Freigabe hat darauf hingewiesen.

## Requirements

### Functional Requirements

- **FR-001**: holzi MUST einer berechtigten, aktiven Erweiterung erlauben, Anfragen mit Text und/oder Bildern an Modelle von holzi zu stellen.
- **FR-002**: Eine Anfrage mit dem Berechtigungszustand `denied` MUST abgelehnt werden, bevor ein Modell angesprochen wird. Der Zustand `ask` MUST zuerst die übliche Berechtigungsabfrage auslösen und darf weder verarbeitet noch als endgültig abgelehnt behandelt werden.
- **FR-003**: Der Nutzer MUST pro Erweiterung festlegen können, welche Modelle sie nutzen darf und mit welchem höchsten Aufwand; ohne ausdrückliche Freigabe MUST kein Modell nutzbar sein. Die Zustände `granted`, `denied` und `ask` folgen dabei dem bestehenden Berechtigungsmodell.
- **FR-004**: Steht die Berechtigung einer Erweiterung auf `ask`, MUST holzi bei der ersten passenden Anfrage den Nutzer um Freigabe bitten, statt stillschweigend zu verarbeiten oder zu scheitern. Die Abfrage folgt den übrigen Berechtigungsabfragen von holzi: Erlauben gilt für die Tresorsitzung oder dauerhaft; ein gespeichertes `denied` hat Vorrang vor einer Freigabe.
- **FR-005**: Ein Widerruf MUST ab der nächsten Anfrage wirken.
- **FR-006**: Verlangt eine Erweiterung mehr Aufwand als erlaubt, MUST er auf das Erlaubte begrenzt werden; das Ergebnis MUST den tatsächlichen Aufwand nennen.
- **FR-036**: Der Nutzer MUST den höchsten Aufwand einer Erweiterung für ein Modell aus den Stufen wählen, die dieses Modell selbst hat (bei Claude `low` bis `max` mit allen Zwischenstufen), nicht aus einer festen Liste. Ein Modell ohne Wahl des Aufwands bietet nur „erlaubt“. Die Erweiterung kann jede dieser Stufen wünschen; holzi nimmt die höchste Stufe des Modells, die den Wunsch nicht übersteigt.
- **FR-007**: Nennt eine Erweiterung nur, was sie braucht, MUST holzi unter den freigegebenen und geeigneten Modellen nach der Rangfolge des Nutzers das passende wählen.
- **FR-008**: Jeder Auftrag MUST ein Ergebnisformat haben, das die Erweiterung mit der Anfrage vorgibt (Name, Version, Aufbau); holzi MUST die Ausgabe dagegen prüfen. Freier Modelltext MUST NICHT als Erfolg gelten.
- **FR-009**: Ein Auftrag MUST aus mehreren Schritten mit je eigenem Modell bestehen können; das Ergebnis eines Schritts MUST dem nächsten als Eingabe dienen.
- **FR-010**: holzi MUST Ausgabe, die nicht zum Format passt, leer ist, sich in Schleifen wiederholt oder in eine andere Schrift wechselt, verwerfen und der Erweiterung einen strukturierten Fehler liefern.
- **FR-011**: Der Fehler für unbrauchbare Ausgabe MUST von „Modell nicht verfügbar“, „nicht freigegeben“, „Zeitüberschreitung“ und „abgebrochen“ unterscheidbar sein.
- **FR-012**: holzi MUST für verworfene Ausgabe keine Markierung im Text des Nutzers erzeugen; die Darstellung bleibt der Erweiterung überlassen.
- **FR-013**: Die Anweisung eines Korrekturschritts liegt bei der Erweiterung. Für haex-notes MUST sie festlegen, dass nur Rechtschreibung und eindeutige Lesefehler geändert werden und Abkürzungen, Wortwahl und Zeilenumbrüche erhalten bleiben (Spec 046). holzi MUST die Ausgabe unverändert zurückgeben und nur die allgemeinen Prüfungen auf Format und offensichtlich unbrauchbare Ausgabe anwenden; die semantische Einhaltung der Korrekturanweisung liegt bei der Erweiterung.
- **FR-014**: holzi MUST ein lokales Modell bevorzugen, wenn es freigegeben und für den Auftrag geeignet ist und kein höher geranktes geeignetes Modell existiert; wird es gewählt, MUST die Verarbeitung lokal bleiben.
- **FR-015**: Vor dem ersten Versand von Auftragsdaten (Text oder Bilder) an ein Ziel außerhalb des Geräts MUST holzi die Datenübertragung und das Ziel sichtbar machen und die Einwilligung des Nutzers einholen; ohne Einwilligung MUST nichts gesendet werden.
- **FR-016**: Die Einwilligung MUST wie jede andere Berechtigungsabfrage in holzi funktionieren: Sie gilt pro Erweiterung und Ziel und wird für die Tresorsitzung oder dauerhaft erteilt. Eine dauerhafte Einwilligung MUST in den Einstellungen widerrufbar sein, eine Sitzungs-Einwilligung endet beim Schließen der Tresorsitzung.
- **FR-017**: holzi MUST NICHT von einem Modell auf ein Ziel ohne erteilte Einwilligung wechseln, auch nicht nach einem Fehler. Ohne Einwilligung endet der Auftrag mit einem Fehler.
- **FR-018**: Der Nutzer MUST einen allgemeinen Anbieter nach einem verbreiteten Schnittstellenstandard (OpenAI-kompatibel) mit Adresse und einem Zugangsschlüssel, falls der Dienst einen verlangt, eintragen können; dessen Modelle MUST in der Freigabeliste erscheinen.
- **FR-019**: Zu jedem Modell MUST bekannt sein, ob es Bilder annimmt (erkannt oder vom Nutzer angegeben); Bildaufträge an Modelle ohne Bildfähigkeit MUST vor der Übertragung abgelehnt werden.
- **FR-020**: Lokale Modelle MUST Bilder annehmen können, wenn das Modell dazu fähig ist.
- **FR-021**: Zugangsdaten, Schlüssel und Adressen interner Dienste MUST der Erweiterung, Fehlermeldungen, Ergebnissen und synchronisierten Daten fernbleiben.
- **FR-022**: Der Nutzer MUST laufende Aufträge abbrechen können; ein Abbruch oder Fehler MUST die Originaldaten der Erweiterung unverändert lassen.
- **FR-023**: Das Ergebnis MUST die Herkunft nennen: Ergebnisformat samt Version, je Schritt Modell, Aufwand und Ort (lokal, anderes Gerät, Server, Cloud).
- **FR-024**: Das Format „Seitenerkennung“, das haex-notes vorgibt, MUST Regionen mit Art (Text, Bild, Tabelle), Position, Lesereihenfolge und Inhalt beschreiben; Tabellen MUST Zeilen und Zellen enthalten. holzi prüft dieses wie jedes andere Format.
- **FR-025**: Ein Auftrag mit mehreren Schritten MUST vor dem ersten Schritt prüfen, dass für alle Schritte ein freigegebenes Modell vorhanden ist.
- **FR-026**: holzi MUST einem Auftrag keine eigenen Werkzeuge, keinen Dateizugriff und keine Befehlsausführung geben, da die Anweisung von der Erweiterung stammt und nicht vertraut wird. Ein Modell ohne eigene Werkzeuge sieht nur die Eingabe, die die Erweiterung mitsendet. Ausgenommen sind Abos (Befehlszeilen-Agenten): Sie bringen eigene Werkzeuge mit und laufen mit dem Zugriff, den sie in holzi haben (FR-032). Werkzeuge von holzi in Aufträgen, zuerst Webabruf und Websuche, regelt eine eigene spätere Spec mit eigener, im Manifest erklärter Berechtigung (Clarifications 2026-10-20).
- **FR-027**: Fällt das bevorzugte Modell aus oder liefert es unbrauchbare Ausgabe, MUST holzi automatisch das nächste freigegebene Modell der Rangfolge versuchen, dessen Ziel schon eine Einwilligung hat, mit höchstens einem Versuch pro Modell. Die Herkunft MUST alle versuchten Modelle und die Gründe des Scheiterns nennen.
- **FR-028**: holzi MUST über Aufträge nichts dauerhaft speichern, weder Inhalte (Bilder, Anweisungen, Antworten) noch Metadaten (Zeit, Modell, Ziel, Ergebnis). Ergebnis und Herkunft gehen nur an die Erweiterung. Der Zustand laufender Aufträge liegt im Arbeitsspeicher und endet mit dem Auftrag. Gespeichert bleiben nur die Berechtigungen und Einwilligungen des Nutzers.
- **FR-029**: holzi MUST Obergrenzen für Bildgröße, Zahl der Bilder pro Auftrag und Dauer eines Auftrags durchsetzen. Die Erweiterung darf sie unterschreiten, aber nicht überschreiten. Der Nutzer MUST die Höchstdauer pro Erweiterung ändern können. Eine Überschreitung der Bildgrenzen MUST vor der Verarbeitung abgelehnt werden, ein Überschreiten der Dauer bricht den Auftrag ab.
- **FR-030**: holzi MUST einer Erweiterung auf Anfrage die Liste der für sie freigegebenen Modelle mit Namen und Fähigkeiten (zum Beispiel „nimmt Bilder an“) und dem Ort (dieses Gerät, anderes Gerät, Server, Cloud) nennen, ohne Adressen, Schlüssel oder andere Zugangsdaten. Die Auswahl des Modells bleibt bei holzi (FR-007).
- **FR-031**: Der Nutzer MUST einer Erweiterung jedes Modell freigeben können, das holzi kennt: lokal installierte Modelle, Modelle über einen Modell-Server oder ein Konto bei einem Cloud-Dienst und verbundene Abos (Claude Code, Codex). Für alle gelten dieselben Regeln zu Freigabe, Rangfolge, Aufwand, Fähigkeiten, Einwilligung, Prüfung der Ausgabe und Herkunft.
- **FR-032**: Ein Abo MUST in einem Auftrag mit dem Zugriff laufen, den es in holzi hat; holzi schränkt es für Aufträge nicht zusätzlich ein. Die Freigabe eines Abos MUST sichtbar sagen, dass die Erweiterung damit einen Agenten anweist, der auf diesem Gerät Dateien lesen und ändern und Befehle ausführen kann. Ein Abo sendet an den Dienst seines Anbieters und hat damit ein Ziel außerhalb des Geräts (FR-015). Der Agent läuft ohne Rückfragen und ohne Verbotsregeln (Autonomie-Modus „Ungegated“), gleich welchen Modus der Nutzer für den Chat eingestellt hat: Ein Auftrag hat keinen Chat zum Fragen und keinen Verlauf zum Protokollieren, und „Gegated (permissiv)“ verweigert in dieser Lage jeden Aufruf (Clarifications 2026-10-24). Die Verbotsregeln des Chats gelten in Aufträgen nicht; die Freigabe sagt das.
- **FR-033**: Ein Auftrag auf einem lokal installierten Modell MUST auf dem Gerät bleiben. Ist dafür ein anderes Modell zu laden als das des Chats, MUST der Auftrag warten, bis ein laufender Chat-Zug fertig ist, und darf ihn weder unterbrechen noch verfälschen; der Chat lädt sein Modell beim nächsten Zug wieder. Die Wartezeit zählt zur Höchstdauer des Auftrags (FR-029).
- **FR-034**: Eine aktive Erweiterung MUST holzis Modell-Seite für sich selbst öffnen können; holzi MUST dabei nur die Modelle der aufrufenden Erweiterung zeigen und eine schon offene Seite nach vorn holen statt eine weitere zu öffnen. Eine Erweiterung MUST NICHT Modelle oder Aufwand für sich festlegen können; das bleibt beim Nutzer in holzi (FR-003).
- **FR-035**: Eine aktive Erweiterung MUST die Freigabe für das, was sie braucht (Text, Bild, gewünschter Aufwand), vor dem ersten Auftrag anstoßen können. holzi MUST dann wie beim ersten Auftrag fragen (FR-004), ohne etwas an ein Modell zu senden und ohne eine Einwilligung für ein Ziel einzuholen. Die Antwort MUST die freigegebenen Modelle wie FR-030 nennen, oder `no_capable_model` beziehungsweise die Ablehnung.

### Key Entities

- **Modellfreigabe**: Pro Erweiterung die erlaubten Modelle, deren Rangfolge und der höchste Aufwand.
- **Auftrag**: Eine Anfrage einer Erweiterung mit Eingabe (Text, Bilder), eigener Anweisung, Ergebnisformat und einem oder mehreren Schritten.
- **Schritt**: Ein einzelner Modellaufruf eines Auftrags mit benötigter Fähigkeit (zum Beispiel Bild lesen, Text korrigieren).
- **Ergebnisformat**: Name, Version und Aufbau des erwarteten Ergebnisses, von der Erweiterung vorgegeben; das erste Beispiel ist „Seitenerkennung“ von haex-notes.
- **Anbieter**: Eine Quelle für Modelle: lokal, ein Konto bei einem Cloud-Dienst, die Anbindung eines Befehlszeilen-Agenten oder ein allgemeiner OpenAI-kompatibler Dienst mit Adresse und Schlüssel.
- **Fähigkeit**: Aussage, ob ein Modell Bilder, strukturierte Ausgabe oder Werkzeuge kann.
- **Ziel**: Wo ein Auftrag verarbeitet wird: dieses Gerät, ein anderes Gerät, ein Server oder die Cloud.
- **Einwilligung**: Die Zustimmung des Nutzers pro Erweiterung und Ziel, Auftragsdaten dorthin zu senden; für die Tresorsitzung oder dauerhaft.
- **Herkunft**: Nicht geheime Angaben, welches Format, Modell, Aufwand und Ziel tatsächlich verwendet wurden.

## Out of Scope

- Der Betrieb eines Schul-Servers: Konten für Schüler, Datenschutz bei Minderjährigen, Kapazität und Kosten.
- Das Canvas in haex-notes (Tafelbild, Neuanordnen, Bearbeiten, PDF-Export); das bleibt Spec 046.
- Die Wahl des konkreten Modells. Sie ist eine Einstellung und wird gemessen, nicht festgeschrieben.
- Ein allgemeiner Chat für Erweiterungen mit Gesprächsverlauf. Ein Abo darf in einem Auftrag seine eigenen Werkzeuge nutzen (FR-032), aber ein Auftrag bleibt ein einzelner Aufruf ohne Verlauf.
- Werkzeuge in Aufträgen (Webabruf, Websuche). Sie kommen mit einer eigenen Spec zusammen mit holzis Websuche (Clarifications 2026-10-20).
- Persönliche Handschriftprofile und Training mit Schülerdaten.
- Das Weiterleiten von Aufträgen an ein anderes Gerät desselben Nutzers über den Tresor. Aufträge laufen auf einem eingetragenen Anbieter.
- Ein Menü in der Erweiterung, in dem Modelle oder Aufwand gewählt werden; die Wahl liegt in holzi (US9).
- Das selbsttätige Erkennen eines lokalen Ollama samt Bildfähigkeit seiner Modelle und eine zweite lokale Laufzeit für Bildmodelle (etwa llama.cpp); beides kann später kommen (Clarifications 2026-10-23).

## Success Criteria

### Measurable Outcomes

- **SC-001**: 100 % der Anfragen einer Erweiterung mit dem Zustand `denied` werden abgelehnt, bevor ein Modell angesprochen wird; Anfragen im Zustand `ask` lösen stattdessen zuerst eine Berechtigungsabfrage aus.
- **SC-002**: Ein Widerruf wirkt bei der nächsten Anfrage; in keinem Testfall wird danach noch verarbeitet.
- **SC-003**: Bei lokaler Verarbeitung verlässt in einem überwachten Test kein Bildbyte das Gerät.
- **SC-004**: Ohne Einwilligung startet keine Übertragung an ein Ziel außerhalb des Geräts; die Abfrage nennt das Ziel in 100 % der Fälle.
- **SC-005**: In einer Testreihe mit gültiger, formatwidriger, leerer, sich wiederholender und schriftwechselnder Ausgabe wird 100 % der unbrauchbaren Ausgabe verworfen und nie als Ergebnis geliefert.
- **SC-006**: 100 % der Bildaufträge an ein Modell ohne Bildfähigkeit werden vor der Übertragung abgelehnt.
- **SC-007**: Ein Nutzer trägt einen neuen Anbieter (Adresse, Schlüssel) in unter zwei Minuten ein und sieht danach dessen Modelle in der Freigabeliste.
- **SC-008**: Nach Abbruch oder Fehler ist das von der Erweiterung gespeicherte Original in 100 % der Fehlerfälle unverändert nutzbar.
- **SC-009**: Eine Testseite mit Text, Bild und Tabelle liefert ein formatgültiges Ergebnis; die Tabelle enthält alle Zeilen und Zellen.
- **SC-010**: Ein Wechsel des Modells für einen Auftrag erfordert weder Änderungen an der Erweiterung noch Datenmigrationen. Die Anweisung der Erweiterung bleibt unverändert.
- **SC-011**: Nach einem abgeschlossenen, abgebrochenen oder fehlgeschlagenen Auftrag findet sich in den gespeicherten Daten von holzi keine Spur von Inhalt oder Metadaten des Auftrags; nur Berechtigungen und Einwilligungen bleiben.
- **SC-012**: Ein Auftrag über den Obergrenzen für Bild oder Dauer wird in 100 % der Testfälle abgelehnt oder abgebrochen, und das Original der Erweiterung bleibt unverändert.
- **SC-013**: Jedes Modell, das holzi kennt, erscheint in der Freigabeliste einer Erweiterung; in einem Test mit je einem Modell jeder Quelle liefert ein freigegebener Textauftrag auf jedem ein formatgültiges Ergebnis.
- **SC-014**: Ein Auftrag auf einem lokal installierten Modell erzeugt in einem überwachten Test keine Netzwerkanfrage nach außen, und ein gleichzeitiger Chat-Zug endet mit derselben Antwort wie ohne Auftrag.
- **SC-015**: Aus einer Erweiterung erreicht der Nutzer ihre Modell-Seite in holzi mit einem Klick; in 100 % der Testfälle zeigt sie die Modelle der aufrufenden Erweiterung, und ein Anstoßen der Freigabe sendet nichts an ein Modell.

## Assumptions

- holzi hat bereits eine gemeinsame Verwaltung von Anbietern und Modellen (lokal, Konto bei einem Cloud-Dienst, Befehlszeilen-Agent); diese Spec ergänzt den Zugriff von Erweiterungen, den allgemeinen Anbieter und Bildfähigkeit für lokale Modelle.
- Das feste Format `paper-note-recognition` aus Spec 047 entfällt: haex-notes gibt „Seitenerkennung“ künftig selbst vor. Die dort gebaute Hostschnittstelle (Start, Abbruch, Ergebnisprüfung, Ereignis bei Abschluss) wird weiterverwendet und verallgemeinert.
- Einwilligung und erste Freigabe nutzen die Berechtigungsabfrage, die holzi für Erweiterungen schon hat (Spec 017, CONTEXT.md): Der Nutzer erlaubt für die Tresorsitzung oder dauerhaft und kann in den Einstellungen widerrufen. Ein Auftrag fragt nicht jedes Mal neu.
- Wählt die Erweiterung kein Modell, entscheidet holzi nach der Rangfolge, die der Nutzer pro Erweiterung festlegt; ein lokales Modell wird gewählt, wenn kein höher geranktes geeignetes Modell existiert.
- Verlangter Aufwand über dem Erlaubten wird begrenzt und nicht abgelehnt; das Ergebnis nennt den tatsächlichen Aufwand.
- Die Qualität einzelner Modelle bei Handschrift, Tabellen und Positionen wird gesondert gemessen (Testfotos liegen vor); die Wahl eines Modells ist daher keine Voraussetzung dieser Spec.
- Das Handy nimmt nur das Foto auf; es läuft dort kein lokales Bildmodell. Wo das Foto ausgewertet wird, bestimmen die freigegebenen Modelle und Ziele.
- Ein Schul-Server ist für holzi nur ein weiterer Anbieter mit Adresse und Schlüssel.
- Die Zahlenwerte der Obergrenzen legt der Plan fest. Sie müssen so gewählt sein, dass übliche Handyfotos einer Seite darunter liegen.
- Ein Protokoll der Aufträge für den Nutzer gibt es nicht; der Nutzer sieht Ziel und Übertragung nur in der Einwilligungsabfrage. Fehlersuche in holzi darf keine Inhalte der Aufträge ausgeben.
- Das Wiederholen verworfener Aufträge (Warteschlange, Wiederholversuche) gehört der Erweiterung; holzi liefert nur den Fehler.
