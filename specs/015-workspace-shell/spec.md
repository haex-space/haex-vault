# Feature Specification: Workspace-Shell (Arbeitsbereiche, Apps und Fenster)

**Feature Branch**: `015-workspace-shell`
**Created**: 2026-09-21
**Status**: Implemented (T057 offen)
**Input**: holzi übernimmt aus haex-vault 2.x (Legacy) das Shell-Konzept aus Workspaces (Arbeitsbereichen), Apps und Fenstern. Die heutige Workspace-Seite ist nur ein Stub mit Vault-Name und Chat-Einstieg; Chat, Einstellungen und Föderation sind eigene Vollseiten. Nach dieser Spec öffnet der Nutzer Apps als Fenster in einem Arbeitsbereich, bündelt mehrere Apps oder Ansichten als Tabs in einem Fenster (Bedienung wie in Firefox), kann mehrere Arbeitsbereiche pro Gerät verwalten, und sein Layout überlebt einen Neustart.

## Beziehung zu bestehenden Specs

- [`002-onboarding-model-prefs`](../002-onboarding-model-prefs/spec.md) bleibt
  maßgeblich für den Onboarding-Wizard und dessen Erzwingung. Die Shell wird
  erst nach abgeschlossenem Onboarding erreicht.
- [`003-agent-tool-loop`](../003-agent-tool-loop/spec.md) bleibt maßgeblich für
  Antwort-Lifecycle, Freigaben und Abbruch. Diese Spec legt nur fest, wie sich
  Fensteraktionen (Minimieren, Schließen, Workspace-Wechsel) dazu verhalten.
- [`004-chat-window-handling`](../004-chat-window-handling/spec.md) bleibt
  maßgeblich für den Inhalt des Chats (jeder Chat-Einstieg beginnt mit einer
  neuen Unterhaltung, Modell-Preload beim Vault-Open, Composer). Wo Spec 004 von
  „Chat-Route“ oder „Chat-Overlay aus dem Workspace“ spricht, ist ab dieser
  Spec der **Chat-Tab** (in einem Fenster) gemeint.
- Eine künftige, noch nicht nummerierte Spec (parallele Chat-Sessions im
  Backend) folgt — Spec 016 ist bereits an `016-e2e-testing` vergeben. Diese
  Spec bereitet nur das Fenster- und Tab-Modell darauf vor; sie erlaubt
  weiterhin genau **einen** Chat (ein Tab in einem Fenster).
  _Seit Spec 030 gilt das nicht mehr: Chat ist eine Mehrfachinstanz-App,
  siehe FR-017 und [`030-app-multi-instance`](../030-app-multi-instance/spec.md)._
- Die Specs 017/018 (Haextension-Host und MCP-Anbindung) folgen. Extensions als
  Fensterinhalt sind nicht Teil dieser Spec.
- Setzt voraus, dass zu jedem Zeitpunkt genau eine Vault-Session aktiv ist
  (Spec 013, umgesetzt). Die Shell lebt innerhalb dieser Session.

## User Scenarios & Testing

### User Story 1 - Apps als Fenster im Arbeitsbereich öffnen (Priority: P1)

Ein Nutzer öffnet eine Vault und landet in seinem Arbeitsbereich. Über einen
Launcher öffnet er Apps — Chat, Einstellungen — als Fenster. Mehrere Fenster
können gleichzeitig sichtbar sein; er arbeitet im Chat und schaut nebenbei in
den Einstellungen nach, ohne die Seite zu wechseln.

**Why this priority**: Das ist der Kern des Features. Ohne öffnbare Fenster gibt
es keine Shell; alles Weitere (Workspaces, Persistenz, mobile Darstellung)
baut darauf auf. Chat und Einstellungen als Fenster sind zugleich der Nachweis,
dass die bestehende Funktionalität ohne Rückschritt in die Shell wandert.

**Independent Test**: Eine Vault mit abgeschlossenem Onboarding öffnen, im
Launcher „Chat“ wählen, dann „Einstellungen“ wählen. Beide Fenster sind
gleichzeitig sichtbar, der Chat ist nutzbar (Nachricht senden), die
Einstellungen sind nutzbar (Gerätename ändern).

**Acceptance Scenarios**:

1. **Given** eine Vault mit abgeschlossenem Onboarding, **When** der Nutzer die
   Vault öffnet, **Then** sieht er den Arbeitsbereich mit dem Launcher und ohne
   offene Fenster (beim ersten Öffnen) statt der bisherigen Stub-Seite.
2. **Given** der Arbeitsbereich ist sichtbar, **When** der Nutzer im Launcher
   eine App wählt, **Then** öffnet sich ein neues Fenster mit einem Tab dieser
   App, ist fokussiert und liegt vor allen anderen Fenstern.
3. **Given** zwei geöffnete Fenster, **When** der Nutzer in das hintere Fenster
   klickt, **Then** kommt es nach vorn und wird als aktiv gekennzeichnet; das
   andere bleibt sichtbar, aber als inaktiv erkennbar.
4. **Given** ein geöffnetes Fenster, **When** der Nutzer es schließt, **Then**
   verschwindet es aus dem Arbeitsbereich und aus der Fensterübersicht.
5. **Given** der Nutzer hat den Chat im Fenster geöffnet, **When** er eine
   Nachricht sendet, **Then** verhält sich der Chat exakt wie in Spec 004
   beschrieben (neue Unterhaltung pro Einstieg, Composer, Freigaben).
6. **Given** der Nutzer hat die Einstellungen im Fenster geöffnet, **When** er
   eine Einstellung ändert (z. B. Gerätename, Standard-Modell), **Then** wird
   sie gespeichert wie bisher; es gibt keine Funktion, die nur in der früheren
   Vollseite erreichbar war.
7. **Given** der Nutzer ruft eine der früheren Vollseiten-Adressen auf (Chat,
   Einstellungen, Föderation), **When** die Seite lädt, **Then** landet er im
   Arbeitsbereich und das passende Fenster ist geöffnet.
8. **Given** das Onboarding ist für dieses Gerät nicht abgeschlossen, **When**
   der Nutzer den Arbeitsbereich aufrufen will, **Then** wird er wie bisher in
   den Onboarding-Wizard geleitet; die Shell erscheint nicht.

---

### User Story 2 - Fenster verwalten (Priority: P1)

Ein Nutzer ordnet seine Fenster: er verschiebt sie, ändert ihre Größe,
maximiert ein Fenster auf den ganzen Arbeitsbereich, minimiert Fenster, die er
gerade nicht braucht, und holt sie über eine Fensterübersicht zurück. Was er in
einem Fenster begonnen hat, geht dabei nicht verloren.

**Why this priority**: Fenster ohne Verwaltung sind unbrauchbar; und die
Zusicherung „Inhalt geht nicht verloren“ ist die Bedingung, unter der der
Chat überhaupt als Fenster taugt (laufende Antworten, Eingabe-Entwürfe).

