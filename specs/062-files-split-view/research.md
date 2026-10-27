# Research: Dateibrowser mit zwei Panels

Alle Entscheidungen beruhen auf dem Stand von `main` (2026-10-26). Es gibt keine offenen
`NEEDS CLARIFICATION`. Die Analyse vom 2026-10-26 (C1, I1, D1–D13) ist eingearbeitet.

## R1 – Wo liegt der Split-Zustand?

- **Decision**: Im Ort des Tabs als zusätzliche Query-Schlüssel. Die einfachen Schlüssel (Pfad der
  Route für die Quelle, `p`, `open`, `q`, `t`, `s`, `d`) gehören immer dem **aktiven** Panel. Dazu
  kommen `split=1` (an), `side=r` (das aktive Panel steht rechts, sonst links) und der Ort des
  **anderen** Panels unter `o.src`, `o.p`, `o.q`, `o.t`, `o.s`, `o.d`. Ist der Split aus, bleiben
  `o.*` stehen: das ist das „Merken“ (FR-005, FR-006). Siehe
  [contracts/split-location.md](contracts/split-location.md).
- **Rationale**: Ein `TabLocation` ist ein Pfad plus Strings, wird vom Verlauf (Spec 020) und von
  der Sitzung (Spec 022) ohnehin gespeichert. Dass die einfachen Schlüssel dem aktiven Panel
  gehören, macht jeden Eintrag eines Verlaufs zu einer gewöhnlichen Dateibrowser-Ansicht; die
  Titel, die Verlaufsliste und `parseFilesPlace` bleiben unverändert, und ein Ort ohne neue
  Schlüssel ist ein Tab ohne Split (Rückwärtskompatibilität von Sitzungen).
- **Alternatives**: (a) Eigener Pinia-Store je Tab: bräuchte eigene Sitzungs-Speicherung und
  Aufräumen. (b) Geräteeinstellung: widerspricht „Split gehört zum Tab“ (FR-007). (c) Zweite Route
  `…/split`: der Rahmen würde beim Umschalten neu gemountet (Spec 044 R13 verlangt, dass
  Ordnerwechsel nicht neu mounten).

## R2 – Verlauf je Panel

- **Decision**: Der Verlauf des Tabs im Window Manager (`TabHistory`, Spec 020) ist immer der des
  **aktiven** Panels. Der Rahmen hält den Verlauf des anderen Panels als „Spur“ (`lanes`) im
  Arbeitsspeicher. Beim Wechsel des aktiven Panels tauscht er beide: die Spur des alten Panels
  wird gesichert, der Verlauf des Tabs wird durch die Spur des neuen ersetzt (oder durch einen
  frischen Verlauf mit dem aktuellen Ort, wenn es noch keine gibt). Dafür bekommt der Window Manager
  **einen** neuen Anschluss: `setHistory(tabId, history)` im Store (mit `saveSoon` für die Sitzung),
  reine Hilfsfunktion in `lib/wm/tabNavigation.ts`, und `swapHistory(next)` am Tab-Router, der den
  alten Verlauf zurückgibt.
- **Rationale**: Alt+←/→, die Schaltflächen samt Verlaufsliste, „Zurück“ des Systems und
  `skipCurrent` arbeiten auf dem Verlauf des Tabs und brauchen keine Änderung; sie wirken
  automatisch auf das aktive Panel (FR-015). Die Sitzung speichert den Verlauf des aktiven Panels wie
  heute. Einen Anschluss, über den eine App Alt+←/→ und die Schaltflächen übernimmt, bräuchten wir
  sonst in `NavButtons`, `WmHistoryMenu`, den Aktionen `wm.tab.*` und dem Systemzurück.
- **Grenzen** (in den Annahmen der Spec festgehalten): Die Spur des anderen Panels überlebt keinen
  Neustart und kein Verschieben des Tabs in ein anderes Fenster (der Rahmen wird neu erzeugt); der
  Verlauf des aktiven Panels überlebt beides. Beim Ausschalten des Splits fällt die andere Spur weg.
