# Feature Specification: Modell bedient holzi über die Aktionen

**Feature Branch**: `032-model-operates-holzi`

**Created**: 2026-09-30

**Status**: Draft

**Input**: Nutzer sollen holzi im Chat in natürlicher Sprache bedienen können —
mit einem lokalen Modell (z. B. Qwen3) ebenso wie mit einem Cloud-Modell
(z. B. Claude per API-Key): Einstellungen lesen und ändern, Apps und Tabs
öffnen. Holzi baut dafür **keinen eigenen Agenten**; es ist nur die Brücke
zwischen dem vorhandenen Werkzeug-Ablauf im Chat (Spec 003) und den
vorhandenen Aktionen des Window Managers (Spec 020), ergänzt um die
Vorkehrungen, die kleine lokale Modelle dafür brauchen.

## Beziehung zu bestehenden Specs

- [`003-agent-tool-loop`](../003-agent-tool-loop/spec.md) liefert den Ablauf
  „Modell ruft Werkzeug, holzi führt aus“ samt Freigabe (Manuell / Auto /
  Plan), Wiederholungen, Abbruch und Rundengrenze. Diese Spec ändert diesen
  Ablauf nicht, sie gibt ihm Werkzeuge: bisher kennt das Modell nur einen
  einzigen, der beliebige Befehle auf dem Rechner ausführt.
- [`020-tab-navigation`](../020-tab-navigation/spec.md) macht jede
  zustandsändernde Bedienung des Window Managers und seiner Apps zu einer
  benannten **Aktion** und hat Agenten als Aufrufer bereits vorgesehen
  (Wirkungsart lesen / ändern / zerstörend, „durch Agenten aufrufbar“ ja/nein,
  Leitplanken für Agenten gesperrt). Bisher ruft nur Testcode Aktionen mit
  Aufrufer „Agent“ auf. Diese Spec öffnet den **eingebauten** Agenten
  (Aufrufer „eingebauter Agent“) für diese Aktionen.
- [`023-settings-app`](../023-settings-app/spec.md) definiert die
  Einstellungs-Aktionen, die hier per Chat erreichbar werden; die
  Leitplanken-Einstellungen bleiben dabei gesperrt.
- [`012-unified-model-capabilities`](../012-unified-model-capabilities/spec.md)
  führt den Fähigkeiten-Eintrag je Modell; er wird um die Fähigkeit
  „Werkzeugnutzung“ erweitert.
- **Nicht Teil dieser Spec**: die haextension-Laufzeit und Werkzeuge aus
  haextensions (017–019), holzi als MCP-Server für externe Agenten mit
  Anmeldung und Berechtigungen je Agent (021), und weitere Cloud-Anbieter
  neben dem vorhandenen (eigene Spec). Sobald Aktionen aus haextensions
  existieren, sollen sie ohne Änderung an dieser Spec im Chat erscheinen
  (siehe Annahmen).
- **CLI-Delegates (Claude Code, Codex; Specs 007/009)** bekommen in dieser
  Spec **keine** holzi-Werkzeuge: sie führen ihren eigenen Ablauf mit eigenen
  Werkzeugen aus. Sie bleiben bewusst erhalten, weil sie Nutzern ein
  vorhandenes Abo im holzi-Chat ohne eigenen API-Key erschließen; mit
  Spec 021 sollen sie holzi über MCP bedienen können (siehe Annahmen).
  Bis dahin zeigt der Chat bei einem Delegate einen Hinweis (US4, FR-023).
- **Download- und Sync-Steuerung (Datenvolumen)**: Große Downloads (Modelle)
  und die Dateisynchronisation pausieren zu können, um auf Mobilgeräten das
  Datenvolumen zu schonen, ist ein eigenes Vorhaben mit eigener Spec (berührt
  005 und 025) und nicht Teil dieser Spec. Bis dahin gilt: Ein Modell
  darf Downloads im Modus „Auto“ wie jede andere ändernde Aktion ohne
  Rückfrage anstoßen (Desktop ist heute das einzige Ziel).

## Clarifications

### Session 2026-09-30

- Q: Soll der Nutzer besonders geschützt oder informiert werden, wenn ein
  Cloud-Modell über Lese-Aktionen Informationen aus holzi zu sehen bekommt?
  → A: Nein, kein zusätzlicher Hinweis und keine „nur lokal“-Kennzeichnung;
  dass Chat-Inhalte an den gewählten Anbieter gehen, ist dem Nutzer klar,
  und die Verlaufszeilen (FR-005) zeigen, was übermittelt wurde.
- Q: Soll der Nutzer das Bedienen von holzi durch ein Modell im Chat komplett
  abschalten können (eigener Schalter, unabhängig vom Freigabe-Modus)?
  → A: Nein, kein eigener Schalter; die Steuerung läuft allein über die
  Freigabe-Modi (Manuell / Auto / Plan), Standard bleibt „immer fragen“.
- Q: Müssen Downloads, die ein Modell anstößt (z. B. ein Modell laden), extra
  erfragt werden? → A: Nein, sie gelten wie jede ändernde Aktion. Eine
  allgemeine Steuerung für große Downloads und Dateisync auf Mobilgeräten
  (pausieren, Datenvolumen schonen) kommt mit einer eigenen Spec.
- Q: Wie entscheidet holzi, welche Werkzeuge ein Modell in einer Antwort
  sieht? → A: Nicht anhand des Nutzertextes. Jedes Modell, lokal wie Cloud,
  bekommt dasselbe feste Kernangebot der häufigsten Aktionen plus ein
  Suchwerkzeug und findet alles Weitere selbst; eine einheitliche Obergrenze
  gilt für alle Modelle, ohne Ausnahme je Modell.