**Independent Test**: Zwei Fenster öffnen, eines verschieben und vergrößern,
das andere minimieren, über die Fensterübersicht wiederherstellen. Im Chat vor
dem Minimieren einen Entwurf tippen: nach dem Wiederherstellen ist er noch da.

**Acceptance Scenarios**:

1. **Given** ein geöffnetes Fenster, **When** der Nutzer die Titelleiste zieht,
   **Then** folgt das Fenster dem Zeiger und bleibt an der losgelassenen
   Position.
2. **Given** ein geöffnetes Fenster, **When** der Nutzer an einer Kante oder
   Ecke zieht, **Then** ändert sich die Größe; sie unterschreitet nie eine
   sinnvolle Mindestgröße der App.
3. **Given** der Nutzer zieht ein Fenster über den Rand des sichtbaren
   Bereichs, **When** er loslässt, **Then** bleibt mindestens ein Teil der
   Titelleiste erreichbar, sodass er es zurückholen kann.
4. **Given** ein geöffnetes Fenster, **When** der Nutzer es minimiert, **Then**
   verschwindet es aus dem Arbeitsbereich, bleibt aber in der Fensterübersicht
   aufgeführt.
5. **Given** ein minimiertes Fenster, **When** der Nutzer es in der
   Fensterübersicht wählt, **Then** erscheint es an seiner vorherigen Position
   und Größe, ist fokussiert und liegt vorn.
6. **Given** die Fensterübersicht ist geöffnet, **When** der Nutzer ein Fenster
   wählt oder schließt, **Then** wird es fokussiert bzw. geschlossen; die
   Übersicht zeigt alle offenen Fenster mit Icon und Titel des aktiven Tabs und
   der Tab-Anzahl.
7. **Given** der Nutzer hat im Chat-Fenster einen Entwurf getippt und eine
   Antwort läuft, **When** er das Fenster minimiert und wiederherstellt,
   **Then** ist der Entwurf unverändert und die Antwort ist weitergelaufen.
8. **Given** mehrere neu geöffnete Fenster, **When** sie erscheinen, **Then**
   liegen sie leicht versetzt (nicht deckungsgleich übereinander).
9. **Given** das Chat-Fenster ist minimiert und eine Antwort wartet auf eine
   Freigabe (Spec 003), **When** die Freigabe-Anfrage entsteht, **Then** trägt
   das Fenster in der Fensterübersicht und am Launcher einen
   Aufmerksamkeitshinweis; die Antwort bleibt pausiert, bis der Nutzer
   entscheidet (kein stilles Zeitlimit).
10. **Given** ein Fenster in normaler Größe, **When** der Nutzer „Maximieren“
    wählt oder die Titelleiste doppelklickt, **Then** füllt es den gesamten
    Arbeitsbereich, und die Schaltfläche zeigt „Wiederherstellen“.
11. **Given** ein maximiertes Fenster, **When** der Nutzer „Wiederherstellen“
    wählt oder die Titelleiste doppelklickt, **Then** kehrt es zu Position und
    Größe vor dem Maximieren zurück.
12. **Given** die Titelleiste eines Fensters, **When** der Nutzer sie betrachtet,
    **Then** stehen rechts in dieser Reihenfolge: Tab-Liste (Chevron),
    Minimieren, Maximieren/Wiederherstellen, Schließen.

---

### User Story 3 - Tabs in Fenstern (Priority: P2)

Ein Nutzer bündelt mehrere Apps in einem Fenster, wie Tabs in Firefox: Jede App
ist ein Tab in der Titelleiste, direkt hinter dem letzten Tab sitzt ein „+“, über
das er einen weiteren Tab öffnet. Im rechten Bereich der Titelleiste, vor den
Fenster-Schaltflächen, öffnet ein Chevron ein Dropdown mit allen Tabs des
Fensters, aus dem er einen auswählt.

**Why this priority**: Tabs halten viele Apps in wenigen Fenstern und sind die
Vorgabe des Betreibers für die Fensterbedienung; ein einzelnes Fenster pro App
trägt aber schon den ersten nutzbaren Schnitt, deshalb P2.

**Independent Test**: Ein Fenster mit Chat öffnen, über „+“ die Einstellungen
als zweiten Tab hinzufügen, über den Chevron zurück zum Chat wechseln, den
Einstellungs-Tab schließen. Der Chat-Entwurf ist nach jedem Wechsel unverändert.

**Acceptance Scenarios**:

1. **Given** ein Fenster mit einem einzigen Tab, **When** der Nutzer die
   Titelleiste betrachtet, **Then** zeigt sie Icon und Titel der App ohne
   Tab-Rahmen, direkt dahinter das „+“, rechts den Chevron und die
   Fenster-Schaltflächen.
2. **Given** der Nutzer klickt das „+“, **When** die Liste der öffnenbaren Apps
   erscheint und er eine App wählt, **Then** entsteht in diesem Fenster ein
   neuer Tab, der aktiv wird und dessen Inhalt sichtbar ist.
3. **Given** eine Einzelinstanz-App hat schon irgendwo (in diesem oder einem
   anderen Fenster oder Arbeitsbereich) einen Tab, **When** der Nutzer sie über
   das „+“ wählt, **Then** entsteht kein zweiter Tab; der vorhandene Tab wird
   aktiv, sein Fenster fokussiert und sein Arbeitsbereich aktiviert.
4. **Given** ein Fenster mit mehreren Tabs, **When** der Nutzer die Titelleiste
   betrachtet, **Then** zeigt jeder Tab Icon, Titel und eine Schließen-Schaltfläche,
   der aktive Tab ist erkennbar, und das „+“ sitzt unmittelbar hinter dem
   letzten Tab.
5. **Given** mehrere Tabs, **When** der Nutzer einen Tab anklickt, **Then** wird
   er aktiv und sein Inhalt sichtbar; die Inhalte der anderen Tabs bleiben
   erhalten (Entwurf, laufende Antwort).
6. **Given** ein Fenster mit mehreren Tabs, **When** der Nutzer den Chevron
   wählt, **Then** listet ein Dropdown alle Tabs dieses Fensters mit Icon und
   Titel, der aktive ist markiert, ein Tab mit Aufmerksamkeitshinweis trägt ihn
   auch dort; die Auswahl eines Eintrags macht diesen Tab aktiv und in der
   Leiste sichtbar.
7. **Given** mehr Tabs, als in die Titelleiste passen, **When** der Nutzer die
   Leiste betrachtet, **Then** lässt sie sich scrollen, der aktive Tab bleibt
   sichtbar, das „+“ und der Chevron bleiben erreichbar.
8. **Given** ein Fenster mit mehreren Tabs, **When** der Nutzer einen Tab
   schließt, **Then** wird sein rechter Nachbar aktiv (beim letzten Tab der
   linke); **When** er den letzten verbleibenden Tab schließt, **Then** schließt
   sich das Fenster.
9. **Given** ein Tab mit laufender Antwort oder ausstehender Freigabe, **When**
   der Nutzer ihn (oder das Fenster) schließt, **Then** verlangt die Shell eine
   Bestätigung (FR-014).
