# Feature Specification: Sprachassistent

**Feature Branch**: `066-voice-assistant`

**Created**: 2026-10-28

**Status**: Draft

**Input**: User description: „ich möchte holzi gerne zu einem vollwertigen ki assistent ausbauen, der
vollständig durch sprache gesteuert werden kann. durch einen button im app launcher soll holzi auf
audio assistant geschaltet werden können. dadurch soll holzi immer per wake word aktiviert werden
(der agent). ab da soll der nutzer einfach mit dem agent ganz normal sprechen können, wie mit einem
menschen. der agent nimmt die fragen/anweisungen entgegen und versucht sie zu beantworten bzw zu
bearbeiten und gibt dem nutzer dann ebenfalls per audio rückmeldung dazu. so sollen komplette
gespräche und workflow ablaufen können. der agent soll auch gezielte nachfragen machen und eventuell
vorschläge bringen, auf die der nutzer entsprechend eingehen kann.“ (Betreiber, 2026-10-28)

## Clarifications

### Session 2026-10-28

- Q: Wann hört haex im Assistent-Modus auf das Wake Word? → A: Solange haex läuft und der Vault
  entsperrt ist. Auf dem Desktop auch minimiert oder verdeckt, auf Android über einen
  Vordergrund-Dienst mit dauerhafter Benachrichtigung, auf iOS nur bei sichtbarer App (Grenze der
  Plattform).
- Q: Welches Wake Word gibt es? → A: Frei wählbar: Der Nutzer spricht ein eigenes Wort ein, haex lernt
  es lokal. Ein mitgeliefertes Standardwort funktioniert ohne Einsprechen.
- Q: Wie werden Freigaben und Geheimnisse im Gespräch behandelt? → A: Alles geht per Sprache.
  Ändernde und zerstörende Aktionen werden mündlich bestätigt; Geheimnisse (etwa Passwörter) liest
  haex nach einer mündlichen Bestätigung pro Abruf laut vor.
- Q: Woher kommt die Sprachausgabe? → A: Standard ist eine lokale Stimme, ausgewählt wie das lokale
  Spracherkennungsmodell in 010. Optional lässt sich ein externer Sprachdienst einrichten; solange er
  aktiv ist, zeigt haex dauerhaft an, dass Text das Gerät verlässt.
- Q: Darf beliebige Sprache haex unterbrechen, während es über Lautsprecher spricht? → A: Ja, das ist
  das Ziel, sofern die Echo-Unterdrückung zuverlässig ist. Wo sie SC-005 auf einer Plattform nicht
  erfüllt, unterbrechen dort über Lautsprecher nur Wake Word und Stopp-Wörter; mit Kopfhörern
  unterbricht immer jede Sprache.
- Q: Welches Standard-Wake-Word wird mitgeliefert? → A: „Haex“ und „Hey Haex“, beide lösen aus. Das
  Erkennungsmodell wird für genau diese Wörter trainiert. Zusätzlich kann der Nutzer ein eigenes Wort
  festlegen, das er dafür mehrmals einspricht (US7).
- Q: Was passiert mit einer laufenden Aufgabe, wenn das Gespräch endet? → A: Beendet der Nutzer das
  Gespräch aktiv, wird auch die Aufgabe abgebrochen. Solange der Agent arbeitet, endet das Gespräch
  nicht durch Stille; haex meldet sich von selbst, wenn die Aufgabe fertig ist oder es eine Rückfrage
  oder Freigabe braucht („erledige das und gib mir Bescheid“).
- Q: Wie ist der Assistent sichtbar, während das haex-Fenster offen ist? → A: Als kleine schwebende,
  verschiebbare Anzeige über allen Fenstern und Tabs, mit einer Darstellung pro Zustand (etwa das
  haex-Eichhörnchen). Ein Klick klappt eine kompakte Live-Mitschrift mit den Bedienelementen auf.
- Q: Wie schnell muss haex mit einem lokalen Modell antworten? → A: Mit einem Cloud-Modell beginnt die
  Antwort auf eine einfache Frage innerhalb von 3 Sekunden, mit einem lokalen Modell auf dem
  Referenz-Desktop innerhalb von 6 Sekunden; bis dahin überbrückt die sofortige Quittung.
- Betreiber-Ergänzung: haex braucht passende Töne, mindestens für das erkannte Wake Word und für eine
  entgegengenommene und bearbeitete Aufgabe oder Frage; die übrigen Anlässe ergänzt die Spec (FR-043).
- Betreiber-Ergänzung: Alle Gespräche werden vollständig transkribiert und in der Vault-Datenbank
  gespeichert, damit man später alles nachlesen und nachvollziehen kann (FR-047, FR-048). Die
  Meeting-Mitschrift mit Sprecherzuordnung ist eine eigene Spec (067).
- Betreiber-Ergänzung: haex liefert einige Stimmen mit; Nutzer können weitere Stimmen selbst
  einrichten (FR-049 bis FR-053).
- Q: Wie richten Nutzer weitere Stimmen ein? → A: Auf drei Wegen: eine Stimme aus einer eigenen
  Aufnahme oder Audiodatei nachbilden (mit bestätigter Einwilligung der sprechenden Person),
  Sprachmodell-Dateien importieren, oder einen externen Sprachdienst bzw. eigenen Server anbinden.
- Q: In welchen Sprachen spricht haex? → A: In sechs Sprachen: Deutsch, Englisch, Französisch,
  Spanisch, Italienisch und Portugiesisch, mit mitgelieferten Stimmen für jede.
- Q: Gelten selbst angelegte Stimmen auf allen Geräten? → A: Ja. Nachgebildete Stimmen werden
  verschlüsselt über den Vault synchronisiert; importierte Modelldateien bleiben wegen ihrer Größe auf
  dem Gerät.

## Begriffe

- **Assistent-Modus**: Gerätebezogener Zustand, in dem haex auf das Wake Word hört. Er wird im
  Launcher ein- und ausgeschaltet.
- **Kurze Ansage**: höchstens ein Satz mit höchstens 12 gesprochenen Wörtern und höchstens
  4 Sekunden Dauer. Muss eine Meldung mehr Inhalt tragen, verweist haex auf die Mitschrift.
- **Wake Word**: Das Wort oder die kurze Wendung, die ein Gespräch beginnt. Erkannt wird es nur auf
  dem Gerät.
- **Gespräch**: Eine Folge von Runden zwischen Nutzer und eingebautem Agenten. Es beginnt mit dem
  Wake Word (oder per Klick bzw. Tastenkürzel) und endet, wenn der Nutzer es aktiv beendet oder nach
  Stille, während keine Aufgabe läuft. Ein Gespräch ist eine normale Chat-Unterhaltung (006) und
  erscheint im Chatverlauf.
- **Runde**: Der Nutzer spricht, haex erkennt das Ende des Satzes, der Agent arbeitet und antwortet
  gesprochen.
- **Nachhörzeit**: Die Zeitspanne nach einer Antwort, in der haex ohne erneutes Wake Word
  weiter zuhört.
- **Aktiv beenden**: Schlusswendung, „Gespräch beenden“ in der Assistent-Anzeige, Ausschalten des
  Modus oder Sperren des Vaults. Bricht eine laufende Aufgabe des Gesprächs ab.
- **Offene Frage**: Eine Rückfrage oder Freigabe des Agenten, auf die noch keine Antwort kam. Der
  Agent wartet; ausgeführt wird bis zur Antwort nichts.
- **Dazwischenreden**: Der Nutzer spricht, während haex noch redet; haex verstummt und hört zu.
- **Assistent-Anzeige**: Die stets sichtbare Anzeige des Modus und seines Zustands (bereit, hört
  zu, denkt nach, arbeitet, spricht, wartet auf Antwort, stumm, pausiert). Im offenen Fenster ist sie
  ein kleines schwebendes Element über allen Fenstern und Tabs, das sich zur **kompakten Mitschrift**
  aufklappen lässt; außerhalb des Fensters ein Symbol im Systembereich (Desktop) bzw. eine
  Benachrichtigung (Android).
- **Stimme**: Die Quelle der Sprachausgabe: eine lokale Stimme oder ein externer Sprachdienst.
- **Mündliche Bestätigung**: Ein klares „Ja“ (oder eine gleichwertige Zustimmung) des Nutzers, nachdem
  haex die anstehende Wirkung laut wiederholt hat.

## Beziehung zu bestehenden Specs

- [`003-agent-tool-loop`](../003-agent-tool-loop/spec.md) und
  [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md): Der Assistent-Modus ist eine
  **Sprachschicht** über dem vorhandenen eingebauten Agenten. Werkzeug-Ablauf, Freigabestufen
  (Manuell / Auto / Plan), Rundengrenze, Abbruch und die Sperre der Leitplanken für Agenten bleiben
  unverändert. Es entsteht kein neuer Agent und keine eigene Planungs- oder Autonomie-Logik. Neu sind
  nur Hören, Sprechen, die Gesprächsführung und eine Anweisung an das Modell für gesprochene
  Gespräche.
