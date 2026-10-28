# Feature Specification: Orchestrator und Modell-Routing

**Feature Branch**: `068-task-orchestration`

**Created**: 2026-10-28

**Status**: Draft

**Input**: User description: „ich würde den workflow aber vermutlich noch weiter unterteilen wollen,
damit nicht alles von einem agent/llm gemacht werden muss. das erfassen der sprache und umwandeln in
text macht ja schon whispers. dann soll das ergebnis zu einem orchestrator geschickt werden, der zb
ein haiku sein kann. dieser agent entscheidet dann, ob er die aufgabe direkt weiter den ausführenden
agent weiterleiten kann, oder ob er noch rückfragen stellen muss. zb um berechtigungen einzuholen.
danach soll die aufgabe an den ausführenden agent geschickt werden, der je nach aufgabe
unterschiedlich sein kann. für leichte aufgaben reicht vielleicht das lokale oder ein luna oder haiku
modell, für komplexere aufgaben wie coding wird es ein opus, sonnet etc brauchen. vielleicht macht es
sinn, hier routerllm oder jev zu nutzen für diese entscheidung. wenn der ausführende agent dann
fertig ist mit der aufgabe, gibt er sein ergebnis an den orchestrator zurück und dieser berichtet dann
dem nutzer davon und wartet auf neue instruktionen“ (Betreiber, 2026-10-28)

## Clarifications

### Session 2026-10-28

- Q: Wo wird die Orchestrierung spezifiziert? → A: In dieser eigenen Spec. Sie gilt für den Chat,
  den Sprachassistenten (066) und die Meeting-Auswertung (067). Bis sie umgesetzt ist, arbeitet dort
  direkt der eingebaute Agent.
- Q: Wo darf der Orchestrator laufen? → A: Wählbar, Standard ist ein lokales Modell. Wählt der Nutzer
  ein Cloud-Modell (etwa Haiku), zeigt haex dauerhaft an, dass Nachrichten das Gerät verlassen.
- Q: Wie wird der ausführende Agent gewählt? → A: Der Orchestrator ordnet jede Aufgabe einer Stufe
  zu (lokal, klein, groß, Delegate); welches Modell hinter jeder Stufe steht, legt der Nutzer fest.
  Ein gelernter Router ist nicht Teil dieser Spec.
- Q: Dürfen mehrere Aufgaben gleichzeitig laufen? → A: Ja. Jede läuft für sich; der Orchestrator
  meldet jede Fertigstellung und jede Rückfrage einzeln.
- Planung (2026-10-28): Delegates arbeiten heute immer in einem Wegwerf-Ordner; für Programmierarbeit
  braucht eine Aufgabe einen Projektordner (FR-026). Es ist höchstens ein lokales Modell geladen
  (FR-027). Jev und RouteLLM wurden geprüft und nicht übernommen (research.md R9).

## Begriffe

- **Orchestrator**: das Modell, mit dem der Nutzer spricht oder schreibt. Es versteht das Anliegen,
  stellt Rückfragen, beantwortet Einfaches selbst, übergibt Aufgaben an ausführende Agenten, überbringt
  deren Rückfragen und berichtet die Ergebnisse. Es führt selbst keine ändernden Aktionen aus.
- **Ausführender Agent** (kurz Ausführer): der Agent, der eine Aufgabe mit Werkzeugen erledigt: der
  eingebaute Agent (003/032) mit dem Modell einer Stufe oder ein Delegate (007/009).
- **Stufe**: eine Klasse von Ausführern, nach Aufwand geordnet: **lokal** (Modell auf dem Gerät),
  **klein** (schnelles Cloud-Modell, etwa Haiku), **groß** (starkes Cloud-Modell, etwa Sonnet oder
  Opus), **Delegate** (Claude Code, Codex; für Programmierarbeit). Der Nutzer belegt jede Stufe mit
  einem Modell oder Delegate.
- **Auftrag**: was der Orchestrator einem Ausführer übergibt: Ziel, nötiger Kontext, bereits geklärte
  Antworten des Nutzers, gewählte Stufe mit Begründung.
