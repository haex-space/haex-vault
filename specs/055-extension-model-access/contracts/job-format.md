# Contract: Auftrag, Ergebnisformat, Aufwand, Prüfung

Was die Erweiterung in `extension_model_job_start` schickt und was holzi prüft. Die Brückenmethoden stehen in
`bridge.md`.

## Auftrag

```text
Job {
  steps: Step[]            // 1 bis 4
}

Step {
  id: string               // [a-z][a-z0-9_-]{0,31}, eindeutig im Auftrag
  needs: ("text" | "image")[]    // mindestens "text"; "image" verlangt ein Modell mit Bildeingabe
  instruction: string      // Anweisung der Erweiterung, höchstens 16 KiB, nicht leer
  input: {
    text?: string          // eigener Text
    images?: Image[]       // höchstens 8 je Auftrag insgesamt
    from?: { step: string, pointer: string }   // früherer Schritt, JSON-Pointer (RFC 6901) auf einen Teil
  }
  format: {
    name: string           // z. B. "page-recognition"
    version: number        // ganze Zahl >= 1
    schema: object         // Schema-Teilmenge, siehe unten
  }
  effort?: "none" | "low" | "medium" | "high"
  maxOutputTokens?: number // bis zur Obergrenze (Standard 4096, höchstens 16384)
}

Image { mimeType: "image/jpeg" | "image/png" | "image/webp", data: string /* Base64, höchstens 25 MiB roh */ }
```

- `from` darf nur auf einen **früheren** Schritt zeigen; der Teil unter `pointer` wird dem Modell als Text
  (JSON) angehängt. Beim Start prüft holzi, dass `step` früher im Auftrag steht und dass `pointer` nach dem
  Schema des referenzierten früheren Schritts vorkommen kann; sonst `InvalidInput`. Fehlt der Teil zur Laufzeit im tatsächlichen
  Ergebnis, endet der Schritt mit `Unusable` (Grund „from fehlt“) und der Ausweichweg greift nicht, weil das
  Ergebnis des früheren Schritts feststeht.
- `images` dürfen nur in einem Schritt stehen, dessen `needs` `"image"` enthält. `from` darf nur in einem
  Schritt stehen, dessen `needs` `"text"` enthält, weil frühere Ergebnisse als Text/JSON übergeben werden.
- Der Text für das Modell besteht aus `instruction` (Systemanweisung), dem Eingabetext und dem Schema als
  Hinweis auf das erwartete JSON. holzi fügt einen festen Satz hinzu, der verlangt, nur gültiges JSON nach dem
  Schema auszugeben. Mehr stellt holzi nicht voran; die Anweisung bleibt die der Erweiterung.

## Schema-Teilmenge

Gemeinsames Modul `S/schema_subset.rs` (aus Spec 018 gehoben).

- Typen: `object`, `array`, `string`, `integer`, `number`, `boolean`, `null` nur als Union mit einem anderen Typ.
- Schlüsselwörter: `type`, `properties`, `required`, `items`, `enum`, `minimum`, `maximum`, `minLength`,
  `maxLength`, `minItems`, `maxItems`, `additionalProperties` (boolesch), `description`.
- Nicht erlaubt: `$ref`, `pattern`, `oneOf`/`anyOf`/`allOf` (außer der Union mit `null`), Verweise, Rekursion.
- Grenzen: Tiefe 8, höchstens 200 Eigenschaften insgesamt, Schema höchstens 32 KiB.
- Ergebnisse höchstens 256 KiB; Zahlen als IEEE-754 ohne Sonderwerte.

## Aufwand

- Die Erweiterung gibt höchstens einen Wunsch an. holzi bildet `min(Wunsch, höchster freigegebener Aufwand)`;
  fehlt der Wunsch, gilt `none`.
