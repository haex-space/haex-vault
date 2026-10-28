# Feature Specification: Meeting-Mitschrift mit Sprecherzuordnung

**Feature Branch**: `067-meeting-transcription`

**Created**: 2026-10-28

**Status**: Draft

**Input**: User description: „alle gespräche sollten zudem transkribiert und in der db gespeichert
werden, so dass man sich alles später auch nochmal durchlesen und nachvollziehen kann. weiterhin wäre
es super, wenn wir das hinkriegen, bei transkription, verschiedene stimmen unterscheiden zu können.
ein use case wäre zb. das aufzeichnen eines meetings. dann soll einfach alles aufgezeichnet werden,
was gerade im meeting gesagt wird und im besten fall richtig zugeordnet werden können“ (Betreiber,
2026-10-28). Der erste Teil (vollständige Mitschrift der Assistent-Gespräche) steht in Spec 066
FR-047/FR-048; diese Spec deckt den Meeting-Teil ab.

## Clarifications

### Session 2026-10-28

- Q: Eigene Spec oder Teil von 066? → A: Eigene Spec 067; sie baut auf der Audio- und
  Sprachgrundlage von 066 auf.
- Q: Wie werden Sprecher benannt? → A: haex trennt die Stimmen als „Sprecher 1, 2, …“; der Nutzer
  benennt sie im Meeting oder danach, auch mit Kontakten aus dem Adressbuch. Nur die eigene Stimme des
  Nutzers erkennt haex automatisch als „Ich“. Stimmprofile fremder Personen gibt es nicht.
- Q: Was passiert mit dem Audio? → A: Standard: nur die Mitschrift wird gespeichert, das Audio
  verworfen. Pro Meeting wählbar: Audio verschlüsselt im Vault behalten, um Stellen nachzuhören und
  Zuordnungen zu korrigieren.
- Q: Wirkt der Agent mit? → A: Live und danach: während des Meetings per Wake Word Fragen stellen
  („Haex, was hat Jonas eben zum Budget gesagt?“), danach Zusammenfassung, Beschlüsse, Aufgaben und
  Fragen zum Meeting.
- Q: Werden Online-Meetings auf demselben Gerät direkt mitgeschnitten, auch mit Kopfhörern? → A: Ja,
  auf dem Desktop nimmt haex zusätzlich den Ton des Geräts auf; die eigene Stimme kommt vom Mikrofon,
  die anderen vom Geräteton. Auf Android und iOS nur das Mikrofon, weil die Plattformen den Ton von
  Konferenz-Apps nicht herausgeben.

## Begriffe

- **Meeting**: eine Aufzeichnung, die der Nutzer startet und beendet. Sie hat Titel, Beginn, Ende,
  Teilnehmer, Mitschrift und optional Audio und Auswertung.
- **Mitschrift**: die zeitlich geordneten Einträge eines Meetings: wer was wann gesagt hat. Sie nutzt
  die Mitschrift aus 066 (FR-047) und erweitert sie um Teilnehmer.
- **Teilnehmer**: eine im Meeting erkannte Stimme. Anfangs „Sprecher 1, 2, …“, vom Nutzer benennbar,
  optional mit einem Kontakt verknüpft. „Ich“ ist der Teilnehmer mit der Stimme des Nutzers.
- **Eigenes Stimmprofil**: ein aus Aufnahmen des Nutzers berechnetes Muster seiner Stimme, mit dem
  haex „Ich“ erkennt. Es gibt nur dieses eine Profil; fremde Stimmen werden nie gespeichert.
- **Vorläufige Zuordnung**: die Sprecherzuordnung während des Meetings. Nach dem Ende prüft haex die
  ganze Aufnahme noch einmal und legt die **endgültige Zuordnung** fest.
- **Auswertung**: vom Agenten erstellte Zusammenfassung, Beschlüsse und Aufgaben eines Meetings.
- **Quelle**: woher ein Meeting Ton bekommt: nur Mikrofon (Raum) oder Mikrofon und Geräteton (der
  Ton, den das Gerät abspielt, etwa ein Online-Meeting).
- **Einwilligung**: die Bestätigung des Nutzers vor dem Start, dass alle Teilnehmer der Aufzeichnung
  zugestimmt haben.

## Beziehung zu bestehenden Specs

- Spec 066 (Voice Assistant) liefert die dauerhafte Aufnahme, die
  Pausenerkennung, die Erkennung pro Äußerung, das Wake Word, die schwebende Anzeige, den
  Android-Vordergrund-Dienst und die Mitschrift (FR-047/FR-048). Fragen per Wake Word während eines
  Meetings sind normale Assistent-Gespräche aus 066, mit dem Meeting als Kontext.
