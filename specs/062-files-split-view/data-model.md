# Data Model: Dateibrowser mit zwei Panels

Kein neuer Speicher; alles ist Zustand im Tab-Ort (`TabLocation`, Spec 020) oder Laufzeitzustand des
Rahmens.

## SplitState (lesen und schreiben in `lib/files/split.ts`, Zustand im Rahmen)

| Feld     | Typ                  | Bedeutung                                                                                                                                              |
| -------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `on`     | boolean              | Der Tab steht in der Split-Ansicht (Query `split=1`).                                                                                                  |
| `side`   | `'left' \| 'right'`  | Auf welcher Seite das **aktive** Panel steht (`side=r` ↔ `right`, sonst `left`). Bleibt bei ausgeschaltetem Split als Seite des übrigen Panels stehen. |
| `active` | `FilesPlace`         | Ort des aktiven Panels: die einfachen Schlüssel des Tab-Orts (Spec 044 `registry.ts`).                                                                 |
| `other`  | `FilesPlace \| null` | Ort des anderen Panels (`o.*`, ohne `open`); bei ausgeschaltetem Split der gemerkte Ort; `null`, wenn nie ein Split war.                               |
| `lane`   | `TabHistory \| null` | Verlauf des **anderen** Panels (nur im Arbeitsspeicher des Rahmens, nicht im Ort). Der Verlauf des aktiven Panels ist der des Tabs im Window Manager.  |

Regeln:

- Die Quelle der Wahrheit für `on`, `side` und `other` ist der Rahmen; der Ort des Tabs spiegelt
  sie in seinem aktuellen Eintrag und wird nur beim Mounten (Start, Sitzung) gelesen. Ältere
  Verlaufseinträge tragen sie nicht (oder veraltet): der Rahmen schreibt sie nach jedem Sprung im
  Verlauf (Zurück, Vor, Sitzung) mit `replace` zurück. Siehe
  [contracts/split-location.md](contracts/split-location.md).
- `other` fehlt oder ist ungültig (unbekannte Quelle, `o.src` nicht lesbar) → beim Einschalten
  gilt der Ort des aktiven Panels (FR-005). Ist die gemerkte Quelle nicht mehr erreichbar, zeigt das
  Panel den Fehler der Quelle (FR-020), und der Nutzer wählt neu.
- **Navigation nur im aktiven Panel.** Ein Schritt eines inaktiven Panels macht es zuerst aktiv.
- **Wechsel des aktiven Panels**: `lane` des Tabs und `lane` des Rahmens werden getauscht
  (der Verlauf des Tabs wird durch die Spur ersetzt, oder durch einen frischen Verlauf mit dem
  neuen aktiven Ort); die Orte `active` und `other` werden getauscht und `side` kehrt sich um, sodass
  kein Panel auf der Oberfläche die Seite wechselt.
- **Ausschalten**: das aktive Panel bleibt als einzige Ansicht, `other` wird gemerkt, `lane` fällt
  weg. **Einschalten**: `active` bleibt, `other` = gemerkter Ort oder `active` (FR-005), `lane`
  = frischer Verlauf (ein Eintrag).
- Mobil oder zu schmal: `on` wird nicht verändert, nur die Darstellung entscheidet (`shown`).

## Sichtbarer Zustand (Rahmen, nicht gespeichert)

| Feld        | Quelle                          | Bedeutung                                   |
| ----------- | ------------------------------- | ------------------------------------------- |
| `available` | Plattform ist nicht Android/iOS | Schalter sichtbar (FR-021).                 |
| `fits`      | `fitsSplit(width)`              | Panelfläche ≥ 720 px (FR-022).              |
| `shown`     | `on && available && fits`       | Zwei Panels sichtbar; sonst nur das aktive. |

## PanelHandle (Laufzeit, je Panel beim Rahmen angemeldet)

| Feld             | Typ                                                                      | Bedeutung                                                                                   |
| ---------------- | ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| `side`           | `'left' \| 'right'`                                                      | Seite auf der Oberfläche (fest je Panel-Instanz).                                           |
| `target()`       | `{ source: SourceRef; path: string \| null; readOnly: boolean } \| null` | Wohin dieses Panel Transfers aufnimmt (Ordner, Quelle, Schreibschutz aus `useFilesManage`). |
| `reload()`       | `() => unknown`                                                          | Lädt den Ordner nach (nach einem Transfer in dieses Panel).                                 |
| `focus()`        | `() => void`                                                             | Fokussiert den zuletzt fokussierten Eintrag, sonst den ersten.                              |
| `keydown(event)` | `(event: KeyboardEvent) => void`                                         | Tasten der Werkzeugleiste und der Seitenleiste, die der Rahmen an das aktive Panel gibt.    |

## Übernommen unverändert (Spec 044)

`FilesPlace`, `Entry`, `Selection`, `FilesClipboard`, `DragPayload`, `Transfer` und die
Geräteeinstellungen `files.view`, `files.sort`, `files.showHidden`.
