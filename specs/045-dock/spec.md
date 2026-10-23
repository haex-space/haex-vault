# Feature Specification: Dock

**Feature Branch**: `045-dock`

**Created**: 2026-10-07

**Status**: Draft

**Input**: User description: "ich möchte gerne unser action menu (app launcher, workspaces, fenstermanagement) konfigurierbar machen. ich möchte daraus ein dock machen, dass der nutzer zum einen beliebig platzieren kann (unten, oben, links, rechts, in den ecken). außerdem soll der nutzer auch system apps und haextensions in das dock integrieren können, so dass häufige apps schnell geöffnet werden können" und "ich möchte neben einer einfachen leiste auch gerne eine art explosionsrad haben (als konfigurierbare option)"

Abgestimmter Entwurf: [`docs/plans/2026-10-07-dock-design.md`](../../docs/plans/2026-10-07-dock-design.md).

## Beziehung zu bestehenden Specs

- [`015-workspace-shell`](../015-workspace-shell/spec.md) FR-002, FR-011, FR-022: Launcher,
  Fensterübersicht und Arbeitsbereichs-Übersicht bleiben unverändert. Diese Spec ersetzt nur die drei
  festen Schaltflächen, die sie öffnen, durch Einträge im Dock. FR-028 (Kompaktmodus) gilt weiter;
  diese Spec legt fest, wo das Dock im Kompaktmodus steht.
- [`030-app-multi-instance`](../030-app-multi-instance/spec.md), Assumptions: Das dort ausgelagerte
  „Klick auf ein App-Symbol in einer Taskleiste fokussiert die laufende Instanz“ wird hier umgesetzt,
  aber nur im Dock. Öffnen aus dem Launcher oder über „+“ erzeugt für Mehrfachinstanz-Apps weiterhin
  immer eine neue Instanz.
- [`017-extension-host`](../017-extension-host/spec.md): haextensions erscheinen im Dock wie
  System-Apps. Diese Spec ändert nichts daran, wann eine Erweiterung als App verfügbar ist.
- [`042-settings-general-restructure`](../042-settings-general-restructure/spec.md): Die Einstellungen
  für das Dock bekommen einen eigenen Unterpunkt.
- [`043-android-build`](../043-android-build/spec.md): Auf dem Telefon gilt der Kompaktmodus; das Dock
  muss dort per Touch bedienbar sein.

## Begriffe

- **Dock**: das Bedienelement des Arbeitsbereichs, das Steuer-Einträge sowie angeheftete und laufende
  Apps zeigt. Es erscheint als **Leiste** oder als **Rad**.
- **Steuer-Eintrag**: einer der drei Einträge Launcher, Fensterübersicht, Arbeitsbereichs-Übersicht.
- **Angeheftete App**: eine App, die der Nutzer dauerhaft ins Dock gelegt hat.
- **Instanz**: eine geöffnete App, entweder in einem eigenen Fenster oder als Tab neben anderen Tabs in
  einem Fenster. Technisch ist jede Instanz ein Tab (siehe `CONTEXT.md`); ein eigenes Fenster ist ein
  Fenster mit genau diesem einen Tab.
- **Laufende App**: eine App mit mindestens einer Instanz in irgendeinem Arbeitsbereich.
- **Platzierung**: Kante (oben, unten, links, rechts) plus Ausrichtung entlang der Kante (Anfang,
  Mitte, Ende). „Unten, Ende“ ist die Ecke unten rechts.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Häufige Apps aus dem Dock starten (Priority: P1)

Als Nutzer hefte ich System-Apps und haextensions, die ich oft brauche, ans Dock und öffne sie dort mit
einem Klick, statt jedes Mal den Launcher zu öffnen. Das Dock ersetzt die drei bisherigen
Schaltflächen. Launcher, Fensterübersicht und Arbeitsbereichs-Übersicht bleiben als konfigurierbare
Steuer-Einträge verfügbar; die Standardbelegung zeigt zusätzlich die wichtigsten System-Apps.