- **Rückkanal**: der Weg, auf dem ein Ausführer während der Arbeit eine Rückfrage oder Freigabe an den
  Orchestrator meldet, der sie dem Nutzer stellt und die Antwort zurückgibt.
- **Bericht**: was ein Ausführer am Ende zurückgibt: Zustand (erledigt, teilweise, gescheitert,
  abgebrochen), Ergebnis, offene Punkte.
- **Aufgabe**: ein Auftrag samt Zustand, Ausführer, Verlauf und Bericht. Mehrere Aufgaben können
  gleichzeitig laufen.

## Beziehung zu bestehenden Specs

- [`003-agent-tool-loop`](../003-agent-tool-loop/spec.md) und
  [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md): Der eingebaute Agent bleibt der
  Ausführer für die Stufen lokal, klein und groß, mit unveränderten Werkzeugen, Freigabestufen,
  Leitplanken und Rundengrenzen. Neu ist nur, wer ihn startet (der Orchestrator) und mit welchem
  Modell.
- [`007-cli-delegate`](../007-cli-delegate/spec.md) und
  [`009-autonomous-delegate-mode`](../009-autonomous-delegate-mode/spec.md): Delegates sind die Stufe
  „Delegate“. Ihre Freigaben pro Werkzeugaufruf laufen schon über die Freigabe-Logik von haex; der
  Rückkanal nutzt das. haex baut keinen eigenen Programmier-Agenten: Programmierarbeit geht an einen
  Delegate.
- [`021-mcp-server`](../021-mcp-server/spec.md): Delegates bekommen haex-Werkzeuge über den MCP-Zugang
  (021 PR F). Bis dahin erledigen Delegates nur Aufgaben ohne haex-Werkzeuge; der Orchestrator
  berücksichtigt das bei der Wahl.
- [`012-unified-model-capabilities`](../012-unified-model-capabilities/spec.md): Als Orchestrator und
  für Stufen mit Werkzeugen kommen nur Modelle mit der Fähigkeit „Werkzeuge“ in Frage.
- Spec 066 (Voice Assistant): Der Sprachassistent spricht mit dem Orchestrator. Quittung,
  Fortschrittsansage, Meldungen aus dem Hintergrund, offene Fragen und
  aktives Beenden (066 FR-023, FR-039, FR-040) gelten pro Aufgabe.
- Spec 067 (Meeting Transcription): Die Auswertung eines Meetings ist eine Aufgabe, die der
  Orchestrator einer Stufe zuordnet.
- [`006-chat-history-management`](../006-chat-history-management/spec.md): Gespräche mit dem
  Orchestrator sind Chat-Unterhaltungen; Aufgaben erscheinen darin.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Aufgabe an den passenden Agenten (Priority: P1)

Anna schreibt im Chat: „Fasse mir die drei neuesten Mails von Jonas zusammen.“ Der Orchestrator
antwortet sofort: „Mach ich.“ Er gibt die Aufgabe an die Stufe „klein“. Der Ausführer liest die Mails,
fasst zusammen und berichtet. Der Orchestrator gibt Anna die Zusammenfassung in seinen Worten und
wartet. Später bittet sie: „Bau in meinem Projekt X einen Export als CSV ein.“ Der Orchestrator gibt
das an den Delegate (Claude Code) und meldet sich, als er fertig ist.

**Why this priority**: Das ist die gewünschte Aufteilung: ein schnelles Gesprächsmodell vorn, die
passende Rechenkraft dahinter.

**Independent Test**: Mit einer Stufenbelegung aus Testmodellen drei Aufgaben unterschiedlichen
Umfangs geben. Jede landet bei der erwarteten Stufe, der Bericht kommt beim Orchestrator an und wird
dem Nutzer mitgeteilt; Stufe und Begründung sind im Verlauf sichtbar.

**Acceptance Scenarios**:

1. **Given** eine Anweisung, die Werkzeuge braucht, **When** der Orchestrator sie verstanden hat,
   **Then** quittiert er sofort und übergibt einen Auftrag an genau eine Stufe.
2. **Given** ein Auftrag, **When** der Ausführer arbeitet, **Then** gelten seine Werkzeuge,
   Freigabestufen und Grenzen unverändert (003/032 bzw. 007/009).
