# Vertrag: Manifest-Block `tools`

Gilt für vault-sdk (Crate `haex-bundle`, Typ `ExtensionManifest`, Werkzeug `haex sign`), holzi und
haex-vault 2.x (Legacy). Normativ für das Format ist das Crate; holzi prüft zusätzlich die Schema-Teilmenge.

## Beispiel

```json
{
  "tools": [
    {
      "name": "count_unread",
      "title": { "de": "Ungelesene Mails zählen", "en": "Count unread mails" },
      "description": "Counts unread messages, optionally in one mailbox role such as inbox.",
      "inputSchema": {
        "type": "object",
        "properties": { "role": { "type": "string", "enum": ["inbox", "all"] } }
      },
      "effect": "read",
      "examples": {
        "de": ["Wie viele ungelesene Mails habe ich?", "Habe ich neue Mails?"],
        "en": ["How many unread mails do I have?", "Do I have new mail?"]
      }
    }
  ]
}
```

## Format-Regeln (Crate, Fehler `manifest_invalid`)

1. `tools` fehlt oder ist eine Liste mit 0–32 Objekten.
2. `name`: `^[a-z][a-z0-9_]{0,31}$`; Namen eindeutig.
3. `title.de`, `title.en`: Strings, 1–80 Zeichen.
4. `description`: String, 1–1024 Zeichen.
5. `inputSchema`: Objekt mit `"type": "object"`; Tiefe ≤ 8.
6. `effect`: genau `read`, `change` oder `destructive`.
7. `examples.de`, `examples.en`: Listen mit 1–10 Strings, je 1–200 Zeichen.
8. Unbekannte Schlüssel in einem Werkzeug → `manifest_invalid`.

Das Manifest ist kanonisches JSON ohne Gleitkommazahlen (RFC 8785, `jcs.rs`): Schemas enthalten nur ganze
Zahlen. Neue Testvektoren: `bad-tools-name.xt`, `bad-tools-effect.xt`, `bad-tools-duplicate.xt`,
`bad-tools-examples.xt`, `ok-tools.xt`.

## Schema-Teilmenge (holzi)

Wie Aktionen (`schema_in_subset`, `chat/tools/action_tool.rs`). Ein Werkzeug außerhalb der Teilmenge wird
nicht angeboten und in den Einstellungen als „nicht unterstützt“ gezeigt; die Installation scheitert daran
nicht.