- Spec 068 (Task Orchestration): Sobald sie umgesetzt ist, spricht der Assistent mit dem
  Orchestrator statt direkt mit dem eingebauten Agenten. Aufgaben im Hintergrund,
  offene Fragen und aktives Beenden (FR-039, FR-040) gelten dann pro Aufgabe aus 068; Quittung und
  Fortschrittsansage kommen vom Orchestrator. Bis dahin gilt FR-020 wie beschrieben.
- [`008-voice-control-stt`](../008-voice-control-stt/spec.md) und
  [`010-stt-model-choice`](../010-stt-model-choice/spec.md): liefern die Spracherkennung und ihre
  Modellwahl. Diese Spec hebt für den Assistent-Modus die Annahmen „kein Wake Word, kein
  Dauerzuhören“ und „nur im Chatfenster“ (008 FR-018, FR-019) auf. Die Stopp-Wörter aus 008 US2
  sind dort noch nicht umgesetzt; der Assistent-Modus braucht sie (FR-017) und setzt sie mit um.
  008 FR-020 (Audio nie speichern) gilt weiter; die einzige Ausnahme ist das Einlernen des Wake Words
  (FR-008).
- [`006-chat-history-management`](../006-chat-history-management/spec.md): Jedes Gespräch ist eine
  Chat-Unterhaltung mit Mitschrift und lässt sich im Chat schriftlich fortsetzen.
- [`034-password-manager`](../034-password-manager/spec.md) und
  [`021-mcp-server`](../021-mcp-server/spec.md): Der eingebaute Agent bekommt nie Geheimnisse. Daran
  ändert der Assistent-Modus nichts: haex spricht ein Geheimnis selbst aus, das Modell sieht es nicht
  (FR-024).
- [`045-dock`](../045-dock/spec.md) und [`015-workspace-shell`](../015-workspace-shell/spec.md): Der
  Schalter sitzt im Launcher; als anheftbarer Eintrag kann er auch ins Dock.
- [`013-vault-lifecycle-isolation`](../013-vault-lifecycle-isolation/spec.md): Hören gibt es nur mit
  entsperrtem Vault; Sperren oder Schließen beendet es.
- [`043-android-build`](../043-android-build/spec.md): Android bekommt einen Vordergrund-Dienst für
  das Hören im Hintergrund.
- Settings-Kategorie „KI“ (`/ai`): Die Einstellungen des Assistent-Modus liegen dort und werden bei
  Auswahl gespeichert, ohne Speichern-Knopf.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Modus einschalten und per Wake Word fragen (Priority: P1)

Anna öffnet den Launcher und tippt auf „Sprachassistent“. haex zeigt die Assistent-Anzeige als
„bereit“. Sie minimiert haex und arbeitet weiter. Später sagt sie „Hey Haex, wie viele ungelesene
Mails habe ich?“. Ein kurzer Ton bestätigt, dass haex zuhört. Nach ihrer Frage antwortet haex
gesprochen: „Du hast drei ungelesene Mails, zwei davon von Jonas.“ Danach hört haex noch einige
Sekunden zu. Als Anna nichts mehr sagt, kehrt es in den Zustand „bereit“ zurück.

**Why this priority**: Das ist der Kern: ein Gespräch ohne Hände, das mit einem Zuruf beginnt und
gesprochen beantwortet wird. Ohne diese Story gibt es kein Produkt.

**Independent Test**: Modus einschalten, haex minimieren, das Standard-Wake-Word und eine Frage
sprechen, die der Agent ohne Werkzeug beantworten kann. Die Antwort kommt gesprochen, die
Unterhaltung steht im Chatverlauf und die Anzeige kehrt nach der Nachhörzeit zu „bereit“ zurück.

**Acceptance Scenarios**:

1. **Given** der Modus ist aus, **When** der Nutzer im Launcher „Sprachassistent“ wählt, **Then**
   schaltet sich der Modus ein, die schwebende Assistent-Anzeige erscheint als „bereit“ und der
   Schalter zeigt den Zustand „an“.
2. **Given** der Modus ist an und haex ist minimiert, **When** der Nutzer das Wake Word spricht,
   **Then** gibt haex einen hörbaren und sichtbaren Hinweis, dass es zuhört, und nimmt die folgende
   Frage auf.
3. **Given** der Nutzer hat seine Frage gesprochen und macht eine Pause, **When** die Pause die
   eingestellte Länge erreicht, **Then** gilt die Frage als beendet, ohne dass der Nutzer etwas
   drückt.
4. **Given** der Agent liefert eine Antwort, **When** sie beginnt, **Then** spricht haex sie, während
   der Rest noch entsteht, und zeigt sie zugleich als Text in der Unterhaltung.
5. **Given** haex hat zu Ende gesprochen, **When** innerhalb der Nachhörzeit niemand spricht,
   **Then** endet das Gespräch und die Anzeige steht wieder auf „bereit“.
6. **Given** der Modus ist an, **When** der Nutzer den Schalter im Launcher oder in der Anzeige
   erneut betätigt, **Then** hört haex sofort auf zuzuhören, ein laufendes Gespräch endet und die
   Anzeige verschwindet.
7. **Given** Sprache ohne Wake Word (Gespräche im Raum, Fernseher), **When** der Modus an ist und
   kein Gespräch läuft, **Then** wird nichts davon erkannt, gespeichert, angezeigt oder an einen
   Dienst geschickt.
8. **Given** der Vault wird gesperrt oder haex beendet, **When** der Modus an war, **Then** hört haex
   sofort auf; nach dem nächsten Entsperren auf diesem Gerät ist der Modus wieder an.
9. **Given** ein Modell, eine Spracherkennung oder eine Stimme fehlt noch, **When** der Nutzer den
   Modus einschaltet, **Then** sagt haex, was fehlt, und bietet den Download an; der Modus schaltet
   sich erst ein, wenn alles bereit ist.
10. **Given** ein Gespräch läuft und das haex-Fenster ist offen, **When** der Nutzer die schwebende
    Anzeige anklickt, **Then** klappt die kompakte Mitschrift mit den letzten Runden auf; „Gespräch
    zeigen“ öffnet die volle Unterhaltung im Chat-Tab.

---

### User Story 2 - Ganz normal weitersprechen (Priority: P1)

Anna fragt nach der Antwort ohne neues Wake Word nach: „Und was will Jonas?“. haex antwortet. Mitten
in einer langen Antwort unterbricht sie: „Okay, reicht.“ haex verstummt sofort und hört zu. Am Ende
sagt sie „Danke, das war's“, und haex beendet das Gespräch.

**Why this priority**: Ein Gespräch „wie mit einem Menschen“ braucht Anschlussfragen ohne Zuruf,
Unterbrechen und ein natürliches Ende. Mit nur einer Frage pro Wake Word wäre der Modus ein
Sprachbefehl, kein Assistent.

**Independent Test**: Eine Frage stellen, in der Nachhörzeit eine Anschlussfrage stellen, die sich
auf die vorige Antwort bezieht („und davon …“), während einer Antwort dazwischenreden und das
Gespräch mit einer Schlusswendung beenden.

**Acceptance Scenarios**:

1. **Given** haex hat geantwortet und die Nachhörzeit läuft, **When** der Nutzer weiterspricht,
   **Then** gilt das als nächste Runde desselben Gesprächs, mit dem bisherigen Verlauf als Kontext.
2. **Given** haex spricht, **When** der Nutzer dazwischenredet, **Then** bricht haex die Ausgabe
   innerhalb kurzer Zeit ab und nimmt das Gesagte als nächste Runde auf.
3. **Given** eine Plattform, auf der die Echo-Unterdrückung SC-005 nicht erfüllt, und Ausgabe über
   Lautsprecher, **When** der Nutzer dazwischenredet, **Then** unterbrechen dort nur Wake Word und
   Stopp-Wörter; das Wake Word stoppt die Ausgabe und haex hört zu.
4. **Given** haex spricht über Lautsprecher, **When** nur seine eigene Stimme im Raum zu hören ist,
   **Then** löst sie weder das Wake Word noch ein Dazwischenreden aus.
5. **Given** der Agent arbeitet noch oder haex spricht, **When** der Nutzer nur „Stopp“, „Halt“ oder
   „Abbrechen“ sagt, **Then** bricht haex die laufende Runde samt Werkzeugaufrufen ab, sagt kurz
   „Abgebrochen“ und hört weiter zu.
6. **Given** ein Gespräch läuft, **When** der Nutzer eine Schlusswendung wie „Danke, das war's“
   sagt, **Then** verabschiedet sich haex kurz und das Gespräch endet.
7. **Given** ein Gespräch endete vor weniger als der eingestellten Fortsetzungszeit, **When** der
   Nutzer erneut das Wake Word sagt, **Then** setzt haex dasselbe Gespräch fort; nach Ablauf dieser
   Zeit beginnt ein neues.