- **Alternatives**: (a) Gemeinsamer Verlauf, in dem jeder Schritt beider Panels steht: einfacher,
  aber Zurück nähme beim aktiven Panel unter Umständen einen Schritt des anderen zurück.
  Vom Betreiber verworfen (2026-10-26). (b) Eigene Stapel im Rahmen und Anschluss für
  Alt+←/→: größer und betrifft jede App.

## R3 – Rahmen und Panel trennen

- **Decision**: `FilesApp.vue` (492 Zeilen) wird Rahmen; fast alles darunter wandert in `Panel.vue`:
  `useFilesFolder`, `useFilesSearch`, `useFilesManage`, `useFilesKeyboard`, `useFilesTabShares`,
  `useFilesAllAccess`, Pfadleiste, Suchleiste, Liste, Drop-Ziel, Aktionsleiste, die Dialoge (Namen,
  Löschen, Freigabe, Hinweise) und der Viewer. Der Rahmen behält `sourcesAsync` (eine Abfrage für
  beide Panels und die Seitenleiste), Werkzeugleiste, Seitenleiste, Transferleiste und Tab-Aufräumen
  (`releaseTabAsync`, Vorschaubilder, Symbole). Das Panel liest seinen Ort **nicht** mehr aus
  `useTabRouter`, sondern bekommt ihn und die Schreibfunktionen (`go`, `setQuery`) über
  [contracts/panel-context.md](contracts/panel-context.md).
- **Rationale**: Auswahl, Tastatur, Dialoge und Ordnerliste sind schon je Ort geschnitten; jede
  Instanz ist damit unabhängig (FR-003, SC-004) und die Zuordnung der Tastatur ergibt sich aus dem
  Fokus: ein `keydown` im Panel trifft den Handler dieses Panels. Der Tab-Router bleibt der einzige
  Schreibpfad; nur der Rahmen darf `router.push/replace/setQuery` aufrufen, damit die Schlüssel des
  anderen Panels beim Schreiben nicht verloren gehen.
- **Alternatives**: Zwei vollständige `FilesApp` in einem Tab: bräuchte zwei Tab-Orte; die
  Sitzung und der Verlauf kennen aber nur einen.

## R4 – Aktives Panel und Wechsel

- **Decision**: Aktiv ist das Panel, in dem zuletzt ein Klick (`pointerdown`) oder ein Fokus
  (`focusin`) war. Navigation und Schreiben in den Ort geschehen **nur im aktiven Panel**; ein `go`
  eines inaktiven Panels macht es zuerst aktiv (Verlaufstausch, R2) und navigiert dann. Beim Wechsel
  schreibt der Rahmen den Ort neu: einfache Schlüssel = neues aktives Panel, `o.*` = bisheriges,
  `side` kehrt sich um, sodass die Panels an derselben Stelle der Oberfläche bleiben. Seitenleiste,
  `Strg+F`, Hinauf und der Split-Schalter wirken auf das aktive Panel. Erkennbar am Rahmen (2 px,
  Primärfarbe) **und** an der Pfadleiste (Textfarbe statt gedämpft), also nicht nur durch Farbe
  (FR-004).
- **Fokus nach dem Öffnen** (Analyse D1): `FolderView.takeFocus` gibt der Liste den Fokus, wenn ein
  Ordner öffnet und der Fokus verloren war. Das darf nur das **aktive** Panel tun (Eigenschaft
  `takesFocus`), sonst aktiviert das zweite Panel beim Einblenden oder beim Start über `focusin`
  sich selbst. Nur `switchPanel` und ein Klick setzen den Fokus ins andere Panel.
- **Rationale**: Kein globaler Tastatur-Fänger; der DOM-Fokus entscheidet. Da nur das aktive Panel
  navigiert, ist der Ort des anderen Panels zwischen den Wechseln konstant.
- **Alternatives**: Aktives Panel nur im Speicher des Rahmens: ginge bei der Sitzung verloren.

