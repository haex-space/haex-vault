# Feature Specification: Dateianhänge im Passwortmanager anzeigen und öffnen

**Feature Branch**: `058-attachment-viewer`

**Created**: 2026-10-23

**Status**: Draft

**Input**: User description: "ich möchte im passwordmanager dateianhänge wie textdateien gerne inline
lesen oder wenigstens über holzi öffnen können" (Betreiber, 2026-10-23). Entscheidungen des Betreibers:
Text und PDF werden in holzi angezeigt; zusätzlich „Mit System-App öffnen“ mit Hinweis auf die
entschlüsselte Kopie wie in Spec 048.

## Clarifications

### Session 2026-10-23

- Q: Welche Anhänge zeigt der Passwortmanager selbst an? → A: Text und PDF, zusätzlich zu den Bildern
  aus Spec 036. Ein Tipp auf die Karte eines Text- oder PDF-Anhangs öffnet die Ansicht statt
  „Speichern unter“. Audio und Video gehören nicht dazu.
- Q: Gibt es „Mit System-App öffnen“ für Anhänge? → A: Ja, mit Hinweis wie in Spec 048 FR-028: holzi
  sagt vorher, dass eine entschlüsselte Kopie auf dem Gerät entsteht, die die andere App behalten
  kann; Haken „Nicht mehr fragen“. Nicht auf Android, solange 044 dort keine System-App anbietet.

## Begriffe

- **Anhang-Ansicht**: das Fenster im Passwortmanager, das einen einzelnen Text- oder PDF-Anhang zum
  Lesen zeigt. Sie verändert den Anhang nie.
- **Textanhang**: ein Anhang, den der Datei-Viewer aus Spec 044 als Text zeigt, erkannt an derselben
  Liste von Dateiendungen (unter anderem txt, md, log, json, csv, xml, yaml, toml, ini, conf, env).
  Passwortmanager und Dateien-App verwenden dieselbe Liste; heute kennt der Passwortmanager nur fünf
  Endungen (036).
- **PDF-Anhang**: ein Anhang mit der Endung pdf.
- **Klartext-Kopie**: die entschlüsselte Datei, die holzi für „Mit System-App öffnen“ auf dem Gerät
  anlegt, weil die andere App die Vault-Datenbank nicht lesen kann.

## Beziehung zu bestehenden Specs

- **Spec 034** (Passwortmanager): Anhänge liegen weiter in der Vault-Datenbank; Größenlimit (25 MiB,
  FR-020), Herunterladen, Umbenennen und Entfernen (FR-021) bleiben unverändert. Wie bisher erreicht
  nur der Nutzer selbst Anhänge; Agenten und Erweiterungen bekommen durch diese Spec keinen Zugang.
- **Spec 036** (Redesign): Diese Spec ändert FR-039 dort. Ein Tipp auf einen Text- oder PDF-Anhang
  öffnet die Anhang-Ansicht; nur ein Anhang anderer Art wird beim Tippen weiter gespeichert. Karten,
  Raster und Lightbox für Bilder (FR-037, FR-038, FR-041) bleiben, wie sie sind.
- **Spec 044** (Dateien): Die Anhang-Ansicht zeigt Text und PDF wie der Viewer dort (FR-009) und
  übernimmt die Grenze für lange Texte (FR-015: bis 5 MB mit Hinweis). Text und PDF sehen in beiden
  Apps gleich aus und verhalten sich gleich.
- **Spec 048** (Verschlüsselte Ordner): „Mit System-App öffnen“ folgt FR-028 dort: Hinweis vorher,
  eigener Ordner für die Kopie außerhalb des gewöhnlichen Zwischenspeichers, entfernt beim Sperren
  oder Schließen der Vault, nach einem Absturz beim nächsten Start. FR-028a (Android) gilt ebenso.
- **Spec 043** (Android): Auf Android gibt es „Mit System-App öffnen“ nicht, wie in 044. Die
  Anhang-Ansicht gibt es auf allen Plattformen.