- [`008-voice-control-stt`](../008-voice-control-stt/spec.md) und
  [`010-stt-model-choice`](../010-stt-model-choice/spec.md): Spracherkennung und Modellwahl. 008
  FR-020 (Audio nie speichern) gilt weiter, außer der Nutzer wählt für ein Meeting „Audio behalten“
  (FR-014 dieser Spec).
- [`051-phone-contacts`](../051-phone-contacts/spec.md): Teilnehmer lassen sich mit Kontakten aus dem
  Adressbuch verknüpfen.
- [`003-agent-tool-loop`](../003-agent-tool-loop/spec.md) und
  [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md): Auswertung und Fragen laufen über
  den eingebauten Agenten mit seinen Werkzeugen und Freigaben.
- Spec 068 (Task Orchestration): Auswertung, Fragen im Meeting-Chat und übergebene Aufgaben laufen
  über den Orchestrator, sobald es ihn gibt; die Auswertung eines
  langen Meetings ist dann eine Aufgabe, die er einer Stufe zuordnet.
- [`006-chat-history-management`](../006-chat-history-management/spec.md): Fragen zu einem Meeting
  sind eine Chat-Unterhaltung, die zum Meeting gehört.
- [`024-own-device-sync`](../024-own-device-sync/spec.md): Meetings, Mitschrift, Teilnehmer und
  Auswertung sind Vault-Daten und kommen auf alle eigenen Geräte.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Meeting aufzeichnen und live mitlesen (Priority: P1)

Anna sitzt mit drei Kollegen im Besprechungsraum. Sie öffnet in haex „Meeting aufzeichnen“, bestätigt,
dass alle einverstanden sind, und startet. Während gesprochen wird, erscheint die Mitschrift live:
„Sprecher 1: Wir sollten das Budget …“, „Ich: Einverstanden, aber …“. Nach einer Stunde beendet sie
das Meeting. Die Mitschrift ist gespeichert und in der Meeting-Liste zu finden.

**Why this priority**: Das ist der Kern: alles, was im Meeting gesagt wird, steht danach lesbar und
mit Sprechern in haex.

**Independent Test**: Eine Testaufnahme mit drei Stimmen und dem Nutzer abspielen und als Meeting
aufzeichnen. Die Mitschrift erscheint live, unterscheidet die Stimmen, zeigt die eigene als „Ich“ und
steht nach dem Ende in der Meeting-Liste.

**Acceptance Scenarios**:

1. **Given** haex ist offen, **When** der Nutzer im Launcher „Meeting aufzeichnen“ wählt, **Then**
   fragt haex einmal pro Meeting nach der Einwilligung aller Teilnehmer und startet erst danach.
2. **Given** ein laufendes Meeting, **When** jemand spricht, **Then** erscheint der Text nach kurzer
   Zeit in der Mitschrift, mit Uhrzeit und einem Sprecher.
3. **Given** der Nutzer spricht, **When** sein Stimmprofil eingerichtet ist, **Then** steht sein Text
   unter „Ich“.
4. **Given** mehrere andere Personen sprechen, **When** ihre Stimmen unterschiedlich klingen, **Then**
   werden sie als „Sprecher 1“, „Sprecher 2“ … getrennt geführt.
5. **Given** ein laufendes Meeting, **When** der Nutzer pausiert, **Then** wird nichts erkannt oder
   gespeichert, bis er fortsetzt; die Pause steht in der Mitschrift.
6. **Given** ein laufendes Meeting, **When** der Nutzer es beendet, **Then** speichert haex die
   Mitschrift, legt die endgültige Zuordnung fest und zeigt das Meeting in der Liste mit Titel, Datum,
   Dauer und Teilnehmern.
7. **Given** ein Meeting läuft, **When** die Assistent-Anzeige oder das Tray sichtbar ist, **Then**
   zeigen sie dauerhaft „Meeting wird aufgezeichnet“ mit Dauer und Knopf zum Beenden.
8. **Given** der Vault wird gesperrt oder haex beendet, **When** ein Meeting läuft, **Then** endet es
   sofort, und die Mitschrift bis dahin bleibt gespeichert.
9. **Given** ein Meeting ist beendet, **When** die endgültige Zuordnung von der vorläufigen abweicht,
   **Then** zeigt die Mitschrift die endgültige und weist einmal darauf hin, dass sich Zuordnungen
   verbessert haben.
