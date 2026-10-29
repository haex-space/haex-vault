# Feature Specification: Dateibrowser mit zwei Panels (Split-Ansicht)

**Feature Branch**: `062-files-split-view`

**Created**: 2026-10-26

**Status**: Implemented (manual M6, M13, A1 and the checks of T044, T048, T052, T055, T056, T062, T063, T064, T067, T069 open)

**Input**: User description: "Dateibrowser: Split-Ansicht mit zwei unabhängigen Panels nebeneinander
(wie Total Commander). Jedes Panel hat eigenes Verzeichnis, eigene Navigation und Auswahl; ein
aktives Panel (Tab wechselt, F5 kopiert ins andere Panel, F6 verschiebt); Drag & Drop zwischen
Panels; jedes Panel kann eine andere Quelle/einen anderen Speicher zeigen (Kopieren zwischen
Speichern); Toolbar-Schalter an/aus, merkt sich die Pfade; auf dem Handy nicht verfügbar. Zwei
Ordneransichten (kein eigener Verzeichnisbaum pro Panel)."

## Begriffe

- **Panel**: eine Ordneransicht im Dateibrowser mit eigener Pfadleiste, eigener Liste oder eigenem
  Raster und eigener Auswahl. Ohne Split gibt es genau eines, mit Split zwei: links und rechts.
- **Split-Ansicht**: der Zustand eines Dateibrowser-Tabs, in dem zwei Panels nebeneinander stehen.
- **Aktives Panel**: das Panel, auf das Tastatur, Zwischenablage, Seitenleiste und Werkzeugleiste
  wirken. Es ist am Rahmen erkennbar. Das andere heißt **Gegenpanel**.
- **Quelle**, **Eintrag**, **Transfer**, **Viewer**: wie in Spec 044.

## Beziehung zu bestehenden Specs

- **Spec 044** (Dateibrowser und Viewer): Diese Spec erweitert den Dateibrowser, ändert aber weder
  Quellen noch Transfers, Namenskonflikte (FR-021), den Schutz vor Kopieren in sich selbst (FR-022),
  Löschen (FR-023) oder den Schutz der eigenen Daten von holzi (FR-037). Kopieren und Verschieben
  zwischen den Panels sind dieselben Transfers wie bisher (FR-018 bis FR-020, FR-025, FR-026).
  FR-007 (jeder Tab hat seinen Ort) gilt weiter: Der Split gehört zum Tab, nicht zum Fenster.
  FR-017b belegt F5 mit „Neuladen“; in der Split-Ansicht übernimmt F5 das Kopieren (FR-008), Neuladen
  bleibt über Strg+R.
- **Spec 056** (Dateizugriff teilen): Andere eigene Geräte und empfangene Freigaben stehen als Quellen
  zur Wahl wie bisher; Schreibschutz einer Freigabe gilt auch, wenn sie Ziel eines Transfers aus dem
  Gegenpanel ist.
- **Spec 048** (verschlüsselte Ordner): Die Regeln für verschlüsselte Ordner gelten unverändert, auch
  wenn Quelle oder Ziel eines Transfers in einem Panel liegt.
- **Spec 022** (Sitzung wiederherstellen): Die Split-Ansicht kommt mit beiden Orten zurück.
- **Spec 036/043** (Handy): Auf Handys gibt es die Split-Ansicht nicht.

## Clarifications

### Session 2026-10-26

- Q: Zwei Ordneransichten oder je Seite ein Verzeichnisbaum? → A: Zwei Ordneransichten mit
  Pfadleiste, wie bei Total Commander; kein eigener Baum je Panel (Betreiber).
- Q: Wie wird kopiert und verschoben? → A: Über ein aktives Panel: Tab wechselt das Panel, F5
  kopiert in das Gegenpanel, F6 verschiebt; dazu Ziehen zwischen den Panels (Betreiber).
