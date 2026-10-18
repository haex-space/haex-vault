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
  needs: ("text" | "image")[]    // mindestens "text", jedes höchstens einmal; "image" genau dann, wenn der
                                 // Schritt Bilder hat (verlangt ein Modell mit Bildeingabe)
  instruction: string      // Anweisung der Erweiterung, höchstens 16 KiB, nicht leer
  input: {
    text?: string          // eigener Text, höchstens 256 KiB; leer oder nur Leerraum zählt nicht als Eingabe
    images?: Image[]       // höchstens 8 je Auftrag insgesamt
    from?: { step: string, pointer: string }   // früherer Schritt, JSON-Pointer (RFC 6901) auf einen Teil
  }
  format: {
    name: string           // z. B. "page-recognition", 1 bis 64 Zeichen
    version: number        // ganze Zahl >= 1
    schema: object         // Schema-Teilmenge, siehe unten
  }
  effort?: "none" | "low" | "medium" | "high"
  maxOutputTokens?: number // bis zur Obergrenze (Standard 4096, höchstens 16384)
}

Image { mimeType: "image/jpeg" | "image/png" | "image/webp", data: string /* Base64, höchstens 25 MiB roh */ }
// Base64 statt einer Zahlenliste: der alte Handler (Spec 047, nie aufgerufen) erwartete Bytes als JSON-Liste;
// ein 25-MiB-Foto wären so rund 100 MB JSON, als Base64 rund 33 MB.
```

- `from` darf nur auf einen **früheren** Schritt zeigen; der Teil unter `pointer` wird dem Modell als Text
  (JSON) angehängt. Beim Start prüft holzi, dass `step` früher im Auftrag steht und dass `pointer` nach dem
  Schema des referenzierten früheren Schritts vorkommen kann; sonst `InvalidInput`. Fehlt der Teil zur Laufzeit im tatsächlichen
  Ergebnis, endet der Schritt mit `unusable` (Grund „from missing“) und der Ausweichweg greift nicht, weil das
  Ergebnis des früheren Schritts feststeht.
- `needs` enthält `"image"` genau dann, wenn der Schritt `images` hat; ein Schritt ohne Bilder, der `"image"`
  verlangt, würde ein Bildmodell ohne Grund binden. Ein Schritt hat mindestens Text, Bilder oder `from`.
- Unbekannte Felder werden abgelehnt.
- Der Text für das Modell besteht aus `instruction` (Systemanweisung), dem Eingabetext und dem Schema als
  Hinweis auf das erwartete JSON. holzi fügt einen festen Satz hinzu, der verlangt, nur gültiges JSON nach dem
  Schema auszugeben. Mehr stellt holzi nicht voran; die Anweisung bleibt die der Erweiterung.

## Schema-Teilmenge

Modul `S/model_jobs/schema.rs` (nicht der Aktionsprüfer von Spec 018: dessen Teilmenge ist kleiner und im Frontend gespiegelt).

- Typen: `object`, `array`, `string`, `integer`, `number`, `boolean`, `null` nur als Union mit einem anderen Typ.
- Schlüsselwörter: `type`, `properties`, `required`, `items`, `enum`, `minimum`, `maximum`, `minLength`,
  `maxLength`, `minItems`, `maxItems`, `additionalProperties` (boolesch), `description`.
- `enum` darf ohne `type` stehen. Ein Objekt mit `properties` erlaubt keine weiteren Schlüssel, außer
  `additionalProperties: true`; ein Objekt ohne `properties` erlaubt jeden Schlüssel, außer
  `additionalProperties: false`. Längen zählen Zeichen, nicht Bytes.
- Ein Schema, das kein Wert erfüllen kann, wird beim Start abgelehnt (es kostete sonst jeden Modellaufruf der
  Rangfolge): ein `enum`-Wert, der nicht zum `type` passt; `minimum` über `maximum` (ebenso bei Längen und
  Anzahlen); ein Schlüsselwort eines anderen Typs (`items` bei `string`, `properties` bei `array` usw.).
- Nicht erlaubt: `$ref`, `pattern`, `oneOf`/`anyOf`/`allOf`, Verweise, Rekursion. Ein optionaler Wert wird als
  Typpaar mit `null` geschrieben (`["string", "null"]`), nicht mit `anyOf`.
- Grenzen: Tiefe 8, höchstens 200 Eigenschaften insgesamt, Schema höchstens 32 KiB.
- Ergebnisse höchstens 256 KiB; Zahlen als IEEE-754 ohne Sonderwerte.

## Aufwand

- Die Erweiterung gibt höchstens einen Wunsch an. holzi bildet `min(Wunsch, höchster freigegebener Aufwand)`;
  fehlt der Wunsch, gilt `none`.
- Die Stufen eines Modells kommen in der vom Anbieter gemeldeten Reihenfolge, die nicht als aufsteigend zugesichert
  ist. Sind alle Namen bekannt (`none`/`off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`), ordnet holzi sie
  nach Rang; sonst gilt die gemeldete Reihenfolge.
- Zuordnung auf das Modell nach seiner Auskunft (`ModelCapabilities.reasoning`):
  - `Unavailable`: Aufwand wirkt nicht; Herkunft: `none`.
  - `ModelManaged`: das Modell regelt es selbst; Herkunft: `managed`.
  - `Presets{options}`: `none` nimmt den Ausschalter (`none`/`off`), wenn das Modell einen hat; `low`, `medium`
    und `high` nehmen ihn nie.
    - Alle Namen bekannt: die Stufe nimmt die höchste Option, die sie nicht übersteigt (`minimal` zählt als
      `low`; `xhigh` und `max` liegen über `high`, der höchsten Freigabe), und die niedrigste, wenn alle höher
      liegen. Beispiel Anthropic `low, medium, high, max`: `high` sendet `high`, nie `max`; bei `low, high`
      sendet `medium` `low`.
    - Ein Name des Anbieters dabei (gemeldete Reihenfolge): eine Option, die wie die Stufe heißt, gewinnt;
      sonst `low` = erste, `high` = letzte, `medium` = mittlere (bei zwei die höhere).
- Die Herkunft nennt in `attempts[].effort` die tatsächlich gesendete Stufe: bei einem bekannten Namen dessen
  Stufe (`xhigh`/`max` als `high`), sonst die gedeckelte. Hat ein Modell keinen Ausschalter, sendet `none` die
  niedrigste Option und die Herkunft nennt `low`.

## Prüfung der Ausgabe (Wächter und Schema)

Reihenfolge je Versuch; der erste Verstoß beendet den Versuch mit `unusable` und einem Grund.

1. **Leer**: nur Leerraum oder keine Ausgabe.
2. **Schleife**: Geprüft wird der Text der Zeichenketten der Antwort (bei ungültigem JSON der ganze Text).
   Eine Schleife ist: bei mindestens 100 Wörtern weniger als ein verschiedenes Wort auf zehn; oder dieselbe
   Wortfolge von 2 bis 8 Wörtern mindestens 6-mal hintereinander (und mindestens 12 Wörter), ein einzelnes
   Wort mindestens 30-mal; oder dieselbe Zeile mit mindestens 8 Buchstaben oder Ziffern dreimal in Folge
   innerhalb einer Zeichenkette. Die Schwellen sind so weit, dass eine lange Seite ähnlicher Zeilen, eine
   Tabelle voller „ja“-Zellen, drei gleiche Zellen nebeneinander („Mathematik“ an drei Tagen) und Antwortlinien
   aus `____` oder `....` nicht auffallen. Bekannter Preis: eine Tabelle, die zwölfmal „ja nein“ abwechselt,
   und lange Tabellen aus wenigen Zahlen als Zeichenketten (eine Wahrheitstafel aus „0“ und „1“) fallen auf.
3. **Schriftwechsel**: Die Antwort enthält mindestens 3 Buchstaben eines Schriftsystems (Griechisch, Kyrillisch,
   Armenisch, Hebräisch, Arabisch, Devanagari, Bengalisch, Tamil, Thai, Georgisch, Hangul, Kana, Han), das weder
   Latein ist noch in Anweisung oder Eingabetext vorkommt, und diese Buchstaben sind mindestens ein Fünftel
   aller Buchstaben. Es zählen nur Buchstaben in einem Wort mit mindestens zwei Buchstaben dieser Schrift:
   `α = 30°`, `Δx` oder die Ecken `a b c α β γ` sind kein Wechsel. Ziffern, Satzzeichen, Symbole und Umlaute
   zählen nie als fremd. Wer Russisch oder Altgriechisch erwartet, nennt die Schrift in der Anweisung.
4. **JSON**: Die Ausgabe ist kein gültiges JSON. Ist sie es nicht als Ganzes, gilt der Inhalt des ersten
   Codezauns, auch mit einem Satz davor oder danach (` ```json `-Kennung wird abgezogen).
5. **Schema**: Der Wert verletzt `format.schema`.

Nach `unusable`, `unavailable` oder `timeout` versucht holzi das nächste Modell der Rangfolge (nur mit
Einwilligung, FR-027), höchstens einmal je Modell. Sind alle erschöpft, endet der Schritt und damit der Auftrag
mit `unusable` (oder `unavailable`/`timeout` je nach letztem Grund).

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