3. **Given** der Ausführer ist fertig, **When** er berichtet, **Then** teilt der Orchestrator dem
   Nutzer das Ergebnis mit, auch was nicht geklappt hat, und wartet auf neue Anweisungen.
4. **Given** Programmierarbeit, **When** ein Delegate eingerichtet ist, **Then** wählt der
   Orchestrator die Stufe „Delegate“.
5. **Given** eine gewählte Stufe ist nicht verfügbar (kein Schlüssel, offline, Delegate nicht
   installiert), **When** der Orchestrator übergeben will, **Then** nimmt er die nächste passende
   verfügbare Stufe und sagt das.
6. **Given** ein Ausführer scheitert, **When** er das berichtet, **Then** nennt der Orchestrator den
   Grund in einfachen Worten und bietet an, es mit der nächsthöheren Stufe erneut zu versuchen.

---

### User Story 2 - Rückfragen vor und während der Ausführung (Priority: P1)

Anna sagt: „Schick die Präsentation an das Team.“ Der Orchestrator fragt zuerst: „Welche
Präsentation, ‚Quartal‘ oder ‚Roadmap‘?“ Erst danach übergibt er. Während der Ausführer arbeitet,
braucht er eine Freigabe zum Senden; der Orchestrator fragt Anna: „Der Agent möchte die Mail an fünf
Personen senden. Soll er?“ Sie sagt „Ja“, und der Ausführer macht weiter.

**Why this priority**: Rückfragen vorab sparen Fehlläufe teurer Modelle; der Rückkanal ist nötig,
weil sich viele Freigaben erst während der Arbeit zeigen.

**Independent Test**: Eine mehrdeutige Anweisung führt zu einer Rückfrage vor der Übergabe. Ein
Ausführer, der eine Freigabe braucht, hält an; die Frage erscheint beim Orchestrator, die Antwort geht
zurück, und die Aktion läuft genau dann, wenn die feste Freigabe-Regel und der Nutzer zustimmen.

**Acceptance Scenarios**:

1. **Given** eine Anweisung mit fehlenden oder mehrdeutigen Angaben, **When** der Orchestrator sie
   nicht eindeutig zuordnen kann, **Then** fragt er nach, bevor er übergibt.
2. **Given** ein Ausführer braucht eine Freigabe, **When** sie ansteht, **Then** stellt der
   Orchestrator sie dem Nutzer mit seiner Erklärung sowie Werkzeugname und Eingabe des
   ursprünglichen Aufrufs und gibt die Antwort zurück.
3. **Given** eine Freigabe, **When** sie entschieden wird, **Then** entscheidet allein die feste
   Freigabe-Regel zusammen mit der Antwort des Nutzers; der Orchestrator kann nie selbst freigeben.
4. **Given** ein Ausführer braucht eine inhaltliche Auskunft („Welcher Ordner?“), **When** er sie
   meldet, **Then** stellt der Orchestrator die Frage und gibt die Antwort als Ergänzung des Auftrags
   zurück.
5. **Given** eine Rückfrage bleibt unbeantwortet, **When** der Nutzer nichts sagt, **Then** wartet der
   Ausführer, die Aufgabe steht auf „wartet auf dich“, und nichts wird ausgeführt.

---

### User Story 3 - Einfaches beantwortet der Orchestrator selbst (Priority: P1)

Anna fragt: „Wie spät ist es?“ oder „Danke, gut gemacht.“ Der Orchestrator antwortet direkt, ohne
einen Ausführer zu starten.

**Why this priority**: Jeder zusätzliche Modellaufruf kostet Zeit und Geld; Smalltalk und einfache
Auskünfte müssen so schnell sein wie heute (066 SC-002).

**Independent Test**: Zwanzig einfache Fragen und Bemerkungen; keine startet eine Aufgabe, alle
werden direkt beantwortet.

**Acceptance Scenarios**:

1. **Given** eine Frage ohne Werkzeugbedarf, **When** der Orchestrator sie beantworten kann, **Then**
   antwortet er selbst, und es entsteht keine Aufgabe.