- Q: Dürfen die Hinweise (unbekanntes Modell, nicht unterstütztes Modell,
  Delegate) nach einem App-Neustart erneut erscheinen? → A: Ja, sie erscheinen
  einmal je Unterhaltung und Zustand, solange die App läuft.
- Q: Gelten für lokale und Cloud-Modelle dieselben Werkzeuge und dasselbe
  Muster? → A: Ja. Jedes Modell, bei dem holzi den Ablauf fährt, bekommt
  dieselben Werkzeuge auf demselben Weg; es gibt keinen Sonderweg je Anbieter.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Holzi per Chat bedienen: lesen und navigieren (Priority: P1)

Ein Nutzer schreibt im Chat „Öffne die Sync-Einstellungen“, „Welche Tabs sind
gerade offen?“ oder „Welches Farbschema ist eingestellt?“. Das Modell führt
dafür die passenden Aktionen aus — die Einstellungs-App öffnet sich an der
richtigen Stelle, bzw. das Modell liest den Wert — und antwortet mit dem
tatsächlichen Ergebnis statt mit einer Anleitung, was der Nutzer selbst
klicken müsste.

**Why this priority**: Das ist die Kernfähigkeit. Sie ist ohne Risiko für
Nutzerdaten (Lesen und Öffnen) und zeigt sofort den Wert: holzi lässt sich
sprechen statt klicken. Alles Weitere baut darauf auf.

**Independent Test**: Mit einem Cloud-Modell und mit einem lokalen Modell je
die drei Beispielsätze eingeben; die Oberfläche zeigt das Erwartete (App offen,
Tab aktiv), die Antwort nennt den echten Wert bzw. die echte Tab-Liste, und im
Verlauf ist sichtbar, welche Aktionen das Modell ausgelöst hat.

**Acceptance Scenarios**:

1. **Given** ein Chat mit einem werkzeugfähigen Modell, **When** der Nutzer
   bittet, eine bestimmte App bzw. einen bestimmten Einstellungsbereich zu
   öffnen, **Then** öffnet holzi genau das — mit derselben Wirkung wie der
   Klick — und das Modell bestätigt es in seiner Antwort.
2. **Given** ein Chat mit einem werkzeugfähigen Modell, **When** der Nutzer
   nach einem Zustand fragt (offene Tabs und Fenster, eine Einstellung),
   **Then** liest das Modell den Wert über eine Aktion und nennt ihn korrekt;
   es rät nicht.
3. **Given** eine Bitte, die sich auf einen bestimmten Tab oder ein Fenster
   bezieht („Schließe den Tab mit den Einstellungen“), **When** das Modell
   sie ausführen soll, **Then** ermittelt es zuerst, welcher Tab bzw. welches
   Fenster gemeint ist (Agenten müssen das Ziel immer ausdrücklich benennen),
   und führt die Aktion erst dann aus.
4. **Given** das Modell löst Aktionen aus, **When** der Nutzer den
   Chat-Verlauf ansieht, **Then** ist jede ausgelöste Aktion mit Name,
   Eingaben und Ergebnis als eigene Zeile erkennbar (wie bei den heutigen
   Werkzeug-Zeilen).
5. **Given** eine Aktion schlägt fehl (unbekannte Eingabe, Ziel nicht
   gefunden, App nicht verfügbar), **When** das Modell das Ergebnis erhält,
   **Then** bekommt es eine verständliche Fehlermeldung, kann die Eingabe
   korrigieren oder dem Nutzer den Grund erklären, und holzi bleibt stabil.

---

### User Story 2 - Änderungen nur mit Freigabe nach Modus, Leitplanken bleiben gesperrt (Priority: P1)

Ein Nutzer bittet das Modell, etwas zu ändern: „Stelle auf das dunkle
Farbschema um“ oder „Schließe alle anderen Tabs“. Ob das Modell es sofort tun
darf oder der Nutzer erst zustimmen muss, richtet sich nach dem gewählten
Freigabe-Modus (Manuell / Auto / Plan) und nach der Wirkungsart der Aktion.
Einstellungen, die den Schutz des Nutzers betreffen (Freigabe-Modus selbst,
Sperrregeln, Zugangsdaten von Anbietern, Berechtigungen von Agenten), kann
das Modell nie ändern — auch nicht mit Zustimmung im Chat.

**Why this priority**: Ohne diese Story wäre US1 nur halb nutzbar (kein
Ändern) oder unsicher (Ändern ohne Kontrolle). Sie ist die Bedingung dafür,
dass holzi Modelle überhaupt an Einstellungen lassen kann.

**Independent Test**: In jedem der drei Modi je eine lesende, eine ändernde
und eine zerstörende Aktion per Chat anfordern und prüfen, ob sie wie in der
Freigabe-Regel (siehe FR-007) erwartet läuft, nachfragt oder abgelehnt wird;
dann jede Leitplanken-Aktion durch das Modell auslösen lassen und prüfen,
dass sie abgelehnt wird.

**Acceptance Scenarios**:

1. **Given** Modus „Manuell“, **When** das Modell irgendeine Aktion
   auslösen will, **Then** fragt holzi den Nutzer vorher mit klarer Angabe,
   welche Aktion mit welchen Eingaben laufen soll; ohne Zustimmung geschieht
   nichts.
2. **Given** Modus „Plan“, **When** das Modell eine ändernde oder
   zerstörende Aktion auslösen will, **Then** wird sie ohne Rückfrage
   abgelehnt und das Modell bekommt die Begründung; lesende Aktionen laufen.
3. **Given** Modus „Auto“, **When** das Modell eine lesende Aktion auslöst,
   **Then** läuft sie ohne Rückfrage; bei ändernden und zerstörenden Aktionen
   gilt die Freigabe-Regel aus FR-007.