- **Spec 013** (Vault-Lebenszyklus): Beim Sperren oder Schließen schließt die Anhang-Ansicht, und
  nichts Entschlüsseltes bleibt im Speicher von holzi.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Einen Textanhang in holzi lesen (Priority: P1)

Ben hat an seinem Eintrag „Router“ eine Datei `wiederherstellung.txt` mit Wiederherstellungscodes
hängen. Bisher musste er sie erst irgendwo speichern und mit einem Editor öffnen. Jetzt tippt er auf
die Karte des Anhangs, und holzi zeigt den Text in der Anhang-Ansicht über dem Eintrag. Er liest die
Codes, markiert eine Zeile, kopiert sie und schließt die Ansicht wieder.

**Why this priority**: Das ist der eigentliche Wunsch. Textanhänge (Wiederherstellungscodes,
Notizen, Konfigurationen) gehören zu den häufigsten Anhängen im Passwortmanager, und heute kommt der
Nutzer an ihren Inhalt nur über einen Umweg außerhalb von holzi.

**Independent Test**: Einem Eintrag eine Textdatei anhängen, auf die Karte tippen: Die Ansicht zeigt
den Text so wie in der Datei, und auf dem Gerät entsteht dabei keine Datei.

**Acceptance Scenarios**:

1. **Given** ein Eintrag mit einem Textanhang, **When** der Nutzer auf dessen Karte tippt, **Then**
   öffnet die Anhang-Ansicht mit dem Text des Anhangs, dem Dateinamen in der Kopfzeile und den
   Aktionen „Speichern unter“ und „Schließen“.
2. **Given** die offene Anhang-Ansicht, **When** der Nutzer Escape drückt oder „Schließen“ wählt,
   **Then** schließt die Ansicht, und der Fokus liegt wieder auf der Karte, die sie geöffnet hat.
3. **Given** ein Textanhang größer als 5 MB, **When** er sich öffnet, **Then** zeigt die Ansicht die
   ersten 5 MB und einen Hinweis, dass der Rest abgeschnitten ist und über „Speichern unter“ ganz zu
   haben ist.
4. **Given** ein Textanhang mit einzelnen ungültigen Zeichen, **When** er sich öffnet, **Then**
   zeigt die Ansicht den Text und an diesen Stellen ein Ersatzzeichen. Ist er gar kein Text (etwa
   eine Binärdatei mit der Endung txt), sagt die Ansicht das wie der Datei-Viewer von 044 und bietet
   „Speichern unter“ und, wo verfügbar, „Mit System-App öffnen“ an.
5. **Given** ein Eintrag im Papierkorb, **When** der Nutzer auf einen Textanhang tippt, **Then**
   öffnet die Ansicht genauso; Umbenennen und Entfernen gibt es weiterhin nicht.
6. **Given** die offene Anhang-Ansicht, **When** der Nutzer den Text markiert und kopiert, **Then**
   liegt er in der Zwischenablage; holzi leert sie nicht von selbst (anders als bei Passwörtern).
7. **Given** ein Anhang, der weder Bild noch Text noch PDF ist, **When** der Nutzer auf die Karte
   tippt, **Then** öffnet sich wie bisher „Speichern unter“.

---

### User Story 2 - Ein PDF in holzi ansehen (Priority: P2)

Clara hat an ihrem Eintrag „Hausratversicherung“ die Police als PDF hängen. Sie tippt auf die Karte,
holzi zeigt das PDF Seite für Seite. Sie blättert zur Seite mit der Vertragsnummer und vergrößert sie.

**Why this priority**: PDFs (Verträge, Rechnungen, Ausweiskopien, Zertifikate) sind nach Texten die
zweithäufigste Art von Anhang. Der PDF-Viewer der Dateien-App existiert bereits.

**Independent Test**: Einem Eintrag ein mehrseitiges PDF anhängen, auf die Karte tippen, zur zweiten
Seite blättern und zoomen.

**Acceptance Scenarios**:

1. **Given** ein Eintrag mit einem PDF-Anhang, **When** der Nutzer auf die Karte tippt, **Then** zeigt
   die Anhang-Ansicht die erste Seite mit Seitenzahl, Blättern und Zoom wie in der Dateien-App.