- Q: Dürfen die Panels verschiedene Quellen zeigen? → A: Ja, jedes Panel wählt seine Quelle frei, so
  wird Kopieren zwischen Speichern möglich (Betreiber).
- Q: Wie schaltet man den Split ein? → A: Ein Schalter in der Werkzeugleiste; ein- und ausschalten
  merkt sich die Orte beider Panels (Betreiber).
- Q: Und auf dem Handy? → A: Nicht verfügbar; es wird dort nicht gebraucht (Betreiber).
- Bestätigt am 2026-10-26 (Betreiber): Aufbau der Panels und Bedienung mit aktivem Panel „wie bei
  Total Commander“; freie Quellwahl je Panel; Schalter in der Werkzeugleiste, Handy ohne Split.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Zwei Ordner nebeneinander und dazwischen kopieren (Priority: P1)

Anna sortiert Fotos. Sie schaltet in der Werkzeugleiste die Split-Ansicht ein. Links steht der
Ordner „Kamera“, rechts erscheint ein zweites Panel, das zunächst denselben Ordner zeigt. Sie
navigiert rechts nach „Urlaub 2026“, wählt links drei Fotos aus und drückt F5. Die Fotos erscheinen
rechts. Mit F6 verschiebt sie weitere.

**Why this priority**: Das ist der Kern des Wunsches: zwei Orte gleichzeitig sehen und Dateien
dazwischen bewegen, ohne Tabs zu wechseln.

**Independent Test**: Split einschalten, in beiden Panels in verschiedene Testordner navigieren, links
Dateien auswählen und per F5 kopieren, per F6 verschieben; der Inhalt stimmt auf der Platte, und
beide Panels zeigen ihn danach.

**Acceptance Scenarios**:

1. **Given** der Dateibrowser zeigt einen Ordner, **When** Anna den Split-Schalter in der
   Werkzeugleiste betätigt, **Then** stehen zwei Panels nebeneinander, das zweite zeigt anfangs den
   Ort des ersten, und das bisherige Panel bleibt aktiv.
2. **Given** zwei Panels in verschiedenen Ordnern, **When** Anna in einem Panel navigiert, sortiert
   oder Einträge auswählt, **Then** ändert sich nur dieses Panel; das andere behält Ordner und
   Auswahl.
3. **Given** das aktive Panel hat eine Auswahl, **When** Anna F5 drückt, **Then** kopiert holzi sie in
   den Ordner des Gegenpanels, mit Transferleiste, Abbruch und Namenskonflikt-Frage wie bisher.
4. **Given** das aktive Panel hat eine Auswahl, **When** Anna F6 drückt, **Then** verschiebt holzi
   sie in den Ordner des Gegenpanels; die Einträge verschwinden aus dem Quellpanel und erscheinen im
   Gegenpanel.
5. **Given** ein Panel hat Fokus in der Liste, **When** Anna Tab drückt, **Then** wird das andere Panel
   aktiv, mit dem Fokus auf seinem zuletzt fokussierten Eintrag; ein Klick in ein Panel macht es
   ebenfalls aktiv.
6. **Given** nichts ist im aktiven Panel ausgewählt, **When** Anna F5 oder F6 drückt, **Then**
   geschieht nichts.
7. **Given** beide Panels zeigen denselben Ordner, **When** in einem Panel eine Datei angelegt oder
   gelöscht wird, **Then** zeigt das andere die Änderung ebenfalls ohne Neuladen.

---

### User Story 2 - Zwischen Panels ziehen (Priority: P2)

Ben bevorzugt die Maus. Er zieht Dateien aus dem linken Panel auf einen Ordner im rechten oder auf
dessen freie Fläche.

**Why this priority**: Ziehen ist neben den Tasten der zweite natürliche Weg und existiert in
holzi schon innerhalb eines Panels (Spec 044 FR-018).

**Independent Test**: In zwei Panels mit Testordnern eine Auswahl auf einen Ordner im Gegenpanel
ziehen, dann auf dessen freie Fläche, jeweils mit und ohne Strg (auf macOS Alt).