10. **Given** die Kompaktdarstellung, **When** ein Fenster mehrere Tabs hat,
    **Then** ersetzt die Titelleiste die Tab-Leiste durch Titel des aktiven Tabs
    und den Chevron mit dem Dropdown, der Wechsel gelingt ohne Scrollen.
11. **Given** ein Tab wird über die Tab-Leiste bedient, **When** der Nutzer mit
    Tastatur arbeitet, **Then** lässt sich die Leiste als Tab-Liste navigieren
    (Pfeiltasten wechseln zwischen Tabs, Eingabe aktiviert), und das „+“ und der
    Chevron sind erreichbar.

---

### User Story 4 - Mehrere Arbeitsbereiche (Priority: P2)

Ein Nutzer trennt seine Arbeit in mehrere Arbeitsbereiche — zum Beispiel einen
für Recherche und einen für Konfiguration. Er legt Arbeitsbereiche an, wechselt
zwischen ihnen, verschiebt Fenster von einem in den anderen und löscht
Arbeitsbereiche, die er nicht mehr braucht. Arbeitsbereiche haben keine eigenen
Namen; sie heißen „Arbeitsbereich 1“, „Arbeitsbereich 2“ usw. (englisch
„Workspace 1“ …) nach ihrer Reihenfolge.

**Why this priority**: Arbeitsbereiche machen die Shell mit vielen Fenstern
handhabbar; für einen ersten nutzbaren Schnitt reicht aber auch ein einzelner
Arbeitsbereich, deshalb P2.

**Independent Test**: Einen zweiten Arbeitsbereich anlegen, ein Fenster dorthin
verschieben, zwischen beiden wechseln, den zweiten löschen.

**Acceptance Scenarios**:

1. **Given** eine Vault ohne gespeicherte Arbeitsbereiche auf diesem Gerät,
   **When** die Shell zum ersten Mal erscheint, **Then** existiert genau ein
   Standard-Arbeitsbereich.
2. **Given** der Nutzer wählt „Arbeitsbereich anlegen“, **When** er bestätigt,
   **Then** entsteht ein neuer, leerer Arbeitsbereich am Ende der Reihenfolge
   (mit der nächsten Nummer), und die Shell wechselt zu ihm.
3. **Given** mehrere Arbeitsbereiche, **When** der Nutzer wechselt, **Then**
   zeigt die Shell nur die Fenster des gewählten Arbeitsbereichs; Fenster der
   anderen bleiben geöffnet und unverändert.
4. **Given** eine Arbeitsbereichs-Übersicht, **When** der Nutzer sie öffnet,
   **Then** sieht er alle Arbeitsbereiche mit ihrer Nummer und der Anzahl der
   Fenster und kann zu einem wechseln.
5. **Given** drei Arbeitsbereiche, **When** der Nutzer den zweiten löscht,
   **Then** heißt der bisherige dritte jetzt „Arbeitsbereich 2“; die Nummern
   sind immer lückenlos.
6. **Given** ein Fenster, **When** der Nutzer es in einen anderen
   Arbeitsbereich verschiebt, **Then** verschwindet es aus dem aktuellen und
   erscheint im Ziel-Arbeitsbereich, mit unverändertem Inhalt.
7. **Given** ein Arbeitsbereich mit offenen Fenstern, **When** der Nutzer ihn
   löschen will, **Then** verlangt die Shell eine Bestätigung, die nennt, dass
   die Fenster geschlossen werden; nach Bestätigung sind Arbeitsbereich und
   Fenster weg.
8. **Given** nur ein einziger Arbeitsbereich existiert, **When** der Nutzer ihn
   löschen will, **Then** ist das nicht möglich (Aktion nicht angeboten oder
   mit Erklärung abgelehnt).
9. **Given** eine App, die nur einmal geöffnet sein darf, ist in einem anderen
   Arbeitsbereich geöffnet, **When** der Nutzer sie im Launcher wählt,
   **Then** wechselt die Shell zu diesem Arbeitsbereich und fokussiert das
   vorhandene Fenster.
10. **Given** das Chat-Fenster in einem nicht sichtbaren Arbeitsbereich wartet
    auf eine Freigabe, **When** der Nutzer den aktuellen Arbeitsbereich
    betrachtet, **Then** zeigt die Arbeitsbereichs-Auswahl einen
    Aufmerksamkeitshinweis am betroffenen Arbeitsbereich.

---

### User Story 5 - Layout bleibt über Neustarts erhalten (Priority: P2)

> **Seit Spec 022** gilt dies nur bei eingeschalteter Einstellung „Sitzung wiederherstellen“ (standardmäßig aus), siehe [`022-session-restore`](../022-session-restore/spec.md). Mit der Einstellung kommen zusätzlich Ort und Vor-/Zurück-Historie jedes Tabs zurück.

Ein Nutzer schließt holzi und öffnet die Vault später wieder. Seine
Arbeitsbereiche und deren Reihenfolge, die Anordnung seiner Fenster und
deren Tabs sind wieder da. Auf einem anderen Gerät, auf dem er dieselbe Vault öffnet, hat
er sein eigenes, davon unabhängiges Layout.

**Why this priority**: Ohne Persistenz muss der Nutzer bei jedem Start neu
einrichten; das entwertet Arbeitsbereiche. Es ist aber auf einem ersten Schnitt
verzichtbar, deshalb P2.

**Independent Test**: Zwei Arbeitsbereiche mit je einem Fenster einrichten, in
einem Fenster zwei Tabs, holzi beenden, Vault erneut öffnen: gleiche
Arbeitsbereiche, gleiche Fenster an gleicher Position und Größe, gleiche Tabs in
gleicher Reihenfolge mit gleichem aktivem Tab, zuletzt aktiver Arbeitsbereich ist
aktiv.

**Acceptance Scenarios**:

1. **Given** ein eingerichtetes Layout, **When** der Nutzer die Vault schließt
   und auf demselben Gerät wieder öffnet, **Then** sind Arbeitsbereiche
   (Reihenfolge), Fenster (Arbeitsbereich, Position, Größe, Minimierungs- und
   Maximierungszustand), deren Tabs (App, Reihenfolge, aktiver Tab) und der
   zuletzt aktive Arbeitsbereich wiederhergestellt.
2. **Given** ein wiederhergestellter Chat-Tab, **When** die Shell erscheint,
   **Then** beginnt er mit einer neuen Unterhaltung (Spec 004); Tab-Inhalte
   werden nicht wiederhergestellt, nur Fenster und Tabs selbst.
3. **Given** die Vault-Datei wird auf ein zweites Gerät kopiert und dort
   erstmals geöffnet, **When** die Shell erscheint, **Then** zeigt sie dort
   einen Standard-Arbeitsbereich, nicht die Arbeitsbereiche des ersten Geräts.
4. **Given** ein gespeichertes Layout verweist auf eine App, die in dieser
   Programmversion nicht existiert, **When** das Layout wiederhergestellt wird,
   **Then** wird dieser Tab still verworfen (hat ein Fenster danach keinen Tab
   mehr, auch das Fenster); alle anderen erscheinen.