8. **Given** ein Gespräch steht im Chatverlauf, **When** der Nutzer es im Chatfenster öffnet,
   **Then** sieht er die vollständige Mitschrift mit Zeitstempeln: alles, was er nach dem Wake Word
   gesagt hat, und alles, was haex gesagt hat, einschließlich Freigaben, Ansagen, Stopp-Wörtern und
   unterbrochener Antworten bis zur Stelle des Abbruchs; er kann schriftlich weitermachen.
9. **Given** haex hat die Äußerung nicht verstanden (leer, unverständlich), **When** die Runde endet,
   **Then** bittet haex kurz um Wiederholung, statt zu raten.

---

### User Story 3 - Aufgaben per Sprache erledigen (Priority: P1)

Anna sagt: „Hey Haex, leg mir einen Termin für morgen um zehn mit Jonas an und schick ihm eine kurze
Mail dazu.“ Der Agent arbeitet mit seinen Werkzeugen. Weil das länger dauert, sagt haex
zwischendurch „Moment, ich lege den Termin an“. Vor dem Senden der Mail fragt haex: „Ich schicke
Jonas jetzt die Mail ‚Termin morgen um 10 Uhr‘. Soll ich?“ Anna sagt „Ja“, haex sendet und meldet:
„Erledigt, Termin steht und die Mail ist raus.“

**Why this priority**: Der Betreiber will komplette Abläufe per Sprache erledigen, nicht nur Fragen
beantworten. Das braucht Rückmeldung während langer Schritte und Freigaben ohne Bildschirm.

**Independent Test**: Mit Freigabestufe „Manuell“ eine Anweisung geben, die zwei ändernde Aktionen
braucht. haex kündigt jede Aktion laut an, führt sie nach „Ja“ aus, lässt sie nach „Nein“ weg und
meldet am Ende das Ergebnis gesprochen.

**Acceptance Scenarios**:

1. **Given** eine Anweisung, die Werkzeuge braucht, **When** der Agent sie bearbeitet, **Then**
   gelten dieselben Werkzeuge, Freigabestufen und Grenzen wie im Chat.
2. **Given** eine Runde dauert länger als 4 Sekunden ohne gesprochene Antwort, **When** der Agent
   noch arbeitet, **Then** sagt haex in einer kurzen Ansage (höchstens 12 Wörter und 4 Sekunden),
   woran es gerade arbeitet, statt zu schweigen.
3. **Given** eine Aktion braucht nach der Freigabestufe eine Freigabe, **When** sie ansteht, **Then**
   sagt haex in einem Satz, was genau passiert (Wirkung und Ziel), und wartet auf eine mündliche
   Bestätigung.
4. **Given** haex wartet auf eine Bestätigung, **When** der Nutzer „Ja“ sagt, **Then** führt haex die
   Aktion aus; **When** er „Nein“ sagt, **Then** lässt haex sie weg und sagt das dem Agenten.
5. **Given** haex wartet auf eine Bestätigung, **When** die Antwort unklar ist („vielleicht“, etwas
   anderes), **Then** fragt haex einmal nach; bleibt die Antwort unklar, gilt die Aktion als
   abgelehnt. **When** niemand antwortet, **Then** bleibt die Frage offen (FR-040) und nichts wird
   ausgeführt.
6. **Given** eine zerstörende Aktion, **When** haex um Bestätigung bittet, **Then** nennt es
   ausdrücklich, dass sie nicht rückgängig zu machen ist.
7. **Given** der Nutzer bittet per Sprache, eine Leitplanke zu ändern (etwa „schalte auf Auto“),
   **When** der Agent das versucht, **Then** wird es wie im Chat abgelehnt und haex sagt, dass das nur
   von Hand in den Einstellungen geht.
8. **Given** eine Aufgabe ist fertig, **When** haex das Ergebnis meldet, **Then** fasst es kurz
   zusammen, was getan wurde und was nicht (etwa wegen Ablehnung oder Fehler).
9. **Given** der Nutzer sagt „Erledige das und gib mir Bescheid“ und schweigt dann, **When** der
   Agent noch arbeitet, **Then** endet das Gespräch nicht; ist die Aufgabe fertig, meldet haex das
   von selbst gesprochen und hört danach für die Nachhörzeit zu.
10. **Given** eine Aufgabe läuft im Hintergrund und haex ist nicht stumm, **When** der Agent eine
    Rückfrage oder Freigabe braucht, **Then** meldet sich haex von selbst mit einem kurzen Hinweiston
    und stellt die Frage. Ist haex stumm, erscheint die Frage nur sichtbar und startet keine
    Nachhörzeit.
11. **Given** eine Aufgabe läuft, **When** der Nutzer das Gespräch aktiv beendet, **Then** bricht haex
    die Aufgabe samt Werkzeugaufrufen ab und sagt kurz, was bis dahin erledigt war.
12. **Given** eine offene Frage, auf die niemand geantwortet hat, **When** der Nutzer das nächste Mal
    das Wake Word sagt, **Then** stellt haex zuerst diese Frage erneut.

---

### User Story 4 - Rückfragen und Vorschläge (Priority: P2)

Anna sagt: „Schick die Präsentation an das Team.“ haex fragt nach: „Ich finde zwei Präsentationen
von dieser Woche: ‚Quartal‘ und ‚Roadmap‘. Welche meinst du?“ Anna sagt „die zweite“. Nach dem Senden
schlägt haex vor: „Soll ich für Freitag eine Erinnerung zum Nachfassen anlegen?“ Sie sagt „nein,
danke“, und haex lässt es.

**Why this priority**: Gezielte Rückfragen machen gesprochene Anweisungen erst verlässlich, weil
gesprochene Sätze öfter mehrdeutig sind als getippte. Vorschläge sind der Mehrwert eines Assistenten
gegenüber einer Sprachsteuerung. Beides baut auf US1 bis US3 auf.

**Independent Test**: Eine mehrdeutige Anweisung geben. haex stellt eine Rückfrage mit höchstens
drei Wahlmöglichkeiten, die Antwort per Ordinalzahl („die zweite“) wird richtig zugeordnet, und
ein abgelehnter Vorschlag führt zu keiner Aktion.

**Acceptance Scenarios**:

1. **Given** eine Anweisung mit mehreren plausiblen Zielen, **When** der Agent sie nicht eindeutig
   zuordnen kann, **Then** fragt haex gezielt nach, statt zu raten.
2. **Given** haex bietet Wahlmöglichkeiten an, **When** es sie vorliest, **Then** sind es höchstens
   drei, jeweils mit höchstens 12 Wörtern benannt; bei mehr nennt haex die Anzahl und fragt nach einem Merkmal zum
   Eingrenzen.
3. **Given** haex hat Wahlmöglichkeiten genannt, **When** der Nutzer mit Ordinalzahl, Name oder
   einem Merkmal antwortet, **Then** wird die richtige Möglichkeit gewählt.
4. **Given** der Agent macht einen Vorschlag, **When** der Nutzer ablehnt oder nicht reagiert,
   **Then** passiert nichts; **When** er zustimmt, **Then** wird der Vorschlag wie eine Anweisung
   bearbeitet, mit denselben Freigaben.
5. **Given** ein Gespräch, **When** der Agent Vorschläge macht, **Then** macht er höchstens einen pro
   Runde und nur zum aktuellen Thema.

---

### User Story 5 - Antworten, die man hören kann (Priority: P2)

Anna fragt nach ihren Terminen der Woche. haex nennt die Zahl und die nächsten zwei Termine und sagt,
dass die volle Liste im Chat steht. Eine Antwort mit Code oder einer langen Adresse liest haex nicht
Zeichen für Zeichen vor, sondern sagt, was es ist, und verweist auf den Bildschirm.

**Why this priority**: Modelle antworten für den Bildschirm (Listen, Tabellen, Formatierung). Wird
das wörtlich vorgelesen, ist das Gespräch unbrauchbar. Ohne diese Story funktionieren US1 bis US4
zwar, klingen aber schlecht.

**Independent Test**: Fragen stellen, deren Antwort im Chat eine lange Liste, eine Tabelle, einen
Link und einen Codeblock enthielte. Die gesprochene Antwort enthält keine Formatierungszeichen, ist
kurz und verweist für Details auf die Mitschrift.

**Acceptance Scenarios**:

1. **Given** der Assistent-Modus, **When** der Agent antwortet, **Then** antwortet er knapp und in
   gesprochener Sprache (kurze Sätze, keine Formatierung), weil haex ihm sagt, dass es ein
   Sprachgespräch ist.
2. **Given** eine Antwort enthält trotzdem Formatierung, Tabellen, Links oder Code, **When** haex sie
   spricht, **Then** liest es keine Formatierungszeichen vor und ersetzt Tabellen, Links und Code
   durch eine kurze Beschreibung mit Verweis auf die Mitschrift.
3. **Given** eine lange Antwort, **When** haex sie spricht, **Then** spricht es eine Zusammenfassung
   und bietet an, mehr vorzulesen; die vollständige Antwort steht in der Mitschrift.
4. **Given** Zahlen, Daten, Uhrzeiten und Abkürzungen, **When** haex sie spricht, **Then** spricht
   es sie so, wie ein Mensch sie vorlesen würde („morgen um zehn Uhr“, nicht „2026-10-29T10:00“).

