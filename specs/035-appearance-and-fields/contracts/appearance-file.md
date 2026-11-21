# Vertrag: Darstellungsdatei (Export/Import)

Dateiendung `.haex-appearance.json`, UTF-8, höchstens 16 KiB.

```json
{
  "format": "haex-appearance",
  "v": 1,
  "colorScheme": "system",
  "appearance": {
    "v": 1,
    "accent": { "preset": "teal" },
    "window": { "preset": "neutral" },
    "container": { "preset": "neutral" },
    "text": { "preset": "neutral" },
    "component": { "preset": "neutral" },
    "windowHint": false
  }
}
```

## Prüfung beim Import (alles oder nichts, FR-021)

Die Datei wird **vollständig** gelesen und geprüft, bevor etwas geschrieben wird. Jede Verletzung
bricht den Import ab; die Meldung nennt die erste Ursache (Schlüssel, nicht den Wert):

| Fall                                             | Meldungsschlüssel                                 |
| ------------------------------------------------ | ------------------------------------------------- |
| größer als 16 KiB oder kein JSON                 | `settings.appearance.import.notJson`              |
| `format` ≠ `haex-appearance`                     | `settings.appearance.import.notAppearance`        |
| `v` oder `appearance.v` unbekannt                | `settings.appearance.import.version`              |
| `colorScheme` nicht `light`, `dark`, `system`    | `settings.appearance.import.field` (mit Feldname) |
| Farbangabe weder gültiges `preset` noch `custom` | `settings.appearance.import.field`                |
| Preset nicht in der passenden Reihe              | `settings.appearance.import.field`                |
| `custom` kein sechsstelliges Hex                 | `settings.appearance.import.field`                |
| `windowHint` kein Wahrheitswert                  | `settings.appearance.import.field`                |
| fehlendes Feld                                   | `settings.appearance.import.field`                |
| Schreiben in die Vault schlägt fehl              | `settings.appearance.import.failed`               |

Anders als beim Lesen aus der Vault (ungültiges Feld → Standard, R1/data-model) ist der Import
**streng**: kein Feld fällt auf den Standard, die Datei ist ganz gültig oder wird abgelehnt.
Unbekannte zusätzliche Felder werden beim Import abgelehnt (`field`), damit Tippfehler auffallen.

Die Datei enthält nur Farbangaben, Schema und Hinweis: keine Vault-Daten, keine Pfade, keine
Gerätenamen (FR-021).

## Schreiben

Beim Import schreibt die Aktion in dieser Reihenfolge `appearance.color_scheme` und
`appearance.theme`; schlägt der zweite Schreibvorgang fehl, wird der erste zurückgeschrieben,
und die Meldung bleibt `settings.appearance.import.failed`.