5. **Given** das gespeicherte Layout ist nicht lesbar, **When** die Shell
   erscheint, **Then** startet sie mit einem Standard-Arbeitsbereich ohne
   Fenster statt mit einem Fehlerbildschirm; die Vault bleibt nutzbar.
6. **Given** die gespeicherte Fenstergeometrie passt nicht in den aktuellen
   sichtbaren Bereich (kleineres Fenster, anderer Monitor), **When** das Fenster
   wiederhergestellt wird, **Then** wird es so angepasst, dass es vollständig
   erreichbar ist.
7. **Given** der Nutzer sperrt oder schließt die Vault, **When** der Vorgang
   abgeschlossen ist, **Then** sind alle Fenster geschlossen und kein
   Fensterzustand ist in eine spätere Vault-Session übernommen, außer dem
   gespeicherten Layout dieses Geräts.

---

### User Story 6 - Kleine Bildschirme: Fenster im Vollbild (Priority: P2)

Ein Nutzer verwendet holzi auf einem schmalen Bildschirm (Handy, schmales
Fenster). Fenster passen dort nicht nebeneinander; jede App öffnet daher im
Vollbild, und der Nutzer wechselt zwischen Apps über die Fensterübersicht.

**Why this priority**: holzi ist als portable, geräteübergreifende App gedacht;
eine Shell, die auf kleinen Bildschirmen unbedienbar ist, ist dort wertlos. Der
erste Schnitt läuft aber auf dem Desktop, deshalb P2.

**Independent Test**: Das Anwendungsfenster auf schmale Breite ziehen (oder auf
einem schmalen Gerät starten), zwei Apps öffnen: jede füllt den sichtbaren
Bereich, der Wechsel gelingt über die Fensterübersicht.

**Acceptance Scenarios**:

1. **Given** ein schmaler Bildschirm (unter der Kompakt-Schwelle, siehe
   Annahmen), **When** der Nutzer eine App öffnet, **Then** füllt ihr Fenster
   den sichtbaren Bereich des Arbeitsbereichs vollständig; Verschieben und
   Größenändern werden nicht angeboten.
2. **Given** mehrere Fenster im Kompaktmodus, **When** der Nutzer die
   Fensterübersicht öffnet und ein anderes Fenster wählt, **Then** ist genau
   dieses sichtbar und fokussiert.
3. **Given** der Nutzer verkleinert das Anwendungsfenster unter die Schwelle,
   **When** die Darstellung wechselt, **Then** gehen keine Fenster oder deren
   Inhalte verloren; beim Vergrößern erscheinen die Fenster mit ihrer zuvor
   gemerkten Geometrie wieder.
4. **Given** der Kompaktmodus, **When** der Nutzer den Launcher, die
   Arbeitsbereichs-Auswahl oder die Fensterübersicht benutzt, **Then** sind
   alle Bedienelemente ohne horizontales Scrollen erreichbar und für Touch
   ausreichend groß.

---

### User Story 7 - Fenster- und Tab-Modell mit mehreren Instanzen einer App (Priority: P3)

Ein Entwickler von holzi definiert eine App so, dass sie mehrfach geöffnet
werden darf — als eigene Tabs oder eigene Fenster — die Voraussetzung dafür,
dass eine künftige Spec (parallele Chat-Sessions) mehrere Chats parallel ermöglichen kann, ohne die Shell
umzubauen. In dieser Spec sind alle ausgelieferten Apps auf eine Instanz
begrenzt.

**Why this priority**: Für den Nutzer folgt hieraus in dieser Spec noch kein
sichtbarer Nutzen; es geht um die Tragfähigkeit des Modells. Sie muss aber
jetzt geklärt sein, weil sie Persistenz und Fensteridentität betrifft, die
später nicht mehr billig zu ändern sind.

**Independent Test**: Mit einer ausschließlich zu Testzwecken definierten
App, die mehrere Instanzen erlaubt, zweimal öffnen (einmal als Tab, einmal als
Fenster): es entstehen unabhängige Instanzen, alle überleben einen Neustart (seit
Spec 022 nur bei eingeschalteter Einstellung „Sitzung wiederherstellen“). Bei
den ausgelieferten Apps öffnet ein zweiter Aufruf keine zweite Instanz.

**Acceptance Scenarios**:

1. **Given** eine App, die nur eine Instanz erlaubt, ist geöffnet, **When** der
   Nutzer sie erneut öffnet (Launcher, „+“, alte Adresse), **Then** wird ihr
   vorhandener Tab aktiv, sein Fenster wiederhergestellt (falls minimiert) und
   fokussiert und ggf. sein Arbeitsbereich aktiv; es entsteht keine zweite
   Instanz.
2. **Given** eine App, die mehrere Instanzen erlaubt, **When** der Nutzer sie
   zweimal öffnet, **Then** entstehen zwei unabhängige Tabs (über „+“ im selben
   Fenster) bzw. Fenster (über den Launcher) mit getrenntem Inhalt.
3. **Given** zwei Tabs derselben App, **When** die Vault neu geöffnet wird,
   **Then** erscheinen beide wieder (Persistenz je Tab, nicht je App; seit Spec
   022 nur bei eingeschalteter Einstellung „Sitzung wiederherstellen“).
4. **Given** die ausgelieferten Apps Chat, Einstellungen und Föderation,
   **When** der Nutzer sie mehrfach öffnet, **Then** bleibt es je App bei einem
   Tab.

### Edge Cases

- **Laufende Antwort und Tab oder Fenster schließen**: Schließt der Nutzer den
  Chat-Tab (oder ein Fenster mit einem solchen Tab), während eine Antwort läuft
  oder eine Freigabe aussteht, fragt die Shell nach; bei Bestätigung wird die
  Antwort nach den Abbruchregeln aus Spec 003 beendet, sonst bleibt der Tab
  offen.
- **Arbeitsbereich mit laufendem Chat löschen**: Das Löschen folgt derselben
  Regel — die Bestätigung nennt die laufende Antwort ausdrücklich.
- **Schnelle Doppelaktionen**: Zweimaliges schnelles Wählen einer
  Einzelinstanz-App im Launcher oder über „+“ ergibt genau einen Tab.
- **Alle Einzelinstanz-Apps schon geöffnet**: Die Liste des „+“ zeigt sie
  trotzdem (Auswahl wechselt zum vorhandenen Tab); sie ist nie leer.
- **Sehr viele Tabs**: Die Tab-Leiste scrollt; der Chevron listet alle Tabs des
  Fensters, auch die aus der Leiste herausgescrollten, und scrollt bei
  Auswahl den Tab in den sichtbaren Bereich.
- **Ein Tab in der Leiste**: Ein Fenster mit genau einem Tab zeigt keinen
  Tab-Rahmen und keine eigene Schließen-Schaltfläche am Tab (die
  Fenster-Schaltfläche schließt den Tab und damit das Fenster).
- **Maximiert und Arbeitsbereich- oder Größenwechsel**: Ein maximiertes Fenster
  bleibt maximiert und passt sich einer geänderten Bereichsgröße an;
  Wiederherstellen kehrt zur gemerkten Normalgeometrie zurück.