---

### User Story 6 - Geheimnisse vorlesen lassen (Priority: P2)

Anna steht am Fernseher und sagt: „Haex, wie ist das WLAN-Passwort vom Gästenetz?“ haex sagt:
„Ich lese das Passwort von ‚Gäste-WLAN‘ laut vor. Soll ich?“ Nach ihrem „Ja“ buchstabiert haex es
eindeutig, mit Groß- und Kleinschreibung und Sonderzeichen. Auf „Nochmal langsamer“ wiederholt haex
es langsamer.

**Why this priority**: Der Betreiber will auch Geheimnisse ohne Bildschirm abrufen. Weil das Modell
nie Geheimnisse sieht (034/021), braucht es einen eigenen Weg. Die Story hängt an US3.

**Independent Test**: Nach einem Passwort fragen. haex fragt einmal pro Abruf nach, spricht nach „Ja“
das Passwort, und weder Modell-Eingabe noch Mitschrift noch Protokoll enthalten es im Klartext. Mit
„Nein“ wird nichts gesprochen.

**Acceptance Scenarios**:

1. **Given** der Nutzer fragt nach einem Geheimnis, **When** der passende Eintrag gefunden ist,
   **Then** nennt haex den Eintrag und fragt, ob es das Geheimnis laut vorlesen soll; das gilt bei
   jeder Freigabestufe, auch bei Auto.
2. **Given** die Rückfrage, **When** der Nutzer bestätigt, **Then** spricht haex das Geheimnis
   eindeutig buchstabiert; **When** er ablehnt oder schweigt, **Then** spricht haex nichts davon.
3. **Given** ein Geheimnis wurde vorgelesen, **When** man die Modell-Eingaben, die Mitschrift und das
   Protokoll ansieht, **Then** enthalten sie das Geheimnis nicht, nur einen Hinweis „Passwort von
   ‚Gäste-WLAN‘ vorgelesen“.
4. **Given** ein externer Sprachdienst ist als Stimme aktiv, **When** haex ein Geheimnis spricht,
   **Then** nutzt es dafür eine lokale Stimme; ist keine lokale Stimme installiert, zeigt haex das
   Geheimnis stattdessen nur auf dem Bildschirm an und sagt das.
5. **Given** der Nutzer bittet um Wiederholung, **When** das Geheimnis eben vorgelesen wurde,
   **Then** wiederholt haex es ohne neue Bestätigung, aber nur innerhalb derselben Runde und
   derselben Nachhörzeit.
6. **Given** ein Eintrag hat mehrere Geheimnisse (Passwort, Einmalcode, PIN), **When** der Nutzer
   nur eins verlangt, **Then** liest haex nur dieses vor.

---

### User Story 7 - Eigenes Wake Word einlernen (Priority: P3)

Anna will statt „Hey Haex“ lieber „Hallo Eichhörnchen“ sagen. In den Einstellungen wählt sie „Eigenes
Wake Word“, spricht es mehrmals ein und probiert es danach aus. haex meldet, wenn das Wort zu kurz
ist oder einem häufigen Wort zu ähnlich klingt.

**Why this priority**: Der Betreiber will ein frei wählbares Wort. Das mitgelieferte Standardwort
genügt aber für alle anderen Stories, deshalb kommt das Einlernen zuletzt.

**Independent Test**: Ein eigenes Wort einlernen, danach löst es aus, „Haex“ und „Hey Haex“ nicht
mehr.
Eine Probephase zeigt, ob das Wort erkannt wird. Die Rohaufnahmen sind nach dem Einlernen gelöscht.

**Acceptance Scenarios**:

1. **Given** die Wake-Word-Einstellungen, **When** der Nutzer „Eigenes Wake Word“ wählt, **Then**
   führt haex ihn durch mehrere Aufnahmen desselben Worts und zeigt den Fortschritt.
2. **Given** die Aufnahmen, **When** das Wort zu kurz ist, die Aufnahmen zu verschieden sind oder das
   Wort zu leicht verwechselt würde, **Then** sagt haex das und bittet um ein anderes Wort oder neue
   Aufnahmen.
3. **Given** das Einlernen ist fertig, **When** der Nutzer die Probephase nutzt, **Then** zeigt haex
   bei jedem Sprechen des Worts, ob es erkannt wurde, bevor er es übernimmt.
4. **Given** ein eigenes Wort ist übernommen, **When** der Modus läuft, **Then** löst nur dieses Wort
   aus; der Nutzer kann jederzeit zum Standardwort zurück.
5. **Given** das Einlernen ist abgeschlossen oder abgebrochen, **When** man den Speicher prüft,
   **Then** sind die Rohaufnahmen gelöscht; nur das erlernte Muster bleibt und gilt nur auf
   diesem Gerät.
6. **Given** zu viele Fehlauslösungen, **When** der Nutzer die Empfindlichkeit senkt, **Then** löst
   das Wort seltener aus, auch fälschlich.

---

### User Story 8 - Stimme wählen und eigene Stimmen einrichten (Priority: P3)

Anna wählt in den Einstellungen eine andere lokale Stimme, hört eine Probe und stellt das Tempo etwas
schneller. Dann spricht sie 20 Sekunden ein, bestätigt, dass es ihre eigene Stimme ist, und haex
antwortet ab sofort mit ihrer Stimme, auch auf ihrem Laptop. Ihr Kollege importiert ein
Community-Modell für Polnisch. Später richtet Anna einen externen Sprachdienst ein, weil der
natürlicher klingt; die Assistent-Anzeige zeigt seitdem, dass Antworten das Gerät verlassen.

**Why this priority**: Eine lokale Standardstimme genügt für US1 bis US7. Wahl und externer Dienst
verbessern das Erlebnis, sind aber nicht nötig.

**Independent Test**: Eine andere lokale Stimme wählen und eine Probe hören; die nächste Antwort nutzt
sie ohne Neustart. Eine Stimme aus einer Aufnahme anlegen; sie erscheint auf einem zweiten Gerät
desselben Vaults. Ein Modell importieren und wählen. Einen externen Dienst einrichten; die Anzeige
kennzeichnet das, bis er wieder abgeschaltet ist.

**Acceptance Scenarios**:

1. **Given** die Stimm-Einstellungen, **When** der Nutzer sie öffnet, **Then** sieht er höchstens
   drei kuratierte lokale Stimmen pro Sprache mit Größe und Eignung für sein Gerät; weitere
   installierbare Stimmen lassen sich über die Suche finden (wie bei der Modellwahl in 010).
2. **Given** eine Stimme in der Auswahl, **When** der Nutzer „Probe“ wählt, **Then** spricht sie einen
   Beispielsatz; eine nicht installierte Stimme wird dafür erst geladen.
3. **Given** der Nutzer wählt eine Stimme oder ändert das Tempo, **When** die nächste Antwort kommt,
   **Then** nutzt sie die neue Einstellung ohne Neustart.
4. **Given** ein externer Sprachdienst ist eingerichtet und aktiv, **When** der Modus läuft, **Then**
   kennzeichnet die Assistent-Anzeige dauerhaft, dass Antworttext an diesen Dienst geht.
5. **Given** der externe Dienst ist nicht erreichbar, **When** haex antworten will, **Then** spricht
   es mit der lokalen Stimme und sagt einmal pro Gespräch, dass der Dienst nicht erreichbar war.
6. **Given** der Nutzer wählt „Eigene Stimme anlegen“, **When** er 10–30 Sekunden einspricht oder
   eine Audiodatei wählt, Sprache und Namen angibt und die Einwilligung bestätigt, **Then** hört er
   eine Probe, bevor er speichert; ohne bestätigte Einwilligung lässt sich nicht speichern.
7. **Given** eine Aufnahme dauert weniger als 10 oder mehr als 30 Sekunden, hat in den 20-ms-
   Sprachrahmen einen medianen RMS-Pegel unter −32 dBFS, erreicht in mehr als 1 % der Samples
   mindestens −1 dBFS oder hat weniger als 10 dB Signal-Rausch-Abstand, **When** der Nutzer sie
   prüfen lässt, **Then** nennt haex den gemessenen Grund und bittet um eine neue Aufnahme.
8. **Given** eine angelegte Stimme, **When** der Nutzer ein zweites Gerät mit demselben Vault öffnet,
   **Then** steht sie dort zur Wahl; fehlt das Sprachmodell dafür, lädt haex es bei der Wahl.
   Löscht er sie auf einem Gerät, verschwindet sie überall, und Geräte, die sie nutzten, fallen auf
   die Standardstimme der Sprache zurück.
9. **Given** der Nutzer wählt „Modell importieren“ mit einem Ordner oder Repository, **When** haex
   die Dateien als unterstütztes Modell erkennt, **Then** gibt der Nutzer Namen, Sprachen und Lizenz
   an, bestätigt sein Nutzungsrecht und kann das Modell danach wie jede lokale Stimme wählen; nicht
   erkannte Dateien lehnt haex mit Grund ab.

