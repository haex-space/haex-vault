# Datenmodell: Modell bedient haex-vault über die Aktionen

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Es gibt **keine Migration** und keine neue Tabelle. Neu sind ein Feld in der
bestehenden Fähigkeiten-JSON, ein optionales Feld im Aktionskatalog (nur
Speicher, TypeScript), eine Zustandsmenge im Speicher von Rust und zwei
eingebettete JSON-Dateien.

## 1. Erweiterung `ActionDefinition` (TypeScript, `src/lib/actions/types.ts`)

| Feld                   | Typ        | Bedeutung                                                                                 |
| ---------------------- | ---------- | ----------------------------------------------------------------------------------------- |
| `builtinAgentCallable` | `boolean?` | Standard `true`; `false`: nur für externe Agenten, nicht für den eingebauten Agenten (R5) |

Belegung: `builtinAgentCallable: false` bei `chat.message.send`, `chat.message.retry`,
`chat.reply.cancel`. Der Runner lehnt `builtinAgent`-Aufrufe für
`builtinAgentCallable === false` mit `forbidden_for_agents` ab.

## 2. `AgentActionDef` (Übergabe Frontend → Rust)

Reine Daten, abgeleitet aus `ActionDefinition` durch
`toAgentActionDef(def, locale)` in `src/lib/actions/agentTools.ts`.

| Feld          | Typ                                | Regel                                                                    |
| ------------- | ---------------------------------- | ------------------------------------------------------------------------ |
| `toolName`    | `string`                           | `id` mit `.` → `_`; `^[A-Za-z0-9_-]{1,64}$`, eindeutig (R3)              |
| `actionId`    | `string`                           | Original-ID, dient Rust nur als Rückgabewert an das Frontend             |
| `description` | `string`                           | `ActionDefinition.description` (Englisch) unverändert (FR-001)           |
| `inputSchema` | JSON-Schema-Teilmenge              | `ActionDefinition.input` unverändert                                     |
| `effect`      | `read` \| `write` \| `destructive` | aus der Aktion                                                           |
| `core`        | `boolean`                          | `true` für die Aktionen des festen Kernangebots (`CORE_AGENT_TOOLS`, R6) |
| `titles`      | `{ de: string, en: string }`       | lokalisierte Titel für die Wortsuche von `find_actions`                  |

Enthalten sind nur Aktionen mit `agentCallable && builtinAgentCallable !== false`.
Leitplanken sind damit nie im Register (FR-003); zusätzlich lehnt der Runner
sie ab (Verteidigung in der Tiefe).

## 3. `ActionTool` (Rust, `chat/tools/action_tool.rs`)

Implementiert `Tool` aus `chat/tools/mod.rs`. Ein `ActionTool` hält die
vollständige `AgentActionDef` (§2), nicht nur Name und Schema: das
Kernangebot braucht `core`, die Suche `titles` (R6).

| Methode          | Wert                                                                                               |
| ---------------- | -------------------------------------------------------------------------------------------------- |
| `name()`         | `toolName`                                                                                         |
| `description()`  | `description`                                                                                      |
| `source()`       | `"action"`                                                                                         |
| `input_schema()` | `inputSchema`                                                                                      |
| `risk_class()`   | `read` → `Safe`, `write` → `Change`, `destructive` → `Risky`                                       |
| `execute()`      | Umlauf über `ActionBridge` (Vertrag: [contracts/tauri-commands.md](./contracts/tauri-commands.md)) |

`RiskClass` (`chat/tools/mod.rs`) wird zu `Safe | Change | Risky`. Wire-Werte
`"safe" | "change" | "risky"` (`chat/events.rs:risk_class_str`). Die Matrix
steht in [research.md R4](./research.md).

## 4. `ActionBridge` (Rust, nur Speicher)