**Acceptance Scenarios**:

1. **Given** zwei Panels derselben Quelle, **When** Ben eine Auswahl auf einen Ordner im Gegenpanel
   zieht, **Then** verschiebt holzi sie in diesen Ordner; mit gehaltener Strg-Taste (macOS: Alt)
   kopiert sie. Zeigen die Panels verschiedene Quellen, kopiert holzi immer, wie bisher (Spec 044).
2. **Given** zwei Panels, **When** Ben eine Auswahl auf die freie Fläche des Gegenpanels zieht,
   **Then** landet sie im geöffneten Ordner des Gegenpanels, mit derselben Regel für Verschieben und
   Kopieren.
3. **Given** ein Drag über einem Panel, **When** das Ziel nicht schreibbar ist (etwa eine
   schreibgeschützte Freigabe), **Then** nimmt holzi den Drop nicht an und zeigt das am Ziel an.
4. **Given** Dateien aus dem System-Dateimanager, **When** Ben sie auf eines der beiden Panels zieht,
   **Then** kopiert holzi sie in den Ordner dieses Panels (Spec 044 FR-024).

---

### User Story 3 - Panels mit verschiedenen Quellen (Priority: P2)

Clara hat links ihr Gerät, rechts einen S3-Speicher. Sie kopiert ein Verzeichnis hoch, wählt dann
rechts ein anderes Gerät aus der Seitenleiste und holt eine Datei von dort.

**Why this priority**: Kopieren zwischen Geräten und Speichern ist der Hauptnutzen gegenüber zwei
getrennten Tabs, und die Seitenleiste muss dafür mit zwei Panels zurechtkommen.

**Independent Test**: Links Gerät, rechts einen Speicher (lokaler S3-Testserver) wählen, hoch- und
herunterkopieren; danach rechts eine andere Quelle wählen.

**Acceptance Scenarios**:

1. **Given** zwei Panels, **When** Clara in der Seitenleiste einen Ort oder Speicher wählt, **Then**
   öffnet er sich im aktiven Panel; das andere bleibt unverändert.
2. **Given** links das Gerät und rechts ein Speicher, **When** Clara F5 drückt, **Then** läuft ein
   Transfer wie bisher (Hochladen mit Fortschritt, Abbruch ohne halbes Objekt).
3. **Given** das Gegenpanel zeigt eine schreibgeschützte Quelle (eine Freigabe ohne Schreibrecht,
   eine Quelle ohne Zugang), **When** Clara F5 oder F6 drückt oder dorthin zieht, **Then** lehnt holzi
   ab und nennt den Grund; verschoben wird nichts.
4. **Given** die Quelle eines Panels ist nicht erreichbar, **When** das Panel sie öffnet, **Then**
   zeigt nur dieses Panel den Grund (wie Spec 044 US5 Szenario 4); das andere Panel bleibt
   bedienbar.

---

### User Story 4 - Split merkt sich Orte und Zustand (Priority: P3)

Dora schaltet den Split aus, um mehr Platz zu haben, und später wieder ein. Beide Panels stehen
wieder in ihren Ordnern. Nach einem Neustart mit wiederhergestellter Sitzung ist es genauso.

**Why this priority**: Macht den Schalter praktisch; ohne wäre jedes Ausschalten ein Verlust.

**Independent Test**: Split einschalten, Panels in verschiedene Ordner stellen, ausschalten,
einschalten; neu starten (Sitzung wiederherstellen) und prüfen.

**Acceptance Scenarios**:

1. **Given** eine Split-Ansicht, **When** Dora den Schalter ausschaltet, **Then** bleibt das aktive
   Panel als einzige Ansicht des Tabs mit Ordner, Sortierung und Suche; der Ort des anderen Panels
   wird gemerkt.
