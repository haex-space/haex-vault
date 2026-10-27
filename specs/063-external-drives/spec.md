# Feature Specification: Externe Laufwerke im Dateibrowser

**Feature Branch**: `063-external-drives`

**Created**: 2026-10-27

**Status**: Draft

**Input**: User description: "ich möchte im dateiexplorer auch gerne mounted devices sehen, wie externe
festplatten, usb sticks etc" (Betreiber, 2026-10-27). Abgestimmtes Design:
[docs/plans/2026-10-27-external-drives-design.md](../../docs/plans/2026-10-27-external-drives-design.md).

## Clarifications

### Session 2026-10-27

- Q: Wie weit geht haex bei externen Laufwerken? → A: Wie Nautilus oder Dolphin: auch nicht
  eingehängte Laufwerke zeigen und per Klick einhängen, die Liste live halten, Name und Art zeigen,
  auswerfen.
- Q: Welche Plattformen? → A: Linux, macOS und Windows vollständig. Android aktualisiert die Liste nur
  live, weil Apps dort Laufwerke nicht einhängen oder auswerfen dürfen.
- Q: Was dürfen andere eigene Geräte und Agents? → A: Sie sehen eingehängte Laufwerke wie heute.
  Einhängen, Entsperren und Auswerfen macht nur der Nutzer im Fenster dieses Geräts.
- Q: Was ist mit verschlüsselten Laufwerken? → A: Sie lassen sich in haex entsperren. Das Passwort
  kann aus dem Passwortmanager kommen und dort gespeichert werden, wenn der Nutzer das will.
- Q: Wie findet haex das Passwort eines Laufwerks? → A: Ein normaler Eintrag im Passwortmanager, dessen
  URL `haex:drive:<Volume-UUID>` ist, mit dem Tag `haex-drive`. Es gibt keine eigene Verknüpfung
  daneben; neue Bezeichner heißen `haex`, nicht `holzi`.
- Q: Was passiert mit einem von haex entsperrten Laufwerk, wenn der Vault sperrt oder haex endet? →
  A: Nichts. Es bleibt entsperrt und eingehängt, bis der Nutzer es auswirft oder abzieht; beim
  Auswerfen sperrt haex den verschlüsselten Container immer wieder.
- Q: Lassen sich eingehängte interne Partitionen in haex wieder aushängen? → A: Ja. Jedes eingehängte
  Laufwerk außer den Systemorten (`/`, `/home`, `/boot` und dergleichen) bekommt einen Knopf:
  „Auswerfen“ (aushängen und ausschalten) bei Wechseldatenträgern, „Aushängen“ bei internen.
- Q: Was zeigt ein wiederhergestellter Tab, dessen Laufwerk steckt, aber nicht eingehängt oder
  gesperrt ist? → A: „Laufwerk <Name> ist nicht eingehängt“ mit „Einhängen“ bzw. „Entsperren“; danach
  öffnet er den gemerkten Ordner. Fehlt das Laufwerk ganz, springt er wie in 044 zum nächsten
  vorhandenen Ordner. Ungefragt eingehängt wird nichts.

## Begriffe

- **Laufwerk**: ein Dateisystem, das der Dateibrowser in der Seitenleiste unter „Laufwerke“ zeigt: das
  Systemlaufwerk, interne Partitionen und Wechseldatenträger (USB-Sticks, externe Festplatten,
  SD-Karten). Ein Stick mit mehreren Partitionen ergibt mehrere Laufwerke.
- **Art**: System, intern oder wechselbar. Sie bestimmt Symbol und Reihenfolge.
- **Zustand**: eingehängt (Inhalt erreichbar), nicht eingehängt (vorhanden, Inhalt noch nicht
  erreichbar) oder gesperrt (verschlüsselt und noch nicht entsperrt).
- **Volume-UUID**: die Kennung, die ein verschlüsseltes Laufwerk selbst trägt. Sie bleibt gleich, egal
  an welchem Gerät oder Anschluss es steckt.
- **Laufwerks-Eintrag**: ein Eintrag im Passwortmanager (Spec 034), dessen URL genau
  `haex:drive:<Volume-UUID>` ist (UUID kleingeschrieben). Er ist ein normaler Eintrag mit normalen
  Freigaben.

## Beziehung zu bestehenden Specs

- **Spec 044** (Dateibrowser): erweitert FR-002 (Laufwerke als Quelle) und die Seitenleiste. Der
  Randfall „Ein Laufwerk wird entfernt“ und das Springen zum nächsten vorhandenen Ordner bleiben.
