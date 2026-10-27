# Feature Specification: holzi als MCP-Server für externe Agenten

**Feature Branch**: `021-mcp-server`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "021: holzi als MCP-Server für externe Agenten (Claude Code, Codex,
künftige MCP-fähige Agenten, Delegates aus 007/009). Externe Agenten bedienen holzi über MCP:
Aktionen aus der Action-Registry (020/032), haextension-Werkzeuge (018/019/055). Nur nach
Registrierung und Freigabe durch den Nutzer; bei der Registrierung legt der Nutzer pro Agent fest,
was er darf (welche haextensions, Passwörter lesen, Shell, Dateien usw.). Kein Agent darf alles.
Guardrail-Einstellungen (Autonomie-Modus, Deny-Regeln, Provider-Zugangsdaten) sind für externe
Agenten gesperrt. haextensions können holzis Historie nie ändern. Wiederverwendung der
Freigabe-Brücke aus 003/007/009/032 (Auto/Plan/Rückfrage bei destruktiven Aktionen). Grundlage für
eine spätere Browser-Werkzeug-Spec."

## Begriffe

- **Externer Agent**: ein Programm auf demselben Gerät, das holzi über MCP bedient, etwa Claude
  Code, Codex oder ein anderer MCP-fähiger Client. Er bringt sein eigenes Modell mit; holzi führt
  für ihn keinen Werkzeug-Ablauf, sondern beantwortet nur seine Aufrufe.
- **MCP-Zugang**: der Dienst in holzi, über den externe Agenten sich verbinden, die Liste ihrer
  Werkzeuge abfragen und Werkzeuge aufrufen. Er gilt je Gerät und ist an die offene Vault gebunden.
- **Registrierung**: die Entscheidung des Nutzers in holzi, dass ein bestimmter externer Agent sich
  verbinden darf, zusammen mit seinen **Agenten-Rechten** und seiner **Freigabestufe**. Erst danach
  bekommt der Agent Werkzeuge.
- **Zugangsnachweis**: das Geheimnis, das ein Agent bei der Registrierung erhält und mit dem er sich
  bei jeder Verbindung ausweist. holzi zeigt ihn nach der Ausgabe nie wieder an.
- **Angegebener Name**: der Name, den ein Agent von sich selbst nennt („Claude Code“). Er ist nicht
  geprüft und wird in holzi immer als Angabe des Agenten gekennzeichnet.
- **Agenten-Rechte**: was ein registrierter Agent darf, in getrennten **Rechtegruppen**:
  - **holzi bedienen**: je Berechtigungsbereich der Aktionen (Spec 020, FR-031) „keine“, „lesen“
    oder „lesen und ändern“. Der Bereich Leitplanken ist nicht wählbar.
  - **haextensions**: je installierter Erweiterung an oder aus.
  - **Dateien**: die Agent-Berechtigungen aus Spec 044 (Dateien des Geräts, je Speicher, je
    verschlüsseltem Ordner, Kontakte, Dateifreigaben).
  - **Passwörter**: eine Freigabe im Sinn von Spec 034 (Art „Lesen“ oder „Lesen und Schreiben“,
    Bereich „alle Einträge“ oder Einträge mit bestimmten Tags).
  - **Befehle ausführen**: Befehle über holzis Befehlswerkzeug (Spec 003) ausführen, an oder aus.
    Es ist der Weg für Fähigkeiten, die es nur in holzi gibt und die der Agent mit seiner eigenen
    Shell nicht erreicht.
- **Freigabestufe**: Manuell, Auto oder Plan wie in Spec 003 und ADR-0006 (lesend, ändernd,
  zerstörend); hier **je Agent** eingestellt.
- **Rückfrage**: der Dialog in holzi, in dem der Nutzer einen einzelnen Aufruf eines Agenten erlaubt
  oder ablehnt.
- **Agenten-Protokoll**: die Liste der Aufrufe eines Agenten auf diesem Gerät.
- **Delegate**: Claude Code oder Codex, von holzi im Chat gestartet (Specs 007/009). Ein Delegate
  ist für diese Spec ein externer Agent, den holzi selbst startet.
- **Aktion**, **Berechtigungsbereich**, **Wirkungsart**, **Aufrufer**: wie in Spec 020.
  **Werkzeug einer Erweiterung**: wie in Spec 018.

## Beziehung zu bestehenden Specs

- [`020-tab-navigation`](../020-tab-navigation/spec.md) liefert den Aktionskatalog mit
  Berechtigungsbereichen, Wirkungsart, „durch Agenten aufrufbar“, dem Aufrufer „externer Agent (mit
  Kennung)“ (FR-029) und der Sperre der Leitplanken (FR-032). Diese Spec öffnet den Katalog für
  registrierte externe Agenten und baut die Rechte je Agent auf den Bereichen auf. Die Zusage aus
  020, dass eingebettete Inhalte holzis Historie nie ändern (FR-035), gilt unverändert; ein Agent
  erreicht Erweiterungen nur über deren erklärte Werkzeuge, nie ihren Rahmen direkt.
- [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md) und
  [ADR-0006](../../docs/adr/0006-actions-as-builtin-agent-tools.md): Der eingebaute Agent ruft die
  Aktionen im Prozess auf; diese Spec veröffentlicht **dieselben Definitionen** über MCP. Aktionen
  mit „nur nicht für den eingebauten Agenten“ (`chat.message.send` u. a.) sind für externe Agenten
  aufrufbar. Der Hinweis im Chat, dass ein Delegate keine holzi-Werkzeuge hat (032 US4, FR-023),
  entfällt für registrierte Delegates (US6).
- [`003-agent-tool-loop`](../003-agent-tool-loop/spec.md),
  [`007-cli-delegate`](../007-cli-delegate/spec.md),
  [`009-autonomous-delegate-mode`](../009-autonomous-delegate-mode/spec.md): Die Freigabe mit drei
  Stufen und die Rückfrage-Brücke der Delegates werden wiederverwendet. Der Autonomie-Modus eines
  Delegates (009) gilt nur für dessen eigene Werkzeuge, nie für holzi-Werkzeuge (FR-024).