- Zuordnung auf das Modell nach seiner Auskunft (`ModelCapabilities.reasoning`):
  - `Unavailable`: Aufwand wirkt nicht; Herkunft: `none`.
  - `ModelManaged`: das Modell regelt es selbst; Herkunft: `managed`.
  - `Presets{options}` (geordnet vom kleinsten zum größten): `low` = erste, `high` = letzte, `medium` = mittlere
    (bei zwei Stufen die höhere); `none` = ausgeschaltet, wenn das Modell es anbietet, sonst die erste.
- Die Herkunft nennt in `attempts[].effort` die Stufe nach dem Deckeln.

## Prüfung der Ausgabe (Wächter und Schema)

Reihenfolge je Versuch; der erste Verstoß beendet den Versuch mit `unusable` und einem Grund.

1. **Leer**: nur Leerraum oder keine Ausgabe.
2. **Schleife**: bei mindestens 12 Wörtern ein Anteil verschiedener Wörter unter 0,35, oder dieselbe Zeile
   dreimal in Folge.
3. **Schriftwechsel**: Die Ausgabe enthält ein Schriftsystem (CJK, Kyrillisch, Arabisch, Devanagari,
   Tamil u. a.), das weder in `instruction` noch im Eingabetext vorkommt. Latein, Ziffern und Satzzeichen gelten
   immer als bekannt.
4. **JSON**: Die Ausgabe ist (nach Abziehen eines Codezauns) kein gültiges JSON.
5. **Schema**: Der Wert verletzt `format.schema`.

Nach `unusable`, `unavailable` oder `timeout` versucht holzi das nächste Modell der Rangfolge (nur mit
Einwilligung, FR-027), höchstens einmal je Modell. Sind alle erschöpft, endet der Schritt und damit der Auftrag
mit `Unusable` (oder `Unavailable`/`Timeout` je nach letztem Grund).

Der Wächter prüft nur „offensichtlich kaputt“. Erfundene, aber wohlgeformte Inhalte (englische Sätze auf einem
unleserlichen Foto) erkennt er nicht und verspricht es nicht.

## Beispiel: Seitenerkennung, wie haex-notes sie vorgeben würde

Zwei Schritte: erst lesen, dann korrigieren. Die Anweisungen sind Sache von haex-notes (Spec 046); hier nur
zur Veranschaulichung der Form.

```json
{
  "steps": [
    {
      "id": "read",
      "needs": ["text", "image"],
      "instruction": "Zerlege die Seite in Regionen ... Gib nur JSON aus.",
      "input": { "images": [{ "mimeType": "image/jpeg", "data": "<base64>" }] },
      "format": {
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
                  "kind": { "enum": ["text", "image", "table"] },
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
                  "text": { "type": "string", "maxLength": 20000 },
                  "rows": {
                    "type": "array",
                    "items": { "type": "array", "items": { "type": "string" } }
                  }
                }
              }
            }
          }
        }
      },
      "effort": "medium",
      "maxOutputTokens": 8192
    },
    {
      "id": "fix",
      "needs": ["text"],
      "instruction": "Korrigiere nur Rechtschreibung und eindeutige Lesefehler. Behalte Abkürzungen ...",
      "input": { "from": { "step": "read", "pointer": "/regions" } },
      "format": {
        "name": "text-fix",
        "version": 1,
        "schema": {
          "type": "object",
          "required": ["texts"],
          "properties": {
            "texts": { "type": "array", "items": { "type": "string" } }
          }
        }
      },
      "effort": "low"
    }
  ]
}
```

## Prüfung (Vertragstests)

- Gültige und ungültige Aufträge (zu viele Schritte, doppelte `id`, `from` auf spätere oder unbekannte Schritte,
  Schema mit `pattern` oder `$ref`).
- Wächter: je ein Fall für leer, Schleife, Schriftwechsel, ungültiges JSON und Schemaverstoß; die erfundenen
  englischen Sätze bleiben absichtlich unerkannt (Test hält das fest).
- Aufwand: Deckeln bei `Presets` mit zwei, drei und fünf Stufen, `ModelManaged`, `Unavailable`.
- Ausweichweg: Modell 1 unbrauchbar, Modell 2 hat Einwilligung, Modell 3 hat keine: Modell 3 wird nie versucht.
