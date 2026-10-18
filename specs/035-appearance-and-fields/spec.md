# Feature Specification: Darstellung und Eingabefelder

**Feature Branch**: `035-ui-foundation`
**Created**: 2026-10-02
**Status**: Draft
**Input**: holzi bekommt überall dieselben Eingabefelder im Stil von haex-vault 2.x (Legacy) (schwebendes
Label, Ring in der Primärfarbe, Fehlertext, Löschen und Kopieren) und eine Darstellung, die
der Nutzer zur Laufzeit wie in GNOME und COSMIC einstellt: Akzentfarbe aus Farbfeldern oder
eigener Farbe, Fenster- und Container-Hintergrund, Hell, Dunkel oder automatisch, Zurücksetzen
sowie Export und Import. Der Umbau des Passwortmanagers (Tabs, Verlauf, Passkeys, Navigation)
ist ausdrücklich nicht Teil dieser Spec, sondern Spec 036. Die Felder kommen aus der gemeinsamen
Oberflächenschicht haex-ui (Pull Request haex-space/haextension#65, gepinnt auf `2dcb8bc`).

## Beziehung zu bestehenden Specs

- [`023-settings-app`](../023-settings-app/spec.md): Die Einstellungen kennen schon Hell,
  Dunkel und System als ein Wert für die ganze Vault (FR-013, FR-014, FR-024) und messen den
  Kontrast beider Schemata. Diese Spec erweitert diese Gruppe zu „Darstellung“ und ersetzt
  die Auswahl nicht, sondern baut darauf auf. Die Regel „Auswahl wird gespeichert, keine
  Speichern-Knöpfe“ gilt weiter.
- [`024-own-device-sync`](../024-own-device-sync/spec.md): Wie das Farbschema geht die
  gesamte Darstellung als Vault-Einstellung über den Sync der eigenen Geräte.
- [`034-password-manager`](../034-password-manager/spec.md): Der Passwortmanager ist der
  größte Nutzer der Felder. Seine Oberfläche wird hier nur auf die neuen Felder umgestellt,
  nicht umgebaut.
- Geplante Spec **036 (Passwortmanager-Oberfläche)** baut auf den Feldern und der
  einstellbaren Akzentfarbe auf, etwa für die Auswahlleiste.

## Clarifications

### Session 2026-10-02

- Q: Gelten Fenster- und Container-Hintergrund getrennt je Schema oder als Tönung für beide? → A: Eine Wahl, die als Tönung in beiden Schemata gilt; Hell bleibt hell, Dunkel bleibt dunkel. Der Umfang der Regler folgt dem Dialog „Aussehen“ von COSMIC (Akzent, Fensterhintergrund, Container-Hintergrund, Texttönung, Komponententönung, Hinweis für das aktive Fenster, alles auf Standard zurücksetzbar, Import und Export). Wird das zu aufwändig, darf der Plan Texttönung, Komponententönung und den Fensterhinweis als eigene spätere Stufe abtrennen.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Eingabefelder sehen überall gleich aus (Priority: P1)

Wer in holzi etwas eintippt, sieht überall dasselbe Feld: Das Label liegt in der Ruhe im
Feld, schwebt bei Fokus oder Inhalt über den Rand, der Fokusring ist in der Primärfarbe, ein
Fehler steht als Text unter dem Feld, und wo es sinnvoll ist, gibt es Löschen und Kopieren.
Das gilt für Passwortmanager, Einstellungen, Chat, Modelle und Einrichtung, für einzeilige
Felder, Passwortfelder, mehrzeilige Felder und Auswahllisten.

**Why this priority**: Das ist der Anlass der Spec und der Teil, den der Nutzer in jeder App
von holzi sofort sieht. Ohne ihn ist die Darstellung (Story 2) nur Farbe auf alten Feldern.

**Independent Test**: In jedem Bereich ein Feld anklicken, tippen, leeren, Fehler auslösen und
die Auswahlliste öffnen. Delivers value allein, weil die Felder auch mit der festen
Standardfarbe einheitlich sind.

**Acceptance Scenarios**:

1. **Given** ein leeres Feld mit Label, **When** der Nutzer hineinklickt, **Then** schwebt
   das Label über den Rand und der Ring erscheint in der Primärfarbe.
2. **Given** ein Feld mit Inhalt, **When** es den Fokus verliert, **Then** bleibt das Label
   oben und das Feld liest sich unverändert.
3. **Given** ein Feld mit Fehler, **When** der Fehler angezeigt wird, **Then** steht der Text
   unter dem Feld, der Rand ist als Fehler erkennbar und ein Screenreader liest den Fehler
   zum Feld vor.
4. **Given** ein Feld in einer Einstellungsgruppe mit abgesetztem Hintergrund, **When** das
   Label schwebt, **Then** unterbricht es den Rand sauber, ohne einen anders getönten Kasten
   zu zeigen.
5. **Given** ein Passwortfeld, **When** der Nutzer es anzeigt, kopiert oder leert, **Then**
   verhalten sich Anzeigen, Kopieren und Leeren wie vor der Umstellung (inklusive der
   zeitgesteuerten Zwischenablage).
6. **Given** eine Auswahlliste mit Wert, **When** der Nutzer sie öffnet und wählt, **Then**
   zeigt sie Label und Wert und die Auswahl wird wie bisher gespeichert.

---

### User Story 2 - Akzentfarbe und Hintergrund wählen (Priority: P1)

In den Einstellungen unter „Darstellung“ wählt der Nutzer die Akzentfarbe aus einer Reihe
von Farbfeldern oder legt über „+“ eine eigene Farbe fest, und stellt wie in COSMIC
Fensterhintergrund, Container-Hintergrund, Texttönung und Komponententönung ein. Die Auswahl
wirkt sofort in der ganzen App und bleibt, ohne dass der Nutzer etwas speichert. Schalter,
Knöpfe, Fokusringe, Auswahlmarkierungen, Links und alle Flächen folgen der Wahl.

**Why this priority**: Das ist der zweite Teil des Auftrags und der Grund, warum die Primärfarbe
nicht fest im Layer stehen darf.

**Independent Test**: Eine Farbe wählen und prüfen, dass Ring, Schalter, Knöpfe und
Hintergründe in Fenster und Seitenleisten die neue Farbe zeigen; die App neu starten und
prüfen, dass sie bleibt.

**Acceptance Scenarios**:

1. **Given** die Standardfarbe, **When** der Nutzer ein Farbfeld wählt, **Then** wechseln
   Fokusring, Schalter, Knöpfe und Auswahlmarkierungen in allen offenen Fenstern sofort
   auf diese Farbe, ohne dass etwas neu geladen wird.
2. **Given** die Auswahl, **When** der Nutzer „+“ öffnet und eine eigene Farbe festlegt,
   **Then** gilt diese Farbe sofort und steht danach als eigenes Feld in der Reihe.
3. **Given** eine gewählte Farbe, **When** die App neu gestartet oder eine andere Vault
   geöffnet wird und wieder diese, **Then** gilt die Wahl weiter und es gibt keinen
   Speichern-Knopf.
4. **Given** zwei Geräte derselben Vault, **When** auf einem die Farbe geändert wird,
   **Then** wechselt das andere Gerät nach dem nächsten Sync ohne Neustart.
5. **Given** der Nutzer wählt Fenster- oder Container-Hintergrund, **When** die Wahl
   getroffen ist, **Then** ändern sich nur die jeweiligen Flächen (Fenster, Seitenleisten
   und Listen als Container) und der Text darauf bleibt lesbar.
6. **Given** der Nutzer ändert Texttönung oder Komponententönung, **When** die Wahl getroffen
   ist, **Then** ändern sich die Textfarben der Oberfläche beziehungsweise die Hintergründe
   von Schaltflächen, Suchfeldern und Eingabefeldern, und alles bleibt lesbar.
7. **Given** der Nutzer schaltet „Akzentfarbe als Hinweis für das aktive Fenster“ ein,
   **When** er zwischen Fenstern des Window Managers (wm) wechselt, **Then** trägt nur das
   aktive Fenster eine Umrandung in der Akzentfarbe.

---

### User Story 3 - Hell, Dunkel oder automatisch (Priority: P1)

Der Nutzer wählt Hell, Dunkel oder „Automatisch“. Bei „Automatisch“ folgt holzi dem System
und wechselt mit ihm, auch während die App läuft. Akzent- und Hintergrundwahl gelten in
beiden Schemata.

**Why this priority**: Die Auswahl gibt es seit 023; sie muss mit der neuen Darstellung
zusammenspielen und bleibt Teil derselben Gruppe.

**Independent Test**: Schema umschalten und das System-Schema ändern; die App folgt nur bei
„Automatisch“.

**Acceptance Scenarios**:

1. **Given** „Automatisch“, **When** das System von Hell auf Dunkel wechselt, **Then**
   wechselt holzi ohne Neustart.
2. **Given** „Hell“ oder „Dunkel“, **When** das System das Schema wechselt, **Then** bleibt
   holzi bei der Wahl.
3. **Given** eine gewählte Akzentfarbe, **When** das Schema wechselt, **Then** bleibt die
   Akzentfarbe, wird aber, wo es der Kontrast verlangt, in der Helligkeit angepasst
   (FR-016).

---

### User Story 4 - Lesbar bleiben und zurücksetzen (Priority: P2)

Der Nutzer kann eine Farbe wählen, bei der Text oder Bedienelemente schwer lesbar würden.
holzi verhindert das: Beschriftung auf Knöpfen und Auswahlleisten wechselt automatisch
zwischen hell und dunkel, und eine Wahl, die die Mindestkontraste nicht einhalten kann,
wird auf den nächsten erlaubten Wert angepasst und der Nutzer erfährt das. „Auf Standard zurücksetzen“ bringt
Akzent und Hintergründe zurück.

**Why this priority**: Verhindert, dass eine Geschmacksentscheidung die App unbenutzbar macht.

**Independent Test**: Sehr helle, sehr dunkle und schwach gesättigte Farben wählen und
Kontraste messen; zurücksetzen und die Standardwerte prüfen.

**Acceptance Scenarios**:

1. **Given** eine sehr helle Akzentfarbe, **When** sie gewählt wird, **Then** ist die
   Schrift auf Knöpfen und Auswahlleisten lesbar (mindestens 4,5:1) und Fokusring und Schalter
   erreichen mindestens 3:1 gegen ihren Untergrund.
2. **Given** ein Hintergrund, der gegen den Text weniger als 4,5:1 hätte, **When** er
   gewählt wird, **Then** wird der nächste erlaubte Ton angezeigt, die Wahl bleibt
   gespeichert und ergibt bei jedem Öffnen denselben angepassten Ton, und der Nutzer sieht,
   dass und warum angepasst wurde.
3. **Given** eine geänderte Darstellung, **When** der Nutzer auf Standard zurücksetzt und
   bestätigt, **Then** gelten wieder alle Standardwerte der Darstellung und das Schema
   bleibt unverändert.

---

### User Story 5 - Darstellung exportieren und importieren (Priority: P3)

Der Nutzer speichert seine Darstellung (Akzent, eigene Farben, Hintergründe, Schema) in
einer Datei und lädt sie später oder auf einem anderen Rechner wieder.

**Why this priority**: Bequem und aus GNOME und COSMIC bekannt, aber nichts hängt davon ab.

**Independent Test**: Exportieren, zurücksetzen, die Datei importieren und die gleiche
Darstellung wiederfinden; eine kaputte Datei importieren und eine verständliche Meldung sehen.

**Acceptance Scenarios**:

1. **Given** eine eingestellte Darstellung, **When** der Nutzer sie exportiert und nach dem
   Zurücksetzen importiert, **Then** ist die Darstellung wieder identisch.
2. **Given** eine Datei, die keine Darstellung ist oder unbekannte oder unlesbare Werte
   enthält, **When** der Nutzer sie importiert, **Then** ändert sich nichts und die Meldung
   sagt, was nicht stimmt.
3. **Given** eine Datei mit einem gültigen und einem ungültigen Wert, **When** sie
   importiert wird, **Then** gilt entweder alles oder nichts, nie die Hälfte.

---

### Edge Cases

- Vor dem Öffnen der Vault gibt es noch keine gespeicherte Darstellung: Es gelten
  Standardfarben und das System-Schema; nach dem Öffnen wird die gespeicherte Darstellung
  ohne sichtbares Aufblitzen der Standardfarben angewandt, soweit das möglich ist.
- Beim Wechsel der Vault zeigt holzi nie die Darstellung der vorherigen Vault.
- Ein gespeicherter Wert, der kein gültiger Farbwert ist (nach einem Sync-Fehler oder per
  Hand verändert), zählt als nicht gesetzt und der Standard gilt.
- Zwei Geräte ändern die Darstellung gleichzeitig: Es gewinnt wie bei allen
  Vault-Einstellungen die jüngere Änderung; beide Geräte zeigen danach dieselbe Darstellung.
  Die Darstellung zählt dabei als eine Einstellung: Ändert ein Gerät den Akzent und das
  andere zugleich einen Hintergrund, bleibt nur die jüngere der beiden Änderungen.
- Eine eigene Farbe wird eingegeben, während der Nutzer noch tippt (halbe Eingabe wie
  „#12“): Es gilt weiter die letzte gültige Farbe, es wird nichts gespeichert.
- Felder ohne Label (zum Beispiel in einer Suchzeile oder einer Tabellenzelle) bleiben
  möglich und behalten Platzhalter, Ring und Fehlerzustand.
- Sehr lange Labels und schmale Fenster (360 Pixel): Das Label wird abgeschnitten, nicht
  umgebrochen, und das Feld bleibt bedienbar.
- Ein Feld ist gesperrt (schreibgeschützt oder deaktiviert): Es sieht gesperrt aus,
  Kopieren bleibt möglich, Löschen nicht.
- Tastatur, Screenreader und Reduzierte Bewegung: Das Label erreicht sein Ziel ohne
  Animation, wenn der Nutzer reduzierte Bewegung eingestellt hat.

## Requirements _(mandatory)_

### Functional Requirements

**Eingabefelder**

- **FR-001**: Alle Eingabefelder in holzi MÜSSEN dieselbe Feldart zeigen: einzeilige Felder,
  Passwortfelder, mehrzeilige Felder und Auswahllisten in Passwortmanager, Einstellungen,
  Chat, Modellverwaltung, Einrichtung und Verknüpfung von Geräten. Kein Bereich darf eine
  eigene, abweichende Feldart behalten, außer ein Feld hat einen dokumentierten Grund
  (zum Beispiel das Eingabefeld des Chats mit seiner wachsenden Höhe), der in der
  Beschreibung des Pull Requests steht.
- **FR-002**: Ein Feld MUSS ein Label tragen können, das in der Ruhe im Feld liegt und bei
  Fokus oder Inhalt über den Rand schwebt; ohne Label verhält sich das Feld wie ein
  gewöhnliches Feld mit Platzhalter.
- **FR-003**: Der Fokus eines Feldes MUSS einen Ring in der aktuellen Akzentfarbe zeigen
  und dabei den Rand in dieser Farbe färben.
- **FR-004**: Ein Feld MUSS einen Fehler anzeigen können: Text unter dem Feld, Rand als
  Fehler erkennbar, und der Fehler MUSS für Hilfstechnik mit dem Feld verbunden sein.
- **FR-005**: Felder MÜSSEN Löschen (leert den Inhalt) und Kopieren (legt den Inhalt in die
  Zwischenablage) anbieten können, wo es zum Feld passt; gesperrte Felder bieten nur
  Kopieren.
- **FR-006**: Das Label eines Feldes MUSS auf einer abgesetzten Fläche (Einstellungsgruppen,
  Dialoge, Karten) mit dem Hintergrund dieser Fläche unterlegt werden können, sodass es den
  Rand sauber unterbricht.
- **FR-007**: Die Umstellung MUSS das Verhalten der Felder unverändert lassen: Eingaben,
  Pflichtfelder, Tastaturbedienung (Enter, Escape, Tab), Fokusreihenfolge, automatische
  Fokusvergabe beim Öffnen von Dialogen, Anzeigen und Verbergen von Passwörtern, und die
  Kennungen der Felder, an denen sich die automatischen Tests orientieren.
- **FR-008**: Eine Auswahlliste MUSS Label, Wert, Fehler und Sperrzustand wie die anderen
  Felder zeigen; das Label MUSS schweben, sobald die Liste offen ist oder einen Wert hat.
- **FR-009**: Die Beschriftungen der Hilfsknöpfe (Löschen, Kopieren, Anzeigen, Verbergen)
  MÜSSEN aus den Sprachdateien von holzi kommen, auf Deutsch und Englisch.
- **FR-010**: Eine Prüfung im Projekt MUSS verhindern, dass neue Eingabefelder wieder in
  der alten Feldart entstehen (zum Beispiel eine statische Suche, die jede Verwendung der
  alten Feldarten außerhalb eines Positivlisten-Eintrags meldet).

**Darstellung**

- **FR-011**: Die Einstellungen MUSS eine Gruppe „Darstellung“ haben, die das bestehende
  Farbschema aufnimmt und um Akzentfarbe, Fensterhintergrund, Container-Hintergrund,
  Texttönung, Komponententönung und den Fensterhinweis (FR-024) erweitert, in der
  Reihenfolge und Art des Dialogs „Aussehen“ von COSMIC; sie MUSS wie die anderen Einstellungen
  ohne Speichern- und Übernehmen-Knopf auskommen (eine Auswahl wird gespeichert); ein Knopf
  „Auf Standard zurücksetzen“ gehört zur Gruppe (FR-017).
- **FR-012**: Der Nutzer MUSS die Akzentfarbe aus einer Reihe vordefinierter Farbfelder
  wählen oder über „+“ eine eigene Farbe festlegen können; die Reihe MUSS die gewählte eigene
  Farbe als weiteres Feld zeigen.
- **FR-013**: Der Nutzer MUSS Fensterhintergrund, Container-Hintergrund, Texttönung und
  Komponententönung wählen können, aus einer kleinen Reihe vordefinierter Töne (Standard und
  mehrere Alternativen) und per eigener Farbe. „Fenster“ ist die Fläche hinter allem,
  „Container“ sind Seitenleisten, Listenboxen, Karten und Dialoge, die „Texttönung“ ist die
  Grundlage, aus der die Textfarben der Oberfläche abgeleitet werden, und die
  „Komponententönung“ liefert die Hintergründe von Schaltflächen, Suchfeldern und
  Eingabefeldern. Jede dieser Wahlen ist eine Tönung (Farbton und Stärke), die auf die
  Standardflächen des jeweiligen Schemas wirkt und in Hell und Dunkel gilt; Hell bleibt
  dadurch hell und Dunkel dunkel.
- **FR-014**: Eine Änderung der Darstellung MUSS sofort in der ganzen App wirken, in allen
  offenen Fenstern und Tabs, ohne neu zu laden.
- **FR-015**: Die Primärfarbe MUSS dort wirken, wo holzi heute die Primärfarbe zeigt:
  Fokusringe, Schalter, Primärknöpfe, Auswahlmarkierungen, Links, aktive Einträge in
  Seitenleisten und Hinweise auf den aktiven Zustand; die Standardfarbe bleibt das heutige
  Blaugrün.
- **FR-016**: holzi MUSS Schrift auf der Akzentfarbe automatisch hell oder dunkel wählen
  und die Akzentfarbe in Hell und Dunkel so anpassen, dass Text auf Flächen mit der Farbe
  mindestens 4,5:1 und Bedienelemente (Fokusring, Schalterspur, Hinweisumrandung des aktiven
  Fensters) gegen ihren Untergrund mindestens 3:1 erreichen. Reine Zierränder von Flächen
  fallen nicht darunter.
- **FR-017**: Eine Wahl von Fenster- oder Container-Hintergrund, Texttönung oder
  Komponententönung, die den Normaltext oder den gedämpften Text unter 4,5:1 oder Bedienelemente
  unter 3:1 brächte, MUSS auf den nächsten erlaubten Wert angepasst werden
  (nicht stillschweigend abgelehnt), und der Nutzer MUSS sehen, dass und warum. „Auf
  Standard zurücksetzen“ MUSS alle Werte der Darstellung (Akzent, beide Hintergründe, beide
  Tönungen, Fensterhinweis, eigene Farben) zurückbringen und das Schema unverändert lassen;
  es MUSS erst nach einer Bestätigung wirken.
- **FR-018**: Das Farbschema (Hell, Dunkel, Automatisch) MUSS bleiben wie in 023 (Automatisch
  folgt dem System live), und Akzent, Hintergründe und Tönungen MÜSSEN in beiden Schemata gelten.
- **FR-019**: Die Darstellung (Schema, Akzent, eigene Farben, Hintergründe, Tönungen,
  Fensterhinweis) MUSS als
  Vault-Einstellung gespeichert werden und damit über den Sync der eigenen Geräte auf alle
  Geräte der Vault gehen, wie das Farbschema seit 023; vor dem Öffnen der Vault gilt der
  Standard.
- **FR-020**: Ungültige gespeicherte Werte MÜSSEN wie nicht gesetzt behandelt werden.
- **FR-021**: Der Nutzer MUSS die Darstellung in eine Datei exportieren und aus einer Datei
  importieren können; ein Import MUSS die Datei vollständig prüfen und entweder alles oder
  nichts übernehmen (nie nur einen Teil), und bei Fehlern die Ursache nennen. Die Datei
  enthält nur Darstellungswerte (keine Vault-Daten, keine Pfade, keine Namen).
- **FR-022**: Die Kontrastprüfung der Einstellungen aus 023 (`settings-color-scheme`) MUSS
  die Darstellung abdecken: Der Kontrast wird für Standard, für jedes vordefinierte Farbfeld
  und für die Extremwerte einer eigenen Farbe in beiden Schemata gemessen. Die vollständige
  Matrix darf ohne laufende App gerechnet werden, solange sie dieselbe Farbrechnung benutzt
  wie die App; die Messung in der laufenden App bestätigt mindestens den Standard.
- **FR-023**: Alle Texte der Darstellung MÜSSEN Deutsch und Englisch haben, und die
  Einstellungen sagen „Sitzung“ und „Window Manager (wm)“ statt „Layout“ oder „Shell“.
- **FR-024**: Der Nutzer MUSS einschalten können, dass das aktive Fenster des Window
  Managers (wm) eine Umrandung in der Akzentfarbe trägt (Standard: aus, das heutige
  Aussehen); nur ein Fenster trägt sie zur gleichen Zeit, und sie erreicht gegen den
  Untergrund mindestens 3:1.

### Key Entities _(include if feature involves data)_

- **Darstellung**: Die Gesamtheit der Einstellungen der Oberfläche einer Vault: Schema
  (hell, dunkel, automatisch), Akzentfarbe, Fensterhintergrund, Container-Hintergrund,
  Texttönung, Komponententönung, Fensterhinweis und die vom Nutzer angelegten eigenen
  Farben.
- **Farbfeld**: Eine benannte oder eigene Farbe, die als Akzent oder Hintergrund wählbar ist.
- **Darstellungsdatei**: Das Austauschformat für Export und Import; enthält nur die Werte
  der Darstellung und eine Versionsangabe.
- **Feld**: Das gemeinsame Eingabeelement mit Label, Fokusring, Fehler, Löschen und Kopieren
  in den Varianten einzeilig, Passwort, mehrzeilig und Auswahlliste.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In allen Bereichen von holzi (Passwortmanager, Einstellungen, Chat, Modelle,
  Einrichtung, Geräteverknüpfung) zeigt jedes Eingabefeld Label, Fokusring und Fehlerzustand
  gleich; eine Durchsicht aller Bereiche findet kein Feld in der alten Feldart, außer den
  dokumentierten Ausnahmen.
- **SC-002**: Alle bestehenden automatischen Prüfungen für Passwortmanager, Einstellungen,
  Chat und Einrichtung laufen nach der Umstellung unverändert durch; es ist kein Verhalten
  verloren gegangen.
- **SC-003**: Ein Nutzer ändert die Akzentfarbe und sieht die neue Farbe in einem offenen
  Fenster sofort, und in 100 % der Fälle nach einem Neustart weiterhin.
- **SC-004**: Für Standard, jedes Farbfeld und die Extremwerte einer eigenen Farbe erreichen
  Text mindestens 4,5:1 und Bedienelemente mindestens 3:1, in Hell und Dunkel.
- **SC-005**: Eine geänderte Darstellung steht nach dem Sync innerhalb der üblichen
  Sync-Zeit auch auf dem zweiten Gerät, ohne dass dort jemand etwas tut.
- **SC-006**: Export und Import einer Darstellung ergeben denselben Zustand; eine
  beschädigte Datei verändert die Darstellung in keinem Fall.
- **SC-007**: Ein Nutzer ohne Vorwissen findet die Darstellung in den Einstellungen und
  stellt eine Farbe in unter 30 Sekunden ein.

## Assumptions

- Die Darstellung gilt für die ganze Vault und geht wie das Farbschema über den Sync
  (FR-019). Das weicht vom ersten Vorschlag „pro Gerät“ ab, weil 023 das Farbschema schon
  als eine Vault-Einstellung führt und zwei Orte für „Darstellung“ verwirren würden;
  Geräte mit anderen Bildschirmen stellen weiter per „Automatisch“ das System-Schema ein.
- Die Feldarten kommen aus der Oberflächenschicht haex-ui im Stand `2dcb8bc`; holzi ändert
  diese Schicht nicht, sondern wünscht Änderungen im Repo haex-space/haextension.
- Akzente und Töne sind je eine kleine feste Reihe (acht bis zwölf Akzente, fünf bis sieben
  Töne je Regler) plus eigene Farbe; die genaue Liste legt der Plan fest.
- Schriftgrößen, Schriftarten, Dichte der Oberfläche und Stil (Eckenradius, Abstände wie
  „Stil“ in COSMIC) sind nicht Teil dieser Spec.
- Der Umfang folgt dem Dialog „Aussehen“ von COSMIC; ist das zu aufwändig, trennt der Plan
  Texttönung, Komponententönung und den Fensterhinweis (FR-013, FR-024) als spätere Stufe ab,
  ohne die Stufe mit Akzent, Hintergründen, Schema, Zurücksetzen, Export und Import zu
  verzögern.
- Der Chat darf sein mehrzeiliges Eingabefeld mit eigenem Verhalten behalten, wenn die
  gemeinsame Feldart es nicht abbilden kann (Begründung im Pull Request, FR-001).
- Die Darstellung wirkt auf holzi selbst. Erweiterungen im Window Manager (wm) mit eigener
  Oberfläche folgen der Farbe nur, soweit sie die Farben der Oberflächenschicht benutzen.
- Der Nutzer legt eigene Farben als Teil der Darstellung an; es gibt keine Bibliothek
  geteilter Designs.