## R5 – Tab, F5, F6 und der Fokus

- **Decision**: `resolveFilesShortcut` (rein) bekommt die Eingabe `split` (Boolean). Mit `split` gilt:
  `Tab` (ohne Umschalt, Fokus in der Liste) → Befehl `switchPanel`; `F5` → `copyToOther`; `F6` →
  `moveToOther`; `Strg+R` bleibt `reload`, `F5` ist nicht mehr `reload`. Ohne `split` bleibt alles wie
  in Spec 044. `Umschalt+Tab` bleibt der Browser-Standard (geht rückwärts aus der Liste heraus). Beim
  Wechsel fokussiert der Rahmen den zuletzt fokussierten Eintrag des Zielpanels, sonst den ersten.
- **Rationale**: Erfüllt „wie Total Commander“ und FR-024: Werkzeugleiste, Suche und Seitenleiste
  liegen vor den Listen und sind mit Umschalt+Tab erreichbar; keine Falle (WCAG 2.1.2).
  Elemente hinter den Listen (Aktions- und Transferleiste) sind per Tastatur nur über das
  Umlaufen des Dokuments erreichbar; bewusst hingenommen, die Maus bleibt, und Strg+C/V/Entf
  brauchen die Aktionsleiste nicht (Spec 044 FR-017b).
- **Alternatives**: Eigene Taste statt Tab: bricht die Erwartung „wie Total Commander“.

## R6 – Viewer über beiden Panels

- **Decision**: Das Panel behält die Viewer-Logik (`open`, Geschwister aus `shown`), rendert den
  Viewer aber mit `<Teleport defer to="#files-viewer-host">` in ein Element des Rahmens, das die
  gesamte Panelfläche überdeckt (FR-019). Weil nur das aktive Panel navigiert, gibt es höchstens ein
  `open` im Tab (kein `o.open`).
- **Rationale**: Die Freigabe-URLs sind an den Tab gebunden (Spec 044 FR-016, `files_open(tabId)`),
  zwei gleichzeitige Viewer würden sich überschreiben.
- **Alternatives**: Viewer im Panel: nur halbe Breite, Video und PDF zu klein.

## R7 – Ziel mit eigener Quelle

- **Decision**: `useFilesManage` bekommt nur eine kleine Änderung: `startAsync` und `placeAsync` nehmen
  die Zielquelle (bisher immer die eigene) und einen `onDone` entgegen, und `placeAsync` übergibt
  `sameSource` an `pasteRefusal`. Die Funktionen `copyToOther()`/`moveToOther()` stehen in der neuen
  Datei `composables/useFilesOtherPanel.ts` (`useFilesManage.ts` hat 465 Zeilen und soll nicht über 500
  wachsen). Sie lesen das Ziel des Gegenpanels (`PanelHandle.target()`: Quelle, Ordner,
  Schreibschutz) aus dem Kontext. Nach einem Transfer laden beide Panels nach (`onDone` ruft
  `reload` beider Handles; auf dem Gerät reicht zusätzlich die Beobachtung, Speicher werden nicht
  beobachtet).
- **`pasteRefusal` und Quellen** (Analyse C1): `pasteRefusal(op, folder, items, target)` vergleicht nur
  Pfad-Strings. Zwei Quellen haben getrennte Pfadräume; ohne Quellenvergleich gälte F6 von Speicher
  A `/` nach Speicher B `/` als „derselbe Ordner“ (`nothing`) und `/backup` nach B `/backup/2026`
  als „in sich selbst“ (`intoItself`). Die Funktion bekommt `sameSource`; `nothing` und `intoItself`
  gelten nur bei gleicher Quelle, `holziOwned` immer. Das ändert auch das bisherige Einfügen
  zwischen Quellen (ein Fehler, den es dort schon gab).
- **Rationale**: Das Backend (`files_transfer`, „Copies or moves between sources“, `change.rs`) kann
  schon zwischen Quellen kopieren und verschieben; die Oberfläche stellte das Ziel bisher fest auf die
  eigene Quelle. Damit entstehen keine neuen Regeln (FR-010; „eine Umsetzung je Regel“).
