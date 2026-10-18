# Feature Specification: Werkzeuge von Erweiterungen für den Agenten

**Feature Branch**: `018-extension-agent-tools`

**Created**: 2026-10-10

**Status**: Draft

**Input**: User description: "Spec 018: Werkzeuge von Erweiterungen für den Agenten über MCP (ADR-0004, Richtung A). Eine installierte haextension deklariert in ihrem signierten Manifest Werkzeuge und stellt sie zur Laufzeit über MCP bereit; holzi verbindet sich als MCP-Client und bietet sie dem eingebauten Agenten an (Quelle `haextension`, standardmäßig Risky, Freigabe je Aufruf, Bestätigung bei der Installation zusammen mit den übrigen Berechtigungen, minimiertes Fenster wenn keins offen). Erkenntnis aus Spec 046: kleine lokale Modelle (Qwen3-4B) finden Werkzeuge außerhalb des festen Erstangebots über find_actions nicht verlässlich; bei 30+ Erweiterungen braucht es einen festen Weg, z. B. dass holzi die Werkzeuge einer vom Nutzer genannten Erweiterung direkt anbietet. Beispiel-Anwendungsfall: „Wie viele ungelesene Mails habe ich?“ beantwortet der Agent über ein Werkzeug von haex-mail statt run_command zu raten. Das Ergänzen der Werkzeuge in den bestehenden haextensions und ein MCP-Baustein im vault-sdk sind Spec 019, nicht Teil dieser Spec; für 018 genügt eine Test-Erweiterung als Gegenstück."

## Beziehung zu bestehenden Specs

- [ADR-0004](../../docs/adr/0004-extension-protocol-split.md): Diese Spec ist die dort geplante
  Richtung A (Agent → Erweiterung). Werkzeuge stehen im signierten Manifest, der Nutzer bestätigt sie
  bei der Installation, die Erweiterung stellt sie zur Laufzeit über MCP bereit, holzi ist der
  Client. Über diese Verbindung erreicht keine Erweiterung das Modell, den Chat oder eine andere
  Erweiterung. Der Operator will Erweiterungen künftig den Agenten nutzen lassen, mit einem Modell, das der
  Nutzer je Anfrage festlegt (Clarification 2026-10-10); das ist nicht Teil dieser Spec.
- [`017-extension-host`](../017-extension-host/spec.md): Installation, Signatur, Update, Deaktivieren,
  Entfernen und das Berechtigungsmodell gelten unverändert; diese Spec ergänzt das Manifest um
  Werkzeuge und den Bestätigungsdialog um sie. Eine Host-Funktion, die eine Erweiterung während eines
  Werkzeugaufrufs nutzt, prüft holzi gegen die Berechtigungen der Erweiterung (017 FR-015 ff.). Die
  Meldung „Aktion angefordert“ des vault-sdk (017, Annahmen) bleibt unbeantwortet; diese Spec braucht
  sie nicht.
- [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md): Freigabe-Modi (FR-007 bis
  FR-009), Sichtbarkeit im Verlauf (FR-005), Fehler ohne interne Details (FR-006) und die
  Unveränderbarkeit von Freigabe-Modus und Berechtigungen (FR-010) gelten für Werkzeuge von
  Erweiterungen genauso. **Änderung an FR-011**: Über das feste Kernangebot hinaus darf holzi im ersten
  Schritt Werkzeuge von Erweiterungen anbieten, die zur Nachricht des Nutzers passen (FR-006 dieser
  Spec). Das Kernangebot selbst bleibt textunabhängig.
- [`046-agent-choice-prompt`](../046-agent-choice-prompt/spec.md): Die Messung dort (research R15,
  R16, R18) zeigt, dass Qwen3-4B Werkzeuge, die es erst suchen muss, nicht verlässlich nutzt und ohne
  passendes Werkzeug zu `run_command` greift. Deshalb verlangt diese Spec einen festen Weg ohne Suche
  durch das Modell.
- [`047-structured-agent-tasks`](../047-structured-agent-tasks/spec.md): die Gegenrichtung (eine
  Erweiterung beauftragt eine KI-Aufgabe). Beide Specs teilen keinen Weg; aus 018 entsteht kein Kanal
  von einer Erweiterung zum Modell.