---

### User Story 9 - Im Hintergrund und auf dem Telefon (Priority: P2)

Anna schaltet den Modus auf ihrem Android-Telefon ein. In der Benachrichtigungsleiste steht „haex hört
zu“ mit einem Knopf zum Ausschalten. Auch bei gesperrtem Bildschirm, solange der Vault entsperrt ist,
reagiert haex auf das Wake Word. Bei einem Telefonat hört haex nicht zu.

**Why this priority**: Der Betreiber will den Assistenten nutzen, solange haex läuft, nicht nur im
Fenster. Auf dem Desktop deckt das US1 ab (minimiert). Android braucht dafür einen eigenen Weg mit
sichtbarer Benachrichtigung.

**Independent Test**: Auf Android den Modus einschalten, die App verlassen und den Bildschirm sperren.
Das Wake Word startet ein Gespräch, die Benachrichtigung ist sichtbar, und ihr Knopf schaltet den
Modus aus.

**Acceptance Scenarios**:

1. **Given** Android und eingeschalteter Modus, **When** die App in den Hintergrund geht, **Then**
   hört haex weiter zu und zeigt eine dauerhafte Benachrichtigung mit Zustand und Knopf zum
   Ausschalten.
2. **Given** iOS, **When** die App in den Hintergrund geht, **Then** hört haex nicht mehr zu, die
   Anzeige erklärt das beim nächsten Öffnen einmal, und im Vordergrund hört haex wieder zu.
3. **Given** ein Telefonat, eine Videokonferenz oder eine andere App belegt das Mikrofon exklusiv,
   **When** das passiert, **Then** pausiert haex das Zuhören, zeigt „pausiert“ und setzt danach von
   selbst fort.
4. **Given** der Desktop, **When** haex minimiert oder verdeckt ist, **Then** bleibt die
   Assistent-Anzeige als Symbol im Systembereich sichtbar und bietet Ein/Aus und „Gespräch zeigen“.
5. **Given** der Nutzer will ohne Zuruf sprechen (laute Umgebung), **When** er in der kompakten
   Mitschrift „Sprechen“ wählt oder das Tastenkürzel drückt, **Then** beginnt ein Gespräch wie nach
   dem Wake Word.
6. **Given** der Nutzer will kurz Ruhe, **When** er „stumm“ wählt, **Then** hört und spricht haex
   nicht, bis er die Stummschaltung aufhebt; der Modus bleibt an. Eine laufende Aufgabe läuft weiter;
   ihr Ergebnis und offene Fragen zeigt haex nur sichtbar an.
7. **Given** der Nutzer ändert eine Assistent-Einstellung unter „KI“, **When** er eine Auswahl trifft,
   **Then** wird sie sofort auf diesem Gerät gespeichert und bleibt nach Neustart erhalten.
8. **Given** die Assistent-Anzeige ist sichtbar, **When** der Nutzer sie verschiebt, **Then** bleibt
   die Position auf diesem Gerät erhalten und Zustand sowie Bedienelemente sind für Screenreader
   zugänglich.
9. **Given** das Klangset wird ausgeliefert, **When** die Release-Prüfung erfolgt, **Then** ist die
   Lizenz für die Weitergabe mit haex dokumentiert und die Herkunft im Repository festgehalten.

---

### Edge Cases

- **Mehrere eigene Geräte im selben Raum**: Ist der Modus auf zwei Geräten an, können beide
  reagieren. Der Modus ist pro Gerät; eine Abstimmung zwischen Geräten gehört nicht in diese Spec.
  Der Nutzer schaltet ihn auf den Geräten ein, die zuhören sollen.
- **Fremde Stimmen**: Jeder, der das Wake Word sagt, kann ein Gespräch beginnen und Freigaben
  mündlich bestätigen, auch für Geheimnisse. Eine Sprechererkennung gibt es in dieser Spec nicht. Die
  Einstellungen weisen beim Einschalten einmal darauf hin.
- **Wake Word im Fernsehen oder in einer Videokonferenz**: Es beginnt ein Gespräch. Kommt danach
  nichts Verständliches, endet das Gespräch nach der Nachhörzeit ohne Antwort und ohne Eintrag im
  Verlauf.
- **Nutzer spricht während einer Freigabe-Frage etwas anderes**: Das gilt als keine Bestätigung; haex
  fragt einmal nach und lässt die Aktion bei erneut unklarer Antwort weg.
- **Nutzer ist weg, während eine Aufgabe läuft**: haex meldet das Ergebnis oder die Frage einmal
  gesprochen. Kommt keine Antwort, bleibt eine Frage offen und die Anzeige steht auf „wartet auf
  Antwort“; ein fertiges Ergebnis steht in der Mitschrift. haex wiederholt sich nicht von selbst.
- **Neues Anliegen, während eine Aufgabe läuft**: Das Wake Word führt in dasselbe Gespräch. Der Nutzer
  kann nach dem Stand fragen oder ein weiteres Anliegen nennen; der Agent bearbeitet es nach der
  laufenden Runde, wie eine weitere Nachricht im Chat.
- **Werkzeug braucht den Bildschirm** (etwa eine Auswahl im Dateidialog oder ein Fenster, das nur
  angezeigt wird): haex sagt, dass dieser Schritt den Bildschirm braucht, und öffnet ihn. Der Schritt
  wird zu einer offenen Frage, bis der Nutzer ihn am Bildschirm erledigt oder das Gespräch beendet.
- **Modell antwortet nicht oder Fehler beim Modell**: haex sagt kurz, dass es gerade nicht antworten
  kann, und nennt den Grund in einfachen Worten; das Gespräch bleibt offen.
- **Spracherkennung oder Stimme bricht ab**: haex bricht die Runde ab, sagt es (oder zeigt es, wenn
  die Stimme ausfällt) und kehrt zu „bereit“ zurück.
- **Sehr lange Äußerung**: Eine Runde hat eine Höchstdauer wie in 008 (FR-017); danach wird das
  bisher Gesagte bearbeitet.
- **Vault wird mitten im Gespräch gesperrt**: Das Gespräch endet sofort, die Ausgabe stoppt, und das
  bis dahin Gesagte bleibt im Verlauf.
- **Ausgewähltes Modell ist ein Delegate (007)**: Er bekommt keine haex-Werkzeuge (032); Gespräche
  funktionieren, Aufgaben in haex nicht. haex sagt das einmal pro Gespräch.
- **Plan-Modus**: Ändernde Aktionen werden wie im Chat nicht ausgeführt; haex sagt, dass es nur einen
  Plan erstellt hat.
- **Kopfhörer**: Mit Kopfhörern hört das Mikrofon die eigene Stimme nicht; Dazwischenreden
  funktioniert genauso.
- **Andere Sprache als die Oberfläche**: Erkennung und Antwort folgen der Sprache, in der der Nutzer
  spricht, sofern es dafür eine Stimme gibt; sonst antwortet haex in der Sprache der Oberfläche.

## Requirements _(mandatory)_

### Functional Requirements

#### Modus und Anzeige

- **FR-001**: Der Launcher MUST einen Eintrag „Sprachassistent“ haben, der den Assistent-Modus auf
  diesem Gerät ein- und ausschaltet und seinen Zustand zeigt. Er MUST sich wie andere Einträge ins
  Dock heften lassen (045).
- **FR-002**: Der Modus MUST pro Gerät gelten und MUST nach einem Neustart und Entsperren auf diesem
  Gerät wieder so stehen, wie der Nutzer ihn verlassen hat. Andere Geräte MUST NOT ihn übernehmen.
- **FR-003**: Solange der Modus an ist, MUST eine Assistent-Anzeige sichtbar sein, die den Zustand
  zeigt (bereit, hört zu, denkt nach, arbeitet, spricht, wartet auf Antwort, stumm, pausiert). Auf
  dem Desktop MUST sie auch bei minimiertem oder verdecktem Fenster als Symbol im Systembereich
  sichtbar bleiben, auf Android als dauerhafte Benachrichtigung.
- **FR-004**: haex MUST nur bei entsperrtem Vault zuhören. Sperren, Schließen oder Beenden MUST das
  Zuhören und ein laufendes Gespräch sofort beenden.
- **FR-005**: Der Modus MUST sich nur einschalten lassen, wenn Spracherkennung, Wake Word, Stimme und
  ein Modell bereit sind. Fehlt etwas, MUST haex sagen, was, und den Download bzw. die Einrichtung
  anbieten.

#### Wake Word

- **FR-006**: Das Wake Word MUST vollständig auf dem Gerät erkannt werden. Bevor es erkannt ist, MUST
  NOT haex Audio speichern, anzeigen, transkribieren oder an einen Dienst schicken.
- **FR-007**: haex MUST ein mitgeliefertes Standard-Wake-Word haben, das ohne Einsprechen
  funktioniert: „Haex“ und „Hey Haex“ lösen beide aus. Für beide Formen gelten SC-003 und SC-004.