2. **Given** eine Frage zu einer laufenden oder erledigten Aufgabe („Wie weit bist du mit der Mail?“),
   **When** der Orchestrator sie stellt, **Then** antwortet er aus Zustand und Bericht der Aufgabe,
   ohne den Ausführer zu unterbrechen.
3. **Given** der Orchestrator darf selbst nur lesende Auskünfte geben, **When** eine Anweisung etwas
   ändern würde, **Then** übergibt er sie immer an einen Ausführer.

---

### User Story 4 - Mehrere Aufgaben gleichzeitig (Priority: P2)

Anna lässt einen Bericht erstellen und sagt, während der läuft: „Und such mir einen Termin mit Mia
nächste Woche.“ Beide laufen. Als der Termin fertig ist, meldet der Orchestrator ihn; später den
Bericht. Zwischendurch sagt sie: „Brich den Bericht ab“, und nur dieser endet.

**Why this priority**: „Erledige das und gib mir Bescheid“ (066) wird erst nützlich, wenn man dabei
weitermachen kann.

**Independent Test**: Drei Aufgaben nacheinander starten; alle laufen gleichzeitig, jede Meldung und
Rückfrage ist ihrer Aufgabe eindeutig zugeordnet, das Abbrechen einer beendet nur diese.

**Acceptance Scenarios**:

1. **Given** eine laufende Aufgabe, **When** der Nutzer eine neue gibt, **Then** startet sie, ohne die
   erste anzuhalten.
2. **Given** mehrere Aufgaben, **When** eine fertig wird oder eine Rückfrage hat, **Then** nennt der
   Orchestrator, zu welcher Aufgabe die Meldung gehört.
3. **Given** mehrere Aufgaben, **When** der Nutzer eine benennt und abbricht, **Then** endet nur diese,
   samt Werkzeugaufrufen.
4. **Given** eine neue Aufgabe betrifft dasselbe Ziel wie eine laufende (dieselbe Datei, dieselbe
   Mail), **When** der Orchestrator das erkennt, **Then** zeigt er die kollidierende Aufgabe und
   fragt, ob die neue warten oder die laufende beendet werden soll; vor der Antwort startet keine.
5. **Given** mehrere offene Rückfragen, **When** der Nutzer antwortet, **Then** stellt der
   Orchestrator sie nacheinander, älteste zuerst, und ordnet jede Antwort ihrer Aufgabe zu.
6. **Given** die Grenze gleichzeitiger Aufgaben ist erreicht, **When** eine weitere kommt, **Then**
   reiht der Orchestrator sie ein und sagt das.

---

### User Story 5 - Stufen und Orchestrator einstellen (Priority: P2)

In den Einstellungen sieht Anna die vier Stufen mit ihrer Belegung: lokal = Qwen3-4B, klein = Haiku,
groß = Sonnet, Delegate = Claude Code. Sie stellt „groß“ auf Opus um. Den Orchestrator lässt sie auf
dem lokalen Modell.

**Why this priority**: Welche Modelle jemand hat und bezahlen will, ist verschieden; ohne Belegung
gibt es kein Routing.

**Independent Test**: Belegungen ändern; die nächste Aufgabe der Stufe nutzt das neue Modell ohne
Neustart. Orchestrator auf ein Cloud-Modell stellen; die Kennzeichnung erscheint.

**Acceptance Scenarios**:

1. **Given** die Einstellungen, **When** der Nutzer die Orchestrierung öffnet, **Then** sieht er das
   Orchestrator-Modell und die vier Stufen mit ihrer Belegung, vorbelegt aus den vorhandenen Modellen,
   Schlüsseln und Delegates.
2. **Given** eine Stufe, **When** der Nutzer sie belegt, **Then** bietet haex nur Modelle mit der
   Fähigkeit „Werkzeuge“ an (bzw. installierte Delegates für die Stufe „Delegate“); eine Stufe darf
   leer bleiben.
3. **Given** der Orchestrator läuft auf einem Cloud-Modell, **When** der Nutzer haex nutzt, **Then**
   zeigt haex dauerhaft, dass Nachrichten an diesen Anbieter gehen.