- **Spec 056** (Dateizugriff zwischen eigenen Geräten): Andere eigene Geräte sehen weiterhin nur
  eingehängte Laufwerke; Einhängen, Entsperren und Auswerfen aus der Ferne gibt es nicht.
- **Spec 034** (Passwortmanager) und **Spec 052** (Zugangsdaten aus dem Passwortmanager wählen):
  Laufwerks-Einträge folgen dem Muster von 052 (vorhandenen Eintrag wählen oder neuen anlegen, Tag als
  Konvention). Tags bleiben gewöhnliche Tags.
- **Spec 024** (Sync): Laufwerks-Einträge sind Vault-Daten und kommen auf alle eigenen Geräte.
- **Spec 022** (Sitzung wiederherstellen): Ein Tab merkt sich zusätzlich das Laufwerk seines Ordners
  (FR-025).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Eingesteckten Stick sehen und öffnen (Priority: P1)

Anna hat den Dateibrowser offen und steckt einen USB-Stick ein. Ohne das Fenster neu zu öffnen,
erscheint er in der Seitenleiste unter „Laufwerke“ mit seinem Namen („FOTOS“) und einem USB-Symbol.
Hat das System ihn nicht selbst eingehängt, klickt sie ihn an; haex hängt ihn ein und zeigt seinen
Inhalt. Zieht sie den Stick ab, verschwindet er wieder aus der Liste.

**Why this priority**: Das ist die eigentliche Lücke: Heute sieht man nur Laufwerke, die beim Öffnen
des Fensters schon eingehängt waren.

**Independent Test**: Bei offenem Dateibrowser einen Stick einstecken, der nicht automatisch eingehängt
wird; er erscheint mit Namen; ein Klick zeigt seinen Inhalt. Abziehen entfernt ihn aus der Liste.

**Acceptance Scenarios**:

1. **Given** ein offener Dateibrowser, **When** ein Stick eingesteckt wird, **Then** erscheint er ohne
   Zutun des Nutzers in der Seitenleiste, mit seinem Namen und als wechselbar gekennzeichnet.
2. **Given** ein nicht eingehängtes Laufwerk in der Seitenleiste, **When** der Nutzer es anklickt,
   **Then** hängt haex es ein, zeigt dabei einen Fortschritt und öffnet danach seinen Inhalt.
3. **Given** das System fragt beim Einhängen nach einer Berechtigung, **When** der Nutzer die Abfrage
   abbricht, **Then** bleibt das Laufwerk nicht eingehängt und haex zeigt einen kurzen Hinweis, keinen
   Fehlerdialog.
4. **Given** ein Laufwerk ohne Namen, **When** es erscheint, **Then** trägt es einen Namen aus Art und
   Größe, etwa „Datenträger (32 GB)“.
5. **Given** ein Stick mit zwei Partitionen, **When** er eingesteckt wird, **Then** erscheinen zwei
   Laufwerke, jedes mit seinem Namen.
6. **Given** ein interner Rechner, **When** der Dateibrowser öffnet, **Then** erscheinen
   Systempartitionen wie EFI, Wiederherstellung oder Auslagerung nicht.
7. **Given** ein Tab zeigt einen Ordner auf einem Stick, **When** der Stick abgezogen wird, **Then**
   verschwindet er aus der Liste und der Tab springt zum nächsten vorhandenen Ordner (044).
8. **Given** ein Tab stand vor dem Neustart in einem Ordner auf einem Stick, der noch steckt, aber
   nicht eingehängt ist, **When** haex die Sitzung wiederherstellt, **Then** zeigt der Tab „Laufwerk
   <Name> ist nicht eingehängt“ mit „Einhängen“ (bei gesperrtem Laufwerk „Entsperren“); nach dem
   Klick öffnet er den gemerkten Ordner, auch wenn das System das Laufwerk an einem anderen Ort
   einhängt.
9. **Given** Android, **When** ein OTG-Stick eingesteckt wird, **Then** erscheint er ohne Neuöffnen in
   der Liste; Einhängen und Auswerfen bietet haex dort nicht an.

---

### User Story 2 - Laufwerk sicher auswerfen (Priority: P2)

Anna ist fertig und klickt neben dem Stick auf „Auswerfen“. haex hängt ihn aus und meldet, dass sie
ihn abziehen kann. Hat sie gerade noch ein Video davon offen, schließt haex das selbst; läuft noch eine
Übertragung auf den Stick, fragt haex erst.