2. **Given** ein Tab, in dem der Split ausgeschaltet wurde, **When** Dora ihn wieder einschaltet,
   **Then** zeigt das zweite Panel den gemerkten Ort; ist die Quelle nicht erreichbar, zeigt es
   den Grund, und Dora wählt dort einen anderen Ort. Gibt es keinen gemerkten Ort, zeigt es den Ort
   des ersten Panels.
3. **Given** ein Tab in der Split-Ansicht, **When** Dora holzi neu startet und die Sitzung
   wiederherstellt (Spec 022), **Then** steht der Tab wieder in der Split-Ansicht mit beiden Orten
   und dem aktiven Panel.
4. **Given** zwei Tabs des Dateibrowsers, **When** Dora in einem den Split einschaltet, **Then**
   bleibt der andere Tab unverändert.

---

### User Story 5 - Gleichzeitige Nutzung ohne Überraschungen (Priority: P3)

Der Split soll sich in die restliche Bedienung einfügen: Zwischenablage, Viewer, Suche und Zurück
folgen dem aktiven Panel, und ein schmales Fenster oder ein Handy zerstört nichts.

**Why this priority**: Ohne diese Regeln wäre der Split zwar benutzbar, aber widersprüchlich zu dem,
was der Dateibrowser sonst tut.

**Independent Test**: Im Split mit Strg+C/V, Strg+F, Alt+←, Datei öffnen und Fenster verkleinern
prüfen.

**Acceptance Scenarios**:

1. **Given** das aktive Panel hat eine Auswahl, **When** Anna Strg+C drückt, wechselt per Tab in das
   andere Panel und drückt Strg+V, **Then** wird in dessen Ordner eingefügt.
2. **Given** eine Datei im Panel, **When** Anna sie öffnet, **Then** öffnet der Viewer über der
   ganzen Ansicht, und „vorige/nächste Datei“ springt innerhalb des Ordners dieses Panels; nach dem
   Schließen steht der Split wie vorher.
3. **Given** eine Split-Ansicht, **When** Anna Strg+F, Alt+↑ oder die Seitenleiste nutzt, **Then**
   wirkt das auf das aktive Panel; Pfadleiste, Such- und Filterleiste gehören je einem Panel.
4. **Given** Anna hat links zwei Ordner nacheinander geöffnet und rechts einen, **When** sie bei
   aktivem rechten Panel Alt+← drückt, **Then** geht nur das rechte Panel zurück; wechselt sie
   mit Tab nach links und drückt Alt+← nochmals, geht das linke einen Ordner zurück und das rechte
   bleibt stehen. Die Zurück- und Vor-Schaltflächen und ihre Verlaufsliste zeigen jeweils den
   Verlauf des aktiven Panels.
5. **Given** ein Fenster schmaler als das Minimum für zwei Panels, **When** Anna es verkleinert,
   **Then** zeigt holzi nur das aktive Panel; der Split-Zustand bleibt erhalten und zwei Panels
   erscheinen wieder, sobald es breit genug ist. Der Split-Schalter ist in dieser Breite deaktiviert
   und nennt den Grund.
6. **Given** ein Handy (Android, iOS), **When** Anna den Dateibrowser öffnet, **Then** gibt es keinen
   Split-Schalter, und ein aus der Sitzung eines anderen Geräts stammender Split-Zustand wird
   ignoriert.

---

### Edge Cases

- **Beide Panels im selben Ordner**: F5 kopiert in sich selbst und führt zur Namenskonflikt-Frage
  (Spec 044 FR-021, „beide behalten“); F6 tut nichts und nennt den Grund.
- **Ein Panel zeigt einen Unterordner des Ordners des anderen**: Verschieben eines Ordners in sich
  selbst oder in einen Unterordner lehnt holzi vor dem Start ab (FR-022 aus 044).
- **Der Ordner eines Panels verschwindet** (gelöscht, Laufwerk entfernt): Nur dieses Panel springt
  zum nächsten vorhandenen übergeordneten Ordner und zeigt den Hinweis (Spec 044).