10. **Given** ein Online-Meeting am Desktop mit Kopfhörern und Quelle „Mikrofon und Geräteton“,
    **When** die anderen Teilnehmer sprechen, **Then** stehen ihre Sätze in der Mitschrift, getrennt
    nach Stimmen, und die Sätze des Nutzers kommen vom Mikrofon und stehen unter „Ich“.

---

### User Story 2 - Sprecher benennen und korrigieren (Priority: P1)

Während des Meetings tippt Anna auf „Sprecher 1“ und wählt „Jonas Weber“ aus dem Adressbuch; alle
seine Einträge heißen ab sofort so. Nach dem Meeting sieht sie, dass ein Satz von Mia fälschlich bei
Jonas steht, und schiebt ihn zu Mia. Zwei Sprecher, die in Wahrheit dieselbe Person waren, führt sie
zusammen.

**Why this priority**: Eine anonyme Zuordnung reicht selten; erst mit Namen ist die Mitschrift
nachvollziehbar. Weil die Erkennung Fehler macht, müssen Korrekturen einfach sein.

**Independent Test**: In einer Testaufnahme einen Sprecher benennen, einen Kontakt verknüpfen, einen
Eintrag umhängen und zwei Sprecher zusammenführen; alle Änderungen wirken auf die ganze Mitschrift
und bleiben nach Neustart erhalten.

**Acceptance Scenarios**:

1. **Given** ein Meeting (laufend oder beendet), **When** der Nutzer einen Sprecher umbenennt oder mit
   einem Kontakt verknüpft, **Then** tragen alle Einträge dieses Sprechers den neuen Namen.
2. **Given** ein Eintrag mit falschem Sprecher, **When** der Nutzer ihn einem anderen Teilnehmer
   zuordnet, **Then** wechselt nur dieser Eintrag; ist Audio vorhanden, kann er die Stelle vorher
   anhören.
3. **Given** zwei Sprecher derselben Person, **When** der Nutzer sie zusammenführt, **Then** gibt es
   danach einen Teilnehmer mit allen Einträgen beider.
4. **Given** ein Eintrag, der zwei Personen vermischt, **When** der Nutzer ihn an einer Stelle teilt,
   **Then** entstehen zwei Einträge, die er getrennt zuordnen kann.
5. **Given** ein Erkennungsfehler im Text, **When** der Nutzer den Text bearbeitet, **Then** wird er
   gespeichert und als „bearbeitet“ gekennzeichnet; der ursprüngliche Text bleibt abrufbar.
6. **Given** „Jonas Weber“ wurde in diesem Meeting benannt, **When** er im nächsten Meeting spricht,
   **Then** erscheint er dort wieder als „Sprecher N“, weil haex fremde Stimmen nicht speichert.
7. **Given** ein Teilnehmer ist mit einem Kontakt verknüpft, **When** der Nutzer den Kontakt öffnet,
   **Then** sieht er dort die Meetings, an denen die Person teilnahm.

---

### User Story 3 - Eigene Stimme einrichten (Priority: P2)

Bevor Anna das erste Meeting aufzeichnet, liest sie in den Einstellungen drei kurze Sätze vor. Ab dann
erscheint ihr eigener Text in jedem Meeting unter „Ich“.

**Why this priority**: „Ich“ ist der häufigste Sprecher und der einzige, den haex wiedererkennen darf.
Ohne Profil funktioniert alles, nur steht der Nutzer als „Sprecher N“ da.

**Independent Test**: Profil einrichten, eine Testaufnahme mit der Stimme des Nutzers und zwei
anderen aufzeichnen; die Einträge des Nutzers stehen unter „Ich“. Profil löschen; im nächsten Meeting
ist er „Sprecher N“.

**Acceptance Scenarios**:

1. **Given** die Meeting-Einstellungen, **When** der Nutzer „Eigene Stimme einrichten“ wählt, **Then**
   führt haex ihn durch das Vorlesen weniger kurzer Sätze und zeigt den Fortschritt.
2. **Given** die Aufnahmen sind fertig, **When** haex das Profil berechnet hat, **Then** sind die
   Aufnahmen verworfen; gespeichert ist nur das Profil.
3. **Given** ein Profil, **When** der Nutzer es löscht, **Then** ist es auf allen eigenen Geräten weg,
   und bestehende Mitschriften bleiben unverändert.
4. **Given** kein Profil, **When** ein Meeting beginnt, **Then** weist haex einmal darauf hin, dass
   die eigene Stimme sonst als „Sprecher N“ erscheint, und bietet die Einrichtung an.