4. **Given** der Nutzer lehnt eine angefragte Aktion ab, **When** das Modell
   die Ablehnung erhält, **Then** wiederholt es dieselbe Aktion nicht
   unverändert, sondern erklärt oder fragt nach.
5. **Given** eine Aktion aus dem Bereich Leitplanken, **When** das Modell sie
   aufruft, **Then** wird sie immer abgelehnt, in jedem Modus, und das Modell
   bekommt eine eindeutige Fehlermeldung, dass der Nutzer dies selbst
   einstellen muss.
6. **Given** das Modell hat eine Änderung vorgenommen, **When** der Nutzer die
   Oberfläche ansieht, **Then** spiegelt sie die Änderung sofort (z. B. neues
   Farbschema) und der Verlauf zeigt die ausgelöste Aktion.

---

### User Story 3 - Alle Modelle bekommen ein kleines, gleiches Angebot und finden den Rest selbst (Priority: P2)

Jedes Modell — lokal oder Cloud — bekommt in jedem Schritt einer Antwort
dasselbe kleine, feste Kernangebot an Werkzeugen (die häufigsten Aktionen)
plus ein Suchwerkzeug. Braucht es etwas anderes („Stelle auf das dunkle
Farbschema um“), sucht es danach; die gefundenen Werkzeuge stehen ihm ab dem
nächsten Schritt derselben Antwort zur Verfügung. Holzi rät nicht aus dem
Text des Nutzers, was gebraucht wird: das Modell formuliert die Suche selbst.
Das Angebot bleibt damit für kleine Modelle überschaubar, sieht für alle
Modelle gleich aus und wächst nicht mit der Zahl der Aktionen, auch nicht,
wenn später Werkzeuge aus haextensions dazukommen.

**Why this priority**: Alle Werkzeuge mitzuschicken kostet heute rund 4 000
Token je Anfrage (51 Aktionen) und wächst mit jeder neuen Quelle; kleine
Modelle werden dadurch langsamer und ungenauer. Ein festes Kernangebot ist
zudem in jeder Antwort dasselbe und damit stabil. Die Story ist erst sinnvoll,
wenn US1/US2 Aktionen überhaupt anbieten.

**Independent Test**: Bei einer Aktionsliste, die größer ist als die
Obergrenze, eine Reihe von Beispielsätzen stellen und prüfen, dass (a) in
keinem Schritt mehr Werkzeuge angeboten werden als die Obergrenze, (b) das
Kernangebot in jeder Antwort und für jedes Modell identisch ist, (c) das
passende Werkzeug entweder im Kernangebot ist oder nach der Suche im nächsten
Schritt angeboten wird und (d) das Modell es aufrufen kann.

**Acceptance Scenarios**:

1. **Given** mehr Aktionen, als gleichzeitig angeboten werden sollen,
   **When** eine Antwort beginnt, **Then** bietet holzi jedem Modell dasselbe
   feste Kernangebot an (höchstens die festgelegte Anzahl, einschließlich
   Suchwerkzeug), unabhängig von Anbieter, Modell und Nutzertext.
2. **Given** eine Bitte, deren Aktion nicht im Kernangebot ist, **When** das
   Modell das Suchwerkzeug mit einem Suchbegriff aufruft, **Then** liefert es
   die passendsten Aktionen (Name, Beschreibung, Eingaben), und sie stehen dem
   Modell ab dem nächsten Schritt derselben Antwort als Werkzeuge zur
   Verfügung; es kann sie danach aufrufen, ohne Eingriff des Nutzers.
3. **Given** Aktionen, die Agenten nie aufrufen dürfen, **When** das Angebot
   oder ein Suchergebnis entsteht, **Then** sind sie nie enthalten.
4. **Given** eine Suche ohne passende Aktion, **When** das Modell das Ergebnis
   erhält, **Then** bekommt es eine eindeutige Antwort „keine passende
   Aktion“ und kann dem Nutzer erklären, dass holzi das nicht kann.
5. **Given** ein leistungsstarkes Cloud-Modell, **When** der Nutzer eine
   Frage stellt, **Then** gelten dasselbe Verfahren und dieselbe Obergrenze
   wie für ein kleines lokales Modell.

---

### User Story 4 - Modelle ohne Werkzeugnutzung werden nicht damit überfordert (Priority: P2)

Jedes Modell hat im Fähigkeiten-Eintrag einen Wert „Werkzeugnutzung“. Ein
Nutzer, der ein Modell ohne Werkzeugunterstützung (oder ein nicht
verlässliches) gewählt hat — auch ein frisch heruntergeladenes, das holzi
beim ersten Einsatz selbst prüft —, kann trotzdem normal chatten; holzi bietet diesem
Modell keine Werkzeuge an und sagt dem Nutzer knapp, warum das Bedienen von
holzi im Chat mit diesem Modell nicht geht und welche Modelle es können.

**Why this priority**: Verhindert kaputte oder verwirrende Antworten
(Modell „behauptet“, etwas getan zu haben), wenn ein Modell Werkzeuge nicht
beherrscht. Muss vor einer breiten Nutzung klar sein, hängt aber von den
Werkzeugen aus US1/US2 ab.

**Independent Test**: Ein Modell mit Wert „nicht unterstützt“ wählen, eine
Bedienfrage stellen: keine Werkzeuge werden angeboten, der Chat antwortet
normal, und ein Hinweis erklärt die Einschränkung. Dasselbe mit „unterstützt“:
Werkzeuge werden angeboten.

**Acceptance Scenarios**:

1. **Given** ein Modell mit „Werkzeugnutzung: unterstützt“, **When** der
   Nutzer eine Bedienfrage stellt, **Then** werden Werkzeuge angeboten.
