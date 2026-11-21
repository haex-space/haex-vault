# Data Model: Werkzeuge von Erweiterungen für den Agenten

## Manifest: `tools[]` (signiert, Teil des Bundles)

Vertrag: [contracts/manifest-tools.md](./contracts/manifest-tools.md).

| Feld          | Typ                                   | Regel                                                      |
| ------------- | ------------------------------------- | ---------------------------------------------------------- |
| `name`        | string                                | `^[a-z][a-z0-9_]{0,31}$`, eindeutig in der Erweiterung     |
| `title`       | `{de: string, en: string}`            | je 1–80 Zeichen                                            |
| `description` | string                                | Englisch, 1–1024 Zeichen; für das Modell                   |
| `inputSchema` | object                                | JSON-Schema, `type: "object"`; nur ganze Zahlen (RFC 8785) |
| `effect`      | `"read" \| "change" \| "destructive"` | Wirkungsart (Clarification 2026-10-10)                     |
| `examples`    | `{de: string[], en: string[]}`        | je 1–10 Sätze, je 1–200 Zeichen                            |

Höchstens 32 Einträge. Fehlt `tools`, hat die Erweiterung keine Werkzeuge.

Rust: `extensions/bundle/manifest.rs` → `Manifest.tools: Vec<ManifestTool>`; `from_verified`,
`from_dev_file`, `from_stored` lesen das Feld. Ein Werkzeug außerhalb der Schema-Teilmenge haex-vaults bekommt
`supported: false` (R5).

## Berechtigung `agentTool` (Tabelle `extension_permissions`, bestehend)

| Spalte              | Wert                                                                |
| ------------------- | ------------------------------------------------------------------- |
| `kind`              | `agentTool` (neu in `PermissionKind`)                               |
| `action`            | `read` \| `change` \| `destructive` (Wirkungsart)                   |
| `target`            | Werkzeugname, oder `*` für den Schalter „Für den Agenten verfügbar“ |
| `status`            | `granted` \| `ask` \| `denied`                                      |
| `declared`          | `1` für Zeilen aus dem Manifest, `0` für die `*`-Zeile des Nutzers  |
| `vault_device_uuid` | Sentinel „alle Geräte“ (nicht gerätegebunden)                       |

Wirkung je Werkzeug (R4):

| Zeilen                        | Angebot | Freigabe                                       |
| ----------------------------- | ------- | ---------------------------------------------- |
| `*` denied                    | nein    | —                                              |
| Werkzeug denied               | nein    | —                                              |
| Werkzeug granted              | ja      | nach Wirkungsart und Modus (032 FR-007)        |
| Werkzeug ask                  | ja      | immer Zustimmung, in jedem Modus (auch „Plan“) |
| keine Zeile (nicht bestätigt) | nein    | —                                              |

Update: `new_declarations` legt nur neue `(agentTool, effect, name)` vor; eine geänderte Wirkungsart ist eine
neue Zeile, die alte fällt mit `apply_declarations` weg. Entwicklungsversionen:
`dev_extension_permissions_no_sync`.

## Laufzeit (nicht gespeichert)

- **`ExtensionToolDef`**: Erweiterungs-Id, Anzeigename, Manifest-`name`, `ManifestTool`, Werkzeugname für das
  Modell (`x_<slug>_<tool>`, R8), effektive Freigabe (`granted`/`ask`), Entwicklungsversion.
  Entsteht aus Manifest + Berechtigungen beim Laden der Vault und bei `extensions-changed`.
- **`ExtensionTool`** (`impl Tool`): `source() = "haextension"`, `origin() = Some(ToolOrigin)`,
  `risk_class()` aus der Wirkungsart (`read` → `Safe`, `change` → `Change` (haex-vaults `ActionEffect::Write`),
  `destructive` → `Risky`); `always_ask()` = Status `ask`; `execute` über die MCP-Verbindung.
- **`McpLink`** je Rahmen-Sitzung: `rmcp`-Client über Kanalpaar, Zustand
  `connecting → ready(tools/list) → closed`; Zähler laufender Aufrufe (für `byAgent`, R11).
- **`ToolOrigin`**: `{extensionId, extensionName, toolTitle: {de, en}, dev: bool}`.
- **Passendes Angebot**: höchstens 3 `ExtensionToolDef` je Antwort, berechnet aus der Nachricht (R6).

## Chat-Verlauf (`chat_messages`, bestehend)

| Spalte        | Änderung                                                           |
| ------------- | ------------------------------------------------------------------ |
| `tool_source` | neuer Wert `haextension` (keine CHECK-Bedingung vorhanden)         |
| `tool_origin` | **neu**, `TEXT NULL`, JSON von `ToolOrigin`; nur bei `haextension` |

Migration in `identity/migrations.rs` (nächste Nummer). Frontend-Typ `toolSource` um `'haextension'`
erweitert, `toolOrigin?: ToolOrigin`.

## Ereignisse (Erweiterungen)

- `PermissionRequestEvent` (`permissions/prompts.rs`): neues Feld `byAgent: bool` (R11).
- `ToolPermissionRequestEvent` (`chat/events.rs`): neues Feld `toolOrigin?: ToolOrigin`.