**Why this priority**: Das ist der Kern des Wunsches und ergibt allein schon ein brauchbares Dock.

**Independent Test**: Arbeitsbereich öffnen, im Launcher eine App über das Kontextmenü anheften,
prüfen, dass sie im Dock erscheint und ein Klick sie öffnet; sie wieder lösen.

**Acceptance Scenarios**:

1. **Given** eine Vault ohne Dock-Konfiguration, **When** der Arbeitsbereich erscheint, **Then** zeigt
   das Dock unten mittig als Leiste die Einträge Launcher, Dateibrowser, Chat, Passwörter und
   Einstellungen in dieser Reihenfolge, und es gibt keine weiteren schwebenden Schaltflächen.
2. **Given** der Launcher ist offen, **When** der Nutzer eine App per Rechtsklick oder Langdruck
   „An Dock anheften“ wählt, **Then** erscheint die App als letzter angehefteter Eintrag im Dock.
3. **Given** eine angeheftete App ohne Instanz, **When** der Nutzer sie im Dock anklickt,
   **Then** öffnet sie sich wie aus dem Launcher.
4. **Given** eine angeheftete App, **When** der Nutzer im Kontextmenü ihres Dock-Eintrags „Lösen“
   wählt, **Then** verschwindet sie aus dem Dock, sofern sie nicht läuft.
5. **Given** eine installierte und aktivierte haextension, **When** der Nutzer sie anheftet, **Then**
   verhält sie sich im Dock wie eine System-App.
6. **Given** der Nutzer hat auf Gerät A eine App angeheftet, **When** die Vault auf Gerät B
   synchronisiert ist, **Then** zeigt das Dock auf Gerät B dieselbe App an derselben Stelle.

---

### User Story 2 - Laufende Apps sehen und zu ihnen springen (Priority: P1)

Als Nutzer sehe ich im Dock, welche Apps gerade offen sind, auch in anderen Arbeitsbereichen, und
komme mit einem Klick zu ihr, statt die Fensterübersicht zu durchsuchen.

**Why this priority**: Ohne diese Story ist das Dock nur ein zweiter Launcher; der Nutzer hat sich
ausdrücklich für eine volle Taskleiste entschieden.

**Independent Test**: Eine angeheftete und eine nicht angeheftete App öffnen, eine davon in einem
zweiten Arbeitsbereich; Indikatoren prüfen, Klicks prüfen, Mehrfachinstanz-App zweimal öffnen und das
Auswahlfeld prüfen.

**Acceptance Scenarios**:

1. **Given** eine angeheftete App hat eine Instanz, **When** der Nutzer das Dock ansieht,
   **Then** trägt ihr Eintrag eine Markierung „läuft“.
2. **Given** eine nicht angeheftete App hat eine Instanz, **When** der Nutzer das Dock ansieht,
   **Then** steht sie, abgetrennt hinter den angehefteten Einträgen, mit Markierung „läuft“ im Dock;
   schließt der Nutzer ihre letzte Instanz, verschwindet sie wieder.
3. **Given** eine App hat genau eine Instanz, in einem anderen Arbeitsbereich und in einem
   minimierten Fenster hinter einem anderen Tab, **When** der Nutzer ihren Dock-Eintrag anklickt,
   **Then** wechselt holzi in diesen Arbeitsbereich, holt das Fenster aus der Minimierung, macht den
   Tab aktiv und fokussiert das Fenster; es entsteht keine neue Instanz.
4. **Given** eine App hat mehrere Instanzen, **When** der Nutzer ihren Dock-Eintrag anklickt,
   **Then** öffnet sich ein Auswahlfeld mit allen Instanzen, nach Arbeitsbereich gruppiert, und dem
   Punkt „Neues Fenster“; die Auswahl einer Instanz verhält sich wie Szenario 3.
5. **Given** eine App hat mehrere Instanzen, **When** der Nutzer das Dock ansieht, **Then** zeigt
   ihr Eintrag deren Anzahl.