2. **Given** ein Modell mit „Werkzeugnutzung: nicht unterstützt“, **When**
   der Nutzer chattet, **Then** werden keine Werkzeuge angeboten und der
   Nutzer erhält einen knappen Hinweis, dass dieses Modell holzi nicht
   bedienen kann.
3. **Given** ein Modell, dessen Werkzeugunterstützung noch nicht bekannt ist
   (z. B. frisch heruntergeladen), **When** der Nutzer chattet, **Then**
   bietet holzi Werkzeuge an und weist den Nutzer knapp darauf hin, dass die
   Bedienung mit diesem Modell unzuverlässig sein kann.
4. **Given** das Modell wird im laufenden Chat gewechselt, **When** die
   nächste Antwort beginnt, **Then** gilt der Wert des neu gewählten
   Modells.
5. **Given** das Modell hat Werkzeugunterstützung, **When** die Verbindung
   zum Modell-Anbieter die Fähigkeit selbst meldet bzw. widerlegt (Anbieter
   sagt „keine Werkzeuge“), **Then** hat die Angabe des Anbieters Vorrang vor
   lokal Hinterlegtem.
6. **Given** ein lokales Modell, dessen Vorlage keine Werkzeuge vorsieht,
   **When** es zum ersten Mal für den Chat gewählt wird, **Then** erkennt
   holzi das ohne Test und führt das Modell als „nicht unterstützt“.
7. **Given** ein lokales Modell mit unbekannter Werkzeugunterstützung,
   dessen Vorlage Werkzeuge grundsätzlich vorsieht, **When** es zum ersten
   Mal für den Chat gewählt wird, **Then** startet holzi im Hintergrund einen
   kurzen Selbsttest mit wenigen Sätzen aus dem Beispielsatz-Satz (US5)
   ohne dass eine Aktion ausgeführt wird; der Chat ist währenddessen sofort nutzbar (mit dem
   Hinweis aus Szenario 3), und nach Abschluss wechselt der Wert auf
   „unterstützt“ oder „nicht unterstützt“, ohne dass der Nutzer etwas tun
   muss.
8. **Given** ein Modell, dessen Selbsttest bereits gelaufen ist, **When** es
   erneut gewählt wird, **Then** wird der gespeicherte Wert genutzt und kein
   neuer Test gestartet; ändert sich das Modell (neue Datei bzw. Version)
   oder der Beispielsatz-Satz grundlegend, wird der Test einmal neu
   durchgeführt.
9. **Given** der Nutzer hat einen CLI-Delegate (Claude Code oder Codex) als
   Modell im Chat gewählt, **When** er eine Bitte zur Bedienung von holzi
   stellt, **Then** bietet holzi diesem Delegate keine Werkzeuge an und
   zeigt einmalig je Unterhaltung den knappen Hinweis, dass dieses Modell
   mit eigenen Werkzeugen arbeitet und holzi noch nicht bedienen kann; der
   Chat mit dem Delegate funktioniert ansonsten unverändert (Freigabe und
   Verlauf wie in den Specs 007/009).

---

### User Story 5 - Zuverlässigkeit je Modell messen (Priority: P3)

Wer holzi pflegt (oder ein Modell für die Empfehlungsliste prüft), führt einen
kleinen, festen Satz von Beispielsätzen gegen ein gewähltes Modell aus und
erhält eine Auswertung, wie zuverlässig das Modell daraus die richtigen
Aktionen mit gültigen Eingaben macht. Damit lässt sich belegen, welche
Modelle empfohlen werden und welche Werkzeugunterstützung sie im
Fähigkeiten-Eintrag bekommen, statt es zu raten.

**Why this priority**: Nicht für Endnutzer sichtbar, aber die Grundlage, um
Empfehlungen (welche lokalen Modelle taugen) und Schwellen ehrlich zu
setzen und Rückschritte bei Änderungen zu bemerken.

**Independent Test**: Den Satz gegen zwei verschiedene Modelle laufen lassen;
beide Läufe liefern eine vergleichbare Auswertung, und derselbe Lauf zweimal
hintereinander zeigt nur geringe Abweichung.

**Acceptance Scenarios**:

1. **Given** ein Satz von Beispielsätzen mit je erwarteter Aktion und
   erwarteten Eingaben, **When** er gegen ein Modell läuft, **Then** liefert
   die Auswertung je Satz, ob die richtige Aktion gewählt wurde und die
   Eingaben gültig und richtig sind, sowie eine Gesamtquote.
2. **Given** Sätze, bei denen **keine** Aktion passend ist (Smalltalk,
   Wissensfragen), **When** sie laufen, **Then** wird gewertet, ob das
   Modell fälschlich Werkzeuge ruft.
3. **Given** ein Lauf, **When** er fertig ist, **Then** verändert er keine
   echten Nutzerdaten und keine echten Einstellungen.
4. **Given** zwei Modelle, **When** beide den Satz durchlaufen, **Then** sind
   die Auswertungen direkt vergleichbar (gleiche Sätze, gleiche Kennzahlen).
5. **Given** der Satz enthält deutsche und englische Formulierungen, **When**
   er läuft, **Then** weist die Auswertung die Quote je Sprache getrennt
   aus.

---

### Edge Cases

- Das Modell nennt eine Aktion, die es nicht gibt, oder gibt Eingaben, die
  nicht zum Schema passen: Fehlerrückgabe mit Feldhinweis; das Modell darf
  innerhalb der bestehenden Rundengrenze korrigieren.
- Das Modell erreicht die Rundengrenze, ohne fertig zu werden: die Antwort
  endet mit der bestehenden klaren Fehlermeldung (003), bereits ausgeführte
  Aktionen bleiben im Verlauf sichtbar.
