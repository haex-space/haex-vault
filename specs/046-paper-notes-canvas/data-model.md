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

## `LatexElement` und Formelzellen (2026-10-26)

Das Element `latex` gibt es im Dokumentmodell seit der ersten Fassung (`app/types/document.ts`): `source` ist die
LaTeX-Quelle (bleibt editierbar; der Editor zeigt sie in der einfachen Schreibweise, wo das geht, FR-031), `svg` das gesetzte Ergebnis in `currentColor`, `color` die Tinte, dazu `x`, `y`,
`width`, `height` und `rotation` wie beim Bild. Neu ist, dass es erzeugt, gezeichnet und bearbeitet wird:

- Eine Region `formula` wird zu einem `latex`-Element auf der Ebene „Text“. Das gesetzte SVG steht in der
  Schriftgröße, die ein einzeiliger Textblock der Höhe der Box hätte (`fontSizeFor`), und wird verkleinert, wenn es
  dann breiter ist als die Box; es wird nie vergrößert. Es steht in der Box senkrecht mittig.
- Lässt sich die Quelle nicht setzen, entsteht stattdessen ein `text`-Element mit der Quelle (FR-030).
- Eine Tabellenzelle, deren ganzer Inhalt `$…$` ist, wird im Zellraster als gesetzte Formel gezeichnet; das SVG wird
  dabei nicht gespeichert, sondern beim Zeichnen aus der Zelle gesetzt (Zwischenspeicher je Quelle). Der Zelltext
  bleibt die Quelle und wird beim Bearbeiten als solche gezeigt.
- Das SVG nutzt `currentColor`; beim Zeichnen wird es mit der Tintenfarbe des Elements gefärbt. Die Seite speichert
  das gesetzte SVG im Element, damit die Anzeige ohne MathJax auskommt, bis jemand die Quelle ändert.
- `collectAssetIds` ändert sich nicht: Formeln sind keine Assets.

## `PaperNoteImport`

Optionales Feld der Seite (neue JSON-Spalte `paper_import`, Migration `0005`). Es fehlt bei gewöhnlichen Seiten.

| Feld                           | Typ                     | Regeln                                                                                                                          |
| ------------------------------ | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `version`                      | integer                 | Beginnt bei `1`; eine unbekannte Version wird nicht still verworfen.                                                            |
| `originalAssetId`              | string                  | Das unveränderte importierte Foto im Asset-Store.                                                                               |
| `workingAssetId`               | string, optional        | Die entzerrte, gedrehte Arbeitskopie, die gelesen wurde; fehlt, wenn sie wegen `processing_failed` nicht erzeugt werden konnte. |
| `rotation`                     | `0 \| 90 \| 180 \| 270` | Drehung der Arbeitskopie gegenüber dem Original, im Uhrzeigersinn.                                                              |
| `sourceMimeType`               | string                  | Ein vom Import unterstütztes Bildformat.                                                                                        |
| `sourceWidth` / `sourceHeight` | positive integer        | Pixelmaße des Originals.                                                                                                        |
| `processing`                   | `PaperNoteProcessing`   | Stand der Erkennung und Herkunft.                                                                                               |
| `regions`                      | `RecognizedRegion[]`    | Eine Zeile je übernommener Region.                                                                                              |
| `closeUps`                     | `PaperNoteCloseUp[]`    | Optional; ein Eintrag je erfolgreich gelesenem Nahfoto.                                                                         |
| `createdAt` / `updatedAt`      | ISO timestamp           | `updatedAt` ändert sich mit jeder Erkennung.                                                                                    |

`collectAssetIds()` nimmt `originalAssetId`, `workingAssetId`, die Nahfotos und die Ausschnitte der Bildregionen auf,
damit eine geteilte Seite ihre Bilder mitbringt.

## `PaperNoteProcessing`

```text
state: "manual" | "processing" | "ready" | "failed"
model?: string           // Name des Modells, das gelesen hat (provenance, Schritt read, outcome "used")
place?: "this_device" | "network" | "cloud"
error?: "no_capable_model" | "consent_denied" | "unusable" | "timeout" | "unavailable" | "processing_failed"
startedAt?: ISO timestamp
finishedAt?: ISO timestamp
```

Regeln:

- Es gibt kein Feld für Adresse, Anbieter oder Schlüssel; holzi gibt sie der Erweiterung nicht (Spec 055 FR-030).
- `state = "failed"` braucht `error` und behält `originalAssetId`. `processing_failed` heißt: die Arbeitskopie oder
  ein Ausschnitt ließ sich auf dem Gerät nicht erzeugen; dann fehlt `workingAssetId`.
- `state = "ready"` braucht `model` und `place`; die Liste der Regionen darf leer sein (leere Seite).
- Ein abgebrochener Auftrag führt zurück zu `manual`, ohne `error`.

## `RecognizedRegion`

| Feld         | Typ                                         | Regeln                                                           |
| ------------ | ------------------------------------------- | ---------------------------------------------------------------- |
| `id`         | string                                      | Eindeutig innerhalb des Imports.                                 |
| `kind`       | `"text" \| "table" \| "image" \| "formula"` | Aus dem Ergebnis von `read`.                                     |
| `sourceBox`  | `[x1, y1, x2, y2]`, je 0–1000               | Relativ zur gelesenen Aufnahme; `x2 > x1` und `y2 > y1`.         |
| `elementIds` | string[]                                    | Die erzeugten Elemente; leer, wenn der Schüler sie gelöscht hat. |
| `closeUpId`  | string, optional                            | Gesetzt, wenn die Region aus einem Nahfoto stammt.               |