5. **Given** im Meeting erscheint der Nutzer trotzdem als „Sprecher N“, **When** er diesen Sprecher als
   „Ich“ markiert, **Then** wird er wie jede andere Benennung behandelt; das Profil ändert sich nicht.

---

### User Story 4 - Audio behalten und nachhören (Priority: P2)

Für ein wichtiges Kundengespräch schaltet Anna vor dem Start „Audio behalten“ ein. Später tippt sie
auf einen Eintrag und hört genau diese Stelle; so prüft sie, wer das gesagt hat. Nach einem Monat
löscht sie das Audio und behält nur die Mitschrift.

**Why this priority**: Nachhören macht Korrekturen verlässlich, kostet aber Speicher und ist
datenschutzrechtlich heikler; deshalb nur auf Wunsch.

**Independent Test**: Ein Meeting mit „Audio behalten“ aufzeichnen; ein Eintrag spielt seine Stelle
ab; „Audio löschen“ entfernt es, die Mitschrift bleibt. Ein Meeting ohne die Option hat kein Audio.

**Acceptance Scenarios**:

1. **Given** der Start eines Meetings, **When** der Nutzer „Audio behalten“ nicht wählt (Standard),
   **Then** wird kein Audio gespeichert, nur die Mitschrift.
2. **Given** „Audio behalten“ ist an, **When** das Meeting läuft, **Then** speichert haex das Audio
   verschlüsselt im Vault, und die Anzeige kennzeichnet das.
3. **Given** ein Meeting mit Audio, **When** der Nutzer auf einen Eintrag tippt, **Then** hört er
   genau diese Stelle; ein Player erlaubt Sprünge im ganzen Meeting.
4. **Given** ein Meeting mit Audio, **When** der Nutzer „Audio löschen“ wählt, **Then** ist das Audio
   auf allen Geräten weg, die Mitschrift bleibt.
5. **Given** ein Meeting ohne Audio, **When** es beendet ist, **Then** lässt sich Audio nicht
   nachträglich hinzufügen.
6. **Given** ein Meeting mit Audio, **When** der Nutzer die Zuordnung nachträglich neu berechnen
   lässt, **Then** nutzt haex das Audio dafür und behält seine eigenen Korrekturen.

---

### User Story 5 - Den Assistenten während des Meetings fragen (Priority: P2)

Mitten im Meeting fragt Anna: „Haex, was hat Jonas eben zum Budget gesagt?“ haex antwortet kurz aus
der laufenden Mitschrift. Frage und Antwort stehen in der Mitschrift, als Assistent-Gespräch
gekennzeichnet.

**Why this priority**: Der Betreiber will den Agenten live nutzen. Es baut auf US1 und dem Wake Word
aus 066 auf.

**Independent Test**: Während eines aufgezeichneten Testmeetings das Wake Word und eine Frage zum
bisher Gesagten sprechen; die Antwort stimmt mit der Mitschrift überein und ist dort gekennzeichnet.

**Acceptance Scenarios**:

1. **Given** ein laufendes Meeting und der Assistent-Modus (066) ist an, **When** jemand das Wake
   Word sagt, **Then** beginnt ein Assistent-Gespräch, das die bisherige Mitschrift des Meetings als
   Kontext hat.
2. **Given** eine Frage zum Meeting, **When** der Agent antwortet, **Then** stützt er sich nur auf
   die Mitschrift und nennt bei Zitaten den Sprecher.
3. **Given** die Meeting-Einstellung „Antworten im Meeting“, **When** sie auf „sprechen“ steht
   (Standard), **Then** spricht haex die Antwort; **When** sie auf „nur anzeigen“ steht, **Then**
   erscheint die Antwort nur in der Mitschrift und der Anzeige.
4. **Given** ein Assistent-Gespräch im Meeting, **When** es läuft, **Then** zeichnet die Mitschrift
   Frage und Antwort als Assistent-Einträge auf und unterbricht die Aufzeichnung nicht.
5. **Given** der Assistent-Modus ist aus, **When** ein Meeting läuft, **Then** löst das Wake Word
   nichts aus; Fragen per Klick in der Anzeige („Fragen“) gehen trotzdem.

---

### User Story 6 - Auswertung nach dem Meeting (Priority: P2)

Nach dem Meeting tippt Anna auf „Auswerten“. haex erstellt eine Zusammenfassung, die Beschlüsse und
die Aufgaben mit Verantwortlichen. Sie fragt danach im Meeting-Chat: „Wann wollte Mia das Angebot
schicken?“ und bekommt die Antwort mit Verweis auf die Stelle.