- Geplante Spec **019**: ergänzt Werkzeuge in den bestehenden haextensions (z. B. haex-mail). Geplante
  Spec **021** (MCP-Server, Freigaben je externem Agent): Ob externe Agenten Werkzeuge von
  Erweiterungen erreichen, regelt 021, nicht diese Spec.

## Begriffe

- **Erweiterungswerkzeug**: eine Fähigkeit, die eine Erweiterung dem eingebauten Agenten anbietet, z. B.
  „ungelesene Mails zählen“. Es hat einen Namen, eine Beschreibung, eine Eingabe, eine Wirkungsart und
  Beispielsätze.
- **Wirkungsart**: lesend, ändernd oder zerstörend, wie bei den Aktionen aus 032 FR-007.
- **Beispielsätze**: kurze Sätze in Deutsch und Englisch, mit denen ein Nutzer nach dem Werkzeug fragen
  würde („Wie viele ungelesene Mails habe ich?“). holzi nutzt sie, um ein Werkzeug zur Nachricht
  passend anzubieten.
- **Passendes Angebot**: die Werkzeuge von Erweiterungen, die holzi im ersten Schritt einer Antwort
  zusätzlich zum Kernangebot anbietet, weil sie zur Nachricht passen.
- **Test-Erweiterung**: eine signierte Erweiterung im Repository, die nur dazu dient, diese Spec ohne
  echte haextension zu prüfen.

## Clarifications

### Session 2026-10-10

- Q: Gilt die im Manifest erklärte Wirkungsart, oder ist jedes Erweiterungswerkzeug „zerstörend“? → A:
  Die erklärte und bestätigte Wirkungsart gilt. Eine Erweiterung kann ohnehin nur tun, wofür sie
  Berechtigungen hat; ein als lesend erklärtes Werkzeug ist damit nicht gefährlicher als die
  Erweiterung selbst. ADR-0004 wird entsprechend angepasst.
- Q: Gehört der MCP-Baustein im vault-sdk in diese Spec oder in Spec 019? → A: In diese Spec, wie
  ADR-0004 die Aufteilung festhält. Die Test-Erweiterung nutzt ihn bereits; Spec 019 trägt nur noch
  Werkzeuge in die bestehenden haextensions ein.
- Q: Muss ein Update bei geänderter Eingabe eines Werkzeugs erneut bestätigt werden? → A: Nein, nur bei
  neuen Werkzeugen oder geänderter Wirkungsart (FR-002, research R4).
- Q: Lehnt holzi Anfragen einer Erweiterung an das Modell grundsätzlich ab? → A: Nein. Erweiterungen sollen
  den Agenten künftig nutzen können; der Nutzer legt fest, welche Anfragen einer Erweiterung mit welchem
  Modell oder Agenten beantwortet werden. Das ist spätere Arbeit (Spec 047 und folgende). In dieser Spec
  lehnt die MCP-Verbindung Anfragen der Erweiterung an holzi ab, weil es dafür noch keine Festlegung des
  Nutzers gibt.
- Q: Prüft holzi die Eingabe eines Werkzeugs selbst, bevor es die Erweiterung aufruft? → A: Nein, der
  Baustein im vault-sdk prüft sie vor dem Handler (FR-021); für Nutzer und Modell ist das Ergebnis
  gleich, und die Prüfung gibt es nur einmal.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Der Agent beantwortet eine Frage mit dem Werkzeug einer Erweiterung (Priority: P1)

Der Nutzer hat eine Erweiterung installiert, die ein Werkzeug anbietet. Er fragt im Chat in eigenen
Worten danach, ohne die Erweiterung zu nennen. holzi bietet dem Modell das passende Werkzeug gleich im
ersten Schritt an, das Modell ruft es auf, und der Agent antwortet mit dem Ergebnis.

**Why this priority**: Das ist der Kern der Spec. Ohne ihn bleibt jede Frage nach Daten einer
Erweiterung unbeantwortet oder endet in einem geratenen Shell-Befehl (046 R18).