**Why this priority**: Ohne Auswerfen muss man für jeden Stick wieder in den Dateimanager des Systems,
und Abziehen ohne Auswerfen riskiert Datenverlust.

**Independent Test**: Einen eingehängten Stick im Dateibrowser öffnen, eine Datei davon im Viewer
ansehen, dann „Auswerfen“; das gelingt beim ersten Versuch, und der Stick ist danach nicht mehr
eingehängt.

**Acceptance Scenarios**:

1. **Given** ein eingehängter Wechseldatenträger, **When** der Nutzer „Auswerfen“ wählt, **Then** ist
   er danach ausgehängt, verschwindet aus der Liste (oder wird als nicht eingehängt gezeigt, solange er
   steckt) und haex meldet, dass er abgezogen werden kann.
2. **Given** haex selbst hält etwas auf dem Laufwerk (offener Ordner, Viewer, Suche), **When** der
   Nutzer auswirft, **Then** gibt haex das vorher frei und das Auswerfen gelingt.
3. **Given** eine laufende Übertragung von oder zu dem Laufwerk, **When** der Nutzer auswirft, **Then**
   fragt haex „Übertragung abbrechen und auswerfen?“ und wirft nur nach Bestätigung aus.
4. **Given** ein anderes Programm hält das Laufwerk, **When** der Nutzer auswirft, **Then** bleibt es
   eingehängt und haex sagt, dass es noch von einem anderen Programm benutzt wird.
5. **Given** das Systemlaufwerk oder ein anderer Systemort, **When** der Nutzer die Seitenleiste
   ansieht, **Then** gibt es dort weder „Auswerfen“ noch „Aushängen“.
6. **Given** eine eingehängte interne Partition, die kein Systemort ist, **When** der Nutzer
   „Aushängen“ wählt, **Then** ist sie danach ausgehängt und bleibt als nicht eingehängt in der Liste.

---

### User Story 3 - Verschlüsseltes Laufwerk entsperren (Priority: P2)

Anna hat eine verschlüsselte Backup-Festplatte. Sie erscheint mit einem Schloss. Ein Klick öffnet den
Dialog „Laufwerk entsperren“; Anna gibt das Passwort ein, haex entsperrt und hängt die Platte ein und
zeigt ihren Inhalt.

**Why this priority**: Ohne Entsperren bleiben verschlüsselte Platten in haex unbrauchbar; Backups sind
häufig verschlüsselt.

**Independent Test**: Ein verschlüsseltes Laufwerk anschließen, im Dateibrowser anklicken, Passwort
eingeben; der Inhalt erscheint. Ein falsches Passwort lässt den Dialog mit Fehlermeldung offen.

**Acceptance Scenarios**:

1. **Given** ein gesperrtes Laufwerk ohne Laufwerks-Eintrag, **When** der Nutzer es anklickt, **Then**
   öffnet sich der Dialog „Laufwerk entsperren“.
2. **Given** der Dialog, **When** der Nutzer das richtige Passwort eingibt, **Then** entsperrt haex das
   Laufwerk, hängt es ein und öffnet seinen Inhalt.
3. **Given** der Dialog, **When** das Passwort falsch ist, **Then** bleibt der Dialog offen und zeigt
   den Fehler.
4. **Given** ein entsperrtes Laufwerk, dessen Inhalt kein Dateisystem ist, das haex lesen kann (etwa
   ein Volume-Verbund), **When** es entsperrt ist, **Then** sagt haex, dass dieser Inhalt nicht
   unterstützt wird.
5. **Given** ein System, das einer App das Entsperren mit Passwort nicht erlaubt, **When** der Nutzer
   ein gesperrtes Laufwerk anklickt, **Then** öffnet haex den Entsperrdialog des Systems; danach
   erscheint das Laufwerk wie jedes andere eingehängte.

---

### User Story 4 - Passwort aus dem Passwortmanager nehmen und merken (Priority: P3)

Anna will das Passwort ihrer Backup-Platte nicht jedes Mal tippen. Im Dialog setzt sie „Im
Passwortmanager merken“. Nach dem Entsperren liegt ein Eintrag „Laufwerk: Backup“ im Passwortmanager.
Beim nächsten Mal, auch an ihrem Laptop, genügt ein Klick auf die Platte. Liegt das Passwort schon in
einem älteren Eintrag, wählt sie diesen im Dialog statt zu tippen.

**Why this priority**: Komfort auf Basis von Story 3; ohne ihn funktioniert Entsperren schon.