4. **Given** der Orchestrator ist auf einem Cloud-Modell und das Netz fehlt, **When** der Nutzer etwas
   sagt, **Then** übernimmt das lokale Modell und haex sagt das einmal.
5. **Given** die Orchestrierung ist ausgeschaltet, **When** der Nutzer chattet, **Then** arbeitet
   haex wie heute mit dem gewählten Chat-Modell direkt.
6. **Given** der lokale Orchestrator und die lokale Aufgabenstufe sind auf verschiedene lokale
   Modelle eingestellt, **When** der Nutzer die Einstellungen öffnet, **Then** sieht er den Hinweis,
   dass lokale Aufgaben warten, während der Orchestrator spricht (FR-027).

---

### User Story 6 - Nachvollziehen, wer was gemacht hat (Priority: P2)

Anna öffnet eine erledigte Aufgabe im Chat: Sie sieht den Auftrag, die gewählte Stufe mit Modell und
Begründung („braucht Werkzeuge, mittlerer Umfang“), die Schritte des Ausführers, die Rückfragen samt
Antworten, den Bericht und die verbrauchten Tokens.

**Why this priority**: Wer mehrere Modelle einsetzt, muss sehen können, welches was getan hat, um der
Wahl zu vertrauen und sie zu korrigieren.

**Independent Test**: Nach einer Aufgabe mit einer Rückfrage zeigt die Aufgabenansicht Auftrag,
Stufe, Modell, Begründung, Schritte, Rückfrage, Antwort, Bericht und Verbrauch.

**Acceptance Scenarios**:

1. **Given** eine Aufgabe, **When** der Nutzer sie öffnet, **Then** sieht er Auftrag, Stufe, Modell,
   Begründung, Schritte, Rückfragen mit Antworten, Bericht und Zeiten.
2. **Given** der Anbieter meldet Verbrauch, **When** die Aufgabe endet, **Then** zeigt sie Tokens
   (und, wo bekannt, Kosten) getrennt nach Orchestrator und Ausführer.
3. **Given** eine Unterhaltung mit Aufgaben, **When** der Nutzer sie im Chat ansieht, **Then**
   erscheint jede Aufgabe als Karte mit Zustand, die sich aufklappen lässt.

---

### User Story 7 - Wahl übersteuern (Priority: P3)

Anna sagt: „Mach das mit dem großen Modell“ oder nach einem schwachen Ergebnis: „Nochmal, aber mit
Opus.“ Der Orchestrator folgt.

**Why this priority**: Die automatische Wahl irrt manchmal; der Nutzer braucht einen einfachen Weg,
sie zu korrigieren.

**Independent Test**: Eine Aufgabe mit genannter Stufe oder genanntem Modell landet dort; „nochmal mit
…“ startet dieselbe Aufgabe neu bei der genannten Stufe.

**Acceptance Scenarios**:

1. **Given** der Nutzer nennt eine Stufe oder ein belegtes Modell, **When** er eine Aufgabe gibt,
   **Then** übergibt der Orchestrator genau dorthin.
2. **Given** eine erledigte Aufgabe, **When** der Nutzer „nochmal mit …“ sagt, **Then** startet eine
   neue Aufgabe mit demselben Auftrag bei der genannten Stufe; die alte bleibt im Verlauf.
3. **Given** im Chat, **When** der Nutzer an einer Aufgabenkarte „Mit anderer Stufe wiederholen“
   wählt, **Then** geschieht dasselbe.

---

### Edge Cases

- **Ausführer überschreitet die Rundengrenze**: Er berichtet „teilweise“ mit Stand; der Orchestrator
  bietet Fortsetzen oder eine höhere Stufe an.
- **Delegate ohne haex-Werkzeuge** (vor 021 PR F): Der Orchestrator gibt einem Delegate nur Aufgaben,
  die keine haex-Werkzeuge brauchen (etwa Code in einem Projektordner).
- **Orchestrator-Modell kann keine Werkzeuge**: Es lässt sich nicht als Orchestrator wählen.
- **Vault wird gesperrt**: Alle Aufgaben enden sofort; im Verlauf stehen sie als „abgebrochen“.
- **Bericht ist leer oder unbrauchbar**: Der Orchestrator sagt, dass der Ausführer kein Ergebnis
  geliefert hat, und bietet eine Wiederholung an.