Eine Region gilt als ersetzt, sobald ein Nahfoto ihre Elemente ersetzt hat; ihr Eintrag fällt dann weg.

Es gibt keine Vorschläge, Konfidenzwerte oder Prüfzustände (kein Prüfteil). Der Text vor der Korrektur wird nicht
aufgehoben.

## `PaperNoteCloseUp`

| Feld        | Typ                                     | Regeln                                                              |
| ----------- | --------------------------------------- | ------------------------------------------------------------------- |
| `id`        | string                                  | Eindeutig innerhalb des Imports.                                    |
| `assetId`   | string                                  | Das unveränderte Nahfoto im Asset-Store.                            |
| `rotation`  | `0 \| 90 \| 180 \| 270`                 | Wie beim Seitenfoto: vom Schüler oder nach Drehhinweis.             |
| `pageBox`   | `[x, y, width, height]`                 | Das Rechteck auf der Seite, in Seitenkoordinaten; Breite, Höhe > 0. |
| `model`     | string                                  | Name des Modells, das gelesen hat.                                  |
| `place`     | `"this_device" \| "network" \| "cloud"` | Ort der Erkennung.                                                  |
| `createdAt` | ISO timestamp                           |                                                                     |

Ein Nahfoto wird erst gespeichert, wenn sein Ergebnis übernommen ist; ein Fehler oder Abbruch hinterlässt nichts.
Das Ersetzen ändert Elemente und `paperImport` in einem Kommando, sodass ein Rückgängig beides zurücknimmt.

## `PageAttachment` (neu, 2026-10-26)

```ts
interface PageAttachment {
  id: string // UUID
  kind: 'original' | 'file' // "original": das Foto der Seite; Entfernen löst die Seite vom Foto (`paperImport` fällt weg)
  assetId: string // Datei im Asset-Speicher, wie `originalAssetId`
  name: string // Dateiname, den der Schüler sieht
  mimeType: string
  size: number // Bytes
  addedAt: string // ISO 8601
}
```

`pages.attachments` (neue JSON-Spalte, Migration `0006`) hält die Liste; `null` heißt keine Anhänge. Höchstens ein
Anhang hat die Art `original`, und zwar genau dann, wenn `paperImport` gesetzt ist; sein `assetId` ist
`paperImport.originalAssetId`. Der Anhang `original` steht unverändert, ungedreht und in voller Auflösung da: die
Drehung steht in `paperImport.rotation`, nicht im Anhang. `collectAssetIds` nimmt die Assets der Anhänge mit, damit
Teilen und Papierkorb sie mitnehmen.

## Ebenen einer importierten Seite

```text
Papierhintergrund (Vorlage, Farbe, Seitenrand)
Text (TextElements)
Objekte (TableElements und ImageElement-Ausschnitte)
Eigene Ergänzungen (aktive Ebene für neue Striche)
```

Es gibt keine Originalebene mehr: Das Original ist der Anhang `original`, und die Ansicht Original/Entwurf/Vergleich
entfällt. Eine Seite ohne Entwurf (`failed`, `manual`) bekommt zusätzlich die Ebene „Foto“ mit dem Foto als
gewöhnlichem `ImageElement` (entzerrt, aufrecht, nicht gesperrt), damit etwas zu bearbeiten ist; der Schüler kann sie
löschen, das Original bleibt als Anhang.

## `PageBackground.margins` (neu, 2026-10-26)

```ts
interface PageMargins {
  top: number
  bottom: number
  left: number
  right: number
} // Seitenpixel
```

`PageBackground.margins` ist optional; fehlt es, gelten die Werte der Vorlage (bisher fest: oben 60, unten 40, links 50,
rechts 40). Die Vorlagen zeichnen ihre Linien innerhalb der Ränder; neue Inhalte lassen sich nur dort ablegen. Ein
Ändern verschiebt keine vorhandenen Elemente. Die Spalte `background` hält den Wert; es braucht keine Migration.

## Zustandsübergänge

```text
manual ──Lesen starten───────────> processing
processing ──Ergebnis────────────> ready
processing ──Drehhinweis─────────> processing (einmal, mit gedrehter Arbeitskopie)
processing ──Fehler / Ablehnung──> failed
processing ──Abbruch─────────────> manual
ready / failed / manual ──Nahfoto einer Stelle──> unverändert; nur die Elemente im Rechteck und die Regionen ändern sich
```

Jeder Übergang behält `originalAssetId`. Ein Nahfoto ändert `processing` nicht; seine Herkunft steht im
`PaperNoteCloseUp`. Auf einer Seite ohne Entwurf (`failed`, `manual`) liest ein Rechteck über die ganze Seite das
Blatt neu.

## Migration and compatibility

- Seiten ohne `paperImport` und Tabellen ohne `cells` laden wie bisher.
- Die Migration `0005` fügt nur die Spalte `paper_import` hinzu; `0006` fügt `attachments` hinzu.
- Eine Seite aus einem Foto mit `processing.state = ready` und einer gesperrten Originalebene (vor `0006`) wird beim Laden umgestellt: Der Anhang `original` entsteht aus `paperImport` (Name und Größe sind dann unbekannt), und die Originalebene entfällt. Bei `failed` und `manual` wird die Originalebene zur Ebene „Foto".
- Eine unbekannte `paperImport.version` bleibt als Metadaten erhalten und wird als nicht unterstützt angezeigt.
- Ein fehlendes Bild-Asset zeigt einen Platzhalter und behält die Element-Daten.