**Why this priority**: Die Auswertung ist der große Nutzen einer Mitschrift, setzt aber eine fertige
Mitschrift voraus.

**Independent Test**: Für eine Testmitschrift mit zwei Beschlüssen und drei Aufgaben „Auswerten“
wählen; Zusammenfassung, beide Beschlüsse und alle drei Aufgaben mit richtigem Verantwortlichen
erscheinen und verweisen auf ihre Stellen.

**Acceptance Scenarios**:

1. **Given** ein beendetes Meeting, **When** der Nutzer „Auswerten“ wählt, **Then** erstellt der
   Agent Zusammenfassung, Beschlüsse und Aufgaben (mit Verantwortlichem und Termin, wenn genannt) und
   speichert sie beim Meeting.
2. **Given** eine Auswertung, **When** der Nutzer einen Punkt antippt, **Then** springt die Mitschrift
   zur Stelle, auf die er sich stützt.
3. **Given** ein beendetes Meeting, **When** der Nutzer im Meeting-Chat eine Frage stellt, **Then**
   antwortet der Agent aus der Mitschrift, mit Verweis auf die Stellen.
4. **Given** Aufgaben in der Auswertung, **When** der Nutzer bei einer „Übernehmen“ wählt, **Then**
   bearbeitet der Agent das wie eine Anweisung (etwa eine Mail oder einen Termin), mit den normalen
   Freigaben.
5. **Given** der Nutzer ändert Benennungen oder Text, **When** er erneut „Auswerten“ wählt, **Then**
   ersetzt die neue Auswertung die alte; die alte bleibt als frühere Fassung abrufbar.

---

### User Story 7 - Meetings wiederfinden und exportieren (Priority: P3)

Anna sucht nach „Angebot Müller“ und findet das Meeting vom letzten Dienstag samt Stelle. Sie
exportiert die Mitschrift mit Auswertung als Markdown-Datei.

**Why this priority**: Nützlich, sobald es viele Meetings gibt; für den Kern nicht nötig.

**Independent Test**: In mehreren Testmeetings nach einem Wort suchen; Treffer zeigen Meeting und
Stelle. Export erzeugt eine Datei mit Sprechern, Uhrzeiten und Auswertung.

**Acceptance Scenarios**:

1. **Given** mehrere Meetings, **When** der Nutzer in der Meeting-Liste sucht, **Then** findet er
   Treffer in Titeln, Teilnehmern, Mitschriften und Auswertungen, mit Sprung zur Stelle.
2. **Given** ein Meeting, **When** der Nutzer „Exportieren“ wählt, **Then** erhält er eine
   Markdown-Datei mit Titel, Datum, Teilnehmern, Mitschrift (Uhrzeit, Sprecher, Text) und, falls
   vorhanden, Auswertung.
3. **Given** ein Meeting, **When** der Nutzer es löscht, **Then** sind Mitschrift, Teilnehmer,
   Audio, Auswertung und der Meeting-Chat auf allen Geräten weg.

---

### Edge Cases

- **Online-Meeting über Lautsprecher**: Das Mikrofon hört den Geräteton mit. haex erkennt das und
  schreibt jede Äußerung nur einmal mit, aus dem Geräteton.
- **Andere Töne des Geräts** (Benachrichtigungen, Musik): Sie sind Teil des Gerätetons; ohne Sprache
  erzeugen sie keine Einträge. Der Hinweis beim Start nennt das.
- **Online-Meeting auf dem Telefon**: Nur das Mikrofon; haex sagt beim Start, dass Teilnehmer mit
  Kopfhörern nicht zu hören sind.
- **Durcheinanderreden**: Sprechen zwei Personen gleichzeitig, ordnet haex den Abschnitt der
  lauteren Stimme zu und markiert ihn als „überlappend“.
- **Mehr als 10 Personen**: haex trennt höchstens so viele Stimmen, wie es verlässlich kann; was es
  nicht zuordnen kann, steht unter „Unbekannt“.
- **Sehr langes Meeting**: bis 4 Stunden ohne Einbußen; danach weist haex alle 30 Minuten darauf hin,
  dass das Meeting noch läuft.
- **Speicher knapp bei „Audio behalten“**: haex warnt vorher, wenn der Platz für die erwartete Dauer
  nicht reicht, und hört bei vollem Speicher auf, Audio zu behalten, ohne die Mitschrift zu
  unterbrechen.