- **Geheimnisse**: Kein Auftrag, kein Bericht und keine Rückfrage trägt ein Geheimnis (ADR-0007,
  ADR-0020); das Vorlesen läuft weiter nur über 066 FR-025.
- **Ausführer fragt nach etwas, das der Nutzer vorab schon beantwortet hat**: Der Orchestrator
  antwortet aus dem Auftrag, ohne den Nutzer erneut zu fragen.
- **Mehrdeutige Abbruch-Bitte bei mehreren Aufgaben** („Brich ab“): Der Orchestrator fragt, welche;
  ein Stopp-Wort allein (066 FR-017) bricht die zuletzt gestartete ab.

## Requirements _(mandatory)_

### Functional Requirements

#### Orchestrator

- **FR-001**: Ist die Orchestrierung an, MUST jede Nachricht des Nutzers (getippt oder gesprochen)
  zuerst an den Orchestrator gehen. Ist sie aus, MUST haex wie bisher direkt mit dem Chat-Modell
  arbeiten.
- **FR-002**: Der Orchestrator MUST pro Nachricht genau eins tun: selbst antworten, nachfragen, eine
  Aufgabe übergeben, eine Aufgabe abbrechen oder eine Rückfrage beantworten lassen.
- **FR-003**: Der Orchestrator MUST NOT selbst ändernde oder zerstörende Aktionen ausführen; er darf
  nur lesende Auskünfte über Uhrzeit, Datum, den Zustand von Aufgaben und den Gesprächsverlauf geben.
- **FR-004**: Vor einer Übergabe MUST der Orchestrator nachfragen, wenn Angaben fehlen oder mehrdeutig
  sind, mit höchstens drei Wahlmöglichkeiten.
- **FR-005**: Bei jeder Übergabe MUST der Orchestrator sofort quittieren (getippt: kurze Nachricht;
  gesprochen: Quittung aus 066).
- **FR-006**: Das Orchestrator-Modell MUST wählbar sein, Standard ist das lokale Standardmodell. Nur
  Modelle mit der Fähigkeit „Werkzeuge“ (012) MUST wählbar sein. Bei einem Cloud-Modell MUST haex
  dauerhaft anzeigen, dass Nachrichten an diesen Anbieter gehen, und ohne Netz MUST das lokale Modell
  übernehmen und haex das einmal sagen.

#### Stufen und Übergabe

- **FR-007**: Es MUST vier Stufen geben (lokal, klein, groß, Delegate), jede vom Nutzer mit einem
  Modell bzw. Delegate belegbar oder leer; die Vorbelegung MUST aus den vorhandenen Modellen,
  Schlüsseln und Delegates kommen.
- **FR-008**: Der Orchestrator MUST jede Aufgabe genau einer Stufe zuordnen und die Begründung in
  einem Satz festhalten. Programmierarbeit MUST an die Stufe „Delegate“ gehen, wenn sie belegt ist.
- **FR-009**: Nennt der Nutzer eine Stufe oder ein belegtes Modell, MUST der Orchestrator dieser Wahl
  folgen.
- **FR-010**: Ist die gewählte Stufe nicht verfügbar, MUST der Orchestrator die nächste verfügbare
  passende Stufe nehmen und das sagen; ist keine verfügbar, MUST er das sagen und nichts starten.
- **FR-011**: Ein Auftrag MUST für sich verständlich sein: Ziel, nötiger Kontext, schon geklärte
  Antworten, Stufe mit Begründung. Er MUST NOT den ganzen Gesprächsverlauf enthalten, wenn er für die
  Aufgabe nicht nötig ist, und MUST NOT ein Geheimnis enthalten.
- **FR-012**: Ausführer MUST mit unveränderten Werkzeugen, Freigabestufen, Leitplanken und Grenzen
  arbeiten (003, 032, 007, 009). Delegates bekommen haex-Werkzeuge nur über 021.

#### Rückkanal und Bericht

- **FR-013**: Ein Ausführer MUST während der Arbeit Rückfragen und Freigaben an den Orchestrator
  melden können und bis zur Antwort warten.