6. **Given** eine Mehrfachinstanz-App läuft bereits, **When** der Nutzer ihren Dock-Eintrag mit der
   mittleren Maustaste anklickt oder im Kontextmenü „Neues Fenster“ wählt, **Then** öffnet sich eine
   weitere Instanz im aktuellen Arbeitsbereich.
7. **Given** eine App verlangt Aufmerksamkeit, **When** der Nutzer das Dock ansieht, **Then** ist ihr
   Eintrag hervorgehoben, wie es heute andere Stellen für Aufmerksamkeit tun.
8. **Given** eine laufende App, **When** der Nutzer im Kontextmenü ihres Dock-Eintrags „Alle
   schließen“ wählt, **Then** schließen sich alle ihre Instanzen (Tabs anderer Apps in denselben
   Fenstern bleiben offen), mit denselben Rückfragen wie beim einzelnen Schließen.

---

### User Story 3 - Dock platzieren (Priority: P2)

Als Nutzer lege ich fest, an welcher Kante und wo entlang der Kante das Dock steht, und ob es Platz
für sich beansprucht, über den Fenstern schwebt oder sich automatisch ausblendet. Die Wahl gilt nur
für dieses Gerät.

**Why this priority**: Ausdrücklich gewünscht, aber das Dock funktioniert auch an der Standardposition.

**Independent Test**: In den Einstellungen alle Kanten und Ausrichtungen durchschalten, dann die drei
Modi mit einem maximierten Fenster prüfen; auf einem zweiten Gerät prüfen, dass dort die eigene Wahl
gilt.

**Acceptance Scenarios**:

1. **Given** die Einstellungen „Dock“, **When** der Nutzer eine Kante und eine Ausrichtung wählt,
   **Then** steht das Dock sofort dort; an linker und rechter Kante sind die Einträge untereinander
   angeordnet.
2. **Given** der Modus „Platz reservieren“ und ein maximiertes Fenster, **When** der Nutzer das Dock
   ansieht, **Then** endet das Fenster am Dock, und das Dock verdeckt nichts davon.
3. **Given** der Modus „Schweben“ und ein maximiertes Fenster, **When** der Nutzer das Dock ansieht,
   **Then** liegt das Dock über dem Fenster, und das Fenster nutzt die ganze Fläche.
4. **Given** der Modus „Automatisch ausblenden“, **When** der Zeiger nicht in der Nähe der Dock-Kante
   ist, **Then** ist das Dock ausgeblendet; **When** der Zeiger an die Kante kommt, **Then** blendet es
   sich ein und bleibt sichtbar, solange ein Auswahlfeld oder Kontextmenü des Docks offen ist.
5. **Given** der Nutzer hat auf Gerät A „links“ gewählt, **When** er auf Gerät B die Vault öffnet,
   **Then** steht das Dock dort weiter an der auf Gerät B gewählten Position.
6. **Given** das Dock ist eingerichtet, **When** der Nutzer mit Rechtsklick auf eine freie Stelle des
   Docks klickt, **Then** bietet das Kontextmenü Kante, Ausrichtung und Modus direkt an.

---

### User Story 4 - Einträge ordnen und Steuer-Einträge ausblenden (Priority: P2)

Als Nutzer bringe ich die Einträge des Docks in meine Reihenfolge und entferne Steuer-Einträge, die ich
nicht brauche. Nur den Launcher kann ich nicht entfernen, damit ich immer an alle Apps komme.

**Why this priority**: Macht das Dock wirklich konfigurierbar, setzt aber Story 1 voraus.

**Independent Test**: In der Leiste einen Eintrag an eine andere Stelle ziehen; in den Einstellungen
die Reihenfolge ändern und „Fenster“ entfernen; versuchen, den Launcher zu entfernen.

**Acceptance Scenarios**:

1. **Given** das Dock als Leiste, **When** der Nutzer einen Eintrag an eine andere Stelle zieht,
   **Then** bleibt er dort, auch nach einem Neustart und auf synchronisierten Geräten.