- **Mikrofon fällt aus oder wird von einer anderen App belegt**: Die Aufzeichnung pausiert sichtbar
  und setzt von selbst fort; die Lücke steht in der Mitschrift.
- **Zweites Meeting**: Es läuft immer höchstens ein Meeting pro Gerät.
- **Assistent-Gespräch außerhalb eines Meetings**: bleibt wie in 066; nur während eines Meetings
  zeichnet haex auch Stimmen ohne Wake Word auf.
- **Meeting auf zwei eigenen Geräten zugleich**: Zwei getrennte Meetings entstehen; ein Zusammenführen
  gibt es nicht.
- **Sprache wechselt im Meeting**: Jeder Eintrag wird in seiner Sprache erkannt (Deutsch, Englisch).
- **Kontakt wird gelöscht**: Der Teilnehmer behält den Namen, die Verknüpfung entfällt.

## Requirements _(mandatory)_

### Functional Requirements

#### Aufzeichnen

- **FR-001**: Der Launcher MUST einen Eintrag „Meeting aufzeichnen“ haben; er MUST sich ins Dock
  heften lassen.
- **FR-002**: Vor jedem Start MUST haex die Einwilligung abfragen („Alle Teilnehmer sind mit der
  Aufzeichnung einverstanden“) und den Zeitpunkt der Bestätigung beim Meeting speichern. Ohne
  Bestätigung MUST NOT ein Meeting starten.
- **FR-003**: Der Nutzer MUST beim Start optional eine kurze Ansage wählen können, die haex zu Beginn
  spricht („Dieses Meeting wird von haex mitgeschrieben“).
- **FR-004**: Während eines Meetings MUST haex alles, was das Mikrofon hört, erkennen und als
  Mitschrift-Einträge mit Uhrzeit, Teilnehmer und Text speichern, ohne Wake Word.
- **FR-005**: Ein laufendes Meeting MUST dauerhaft sichtbar sein (Assistent-Anzeige, Tray,
  Android-Benachrichtigung) mit Dauer, Pause und Beenden.
- **FR-006**: Der Nutzer MUST ein Meeting pausieren, fortsetzen und beenden können; während der Pause
  MUST NOT haex etwas erkennen oder speichern. Pausen und Lücken MUST in der Mitschrift stehen.
- **FR-007**: Sperren des Vaults oder Beenden von haex MUST ein Meeting sofort beenden und das bis
  dahin Erkannte speichern.
- **FR-008**: Es MUST höchstens ein Meeting pro Gerät laufen.
- **FR-009**: Ein Meeting MUST auf Desktop und Android auch bei minimiertem Fenster bzw. im Hintergrund
  und gesperrtem Bildschirm weiterlaufen (wie 066 FR-035); auf iOS nur bei sichtbarer App, worauf haex
  beim Start hinweist.

#### Sprecher

- **FR-010**: haex MUST die Stimmen eines Meetings trennen und als „Sprecher 1, 2, …“ führen; nicht
  Zuordenbares MUST unter „Unbekannt“ stehen, Überlappungen MUST gekennzeichnet sein.
- **FR-011**: Nach dem Ende MUST haex die Zuordnung über das ganze Meeting neu bestimmen, ohne
  Benennungen und Korrekturen des Nutzers zu überschreiben, und einmal auf geänderte Zuordnungen
  hinweisen.
- **FR-012**: Der Nutzer MUST Teilnehmer jederzeit umbenennen, mit einem Kontakt verknüpfen, als „Ich“
  markieren, zusammenführen; Einträge einem anderen Teilnehmer zuordnen und teilen; Text bearbeiten
  (mit Kennzeichnung und abrufbarem Original).
- **FR-013**: haex MUST ein eigenes Stimmprofil nur für den Nutzer anbieten (Einrichtung durch
  Vorlesen weniger Sätze, Aufnahmen danach verworfen), damit erkennen und es löschen lassen. Es MUST
  mit den eigenen Geräten synchronisiert werden. Stimmprofile anderer Personen MUST NOT gespeichert
  werden, auch nicht aus Benennungen.

#### Audio

- **FR-014**: Standardmäßig MUST haex kein Audio eines Meetings speichern. Pro Meeting MUST der Nutzer
  vor dem Start „Audio behalten“ wählen können; dann MUST das Audio verschlüsselt im Vault gespeichert
  und die Anzeige entsprechend gekennzeichnet werden.
