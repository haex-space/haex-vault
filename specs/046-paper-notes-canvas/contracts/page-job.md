# Seitenerkennung: Auftrag der Erweiterung

haex-notes gibt die Erkennung einer Seite als Modellauftrag an holzi (Spec 055, vault-sdk ab 4.7.1,
`sdk.models.runAsync`). Brücke, Freigaben und Einwilligung stehen in
[055 bridge.md](../../055-extension-model-access/contracts/bridge.md), Auftrags- und Schemaregeln in
[055 job-format.md](../../055-extension-model-access/contracts/job-format.md). Dieser Vertrag legt fest, was
haex-notes sendet und wie es das Ergebnis abbildet. Anweisungen und Formate sind dieselben wie in der
Test-Erweiterung von Spec 055 (`src-tauri/tests/fixtures/extension_e2e/model-jobs/`, dort mit Testmodi).

## Auftrag

```json
{
  "steps": [
    {
      "id": "read",
      "needs": ["text", "image"],
      "instruction": "<READ>",
      "input": {
        "images": [
          {
            "mimeType": "image/jpeg",
            "data": "<Base64 der Arbeitskopie>"
          }
        ]
      },
      "format": "<page-recognition v2>",
      "maxOutputTokens": 8192
    },
    {
      "id": "fix",
      "needs": ["text"],
      "instruction": "<FIX>",
      "input": {
        "from": {
          "step": "read",
          "pointer": "/regions"
        }
      },
      "format": "<text-fix v1>"
    }
  ]
}
```

Die Arbeitskopie ist das entzerrte, bei Bedarf gedrehte JPEG (höchstens 25 MiB, längste Kante 2048 px). Ein
Nahfoto einer Stelle (research R13) läuft unverändert durch denselben Auftrag; seine Boxen beziehen sich auf das
Nahfoto und werden in das gewählte Rechteck der Seite eingepasst.

### Anweisung `READ`

```text
This is a photo of a student's handwritten notes, a blackboard picture or a worksheet, in German.
Split the page into regions and return ONLY a JSON object:
{"regions": [{"kind": "text" | "table" | "image" | "formula", "box": [x1, y1, x2, y2], "text": "...", "latex": "...", "rows": [["cell", ...], ...]}], "rotation": 0}
Rules:
- box uses coordinates 0-1000 relative to the image width and height.
- "text": transcribe the handwriting exactly as written, keep line breaks, do not correct anything.
- "table": give "rows" as a list of rows of cell strings, an empty cell as "", no "text".
- "formula": a line that is a formula of mathematics, chemistry or physics and nothing else. Give "latex" as valid
  LaTeX, no "text". Write chemistry with \ce{...}, for example \ce{H2SO3 + H2O -> H3O+ + HSO3-}, and keep charges
  and indices. Do not use "formula" for a formula inside a sentence.
- Inside a sentence write a formula in plain text with Unicode indices and charges (H₃O⁺, SO₄²⁻).
- In a table cell that contains only a formula, write it as $...$ with the same LaTeX rules.
- "image": drawings, diagrams, sketches and photos, no "text". Structural formulas (Lewis structures) are images.
- List regions in reading order. Do not invent content that is not on the page.
- "rotation": the clockwise degrees (0, 90, 180 or 270) the photo must be turned to stand upright.
```

### Anweisung `FIX`

```text
These are the regions of a German page read from handwriting. Correct only spelling and words
that were clearly misread in their "text"; keep abbreviations, line breaks and the wording otherwise.
Return ONLY {"texts": [...]} with one corrected text per region that has a "text", in the same order.
```

### Format `page-recognition` v2

```json
{
  "name": "page-recognition",
  "version": 1,
  "schema": {
    "type": "object",
    "required": ["regions"],
    "properties": {
      "regions": {
        "type": "array",
        "maxItems": 512,
        "items": {
          "type": "object",
          "required": ["kind", "box"],
          "properties": {
            "kind": {
              "enum": ["text", "image", "table", "formula"]
            },
            "box": {
              "type": "array",
              "items": {
                "type": "number",
                "minimum": 0,
                "maximum": 1000
              },
              "minItems": 4,
              "maxItems": 4
            },
            "text": {
              "type": "string",
              "maxLength": 20000
            },
            "latex": {
              "type": "string",
              "maxLength": 4000
            },
            "rows": {
              "type": "array",
              "items": {
                "type": "array",
                "items": {
                  "type": "string"
                }
              }
            }
          }
        }
      },
      "rotation": {
        "enum": [0, 90, 180, 270],
        "description": "Clockwise degrees the photo must be turned to stand upright"
      }
    }
  }
}
```

### Format `text-fix` v1

```json
{
  "name": "text-fix",
  "version": 1,
  "schema": {
    "type": "object",
    "required": ["texts"],
    "properties": {
      "texts": {
        "type": "array",
        "items": {
          "type": "string"
        }
      }
    }
  }
}
```

Ändert sich eine Anweisung so, dass ältere Ergebnisse anders zu lesen wären, steigt die `version` des Formats. `page-recognition` v2 (2026-10-26) fügt die Art `formula` und das Feld `latex` hinzu; die Anweisung `FIX` ändert sich nicht, denn sie liest nur Regionen mit `text`.

## Abbildung des Ergebnisses

| Teil von `read`                                                       | Wird zu                                                                                                         |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `kind: "text"` mit `text`                                             | `TextElement` auf der Textebene; Text aus `fix` (research R12), Font des gewählten Handschriftstils             |
| `kind: "table"` mit `rows`                                            | `TableElement` mit `cells` auf der Objektebene (research R9)                                                    |
| `kind: "image"`                                                       | `ImageElement` mit dem Ausschnitt der Quellbox aus der Arbeitskopie (Asset)                                     |
| `kind: "formula"` mit `latex`                                         | `LatexElement` auf der Textebene (data-model.md); lässt sich die Quelle nicht setzen, ein `TextElement` mit ihr |
| Zelle `$…$` einer Tabelle                                             | bleibt `$…$` in `cells` und wird beim Zeichnen als Formel gesetzt                                               |
| `box`                                                                 | Seitenkoordinaten nach research R11; eine Box mit `x2 <= x1` oder `y2 <= y1` wird übergangen                    |
| `rotation` ungleich 0                                                 | Arbeitskopie drehen, Auftrag einmal neu starten (research R10)                                                  |
| Region ohne Inhalt (leerer `text`, leeres `latex`, `rows` ohne Zelle) | wird übergangen                                                                                                 |

Die `provenance` des Auftrags ergibt die Herkunft der Seite: Name und Ort des Modells aus dem Versuch mit
`outcome: "used"` im Schritt `read`.

## Fehler

| Ende                                        | haex-notes                                                                      |
| ------------------------------------------- | ------------------------------------------------------------------------------- |
| Start abgelehnt `8001` (`no_capable_model`) | Hinweis: in holzi ein Modell freigeben, das Bilder annimmt; Seite mit Original  |
| Start abgelehnt `1002` (`consent_denied`)   | Hinweis: Übertragung abgelehnt; Seite mit Original                              |
| `failed` mit `unusable`                     | Hinweis: nichts Brauchbares erkannt, neues Foto empfohlen; Seite mit Original   |
| `failed` mit `timeout` oder `unavailable`   | Hinweis: Modell nicht erreichbar oder zu langsam; „Erneut lesen“ bleibt möglich |
| `cancelled`                                 | keine Meldung; Seite mit Original                                               |

In keinem Fall wird das Original verändert, und der Schüler kann manuell weiterarbeiten (FR-016).