**Independent Test**: Test-Erweiterung mit einem lesenden Werkzeug „Einträge zählen“ installieren; im
Chat „Wie viele Einträge habe ich?“ fragen; der Agent ruft das Werkzeug auf und nennt die Zahl, die die
Test-Erweiterung liefert.

**Acceptance Scenarios**:

1. **Given** eine installierte Erweiterung mit einem lesenden Werkzeug und Beispielsätzen, **When** der
   Nutzer eine Frage stellt, die einem Beispielsatz ähnelt, **Then** ist das Werkzeug im ersten Schritt
   angeboten, ohne dass das Modell suchen muss.
2. **Given** das Modell ruft das Werkzeug auf, **When** die Erweiterung ein Ergebnis liefert, **Then**
   antwortet der Agent damit, und der Verlauf zeigt den Aufruf mit dem Namen der Erweiterung, dem
   Werkzeug in Klartext, der Eingabe und dem Ergebnis.
3. **Given** kein Fenster der Erweiterung ist offen, **When** das Modell ihr Werkzeug aufruft, **Then**
   startet holzi die Erweiterung für den Aufruf, ohne dass sich ein Fenster in den Vordergrund schiebt,
   und der Aufruf gelingt.
4. **Given** 30 installierte Erweiterungen mit je mehreren Werkzeugen, **When** der Nutzer nach etwas
   fragt, das genau ein Werkzeug abdeckt, **Then** ist dieses Werkzeug im ersten Schritt angeboten, und
   das Angebot bleibt innerhalb der Grenze für die Anzahl der Werkzeuge.
5. **Given** eine Frage, zu der keine Erweiterung ein Werkzeug hat, **When** der Agent antwortet,
   **Then** bietet holzi keine Werkzeuge von Erweiterungen zusätzlich an.

---

### User Story 2 - Werkzeuge bei der Installation sehen und bestätigen (Priority: P1)

Bei der Installation zeigt holzi neben den Berechtigungen auch die Werkzeuge, die die Erweiterung dem
Agenten anbieten will, jeweils mit Name, Beschreibung und Wirkungsart. Der Nutzer bestätigt alles in
einem Dialog. Bei einem Update mit neuen oder geänderten Werkzeugen fragt holzi nur nach diesen.

**Why this priority**: Ohne Bestätigung dürfte holzi keinem Werkzeug vertrauen; ADR-0004 verlangt
dieselbe Bestätigung wie für Berechtigungen.

**Independent Test**: Test-Erweiterung installieren: Der Dialog listet ihre Werkzeuge. Eine Version mit
einem zusätzlichen Werkzeug installieren: Der Dialog fragt nur nach dem neuen.

**Acceptance Scenarios**:

1. **Given** ein Bundle mit Werkzeugen im Manifest, **When** der Nutzer installiert, **Then** zeigt der
   Bestätigungsdialog jedes Werkzeug mit Name, Beschreibung und Wirkungsart in Klartext.
2. **Given** ein Update fügt ein Werkzeug hinzu oder ändert die Wirkungsart eines bestehenden, **When**
   der Nutzer es einspielt, **Then** fragt holzi nur nach diesen Änderungen; bis zur Bestätigung bietet
   holzi das betroffene Werkzeug nicht an.
3. **Given** eine Erweiterung bietet zur Laufzeit ein Werkzeug an, das nicht im Manifest steht, **When**
   holzi ihre Werkzeuge abfragt, **Then** bietet holzi dieses Werkzeug nicht an.

---

### User Story 3 - Freigabe je Aufruf und Kontrolle in den Einstellungen (Priority: P1)

Ein Aufruf eines Erweiterungswerkzeugs folgt dem gewählten Freigabe-Modus wie jede Aktion. Der Nutzer
sieht in der Abfrage, welche Erweiterung was tun will. In den Einstellungen kann er die Werkzeuge einer
Erweiterung für den Agenten abschalten, ohne die Erweiterung zu deaktivieren.

**Why this priority**: Werkzeuge führen fremden Code aus; der Nutzer muss jeden Aufruf kontrollieren
und den Zugang des Agenten entziehen können.

