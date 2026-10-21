# Data Model: Paper Notes to Editable Canvas

**Stand**: 2026-10-21 (Erkennung über Spec 055, ohne Prüfteil)

## Existing model to extend

Revision `6dfb015` von haex-notes speichert eine Seite als `PageDoc` mit `PageLayer[]`, `PageElement[]`, einem
`PageBackground` und den Seitenmaßen (`app/types/document.ts`, Spalten `layers` und `background` der
Seitentabelle). `TextElement`, `ImageElement` und `TableElement` sind die Elemente, die eine erkannte Seite
braucht. Die Commands in `app/lib/commands.ts` fügen hinzu, entfernen, ersetzen und ändern rückgängig machbar.

## `TableElement.cells` (neu)

| Feld    | Typ          | Regeln                                                                                                |
| ------- | ------------ | ----------------------------------------------------------------------------------------------------- |
| `cells` | `string[][]` | `rows` Zeilen zu je `columns` Zellen; eine leere Zelle ist `""`. Fehlt das Feld, ist jede Zelle leer. |

Zeilen oder Spalten hinzufügen und entfernen (`app/lib/tableActions.ts`) hält `cells` passend: neue Zellen sind
leer, entfernte Zellen gehen mit. Eine geteilte Seite nimmt die Zelltexte im Seiten-JSON mit.

## `PaperNoteImport`

Optionales Feld der Seite (neue JSON-Spalte `paper_import`, Migration `0005`). Es fehlt bei gewöhnlichen Seiten.

| Feld                           | Typ                     | Regeln                                                               |
| ------------------------------ | ----------------------- | -------------------------------------------------------------------- |
| `version`                      | integer                 | Beginnt bei `1`; eine unbekannte Version wird nicht still verworfen. |
| `originalAssetId`              | string                  | Das unveränderte importierte Foto im Asset-Store.                    |
| `workingAssetId`               | string                  | Die entzerrte, gedrehte Arbeitskopie, die gelesen wurde.             |
| `rotation`                     | `0 \| 90 \| 180 \| 270` | Drehung der Arbeitskopie gegenüber dem Original, im Uhrzeigersinn.   |
| `sourceMimeType`               | string                  | Ein vom Import unterstütztes Bildformat.                             |
| `sourceWidth` / `sourceHeight` | positive integer        | Pixelmaße des Originals.                                             |
| `processing`                   | `PaperNoteProcessing`   | Stand der Erkennung und Herkunft.                                    |
| `regions`                      | `RecognizedRegion[]`    | Eine Zeile je übernommener Region.                                   |
| `createdAt` / `updatedAt`      | ISO timestamp           | `updatedAt` ändert sich mit jeder Erkennung.                         |

`collectAssetIds()` nimmt `originalAssetId`, `workingAssetId` und die Ausschnitte der Bildregionen auf, damit eine
geteilte Seite ihre Bilder mitbringt.

## `PaperNoteProcessing`

```text
state: "manual" | "processing" | "ready" | "failed"
model?: string           // Name des Modells, das gelesen hat (provenance, Schritt read, outcome "used")
place?: "this_device" | "network" | "cloud"
error?: "no_capable_model" | "consent_denied" | "unusable" | "timeout" | "unavailable"
startedAt?: ISO timestamp
finishedAt?: ISO timestamp
```

Regeln:

- Es gibt kein Feld für Adresse, Anbieter oder Schlüssel; holzi gibt sie der Erweiterung nicht (Spec 055 FR-030).
- `state = "failed"` braucht `error` und behält `originalAssetId`.
- `state = "ready"` braucht `model` und `place`; die Liste der Regionen darf leer sein (leere Seite).
- Ein abgebrochener Auftrag führt zurück zu `manual`, ohne `error`.

## `RecognizedRegion`

| Feld         | Typ                            | Regeln                                                           |
| ------------ | ------------------------------ | ---------------------------------------------------------------- |
| `id`         | string                         | Eindeutig innerhalb des Imports.                                 |
| `kind`       | `"text" \| "table" \| "image"` | Aus dem Ergebnis von `read`.                                     |
| `sourceBox`  | `[x1, y1, x2, y2]`, je 0–1000  | Relativ zur Arbeitskopie; `x2 > x1` und `y2 > y1`.               |
| `elementIds` | string[]                       | Die erzeugten Elemente; leer, wenn der Schüler sie gelöscht hat. |

Es gibt keine Vorschläge, Konfidenzwerte oder Prüfzustände (kein Prüfteil). Der Text vor der Korrektur wird nicht
aufgehoben.

## Ebenen einer importierten Seite

```text
Papierhintergrund
Original (sichtbar, gesperrt; ImageElement mit rotation, damit es aufrecht steht)
Text (TextElements)
Objekte (TableElements und ImageElement-Ausschnitte)
Eigene Ergänzungen (aktive Ebene für neue Striche)
```

Die Originalebene ist nie die aktive Ebene; Änderungen aus dem Editor erreichen sie nicht.

## Zustandsübergänge

```text
manual ──Lesen starten───────────> processing
processing ──Ergebnis────────────> ready
processing ──Drehhinweis─────────> processing (einmal, mit gedrehter Arbeitskopie)
processing ──Fehler / Ablehnung──> failed
processing ──Abbruch─────────────> manual
failed ──Erneut lesen────────────> processing
ready ──Erneut lesen─────────────> processing (ersetzt nur die erzeugten, unveränderten Elemente)
```

Jeder Übergang behält `originalAssetId`. Erneutes Lesen nach Änderungen des Schülers fragt vorher, ob die
erzeugten Elemente ersetzt werden sollen.

## Migration and compatibility

- Seiten ohne `paperImport` und Tabellen ohne `cells` laden wie bisher.
- Die Migration `0005` fügt nur die Spalte `paper_import` hinzu.
- Eine unbekannte `paperImport.version` bleibt als Metadaten erhalten und wird als nicht unterstützt angezeigt.
- Ein fehlendes Bild-Asset zeigt einen Platzhalter und behält die Element-Daten.