- [`018-extension-agent-tools`](../018-extension-agent-tools/spec.md) und
  [`055-extension-model-access`](../055-extension-model-access/spec.md): Werkzeuge von
  Erweiterungen gibt es bisher nur für den eingebauten Agenten; 018 überlässt externe Agenten
  ausdrücklich dieser Spec.
- [`034-password-manager`](../034-password-manager/spec.md): legt fest, was eine Freigabe für
  Passwörter bedeutet und wie sie geprüft wird (FR-028–FR-030), und überlässt das Vergeben und
  Anzeigen für externe Agenten dieser Spec. Der eingebaute Agent bekommt weiterhin nie Geheimnisse.
- [`044-file-browser`](../044-file-browser/spec.md),
  [`051-phone-contacts`](../051-phone-contacts/spec.md),
  [`056-file-access-sharing`](../056-file-access-sharing/spec.md): Die Agent-Berechtigungen für
  Dateien, Speicher, Kontakte und Dateifreigaben gelten für externe Agenten unverändert; externe
  Agenten haben ab Werk keine davon (044).
- [`046-agent-choice-prompt`](../046-agent-choice-prompt/spec.md): Externe Agenten bekommen
  „Auswahl nötig“ mit den Kandidaten als Ergebnis und entscheiden selbst.
- [`049-external-bridge`](../049-external-bridge/spec.md) ist das Vorbild für Zulassung,
  geräteeigene Speicherung, Sperre, Widerruf und die Abwehr von Webseiten auf einem lokalen Port.
- [ADR-0003](../../docs/adr/0003-one-vault-session-per-app-process.md): Ein Prozess hat genau eine
  Vault; der MCP-Zugang lebt mit ihr.
- **Folge-Spec Browser-Werkzeuge**: Agenten sollen später einen Browser über holzi steuern. Sie
  kommt als neue Rechtegruppe hinzu (FR-008); diese Spec legt dafür nur die Grundlage.

## Clarifications

### Session 2026-10-26

- Q: Darf ein unbekannter Agent die Registrierung selbst auslösen? → A: Ja. Verbindet sich ein
  unbekannter Agent, zeigt holzi den Registrierungsdialog wie bei der Browser-Erweiterung (049);
  begrenzt auf einen Dialog je Agent und drei gleichzeitig (FR-006, FR-011).
- Q: Darf ein externer Agent mit passender Passwort-Freigabe Geheimnisse im Klartext bekommen? →
  A: Ja, aber nur mit einer Rückfrage bei jedem Abruf, auch in Auto (FR-019).
- Q: Was passiert, wenn ein Agent holzi erreichen will, holzi aber nicht läuft oder keine Vault
  offen ist? → A: Der Agent bekommt nur die Meldung, dass holzi nicht offen ist; nichts startet
  holzi oder fordert zum Entsperren auf (FR-002).
- Q: Wie genau hält das Agenten-Protokoll fest, was ein Agent getan hat? → A: Mit Ziel und
  Eingaben, ohne Geheimnisse und Dateiinhalte, damit sichtbar ist, was geändert wurde (FR-028).
- Q: Läuft der Zugangsnachweis eines Agenten von selbst ab? → A: Ja, nach 90 Tagen ohne
  Verbindung; danach erscheint der Registrierungsdialog mit den bisherigen Rechten vorausgefüllt
  (FR-007).
- Q: Gibt es für externe Agenten eine eigene Rechtegruppe „Befehle ausführen“? → A: Ja, ab Werk
  aus. holzi wird Fähigkeiten bekommen, die es nur dort gibt und nicht über die eigene Shell des
  Agenten; jeder Befehl gilt als zerstörend (FR-012).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Claude Code mit holzi verbinden und registrieren (Priority: P1)

Anna nutzt Claude Code im Terminal ihres Laptops und möchte, dass es ihre offenen Tabs in holzi
lesen und Einstellungen öffnen kann. In holzi öffnet sie „KI → Agenten“, schaltet den MCP-Zugang
ein und wählt „Agent verbinden“. holzi zeigt für Claude Code, Codex und „anderer Agent“ je eine
fertige Anleitung zum Kopieren. Sie führt sie aus; beim ersten Verbinden zeigt holzi einen
Registrierungsdialog: „Claude Code“ (Angabe des Agenten), ab Werk „holzi bedienen: lesen“, sonst
nichts, Freigabestufe Manuell. Sie lässt zu. Claude Code sieht jetzt die lesenden holzi-Werkzeuge
und beantwortet „Welche Tabs sind offen?“.

**Why this priority**: Ohne Verbindung und Registrierung gibt es keinen externen Zugang; hier
entscheidet sich, dass nur Agenten, die der Nutzer kennt, überhaupt Werkzeuge sehen.

**Independent Test**: holzi mit offener Vault, MCP-Zugang an; einen MCP-Testclient und Claude Code
nach der Anleitung verbinden; Dialog, Registrieren, Werkzeugliste prüfen; Neustart von holzi und
Agent, erneutes Verbinden ohne Dialog.

**Acceptance Scenarios**:

1. **Given** eine offene Vault und der MCP-Zugang an, **When** sich ein unbekannter Agent
   verbindet, **Then** zeigt holzi den Registrierungsdialog mit angegebenem Namen (als Angabe
   gekennzeichnet), voreingestellten Rechten und Freigabestufe, und der Agent erfährt „wartet auf
   Registrierung“, ohne ein Werkzeug zu sehen.
2. **Given** der Dialog, **When** Anna registriert, **Then** erhält der Agent seinen
   Zugangsnachweis und danach die Werkzeuge, die seine Rechte decken, ohne neu verbinden zu
   müssen.
3. **Given** ein registrierter Agent, **When** er sich nach einem Neustart von holzi oder des
   Agenten mit seinem Zugangsnachweis wieder verbindet, **Then** ist er ohne Dialog verbunden.
4. **Given** der Dialog, **When** Anna ablehnt und „merken“ setzt, **Then** wird dieser Agent ohne
   Dialog abgewiesen, bis sie die Sperre aufhebt; ohne „merken“ gilt die Ablehnung bis zum
   Schließen der Vault.