**Independent Test**: Im Modus „Manuell“ ein Werkzeug der Test-Erweiterung auslösen: Abfrage mit
Erweiterung, Werkzeug und Eingabe; „Ablehnen“ führt nichts aus. In den Einstellungen die Werkzeuge der
Test-Erweiterung abschalten: Der Agent bekommt sie nicht mehr angeboten.

**Acceptance Scenarios**:

1. **Given** Modus „Manuell“, **When** das Modell ein Erweiterungswerkzeug aufruft, **Then** erscheint
   die Zustimmungsabfrage mit dem Namen der Erweiterung, dem Werkzeug in Klartext und der Eingabe.
2. **Given** der Nutzer lehnt ab, **When** der Turn weiterläuft, **Then** hat die Erweiterung nichts
   ausgeführt, und das Modell erhält die Ablehnung (032 FR-009).
3. **Given** der Nutzer schaltet die Werkzeuge einer Erweiterung in den Einstellungen ab, **When** er
   danach fragt, **Then** bietet holzi keines ihrer Werkzeuge mehr an; die Erweiterung selbst läuft
   weiter als App.
4. **Given** ein Werkzeug nutzt während des Aufrufs eine Host-Funktion, für die die Erweiterung keine
   Berechtigung hat, **When** holzi nachfragt, **Then** sagt die Abfrage, dass ein Aufruf des Agenten
   sie ausgelöst hat.

---

### User Story 4 - Erweiterungen mit Werkzeugen entwickeln und prüfen (Priority: P2)

Wer eine Erweiterung entwickelt, kann ihre Werkzeuge im Entwicklermodus (017 US12) mit dem Agenten
ausprobieren und mit der Modellprüfung messen, ob ein lokales Modell sie trifft.

**Why this priority**: Spec 019 und spätere Erweiterungen brauchen einen Weg, ihre Werkzeuge vor der
Veröffentlichung zu prüfen; ohne ihn bleibt die Qualität der Beispielsätze dem Zufall überlassen.

**Independent Test**: Test-Erweiterung im Entwicklermodus laden, Werkzeug im Chat aufrufen lassen; die
Modellprüfung mit den Beispielsätzen der Test-Erweiterung laufen lassen und die Quote ablesen.

**Acceptance Scenarios**:

1. **Given** eine Entwicklungsversion mit Werkzeugen, **When** der Nutzer im Chat danach fragt,
   **Then** bietet holzi ihre Werkzeuge an, und der Verlauf kennzeichnet sie als Entwicklungsversion.
2. **Given** die Modellprüfung, **When** sie mit installierten Erweiterungswerkzeugen läuft, **Then**
   enthält ihr Bericht je Werkzeug, ob es angeboten und aufgerufen wurde.

---

### Edge Cases

- Die Erweiterung ist deaktiviert, entfernt oder auf diesem Gerät nicht bereit („Wird übertragen“,
  Signaturfehler): holzi bietet ihre Werkzeuge nicht an. Ein noch laufender Aufruf endet mit einem
  verständlichen Fehler an das Modell.
- Die Erweiterung antwortet nicht oder zu langsam: Der Aufruf endet nach einer festen Zeit mit einem
  Fehler; der Turn läuft weiter.
- Der Nutzer stoppt den Turn während eines Aufrufs: holzi bricht den Aufruf ab, und die Erweiterung
  erhält den Abbruch.
- Das Ergebnis ist sehr groß oder enthält Text, der wie eine Anweisung an das Modell aussieht: holzi
  kürzt es auf eine feste Größe und kennzeichnet es dem Modell gegenüber als Daten einer Erweiterung.
  Ein Ergebnis kann Freigabe-Modus, Sperrregeln oder Berechtigungen nicht ändern (032 FR-010).
- Zwei Erweiterungen bieten Werkzeuge mit demselben Namen an: Beide bleiben unterscheidbar, für Modell
  und Nutzer.
- Ein Werkzeug passt zur Nachricht, aber die Grenze für die Anzahl der Werkzeuge ist erreicht: holzi
  bietet die am besten passenden an; die übrigen bleiben über die Suche erreichbar.