**Independent Test**: Laufwerk mit „merken“ entsperren, auswerfen, wieder anklicken: es öffnet ohne
Dialog. Auf einem zweiten eigenen Gerät nach dem Sync dasselbe.

**Acceptance Scenarios**:

1. **Given** genau ein Laufwerks-Eintrag passt zur Volume-UUID, **When** der Nutzer das gesperrte
   Laufwerk anklickt, **Then** entsperrt haex es ohne Dialog.
2. **Given** das gemerkte Passwort passt nicht mehr, **When** haex damit entsperren will, **Then**
   öffnet sich der Dialog mit dem Hinweis, dass das gespeicherte Passwort nicht mehr passt.
3. **Given** mehrere Laufwerks-Einträge passen, **When** der Nutzer das Laufwerk anklickt, **Then**
   zeigt der Dialog diese Einträge zur Auswahl.
4. **Given** der Dialog mit „Im Passwortmanager merken“ und einem getippten Passwort, **When** das
   Entsperren gelingt, **Then** entsteht ein normaler Eintrag mit Titel „Laufwerk: <Name>“, der URL
   `haex:drive:<Volume-UUID>` und dem Tag `haex-drive`.
5. **Given** derselbe Dialog, **When** das Entsperren scheitert, **Then** entsteht kein Eintrag.
6. **Given** der Nutzer wählt im Dialog einen vorhandenen Eintrag ohne URL und setzt „merken“, **When**
   das Entsperren gelingt, **Then** trägt haex die Laufwerks-URL in diesen Eintrag ein.
7. **Given** der gewählte Eintrag hat schon eine andere URL, **When** der Nutzer „merken“ setzt,
   **Then** fragt haex, ob die URL ersetzt werden soll; ohne Zustimmung nutzt haex den Eintrag nur
   dieses eine Mal.
8. **Given** kein Vault ist offen, **When** der Dialog erscheint, **Then** funktioniert das
   Passwortfeld, der Passwortmanager-Teil ist ausgegraut.

---

### Edge Cases

- **Kein Laufwerksdienst des Systems vorhanden** (minimale Linux-Installation, Sandbox): haex zeigt wie
  heute nur die eingehängten Laufwerke, ohne Einhängen und Auswerfen.
- **Unformatierte Partition**: erscheint nicht.
- **Einhängen scheitert aus anderem Grund** (Dateisystem beschädigt, Windows im Ruhezustand): haex
  zeigt den Grund, den das System nennt; das Laufwerk bleibt nicht eingehängt.
- **Read-only-Datenträger** (etwa ein Installationsstick): erscheint und lässt sich öffnen; Ändern
  schlägt wie bei jedem schreibgeschützten Ordner fehl.
- **Stick wird während des Einhängens abgezogen**: Das Einhängen bricht mit einem Hinweis ab; der
  Eintrag verschwindet.
- **Andere eigene Geräte**: sehen ein Laufwerk erst, wenn es auf diesem Gerät eingehängt ist; ein
  gesperrtes oder nicht eingehängtes sehen sie nicht.
- **Agents**: sehen eingehängte Laufwerke wie jeden anderen Pfad (044); sie können nichts einhängen,
  entsperren oder auswerfen.
- **Mehrere Dateibrowser-Fenster**: Alle zeigen dieselbe Liste; ein Auswerfen in einem Fenster wirkt in
  allen.
- **Laufwerks-Eintrag gelöscht**: Beim nächsten Klick öffnet sich der Dialog.
- **Vault sperrt oder haex endet**: Entsperrte und eingehängte Laufwerke bleiben es; sie gehören dem
  System, nicht dem Vault.
- **Nutzer trägt die URL `haex:drive:<UUID>` selbst in einen Eintrag ein**: Der Eintrag gilt wie ein
  gemerkter.

## Requirements _(mandatory)_

### Functional Requirements

**Liste**

- **FR-001**: Die Seitenleiste MUSS unter „Laufwerke“ das Systemlaufwerk, interne Partitionen und alle
  Wechseldatenträger zeigen, eingehängt oder nicht. Partitionen, die das System als Systempartition
  oder als zu ignorieren kennzeichnet, und unformatierte Partitionen MÜSSEN fehlen.
- **FR-002**: Jedes Laufwerk MUSS seinen Namen (Bezeichnung des Datenträgers, sonst „Datenträger
  (<Größe>)“), ein Symbol nach Art und bei eingehängten den freien Platz zeigen, wo bekannt. Gesperrte
  Laufwerke MÜSSEN ein Schloss zeigen.