2. **Given** die Einstellungen „Dock“, **When** der Nutzer die Liste der Einträge umsortiert oder über
   „Hinzufügen“ eine App oder einen Steuer-Eintrag ergänzt, **Then** übernimmt das Dock die Änderung
   sofort.
3. **Given** Steuer-Einträge und Apps, **When** der Nutzer sortiert, **Then** kann er sie beliebig
   mischen; Steuer-Einträge stehen nicht zwingend vorn.
4. **Given** der Eintrag Arbeitsbereiche oder Fenster, **When** der Nutzer ihn entfernt, **Then**
   verschwindet er aus dem Dock; über den Launcher erreicht der Nutzer die Einstellungen „Dock“ und
   kann ihn dort wieder hinzufügen.
5. **Given** der Launcher-Eintrag, **When** der Nutzer ihn im Dock oder in den Einstellungen
   entfernen will, **Then** bietet holzi diese Möglichkeit nicht an.

---

### User Story 5 - Dock als Rad (Priority: P3)

Als Nutzer wähle ich statt der Leiste ein Rad: eine einzelne runde Schaltfläche an der gewählten
Position, die auf Klick alle Einträge im Bogen auffächert. So bleibt der Arbeitsbereich frei, und ich
habe trotzdem alles einen Klick entfernt.

**Why this priority**: Ausdrücklich gewünschte Option, aber die Leiste deckt den Bedarf schon ab.

**Independent Test**: Stil auf „Rad“ stellen, an einer Ecke und an einer Kante auffächern, mehr
Einträge anheften als in einen Bogen passen, einen Eintrag aktivieren, per Tastatur bedienen.

**Acceptance Scenarios**:

1. **Given** der Stil „Rad“ in einer Ecke, **When** der Nutzer die Schaltfläche anklickt, **Then**
   fächern die Einträge im Viertelkreis in den Arbeitsbereich hinein auf.
2. **Given** der Stil „Rad“ in der Mitte einer Kante, **When** der Nutzer die Schaltfläche anklickt,
   **Then** fächern die Einträge im Halbkreis auf.
3. **Given** mehr Einträge, als in einen Bogen passen, **When** das Rad auffächert, **Then** stehen
   die übrigen Einträge in einem zweiten, äußeren Bogen, und keiner überlappt einen anderen.
4. **Given** das Rad ist aufgefächert, **When** der Nutzer einen Eintrag aktiviert, Escape drückt
   oder außerhalb klickt, **Then** klappt das Rad zu; die Aktivierung verhält sich wie in Story 1
   und 2.
5. **Given** das Rad ist zugeklappt und eine App verlangt Aufmerksamkeit, **When** der Nutzer die
   Schaltfläche ansieht, **Then** trägt sie eine Markierung.
6. **Given** der Stil „Rad“ mit Modus „Platz reservieren“, **When** ein Fenster maximiert ist, **Then**
   endet das Fenster vor dem Streifen, den die Schaltfläche des Rads belegt; die aufgefächerten
   Einträge liegen über den Fenstern. Mit „Schweben“ liegt auch die Schaltfläche über den Fenstern.

---

### User Story 6 - Dock im Kompaktmodus (Priority: P2)

Als Nutzer auf dem Telefon oder in einem schmalen Fenster habe ich ein Dock, das dort gut bedienbar
ist, unabhängig davon, wo ich es am Desktop platziert habe.

**Why this priority**: holzi läuft auf Android (Spec 043); ein seitliches Dock wäre dort unbrauchbar.

**Independent Test**: Mit Stil „Leiste, links“ und mit Stil „Rad, oben links“ das Fenster unter die
Kompakt-Schwelle verkleinern und zurück; auf dem Telefon per Touch bedienen.

**Acceptance Scenarios**:

1. **Given** Stil „Leiste“ mit beliebiger Platzierung, **When** holzi in den Kompaktmodus wechselt,
   **Then** steht die Leiste unten mittig und reserviert Platz; passen nicht alle Einträge hinein,
   lässt sie sich seitlich wischen.