- Die Eingabe des Modells passt nicht zum Schema des Werkzeugs: Der Baustein im vault-sdk prüft sie, bevor
  er die Funktion der Erweiterung aufruft; das Modell erhält den Fehler mit dem betroffenen Feld, und der
  Code der Erweiterung läuft nicht (FR-021).
- Das Modell hat „Werkzeugnutzung: nicht unterstützt“ oder ein Delegate (Claude Code, Codex) ist
  gewählt: holzi bietet keine Erweiterungswerkzeuge an (032 FR-016).
- Die Erweiterung startet auf einem Gerät ohne die Host-Funktion, die das Werkzeug braucht (z. B. ohne
  Shell auf dem Telefon): Das Werkzeug liefert einen verständlichen Fehler; holzi stürzt nicht ab.
- Dieselbe Erweiterung ist auf einem anderen Gerät installiert und dort nicht bereit: Auf diesem Gerät
  ändert das nichts.

## Requirements _(mandatory)_

### Functional Requirements

**Manifest und Bestätigung**

- **FR-001**: Das Manifest einer Erweiterung MUSS Werkzeuge erklären können, je Werkzeug mit Name,
  Titel in Deutsch und Englisch, Beschreibung, Eingabeschema, Wirkungsart und Beispielsätzen in Deutsch
  und Englisch. Das Manifest ist signiert; ein geändertes Werkzeug ist eine geänderte Signatur (017
  FR-002).
- **FR-002**: Der Bestätigungsdialog bei Installation und Update MUSS die Werkzeuge mit Titel,
  Beschreibung und Wirkungsart zeigen, zusammen mit den Berechtigungen in einem Dialog (017 FR-006).
  Ein Update MUSS nur neue Werkzeuge und Werkzeuge mit geänderter Wirkungsart zur Bestätigung
  vorlegen; eine geänderte Eingabe ändert nicht, was das Werkzeug tun darf.
- **FR-003**: holzi DARF ein Werkzeug nur anbieten, wenn es im Manifest der laufenden Version steht und
  bestätigt ist. Ein Werkzeug, das die Erweiterung zur Laufzeit zusätzlich meldet, MUSS ignoriert
  werden; ein erklärtes, das sie zur Laufzeit nicht meldet, MUSS als nicht verfügbar gelten.
- **FR-004**: Es MUSS die Wirkungsart gelten, die das signierte Manifest erklärt und der Nutzer bei
  der Installation bestätigt hat. Ein lesendes Werkzeug läuft damit in den Modi „Auto“ und „Plan“ ohne
  Abfrage, wie eine lesende Aktion von holzi. Angaben, die die Erweiterung zur Laufzeit macht, DÜRFEN
  die Wirkungsart nie ändern. Das ändert ADR-0004 („Tools are `Risky` by default“).

**Angebot an das Modell**

- **FR-005**: holzi MUSS die Werkzeuge jeder installierten, aktivierten und auf diesem Gerät bereiten
  Erweiterung dem eingebauten Agenten anbieten können, sofern der Nutzer sie nicht abgeschaltet hat
  (FR-016).
- **FR-006**: Vor dem ersten Schritt einer Antwort MUSS holzi die Nachricht des Nutzers mit Titel,
  Beschreibung und Beispielsätzen aller anbietbaren Erweiterungswerkzeuge vergleichen und die am besten
  passenden zusätzlich zum Kernangebot anbieten (passendes Angebot), ohne dass das Modell suchen muss.
  Ein Werkzeug passt auch, wenn die Nachricht die Erweiterung beim Namen nennt.
- **FR-007**: Das passende Angebot MUSS leer bleiben, wenn kein Werkzeug ausreichend passt, und MUSS die
  Gesamtgrenze für Werkzeuge in einem Schritt einhalten.
- **FR-008**: Jedes Erweiterungswerkzeug MUSS zusätzlich über die Aktionssuche des Agenten (032 FR-012)
  auffindbar sein, auch wenn es nicht im passenden Angebot war.
