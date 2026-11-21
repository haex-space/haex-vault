# Feature Specification: Agent-Rückfrage mit Auswahl

**Feature Branch**: `046-agent-choice-prompt`

**Created**: 2026-10-08

**Status**: Draft

**Input**: User description: "ich möchte den namen der erweiterung auch mal falsch schreiben können und der agent sollte sie entweder finden, oder mir eine selektion frage stellen in der art. deine app habe ich nicht gefunden. Meintest du vielleicht: x, y oder z? dann will ich via radio group eins auswählen können und das sollte dann geöffnet werden. so hätte ich das allgemein gerne vom agent. wenn er mit meiner anweisung nichts anfangen kann, dann soll er eine frage stellen und alternativen anbieten"

Abgestimmter Entwurf: [`docs/plans/2026-10-08-agent-choice-prompt-design.md`](../../docs/plans/2026-10-08-agent-choice-prompt-design.md).

## Beziehung zu bestehenden Specs

- [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md): Der eingebaute Agent bedient
  HaexVault über Aktionen. Diese Spec ergänzt zwei Dinge: Aktionen können statt eines Fehlers „Auswahl
  nötig“ mit Kandidaten melden, und der Agent kann den Nutzer selbst etwas fragen. FR-006 (Fehler ohne
  interne Details an das Modell) gilt weiter; „Auswahl nötig“ ist kein Fehler im Sinne von FR-006,
  sondern trägt nur Kandidaten, die der Nutzer ohnehin sehen darf. Die Freigabe-Regeln FR-007 bis
  FR-009 gelten unverändert, siehe FR-012 dieser Spec.
- [`020-tab-navigation`](../020-tab-navigation/spec.md): `wm.app.open` und `wm.tab.new` bekommen eine
  tolerante Auflösung der App-Angabe. Eine gültige App-ID wirkt genau wie bisher.
- [`017-extension-host`](../017-extension-host/spec.md): haextensions sind Apps mit einer ID, die kein
  Mensch und kein Modell aus ihrem Namen ableiten kann. Diese Spec macht sie über ihren Namen
  erreichbar.
- [`003-agent-tool-loop`](../003-agent-tool-loop/spec.md): Die Rückfrage hält den laufenden Turn an
  wie eine Zustimmungsabfrage und setzt ihn mit der Antwort fort.

## Begriffe

- **Rückfrage**: eine Frage des Agenten an den Nutzer im Chat, mit vorgeschlagenen Antworten. Der Turn
  wartet, bis der Nutzer antwortet oder abbricht.
- **Kandidat**: eine vorgeschlagene Antwort einer Rückfrage, mit einem für den Nutzer lesbaren Namen
  und dem Wert, mit dem weitergearbeitet wird (bei Apps: der Name der App und ihre ID).
- **Klarer Treffer**: die App-Angabe passt zu genau einer App so gut, dass eine Verwechslung nicht zu
  erwarten ist.
- **Auswahl nötig**: das Ergebnis einer Aktion, die ihre Eingabe nicht eindeutig zuordnen kann und
  dafür Kandidaten nennt.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - App per Name öffnen, auch mit Tippfehler (Priority: P1)

Ich bitte den Agenten „öffne haex-mial“ oder „öffne Notizen“. Der Agent findet die gemeinte App,
auch wenn ich mich vertippe oder nur einen Teil des Namens nenne, und öffnet sie. Ich muss keine
technische ID kennen.

**Why this priority**: Das ist der gemeldete Fehler: Erweiterungen ließen sich per Chat überhaupt
nicht öffnen.

**Independent Test**: Erweiterung „haex-mail“ installieren, im Chat „öffne haex-mial“ schreiben,
prüfen, dass haex-mail ohne Rückfrage geöffnet wird.

**Acceptance Scenarios**:

1. **Given** die Erweiterung „haex-mail“ ist installiert, **When** der Nutzer „öffne haex-mail“
   schreibt, **Then** öffnet sich haex-mail.
2. **Given** dieselbe Lage, **When** der Nutzer „öffne haex-mial“ schreibt, **Then** öffnet sich
   haex-mail ohne Rückfrage.
3. **Given** die System-App Einstellungen, **When** der Nutzer „öffne die Einstellungen“ schreibt,
   **Then** öffnen sich die Einstellungen.
4. **Given** der Agent verwendet eine ID, die es nicht gibt, die aber klar zu einer App passt (etwa
   „system.notes“ bei installiertem „haex-notes“), **When** die Aktion ausgeführt wird, **Then**
   öffnet sich haex-notes.