- **Das Gegenpanel zeigt einen Ort von holzi selbst** (eigene Daten): Als Ziel nicht schreibbar, wie in
  Spec 044 FR-037.
- **Verschieben über Quellen hinweg**: wirkt wie Kopieren und Löschen am Ursprung nach vollständigem
  Kopieren; scheitert der Transfer, bleibt der Ursprung unverändert (Spec 044 FR-020).
- **Der Transfer berührt den Ordner eines Panels**: Beide Panels zeigen den neuen Stand, sobald er
  fertig ist, auch während derselbe Ordner in beiden offen ist.
- **Vault wird gesperrt**: Transfers brechen ab (FR-026 aus 044); beim Entsperren steht der Split
  wie vorher.
- **Tab-Taste und Tastaturbedienung**: Tab dient im Split auch dem Panelwechsel. Der Rest der
  Oberfläche (Werkzeugleiste, Seitenleiste) bleibt trotzdem mit der Tastatur erreichbar.
- **Löschen im aktiven Panel**: Entf wirkt nur auf die Auswahl des aktiven Panels, nie auf die des
  anderen.
- **Gleiche Auswahl in beiden Panels**: Auswahlen sind je Panel; Strg+A wählt nur im aktiven.

## Requirements _(mandatory)_

### Functional Requirements

**Split-Ansicht**

- **FR-001**: Die Werkzeugleiste des Dateibrowsers MUSS einen Schalter haben, der die Split-Ansicht
  für den Tab ein- und ausschaltet. Er MUSS seinen Zustand erkennbar machen.
- **FR-002**: Die Split-Ansicht MUSS zwei Panels nebeneinander zeigen, links und rechts, mit
  fester Trennlinie. Ein Verschieben der Trennlinie ist nicht nötig; beide Panels sind gleich
  breit. [Annahme in Assumptions]
- **FR-003**: Jedes Panel MUSS eine eigene Quelle, einen eigenen Ordner, eine eigene Pfadleiste, eine
  eigene Auswahl, einen eigenen Fokus und eigene Such- und Filterwerte haben. Eine Handlung in einem
  Panel DARF das andere nicht verändern. Ein eigener Verzeichnisbaum je Panel gehört nicht dazu.
- **FR-004**: Genau ein Panel MUSS aktiv sein; es MUSS am Rahmen erkennbar sein, und zwar nicht nur
  durch Farbe. Der Wechsel MUSS erfolgen durch Klick in ein Panel, durch Tab bei Fokus in einer
  Liste und durch Fokussieren eines Elements im Panel.
- **FR-005**: Beim Einschalten MUSS das zweite Panel den gemerkten Ort des letzten Splits in diesem
  Tab zeigen, falls es einen gibt, sonst den Ort des ersten Panels. Ist die gemerkte Quelle nicht
  erreichbar, MUSS das Panel den Grund zeigen (FR-020); der Ort wird nicht still ersetzt. Das
  bisherige Panel MUSS aktiv bleiben.
- **FR-006**: Beim Ausschalten MUSS das aktive Panel zur einzigen Ansicht des Tabs werden; der Ort des
  anderen Panels MUSS gemerkt werden (FR-005).
- **FR-007**: Der Zustand einer Split-Ansicht (an oder aus, Ort beider Panels, aktives Panel, der
  gemerkte Ort) MUSS zum Tab gehören und mit der Sitzung (Spec 022) wiederhergestellt werden.
  Andere Tabs DÜRFEN davon nicht berührt werden.

**Kopieren und Verschieben**

- **FR-008**: In der Split-Ansicht MUSS F5 die Auswahl des aktiven Panels in den Ordner des
  Gegenpanels kopieren und F6 sie dorthin verschieben. Ohne Auswahl DARF nichts passieren. Neuladen
  MUSS in der Split-Ansicht über Strg+R weiter möglich sein (und Cmd+R auf macOS). Außerhalb der
  Split-Ansicht bleibt F5 „Neuladen“ (Spec 044 FR-017b). Die Kürzel MÜSSEN im Menü und an den Schaltflächen der
  Aktionsleiste genannt werden, wo sie gelten.