- Der Nutzer bricht eine Antwort mitten in einer laufenden Aktion ab: es gilt
  das bestehende Abbruchverhalten (003); eine bereits abgeschlossene Änderung
  wird nicht stillschweigend zurückgenommen, bleibt aber im Verlauf sichtbar.
- Eine Aktion gehört zu einer App, die gerade nicht offen ist: die Aktion
  öffnet die App zuerst (wie bei Bedienung durch den Nutzer).
- Ein Tab, auf den sich eine Bitte bezieht, wird zwischen Anfrage und
  Ausführung geschlossen: die Aktion meldet „Ziel nicht gefunden“, das Modell
  erklärt es.
- Ein Lesewert enthält etwas Geheimes (Zugangsdaten, Schlüssel): solche Werte
  werden nie an ein Modell geliefert, weder lokal noch an einen Cloud-Anbieter
  (ergibt sich schon daraus, dass sie Leitplanken sind; wird hier ausdrücklich
  abgesichert).
- Ein Cloud-Modell bekommt Werte aus Lese-Aktionen zu sehen: sie verlassen
  damit das Gerät wie jede andere Chat-Eingabe; das gilt bewusst und bleibt
  dem Nutzer durch die Wahl des Modells überlassen, ohne zusätzlichen
  Hinweis (Klärung 2026-09-30, siehe Annahmen).
- Die Aktionsliste ändert sich während eines Chats (App wird geöffnet oder
  geschlossen, später haextensions): das nächste Angebot nutzt den aktuellen
  Stand.
- Mehrere Fenster oder Sitzungen sind offen: eine Aktion wirkt nur auf das
  ausdrücklich genannte Ziel, nie auf „irgendein“ Fenster.
- Kein werkzeugfähiges Modell installiert oder verbunden: der Chat bleibt
  nutzbar; der Hinweis aus US4 nennt, wie man ein passendes Modell bekommt.

## Requirements _(mandatory)_

### Functional Requirements

**Aktionen als Werkzeuge**

- **FR-001**: Das System MUSS jede Aktion, die für Agenten aufrufbar und für
  den eingebauten Agenten freigegeben ist (`builtinAgentCallable !== false`),
  dem eingebauten Agenten im Chat als Werkzeug anbieten können;
  Name, Beschreibung und Eingabeschema stammen aus der Aktionsdefinition, es
  gibt keine zweite, parallel gepflegte Werkzeugbeschreibung. Ausgenommen
  sind Aktionen, die sich im laufenden Chat selbst auslösen würden
  (Nachricht senden, Antwort wiederholen, Antwort abbrechen); sie bleiben
  für externe Agenten (Spec 021) aufrufbar, dem eingebauten Agenten aber
  verschlossen.
- **FR-002**: Wird ein solches Werkzeug aufgerufen, MUSS das System die
  Aktion über denselben Ablauf ausführen wie bei Bedienung durch den Nutzer
  (Prüfung der Eingaben, Zielauflösung, Öffnen der zugehörigen App), mit
  Aufrufer „eingebauter Agent“, und Ergebnis oder Fehler an das Modell
  zurückgeben.
- **FR-003**: Aktionen, die nicht für Agenten aufrufbar sind (alle im
  Bereich Leitplanken), MÜSSEN dem Modell weder angeboten noch — falls es sie
  dennoch anfordert — ausgeführt werden; der Aufruf MUSS mit einer
  eindeutigen Fehlermeldung abgelehnt werden, in jedem Freigabe-Modus und
  unabhängig von einer Zustimmung des Nutzers.
- **FR-004**: Bei Aktionen mit Ziel (Tab, Fenster, Arbeitsbereich) MUSS das
  Modell das Ziel ausdrücklich angeben; dafür MUSS es die offenen Tabs,
  Fenster und Arbeitsbereiche über lesende Aktionen ermitteln können.
- **FR-005**: Jede vom Modell ausgelöste Aktion MUSS im Chat-Verlauf mit
  Name, Eingaben und Ergebnis sichtbar sein; der Verlauf MUSS auch nach dem
  Neuöffnen derselben Unterhaltung erhalten bleiben (wie heutige
  Werkzeug-Zeilen).
- **FR-006**: Fehler einer Aktion MÜSSEN dem Modell in einer Form
  zurückgegeben werden, aus der es Ursache und betroffene Eingabe erkennen
  kann, ohne interne Fehlerdetails (Stack, Pfade) preiszugeben.

**Freigabe**

- **FR-007**: Die Freigabe-Modi MÜSSEN auf die Wirkungsart der Aktion
  abgebildet werden:
  - Modus „Manuell“: jede Aktion, auch lesende, erfordert vorherige
    Zustimmung des Nutzers.
  - Modus „Plan“: lesende Aktionen laufen ohne Rückfrage; ändernde und
    zerstörende werden ohne Rückfrage abgelehnt.
  - Modus „Auto“: lesende und ändernde Aktionen laufen ohne Rückfrage;
    zerstörende Aktionen erfordern vorherige Zustimmung des Nutzers. Damit
    unterscheidet die Freigabe drei Wirkungsarten, nicht mehr nur „sicher“
    und „riskant“ (Änderung an Spec 003, siehe Annahmen).
- **FR-008**: Die Zustimmungsabfrage MUSS den Nutzer in verständlicher
  Sprache informieren, was geschehen soll (Aktion in Klartext, betroffenes
  Ziel, Eingaben), nicht nur technische Namen.
- **FR-009**: Lehnt der Nutzer ab oder lehnt Modus „Plan“ ab, MUSS das Modell
  die Ablehnung als Ergebnis erhalten; die Ablehnung MUSS sich im Verlauf
  niederschlagen.