5. **Given** die Anleitung für Claude Code oder Codex, **When** Anna sie befolgt, **Then** ist der
   Agent ohne weitere Handarbeit verbindungsbereit; für „anderer Agent“ nennt die Anleitung, was
   ein MCP-Client eintragen muss.
6. **Given** holzi läuft nicht, keine Vault ist offen oder der MCP-Zugang ist aus, **When** ein
   Agent sich zu verbinden versucht, **Then** erhält er die Meldung, dass holzi nicht offen ist,
   und es öffnet sich kein Fenster.

---

### User Story 2 - Ein Agent darf nur, was ihm erlaubt ist (Priority: P1)

Annas Claude Code darf holzi lesen, aber nichts ändern und keine Passwörter. Es versucht, das
Farbschema umzustellen und einen Passwort-Eintrag zu lesen. Beides wird abgelehnt; die
Werkzeugliste von Claude Code enthält diese Werkzeuge gar nicht erst. Die Leitplanken (Freigabe-
stufen, Deny-Regeln, Anbieter-Zugangsdaten, Agenten-Rechte) kann kein Agent sehen oder ändern.

**Why this priority**: „Kein Agent darf alles“ ist die Kernzusage dieser Spec. Ohne sie wäre die
Registrierung nur ein Dialog ohne Wirkung.

**Independent Test**: Einen Testagenten mit genau einer Rechtegruppe registrieren; für jede
Rechtegruppe und jede Wirkungsart prüfen, dass die Werkzeugliste nur Erlaubtes enthält und ein
direkter Aufruf eines nicht gelisteten Werkzeugs abgelehnt wird.

**Acceptance Scenarios**:

1. **Given** ein Agent mit „holzi bedienen: lesen“ für alle Bereiche, **When** er die
   Werkzeugliste abfragt, **Then** enthält sie nur lesende Aktionen, keine Werkzeuge von
   Erweiterungen, keine Datei-, Passwort- oder Befehlswerkzeuge und nichts aus dem Bereich
   Leitplanken.
2. **Given** derselbe Agent, **When** er ein nicht gelistetes Werkzeug trotzdem aufruft, **Then**
   lehnt holzi mit dem Grund „nicht erlaubt“ ab, ohne die Aktion auszuführen und ohne
   preiszugeben, ob ein bestimmtes Ziel (Eintrag, Datei, Tab) existiert.
3. **Given** ein Agent mit allen wählbaren Rechten, **When** er eine Aktion aus dem Bereich
   Leitplanken aufruft oder seine eigenen oder fremde Agenten-Rechte ändern will, **Then** wird er
   abgelehnt; diese Aktionen sind für keinen Agenten wählbar.
4. **Given** ein verbundener Agent, **When** Anna seine Rechte erweitert oder einschränkt, **Then**
   gilt die Änderung ab dem nächsten Aufruf, und der Agent wird benachrichtigt, dass sich seine
   Werkzeugliste geändert hat.

---

### User Story 3 - Rückfrage je Aufruf nach Freigabestufe (Priority: P1)

Anna hat Codex mit „lesen und ändern“ für die Einstellungen registriert, Freigabestufe Auto. Codex
stellt das Farbschema um, ohne zu fragen. Dann will es eine Unterhaltung löschen (zerstörend):
holzi zeigt eine Rückfrage „Codex (Angabe des Agenten) möchte: Unterhaltung „Urlaub“ löschen“.
Anna lehnt ab, Codex bekommt „vom Nutzer abgelehnt“.

**Why this priority**: Die Rechte legen fest, was ein Agent überhaupt darf; die Freigabestufe
legt fest, was davon ohne den Nutzer geschieht. Beides zusammen ist die Wiederverwendung der
bekannten Freigabe aus 003/032.

**Independent Test**: Denselben Testagenten nacheinander mit Manuell, Auto und Plan betreiben und
für je eine lesende, ändernde und zerstörende Aktion prüfen, ob holzi fragt, ausführt oder
ablehnt.

**Acceptance Scenarios**:

1. **Given** Freigabestufe Manuell, **When** der Agent irgendein Werkzeug aufruft, **Then** fragt
   holzi vor der Ausführung.
2. **Given** Auto, **When** der Agent lesend oder ändernd aufruft, **Then** führt holzi ohne
   Rückfrage aus; **When** zerstörend, **Then** fragt holzi.
3. **Given** Plan, **When** der Agent ändernd oder zerstörend aufruft, **Then** lehnt holzi ohne
   Rückfrage mit „im Modus Plan nicht erlaubt“ ab; lesende Aufrufe laufen.
4. **Given** eine Rückfrage, **When** sie erscheint, **Then** nennt sie den angegebenen Namen des
   Agenten (als Angabe gekennzeichnet), die Aktion mit ihrem lokalisierten Namen, Ziel und
   Eingaben (ohne Geheimnisse) und bietet Erlauben und Ablehnen.
5. **Given** eine Rückfrage und der Nutzer antwortet nicht innerhalb der Wartezeit, **When** sie
   abläuft, **Then** bekommt der Agent „keine Antwort vom Nutzer“, und nichts wird ausgeführt.
6. **Given** holzi ist nicht im Vordergrund, **When** eine Rückfrage entsteht, **Then** macht
   holzi über eine Systembenachrichtigung darauf aufmerksam, wo das Gerät sie anbietet.

---

### User Story 4 - Werkzeuge von haextensions für externe Agenten (Priority: P2)

Anna hat die Mail-Erweiterung installiert. Sie gibt Claude Code die Erweiterung „Mail“ frei.
Claude Code sieht die Mail-Werkzeuge und beantwortet „Welche Mails sind neu?“. Andere
Erweiterungen sieht es nicht.

**Why this priority**: haextensions über MCP für externe Agenten anzubieten ist der erklärte
Schwerpunkt von holzi; es baut auf US1–US3 auf.

**Independent Test**: Zwei Test-Erweiterungen mit je einem lesenden und einem ändernden Werkzeug;
einem Agenten nur eine freigeben; Werkzeugliste, Aufrufe und Rückfragen prüfen.

**Acceptance Scenarios**:

1. **Given** ein Agent mit einer freigegebenen Erweiterung, **When** er die Werkzeugliste abfragt,
   **Then** enthält sie deren Werkzeuge mit Name, Beschreibung und Eingabeschema, wie sie der
   eingebaute Agent erhält, und keine Werkzeuge anderer Erweiterungen.
2. **Given** ein Werkzeug einer Erweiterung, **When** der Agent es aufruft, **Then** gilt die
   Freigabestufe des Agenten nach der Wirkungsart des Werkzeugs, und zusätzlich jede Einschränkung,
   die der Nutzer für das Werkzeug in der Erweiterung festgelegt hat (Spec 018); die strengere
   Regel gewinnt.
3. **Given** eine neu installierte Erweiterung, **When** ein registrierter Agent die Werkzeugliste
   abfragt, **Then** fehlt sie, bis der Nutzer sie ihm freigibt.
4. **Given** eine freigegebene Erweiterung wird deinstalliert oder deaktiviert, **When** der Agent
   ein Werkzeug von ihr aufruft, **Then** erhält er „Werkzeug nicht verfügbar“, und seine
   Werkzeugliste wird aktualisiert.

---

### User Story 5 - Dateien, Kontakte und Passwörter für externe Agenten (Priority: P2)

Anna gibt Claude Code „Dateien des Geräts“ und für Passwörter „Lesen, Einträge mit Tag dev“. Es
sucht ein Dokument und liest es; es listet die dev-Einträge (nur Kopfdaten). Als es das Passwort
eines Eintrags braucht, fragt holzi Anna jedes Mal einzeln.

**Why this priority**: Diese Rechte sind in 034 und 044 schon vorbereitet und warten nur auf eine
Stelle, an der der Nutzer sie einem externen Agenten gibt.

**Independent Test**: Einen Testagenten mit je einer Datei-, Speicher-, Kontakt- und
Passwort-Freigabe registrieren und die Grenzen aus 034/044/051 für ihn prüfen; einen Agenten ohne
diese Rechte gegenprüfen.

**Acceptance Scenarios**:

1. **Given** ein externer Agent ohne Datei-Rechte, **When** er ein Dateiwerkzeug aufruft, **Then**
   gilt dasselbe wie für einen Agenten ohne Berechtigung in 044 (Ablehnung mit Grund oder
   Anfrage), und die eigenen Daten von holzi bleiben gesperrt.
2. **Given** eine Passwort-Freigabe „Lesen, Tag dev“, **When** der Agent Einträge listet, **Then**
   bekommt er nur Kopfdaten von Einträgen mit Tag dev, wie es 034 für Listen festlegt.
3. **Given** dieselbe Freigabe und Freigabestufe Auto, **When** der Agent das Geheimnis eines
   Eintrags abfragt, **Then** fragt holzi trotzdem nach, mit dem Hinweis, dass das Geheimnis an den
   Agenten und dessen Modellanbieter geht; nur nach Erlauben erhält er es (FR-019).
4. **Given** keine Passwort-Freigabe, **When** der Agent einen Eintrag abfragt, **Then** lehnt holzi
   ab, ohne zu verraten, ob der Eintrag existiert (034 FR-029).

---

### User Story 6 - Delegates im Chat bedienen holzi (Priority: P2)

Anna nutzt im holzi-Chat Claude Code als Delegate mit ihrem Abo. Bisher konnte es holzi nicht
bedienen. Beim ersten Mal zeigt der Chat „Claude Code hat noch keine Rechte in holzi“ mit „Rechte
festlegen“. Sie legt sie fest; danach bedient der Delegate holzi im Chat mit genau diesen Rechten,
und Rückfragen erscheinen im Chat wie die bisherigen Delegate-Freigaben.

**Why this priority**: Das ist der Grund, die Delegates zu behalten (032); es nutzt dieselben
Rechte und Rückfragen wie US1–US3.

**Independent Test**: Mit einem Test-Delegate (oder Claude Code/Codex) einen Chat ohne und mit
Registrierung führen; Werkzeuge, Rückfrage im Chat und Ablehnung im Modus Plan prüfen; Autonomie-
Modus „Ungated“ einstellen und prüfen, dass holzi-Werkzeuge trotzdem nach der Freigabestufe des
Delegates fragen.

**Acceptance Scenarios**:

1. **Given** ein Delegate ohne Registrierung, **When** der Nutzer ihn im Chat wählt, **Then**
   arbeitet er wie bisher ohne holzi-Werkzeuge, und der Chat bietet „Rechte festlegen“ an.
2. **Given** ein registrierter Delegate, **When** er im Chat läuft, **Then** erhält er die
   holzi-Werkzeuge seiner Rechte, ohne dass der Nutzer etwas am Delegate einrichtet.
3. **Given** eine Rückfrage für einen Delegate im Chat, **When** sie entsteht, **Then** erscheint
   sie im Chat an der Stelle der bisherigen Delegate-Freigaben, nicht als getrennter Dialog.
4. **Given** der Delegate läuft im Autonomie-Modus „Ungated“ (009), **When** er ein holzi-Werkzeug
   aufruft, **Then** gelten trotzdem seine Agenten-Rechte und seine Freigabestufe.

---

### User Story 7 - Agenten verwalten, Protokoll und Widerruf (Priority: P2)

Anna öffnet „KI → Agenten“. Sie sieht Claude Code (verbunden, zuletzt vor zwei Minuten, Manuell)
und Codex (nicht verbunden, Auto) mit ihren Rechten. Im Protokoll von Codex sieht sie, dass es
gestern drei Einstellungen geändert hat. Sie stellt Codex auf Plan und widerruft einen alten
Testagenten.

**Why this priority**: Ohne Übersicht, Protokoll und Widerruf bleibt eine Registrierung
unkontrolliert; es baut auf US1 auf.

**Independent Test**: Zwei Agenten registrieren, einen sperren; Rechte und Freigabestufe ändern,
widerrufen, Sperre aufheben; das Verhalten offener Verbindungen und das Protokoll prüfen.

**Acceptance Scenarios**:

1. **Given** registrierte und gesperrte Agenten, **When** Anna „KI → Agenten“ öffnet, **Then**
   sieht sie je Agent angegebenen Namen, Zeitpunkt der Registrierung, letzte Verbindung,
   verbunden ja/nein, Freigabestufe und eine Zusammenfassung der Rechte.
2. **Given** ein verbundener Agent, **When** Anna ihn widerruft, **Then** endet seine Verbindung
   sofort, wartende Rückfragen und Aufrufe enden mit einer Ablehnung, sein Zugangsnachweis ist
   ungültig, und ein erneutes Verbinden führt zu einem neuen Registrierungsdialog.
3. **Given** das Protokoll eines Agenten, **When** Anna es öffnet, **Then** sieht sie je Aufruf
   Zeit, Werkzeug, Wirkungsart, Ziel und Eingaben (etwa „Farbschema → dunkel“), Ergebnis
   (ausgeführt, abgelehnt durch Rechte, abgelehnt durch Nutzer, Fehler) und ob eine Rückfrage
   beantwortet wurde, ohne Geheimnisse und ohne Dateiinhalte.
4. **Given** ein Agent, der sich 90 Tage nicht verbunden hat, **When** er sich wieder verbindet,
   **Then** wird sein alter Nachweis abgelehnt, und holzi zeigt den Registrierungsdialog mit
   seinen bisherigen Rechten und seiner Freigabestufe vorausgefüllt.
5. **Given** ein Protokoll, **When** Anna es leert, **Then** ist es auf diesem Gerät leer; ältere
   Einträge verschwinden außerdem von selbst (FR-028).

---

### User Story 8 - Fremde Programme und Webseiten kommen nicht durch (Priority: P1)

Eine Webseite in Annas Browser versucht im Hintergrund, holzis MCP-Zugang auf dem Gerät
anzusprechen. Ein anderes Programm versucht es mit einem erfundenen Zugangsnachweis, ein drittes
verbindet sich hundertmal hintereinander. Keines bekommt Werkzeuge, und Anna sieht höchstens einen
Registrierungsdialog.

**Why this priority**: Ein lokaler Dienst ist für Webseiten und andere Programme erreichbar. Wenn
hier etwas nicht stimmt, ist jede Registrierung wertlos.

**Independent Test**: Verbindungen von einer Webseiten-Herkunft, mit falschem Hostnamen, von einer
anderen Netzschnittstelle, mit ungültigem oder widerrufenem Zugangsnachweis und in schneller Folge
aufbauen; prüfen, dass keine Werkzeuge, Daten oder mehr als ein Dialog je Agent entstehen.

**Acceptance Scenarios**:

1. **Given** eine Anfrage aus einem Browser mit der Herkunft einer Webseite, **When** sie den
   MCP-Zugang erreicht, **Then** weist holzi sie ab, ohne Dialog.
2. **Given** eine Verbindung von einem anderen Gerät im Netz, **When** sie versucht wird, **Then**
   kommt sie nicht zustande, weil der Zugang nur auf dem Gerät selbst erreichbar ist.
3. **Given** ein ungültiger, abgelaufener oder widerrufener Zugangsnachweis, **When** ein Agent ihn
   nutzt, **Then** bekommt er keine Werkzeuge und keine Daten.
4. **Given** derselbe unbekannte Agent verbindet sich hundertmal, **When** holzi fragt, **Then**
   steht genau ein Registrierungsdialog für ihn da; weitere unbekannte Agenten über die Grenze der
   gleichzeitig offenen Dialoge hinaus werden ohne Dialog abgewiesen.

---

### Edge Cases

- **Vault wird geschlossen oder gesperrt**: Alle Verbindungen enden, offene Rückfragen und
  Registrierungsdialoge enden mit Ablehnung, der Zugang hört auf zu lauschen.
- **Mehrere holzi-Instanzen auf einem Gerät (Spec 030)**: Jede Instanz hat ihre eigene Vault und
  ihren eigenen Zugang; ein Agent ist bei der Instanz registriert, bei der er sich registriert hat,
  und kann nicht versehentlich eine andere Vault bedienen.
- **Kein Fenster antwortet** (Aktion braucht die Oberfläche, aber die Webview hängt): Der Aufruf
  scheitert nach der Wartezeit mit einem Fehler, wie in ADR-0006 (`action_timeout`).
- **Mehrere Agenten gleichzeitig**: Jeder sieht nur seine Werkzeuge; Rückfragen verschiedener
  Agenten erscheinen nacheinander, die älteste zuerst, jede mit dem Namen ihres Agenten.
- **Ein Agent ruft viele Werkzeuge in kurzer Zeit auf**: holzi begrenzt die Zahl gleichzeitig
  laufender Aufrufe je Agent; was darüber liegt, wartet oder wird mit „zu viele Aufrufe“
  abgelehnt, und die Oberfläche bleibt bedienbar.
- **Ein Werkzeug liefert sehr viel**: Ergebnisse über einer Größengrenze werden gekürzt oder als
  Verweis geliefert, wie für den eingebauten Agenten.
- **Ein Agent meldet sich mit dem Namen eines anderen** („Claude Code“): holzi zeigt den Namen nur
  als Angabe; maßgeblich ist der Zugangsnachweis. Zwei Agenten mit gleichem Namen sind zwei
  Einträge.
- **Eine Aktion fragt ihrerseits** (046 „Auswahl nötig“, 044 Speicher-Anfrage): Externe Agenten
  bekommen bei 046 das Ergebnis mit Kandidaten; eine Berechtigungsanfrage aus 044 erscheint als
  Rückfrage mit dem Namen des Agenten.
- **Ein Agent ändert den Fensterzustand**: Aktionen wirken wie beim Nutzer (020 FR-029); Ziele
  müssen ausdrücklich angegeben sein (020 FR-030).
- **Agent-Rechte auf einem anderen Gerät**: Eine Registrierung gilt nur auf dem Gerät, auf dem sie
  erteilt wurde; auf einem anderen Gerät derselben Vault ist derselbe Agent unbekannt.

## Requirements _(mandatory)_

### Functional Requirements

**Zugang und Verbindung**