- **FR-008**: Der Nutzer MUST ein eigenes Wake Word durch mehrere Aufnahmen auf dem Gerät einlernen
  können, mit Prüfung auf Eignung (Länge, Gleichmäßigkeit, Verwechslungsgefahr) und einer Probephase.
  Die Rohaufnahmen MUST nach Abschluss oder Abbruch gelöscht werden; nur das erlernte Muster bleibt
  und gilt nur auf diesem Gerät.
- **FR-009**: Der Nutzer MUST die Empfindlichkeit des Wake Words in mehreren Stufen einstellen
  können.
- **FR-010**: Die eigene Sprachausgabe von haex MUST NOT das Wake Word auslösen.
- **FR-011**: Ein Gespräch MUST auch ohne Wake Word beginnen können: per „Sprechen“ in der Anzeige
  und per Tastenkürzel im haex-Fenster. Das Kürzel hat eine Standardbelegung im Aktionskatalog und
  wird umbelegbar, sobald die spätere Kürzel-Spec das für alle Kürzel einführt.

#### Gesprächsführung

- **FR-012**: haex MUST das Ende einer Äußerung an einer Sprechpause erkennen. Die Länge dieser Pause
  MUST einstellbar sein.
- **FR-013**: Nach jeder Antwort MUST haex für die Nachhörzeit ohne Wake Word weiter zuhören. Eine
  Äußerung darin MUST als nächste Runde desselben Gesprächs gelten. Die Nachhörzeit MUST einstellbar
  sein.
- **FR-014**: Ein Gespräch MUST enden, wenn der Nutzer es aktiv beendet (Schlusswendung, „Gespräch
  beenden“, Ausschalten des Modus, Sperren des Vaults), oder wenn die Nachhörzeit ohne Äußerung
  abläuft, während weder eine Aufgabe läuft noch eine offene Frage besteht. Stummschalten beendet ein
  Gespräch nur, wenn keine Aufgabe läuft.
- **FR-015**: Ein Wake Word innerhalb der Fortsetzungszeit nach Gesprächsende MUST dasselbe Gespräch
  fortsetzen; danach MUST es ein neues beginnen. Die Fortsetzungszeit MUST einstellbar sein.
- **FR-016**: Spricht der Nutzer, während haex spricht, MUST haex die Ausgabe abbrechen und das
  Gesagte als nächste Runde aufnehmen. Die eigene Ausgabe von haex MUST NOT als Dazwischenreden
  gelten. Erfüllt die Echo-Unterdrückung auf einer Plattform SC-005 bei Ausgabe über Lautsprecher
  nicht, MUST dort während der Ausgabe nur das Wake Word oder ein Stopp-Wort unterbrechen; mit
  Kopfhörern MUST jede Sprache unterbrechen. Welche Plattformen welches Verhalten haben, MUST
  dokumentiert sein.
- **FR-017**: Die Stopp-Wörter aus 008 („Stopp“, „Halt“, „Abbrechen“) MUST im Assistent-Modus eine
  laufende Runde samt Ausgabe und Werkzeugaufrufen abbrechen, ohne über das Modell zu gehen; das
  Gespräch bleibt offen.
- **FR-018**: Jede Runde MUST hörbar und sichtbar quittiert werden, mit den Tönen aus FR-043 und
  dem passenden Zustand der Anzeige.
- **FR-019**: Leere oder unverständliche Äußerungen MUST zu einer kurzen Bitte um Wiederholung führen,
  nicht zu einer Modell-Anfrage. Ein Gespräch ohne verständliche Äußerung MUST NOT im Verlauf
  erscheinen.

#### Agent und Antworten

- **FR-020**: Gespräche MUST über den eingebauten Agenten laufen, mit denselben Werkzeugen,
  Freigabestufen, Leitplanken und Grenzen wie im Chat (003, 032). Das Modell für Gespräche MUST
  einstellbar sein; Standard ist das Standard-Chatmodell des Geräts.
- **FR-021**: Im Assistent-Modus MUST haex dem Modell mitteilen, dass es ein gesprochenes Gespräch
  ist: knapp antworten, ohne Formatierung, bei Mehrdeutigkeit gezielt nachfragen statt raten,
  höchstens drei Wahlmöglichkeiten nennen, höchstens einen Vorschlag pro Runde zum aktuellen Thema
  machen und nichts aus einem Vorschlag ohne Zustimmung tun.
- **FR-022**: haex MUST Antworten sprechen, während sie noch entstehen. Formatierungszeichen MUST NOT
  gesprochen werden; Tabellen, Links und Code MUST durch eine kurze Beschreibung mit Verweis auf die
  Mitschrift ersetzt werden; Zahlen, Daten und Uhrzeiten MUST natürlich gesprochen werden.
- **FR-023**: Dauert eine Runde länger als 4 Sekunden ohne gesprochene Antwort, MUST haex eine kurze
  Ansage im Sinne des Begriffs „Kurze Ansage“ sagen, woran der Agent gerade arbeitet.

#### Freigaben und Geheimnisse

- **FR-024**: Braucht eine Aktion nach der Freigabestufe eine Freigabe, MUST haex ihre Wirkung und ihr
  Ziel in einem Satz sagen (bei zerstörenden Aktionen mit dem Hinweis, dass sie endgültig sind) und
  auf eine mündliche Bestätigung warten. Nur eine eindeutige Zustimmung MUST als Freigabe gelten;
  Ablehnung und eine unklare Antwort nach einer Nachfrage MUST als abgelehnt gelten. Ohne Antwort MUST
  die Freigabe eine offene Frage bleiben (FR-040); ausgeführt wird nichts.
- **FR-025**: Geheimnisse MUST nur nach einer mündlichen Bestätigung pro Abruf vorgelesen werden, bei
  jeder Freigabestufe. haex MUST das Geheimnis selbst sprechen; es MUST NOT in die Eingabe des
  Modells, die Mitschrift, das Protokoll oder einen externen Dienst gelangen. In der Mitschrift MUST
  stattdessen ein Hinweis stehen, welches Geheimnis von welchem Eintrag vorgelesen wurde.
- **FR-026**: Geheimnisse MUST eindeutig buchstabiert werden (Groß- und Kleinschreibung, Ziffern,
  Sonderzeichen) und MUST innerhalb derselben Runde und Nachhörzeit ohne neue Bestätigung wiederholt
  und langsamer gesprochen werden können.
- **FR-027**: Ist ein externer Sprachdienst aktiv, MUST haex Geheimnisse mit einer lokalen Stimme
  sprechen. Gibt es keine, MUST es das Geheimnis nur auf dem Bildschirm zeigen und das sagen.
- **FR-028**: Leitplanken MUST auch per Sprache nicht änderbar sein; haex MUST bei solchen Bitten
  sagen, dass das nur von Hand in den Einstellungen geht.

#### Stimme

- **FR-029**: haex MUST für jede der sechs Sprachen (FR-052) zwei bis drei mitgelieferte lokale
  Stimmen anbieten, eine davon als Standard, mit Größe und Eignung für das Gerät wie die Modellwahl
  in 010. Mitgelieferte Stimmen MUST in ihrer Sprache ohne fremden Akzent klingen (SC-017). Die
  Speicherung der Wahl MUST NOT an eine bestimmte Sprachausgabe-Technik gebunden sein.
- **FR-030**: Der Nutzer MUST eine Probe jeder Stimme hören und das Sprechtempo einstellen können.
  Änderungen MUST ab der nächsten Ausgabe ohne Neustart gelten.
- **FR-031**: Der Nutzer MUST einen externen Sprachdienst einrichten können. Seine Zugangsdaten MUST
  so behandelt werden wie die anderer externer Dienste. Solange er aktiv ist, MUST die
  Assistent-Anzeige das dauerhaft kennzeichnen. Ist er nicht erreichbar, MUST haex mit der lokalen
  Stimme sprechen und das einmal pro Gespräch sagen. Ist auch keine lokale Stimme verfügbar, MUST
  haex die Antwort sichtbar anzeigen und darauf hinweisen, dass Sprachausgabe nicht verfügbar ist.
- **FR-032**: Nutzt die Spracherkennung einen externen Dienst (008 US3), MUST die Anzeige auch das
  dauerhaft kennzeichnen.

#### Daten und Plattformen

- **FR-033**: Jedes Gespräch mit mindestens einer verständlichen Äußerung MUST als Chat-Unterhaltung
  im Verlauf erscheinen, als Sprachgespräch gekennzeichnet, und MUST sich schriftlich fortsetzen
  lassen.
- **FR-034**: Audio MUST nach der Erkennung verworfen werden und MUST NOT gespeichert werden (008
  FR-020). Die einzige Ausnahme sind die Aufnahmen beim Einlernen (FR-008).
- **FR-035**: Auf Android MUST haex im Hintergrund und bei gesperrtem Bildschirm über einen
  Vordergrund-Dienst mit dauerhafter Benachrichtigung zuhören können. Auf iOS MUST haex nur bei
  sichtbarer App zuhören und das einmal erklären.