- **FR-010**: Der Freigabe-Modus, Sperrregeln, Zugangsdaten von Anbietern und
  Berechtigungen von Agenten MÜSSEN für das Modell unveränderbar und
  möglichst auch nicht lesbar sein; Zugangsdaten und Schlüssel DÜRFEN nie im
  Ergebnis einer Aktion an ein Modell gelangen.

**Werkzeug-Angebot pro Schritt**

- **FR-011**: Das System MUSS in jedem Schritt einer Antwort ein begrenztes
  Werkzeug-Angebot machen: ein fest definiertes Kernangebot der häufigsten
  Aktionen (höchstens 10 Werkzeuge, einschließlich des Suchwerkzeugs) plus
  höchstens 5 zuvor gefundene. Das Kernangebot ist für alle Modelle gleich
  (lokal wie Cloud), hängt nicht vom Text der Nutzernachricht ab, und die
  Obergrenze gilt einheitlich ohne Ausnahme je Modell.
- **FR-012**: Das System MUSS ein Suchwerkzeug anbieten, mit dem das Modell
  unter allen für Agenten aufrufbaren Aktionen nach einem Suchbegriff suchen
  kann (höchstens 5 Treffer mit Name, Beschreibung und Eingabeschema). Gefundene
  Aktionen MÜSSEN dem Modell ab dem nächsten Schritt derselben Antwort als
  Werkzeuge zur Verfügung stehen, bei lokalen und Cloud-Modellen auf demselben
  Weg; eine spätere Suche ersetzt die früheren Treffer, damit die Obergrenze
  eingehalten bleibt.
- **FR-013**: Das Angebot MUSS ohne Zutun des Nutzers entstehen und darf die
  Erreichbarkeit einer Aktion nicht einschränken: jede für Agenten aufrufbare
  Aktion ist über die Suche erreichbar.

**Fähigkeit „Werkzeugnutzung“**

- **FR-014**: Der Fähigkeiten-Eintrag je Modell MUSS einen Wert
  „Werkzeugnutzung“ führen: unterstützt, nicht unterstützt oder unbekannt.
- **FR-015**: Der Wert MUSS gesetzt werden aus (in dieser Rangfolge): einer
  Angabe des Modell-Anbieters, sofern vorhanden; dem Ergebnis der
  vollständigen Modellprüfung (FR-019 bis FR-022) bzw. der gepflegten
  Empfehlungsliste für lokale Modelle; der Vorlagenprüfung (FR-018a); dem
  Ergebnis des Selbsttests (FR-018b); sonst „unbekannt“.
- **FR-016**: Für ein Modell „nicht unterstützt“ MUSS das System keine
  Werkzeuge anbieten, den Chat aber uneingeschränkt als normalen Chat
  betreiben und einmalig je Unterhaltung kurz erklären, warum holzi damit
  nicht bedient werden kann.
- **FR-017**: Für „unbekannt“ MUSS das System Werkzeuge anbieten und den
  Nutzer einmalig je Unterhaltung knapp darauf hinweisen, dass die Bedienung
  mit diesem Modell unzuverlässig sein kann.
- **FR-018**: Der Wert MUSS pro Modell gespeichert bleiben, nicht pro Antwort
  neu ermittelt werden, und beim Modellwechsel sofort gelten.
- **FR-018a**: Bei einem lokalen Modell MUSS das System beim ersten Einsatz
  prüfen, ob dessen Vorlage Werkzeuge überhaupt vorsieht. Sieht sie keine
  vor, MUSS der Wert „nicht unterstützt“ gesetzt werden. Sieht sie welche
  vor, folgt daraus allein noch nicht „unterstützt“.
- **FR-018b**: Bei einem lokalen Modell mit Wert „unbekannt“, dessen Vorlage
  Werkzeuge vorsieht, MUSS das System einmalig einen kurzen Selbsttest mit
  wenigen Sätzen des Beispielsatz-Satzes im Hintergrund durchführen
  (es wird keine Aktion ausgeführt, derselbe Bewertungsablauf wie FR-020).
  Der Chat MUSS währenddessen ohne Wartezeit nutzbar bleiben. Nach dem Test
  MUSS der Wert auf „unterstützt“ oder „nicht unterstützt“ wechseln; der
  Selbsttest MUSS für Modelle mit Anbieter-Angabe oder Empfehlungsliste
  entfallen.

**Modellprüfung (Eval)**

- **FR-019**: Das System MUSS einen festen, versionierten Satz von
  Beispielsätzen bereitstellen, der je Satz erwartete Aktion(en) und
  erwartete Eingaben bzw. „keine Aktion erwartet“ festhält, in Deutsch und
  Englisch, abgedeckt über lesen/navigieren, ändern und Smalltalk.
- **FR-020**: Ein Lauf gegen ein gewähltes Modell MUSS je Satz bewerten:
  richtige Aktion gewählt, Eingaben gültig und korrekt, keine unnötigen
  Aufrufe; und eine Gesamtquote sowie Quoten je Sprache und Art ausweisen.
- **FR-021**: Ein Lauf DARF keine echten Nutzerdaten oder Einstellungen
  verändern (es wird keine Aktion ausgeführt, nur der Aufruf bewertet), MUSS mit festen
  Einstellungen (keine Zufallsstreuung, soweit das Modell es zulässt)
  ausführbar sein und MUSS wiederholbar vergleichbare Ergebnisse liefern.
- **FR-022**: Die Prüfung MUSS ohne Zugriff auf einen Tresor des Nutzers
  ausführbar sein, damit sie in automatisierten Läufen und bei der Pflege
  der Empfehlungsliste nutzbar ist; sie ist ein Werkzeug für Pflegende,
  keine Nutzeroberfläche für Endnutzer.