---

### User Story 2 - Rückfrage mit Auswahl, wenn die App nicht eindeutig ist (Priority: P1)

Passt meine Angabe zu mehreren Apps oder zu keiner gut genug, öffnet der Agent nichts und lehnt auch
nicht ab. Stattdessen erscheint im Chat: „Ich habe die App nicht eindeutig gefunden. Meintest du
…?“ mit den passendsten Apps als Auswahl. Ich wähle eine aus, und genau die wird geöffnet.

**Why this priority**: Ohne Rückfrage endet jede unklare Angabe wieder in „nicht möglich“. Diese
Story macht User Story 1 robust.

**Independent Test**: Zwei Erweiterungen mit ähnlichem Namen installieren (etwa „haex-mail“ und
„haex-notes“), „öffne haex“ schreiben, prüfen, dass die Rückfrage beide anbietet und die gewählte
App öffnet.

**Acceptance Scenarios**:

1. **Given** „haex-mail“, „haex-notes“ und „haex-files“ sind installiert, **When** der Nutzer „öffne
   haex“ schreibt, **Then** erscheint eine Rückfrage mit diesen Apps als Auswahl, und es öffnet sich
   noch nichts.
2. **Given** die Rückfrage aus 1, **When** der Nutzer „haex-notes“ wählt und bestätigt, **Then**
   öffnet sich haex-notes und der Agent meldet das Ergebnis.
3. **Given** die Rückfrage aus 1, **When** der Nutzer „Etwas anderes …“ wählt und „Einstellungen“
   eingibt, **Then** öffnen sich die Einstellungen.
4. **Given** die Rückfrage aus 1, **When** der Nutzer abbricht, **Then** öffnet sich nichts und der
   Agent bestätigt, dass nichts geöffnet wurde.
5. **Given** der Nutzer gibt unter „Etwas anderes …“ wieder eine mehrdeutige Angabe ein, **When** er
   bestätigt, **Then** erscheint eine neue Rückfrage zu dieser Angabe.

---

### User Story 3 - Der Agent fragt nach, statt eine Anweisung abzulehnen (Priority: P2)

Gebe ich dem Agenten eine Anweisung, die er nicht eindeutig umsetzen kann, fragt er nach und bietet
mir passende Möglichkeiten an, statt zu antworten, das gehe nicht. Ich wähle eine Möglichkeit oder
schreibe frei, und der Agent macht damit weiter.

**Why this priority**: Das ist das allgemeine Verhalten, das der Nutzer sich wünscht. Es baut auf
derselben Rückfrage wie User Story 2 auf, hängt aber davon ab, dass das Modell die Rückfrage von sich
aus stellt, und ist deshalb weniger verlässlich als der feste Weg für Apps.

**Independent Test**: Im Chat eine mehrdeutige Anweisung geben, etwa „mach das dunkler“, prüfen, dass
eine Rückfrage mit Möglichkeiten erscheint (etwa dunkles Farbschema oder ein anderes Thema) und die
gewählte umgesetzt wird.

**Acceptance Scenarios**:

1. **Given** ein Modell mit Werkzeugnutzung, **When** der Nutzer eine Anweisung gibt, die mehrere
   sinnvolle Deutungen hat, **Then** stellt der Agent eine Rückfrage mit diesen Deutungen als
   Auswahl.
2. **Given** die Rückfrage aus 1, **When** der Nutzer eine Möglichkeit wählt, **Then** setzt der
   Agent im selben Turn genau diese um.
3. **Given** die Rückfrage aus 1, **When** der Nutzer abbricht, **Then** führt der Agent nichts aus
   und sagt das.

---

### Edge Cases

- **Nur ein Kandidat, aber kein klarer Treffer** (z. B. „öffne Kalender“, und es gibt nur eine
  entfernt ähnliche App): Rückfrage mit diesem einen Kandidaten plus „Etwas anderes …“ und
  Abbrechen, nicht automatisch öffnen.
- **Gar kein Kandidat**: Rückfrage ohne Vorschläge, nur „Etwas anderes …“ und Abbrechen, mit dem
  Hinweis, dass keine passende App gefunden wurde.
- **Gewählte App ist inzwischen weg** (deinstalliert oder auf diesem Gerät nicht verfügbar, während
  die Rückfrage offen war): Die Aktion meldet das als Fehler an das Modell; es öffnet sich nichts.