- **FR-014**: Freigaben MUST weiterhin allein von der festen Freigabe-Regel und der Antwort des
  Nutzers entschieden werden; der Orchestrator MUST sie nur stellen und die Antwort weitergeben und
  MUST NOT selbst zustimmen. Die Rückfrage MUST neben der Erklärung des Orchestrators den
  ursprünglichen Werkzeugnamen und dessen Eingabe unverändert zeigen.
- **FR-015**: Kennt der Orchestrator die Antwort auf eine Rückfrage schon aus dem Auftrag, MUST er sie
  ohne den Nutzer geben; sonst MUST er den Nutzer fragen. Unbeantwortete Rückfragen MUST offen bleiben,
  ohne dass etwas ausgeführt wird.
- **FR-016**: Am Ende MUST jeder Ausführer einen Bericht liefern (erledigt, teilweise, gescheitert,
  abgebrochen; Ergebnis; offene Punkte). Der Orchestrator MUST ihn dem Nutzer in eigenen Worten
  mitteilen, einschließlich dessen, was nicht geklappt hat, und danach auf neue Anweisungen warten.
- **FR-017**: Bei „gescheitert“ oder „teilweise“ MUST der Orchestrator eine Wiederholung oder
  Fortsetzung anbieten, auf Wunsch mit höherer Stufe.

#### Mehrere Aufgaben

- **FR-018**: Mehrere Aufgaben MUST gleichzeitig laufen können, bis zu einer einstellbaren Grenze
  (Standard 3); weitere MUST eingereiht werden.
- **FR-019**: Jede Meldung, Rückfrage und jeder Bericht MUST der Aufgabe zugeordnet und so benannt
  werden, dass der Nutzer sie erkennt.
- **FR-020**: Der Nutzer MUST jede Aufgabe einzeln abbrechen können; ein Abbruch MUST nur diese
  Aufgabe samt Werkzeugaufrufen beenden. Aktives Beenden eines Sprachgesprächs (066 FR-039) MUST alle
  Aufgaben dieses Gesprächs beenden.
- **FR-021**: Erkennt der Orchestrator, dass eine neue Aufgabe dasselbe Ziel wie eine laufende
  betrifft, MUST er den Konflikt samt kollidierendem Ziel zeigen und fragen, ob die neue Aufgabe
  warten oder die laufende beendet werden soll. Bei Warten MUST die neue Aufgabe bis zum Ende der
  kollidierenden Aufgabe eingereiht bleiben; bei Beenden MUST die laufende Aufgabe vor dem Start der
  neuen abgebrochen werden. Solange der Nutzer nicht entschieden hat, MUST keine der kollidierenden
  Aufgaben neu gestartet werden.

#### Nachvollziehen und Einstellungen

- **FR-022**: Jede Aufgabe MUST mit Auftrag, Stufe, Modell, Begründung, Schritten, Rückfragen und
  Antworten, Bericht, Zeiten und (wo gemeldet) Verbrauch getrennt nach Orchestrator und Ausführer in
  der Vault-Datenbank gespeichert und im Chat als aufklappbare Karte sichtbar sein.
- **FR-023**: Der Nutzer MUST eine Aufgabe mit einer anderen Stufe wiederholen können; die frühere
  bleibt im Verlauf.
- **FR-024**: Orchestrierung an/aus, Orchestrator-Modell, Belegung der Stufen und Grenze gleichzeitiger
  Aufgaben MUST in der Settings-Kategorie „KI“ liegen, bei Auswahl gespeichert werden und ohne
  Neustart wirken. Die Belegung gilt pro Gerät.
- **FR-025**: Die Einstellungen der Orchestrierung MUST für Agenten gesperrt sein (Leitplanken).
- **FR-026**: Aufgaben der Stufe „Delegate“ MUST in einem Projektordner laufen, den der Nutzer nennt
  oder bestätigt (der Orchestrator fragt, wenn er nicht klar ist); haex MUST sich bekannte
  Projektordner pro Gerät merken. Ohne Ordner MUST der Delegate wie bisher in einem leeren
  Arbeitsordner arbeiten.