- **FR-001**: holzi MUSS einen MCP-Zugang anbieten, den der Nutzer je Gerät in „KI → Agenten“ ein-
  und ausschaltet; ab Werk ist er aus. Die Einstellung MUSS geräteeigen sein und ist eine
  Leitplanke (für Agenten gesperrt).
- **FR-002**: Der Zugang MUSS ausschließlich auf dem Gerät selbst erreichbar sein (nie über andere
  Netzschnittstellen) und DARF nur bereitstehen, solange eine Vault offen ist. Schließen oder
  Sperren der Vault und Ausschalten des Zugangs MÜSSEN alle Verbindungen beenden und offene
  Rückfragen und Registrierungsdialoge mit einer Ablehnung beenden. Läuft holzi nicht oder ist
  keine Vault offen, MUSS ein Agent, der sich verbinden will, die Meldung „holzi ist nicht offen“
  erhalten; dabei DARF weder holzi gestartet noch zum Öffnen oder Entsperren einer Vault
  aufgefordert werden.
- **FR-003**: Der Zugang MUSS das Model Context Protocol in einer Form sprechen, mit der sich
  Claude Code und Codex in ihren aktuellen Versionen ohne Änderung verbinden, und zwar sowohl
  Agenten, die einen MCP-Server als Programm starten, als auch solche, die eine lokale Adresse
  aufrufen. Wo das Protokoll für den gewählten Weg eine Anmeldung vorsieht, MUSS holzi sie nutzen.
- **FR-004**: holzi MUSS Anfragen abweisen, die aus einem Browser mit der Herkunft einer Webseite
  kommen oder einen anderen Hostnamen als den des Geräts selbst nennen, bevor ein Dialog entsteht.
- **FR-005**: holzi MUSS für Claude Code, Codex und „anderer Agent“ je eine Anleitung zum
  Verbinden anbieten, die der Nutzer kopieren kann; sie DARF keinen Zugangsnachweis enthalten.

**Registrierung**

- **FR-006**: Ein Agent MUSS registriert sein, bevor er ein Werkzeug sieht oder aufruft. Verbindet
  sich ein unbekannter Agent, MUSS holzi einen Registrierungsdialog mit angegebenem Namen (als
  Angabe gekennzeichnet), Rechten und Freigabestufe zeigen; der Nutzer kann registrieren oder
  ablehnen, eine Ablehnung wahlweise gemerkt (Sperre). Der Agent DARF den Dialog selbst auslösen,
  wie die Browser-Erweiterung in 049; der Nutzer muss die Registrierung nicht vorher in den
  Einstellungen beginnen (Klärung 2026-10-26).
- **FR-007**: Bei der Registrierung MUSS holzi dem Agenten einen eigenen Zugangsnachweis ausgeben,
  mit dem er sich später ohne Dialog ausweist. Ein Agent ohne gültigen Zugangsnachweis DARF keine
  Werkzeuge und keine Daten erhalten. Der Nachweis MUSS widerrufbar sein und DARF nach der Ausgabe
  nirgends in holzi angezeigt werden. Hat sich ein Agent 90 Tage lang nicht verbunden, MUSS seine
  Registrierung ablaufen: Sein Nachweis ist ungültig, und beim nächsten Verbinden (bei einem
  Delegate: beim nächsten Start im Chat) erscheint der Registrierungsdialog mit seinen bisherigen
  Rechten und seiner Freigabestufe vorausgefüllt.
- **FR-008**: Die Agenten-Rechte MÜSSEN je Rechtegruppe vergeben werden (holzi bedienen,
  haextensions, Dateien, Passwörter, Befehle ausführen). Spätere Specs DÜRFEN neue Rechtegruppen
  hinzufügen (etwa Browser-Werkzeuge); eine neue Rechtegruppe MUSS für alle bestehenden Agenten
  aus sein, bis der Nutzer sie erteilt.
- **FR-009**: Ab Werk MUSS ein neuer Agent nur „holzi bedienen: lesen“ für die nicht gesperrten
  Bereiche und die Freigabestufe Manuell haben; alle anderen Rechtegruppen sind aus. Der Nutzer
  kann das im Dialog vor dem Registrieren ändern.
- **FR-010**: Registrierungen, Sperren und Agenten-Rechte MÜSSEN geräteeigen sein: Sie gelten nur
  auf dem Gerät und in der Vault, in der sie erteilt wurden.
- **FR-011**: Es DARF höchstens ein offener Registrierungsdialog je Agent und höchstens drei
  gleichzeitig offene Registrierungsdialoge insgesamt geben; weitere unbekannte Agenten werden ohne
  Dialog mit „Registrierung gerade nicht möglich“ abgewiesen.

**Werkzeuge und Rechte**

- **FR-012**: Die Werkzeugliste eines Agenten MUSS genau die Werkzeuge enthalten, die seine Rechte
  decken: Aktionen mit „durch Agenten aufrufbar“ aus den erlaubten Bereichen in der erlaubten
  Wirkungsart („lesen“: nur lesende; „lesen und ändern“: lesende, ändernde und zerstörende),
  Werkzeuge der freigegebenen Erweiterungen, Datei- und Passwortwerkzeuge nach seinen Freigaben
  und holzis Befehlswerkzeug, wenn „Befehle ausführen“ erteilt ist. Das Befehlswerkzeug MUSS als
  zerstörend gelten (Auto fragt, Plan lehnt ab).
- **FR-013**: Name, Beschreibung und Eingabeschema eines Werkzeugs MÜSSEN dieselben sein, die der
  eingebaute Agent erhält (eine Definition je Aktion, ADR-0006). Aktionen, die nur für den
  eingebauten Agenten gesperrt sind, MÜSSEN für externe Agenten angeboten werden.
- **FR-014**: holzi MUSS jeden Aufruf eines externen Agenten unabhängig von der Werkzeugliste
  erneut gegen seine aktuellen Rechte prüfen; ein nicht gedeckter Aufruf MUSS mit „nicht erlaubt“
  abgelehnt werden, ohne auszuführen und ohne preiszugeben, ob ein Ziel existiert.