- **FR-009**: Für Modell und Nutzer MUSS jedes Werkzeug eindeutig seiner Erweiterung zugeordnet sein,
  auch wenn zwei Erweiterungen denselben Werkzeugnamen verwenden.
- **FR-010**: Kommt eine Erweiterung hinzu, wird entfernt, deaktiviert oder wieder bereit, MUSS das
  Angebot ab der nächsten Antwort stimmen, ohne Neustart von holzi.

**Aufruf**

- **FR-011**: Ein Aufruf MUSS dem Freigabe-Modus nach der Wirkungsart des Werkzeugs folgen (032 FR-007
  bis FR-009). Die Zustimmungsabfrage MUSS die Erweiterung, den Titel des Werkzeugs und die Eingabe in
  Klartext zeigen.
- **FR-012**: Ist kein Fenster der Erweiterung offen, MUSS holzi sie für den Aufruf starten, ohne ein
  Fenster in den Vordergrund zu holen oder den Fokus zu ändern. Ist eines offen, MUSS holzi dieses
  nutzen.
- **FR-013**: Ein Aufruf MUSS nach einer festen Zeit abbrechen, beim Stoppen des Turns abbrechen, und
  sein Ergebnis MUSS auf eine feste Größe begrenzt sein. Das Ergebnis MUSS im Verlauf mit Erweiterung,
  Werkzeug, Eingabe und Ergebnis stehen (032 FR-005) und dem Modell als Daten der Erweiterung
  gekennzeichnet werden.
- **FR-014**: Eine Host-Funktion, die die Erweiterung während eines Aufrufs nutzt, MUSS gegen die
  Berechtigungen der Erweiterung geprüft werden; eine daraus folgende Abfrage MUSS sagen, dass ein
  Aufruf des Agenten sie ausgelöst hat.
- **FR-015**: Über die Verbindung dieser Spec DARF keine Erweiterung das Modell, den Chat, die
  Freigabe-Einstellungen oder eine andere Erweiterung erreichen; Anfragen der Erweiterung an holzi auf
  dieser Verbindung MÜSSEN abgelehnt werden. Ein Vertragstest MUSS das prüfen. Dass eine Erweiterung den
  Agenten mit einem vom Nutzer gewählten Modell nutzt, ist eine eigene Richtung (Spec 047 und später), kein
  Teil dieser Spec.

**Kontrolle**

- **FR-016**: Die Einstellungs-App MUSS in der Kategorie „Erweiterungen“ je Erweiterung ihre Werkzeuge
  mit Titel und Wirkungsart zeigen und einen Schalter „Für den Agenten verfügbar“ bieten. Der Schalter
  gilt für die Vault und ist für das Modell nicht änderbar (032 FR-010).
- **FR-017**: Delegates und Modelle mit „Werkzeugnutzung: nicht unterstützt“ DÜRFEN keine
  Erweiterungswerkzeuge angeboten bekommen.

**Prüfen**

- **FR-018**: Das Repository MUSS eine signierte Test-Erweiterung mit mindestens einem lesenden und
  einem ändernden Werkzeug enthalten, die den Baustein aus FR-021 nutzt und mit der sich FR-001 bis
  FR-017 automatisch prüfen lassen.
- **FR-019**: Die Modellprüfung (032 FR-019 ff.) MUSS Sätze für Erweiterungswerkzeuge enthalten und je
  Satz bewerten, ob das passende Werkzeug angeboten und aufgerufen wurde. Sie MUSS mit vielen
  installierten Erweiterungen laufen können (mindestens 30).
- **FR-020**: Das Laden einer Entwicklungsversion (017 US12) MUSS ihre Werkzeuge anbieten; der Verlauf
  MUSS solche Aufrufe als Entwicklungsversion kennzeichnen.

**vault-sdk**

- **FR-021**: Das vault-sdk MUSS einen MCP-Server-Baustein bereitstellen: Eine Erweiterung beschreibt
  ihre Werkzeuge und gibt je Werkzeug eine Funktion an, der Baustein stellt sie holzi über MCP bereit,
  ohne dass die Erweiterung das Protokoll selbst umsetzt. Der Baustein MUSS
  Eingaben gegen das erklärte Schema prüfen, bevor er die Funktion aufruft, und einen Abbruch durch
  holzi an die Funktion weitergeben. Erweiterungen ohne Werkzeuge MÜSSEN mit dem neuen vault-sdk
  unverändert laufen, in holzi wie in haex-vault 2.x (Legacy).