| Feld       | Typ                                                  | Zweck                                                       |
| ---------- | ---------------------------------------------------- | ----------------------------------------------------------- |
| `emitter`  | `OnceLock<EventEmitter>`                             | in `setup()` gesetzt; vor dem Setzen → `action_unavailable` |
| `pending`  | `Mutex<HashMap<Uuid, oneshot::Sender<ActionReply>>>` | offene Aufrufe, Schlüssel = `requestId`                     |
| `run_lock` | `tokio::sync::Mutex<()>`                             | serialisiert die Ausführung innerhalb einer Runde           |

`ActionReply = { ok: true, result: Value } | { ok: false, code: String,
field: Option<String>, message: String }`. Zustandsübergänge: _angelegt_ →
(_beantwortet_ | _Zeitüberschreitung_ | _abgebrochen_ | _Tresor geschlossen_).
Nach jedem Übergang wird der Eintrag entfernt; eine späte Antwort ist ein
Nichts.

## 5. `ToolUse` in `ModelCapabilities` (Rust, `model_capabilities.rs`)

```text
ModelCapabilities { …, tool_use: Option<ToolUse> }   // None = unbekannt
ToolUse { support: Supported | Unsupported,
          basis:   Provider | Curated | Template | SelfTest }
```

JSON (`capabilities_json`, camelCase, `#[serde(default)]`): ein Eintrag ohne
`toolUse` liest sich als `None`. `is_undetermined()` bleibt „gleich
`Default`“. Übergänge (Rangfolge FR-015, höherer Rang überschreibt niedrigeren,
nie umgekehrt):

```text
unbekannt ──Probe: Vorlage ohne Werkzeuge──▶ Unsupported/Template
unbekannt ──Selbsttest ≥ Schwelle──────────▶ Supported/SelfTest
unbekannt ──Selbsttest < Schwelle──────────▶ Unsupported/SelfTest
beliebig  ──Provider/Katalog gibt Wert─────▶ Supported|Unsupported / Provider|Curated
neue Modelldatei (erneuter Download) ──────▶ unbekannt (Feld zurückgesetzt)
```

`basis` dient nur der Rangfolge und der Fehlersuche, das Frontend zeigt sie
nicht. TypeScript: `ModelCapabilities.toolUse: { support, basis } | null`
in `src/composables/useModels.ts`.

**Katalog** (`src-tauri/src/catalog/model_catalog.json`): `CatalogEntry`
bekommt das optionale Feld `tool_use` (gleiche Form ohne `basis`, Basis ist
dann `Curated`). Zunächst leer; nach dem ersten Messlauf gefüllt.

## 6. `ToolAvailability` (Rust → Frontend, vorübergehend)

`Offered | OfferedUnverified | Unsupported | Delegate`. Je Zug aus
`(provider_kind, capabilities.tool_use)` bestimmt; nicht gespeichert.

| Bedingung                         | Zustand             | Werkzeuge angeboten |
| --------------------------------- | ------------------- | ------------------- |
| `provider_kind == CliDelegate`    | `Delegate`          | nein                |
| `tool_use.support == Unsupported` | `Unsupported`       | nein                |
| `tool_use.support == Supported`   | `Offered`           | ja                  |
| `tool_use == None`                | `OfferedUnverified` | ja                  |

## 7. Beispielsatz-Satz und Bericht (eingebettete JSON, [contracts/eval-format.md](./contracts/eval-format.md))

| Entität        | Felder                                                                                                                                                |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `EvalSet`      | `version` (ganze Zahl), `sentences[]`                                                                                                                 |
| `EvalSentence` | `id`, `lang` (`de` \| `en`), `kind` (`read` \| `change` \| `smalltalk`), `text`, `expect` (`none` oder Liste aus `{ tool, args }`), `selfTest` (bool) |
| `EvalTools`    | Schnappschuss der `AgentActionDef`-Liste, erzeugt aus dem Katalog                                                                                     |
| `EvalReport`   | `setVersion`, `model`, `total`, `perLang`, `perKind`, `reachRate`, `spuriousCalls`, `failures[]`                                                      |

Regel `expect.args`: nur die Felder, auf die es ankommt; zusätzliche Felder
des Modells sind erlaubt, solange das Schema sie zulässt.