- **Sehr viele Fenster**: Die Shell bleibt bedienbar; die Fensterübersicht
  scrollt, statt Einträge abzuschneiden.
- **Größenwechsel des Anwendungsfensters**: Verkleinern rückt Fenster in den
  sichtbaren Bereich, ohne ihre Größe unter die Mindestgröße zu drücken.
- **Modell-Load beim Öffnen**: Läuft der Preload aus Spec 004, bleibt die Shell
  sofort bedienbar; der Lade-/Bereitschaftszustand ist unaufdringlich in der
  Shell sichtbar, auch wenn kein Fenster offen ist.
- **Speichern schlägt fehl**: Ein Fehler beim Sichern des Layouts unterbricht
  die Bedienung nicht; die Shell arbeitet mit dem Zustand im Speicher weiter
  und der nächste Speicherversuch beim folgenden Layoutwechsel wird ausgeführt.
- **Tastatur und Screenreader**: Alle Shell-Aktionen (Launcher, Fensterwahl,
  Schließen, Minimieren, Arbeitsbereichswechsel) sind ohne Maus erreichbar und
  haben zugängliche Namen.
- **Fenster wird außerhalb eines Arbeitsbereichs geöffnet**: nicht möglich; ein
  Fenster gehört immer genau einem Arbeitsbereich.

## Requirements

### Functional Requirements

**Shell und Launcher**

- **FR-001**: Nach erfolgreichem Vault-Open und abgeschlossenem Onboarding MUSS
  holzi den aktiven Arbeitsbereich der Shell anzeigen; die Erzwingung des
  Onboardings aus Spec 002 MUSS unverändert bleiben.
- **FR-002**: Die Shell MUSS einen Launcher bereitstellen, der alle verfügbaren
  Apps mit lokalisiertem Namen und Icon listet und per Auswahl ein Fenster
  öffnet.
- **FR-003**: Die Apps dieser Spec MÜSSEN Chat, Einstellungen und Föderation
  sein. Die Einstellungs-App enthält alle Funktionen der bisherigen
  Einstellungsseite einschließlich Modell- und Provider-Verwaltung. Die
  Föderations-App zeigt vorerst den bestehenden Platzhalterinhalt.
- **FR-004**: Die bisherigen Vollseiten-Adressen für Chat, Einstellungen und
  Föderation MÜSSEN auf den Arbeitsbereich mit geöffnetem passendem Fenster
  weiterleiten.

> **Seit Spec 023** gibt es keine eigene Föderations-App mehr: Sie ist die Kategorie „Föderation“ der Einstellungen. Die alte Adresse und Aufrufe von `system.federation` öffnen die Einstellungen in dieser Kategorie, siehe [`023-settings-app`](../023-settings-app/spec.md).

- **FR-005**: ~~Die Shell MUSS den Modell-Lade-/Bereitschaftszustand (Spec 004)
  unabhängig von geöffneten Fenstern sichtbar halten.~~ Zurückgezogen durch
  Betreiberentscheidung vom 2026-09-25 (beim Test von Spec 020): Den
  Modellstatus zeigt nur der Chat, der Arbeitsbereich zeigt keine Statusleiste.

**Fenster**

- **FR-006**: Ein Arbeitsbereich MUSS beliebig viele Fenster gleichzeitig
  enthalten können; jedes Fenster gehört genau einem Arbeitsbereich und enthält
  mindestens einen Tab.
- **FR-007**: Jedes Fenster MUSS eine Titelleiste besitzen: links den Tab-Bereich
  (FR-031), rechts in dieser Reihenfolge Tab-Liste (Chevron), Minimieren,
  Maximieren/Wiederherstellen und Schließen; das aktive Fenster MUSS erkennbar
  sein. Die Schaltfläche Schließen eines Fensters mit mehreren Tabs schließt
  das ganze Fenster.
- **FR-008**: Ein Klick oder eine Tastaturaktion auf ein Fenster MUSS es
  fokussieren und in der Stapelreihenfolge nach vorn bringen.
- **FR-009**: Fenster MÜSSEN per Titelleiste verschiebbar und an Kanten und
  Ecken in der Größe änderbar sein, mit einer je App definierten Mindestgröße;
  ein Fenster MUSS immer so weit im sichtbaren Bereich bleiben, dass es
  erreichbar ist.
- **FR-010**: Fenster MÜSSEN minimiert und wiederhergestellt werden können;
  minimierte Fenster bleiben über die Fensterübersicht erreichbar.
- **FR-011**: Die Shell MUSS eine Fensterübersicht bieten, die alle offenen
  Fenster (auch minimierte) mit Icon und Titel des aktiven Tabs und der
  Tab-Anzahl zeigt und Fokussieren und Schließen erlaubt.
- **FR-012**: Neue Fenster MÜSSEN mit App-spezifischer Standardgröße und
  leicht versetzter Position erscheinen.
- **FR-013**: Der Inhalt jedes Tabs MUSS beim Minimieren, Maximieren,
  Wiederherstellen, Fokuswechsel, Tab-Wechsel und Arbeitsbereichswechsel
  erhalten bleiben; laufende Antworten werden dadurch nicht abgebrochen und
  Eingabe-Entwürfe nicht verworfen.
- **FR-014**: Schließt der Nutzer einen Tab oder ein Fenster mit einem Tab mit
  laufender Antwort oder ausstehender Freigabe, MUSS die Shell eine Bestätigung
  verlangen; nach Bestätigung MUSS die Antwort nach den Abbruchregeln aus
  Spec 003 beendet werden.
- **FR-015**: Wartet eine Antwort auf eine Freigabe und der zuständige Tab ist
  nicht sichtbar (inaktiver Tab, minimiertes Fenster oder nicht sichtbarer
  Arbeitsbereich), MUSS die Shell einen Aufmerksamkeitshinweis am Tab (Leiste
  und Tab-Liste), am Fenster (Fensterübersicht, Launcher) und am Arbeitsbereich
  zeigen.
- **FR-039** _(nachträglich ergänzt, daher hier bei den verwandten
  Fenster-Anforderungen statt nach FR-038 platziert)_: Fenster MÜSSEN
  maximiert und wiederhergestellt werden können (Schaltfläche oder
  Doppelklick auf die Titelleiste); ein maximiertes Fenster füllt den
  Arbeitsbereich, Wiederherstellen kehrt zur Geometrie vor dem Maximieren
  zurück, und der Maximierungszustand wird wie die Geometrie persistiert.

**Apps und Instanzen**

- **FR-016**: Jede App-Definition MUSS festlegen, ob sie einmal oder mehrfach
  geöffnet werden darf. Bei Einzelinstanz-Apps MUSS erneutes Öffnen (Launcher,
  „+“, alte Adresse) den vorhandenen Tab aktivieren, dessen Fenster
  wiederherstellen und fokussieren und dessen Arbeitsbereich aktivieren, statt
  eine zweite Instanz zu erzeugen.