- **FR-003**: Die Reihenfolge MUSS System, intern, wechselbar sein.
- **FR-004**: Solange ein Dateibrowser-Fenster offen ist, MUSS die Liste Einstecken, Abziehen,
  Einhängen und Aushängen ohne Zutun des Nutzers innerhalb weniger Sekunden zeigen, auch wenn ein
  anderes Programm die Änderung ausgelöst hat. Ohne offenes Fenster MUSS haex dafür nichts tun.
- **FR-005**: Jede Partition eines Datenträgers MUSS ein eigenes Laufwerk sein.

**Einhängen**

- **FR-006**: Ein Klick auf ein nicht eingehängtes Laufwerk MUSS es einhängen, einen Fortschritt zeigen
  und danach seinen Inhalt öffnen. Das Einhängen MUSS ohne Administratorrechte von haex auskommen;
  verlangt das System eine Bestätigung, fragt das System selbst.
- **FR-007**: Ein abgebrochenes oder verweigertes Einhängen MUSS einen kurzen Hinweis geben und das
  Laufwerk nicht eingehängt lassen.

**Auswerfen**

- **FR-008**: Eingehängte Wechseldatenträger MÜSSEN einen „Auswerfen“-Knopf haben, eingehängte interne
  Laufwerke einen „Aushängen“-Knopf. Systemorte (`/`, `/home`, `/boot` und dergleichen) DÜRFEN keinen
  der beiden haben. FR-009 bis FR-012 gelten für beide; ausgeschaltet werden nur Wechseldatenträger.
- **FR-009**: Vor dem Aushängen MUSS haex alles freigeben, was es selbst auf dem Laufwerk offen hält
  (Ordnerbeobachtung, Viewer, Suche).
- **FR-010**: Läuft eine Übertragung von oder zu dem Laufwerk, MUSS haex vor dem Auswerfen fragen und
  die Übertragung nur nach Bestätigung abbrechen.
- **FR-011**: Nach erfolgreichem Auswerfen MUSS haex melden, dass der Datenträger abgezogen werden
  kann; wo das System es erlaubt, MUSS haex den Datenträger auch ausschalten. Ein verschlüsseltes
  Laufwerk MUSS beim Auswerfen wieder gesperrt werden.
- **FR-012**: Hält ein anderes Programm das Laufwerk, MUSS haex das sagen und das Laufwerk eingehängt
  lassen.

**Entsperren**

- **FR-013**: Ein Klick auf ein gesperrtes Laufwerk MUSS nach Laufwerks-Einträgen mit genau der URL
  `haex:drive:<Volume-UUID>` suchen. Bei genau einem Treffer MUSS haex ohne Dialog entsperren, bei
  keinem oder mehreren den Dialog „Laufwerk entsperren“ öffnen (bei mehreren mit diesen zur Auswahl).
- **FR-014**: Der Dialog MUSS ein Passwortfeld und „Aus Passwortmanager wählen“ anbieten sowie die
  Option „Im Passwortmanager merken“ (Vorgabe: aus).
- **FR-015**: Nach erfolgreichem Entsperren MUSS haex das Laufwerk einhängen und seinen Inhalt öffnen.
  Ein falsches Passwort MUSS den Dialog mit Fehlermeldung offen lassen; scheitert ein gemerktes
  Passwort, MUSS der Dialog sagen, dass es nicht mehr passt.
- **FR-016**: Gemerkt werden DARF erst nach erfolgreichem Entsperren:
  - getipptes Passwort: neuer normaler Eintrag, Titel „Laufwerk: <Name>“, URL
    `haex:drive:<Volume-UUID>`, Tag `haex-drive`;
  - gewählter Eintrag ohne URL: haex setzt die URL;
  - gewählter Eintrag mit anderer URL: haex ersetzt sie nur nach Zustimmung, sonst bleibt es bei einer
    einmaligen Nutzung.
- **FR-017**: Das Passwort DARF nur an den Entsperrdienst des Systems gehen. Es DARF nicht protokolliert,
  nicht als Programmargument übergeben (in Prozesslisten sichtbar) und nach dem Entsperren nicht in der
  Oberfläche gehalten werden.
- **FR-018**: Ohne offenen Vault MUSS das Passwortfeld funktionieren; der Passwortmanager-Teil MUSS
  ausgegraut sein.