- **Alternatives**: Über die Zwischenablage (`copy` + `paste` im Gegenpanel): zwei Schritte, die
  Zwischenablage des Nutzers würde überschrieben.

## R8 – Ziehen zwischen Panels

- **Decision**: Das vorhandene Ziehen (`FolderView` → `onDrop`, `dropOp`) funktioniert zwischen den
  beiden Panels bereits, weil Nutzlast (`DragPayload` mit Quelle) und Ziel-Panel unabhängig sind.
  Zu ändern ist nur FR-011: `FolderView` bekommt `droppable` (false bei schreibgeschütztem Ziel:
  eigene Daten, Freigabe ohne Schreibrecht) und setzt `dropEffect = 'none'`, damit das
  Überfahren den Abwurf verweigert, statt erst beim Loslassen mit einer Meldung abzulehnen.
  Zwischen verschiedenen Quellen kopiert `dropOp` immer (Spec 044; FR-009 danach angepasst).
  Ablegen aus dem System (`DropTarget`) prüft die Fläche je Instanz und funktioniert je Panel.
- **Alternatives**: Eigene Panel-zu-Panel-Ziehlogik: unnötig.

## R9 – Mindestbreite und Mobil

- **Decision**: Mindestbreite der Panelfläche (ohne Seitenleiste): **720 px**, je Panel 360 px. Die
  reine Funktion `fitsSplit(width)` steht in `lib/files/split.ts`, `useSplitFit` misst die
  Panelfläche mit `ResizeObserver`. Mobil (`PlatformName` `android` oder `ios`, aus
  `useDeviceCapabilities`) zeigt keinen Schalter, und `split=1` im Ort wird als „aus“ gelesen,
  ohne den Ort zu verändern (FR-021). Zu schmal: nur das aktive Panel, Zustand bleibt (FR-022).
- **Rationale**: 360 px zeigen noch Pfadleiste, Namen und Größe (die Liste blendet Spalten nach
  Container-Breite aus). Die Seitenleiste ist bei schmaler Breite ohnehin eine Überlagerung und
  wird nicht eingerechnet. Bis die Fähigkeiten geladen sind, gilt „Desktop“ (Spec 043, wie
  `useDeviceCapabilities` es beschreibt); in der Vorschau ohne Tauri bleibt der Split verfügbar.
- **Alternatives**: Panels untereinander bei schmaler Breite: außerhalb des Umfangs (Spec).

## R10 – Beide Panels im selben Ordner

- **Decision**: Zwei `useFilesFolder` laden und beobachten denselben Ordner unabhängig (zwei
  Beobachter, zwei Listen). Das kostet einmal Speicher und ist einfach; eine gemeinsame
  Ordner-Instanz je (Quelle, Pfad) wird nur gebaut, wenn Messung (SC-006) es verlangt.
- **Rationale**: Die Beobachter (`watchAsync`) sind je Aufruf unabhängig und werden schon je Tab
  benutzt (zwei Tabs im selben Ordner kommen vor).

## R11 – Tests

- **Decision**: Reine Teile unter `node --test` (`check-files-split.ts`: Ort lesen/schreiben,
  Umschalten, Tausch beim Ausschalten, Mindestbreite, Mobil; `check-files-shortcuts.ts`: Tab, F5,
  F6, Strg+R mit und ohne Split; `check-files-registry.ts`: Schlüssel gehen beim Bauen eines Orts
  nicht verloren). E2E im vorhandenen Rig: US1 (F5, F6 mit zwei Testordnern), US2 (Ziehen), US3
  (Gerät ↔ lokaler S3-Server), US4 (Sitzung wiederherstellen, Ausschalten/Einschalten), schmales
  Fenster. Android hat keine E2E-Prüfung für den Split (er ist dort ausgeblendet; der Test
  prüft nur, dass kein Schalter erscheint, im bestehenden Android-Lauf `files-basic`).