- **App ist installiert, aber auf diesem Gerät nicht verfügbar** (Erweiterung wird noch übertragen):
  erscheint als Kandidat mit dem Hinweis, warum sie gerade nicht öffnet, und ist nicht wählbar, wie
  im Launcher.
- **Turn wird abgebrochen** (Stopp-Button) oder die App beendet, während eine Rückfrage offen ist:
  Die Rückfrage verschwindet, nichts wird ausgeführt; der Verlauf zeigt die Rückfrage als
  unbeantwortet.
- **Der Nutzer antwortet lange nicht**: Die Rückfrage bleibt offen, solange der Turn läuft; sie läuft
  nicht von selbst ab.
- **Mehrere Rückfragen in einem Turn** (zwei Aktionen eines Schritts brauchen beide eine Auswahl):
  Sie erscheinen nacheinander, die älteste zuerst.
- **Externe Agenten** (über MCP): bekommen „Auswahl nötig“ mit den Kandidaten als Ergebnis und
  entscheiden selbst; HaexVault zeigt für sie keine Rückfrage an.
- **Eine gültige App-ID**: wirkt wie bisher, ohne Suche und ohne Rückfrage.

## Requirements _(mandatory)_

### Functional Requirements

**App-Angabe auflösen**

- **FR-001**: Die Aktionen zum Öffnen einer App MÜSSEN als App-Angabe neben der App-ID auch einen
  Namen oder Teil eines Namens annehmen, in beliebiger Groß- und Kleinschreibung und mit kleinen
  Tippfehlern.
- **FR-002**: Die Auflösung MUSS in dieser Reihenfolge prüfen: gültige App-ID, dann eine ersetzte
  App-ID (Alias), dann die Ähnlichkeit zu den Namen aller öffnenbaren Apps, System-Apps und
  haextensions gleichermaßen.
- **FR-003**: Bei genau einem klaren Treffer MUSS die Aktion diese App ohne Rückfrage öffnen.
- **FR-004**: Ohne klaren Treffer DARF die Aktion nichts öffnen und MUSS „Auswahl nötig“ melden, mit
  höchstens 5 Kandidaten, nach Ähnlichkeit geordnet, jeweils mit dem Namen der App, wie ihn der
  Launcher zeigt, und ihrer ID.

**Rückfrage**

- **FR-005**: Meldet eine vom eingebauten Agenten aufgerufene Aktion „Auswahl nötig“, MUSS das System
  dem Nutzer im Chat dieser Unterhaltung eine Rückfrage zeigen, ohne dass das Modell dafür einen
  weiteren Schritt machen muss.
- **FR-006**: Die Rückfrage MUSS zeigen: eine verständliche Frage („Meintest du …?“), die Kandidaten
  als Einfachauswahl, die Möglichkeit „Etwas anderes …“ mit einem Textfeld, Bestätigen und Abbrechen.
- **FR-007**: Wählt der Nutzer einen Kandidaten, MUSS das System dieselbe Aktion mit dessen Wert
  erneut ausführen und das Ergebnis an das Modell geben.
- **FR-008**: Gibt der Nutzer unter „Etwas anderes …“ Text ein, MUSS das System dieselbe Aktion mit
  diesem Text als Angabe erneut ausführen; FR-003 bis FR-007 gelten dafür erneut.
- **FR-009**: Bricht der Nutzer ab, MUSS das Modell die Ablehnung als Ergebnis erhalten, und es DARF
  nichts ausgeführt werden.
- **FR-010**: Die Rückfrage und die Antwort des Nutzers MÜSSEN im Verlauf der Unterhaltung sichtbar
  bleiben, auch nach dem Neuöffnen (wie Spec 032 FR-005).
- **FR-011**: Während eine Rückfrage offen ist, MUSS der Turn warten, ohne Zeitlimit; Abbrechen des
  Turns MUSS die Rückfrage schließen, ohne etwas auszuführen.
- **FR-012**: Die Freigabe-Regeln (Spec 032 FR-007) MÜSSEN vor der ersten Ausführung gelten wie
  bisher. Die Auswahl in der Rückfrage MUSS als Zustimmung zur gewählten Ausführung gelten; eine
  zweite Zustimmungsabfrage für dieselbe Aktion DARF NICHT folgen.

**Rückfrage durch den Agenten selbst**