- **FR-036**: Belegt ein Telefonat oder eine andere App das Mikrofon exklusiv, MUST haex das Zuhören
  pausieren, „pausiert“ anzeigen und danach von selbst fortsetzen.
- **FR-037**: Beim ersten Einschalten MUST haex einmal darauf hinweisen, dass jede Person, die das
  Wake Word sagt, Gespräche führen und Freigaben bestätigen kann, auch für Geheimnisse.
- **FR-038**: Alle Einstellungen des Assistent-Modus MUST in der Settings-Kategorie „KI“ liegen,
  pro Gerät gelten und bei Auswahl gespeichert werden.

#### Laufende Aufgaben

- **FR-039**: Beendet der Nutzer ein Gespräch aktiv, MUST haex eine laufende Aufgabe dieses Gesprächs
  samt Werkzeugaufrufen abbrechen und kurz sagen, was bis dahin erledigt war.
- **FR-040**: Läuft eine Aufgabe des Gesprächs weiter, MUST haex sich von selbst melden, wenn sie
  fertig ist oder der Agent eine Rückfrage oder Freigabe braucht: mit Hinweiston und kurzer Ansage,
  bei Stummschaltung nur sichtbar. Solange haex stumm ist, MUST es weder zuhören noch eine
  Nachhörzeit starten oder verlängern. Erst nach dem Aufheben der Stummschaltung MUST haex für die
  Nachhörzeit zuhören. Bleibt eine Frage
  unbeantwortet, MUST sie offen bleiben, die Anzeige MUST „wartet auf Antwort“ zeigen, und beim
  nächsten Wake Word MUST haex sie zuerst stellen. Von selbst wiederholen MUST haex sie nicht.

#### Schwebende Anzeige

- **FR-041**: Im offenen haex-Fenster (Desktop und Telefon) MUST die Assistent-Anzeige ein kleines
  schwebendes Element über allen Fenstern und Tabs sein, das keinen Platz im Window Manager belegt.
  Es MUST sich verschieben lassen; die Position MUST pro Gerät erhalten bleiben. Jeder Zustand MUST
  eine eigene Darstellung haben (etwa das haex-Eichhörnchen) und zusätzlich als Text für
  Screenreader und Tooltip verfügbar sein.
- **FR-042**: Ein Klick auf die schwebende Anzeige MUST die kompakte Mitschrift aufklappen: die
  letzten Runden des laufenden Gesprächs live, eine offene Frage hervorgehoben, dazu „Sprechen“,
  Stumm, Ein/Aus, „Gespräch zeigen“ (öffnet die Unterhaltung im Chat-Tab) und „Gespräch beenden“.
  Ein weiterer Klick oder Escape MUST sie wieder einklappen.

#### Töne

- **FR-043**: haex MUST ein eigenes, mitgeliefertes Klangset mit je einem kurzen, unterscheidbaren
  Ton für diese Anlässe haben:
  - **Hört zu**: Wake Word erkannt oder Gespräch per „Sprechen“ bzw. Tastenkürzel begonnen.
  - **Verstanden**: Ende einer Äußerung erkannt; haex nimmt sie entgegen und bearbeitet sie.
  - **Arbeitet**: ein leiser, wiederkehrender Ton, solange eine Runde länger als einige Sekunden ohne
    gesprochene Antwort läuft, zusätzlich zur Ansage aus FR-023.
  - **Frage**: haex stellt eine Rückfrage oder bittet um Freigabe.
  - **Fertig**: Eine Aufgabe ist erfolgreich abgeschlossen.
  - **Fehler**: Eine Aufgabe oder ein Schritt ist fehlgeschlagen, oder haex kann gerade nicht
    antworten.
  - **Abgebrochen**: Ein Stopp-Wort oder aktives Beenden hat eine Runde oder Aufgabe abgebrochen.
  - **Nicht verstanden**: Die Äußerung war leer oder unverständlich (FR-019).
  - **Gesprächsende**: Die Nachhörzeit ist abgelaufen oder das Gespräch wurde beendet; haex hört
    wieder nur auf das Wake Word.
  - **Aufmerksamkeit**: haex meldet sich von selbst zu einer laufenden Aufgabe (FR-040).
  - **Geheimnis**: unmittelbar bevor haex ein Geheimnis laut vorliest.
  - **Modus ein / Modus aus** und **Stumm / Stummschaltung aufgehoben**.
- **FR-044**: Die Töne MUST kurz sein (der wiederkehrende Ton „Arbeitet“ ausgenommen), als
  zusammengehörige Familie klingen, ohne Netz funktionieren und dürfen weder das Wake Word noch ein
  Dazwischenreden auslösen. Jeder Ton MUST eine sichtbare Entsprechung in der Anzeige haben. Töne
  MUST NOT eine gesprochene Frage oder Freigabe ersetzen; sie gehen ihr nur voraus.
- **FR-045**: Der Nutzer MUST die Lautstärke der Töne getrennt von der Sprachausgabe einstellen und
  die Töne ausschalten können; ausgenommen sind „Hört zu“ und „Geheimnis“, die immer erklingen,
  solange haex nicht stummgeschaltet ist. Bei Stummschaltung MUST haex keine Töne spielen.
- **FR-046**: Die Töne MUST unter einer Lizenz stehen, die die Weitergabe mit haex erlaubt, und ihre
  Herkunft MUST im Repository dokumentiert sein.

#### Mitschrift

- **FR-047**: Jedes Gespräch MUST vollständig in der Vault-Datenbank mitgeschrieben werden, jeder
  Eintrag mit Zeitpunkt, Sprecher und Art (Mitschrift-Eintrag): jede Äußerung des Nutzers nach dem
  Wake Word, auch Antworten auf Freigaben und Rückfragen, Stopp-Wörter, Schlusswendungen und nicht
  Verstandenes (als „nicht verstanden“ mit dem erkannten Rest), und jede Ausgabe von haex, auch
  Ansagen, Freigabe-Fragen, Fortschrittsansagen und Meldungen aus dem Hintergrund. Bei Freigaben MUST
  das Ergebnis (bestätigt, abgelehnt, offen, abgebrochen) dabeistehen, bei unterbrochener Ausgabe die
  Stelle, bis zu der haex gesprochen hat, und der vollständige Text. Geheimnisse MUST nur als
  Vorlese-Hinweis erscheinen (FR-025). Audio vor dem Wake Word MUST NOT mitgeschrieben werden
  (FR-006).
- **FR-048**: Die Mitschrift MUST mit dem Gespräch synchronisiert, durchsuchbar und im Chatfenster als
  eigene Ansicht mit Zeitstempeln lesbar sein; sie MUST erhalten bleiben, bis der Nutzer das Gespräch
  löscht, und MUST mit ihm gelöscht werden.

#### Eigene Stimmen und Sprachen

- **FR-049**: Der Nutzer MUST eine Stimme aus einer Aufnahme in haex oder einer Audiodatei
  (10–30 Sekunden) nachbilden können, mit Name und Sprache. haex MUST die Aufnahme vor dem Speichern
  prüfen (Länge, Lautstärke, Übersteuerung, Rauschen) und eine Probe abspielen. Die Nachbildung MUST
  ganz auf dem Gerät geschehen.
- **FR-050**: Vor dem Speichern MUST der Nutzer bestätigen, dass es seine eigene Stimme ist oder die
  sprechende Person eingewilligt hat; der Zeitpunkt der Bestätigung MUST gespeichert werden.
  Nachgebildete Stimmen MUST verschlüsselt im Vault liegen, mit ihm synchronisiert werden und sich
  löschen lassen; das Löschen MUST auf allen Geräten wirken, die dann auf die Standardstimme der
  Sprache zurückfallen.
- **FR-051**: Der Nutzer MUST Sprachmodell-Dateien aus einem lokalen Ordner oder einem
  Modell-Repository importieren können. haex MUST die Modellart an den Dateien erkennen und nicht
  unterstützte Dateien mit Grund ablehnen. Name, Sprachen und Lizenzangabe MUST angegeben und das
  Nutzungsrecht bestätigt werden. Importierte Modelle MUST nur auf diesem Gerät liegen, MUST keinen
  ausführbaren Code mitbringen oder laden und MUST bei Modellen mit mehreren Sprechern die Wahl des
  Sprechers erlauben.
- **FR-052**: haex MUST in Deutsch, Englisch, Französisch, Spanisch, Italienisch und Portugiesisch
  sprechen und verstehen: Schlusswendungen, Stopp-Wörter, Antworten auf Freigaben und Rückfragen,
  Bitten um Wiederholung und Buchstabieren MUST in allen sechs Sprachen funktionieren. Die Oberfläche
  bleibt Deutsch und Englisch.
- **FR-053**: Nachgebildete und importierte Stimmen MUST als lokale Stimmen gelten (auch für
  FR-027). Ein externer Sprachdienst MUST als extern gelten, auch wenn er im eigenen Netz läuft.

### Key Entities