**CLI-Delegates**

- **FR-023**: Ist ein CLI-Delegate (Claude Code, Codex) als Modell gewählt,
  MUSS das System ihm keine holzi-Werkzeuge anbieten, den bestehenden
  Ablauf der Specs 007/009 unverändert lassen und den Nutzer einmalig je
  Unterhaltung knapp darauf hinweisen, dass dieses Modell holzi noch nicht
  bedienen kann. Für Delegates entfällt die Fähigkeit „Werkzeugnutzung“
  (kein Selbsttest, kein Wert „unbekannt“, kein Unzuverlässigkeits-Hinweis
  nach FR-017).
- **FR-024**: Die Werkzeug-Definitionen MÜSSEN so angelegt sein, dass sie
  ohne Umbau in Spec 021 über MCP an Delegates und andere externe Agenten
  ausgeliefert werden können (siehe Annahmen); diese Spec baut diesen
  Zugang nicht.

### Key Entities

- **Aktion**: Benannte Bedienung von holzi aus Spec 020 mit Eingabeschema,
  Wirkungsart (lesen / ändern / zerstörend), Bereich und Kennzeichen „durch
  Agenten aufrufbar“. Hier Quelle der Werkzeuge des Modells.
- **Werkzeug-Angebot**: Die Aktionen, die einem Modell in einem Schritt
  gezeigt werden: ein festes Kernangebot (höchstens 10, einschließlich
  Suchwerkzeug), für alle Modelle gleich, plus bis zu 5 zuvor vom Modell
  gefundene Aktionen.
- **Werkzeugnutzung (Fähigkeit)**: Wert im Fähigkeiten-Eintrag eines
  Modells — unterstützt, nicht unterstützt, unbekannt —, bestimmt, ob ein
  Werkzeug-Angebot gemacht wird.
- **Beispielsatz**: Eine Nutzeräußerung mit Sprache, Art (lesen / ändern /
  Smalltalk), erwarteter Aktion samt Eingaben oder „keine“.
- **Prüfergebnis**: Auswertung eines Laufs für ein Modell: Quote gesamt,
  je Sprache, je Art, Liste der Fehltreffer; verknüpft mit Modell und
  Version des Beispielsatz-Satzes.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Bei den einfachen Einzelschritt-Sätzen des Beispielsatz-Satzes
  (lesen, navigieren, eine Änderung) wählt ein unterstütztes Cloud-Modell in
  mindestens 95 % der Sätze die richtige Aktion mit korrekten Eingaben, ein
  als empfohlen geführtes lokales Modell in mindestens 80 %. Die Werte sind
  Anfangsziele und werden nach dem ersten Messlauf bestätigt oder
  angepasst.
- **SC-002**: Ein Modell, das im Messlauf unter 50 % der Einzelschritt-Sätze
  erreicht, wird nicht als „Werkzeugnutzung: unterstützt“ geführt.
- **SC-002a**: Der Selbsttest (FR-018b) blockiert den Chat nie; er ist
  nach spätestens zwei Minuten auf einem Rechner ohne Grafikbeschleunigung
  (nur Prozessor) mit dem Standard-Modell Qwen3-4B abgeschlossen,
  und sein Ergebnis stimmt bei mindestens 90 % der vollständig gemessenen
  Modelle mit dem Ergebnis des vollständigen Messlaufs überein.
- **SC-003**: 100 % der vom Modell ausgelösten ändernden und zerstörenden
  Aktionen folgen der Freigabe-Regel des jeweils gewählten Modus; in 100 %
  der Prüfungen mit Leitplanken-Aktionen wird der Aufruf durch das Modell
  abgelehnt, in jedem Modus.
- **SC-004**: Kein Ergebnis einer vom Modell ausgelösten Aktion enthält
  Zugangsdaten oder Schlüssel (geprüft über alle für Agenten aufrufbaren
  Lese-Aktionen). Für `chat.messages.list` bestätigt ein Test mit einer
  geheimnisbehafteten Nachricht zusätzlich, dass das Agent-Ergebnis weder
  `content` noch den geheimen Wert enthält.
- **SC-005**: Auch bei einer größeren Aktionsliste werden in keinem Schritt
  mehr als 15 Aktionswerkzeuge angeboten (Kernangebot höchstens 10 plus
  höchstens 5 eindeutige Nicht-Kern-Treffer). `run_command` und
  MCP-Werkzeuge bleiben zusätzlich erhalten und zählen nicht in dieses
  Aktionslimit. Das Kernangebot ist für alle Modelle identisch, und für
  mindestens 95 % der Beispielsätze steht das passende Werkzeug spätestens im
  zweiten Schritt zur Verfügung (im Kernangebot oder nach einer Suche), mit
  höchstens einem zusätzlichen Schritt.
- **SC-006**: Mit einem Modell „Werkzeugnutzung: nicht unterstützt“ oder
  mit einem CLI-Delegate werden in 100 % der Antworten keine holzi-Werkzeuge
  angeboten, und der Chat antwortet weiterhin normal; bei einem Delegate
  erscheint der Hinweis aus FR-023 genau einmal je Unterhaltung.
- **SC-007**: Der Nutzer sieht in 100 % der Fälle im Verlauf, welche
  Aktionen das Modell ausgelöst hat, samt Ergebnis oder Ablehnung.
- **SC-008**: Zwei aufeinanderfolgende Messläufe mit demselben Modell und
  denselben Einstellungen weichen in der Gesamtquote um höchstens 10
  Prozentpunkte voneinander ab.