- **FR-027**: Sind Orchestrator und Stufe „lokal“ verschiedene lokale Modelle, MUST die Einstellung
  darauf hinweisen, dass lokale Aufgaben dann warten, während der Orchestrator spricht.

### Key Entities

- **Orchestrierungs-Einstellungen (pro Gerät)**: an/aus, Orchestrator-Modell, Belegung der vier
  Stufen, Grenze gleichzeitiger Aufgaben.
- **Aufgabe**: gehört zu einer Unterhaltung; Auftrag, Stufe, Modell oder Delegate, Begründung, Zustand
  (eingereiht, läuft, wartet auf Antwort, erledigt, teilweise, gescheitert, abgebrochen), Beginn,
  Ende, Bericht, Verbrauch; Verweis auf eine frühere Aufgabe bei Wiederholung.
- **Rückfrage**: gehört zu einer Aufgabe; Art (Freigabe, Auskunft), Text, Antwort, wer geantwortet hat
  (Nutzer oder Orchestrator aus dem Auftrag), Zeiten.
- **Schritt**: ein Werkzeugaufruf oder eine Zwischenantwort des Ausführers (aus dem vorhandenen
  Verlauf von 003).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Eine Übergabe wird nach Ende der Nachricht mit lokalem Orchestrator innerhalb von 2
  Sekunden quittiert, mit Cloud-Orchestrator innerhalb von 1 Sekunde.
- **SC-002**: Einfache Fragen ohne Werkzeug beantwortet der Orchestrator selbst so schnell wie in 066
  SC-002 (3 s Cloud, 6 s lokal); in einem Testsatz von 20 solchen Fragen startet keine eine Aufgabe.
- **SC-003**: In einem Testsatz von 40 Aufgaben mit menschlichem Stufenlabel ordnet der Orchestrator
  mindestens 85 % der Stufe zu,
  die ein Mensch gewählt hätte; keine Programmieraufgabe landet bei „lokal“ oder „klein“, wenn
  „Delegate“ belegt ist.
- **SC-004**: In keinem Test wird eine Aktion ausgeführt, die nicht die feste Freigabe-Regel und —
  wo sie fragt — der Nutzer erlaubt hat; der Orchestrator gibt in keinem Test selbst frei.
- **SC-005**: Mit drei gleichzeitigen Aufgaben ist jede Meldung, Rückfrage und jeder Bericht in allen
  Tests der richtigen Aufgabe zugeordnet; ein Abbruch beendet nur die genannte Aufgabe innerhalb von
  einer Sekunde.
- **SC-006**: Die Kosten einer typischen Aufgabe der Stufe „klein“ liegen mit Orchestrierung
  höchstens 20 % über denen eines direkten Aufrufs desselben Modells.
- **SC-007**: In keinem Test enthält ein Auftrag, ein Bericht oder eine Rückfrage ein Geheimnis.

## Assumptions

- **Kein gelernter Router**: Die Stufenwahl trifft der Orchestrator. Ein gelernter Router (etwa
  RouteLLM) ist eine spätere Option, falls Messungen (SC-003) zeigen, dass die Wahl des Orchestrators
  nicht reicht. Jev (gehostete API) wurde geprüft und nicht übernommen (research.md R9).
- **Kein eigener Programmier-Agent**: Programmierarbeit geht an Delegates; haex baut die Fähigkeiten
  externer Coding-Agenten nicht nach.
- **Ausführer sind keine neuen Agenten**: Der eingebaute Agent (003/032) mit wechselndem Modell und die
  Delegates (007/009) sind die Ausführer.
- **Eine Unterhaltung pro Gespräch**: Aufgaben gehören zu der Unterhaltung, in der sie gegeben
  wurden; Unteraufgaben (ein Ausführer startet weitere Ausführer) sind nicht Teil dieser Spec.
- **Externe Agenten über MCP (021)** werden nicht orchestriert; sie steuern haex selbst.
- **Phasenfolge**: Umsetzung nach 032 und 012 (im Einsatz) und bevor 066 PR E auf Aufgaben im
  Hintergrund aufbaut, damit 066 die Aufgaben aus dieser Spec nutzt.