- **FR-013**: Das System MUSS dem eingebauten Agenten ein Werkzeug anbieten, mit dem er den Nutzer
  etwas fragen und 2 bis 5 Antworten vorschlagen kann. Die Rückfrage MUSS dieselbe Darstellung und
  dieselben Antwortmöglichkeiten haben wie in FR-006.
- **FR-014**: Das Ergebnis dieses Werkzeugs an das Modell MUSS die gewählte Antwort, den
  eingegebenen Text oder die Ablehnung sein.
- **FR-015**: Dieses Werkzeug MUSS in jedem Schritt angeboten werden, in dem der Agent Werkzeuge
  bekommt, und DARF in keinem Freigabe-Modus eine Zustimmung erfordern, weil es nichts verändert.
- **FR-016**: Die Anweisungen an den Agenten MÜSSEN festlegen: Kann er eine Anweisung nicht eindeutig
  umsetzen, fragt er mit diesem Werkzeug nach und bietet Möglichkeiten an, statt die Anweisung
  abzulehnen.

**Prüfung**

- **FR-017**: Der Satz von Beispielsätzen der Modellprüfung (Spec 032 FR-019) MUSS um Sätze mit
  Tippfehlern im App-Namen und um mehrdeutige Anweisungen erweitert werden, bei denen das Werkzeug
  aus FR-013 als erwartete Aktion gilt; in Deutsch und Englisch.

### Key Entities

- **Rückfrage**: Frage, Kandidaten, Herkunft (eine Aktion mit dem betroffenen Eingabefeld oder der
  Agent selbst), Zustand (offen, beantwortet, abgebrochen) und Antwort (Kandidat, Text oder
  Ablehnung). Gehört zu genau einem Turn einer Unterhaltung.
- **Kandidat**: Anzeigename und Wert; bei Apps zusätzlich, ob die App auf diesem Gerät gerade
  verfügbar ist.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Jede installierte, verfügbare App lässt sich per Chat mit ihrem Namen öffnen, wie ihn
  der Launcher zeigt; das gilt für 100 % der System-Apps und haextensions.
- **SC-002**: Bei einem Tippfehler von einem Zeichen in einem eindeutigen App-Namen öffnet sich die
  richtige App ohne Rückfrage, in mindestens 9 von 10 geprüften Fällen.
- **SC-003**: In keinem Fall öffnet der Agent eine App, wenn die Angabe zu zwei oder mehr Apps
  ähnlich gut passt; stattdessen erscheint die Rückfrage.
- **SC-004**: Vom Absenden „öffne <App mit Tippfehler>“ bis zur geöffneten App oder zur Rückfrage
  braucht der Nutzer keinen weiteren Schritt; nach einer Rückfrage genau einen (Auswahl bestätigen).
- **SC-005**: Mit dem empfohlenen lokalen Modell endet keine der App-Öffnen-Anweisungen der
  Modellprüfung mehr mit einer Absage wie „nicht möglich“, wenn die gemeinte App installiert ist.
- **SC-006**: Bei mehrdeutigen Anweisungen der Modellprüfung stellt der Agent in mindestens 7 von
  10 Fällen eine Rückfrage, statt abzulehnen oder zu raten.

## Assumptions

- Die tolerante Auflösung und die feste Rückfrage gelten in dieser Spec nur für die App-Angabe. Der
  Mechanismus „Auswahl nötig“ ist allgemein; weitere Aktionen (Dateien, Passwörter, Tabs) können ihn
  später nutzen, ohne dass diese Spec das festlegt.
- Ein falsch geöffnetes App-Fenster richtet keinen Schaden an und ist mit einem Klick geschlossen.
  Deshalb ist automatisches Öffnen bei einem klaren Treffer vertretbar; die Schwelle für „klar“ legt
  der Plan fest.
- Modelle ohne Werkzeugnutzung (Spec 032 FR-016) bekommen das Werkzeug aus FR-013 nicht; für sie
  ändert sich nichts.
- User Story 3 hängt davon ab, dass das Modell die Rückfrage von sich aus stellt. Kleine lokale
  Modelle tun das nicht verlässlich; SC-006 ist darauf ausgelegt. Der feste Weg aus User Story 2
  hängt davon nicht ab.
- Diese Spec baut auf dem Fix `fix/agent-unknown-app` auf, der eine unbekannte App-ID als korrigierbaren
  Eingabefehler statt als verdeckten Fehler an das Modell gibt.
- Rückfragen werden nicht zwischen Geräten synchronisiert, solange sie offen sind; im Verlauf
  erscheinen sie wie andere Werkzeug-Zeilen.