2. **Given** Stil „Rad“ mit Ausrichtung „Anfang“, **When** holzi in den Kompaktmodus wechselt,
   **Then** steht das Rad unten links; bei jeder anderen Ausrichtung unten rechts.
3. **Given** der Kompaktmodus, **When** holzi ihn wieder verlässt, **Then** gilt wieder die gewählte
   Platzierung, ohne dass der Nutzer etwas einstellen muss.
4. **Given** ein Fenster knapp oberhalb der Kompakt-Schwelle und eine Leiste links mit Modus „Platz
   reservieren“, **When** das Dock Platz beansprucht, **Then** wechselt holzi deswegen nicht in den
   Kompaktmodus.
5. **Given** ein Touch-Gerät, **When** der Nutzer einen Dock-Eintrag lange drückt, **Then** öffnet
   sich das Kontextmenü.

---

### Edge Cases

- Eine angeheftete haextension wird deinstalliert oder deaktiviert: Ihr Eintrag verschwindet aus dem
  Dock, bleibt aber gemerkt; wird sie wieder verfügbar, ist sie wieder angeheftet. In den Einstellungen
  „Dock“ steht sie als „nicht verfügbar“ und lässt sich dort endgültig entfernen.
- Die gespeicherte Dock-Konfiguration ist unlesbar oder unvollständig: Das Dock zeigt die
  Standard-Einträge und überschreibt die gespeicherte Konfiguration erst, wenn der Nutzer selbst etwas
  ändert.
- Die Konfiguration enthält eine App doppelt: Sie erscheint einmal, an ihrer ersten Stelle.
- Die Konfiguration enthält keinen Launcher (etwa von einem anderen Gerät): Das Dock zeigt ihn trotzdem
  an erster Stelle.
- Eine App ist angeheftet und läuft: Sie erscheint einmal, an ihrer angehefteten Stelle, mit Markierung
  „läuft“.
- Der Arbeitsbereich einer Instanz wird geschlossen, während das Auswahlfeld offen ist: Das Auswahlfeld
  zeigt nur Instanzen, die es noch gibt; ist keine mehr übrig, schließt es sich.
- holzi wechselt zwischen Kompakt- und Normalmodus, während das Rad aufgefächert oder ein Auswahlfeld
  offen ist: Beides schließt sich.
- Zwei Geräte ändern die Reihenfolge gleichzeitig: Nach der Synchronisierung gilt die zuletzt
  geschriebene Reihenfolge vollständig; es entsteht keine Mischung.
- Eine App, die nur in Entwicklungs-Builds existiert, ist angeheftet und die Vault wird in einem
  Release-Build geöffnet: wie eine nicht verfügbare haextension.

## Requirements _(mandatory)_

### Functional Requirements

**Inhalt und Aufbau**

- **FR-001**: holzi MUSS im Arbeitsbereich ein Dock anzeigen, das die drei bisherigen schwebenden
  Schaltflächen für Launcher, Fensterübersicht und Arbeitsbereichs-Übersicht ersetzt.
- **FR-002**: Das Dock MUSS eine geordnete Liste von Einträgen zeigen. Ein Eintrag ist entweder ein
  Steuer-Eintrag (Launcher, Fensterübersicht, Arbeitsbereichs-Übersicht) oder eine angeheftete App.
- **FR-003**: Ohne gespeicherte Konfiguration MUSS das Dock die Einträge Launcher, Dateibrowser, Chat,
  Passwörter und Einstellungen in dieser Reihenfolge zeigen, als Leiste unten mittig im Modus „Platz
  reservieren“. Die Steuer-Einträge Arbeitsbereiche und Fenster lässt der Nutzer über die
  Dock-Einstellungen ergänzen.
- **FR-004**: Jede App aus dem Launcher, ob System-App oder haextension, MUSS anheftbar sein.
- **FR-005**: Hinter den Einträgen der Liste MUSS das Dock, optisch abgetrennt, jede laufende App aus
  allen Arbeitsbereichen zeigen, die nicht angeheftet ist; sie verschwindet, sobald ihre letzte Instanz
  geschlossen ist.