- **FR-009**: Ziehen von Einträgen in das Gegenpanel MUSS möglich sein, auf einen Ordner dort (Ziel
  ist dieser Ordner) und auf die freie Fläche (Ziel ist der geöffnete Ordner). Innerhalb einer Quelle
  MUSS ohne Zusatztaste verschoben werden, mit Strg (macOS: Alt) kopiert; zwischen verschiedenen
  Quellen MUSS immer kopiert werden. Das ist die Regel aus Spec 044, unverändert.
- **FR-010**: Kopieren, Verschieben und Ziehen zwischen den Panels MÜSSEN dieselben Transfers sein wie
  in Spec 044, mit Transferleiste, Fortschritt, Abbruch, Namenskonflikt-Frage, Abbruch ohne halbe
  Datei und den Regeln für verschlüsselte Ordner, Freigaben und eigene Daten von holzi. Neue
  Ausnahmen DARF die Split-Ansicht nicht einführen.
- **FR-011**: Ist das Ziel nicht schreibbar (Freigabe ohne Schreibrecht, eigene Daten von holzi,
  Quelle ohne Zugang), MUSS holzi F5, F6 und Ziehen mit einem Grund ablehnen und nichts ändern; ein
  Drag MUSS dem Nutzer schon beim Überfahren anzeigen, dass dort nicht abgelegt wird.
- **FR-012**: Strg+C, Strg+X, Strg+V, Entf, F2 und die anderen Kürzel aus Spec 044 FR-017b MÜSSEN auf
  das aktive Panel wirken. Strg+V MUSS in den Ordner des aktiven Panels einfügen.
- **FR-013**: Zeigen beide Panels denselben Ordner (oder ist ein Ordner im anderen enthalten), MÜSSEN
  F5 und F6 die Regeln aus Spec 044 (FR-021, FR-022) anwenden: F5 in denselben Ordner führt in die
  Namenskonflikt-Frage, F6 in denselben Ordner nennt den Grund und tut nichts.

**Navigation, Quellen und Anzeige**

- **FR-014**: Die Seitenleiste (bekannte Orte, Laufwerke, Speicher, andere Geräte, Freigaben) MUSS
  geteilt bleiben; eine Wahl dort MUSS im aktiven Panel öffnen. Jedes Panel MUSS jede Quelle aus
  Spec 044 und 056 zeigen können, auch verschieden voneinander.
- **FR-015**: Jedes Panel MUSS seinen eigenen Verlauf haben. Zurück, Vor (Alt+←, Alt+→, die
  Schaltflächen des Window Managers samt Verlaufsliste) und Hinauf (Alt+↑, Schaltfläche) MÜSSEN auf
  das aktive Panel wirken und das andere nicht verändern. Mit dem aktiven Panel wechselt, was
  Zurück und Vor tun. Jeder Schritt in einem Panel (Ordner öffnen, Quelle wechseln, Datei öffnen)
  ist ein Eintrag in dessen Verlauf; Suche und Filter sind es nicht (wie in Spec 020). Nach einem
  Neustart mit Sitzung (Spec 022) MUSS der Verlauf des aktiven Panels zurückkommen; der des anderen
  beginnt neu. Beim Ausschalten des Splits bleibt der Verlauf des Panels, das übrig bleibt.
- **FR-016**: Suche und Filter (Spec 044 US4) MÜSSEN je Panel laufen; Strg+F fokussiert die Suche des
  aktiven Panels. Eine laufende Suche in einem Panel DARF das andere nicht beeinträchtigen.