2. **Given** ein PDF, das sich nicht lesen lässt (beschädigt oder mit Passwort geschützt), **When** es
   sich öffnet, **Then** zeigt die Ansicht einen Hinweis, dass holzi es nicht anzeigen kann, und bietet
   „Speichern unter“ und, wo verfügbar, „Mit System-App öffnen“ an.
3. **Given** die offene Ansicht eines PDFs, **When** der Nutzer sie schließt, **Then** gibt holzi
   alles frei, was es zum Anzeigen geladen hat.

---

### User Story 3 - Einen Anhang mit der System-App öffnen (Priority: P3)

David will eine Tabellen-Datei aus einem Eintrag bearbeiten, die holzi selbst nicht anzeigt. Auf der
Karte wählt er „Mit System-App öffnen“. holzi weist ihn einmal darauf hin, dass dafür eine
entschlüsselte Kopie auf seinem Gerät entsteht, die die andere App behalten kann, und dass holzi seine
Kopie beim Sperren der Vault entfernt. David setzt den Haken „Nicht mehr fragen“ und bestätigt; die
Datei öffnet in seinem Tabellenprogramm.

**Why this priority**: Der Ausweg für alles, was holzi nicht selbst anzeigt; er ist aber mit einer
Klartext-Kopie außerhalb der Vault verbunden und kommt daher erst nach dem Anzeigen in holzi.

**Independent Test**: Auf einem Desktop-System einen Anhang mit „Mit System-App öffnen“ öffnen: Der
Hinweis erscheint, nach Bestätigen öffnet die System-App die Datei; nach dem Sperren der Vault ist
holzis Kopie entfernt.

**Acceptance Scenarios**:

1. **Given** ein Anhang auf einem Desktop-System, **When** der Nutzer auf der Karte oder in der
   Anhang-Ansicht „Mit System-App öffnen“ wählt, **Then** zeigt holzi vor dem Öffnen den Hinweis auf
   die entschlüsselte Kopie mit „Öffnen“, „Abbrechen“ und dem Haken „Nicht mehr fragen“.
2. **Given** der Hinweis, **When** der Nutzer abbricht, **Then** entsteht keine Kopie, und nichts
   öffnet sich.
3. **Given** der Nutzer hat „Nicht mehr fragen“ bestätigt, **When** er später auf demselben Gerät
   einen Anhang derselben Vault mit der System-App öffnet, **Then** öffnet holzi ohne Hinweis.
4. **Given** ein mit der System-App geöffneter Anhang, **When** der Nutzer die Vault sperrt oder
   schließt, **Then** entfernt holzi seine Kopie; nach einem Absturz entfernt holzi sie beim nächsten
   Start.
5. **Given** holzi auf Android, **When** der Nutzer eine Karte oder die Anhang-Ansicht ansieht,
   **Then** gibt es dort „Mit System-App öffnen“ nicht.
6. **Given** keine App auf dem Gerät öffnet diese Art Datei, **When** der Nutzer bestätigt, **Then**
   zeigt holzi eine verständliche Fehlermeldung und entfernt die gerade angelegte Kopie.

---

### Edge Cases

- **Anhang wird während des Lesens auf einem anderen Gerät umbenannt, ersetzt oder entfernt**: Die
  Ansicht zeigt weiter den Stand, mit dem sie geöffnet wurde; ist der Anhang entfernt, schließt sie mit
  einem Hinweis, sobald holzi die Änderung bemerkt.
- **Umbenennen macht aus einem Text eine andere Art** (etwa `notizen.txt` → `notizen.bin`): Danach
  gilt die neue Art; ein Tipp speichert.
- **Leerer Textanhang** (0 Byte): Die Ansicht öffnet und sagt, dass die Datei leer ist.
- **Sehr lange Zeilen** (minifiziertes JSON): Zeilen brechen um; die Ansicht scrollt nicht seitwärts
  aus dem Fenster.