> **Seit Spec 030** ist Chat eine Mehrfachinstanz-App; FR-017 galt nur zum
> Zeitpunkt dieser Spec. Einstellungen bleibt unverändert Einzelinstanz-App.
> Siehe [`030-app-multi-instance`](../030-app-multi-instance/spec.md).

- **FR-017**: Chat, Einstellungen und Föderation MÜSSEN in dieser Spec
  Einzelinstanz-Apps sein. Identität und Persistenz MÜSSEN je Fenster und je
  Tab statt je App geführt werden, sodass mehrere Instanzen derselben App (als
  Tabs oder Fenster) ohne Änderung des Modells möglich sind.

**Arbeitsbereiche**

- **FR-018**: Zu jedem Zeitpunkt MUSS auf einem Gerät mindestens ein
  Arbeitsbereich existieren; fehlt beim Öffnen einer Vault jeder, MUSS ein
  Standard-Arbeitsbereich angelegt werden.
- **FR-019**: Nutzer MÜSSEN Arbeitsbereiche anlegen, wechseln und löschen
  können; der letzte verbleibende Arbeitsbereich MUSS sich nicht löschen lassen.
  Arbeitsbereiche tragen keinen gespeicherten Namen: Die Oberfläche zeigt sie in
  der Sprache des Nutzers als „Arbeitsbereich N“ bzw. „Workspace N“, wobei N ihre
  Position in der Reihenfolge ist (lückenlos, auch nach dem Löschen eines
  mittleren Arbeitsbereichs).
- **FR-020**: Nutzer MÜSSEN Fenster (mit allen ihren Tabs) in einen anderen
  Arbeitsbereich verschieben können, ohne dass ein Tab-Inhalt verloren geht.
- **FR-021**: Das Löschen eines Arbeitsbereichs mit offenen Fenstern MUSS eine
  Bestätigung verlangen, die das Schließen der Fenster (und eine ggf.
  laufende Antwort) nennt; nach Bestätigung MÜSSEN die Fenster geschlossen
  werden (FR-014 gilt sinngemäß).
- **FR-022**: Die Shell MUSS eine Arbeitsbereichs-Übersicht bieten, die alle
  Arbeitsbereiche mit ihrer Nummer und der Fensteranzahl zeigt und den Wechsel
  erlaubt.

**Persistenz**

> **Seit Spec 022** gelten FR-023 bis FR-025 nur bei eingeschalteter Einstellung „Sitzung wiederherstellen“ (standardmäßig aus), siehe [`022-session-restore`](../022-session-restore/spec.md).

- **FR-023**: Arbeitsbereiche (Reihenfolge), Fenster (Arbeitsbereich,
  Position, Größe, Minimierungs- und Maximierungszustand), Tabs (App,
  Reihenfolge, aktiver Tab je Fenster) und der zuletzt aktive Arbeitsbereich
  MÜSSEN je Gerät persistiert und beim nächsten Öffnen der Vault auf demselben
  Gerät wiederhergestellt werden. Tab-Inhalte werden nicht wiederhergestellt;
  ein wiederhergestellter Chat-Tab beginnt gemäß Spec 004 mit einer neuen
  Unterhaltung.
- **FR-024**: Persistierte Arbeitsbereiche, Fenster und Tabs MÜSSEN
  gerätebezogen sein: Auf einem anderen Gerät — auch mit einer Kopie derselben
  Vault-Datei — MÜSSEN sie nicht erscheinen (Konvention aus ADR-0001).
- **FR-025**: Ein nicht lesbares Layout oder eine unbekannte App im Layout
  DARF NICHT zu einem Fehlerbildschirm führen; die Shell MUSS mit dem
  verwertbaren Rest bzw. einem Standard-Arbeitsbereich starten. Ein Tab mit
  unbekannter App wird verworfen; ein Fenster ohne verbleibenden Tab ebenso.
- **FR-026**: Wiederhergestellte Fenstergeometrie, die nicht in den aktuellen
  sichtbaren Bereich passt, MUSS so korrigiert werden, dass das Fenster
  erreichbar ist.
- **FR-027**: Beim Sperren oder Schließen der Vault MÜSSEN alle Fenster
  geschlossen und das Layout zuvor gesichert werden; kein Fensterzustand
  DARF in eine spätere Vault-Session gelangen.

**Kompaktdarstellung und Bedienung**

- **FR-028**: Unterhalb der Kompakt-Schwelle MÜSSEN Fenster den sichtbaren
  Bereich vollständig füllen, ohne Verschieben, Größenändern und Maximieren; der
  Wechsel zwischen Fenstern erfolgt über die Fensterübersicht, der zwischen Tabs
  über die Tab-Liste (FR-036). Beim Wechsel zwischen Kompakt- und normaler
  Darstellung MUSS die zuvor gemerkte Geometrie erhalten bleiben.
- **FR-029**: Alle Shell-Aktionen (Launcher, Fenster und Tabs
  fokussieren/öffnen/schließen, Minimieren, Maximieren, Fensterübersicht,
  Arbeitsbereichswechsel, -verwaltung) MÜSSEN per Tastatur bedienbar sein und
  zugängliche Namen tragen.
- **FR-030**: Alle neuen sichtbaren Texte MÜSSEN in Deutsch und Englisch im
  Gleichschritt vorliegen; der UI-Begriff für Workspace ist „Arbeitsbereich“
  (CONTEXT.md).

**Tabs** (Bedienung wie Firefox; Vorgabe des Betreibers)

- **FR-031**: Der Tab-Bereich der Titelleiste MUSS bei einem Tab Icon und Titel
  ohne Tab-Rahmen zeigen und bei mehreren Tabs eine Leiste aus Tabs mit Icon,
  Titel und je einer Schließen-Schaltfläche; der aktive Tab MUSS erkennbar sein.
- **FR-032**: Unmittelbar hinter dem letzten Tab (bzw. dem Titel bei einem Tab)
  MUSS ein „+“ stehen, das eine Liste der öffnenbaren Apps zeigt; die Auswahl
  MUSS in diesem Fenster einen neuen Tab öffnen und aktivieren. Bei mehr Tabs,
  als in die Leiste passen, MUSS das „+“ am rechten Ende der Leiste sichtbar
  bleiben.
- **FR-033**: Eine Einzelinstanz-App MUSS über das „+“ keinen zweiten Tab
  erzeugen; die Auswahl aktiviert stattdessen den vorhandenen Tab (FR-016). Die
  Liste des „+“ MUSS jede App zeigen (nie leer).
- **FR-034**: Im rechten Bereich der Titelleiste, vor Minimieren, Maximieren und
  Schließen, MUSS ein Chevron stehen, der ein Dropdown mit **allen** Tabs des
  Fensters öffnet (Icon, Titel, aktiver Tab markiert, Aufmerksamkeitshinweis);
  die Auswahl MUSS den Tab aktivieren und in der Leiste sichtbar machen.
- **FR-035**: Passen die Tabs nicht in die Leiste, MUSS sie scrollbar sein und
  den aktiven Tab sichtbar halten.