- **FR-006**: Der Launcher-Eintrag MUSS immer im Dock stehen und DARF NICHT entfernbar sein.

**Anzeige**

- **FR-007**: Ein App-Eintrag MUSS zeigen, ob die App läuft, und ab zwei Instanzen deren Anzahl.
- **FR-008**: Ein App-Eintrag MUSS hervorgehoben sein, solange die App Aufmerksamkeit verlangt.
- **FR-009**: Ein App-Eintrag MUSS das Symbol und den Namen der App tragen, wie der Launcher sie zeigt;
  der Name MUSS mindestens als Tooltip und für Screenreader verfügbar sein.

**Aktivieren**

- **FR-010**: Ein Klick auf eine App ohne Instanz MUSS sie öffnen wie der Launcher.
- **FR-011**: Ein Klick auf eine App mit genau einer Instanz MUSS in deren Arbeitsbereich wechseln,
  ihr Fenster aus der Minimierung holen, ihren Tab aktiv machen und das Fenster fokussieren, ohne eine
  neue Instanz zu öffnen.
- **FR-012**: Ein Klick auf eine App mit mehreren Instanzen MUSS ein Auswahlfeld öffnen, das die
  Instanzen nach Arbeitsbereich gruppiert zeigt und „Neues Fenster“ anbietet.
- **FR-013**: Ein Klick mit der mittleren Maustaste MUSS bei Mehrfachinstanz-Apps ein neues Fenster im
  aktuellen Arbeitsbereich öffnen und sich bei Einzelinstanz-Apps wie FR-010/FR-011 verhalten.
- **FR-014**: Ein Klick auf einen Steuer-Eintrag MUSS dieselbe Übersicht öffnen wie bisher die
  entsprechende Schaltfläche.

**Kontextmenüs**

- **FR-015**: Rechtsklick oder Langdruck auf einen App-Eintrag im Dock MUSS ein Kontextmenü öffnen mit
  „Anheften“ bzw. „Lösen“, „Neues Fenster“ (nur Mehrfachinstanz-Apps) und „Alle schließen“ (nur
  laufende Apps).
- **FR-016**: „Alle schließen“ MUSS jede Instanz der App so schließen, als hätte der Nutzer ihren Tab
  einzeln geschlossen; Tabs anderer Apps bleiben offen, einschließlich vorhandener Rückfragen.
- **FR-017**: Rechtsklick oder Langdruck auf eine App im Launcher MUSS „An Dock anheften“ bzw. „Vom Dock
  lösen“ anbieten.
- **FR-018**: Rechtsklick auf eine freie Stelle des Docks MUSS Kante, Ausrichtung, Stil und Modus zur
  Auswahl anbieten.

**Ordnen**

- **FR-019**: In der Leiste MUSS der Nutzer Einträge per Ziehen umsortieren können, Steuer-Einträge
  und Apps gemischt.
- **FR-020**: Die Einstellungen MÜSSEN einen Unterpunkt „Dock“ haben mit einer sortierbaren Liste aller
  Einträge, „Hinzufügen“ für Apps und entfernte Steuer-Einträge sowie „Entfernen“ für jeden Eintrag
  außer dem Launcher.
- **FR-021**: Die Liste in den Einstellungen MUSS gemerkte, aber nicht verfügbare Apps als „nicht
  verfügbar“ zeigen und ihr Entfernen erlauben.

**Platzierung und Stil**

- **FR-022**: Der Nutzer MUSS eine Kante (oben, unten, links, rechts) und eine Ausrichtung (Anfang,
  Mitte, Ende) wählen können; das ergibt zwölf Positionen.
- **FR-023**: Der Nutzer MUSS den Stil „Leiste“ oder „Rad“ wählen können.
- **FR-024**: Für Leiste und Rad MUSS der Nutzer einen Modus wählen können: „Platz reservieren“ (Fenster,
  auch maximierte, enden am Dock), „Schweben“ (Dock liegt über den Fenstern) oder „Automatisch
  ausblenden“ (Dock erscheint, wenn der Zeiger die Kante erreicht, und verschwindet nach kurzer
  Verzögerung, wenn er sie verlässt).