- **Sperren der Vault bei offener Ansicht**: Die Ansicht schließt mit der Vault; nichts vom Inhalt
  bleibt sichtbar.
- **Anhang, dessen Daten noch nicht auf diesem Gerät angekommen sind**: Die Ansicht sagt, dass der
  Anhang auf diesem Gerät noch nicht verfügbar ist, statt eines technischen Fehlers.
- **Schmales Fenster und Telefon**: Die Ansicht füllt das Fenster des Passwortmanagers; Kopfzeile und
  Aktionen bleiben erreichbar.

## Requirements _(mandatory)_

### Functional Requirements

**Anhang-Ansicht**

- **FR-001**: Ein Tipp auf die Karte eines Text- oder PDF-Anhangs MUSS die Anhang-Ansicht öffnen;
  ein Tipp auf einen Anhang anderer Art MUSS wie bisher „Speichern unter“ öffnen, ein Tipp auf ein
  Bild wie bisher die Lightbox (ändert 036 FR-039).
- **FR-002**: Ob ein Anhang als Text gilt, MUSS dieselbe Regel entscheiden wie im Datei-Viewer von
  Spec 044; es DARF keine zweite, abweichende Liste von Textendungen im Passwortmanager geben.
- **FR-003**: Die Anhang-Ansicht MUSS Text so zeigen und begrenzen wie der Viewer von 044 (bis 5 MB,
  danach mit Hinweis auf das Abschneiden; FR-015 dort), nur lesend, mit Zeilenumbruch, mit
  markierbarem und kopierbarem Text; ungültige Zeichen MÜSSEN als Ersatzzeichen erscheinen, und eine
  Datei, die der Viewer von 044 als Binärdatei erkennt, MUSS statt des Texts einen Hinweis mit
  „Speichern unter“ und, wo verfügbar, „Mit System-App öffnen“ zeigen.
- **FR-004**: Die Anhang-Ansicht MUSS PDFs mit denselben Bedienelementen zeigen wie der Viewer von 044
  (Seite für Seite, Blättern, Zoom); kann holzi ein PDF nicht anzeigen, MUSS die Ansicht das sagen und
  „Speichern unter“ sowie, wo verfügbar, „Mit System-App öffnen“ anbieten.
- **FR-005**: Die Anhang-Ansicht MUSS den Dateinamen zeigen und „Speichern unter“, „Mit System-App
  öffnen“ (wo verfügbar) und „Schließen“ anbieten; Escape MUSS sie schließen, und der Fokus MUSS danach
  auf der Karte liegen, die sie geöffnet hat.
- **FR-006**: Die Anhang-Ansicht DARF den Anhang nicht verändern und DARF auf dem Gerät keine Datei
  anlegen; der Inhalt DARF nur im Arbeitsspeicher von holzi liegen und MUSS freigegeben werden, wenn
  die Ansicht schließt.
- **FR-007**: Die Anhang-Ansicht MUSS überall verfügbar sein, wo der Passwortmanager Anhänge als
  Karten zeigt: in der Ansicht und im Editor eines Eintrags und im Papierkorb.
- **FR-008**: Beim Sperren oder Schließen der Vault MUSS eine offene Anhang-Ansicht schließen.

**Mit System-App öffnen**

- **FR-009**: Auf Desktop-Systemen MUSS jede Anhangkarte und die Anhang-Ansicht „Mit System-App
  öffnen“ anbieten, für Anhänge jeder Art; auf Android DARF die Aktion nicht erscheinen, solange 044
  sie dort nicht anbietet.
- **FR-010**: Vor dem Öffnen MUSS holzi darauf hinweisen, dass eine entschlüsselte Kopie auf dem Gerät
  entsteht, die die andere App behalten kann, und dass holzi seine Kopie beim Sperren oder Schließen
  der Vault entfernt. Mit dem Haken „Nicht mehr fragen“ MUSS das für alle Anhänge dieser Vault auf
  diesem Gerät gelten; auf anderen Geräten DARF die Wahl nicht wirken.