- **FR-036**: In der Kompaktdarstellung MUSS die Leiste durch Titel des aktiven
  Tabs, „+“ und den Chevron mit dem Tab-Dropdown ersetzt werden.
- **FR-037**: Das Schließen eines Tabs MUSS seinen rechten Nachbarn aktivieren
  (beim letzten Tab den linken); das Schließen des letzten Tabs MUSS das Fenster
  schließen.
- **FR-038**: Die Tab-Leiste MUSS für Tastatur und Screenreader als Tab-Liste
  bedienbar sein (Pfeiltasten wechseln zwischen Tabs, Eingabe aktiviert);
  „+“ und Chevron MÜSSEN ohne Maus erreichbar sein.
- **FR-039** (Nachtrag 2026-09-30): Die Chat-App MUSS ohne eigene Kopfzeile
  auskommen (kein Titel, keine Modell-Anzeige, keine Knöpfe für Sperren,
  Einstellungen oder neuen Chat oberhalb des Inhalts). Die Eingabe MUSS immer am
  unteren Rand des Fensters sitzen. Die Seitenleiste mit „Neuer Chat“ und den
  Verläufen folgt der Breite des Fensters, nicht der des Bildschirms: ab 42 rem
  Fensterbreite steht sie neben dem Inhalt, ein Knopf in der Werkzeugzeile
  blendet sie aus und wieder ein; darunter ist sie ausgeblendet, derselbe Knopf
  öffnet sie über dem Inhalt, und Auswahl eines Verlaufs, „Neuer Chat“ oder
  Escape schließen sie wieder. Der Zustand gilt nur für den geöffneten Tab und
  wird nicht gespeichert. Das Muster entspricht der Einstellungs-App (Spec 023,
  FR-004).
- **FR-040** (Nachtrag 2026-09-30): Die Seitenleiste des Chats MUSS nur den
  Chat-Inhalt tragen (neue Unterhaltung, Verläufe), keine Einstellungen und kein
  Sperren. Einstellungen öffnet der Launcher als App; das Sperren der Vault ist
  der letzte Eintrag des Launchers (Spec 013, `useVaultLock`) und damit unabhängig
  von Fenstergröße und geöffneter App erreichbar.

### Key Entities

- **Arbeitsbereich (Workspace)**: Ein geordneter Container für Fenster. Gehört
  einem Gerät (nicht der Vault insgesamt). Attribut: Reihenfolge; einen Namen
  gibt es nicht, die Anzeige „Arbeitsbereich N“ ergibt sich aus der Position.
  Pro Gerät ist genau einer der aktive.
- **Fenster (Window)**: Ein Rahmen mit Titelleiste in genau einem
  Arbeitsbereich, der einen oder mehrere Tabs enthält. Attribute:
  Arbeitsbereich, Position, Größe (Normalgeometrie), Minimierungs- und
  Maximierungszustand, Stapelreihenfolge, geordnete Tabs, aktiver Tab. Hat eine
  eigene Identität, unabhängig von seinen Tabs.
- **Tab**: Eine geöffnete App-Instanz in genau einem Fenster. Attribute: App,
  Position in der Leiste, Aufmerksamkeitshinweis (flüchtig). Hat eine eigene
  Identität, unabhängig von der App; nur ein Tab je Fenster ist aktiv.
- **App-Definition (App)**: Beschreibt, was in einem Tab läuft. Attribute:
  Kennung, lokalisierter Name, Icon, Standardgröße und Mindestgröße für ein
  neues Fenster, ob mehrere Instanzen erlaubt sind. In dieser Spec: Chat,
  Einstellungen, Föderation.
- **Layout eines Geräts**: Die Gesamtheit aus Arbeitsbereichen, ihren
  Fenstern samt Tabs und dem zuletzt aktiven Arbeitsbereich für ein Gerät der
  Vault.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Ein Nutzer öffnet nach dem Vault-Open in höchstens zwei
  Interaktionen (Launcher öffnen, App wählen) ein Fenster von Chat bzw.
  Einstellungen.
- **SC-002**: Bei zehn gleichzeitig geöffneten Fenstern folgt ein gezogenes
  oder in der Größe geändertes Fenster dem Zeiger mit höchstens 50 ms sichtbarer
  Verzögerung, und Fokus- und Minimierungsaktionen wirken innerhalb von 100 ms.
- **SC-003**: Nach Beenden und Neuöffnen sind 100 % der Arbeitsbereiche,
  Fenster und Tabs eines Testlayouts mit fünf Arbeitsbereichen, zehn Fenstern und
  zwanzig Tabs identisch in Reihenfolge, Zuordnung, Position, Größe,
  Minimierungs- und Maximierungszustand und aktivem Tab wiederhergestellt.
- **SC-004**: Bei Minimieren, Maximieren, Fokuswechsel, Tab-Wechsel und
  Arbeitsbereichswechsel gehen in keinem getesteten Fall ein Eingabe-Entwurf
  oder eine laufende Antwort verloren.
- **SC-005**: Auf einem Bildschirm unterhalb der Kompakt-Schwelle sind alle
  Shell-Bedienelemente und alle drei Apps ohne horizontales Scrollen vollständig
  bedienbar.
- **SC-006**: Eine Kopie der Vault auf einem zweiten Gerät zeigt dort in 100 %
  der Fälle nicht die Arbeitsbereiche des Quellgeräts, sondern einen
  Standard-Arbeitsbereich.
- **SC-007**: Sämtliche Akzeptanzszenarien der Specs 002 und 004, die Chat und
  Einstellungen betreffen, bleiben innerhalb der Fenster erfüllt (keine
  Funktionsregression gegenüber den Vollseiten).
- **SC-008**: Für alle neuen sichtbaren Texte existieren deutsche und
  englische Fassungen; kein Text erscheint im Fallback oder als roher Schlüssel.
- **SC-009**: In einem Fenster mit zehn Tabs erreicht ein Nutzer jeden Tab mit
  höchstens zwei Klicks (Chevron, Eintrag), und Tab-Aktionen (wechseln, öffnen,
  schließen) wirken innerhalb von 100 ms.

## Assumptions

- Die Shell ist eine In-App-Oberfläche in **einem** Anwendungsfenster. Separate
  Betriebssystemfenster für Apps sind nicht Teil dieser Spec.
- **Kompakt-Schwelle**: Unterhalb einer Anwendungsfensterbreite von 768
  CSS-Pixeln gilt die Kompaktdarstellung (Vollbild-Fenster). Der Wert ist eine
  übliche Tablet-Grenze und in der Planung veränderbar.
- **Tabs in Fenstern, Bedienung wie Firefox** (Vorgabe des Betreibers). Anders
  als in haex-vault 2.x sitzt das „+“ nicht rechts bei den Fenster-Schaltflächen,
  sondern unmittelbar hinter dem letzten Tab; an seiner alten Stelle steht der
  Chevron mit der Tab-Liste. Das „+“ öffnet wie in haex-vault 2.x eine Liste der
  Apps (kein automatisches Duplizieren des aktiven Tabs).