- **FR-025**: Eine ausgeblendete Leiste MUSS sichtbar bleiben, solange eines ihrer Auswahlfelder oder
  Kontextmenüs offen ist oder der Tastaturfokus in ihr liegt.
- **FR-026**: An linker und rechter Kante MUSS die Leiste ihre Einträge untereinander anordnen; passen
  nicht alle Einträge hinein, MUSS sie sich in ihrer Richtung scrollen lassen.
- **FR-027**: Das Rad MUSS als einzelne runde Schaltfläche an der gewählten Position stehen und auf
  Aktivierung alle Einträge im Bogen auffächern: als Viertelkreis in einer Ecke, als Halbkreis in der
  Mitte einer Kante, jeweils in den Arbeitsbereich hinein.
- **FR-028**: Passen die Einträge nicht in einen Bogen, MUSS das Rad sie auf weitere, äußere Bögen
  verteilen, ohne dass Einträge einander überlappen.
- **FR-029**: Das Rad MUSS sich schließen, wenn ein Eintrag aktiviert wird, der Nutzer Escape drückt oder
  außerhalb klickt.
- **FR-030**: Im Modus „Platz reservieren“ MUSS das Rad an seiner Kante einen Streifen in der Größe
  seiner Schaltfläche freihalten; seine aufgefächerten Einträge MÜSSEN immer über den Fenstern liegen.

**Kompaktmodus**

- **FR-031**: Im Kompaktmodus MUSS die Leiste unten mittig stehen und Platz reservieren, unabhängig von
  der gewählten Platzierung und dem gewählten Modus.
- **FR-032**: Im Kompaktmodus MUSS das Rad unten links stehen, wenn die Ausrichtung „Anfang“ ist, sonst
  unten rechts.
- **FR-033**: Ob holzi im Kompaktmodus ist, MUSS sich nach der Größe des gesamten App-Fensters richten,
  nicht nach der Fläche, die das Dock übrig lässt.
- **FR-034**: Beim Wechsel zwischen Kompakt- und Normalmodus MÜSSEN ein aufgefächertes Rad und ein
  offenes Auswahlfeld schließen; die gespeicherte Platzierung DARF sich dabei NICHT ändern.

**Speichern und Synchronisieren**

- **FR-035**: Die Einträge des Docks MÜSSEN in der Vault gespeichert und mit den eigenen Geräten
  synchronisiert werden.
- **FR-036**: Platzierung, Stil und Modus MÜSSEN pro Gerät gespeichert und NICHT synchronisiert werden.
- **FR-037**: Eine angeheftete App, die auf diesem Gerät nicht verfügbar ist, MUSS im Dock ausgeblendet
  und in der gespeicherten Konfiguration erhalten bleiben.
- **FR-038**: Eine unlesbare oder unvollständige gespeicherte Konfiguration MUSS zu den
  Standard-Einträgen (FR-003) führen, ergänzt um einen fehlenden Launcher, und DARF erst bei der
  nächsten Änderung durch den Nutzer überschrieben werden.
- **FR-039**: Doppelte Einträge MÜSSEN beim Anzeigen auf das erste Vorkommen reduziert werden.
- **FR-040**: Änderungen an der Konfiguration, auch von einem anderen Gerät, MÜSSEN ohne Neustart im
  Dock sichtbar werden.

**Bedienbarkeit**

- **FR-041**: Die Leiste MUSS per Tastatur erreichbar sein; innerhalb der Leiste MÜSSEN die Pfeiltasten
  zwischen den Einträgen wechseln und Enter den Eintrag aktivieren.
- **FR-042**: Das Rad MUSS per Tastatur auffächerbar sein; im aufgefächerten Rad MÜSSEN die Pfeiltasten
  den Bögen folgen und Enter den Eintrag aktivieren.