- **Assistent-Einstellungen (pro Gerät)**: an/aus, stumm, Wake Word (Standard oder eigenes),
  Empfindlichkeit, Lautstärke und An/Aus der Töne, Pausenlänge, Nachhörzeit, Fortsetzungszeit,
  Modell für Gespräche, Stimme pro Sprache, Tempo, externer Sprachdienst, Position der schwebenden
  Anzeige. Gilt nur auf diesem Gerät.
- **Eigenes Wake Word**: das erlernte Muster eines vom Nutzer eingesprochenen Worts, mit Bezeichnung.
  Ohne Rohaufnahmen; gilt nur auf diesem Gerät.
- **Stimme**: eine mitgelieferte lokale Stimme (Sprache, Größe, Eignung, installiert ja/nein), eine
  nachgebildete Stimme, ein importiertes Modell oder ein externer Sprachdienst mit Zugangsdaten.
- **Nachgebildete Stimme**: Name, Sprache, die Referenzaufnahme und der Zeitpunkt der bestätigten
  Einwilligung; verschlüsselt im Vault, auf allen Geräten.
- **Importiertes Modell**: Name, Modellart, Sprachen, Sprecher, Lizenzangabe, Herkunft; nur auf diesem
  Gerät.
- **Sprachgespräch**: eine Chat-Unterhaltung (006) mit der Kennzeichnung „Sprachgespräch“, Zeitpunkt
  des Endes (für die Fortsetzungszeit) und den Runden als Mitschrift.
- **Klangset**: die mitgelieferten Hinweistöne, je einer pro Anlass aus FR-043, als zusammengehörige
  Familie gestaltet. Lautstärke und An/Aus sind Teil der Assistent-Einstellungen.
- **Mitschrift-Eintrag**: eine Zeile der vollständigen Mitschrift eines Gesprächs: Zeitpunkt,
  Sprecher (Nutzer oder haex), Art (Äußerung, Antwort, Ansage, Freigabe-Frage, Freigabe-Antwort,
  Antwort auf eine Rückfrage, Stopp, Schlusswendung, nicht verstanden, Vorlese-Hinweis) und Text; bei
  unterbrochener Ausgabe die Stelle des Abbruchs, bei Freigaben das Ergebnis.
- **Vorlese-Hinweis**: ein Eintrag in der Mitschrift statt eines Geheimnisses: welcher
  Passwort-Eintrag, welches Feld, wann.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Nach dem Ende einer Äußerung quittiert haex sie innerhalb von 0,5 Sekunden hörbar; den
  Ton „hört zu“ spielt haex innerhalb von 0,3 Sekunden nach dem Ende des Wake Words.
- **SC-002**: Sobald der Agent die ersten Wörter einer Antwort liefert, beginnt haex innerhalb einer
  Sekunde zu sprechen. Eine Antwort auf eine einfache Frage ohne Werkzeug beginnt nach dem Ende der
  Äußerung mit einem Cloud-Modell innerhalb von 3 Sekunden, mit dem lokalen Standardmodell auf dem
  Referenz-Desktop innerhalb von 6 Sekunden.
- **SC-003**: Das Standard-Wake-Word („Haex“ wie „Hey Haex“) wird in mindestens 9 von 10 Fällen
  erkannt, wenn es in normaler Lautstärke aus bis zu 3 Metern in einem ruhigen Raum gesprochen wird.
- **SC-004**: In 8 Stunden typischer Hintergrundgeräusche (Gespräche, Fernsehen, Musik) löst das
  Standard-Wake-Word bei mittlerer Empfindlichkeit höchstens einmal fälschlich aus.
- **SC-005**: Dazwischenreden stoppt die Ausgabe innerhalb von 0,5 Sekunden; die eigene Ausgabe über
  Lautsprecher löst in einem Test mit 50 Antworten kein Wake Word und kein Dazwischenreden aus. Eine
  Plattform, die das bei freiem Dazwischenreden nicht schafft, nutzt dort das Verhalten nur mit Wake
  Word und Stopp-Wort (FR-016) und muss es damit schaffen.
- **SC-006**: „Stopp“ bricht eine laufende Runde innerhalb einer Sekunde ab, unabhängig davon, was der
  Agent gerade tut.
- **SC-007**: Ein Ablauf aus drei Schritten mit einer Rückfrage und zwei Freigaben (wie in US3/US4)
  lässt sich vollständig per Sprache erledigen, ohne den Bildschirm zu berühren.
- **SC-008**: In keinem Test gelangt ein vorgelesenes Geheimnis in die Modell-Eingabe, die
  Mitschrift, das Protokoll oder an einen externen Dienst.
- **SC-009**: Keine Aktion, die eine Freigabe braucht, wird ohne eindeutige mündliche Zustimmung
  ausgeführt; Stille führt in allen Tests zu keiner Ausführung, unklare Antworten nach einer
  Nachfrage zu „abgelehnt“.
- **SC-010**: Im Zustand „bereit“ belegt das Zuhören auf dem Referenz-Desktop im Mittel weniger als
  5 % eines Prozessorkerns. Auf dem Referenz-Telefon verbraucht eine Stunde Zuhören höchstens 3 % des
  Akkus.
- **SC-011**: Ein eigenes Wake Word lässt sich in unter 2 Minuten einlernen und ausprobieren.
- **SC-012**: Vor dem Wake Word verlässt in keinem Test Audio oder Text das Gerät, und nichts davon
  wird gespeichert.
- **SC-013**: Eine Aufgabe, die der Nutzer mit „gib mir Bescheid“ übergibt, meldet haex in allen
  Tests von selbst als fertig oder mit ihrer Rückfrage, ohne dass der Nutzer erneut das Wake Word
  sagt; aktives Beenden bricht sie in allen Tests ab.
- **SC-014**: Testpersonen ordnen nach einmaligem Anhören mindestens 4 von 5 Kerntönen („Hört zu“,
  „Verstanden“, „Frage“, „Fertig“, „Fehler“) richtig zu.
- **SC-015**: In einem Testgespräch mit Freigabe, Rückfrage, Stopp und Dazwischenreden erscheint
  jede Äußerung und jede Ausgabe in der Mitschrift, in richtiger Reihenfolge, mit Zeitstempel und
  Art; das Geheimnis eines vorgelesenen Eintrags erscheint nicht.
- **SC-016**: Eine eigene Stimme lässt sich in unter 2 Minuten anlegen und anhören; 3 von 5
  Testpersonen erkennen in der Probe die nachgebildete Person.
- **SC-017**: Für jede der sechs Sprachen beurteilen mindestens 4 von 5 Muttersprachlern die
  Standardstimme als ohne fremden Akzent und gut verständlich.

## Assumptions

- **Kein neuer Agent**: Der Assistent ist eine Sprachschicht über dem eingebauten Agenten (003/032)
  bzw. über dem Orchestrator aus 068, sobald es ihn gibt.
  Wie gut Aufgaben gelingen, hängt vom gewählten Modell ab, wie im Chat. Planungs- oder
  Autonomie-Fähigkeiten über 003/032 hinaus gehören nicht in diese Spec.
- **Phasenfolge**: Die Spec wird vorab geschrieben. Umgesetzt wird sie erst, wenn ihre
  Voraussetzungen (032, die Spracherkennung aus 008/010) im täglichen Gebrauch sind.
- **Keine Sprechererkennung**: Der Betreiber hat „alles per Sprache“ gewählt. Wer das Wake Word sagt,
  darf alles, was der Nutzer per Sprache darf; FR-037 weist darauf hin. Eine Bindung an die Stimme des
  Nutzers kann eine spätere Spec ergänzen.
- **Kein Sprechen von sich aus**: haex beginnt nie selbst ein Gespräch (etwa für Erinnerungen oder
  Benachrichtigungen). Die einzige Ausnahme sind Meldungen zu Aufgaben, die der Nutzer im laufenden
  Gespräch angestoßen hat (FR-040). Vorschläge gibt es nur innerhalb eines Gesprächs.
- **Keine Abstimmung zwischen Geräten**: Wer auf mehreren Geräten den Modus einschaltet, bekommt
  mehrere Antworten. Eine Abstimmung darüber, welches Gerät antwortet, ist eine Folge-Spec.
- **Externe Agenten (021) und Erweiterungen** sprechen nicht über den Assistent-Modus; er gehört dem
  eingebauten Agenten.
- **Sprachen**: haex spricht und versteht sechs Sprachen (FR-052); die Oberfläche bleibt Deutsch und
  Englisch. Weitere Sprachen ergeben sich später aus Erkennungsmodellen und importierten Stimmen.
- **Nachbilden fremder Stimmen**: haex kann nicht prüfen, ob die Einwilligung wirklich vorliegt; die
  Verantwortung liegt beim Nutzer, der sie bestätigt (FR-050).
- **Referenzgeräte** für SC-002 und SC-010 werden im Plan festgelegt (ein Mittelklasse-Desktop ohne
  dedizierte Grafikkarte und ein Mittelklasse-Android-Telefon).
- **Musik und andere Audioausgaben** werden während der Sprachausgabe nicht leiser gestellt; das ist
  eine mögliche Erweiterung.