- **FR-015**: Jeder Aufruf eines externen Agenten MUSS mit dem Aufrufer „externer Agent“ und der
  Kennung dieses Agenten ausgeführt werden, sodass alle Prüfungen (Aktionen, Dateien, Kontakte,
  Passwörter, Erweiterungen) ihn als diesen Agenten sehen.
- **FR-016**: Aktionen im Bereich Leitplanken MÜSSEN für jeden Agenten gesperrt sein. Dazu gehören
  mindestens Freigabestufen, Deny-Regeln und Autonomie-Modus der Delegates, Anbieter-Zugangsdaten,
  der MCP-Zugang selbst, Registrierung, Sperre, Widerruf und Rechte von Agenten. Das gilt auch für
  den eingebauten Agenten (020).
- **FR-017**: Ändert sich die Werkzeugliste eines verbundenen Agenten (Rechte geändert,
  Erweiterung installiert, entfernt oder freigegeben), MUSS holzi ihn darüber benachrichtigen,
  soweit das Protokoll es vorsieht; ändern sich die Rechte, gelten sie ab dem nächsten Aufruf.
- **FR-018**: Werkzeuge von Erweiterungen MÜSSEN je Agent und Erweiterung freigegeben werden. Für
  einen Aufruf gelten zusätzlich die Einstellungen, die der Nutzer für das Werkzeug in der
  Erweiterung getroffen hat (018); die strengere Regel gewinnt.
- **FR-019**: Für Passwörter MUSS die Prüfung aus 034 mit der Freigabe des Agenten gelten
  (Kopfdaten in Listen, Geheimnis nur auf einzelne Anfrage, keine Auskunft außerhalb des
  Bereichs). Ein externer Agent mit passender Freigabe DARF das Geheimnis eines Eintrags
  (Passwort, TOTP-Code, geheime Felder) im Klartext erhalten, aber nur nach einer Rückfrage bei
  jedem einzelnen Abruf, in jeder Freigabestufe, auch in Auto. Die Rückfrage MUSS sagen, dass das
  Geheimnis an den Agenten und dessen Modellanbieter geht (Klärung 2026-10-26).
- **FR-020**: Für Dateien, Speicher, verschlüsselte Ordner, Kontakte und Dateifreigaben MÜSSEN die
  Agent-Berechtigungen aus 044/048/051/056 mit der Kennung des Agenten gelten; ein externer Agent
  hat ab Werk keine davon, und die eigenen Daten von holzi bleiben gesperrt.

**Freigabestufe und Rückfrage**

- **FR-021**: Jeder registrierte Agent MUSS eine eigene Freigabestufe (Manuell, Auto, Plan) haben,
  die nur der Nutzer ändert. Sie MUSS wie in 032/ADR-0006 wirken: Manuell fragt bei allem; Auto
  führt lesend und ändernd ohne Rückfrage aus und fragt bei zerstörend; Plan führt nur lesend aus
  und lehnt den Rest ohne Rückfrage ab. Der Abruf eines Geheimnisses (FR-019) MUSS in jeder
  Stufe fragen.
- **FR-022**: Eine Rückfrage MUSS den angegebenen Namen des Agenten (als Angabe gekennzeichnet),
  den lokalisierten Namen der Aktion, ihr Ziel und ihre Eingaben ohne Geheimnisse zeigen und
  Erlauben und Ablehnen anbieten. Rückfragen mehrerer Aufrufe MÜSSEN nacheinander erscheinen, die
  älteste zuerst.
- **FR-023**: Eine Rückfrage MUSS nach einer Wartezeit (ab Werk 5 Minuten) mit „keine Antwort vom
  Nutzer“ enden, ohne auszuführen. Ist holzi nicht im Vordergrund, MUSS holzi mit einer
  Systembenachrichtigung auf eine Rückfrage aufmerksam machen, wo das Gerät das anbietet.

**Delegates**

- **FR-024**: Ein Delegate (Claude Code, Codex) MUSS als externer Agent registriert werden können,
  je Art einmal je Gerät. Ist er registriert, MUSS holzi ihm beim Start im Chat den Zugang zu
  seinen holzi-Werkzeugen mitgeben, ohne dass der Nutzer am Delegate etwas einrichtet. Der
  Autonomie-Modus des Delegates (009) DARF nur dessen eigene Werkzeuge betreffen; für
  holzi-Werkzeuge gelten immer seine Agenten-Rechte und seine Freigabestufe.
- **FR-025**: Rückfragen eines Delegates zu holzi-Werkzeugen MÜSSEN im Chat erscheinen, an der
  Stelle der bisherigen Delegate-Freigaben. Ein Delegate ohne Registrierung MUSS wie bisher
  arbeiten; der Chat MUSS dann „Rechte festlegen“ anbieten, das den Registrierungsdialog öffnet.

**Verwaltung und Protokoll**

- **FR-026**: „KI → Agenten“ MUSS je Agent angegebenen Namen, Zeitpunkt der Registrierung, letzte
  Verbindung, verbunden ja/nein, Freigabestufe und Rechte zeigen und Rechte, Freigabestufe,
  Widerruf und Aufheben einer Sperre erlauben. Änderungen gelten mit der Auswahl, ohne Knopf zum
  Speichern (023 FR-021).
- **FR-027**: Ein Widerruf MUSS die Verbindung des Agenten sofort beenden, wartende Rückfragen und
  Aufrufe mit Ablehnung beenden und seinen Zugangsnachweis ungültig machen.
- **FR-028**: holzi MUSS je Agent ein Agenten-Protokoll führen: Zeit, Werkzeug, Wirkungsart, Ziel,
  Eingaben, Ergebnis (ausgeführt, abgelehnt durch Rechte, abgelehnt durch Nutzer, keine Antwort,
  Fehler) und ob eine Rückfrage beantwortet wurde. Ziel und Eingaben MÜSSEN so lesbar sein, dass
  der Nutzer sieht, was geändert wurde (etwa „Farbschema → dunkel“). Das Protokoll DARF keine
  Geheimnisse, keine Dateiinhalte und keine Werkzeugergebnisse enthalten; Eingaben, die ein
  Geheimnis wären, MÜSSEN durch einen Platzhalter ersetzt werden. Es MUSS geräteeigen sein, ältere
  Einträge als 30 Tage von selbst entfernen und vom Nutzer geleert werden können.