- **FR-017**: Ansicht (Liste oder Raster) und Sortierung (Spec 044 FR-003) MÜSSEN je Panel gelten
  (Entscheidung 2026-10-28, ersetzt „gemeinsam“). Das Gerät liefert den Anfangswert; wer im Split
  ändert, ändert nur sein Panel, und der Wert steht im Ort des Tabs (`vw`, `so`), kommt also mit der
  Sitzung zurück. Ohne Split ändert die Wahl wie bisher die Geräteeinstellung. Versteckte Dateien
  bleiben je Gerät gemeinsam. Zurück, Vor, Ansicht und Sortierung stehen im Kopf jedes Panels, nicht
  mehr in der Werkzeugleiste; oben bleiben Seitenleiste und Split-Schalter (rechts). Mit einem Panel
  steht die Leiste des Panels in der Werkzeugleiste.
- **FR-018**: Beide Panels MÜSSEN Änderungen auf dem Gerät (Spec 044 FR-006) ohne Neuladen zeigen,
  auch wenn beide denselben Ordner zeigen. Nach einem Transfer MÜSSEN beide den neuen Stand zeigen.
- **FR-019**: Der Viewer MUSS wie bisher über der gesamten Ansicht öffnen; „voriges/nächstes“ MUSS im
  Ordner des Panels springen, aus dem die Datei geöffnet wurde. Nach dem Schließen MUSS der Split
  unverändert stehen.
- **FR-020**: Ein Fehler in einem Panel (Quelle nicht erreichbar, Ordner verschwunden, keine
  Berechtigung) MUSS nur dieses Panel betreffen, das andere bleibt bedienbar.

**Plattformen und Platz**

- **FR-021**: Auf Android und iOS DARF der Dateibrowser keinen Split-Schalter anbieten, und ein
  vorhandener Split-Zustand aus einer Sitzung MUSS dort ignoriert werden (ohne ihn zu löschen).
- **FR-022**: Unterhalb einer Mindestbreite, bei der zwei Panels nicht sinnvoll lesbar sind, MUSS der
  Split-Schalter deaktiviert sein (mit Grund) und eine laufende Split-Ansicht nur das aktive Panel
  zeigen; der Zustand und das ausgeblendete Panel MÜSSEN erhalten bleiben und beim Verbreitern
  zurückkommen.
- **FR-023**: Der Split-Zustand DARF Agents (Spec 044 FR-032a, Spec 032, Spec 021) nicht anders
  betreffen: Ein Agent, der den Dateibrowser öffnet, öffnet einen Tab wie bisher; ein Tab im Split
  zeigt nur im aktiven Panel die geöffnete Datei oder den Ordner, den der Agent nennt.
- **FR-024**: Die Split-Ansicht MUSS ohne Maus bedienbar sein: Schalter, Panelwechsel, Auswahl, F5 und
  F6 gehen mit der Tastatur; die Panels MÜSSEN der Tastatur beim Fokussieren keine Falle sein (man
  erreicht Werkzeugleiste und Seitenleiste weiterhin).

### Key Entities

- **Split-Zustand eines Tabs**: ob der Split an ist; Ort des linken und rechten Panels (Quelle, Ordner,
  Suche, Filter; dazu die geöffnete Datei des aktiven Panels, da nur dieses navigiert und Dateien
  öffnet); welches Panel aktiv ist; der gemerkte Ort des anderen Panels bei
  ausgeschaltetem Split. Gehört zum Tab (Spec 022).
- **Panel**: Ort, Verlauf, Auswahl, Fokus; entspricht einem Tab des Dateibrowsers ohne Split.
- **Transfer**: wie Spec 044; Quelle und Ziel sind die Orte der beiden Panels.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer kann aus dem einfachen Dateibrowser den Split einschalten und eine Auswahl
  von Dateien in einen Ordner im zweiten Panel kopieren, in unter 15 Sekunden, ohne ein Fenster zu
  öffnen oder einen Tab zu wechseln.