- **FR-015**: Bei behaltenem Audio MUST jeder Eintrag seine Stelle abspielen können, und ein Player
  MUST das ganze Meeting abspielen. Der Nutzer MUST das Audio eines Meetings löschen können, ohne die
  Mitschrift zu verlieren.
- **FR-016**: Vor einem Meeting mit „Audio behalten“ MUST haex warnen, wenn der freie Speicher für 4
  Stunden nicht reicht; bei vollem Speicher MUST es das Audio-Speichern beenden und die Mitschrift
  fortsetzen.

#### Agent

- **FR-017**: Während eines Meetings MUST das Wake Word aus 066 (falls der Assistent-Modus an ist)
  und „Fragen“ in der Anzeige ein Assistent-Gespräch mit der bisherigen Mitschrift als Kontext
  beginnen. Die Aufzeichnung MUST dabei weiterlaufen; Frage und Antwort MUST als Assistent-Einträge in
  der Mitschrift stehen.
- **FR-018**: Pro Meeting MUST einstellbar sein, ob haex Antworten im Meeting spricht (Standard) oder
  nur anzeigt.
- **FR-019**: Nach dem Ende MUST der Nutzer eine Auswertung anfordern können: Zusammenfassung,
  Beschlüsse, Aufgaben (Verantwortlicher und Termin, wenn genannt), jeweils mit Verweis auf die
  Stellen. Eine neue Auswertung MUST die alte ersetzen und die alte als frühere Fassung behalten.
- **FR-020**: Jedes Meeting MUST einen Meeting-Chat haben, in dem der Agent Fragen aus der Mitschrift
  beantwortet und Stellen verweist. Aufgaben aus der Auswertung MUST sich als Anweisung an den Agenten
  übergeben lassen, mit den normalen Freigaben (003/032).
- **FR-021**: Der Agent MUST die Mitschrift eines Meetings nur lesen, wenn er im Meeting-Chat, in
  einem Assistent-Gespräch während dieses Meetings oder auf ausdrückliche Bitte des Nutzers arbeitet;
  ändern MUST er sie nicht.

#### Verwalten

- **FR-022**: Eine Meeting-Liste MUST alle Meetings mit Titel (Standard: Datum und Uhrzeit,
  umbenennbar), Datum, Dauer, Teilnehmern und Kennzeichen „Audio“ zeigen.
- **FR-023**: Die Suche MUST Titel, Teilnehmer, Mitschriften und Auswertungen durchsuchen und zur
  Stelle springen.
- **FR-024**: Der Nutzer MUST ein Meeting als Markdown exportieren können (Titel, Datum, Teilnehmer,
  Mitschrift mit Uhrzeit und Sprecher, Auswertung).
- **FR-025**: Löschen eines Meetings MUST Mitschrift, Teilnehmer, Audio, Auswertung und Meeting-Chat
  auf allen eigenen Geräten entfernen.
- **FR-026**: Ein mit einem Kontakt verknüpfter Teilnehmer MUST beim Kontakt die Meetings zeigen, an
  denen er teilnahm.
- **FR-027**: Meetings, Mitschrift, Teilnehmer, Auswertung und Stimmprofil MUST Vault-Daten sein und
  mit den eigenen Geräten synchronisiert werden; Audio ebenso, solange es behalten wird.

#### Quelle

- **FR-028**: Auf Desktop-Geräten MUST der Nutzer beim Start als Quelle „Mikrofon“ oder „Mikrofon und
  Geräteton“ wählen können (Standard: Mikrofon und Geräteton, wenn gerade ein anderes Programm Ton
  abspielt, sonst Mikrofon). Mit Geräteton MUST haex Stimmen aus dem Geräteton auch dann mitschreiben,
  wenn der Nutzer Kopfhörer trägt. Was über Lautsprecher abgespielt und vom Mikrofon erneut gehört
  wird, MUST NOT doppelt in der Mitschrift stehen.
- **FR-029**: Bei „Mikrofon und Geräteton“ MUST haex Stimmen aus dem Mikrofon und aus dem Geräteton
  getrennt führen; Sprache vom Mikrofon eines Kopfhörer-Nutzers MUST ohne Stimmprofil als „Ich“
  vorgeschlagen werden. Auf Android und iOS MUST nur „Mikrofon“ angeboten werden, mit Hinweis.

### Key Entities

- **Meeting**: Titel, Beginn, Ende, Gerät, Quelle, Einwilligung (Zeitpunkt), Ansage ja/nein, Audio behalten
  ja/nein, Antworten im Meeting (sprechen / nur anzeigen), Zustand (läuft, pausiert, beendet,
  Zuordnung wird bestimmt).