**Grenzen**

- **FR-029**: holzi MUSS die Zahl gleichzeitig laufender Aufrufe je Agent begrenzen und die Größe
  von Ergebnissen wie für den eingebauten Agenten begrenzen; die Oberfläche MUSS dabei bedienbar
  bleiben.
- **FR-030**: Ein externer Agent DARF Erweiterungen nur über ihre erklärten Werkzeuge erreichen,
  nie ihren Rahmen, ihre Daten oder holzis Historie direkt.

### Key Entities _(include if feature involves data)_

- **Registrierter Agent**: Kennung, angegebener Name, Art (Delegate Claude Code, Delegate Codex,
  anderer Agent), Zeitpunkt der Registrierung, letzte Verbindung, Freigabestufe, Zugangsnachweis
  (nur in geprüfter Form gespeichert), Zustand (registriert, abgelaufen, gesperrt). Geräteeigen.
  Registriert → abgelaufen nach 90 Tagen ohne Verbindung; abgelaufen → registriert durch erneute
  Zustimmung im Dialog; jeder Zustand → entfernt durch Widerruf.
- **Agenten-Recht**: gehört zu genau einem Agenten und einer Rechtegruppe; je Gruppe mit eigenem
  Inhalt (Bereich und Stufe; Erweiterung; Datei-Berechtigung aus 044; Passwort-Freigabe aus 034;
  Befehle an/aus).
- **Rückfrage**: Agent, Aufruf, Zeitpunkt, Wartezeit, Ergebnis.
- **Protokolleintrag**: Agent, Zeit, Werkzeug, Wirkungsart, Ziel, Eingaben (ohne Geheimnisse),
  Ergebnis, Rückfrage ja/nein.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer ohne Vorwissen verbindet Claude Code oder Codex nach der Anleitung in
  holzi in unter 3 Minuten und kann es danach „Welche Tabs sind offen?“ beantworten lassen.
- **SC-002**: In einer automatischen Prüfung über alle Rechtegruppen und Wirkungsarten erhält ein
  Agent in 100 % der Fälle nur Werkzeuge und Ergebnisse, die seine Rechte decken; kein Aufruf aus
  dem Bereich Leitplanken gelingt.
- **SC-003**: Kein Versuch aus der Prüfliste von US8 (Webseiten-Herkunft, fremder Hostname, anderes
  Gerät, ungültiger oder widerrufener Nachweis, Verbindungsflut) liefert Werkzeuge oder Daten oder
  mehr als einen Dialog je Agent.
- **SC-004**: Nach einem Widerruf erhält der Agent innerhalb von 1 Sekunde keine weiteren
  Antworten mehr.
- **SC-005**: Ein Agent sieht seine Werkzeugliste innerhalb von 2 Sekunden nach dem Verbinden; ein
  lesender Aufruf ohne Rückfrage dauert für den Agenten höchstens 0,5 Sekunden länger als dieselbe
  Aktion im eingebauten Agenten.
- **SC-006**: Ein registrierter Delegate bedient holzi im Chat ohne einen einzigen Handgriff am
  Delegate selbst.
- **SC-007**: Für jeden Aufruf eines externen Agenten steht ein Eintrag im Agenten-Protokoll, und
  keiner enthält ein Geheimnis.

## Assumptions

- Externe Agenten laufen auf demselben Gerät wie holzi. Zugriff von anderen eigenen Geräten
  (Fernaufruf über die Geräte-Föderation, wie in `docs/design/founding.md` skizziert) ist eine
  eigene Spec.
- Der angegebene Name eines Agenten ist nicht prüfbar. Ein Programm, das unter demselben
  Betriebssystem-Benutzer läuft und den gespeicherten Zugangsnachweis eines Agenten liest, kann
  sich als dieser Agent ausgeben; dagegen schützt holzi nicht, so wie 049 nicht gegen ein
  Programm schützt, das den Schlüssel einer Erweiterung liest. Schutz bieten Rechte, Freigabestufe,
  Protokoll und Widerruf.
- Was ein Agent aus holzi liest, gelangt zu dessen Modellanbieter. Inhalte aus Mails, Dateien oder
  Webseiten können Anweisungen enthalten, die einen Agenten zu Aufrufen verleiten; dagegen helfen
  die Rechte je Agent und die Freigabestufe, nicht eine Prüfung der Inhalte.
- Der MCP-Zugang läuft auf allen Plattformen, auf denen holzi läuft. Auf Mobilgeräten erreichen ihn
  nur Programme auf demselben Gerät und nur, solange holzi läuft; Delegates gibt es wie bisher nur
  auf dem Desktop.
- Die Rechte für Dateien und Kontakte (044/051/056) und die Passwort-Prüfung (034) sind schon mit
  einer Agenten-Kennung gebaut; diese Spec vergibt sie, sie ändert ihre Bedeutung nicht.
- Für den eingebauten Agenten ändert sich nichts: Er behält seine Freigabestufe aus den
  Chat-Einstellungen, bekommt nie Geheimnisse (034) und hat keine Rechte je Erweiterung über 018
  hinaus.
- Die Wartezeit der Rückfrage (5 Minuten), die Aufbewahrung des Protokolls (30 Tage) und die
  Grenze der offenen Registrierungsdialoge (3) sind übliche Werte und in der Planung änderbar.
- ADR-0005 ist für diese Spec reserviert (ADR-0006); die Entscheidung über Weg und Anmeldung des
  Zugangs gehört dorthin.

## Nicht im Umfang

- Browser-Werkzeuge für Agenten (eigene Folge-Spec, nutzt FR-008).
- MCP-Zugang von anderen Geräten oder über das Netz.
- Eigene Agenten-Laufzeit in holzi; holzi beantwortet nur Aufrufe (siehe
  `docs/design/founding.md`, „External MCP access“).
- Ressourcen und Prompts im Sinn von MCP; diese Spec bietet nur Werkzeuge an.
- Rechte je Erweiterung für den eingebauten Agenten über 018 hinaus.