- **Ein Tab zeigt keinen Tab-Rahmen** (wie haex-vault 2.x): Icon und Titel, ohne
  eigene Schließen-Schaltfläche; ab zwei Tabs erscheint die Tab-Leiste mit
  Schließen-Schaltflächen je Tab.
- **Fenster- und Tab-Titel** kommen aus der App-Definition; ein dynamischer Titel
  der App (etwa Thread-Name) ist möglich, aber kein Anforderungsinhalt.
- **Maximieren füllt den Arbeitsbereich**, nicht den Bildschirm; Doppelklick auf
  die Titelleiste schaltet um (wie haex-vault 2.x).
- **Kein Desktop-Raster.** Symbole oder Verknüpfungen direkt auf dem
  Arbeitsbereich (haex-vault 2.x) sind nicht Teil dieser Spec; Apps starten aus dem
  Launcher.
- **Wiederherstellung betrifft Fenster und Tabs, nicht deren Inhalte.** Ein
  wiederhergestellter Chat-Tab beginnt mit einer neuen Unterhaltung, weil
  Spec 004 jeden Chat-Einstieg so definiert; Entwürfe überleben keinen
  Neustart.
- **Löschen mit offenen Fenstern schließt diese** (nach Bestätigung) und
  verschiebt sie nicht in einen anderen Arbeitsbereich.
- **Chat bleibt in dieser Spec auf einen Tab begrenzt**, weil das Backend
  heute app-weit nur einen laufenden Turn zulässt; das Aufheben dieser
  Grenze ist eine künftige, noch nicht nummerierte Spec.
- **Die Föderations-App ist ein Platzhalter.** Sie existiert, damit die
  Zuordnung „bisherige Vollseite → Fenster“ vollständig ist und eine App ohne
  echten Inhalt getestet wird. Geplant ist, die Föderation in einer Folge-Spec
  zur Einstellungs-App als Kategorie der Einstellungen zu machen; die
  Föderations-App entfällt dann (siehe „Geplante Folge-Specs“).
- **Vorgaben aus dem Projekt** (Randbedingungen an die Planung, keine
  Anforderungen an Nutzer):
  - Persistenz liegt im Vault-Storage auf der Rust-Seite (Tauri-Commands,
    ts-rs-Bindings, wie Preferences und Threads) und folgt der
    Geräte-Konvention aus ADR-0001 — nicht per Drizzle aus dem Frontend wie
    in haex-vault 2.x.
  - Die Oberfläche nutzt den im Projekt gesetzten Stack (haex-ui-Layer mit
    shadcn-vue/reka-ui, Tailwind, `@lucide/vue`), nicht Nuxt UI wie
    haex-vault 2.x; deshalb werden dessen Komponenten portiert, nicht kopiert.
  - Sichtbare Texte laufen über `@nuxtjs/i18n` in `de.json` und `en.json`
    (CONTEXT.md, Abschnitt Internationalisierung).
  - Ein Design-Beschluss zum Zuschnitt (Rust-seitiger Speicher, Fenstermodell mit
    Instanzidentität) ist bei Bedarf als ADR unter `docs/adr/` festzuhalten
    (Constitution).

## Nicht im Umfang

- Haextension-Host und Extensions als Fensterinhalt (Spec 017).
- MCP-Anbindung von Extensions, in beide Richtungen (Spec 017/018).
- Parallele Chat-Sessions im Backend und mehrere gleichzeitige Chat-Tabs oder
  -Fenster (künftige, noch nicht nummerierte Spec — Spec 016 ist bereits an
  `016-e2e-testing` vergeben).
- Desktop-Symbole und -Raster auf dem Arbeitsbereich, Drag-and-Drop von
  Symbolen (Folge-Spec geplant).
- Natives Betriebssystemfenster je App (Folge-Spec geplant).
- Tabs per Drag umsortieren, zwischen Fenstern verschieben oder zu einem
  eigenen Fenster lösen (Folge-Spec geplant) sowie Tastenkürzel wie Strg+T,
  Strg+W, Strg+Tab (Folge-Spec geplant).
- Navigation innerhalb eines Tabs (Vor/Zurück, Unteransichten einer App;
  Folge-Spec geplant).
- Neue Inhalte der Föderations-App.
- Synchronisation von Arbeitsbereichen zwischen Geräten (bewusst gerätebezogen).

### Geplante Folge-Specs

Mit dem Betreiber abgestimmt (2026-09-25); Nummern werden bei der
Spezifikation vergeben. Diese Spec setzt sie nicht um, ihr Modell soll ihnen
aber nicht im Weg stehen.

1. **Navigation im Tab**: Jeder Tab hat einen Ort innerhalb seiner App und
   eine eigene Vor/Zurück-Historie wie ein Browser-Tab; dazu eine
   Befehls-Registry mit festen Standard-Tastenkürzeln.
2. **Einstellungs-App im Stil von haex-vault 2.x**: Kategorien-Seitenleiste,
   Unteransichten als Navigation im Tab, Föderation als Kategorie,
   Erscheinungsbild.
3. **Befehle und Tastenkürzel**: einheitliches Konzept, zur Laufzeit vom
   Nutzer umbelegbar.
4. **Desktop-Symbole und Raster** auf dem Arbeitsbereich, ohne Überlappungen.
5. **Tabs per Drag & Drop**: umsortieren, zwischen Fenstern verschieben, zu
   einem eigenen Fenster lösen.
6. **Native Fenster** je App (zumindest auf dem Desktop).

Voraussetzungen, die das Modell dieser Spec dafür schon erfüllt oder erfüllen
soll: Ein Tab behält seine Identität, wenn er das Fenster wechselt (FR-017);
ein Fenster soll später außer im Arbeitsbereich auch als natives Fenster
dargestellt werden können.

## Referenzen

Zum Portieren von Verhalten, nicht zum 1:1-Kopieren. Unveränderlich gepinnt
(Constitution: Cross-Repo-Referenzen):

- `haex-space/haex-vault-legacy` @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`
  - `src/stores/desktop/windowManager/state.ts`, `lifecycle.ts`, `tabs.ts`
    — Fenstermodell, Lebenszyklus, Tab-Logik (Singleton-Suche über alle
    Fenster, Nachbar-Aktivierung beim Schließen; die aus dem aktiven Tab
    abgeleiteten Legacy-Felder nicht übernehmen)
  - `src/stores/desktop/workspace.ts` — Arbeitsbereichs-Verwaltung
  - `src/components/haex/window/index.vue`, `button.vue`, `overview.vue`,
    `resizeHandles.vue` — Fenster mit Tab-Leiste (Scroll-Pfeile bei Überlauf,
    „+“-Menü, Kompakt-Auswahl), Fenster-Schaltflächen inkl. Maximieren,
    Fensterübersicht, Größenänderung
  - `src/components/haex/desktop/overview-carousel.vue`,
    `workspace-slide.vue` — Arbeitsbereichs-Übersicht und -Wechsel
  - `src/components/haex/workspace/card.vue`, `drawer.vue` —
    Arbeitsbereichs-Karte und -Schublade