- **FR-019**: Wo das System einer App das Entsperren mit Passwort nicht erlaubt, MUSS haex den
  Entsperrdialog des Systems öffnen; der Passwortmanager-Teil entfällt dort.
- **FR-020**: Enthält ein entsperrtes Laufwerk kein Dateisystem, das haex direkt öffnen kann, MUSS haex
  das sagen.

**Grenzen**

- **FR-021**: Andere eigene Geräte (056) MÜSSEN nur eingehängte Laufwerke dieses Geräts sehen. Es DARF
  kein Einhängen, Entsperren oder Auswerfen aus der Ferne geben.
- **FR-022**: Agents DÜRFEN Laufwerke weder einhängen noch entsperren noch auswerfen; eingehängte
  erreichen sie wie jeden Pfad (044).
- **FR-023**: Auf Android MUSS die Liste live sein (FR-004); Einhängen, Entsperren und Auswerfen bietet
  haex dort nicht an.
- **FR-024**: Fehlt der Laufwerksdienst des Systems, MUSS haex die eingehängten Laufwerke wie bisher
  zeigen, ohne Einhängen und Auswerfen.

**Sitzung**

- **FR-025**: Zum Ordner eines Tabs MUSS sich haex merken, auf welchem Laufwerk und wo darin er liegt.
  Steckt das Laufwerk beim Wiederherstellen (Spec 022), ist aber nicht eingehängt oder gesperrt, MUSS
  der Tab das sagen und „Einhängen“ bzw. „Entsperren“ anbieten und danach den Ordner öffnen, auch an
  einem neuen Einhängeort. Ungefragt DARF haex dabei nichts einhängen. Fehlt das Laufwerk, gilt 044.

### Key Entities

- **Laufwerk**: Kennung (stabil, solange der Datenträger steckt), Name, Art (System, intern,
  wechselbar), Zustand (eingehängt, nicht eingehängt, gesperrt), Pfad wenn eingehängt, Gesamt- und
  freier Platz wenn bekannt, ob auswerfbar oder aushängbar, Volume-UUID bei verschlüsselten. Wird nicht gespeichert,
  sondern vom System erfragt.
- **Laufwerks-Eintrag**: ein normaler Eintrag des Passwortmanagers; URL `haex:drive:<Volume-UUID>`,
  Tag `haex-drive`, Passwort. Synchronisiert wie jeder Eintrag.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein eingesteckter oder abgezogener Datenträger erscheint bzw. verschwindet in einem
  offenen Dateibrowser innerhalb von 3 Sekunden, ohne dass der Nutzer etwas tut.
- **SC-002**: Von einem nicht eingehängten Stick in der Seitenleiste zu seinem Inhalt braucht es einen
  Klick (plus eine etwaige Bestätigung des Systems).
- **SC-003**: Ein verschlüsseltes Laufwerk mit gemerktem Passwort öffnet mit einem Klick, ohne
  Passworteingabe, auf jedem eigenen Gerät nach dem Sync.
- **SC-004**: Nach Arbeit nur in haex (Ordner, Viewer, Suche) gelingt Auswerfen beim ersten Versuch in
  100 % der Fälle.
- **SC-005**: In keinem Fall landet ein falsches Passwort im Passwortmanager, und kein Passwort
  erscheint in Protokollen oder Prozesslisten.
- **SC-006**: Systempartitionen (EFI, Wiederherstellung, Auslagerung) erscheinen auf keinem der drei
  Desktop-Systeme in der Liste.

## Assumptions

- Die Desktop-Systeme bieten einen Dienst, über den eine normale App Laufwerke ohne eigene
  Administratorrechte einhängen und auswerfen kann (Linux: udisks2; macOS und Windows hängen ohnehin
  selbst ein). Ob BitLocker To Go ohne Administratorrechte mit Passwort entsperrbar ist, klärt die
  Research; sonst greift FR-019.
- Die Warnung vor dem Löschen eines Eintrags, den eine haex-Funktion nutzt (034 FR-034), gilt für
  Laufwerks-Einträge nicht: Gelöscht heißt nur, dass beim nächsten Mal der Dialog kommt.
- Volume-Verbünde (LVM), RAID und Netzlaufwerke sind nicht Teil dieser Spec; Netzlaufwerke, die das
  System eingehängt hat, erscheinen wie bisher, wenn sie unter den üblichen Orten liegen.
- Formatieren, Partitionieren und Umbenennen von Datenträgern sind nicht Teil dieser Spec.
- Ziehen aus haex heraus auf einen Stick des Systems ist durch 044 („später“) unverändert.
