# Vertrag: Commands und Ereignisse (Rust ↔ Frontend)

**Spec**: [../spec.md](../spec.md) | **Datenmodell**: [../data-model.md](../data-model.md)

Alle Nutzlasten camelCase. „Command“ meint hier ausschließlich Tauri-Commands
(Begriff aus `CONTEXT.md`); fachlich heißt es „Aktion“. Die Commands laufen
durch das Tresor-Gate (`vault_gate`) und sind nur bei offener Sitzung
aufrufbar.

## Commands

### `set_agent_actions`

Ersetzt alle Werkzeuge der Quelle `action` im Register durch die übergebenen.
Wird vom Frontend nach dem Start der Tresor-Sitzung und nach jedem
Sprachwechsel aufgerufen. Idempotent.

```text
args:   { actions: AgentActionDef[] }         // Form: data-model.md §2
result: { registered: number }
errors: InvalidInput  // doppelter toolName, toolName verletzt ^[A-Za-z0-9_-]{1,64}$,
                      // Schema außerhalb der Teilmenge
```

Nebenwirkung: `ToolRegistry` behält `run_command` und MCP-Werkzeuge, ersetzt nur
Werkzeuge mit Quelle `action`. Das Meta-Werkzeug `find_actions` ist immer
vorhanden, sobald mindestens eine Aktion registriert ist. Die Begrenzung auf
15 gilt nur für Aktionswerkzeuge; `run_command` und MCP-Werkzeuge bleiben
zusätzlich erhalten und zählen nicht in dieses Limit.

### `respond_action_call`

Antwort des Frontends auf `action-call-request`.

```text
args:   { requestId: Uuid, outcome: ActionOutcomeWire }
        ActionOutcomeWire = { ok: true, result: any }
                          | { ok: false, code: string, field?: string, message: string }
result: ()
errors: nie für eine unbekannte oder späte requestId (stilles Nichts)
```

Das Frontend sendet nur `code`, `field` und `message` des Runners. Das Feld
`error` (roher Fehler) wird nicht übertragen (FR-006). Bei `code == "failed"`
ersetzt Rust die `message` durch den festen Text `"The action failed."`.

## Ereignisse (Rust → Frontend)

### `action-call-request`

```text
{ requestId: Uuid, actionId: string, input: object }
```

Der globale Listener (`src/composables/useAgentActions.ts`, gestartet von der
Workspace-Seite, sobald der Tresor offen ist) führt
`wm.runAction(actionId, input, { kind: 'builtinAgent' })` aus und antwortet mit
`respond_action_call`. Bleibt die Antwort 60 s aus, bricht Rust mit dem
Werkzeugfehler `action_timeout` ab. Ein Thread ist nicht Teil der Nutzlast:
`Tool::execute` kennt ihn nicht, und der Listener braucht ihn nicht.

### `chat-tool-availability`

```text
{ threadId: Uuid, state: "offered" | "offeredUnverified" | "unsupported" | "delegate",
  delegateKind?: "claudeCodeDelegate" | "codexDelegate" }
```

Einmal je Zug zu Beginn gesendet. Das Frontend zeigt für die drei Zustände
außer `offered` einmal je Unterhaltung und Zustand pro App-Sitzung einen
schließbaren Hinweis (`chat.toolNotice.*`). Seit Spec 021 US6 trägt ein
`delegate`-Zug `delegateKind`; der Hinweis richtet sich dann nach
`mcp_delegate_status` (registriert: keiner; sonst mit „Rechte festlegen“ bzw.
dem Hinweis, dass der MCP-Zugang aus ist).

### `model-tool-use-updated`

```text
{ modelId: string }
```

Nach Probe oder Selbsttest. Das Frontend lädt die Modelllisten neu
(`list_installed_models` / `list_provider_models`), die `toolUse` in
`capabilities` tragen.

## Geänderte bestehende Verträge

| Vertrag                                          | Änderung                                                                                                                |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `tool-permission-request.riskClass`              | neuer Wert `"change"` (vorher `"safe" \| "risky"`)                                                                      |
| `tool-permission-request.toolSource`             | neu: `"mcp" \| "cli" \| "action"`; der Dialog formuliert Aktionen in Klartext                                           |
| `chat-tool-call.toolSource`                      | neuer Wert `"action"` (vorher `"mcp" \| "cli"`); TS-Typ `ToolCallEvent.toolSource`                                      |
| `list_installed_models` / `list_provider_models` | `capabilities.toolUse` (`{ support, basis } \| null`)                                                                   |
| Werkzeug-Fehlertexte                             | neu: `action_timeout`, `action_unavailable`; bestehend: `denied_by_user`, `blocked_by_plan_mode`, `tool_call_cancelled` |

## Suchwerkzeug `find_actions` (für das Modell)

```text
name:        find_actions
description: Search the actions holzi offers by keywords (English works best),
             or enumerate the complete catalog page by page when query is absent.
             Matches become callable tools in the next step.
input:       { query?: string, cursor?: string, limit?: integer }
risk:        Safe, source "action"
result:      { actions: [{ tool: string, description: string, inputSchema: object }],
              nextCursor: string | null }
```

- `limit` ist optional, standardmäßig 5 und höchstens 5. Mit `query` gibt es
  höchstens 5 Treffer je Seite, wortbasiert über camelCase-zerlegte
  ID-Segmente, Beschreibung und deutsche und englische Titel; Gleichstand nach
  stabiler ID-Reihenfolge. Ohne Treffer: `{ actions: [], nextCursor: null }`
  und der Hinweis `"no matching action"`.
- Ohne `query` werden alle registrierten, für den eingebauten Agenten
  aufrufbaren Aktionen in stabiler ID-Reihenfolge paginiert. `nextCursor` ist
  ein undurchsichtiger Fortsetzungstoken oder `null` auf der letzten Seite.
  Der vollständige Katalog ist daher abrufbar, indem der Aufrufer denselben
  Suchaufruf mit jedem `nextCursor` wiederholt; ein fehlender `query` bedeutet
  ausdrücklich „alle Aktionen“, nicht nur das Kernangebot.
- Die Treffer stammen aus dem Register, nicht aus einem Frontend-Umlauf.
  Nach einer Runde mit diesem Werkzeug hängt der Zug die Treffer an
  `request.tools` an. Vor dem Anhängen entfernt `extend_offer` Treffer, deren
  `toolName` bereits im Kernangebot vorkommt, und dedupliziert die übrigen
  Treffer nach `toolName`; eine spätere Suche ersetzt die früheren. So bleiben
  höchstens 15 Aktionswerkzeuge im Angebot (Kernangebot ≤ 10 plus höchstens 5
  eindeutige Nicht-Kern-Treffer). `run_command` und MCP-Werkzeuge sind davon
  ausgenommen und werden nicht entfernt.
- Das Kernangebot (`core: true` in `AgentActionDef`) ist in jedem Schritt für
  jedes Modell gleich.