- **SC-002**: Mit der Tastatur allein (Tab, Pfeile, F5, F6) gelingt dieselbe Aufgabe ohne Maus.
- **SC-003**: In 100 % der geprüften Fälle (Transfer abgebrochen, Netzfehler, schreibgeschütztes Ziel,
  Vault gesperrt) bleibt am Ziel keine halbe Datei, und der Ursprung eines gescheiterten
  Verschiebens bleibt vollständig.
- **SC-004**: Eine Handlung in einem Panel (Navigation, Auswahl, Suche, Sortierung ausgenommen, da
  geteilt) verändert in 100 % der geprüften Fälle das andere Panel nicht.
- **SC-005**: Nach Aus- und Einschalten und nach einem Neustart mit Sitzung stehen beide Panels in
  100 % der geprüften Fälle wieder an ihren Orten.
- **SC-006**: Ein Ordner mit 50 000 Einträgen in einem Panel macht das andere nicht langsamer
  bedienbar: Auswahl und Navigation im anderen Panel reagieren weiter in unter 200 ms.
- **SC-007**: Auf allen Desktop-Plattformen (Linux, Windows, macOS) läuft die Split-Ansicht: die
  reinen Prüfungen und die Typprüfung laufen in der CI auf allen dreien, die E2E-Prüfung auf Linux;
  auf Android und iOS erscheint der Schalter nicht (geprüft im Android-Lauf).

## Assumptions

- **Panelbreite**: Beide Panels sind gleich breit, eine verschiebbare Trennlinie, Untereinander-Anordnung
  oder mehr als zwei Panels sind nicht Teil dieser Spec.
- **Ein Split je Tab**: Der Split gehört zum Tab, nicht zum Gerät; ein neuer Tab beginnt ohne Split.
- **Geteilte Ansichtswahl**: Ansicht, Sortierung und versteckte Dateien gelten für beide Panels
  gemeinsam, weil Spec 044 sie je Gerät speichert; getrennte Wahl je Panel kann später folgen.
- **Keine Bestätigung vor F5/F6**: Anders als bei Total Commander fragt holzi nicht nach dem Ziel,
  weil es sichtbar ist und der Transfer abbrechbar bleibt; Namenskonflikte und Löschen fragen wie in
  Spec 044. Verschieben ist keine Zerstörung, bei Fehlern bleibt der Ursprung vollständig.
- **Mindestbreite**: Der Plan legt den Wert fest; er liegt so, dass zwei Panels mit Pfadleiste und der
  Liste noch lesbar sind.
- **Panelwechsel mit Tab**: Tab wechselt das Panel nur bei Fokus in einer Liste; Shift+Tab geht
  zurück zum vorigen Element. Wie der Rest der Oberfläche erreichbar bleibt, legt der Plan fest.
- **Nicht-Ordneransichten**: Ansichten wie „Meine Dateifreigaben“ (Spec 056) zeigen sich wie bisher
  und ersetzen dabei vorübergehend den Split; der Split-Zustand bleibt erhalten.
- **Verlauf**: Der Verlauf des Tabs (Spec 020) gehört dem aktiven Panel; das andere hält seinen
  Verlauf, solange der Tab offen ist, und bekommt ihn beim Wechsel zurück. Nach einem Neustart
  mit Sitzung kommt nur der Verlauf des aktiven Panels zurück (die Sitzung speichert einen Verlauf
  je Tab); wird der Tab in ein anderes Fenster verschoben, gilt dasselbe. Das braucht einen kleinen
  Anschluss am Window Manager, mit dem ein Tab seinen Verlauf austauschen kann; sonst ändert sich
  dort nichts.
- **Nicht im Umfang**: weitere Total-Commander-Kürzel (F7 Ordner anlegen, F8 Löschen), Abgleich der
  Panels („Gegenpanel gleichsetzen“), Vergleich von Ordnern, Synchronisieren, Ordner-Baum je Panel,
  Registerkarten innerhalb eines Panels.
- **Abhängigkeiten**: Spec 044 ist gebaut; 056 (Quellen anderer Geräte und Freigaben) ebenfalls.