- **SC-009**: Ein Nutzer ohne Vorwissen kann mit einem empfohlenen lokalen
  Modell die drei Beispiele aus US1 („Sync-Einstellungen öffnen“, „Welche
  Tabs sind offen?“, „Welches Farbschema?“) im ersten Versuch durch freie
  Formulierung erreichen.

## Assumptions

- Der eingebaute Agent ist der Chat von holzi, der bereits Werkzeuge
  aufrufen kann (Spec 003). Externe Agenten (Claude Code, Codex als
  Delegate; MCP) sind nicht Teil dieser Spec; ihre Anbindung folgt mit
  Spec 021.
- Die Aktionen und ihre Kennzeichnung (Wirkungsart, „durch Agenten
  aufrufbar“, Bereich Leitplanken gesperrt) aus Spec 020/023 sind die
  maßgebliche Quelle; diese Spec ändert daran nichts außer dem Aufrufer
  „eingebauter Agent“ tatsächlich zu nutzen.
- Die Werkzeug-Definition (Name, Beschreibung, Eingabeschema) ist dieselbe,
  die Spec 021 später als MCP-Werkzeug für externe Agenten veröffentlicht.
  Der Chat selbst geht dafür nicht über das MCP-Protokoll, sondern ruft die
  Aktionen im Prozess auf; damit kann 021 ohne Umbau darauf aufsetzen. Das
  Werkzeug-Angebot (FR-011) und die Fähigkeit „Werkzeugnutzung“ gelten
  für Modelle, bei denen holzi den Werkzeug-Ablauf selbst fährt: lokale
  Modelle und Anbieter mit API-Key (z. B. Claude per Schlüssel). Externe
  Agenten bringen ihr Modell mit und erhalten in 021 die vollständige Liste
  gemäß ihren Berechtigungen.
- CLI-Delegates (Claude Code, Codex; Specs 007/009) bleiben erhalten. Ihr
  Wert liegt darin, ein vorhandenes Abo im holzi-Chat ohne eigenen API-Key
  zu nutzen; das ist mit reinem MCP-Zugang von außen nicht gegeben. Sobald
  Spec 021 holzi als MCP-Server bereitstellt, sollen Delegates damit holzi
  im Chatfenster bedienen können — das ist der Grund, sie zu behalten und
  nicht zu entfernen. Ob sie langfristig bleiben, wird nach 021 anhand der
  Nutzung entschieden, nicht in dieser Spec.
- Es gibt keinen eigenen Schalter, der das Bedienen von holzi durch ein
  Modell abschaltet (Klärung 2026-09-30). Wer es nicht will, nutzt den
  Modus „Manuell“ (Standard, fragt bei allem nach); ein Schalter kann später
  ergänzt werden, wenn er gebraucht wird.
- Die Mindestquote des Selbsttests (wie viele der wenigen Sätze bestanden
  sein müssen) wird aus dem ersten vollständigen Messlauf abgeleitet; ein
  Selbsttest ist bewusst klein und darf bei Grenzfällen mit „nicht
  unterstützt“ entscheiden (der Nutzer kann das Modell weiterhin normal
  nutzen).
- Die vorhandene Rundengrenze, Wiederholungsregeln und der Abbruch aus
  Spec 003 gelten unverändert. Geändert wird nur die Freigabe: Sie
  unterscheidet künftig lesend, ändernd und zerstörend (bisher sicher und
  riskant), damit „Auto“ gewöhnliche Änderungen ohne Rückfrage zulässt, „Plan“
  aber weiterhin jede Änderung blockiert. Werkzeuge ohne Wirkungsart aus
  Spec 003 (Befehl auf dem Rechner, MCP-Werkzeuge) bleiben wie bisher
  riskant, also zerstörend eingestuft.
- Werkzeuge aus haextensions (017–019) kommen später als weitere Aktionen
  hinzu und sollen dann im Werkzeug-Angebot ohne Änderung der
  Freigabe-/Filterlogik erscheinen. Kalender- und Einkaufslisten-Beispiele
  („Was steht heute im Kalender?“, „Trag Milch ein“) sind deshalb
  ausdrücklich erst mit diesen Specs erreichbar, nicht mit dieser.
- Wer ein Cloud-Modell wählt, akzeptiert, dass Chat-Inhalte — einschließlich
  Ergebnissen von Lese-Aktionen — an den gewählten Anbieter gehen, wie bei
  jeder anderen Chat-Eingabe. Geheimnisse gehören nie dazu (FR-010).
- Das Kernangebot besteht am Anfang aus neun häufigen Aktionen (Tabs und
  Apps öffnen und wechseln, Zustand und Einstellungen lesen, Farbschema
  setzen, Modelle auflisten) plus dem Suchwerkzeug; welche Aktionen dazugehören,
  wird nach dem ersten Messlauf bestätigt oder angepasst. Die Suche arbeitet
  wortbasiert über Name, Beschreibung und die deutschen und englischen Titel
  der Aktionen; das Modell formuliert den Suchbegriff.
- Die Hinweise zu unbekannten und nicht unterstützten Modellen und zu
  Delegates erscheinen einmal je Unterhaltung und Zustand, solange die App
  läuft; nach einem Neustart können sie erneut erscheinen.
- Der Katalog der Empfehlungsliste für lokale Modelle enthält heute nur
  kleine Modelle; ob ein größeres Modell aufgenommen wird, entscheidet sich
  nach dem ersten Messlauf und gehört nicht zu dieser Spec.
- Der Beispielsatz-Satz beginnt klein (Größenordnung zwanzig bis dreißig
  Sätze) und wächst mit den Aktionen; eine Erweiterung ist Teil der Pflege,
  nicht einer neuen Spec.
- Nutzer schreiben im Chat meist Deutsch oder Englisch; weitere Sprachen sind
  nicht ausgeschlossen, aber nicht Ziel der Messung.