- **Teilnehmer**: gehört zu einem Meeting; Bezeichnung („Sprecher N“ oder Name), optional Kontakt,
  „Ich“ ja/nein, zusammengeführt mit (Verweis).
- **Mitschrift-Eintrag**: wie in 066 (Zeitpunkt, Sprecher, Art, Text), Sprecher hier ein Teilnehmer
  oder haex; zusätzlich Ende, überlappend ja/nein, bearbeitet ja/nein mit Original, vorläufig oder
  endgültig zugeordnet, vom Nutzer zugeordnet ja/nein.
- **Meeting-Audio**: verschlüsselte Aufnahme eines Meetings, nur wenn gewählt; löschbar.
- **Auswertung**: Fassung, Zeitpunkt, Modell, Zusammenfassung, Beschlüsse, Aufgaben (Text,
  Verantwortlicher, Termin), je mit Verweisen auf Mitschrift-Einträge.
- **Eigenes Stimmprofil**: ein Muster der Stimme des Nutzers, ohne Aufnahmen; höchstens eins pro
  Vault.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Gesprochenes erscheint während des Meetings spätestens 5 Sekunden nach Ende des Satzes in
  der Mitschrift.
- **SC-002**: Ein Meeting von 4 Stunden läuft auf dem Referenz-Desktop und dem Referenz-Telefon durch,
  ohne dass die Mitschrift zurückfällt oder Einträge verloren gehen.
- **SC-003**: In einer Testaufnahme mit vier Sprechern in einem ruhigen Raum ist nach dem Ende
  mindestens 85 % der Sprechzeit dem richtigen Teilnehmer zugeordnet, während des Meetings mindestens
  75 %.
- **SC-004**: Mit eingerichtetem Profil stehen mindestens 95 % der Sprechzeit des Nutzers unter „Ich“,
  und höchstens 2 % fremder Sprechzeit fälschlich dort.
- **SC-005**: Die Wortfehlerrate der Mitschrift liegt in einem ruhigen Raum mit dem
  Standard-Erkennungsmodell bei höchstens 15 % (Deutsch und Englisch).
- **SC-006**: Ein Sprecher lässt sich in höchstens zwei Schritten benennen; ein Eintrag in höchstens
  zwei Schritten umhängen.
- **SC-007**: Eine Stunde Meeting kostet auf dem Referenz-Telefon höchstens 15 % Akku; eine Stunde
  behaltenes Audio belegt höchstens 30 MB.
- **SC-008**: Die Auswertung eines einstündigen Meetings liegt mit einem Cloud-Modell nach höchstens
  60 Sekunden vor, mit dem lokalen Standardmodell nach höchstens 5 Minuten; in einer Testmitschrift
  werden alle Beschlüsse und Aufgaben mit dem richtigen Verantwortlichen gefunden.
- **SC-009**: In keinem Test wird ohne bestätigte Einwilligung aufgezeichnet, ohne „Audio behalten“
  Audio gespeichert oder ein Stimmprofil einer anderen Person als des Nutzers angelegt.
- **SC-010**: In einem Test-Online-Meeting am Desktop mit drei entfernten Teilnehmern stehen mit
  Kopfhörern mindestens 95 % ihrer Sätze in der Mitschrift; über Lautsprecher erscheint kein Satz
  doppelt.

## Assumptions

- **Rechtliches**: Die Einwilligung fragt haex ab und speichert den Zeitpunkt; ob sie tatsächlich
  vorliegt, verantwortet der Nutzer. haex gibt keine Rechtsberatung, der Hinweistext verweist
  allgemein auf die Pflicht zur Einwilligung.
- **Kein Teilen**: Mitschriften mit Teilnehmern zu teilen gehört zu den Daten-Freigaben (028), nicht
  in diese Spec; der Export deckt den Bedarf bis dahin.
- **Kein Live-Transkript für andere**: Nur der Nutzer sieht die Mitschrift; eine Anzeige für den Raum
  (Beamer) ist nicht vorgesehen.
- **Sprachen**: Deutsch und Englisch wie in 008/010.
- **Phasenfolge**: Umsetzung erst nach 066 PR B–D (Audio-Grundlage, Gespräch, Wake Word) und für das
  Hintergrund-Aufzeichnen auf Android nach 066 PR H.
- **Referenzgeräte** wie in 066 plan.md.
- **Kein automatisches Löschen**: Meetings und Audio bleiben, bis der Nutzer sie löscht.