### Key Entities

- **Erweiterungswerkzeug**: gehört zu genau einer Erweiterung; Name (eindeutig innerhalb der
  Erweiterung), Titel de/en, Beschreibung, Eingabeschema, Wirkungsart, Beispielsätze de/en; Herkunft
  ist das signierte Manifest.
- **Werkzeugbestätigung**: je Vault und Erweiterung, welche Werkzeuge in welcher Fassung bestätigt sind;
  wird mit der Installation bzw. dem Update angelegt.
- **Agentenzugang**: je Vault und Erweiterung der Schalter „Für den Agenten verfügbar“.
- **Werkzeugaufruf**: ein Eintrag im Chat-Verlauf mit Erweiterung, Werkzeug, Eingabe, Ergebnis oder
  Fehler und Kennzeichnung als Entwicklungsversion.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Mit 30 installierten Erweiterungen ist das passende Werkzeug bei mindestens 9 von 10
  Sätzen der Modellprüfung für Erweiterungswerkzeuge im ersten Schritt angeboten.
- **SC-002**: Mit dem empfohlenen lokalen Modell ruft der Agent bei mindestens 7 von 10 dieser Sätze
  das passende Werkzeug auf.
- **SC-003**: Bei keinem dieser Sätze versucht der Agent, stattdessen einen Befehl auf dem Gerät
  auszuführen.
- **SC-004**: Ein lesender Aufruf der Test-Erweiterung, deren Fenster nicht offen ist, liefert dem
  Nutzer das Ergebnis höchstens 3 Sekunden später als bei offenem Fenster.
- **SC-005**: Kein Werkzeug, das nicht im bestätigten Manifest steht, wird je angeboten oder ausgeführt
  (automatisch geprüft mit der Test-Erweiterung).
- **SC-006**: Das Abschalten in den Einstellungen wirkt ab der nächsten Antwort, ohne Neustart.

## Assumptions

- Die Werkzeuge kommen vom eingebauten Agenten. Externe Agenten über holzis MCP-Server regelt Spec 021.
- Eine Erweiterung stellt ihre Werkzeuge über MCP bereit, wie ADR-0004 festlegt. Den Baustein dafür
  bringt das vault-sdk mit (FR-021); die Änderungen an vault-sdk und am Manifest-Schema werden über
  volle Commit-SHAs referenziert (ADR-0004, Konsequenzen).
- Die Beispielsätze im Manifest stammen von der Erweiterung. Ihre Qualität bestimmt, wie gut das
  passende Angebot trifft; FR-019 macht sie messbar.
- Die Gesamtgrenze für Werkzeuge in einem Schritt legt der Plan fest; sie wird mit der Websuche ohnehin
  wachsen (046 R16). Das passende Angebot bekommt davon einen festen Anteil.
- Erweiterungen laufen auch auf Android (043); Werkzeuge gelten dort genauso, sofern der eingebaute
  Agent dort ein Modell mit Werkzeugnutzung hat.
- Das Abschalten des Agentenzugangs wird wie andere Einstellungen der Vault zwischen eigenen Geräten
  synchronisiert.

## Nicht im Umfang

- Werkzeuge in den bestehenden haextensions (haex-mail, haex-notes, haex-files): Spec 019.
  Der vault-sdk-Baustein (FR-021) gehört dagegen in diese Spec.
- Werkzeuge von Erweiterungen für externe Agenten: Spec 021.
- Ein Weg von einer Erweiterung zum Agenten oder Modell: Spec 047 und spätere Specs, mit einem vom Nutzer
  festgelegten Modell je Anfrage.
- Werkzeuge, die eine Erweiterung erst zur Laufzeit erfindet, ohne sie im Manifest zu erklären.
- Ein Marktplatz für Erweiterungen mit Werkzeugen.