- **FR-043**: Auf Touch-Geräten MUSS ein Langdruck das Kontextmenü öffnen.

### Key Entities

- **Dock-Konfiguration (Vault)**: geordnete Liste von Einträgen; jeder Eintrag ist entweder ein
  Steuer-Eintrag (Launcher, Fensterübersicht, Arbeitsbereichs-Übersicht) oder ein Verweis auf eine App
  über deren Kennung. Gilt für alle eigenen Geräte.
- **Dock-Platzierung (Gerät)**: Stil (Leiste oder Rad), Kante, Ausrichtung und Modus (Platz
  reservieren, Schweben, Automatisch ausblenden). Gilt nur für dieses Gerät.
- **Laufende App**: keine gespeicherte Größe, sondern abgeleitet aus den offenen Tabs aller
  Arbeitsbereiche: App, ihre Instanzen mit Fenster und Arbeitsbereich, und ob die App Aufmerksamkeit
  verlangt.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Eine angeheftete App öffnet sich mit einem einzigen Klick aus dem Arbeitsbereich, gegenüber
  heute mindestens zwei (Launcher öffnen, App wählen).
- **SC-002**: Zu einer Instanz einer App in einem anderen Arbeitsbereich kommt der Nutzer mit
  höchstens zwei Klicks (Eintrag, bei mehreren Instanzen Auswahl).
- **SC-003**: Alle zwölf Positionen, beide Stile und alle drei Modi der Leiste lassen sich ohne
  Neustart einstellen; die Wirkung ist sofort sichtbar.
- **SC-004**: Ein maximiertes Fenster wird im Modus „Platz reservieren“ an keiner der zwölf Positionen
  vom Dock verdeckt.
- **SC-005**: Eine auf einem Gerät angeheftete App erscheint nach der Synchronisierung auf jedem anderen
  eigenen Gerät, während die Platzierung jedes Geräts unverändert bleibt.
- **SC-006**: Nach dem Deinstallieren und erneuten Installieren einer angehefteten haextension steht sie
  wieder an ihrer alten Stelle im Dock.
- **SC-007**: Jede Funktion des Docks ist ohne Maus erreichbar (Tastatur) und auf dem Telefon per Touch
  bedienbar.
- **SC-008**: Im Kompaktmodus ist das Dock auf einem Telefon im Hochformat vollständig bedienbar, ohne
  dass ein Eintrag außerhalb des Bildschirms unerreichbar bleibt.

## Assumptions

- Die Übersichten selbst (Launcher, Fensterübersicht, Arbeitsbereichs-Übersicht) bleiben unverändert;
  diese Spec ändert nur, wie man sie erreicht. Fensterübersicht und Arbeitsbereichs-Übersicht haben
  heute kein Tastenkürzel. Entfernt der Nutzer ihren Dock-Eintrag, verzichtet er bewusst auf diesen Weg;
  der immer vorhandene Launcher führt zu den Einstellungen, wo er den Eintrag wieder hinzufügen kann.
- Der Launcher ändert sein Verhalten beim Öffnen nicht (Spec 030 bleibt gültig); nur das Dock fokussiert
  laufende Instanzen.
- Gleichzeitiges Umsortieren auf zwei Geräten ist selten; die zuletzt geschriebene Reihenfolge gilt
  vollständig. Eine Zusammenführung pro Eintrag ist nicht vorgesehen.
- Im Rad lässt sich nicht per Ziehen sortieren; dafür gibt es die Liste in den Einstellungen oder den
  Wechsel zur Leiste.
- Instanzen im Dock-Auswahlfeld tragen den Titel ihres Tabs, wie in der Tableiste.
- Größe der Dock-Einträge, Animationen und Abstände folgen dem bestehenden Erscheinungsbild; eine eigene
  Einstellung für die Größe des Docks ist nicht vorgesehen.
- holzi hat noch keine Nutzer; die bisherigen drei Schaltflächen entfallen ersatzlos, ohne
  Übergangsoption.