- **FR-011**: Die Klartext-Kopie MUSS in einem eigenen Ordner von holzi liegen, außerhalb des
  gewöhnlichen Zwischenspeichers aus 044, den Dateinamen des Anhangs tragen (für das Betriebssystem
  unbedenklich gemacht) und MUSS spätestens beim Sperren oder Schließen der Vault entfernt werden,
  nach einem Absturz beim nächsten Start (wie 048 FR-028).
- **FR-012**: Scheitert das Öffnen (keine passende App, Fehler beim Schreiben), MUSS holzi eine
  verständliche Meldung zeigen und die dafür angelegte Kopie sofort entfernen.

**Alle**

- **FR-013**: Logs und Fehlermeldungen DÜRFEN keinen Inhalt von Anhängen enthalten, auch nicht auf
  Debug-Stufe.
- **FR-014**: Anzeigen und Öffnen MÜSSEN wie alle Anhang-Aktionen dem Nutzer selbst vorbehalten sein;
  Agenten und Erweiterungen DÜRFEN darüber keinen Zugang zu Anhängen bekommen.
- **FR-015**: Alle neuen Texte der Oberfläche MÜSSEN auf Deutsch und Englisch vorliegen.

### Key Entities

- **Anhang** (aus 034): Dateiname, Größe, Prüfsumme und Daten in der Vault-Datenbank; die Art (Bild,
  Text, PDF, andere) folgt aus der Endung des Dateinamens.
- **Hinweis-Bestätigung für Anhänge**: je Vault und Gerät, ob der Nutzer „Nicht mehr fragen“ gewählt
  hat; wirkt nur auf diesem Gerät.
- **Klartext-Kopie**: eine Datei je Öffnen im eigenen Ordner von holzi; lebt höchstens bis zum Sperren
  oder Schließen der Vault.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer liest den Inhalt eines Textanhangs mit einem Tipp, ohne holzi zu verlassen
  und ohne eine Datei auf dem Gerät anzulegen.
- **SC-002**: Ein Textanhang bis 1 MB und die erste Seite eines PDF-Anhangs bis 10 MB erscheinen auf
  einem aktuellen Desktop-Rechner in unter 1 Sekunde nach dem Tippen.
- **SC-003**: Derselbe Text- oder PDF-Anhang sieht in der Anhang-Ansicht und im Datei-Viewer gleich aus
  und hat dieselben Bedienelemente.
- **SC-004**: Nach dem Sperren der Vault liegt keine Klartext-Kopie eines Anhangs mehr in holzis
  Ordnern, in 100 % der Fälle, auch nach einem Absturz beim nächsten Start.
- **SC-005**: Kein Text, kein PDF und keine Klartext-Kopie eines Anhangs entsteht ohne Handlung des
  Nutzers: Vorschauen außer Bildern gibt es nicht.

## Assumptions

- Text wird als UTF-8 gelesen; andere Kodierungen erscheinen mit Ersatzzeichen. Eine Auswahl der
  Kodierung gehört nicht zu dieser Spec.
- Die Anhang-Ansicht zeigt einen Anhang zur Zeit; Blättern zum nächsten Anhang des Eintrags gibt es
  nur in der Lightbox der Bilder (036).
- Bearbeiten eines Textanhangs in holzi gehört nicht zu dieser Spec. Wer ihn ändern will, speichert
  ihn, ändert ihn außerhalb und hängt ihn neu an; die Klartext-Kopie aus „Mit System-App öffnen“ wird
  nicht zurück in die Vault übernommen.
- Die Lightbox der Bilder bekommt kein „Mit System-App öffnen“; dafür gibt es die Karte.
- Kartenvorschauen für Text und PDF (etwa die erste Zeile oder die erste Seite) gibt es nicht; die
  Karten zeigen weiter das Symbol der Art.
- Für „Nicht mehr fragen“ gibt es in dieser Spec keine eigene Einstellung zum Zurücknehmen, wie in 048.
- Markdown erscheint als Text, nicht gerendert.
- Der Verlauf eines Eintrags zeigt Anhänge alter Stände weiter nur als Namen (036); sie dort zu
  öffnen gehört nicht zu dieser Spec.
