# Contracts: Tauri Commands & Events (Agent Tool Loop)

Alle Kommandos sind async, laufen im Tauri-`invoke_handler` und geben `Result<T, HaexError>`
zurück. Rust-`snake_case` wird beim JSON-Payload zu `camelCase` per
`#[serde(rename_all = "camelCase")]` — bestehende Konvention, unverändert.

## Geänderte Commands

### `send_message(args) -> SendMessageResult`

**Args/Result-Typen unverändert** (`thread_id`, `content`, `system_prompt`, `max_new_tokens`,
`idempotency_key` → `{ thread_id, user_message_id, assistant_message_id }`).

**Verhalten geändert**: löst statt eines einzelnen LLM-Calls die Turn/Step-Loop aus (spec.md User
Story 1+2). `assistant_message_id` wird zu Beginn des Requests genau einmal vorab geminted und
bleibt über alle LLM-Retries hinweg stabil. Wenn eine finale Assistant-Zeile persistiert wird, ist
es deren ID; dazwischenliegende `tool_call`/`tool_result`-Zeilen (siehe data-model.md) bekommen
eigene, neu geminted IDs, die über die neuen Events unten bekannt gegeben werden — kein
Args/Result-Contract-Bruch, nur mehr Events pro Aufruf.

### `abort_current_generation() -> ()`

**Args/Result unverändert.**

**Verhalten geändert**: bricht nicht mehr nur den LLM-Stream ab, sondern den gesamten laufenden Turn
— inklusive eines aktuell laufenden Tool-Prozesses und eines offenen `tool-permission-request`
(spec.md FR-009/FR-010). Weiterhin idempotent.

## Neue Commands

### `respond_tool_permission(request_id, decision) -> ()`

Beantwortet eine offene Freigabe-Anfrage (spec.md FR-005). Zweiter Teil des Request/Response-Paars
zum `tool-permission-request`-Event unten.

**Args**:

```typescript
{ requestId: string /* uuid */, decision: 'allow' | 'deny' }
```

**Returns**: `void`.

**Fehler**:

- `HaexError::InvalidInput` wenn `requestId` keiner offenen Anfrage entspricht (bereits
  beantwortet, oder der Turn wurde inzwischen abgebrochen). Aufgelöste IDs werden als Tombstone
  behalten: eine späte Antwort auf eine durch Cancellation aufgelöste ID ist erfolgreich und wird
  als No-op behandelt; `InvalidInput` gilt nur für eine niemals bekannte ID.

**Verhalten**: löst den in `ChatState.pending_tool_approvals` wartenden `oneshot::Sender` auf und
markiert bei Cancellation die Request-ID als aufgelöst. Kein Timeout — die Anfrage bleibt offen, bis
beantwortet oder der Turn abgebrochen wird (spec.md FR-005, Acceptance Scenario 5). T027 testet
jeweils eine Cancellation-resolved-ID und eine unbekannte ID.

## Neue Events (Backend → Frontend)

Bestehende Events (`chat-token`, `chat-message-complete`, `chat-message-error`,
`model-load-progress`) bleiben unverändert in Form und Bedeutung; die ersten drei feuern weiterhin
pro Step statt nur einmal pro `send_message`-Aufruf (ein Turn kann mehrere Steps haben).
`model-load-progress` bleibt ausschließlich dem Modell-Laden zugeordnet. `chat-message-complete` und
`chat-message-error` beenden daher keinen Turn im Frontend; dafür gibt es das abschließende Event
`chat-turn-complete`.

### `tool-permission-request`

Emittiert, wenn das Freigabe-Gate laut aktivem `chat.permission_mode` eine Bestätigung braucht
(spec.md FR-005). Beantwortet über `respond_tool_permission`.

```typescript
{
  requestId: string,      // uuid, Korrelation zur Antwort
  threadId: string,
  toolName: string,
  toolInput: unknown,     // geparstes JSON, wie es dem Modell/Tool vorliegt
  riskClass: 'safe' | 'risky',
}
```

### `chat-tool-call`

Eine `tool_call`-Zeile (data-model.md) wurde persistiert — das Modell hat ein Tool aufgerufen. Das
Event wird an dieser Persistenzgrenze emittiert, auch wenn der Aufruf anschließend im Plan-Modus
blockiert wird; eine vorherige Freigabe ist keine Voraussetzung.

```typescript
{ messageId: string, threadId: string, toolName: string, toolInput: unknown, toolSource: 'mcp' | 'cli' }
```

### `chat-tool-result`

Eine `tool_result`-Zeile wurde persistiert — Ausführung abgeschlossen (erfolgreich, fehlgeschlagen,
oder als "blocked" laut Plan-Modus, spec.md FR-006).

```typescript
{ messageId: string, threadId: string, toolCallId: string, content: string, isError: boolean }
```

### `chat-retry`

Transient, **nicht persistiert** (design doc §3/§6) — informiert die UI über einen laufenden
automatischen Retry-Versuch (spec.md User Story 4), ohne dass ein fehlgeschlagener Versuch je in der
Konversation sichtbar wird.

```typescript
{ threadId: string, assistantMessageId: string, attempt: number /* 1-basiert */ }
```

### `chat-turn-complete`

Wird genau einmal nach dem letzten Step eines `send_message`-Turns emittiert. Es folgt auf das
jeweilige per-Step-Event und signalisiert dem Frontend erst dann, `streamingMessageId` und `busy` zu
löschen. `chat-message-complete`/`chat-message-error` bleiben weiterhin pro Step erhalten.

```typescript
{
  threadId: string,
  assistantMessageId: string | null, // ID der terminalen assistant-Zeile, falls persistiert
  finishReason: 'complete' | 'cancelled' | 'error' | 'tool_limit_reached',
}
```

Bei `tool_limit_reached` wird eine terminale Assistant-Zeile mit der vorab geminteten
`assistantMessageId` persistiert; diese ID stimmt mit `send_message` und dem Event überein. Bei
`cancelled` oder einem Fehler ohne Assistant-Zeile ist `assistantMessageId` `null`.

## Geänderte Datentypen

### `FinishReason` (bestehend: `Complete` | `Cancelled` | `Error`)

**Neue Variante**: `ToolLimitReached` — Turn hat die feste Rundenobergrenze (spec.md FR-016)
erreicht, ohne eine finale Antwort zu liefern. Muss von `Error` unterscheidbar bleiben (spec.md
Edge Cases: "distinguishable from a user-initiated stop and from retry exhaustion"), damit Frontend
eine andere Meldung zeigen kann als bei einem echten Fehler.

```typescript
type FinishReason = 'complete' | 'cancelled' | 'error' | 'tool_limit_reached'
```

## Nicht geänderte Contracts (zur Klarstellung)

- `chat.permission_mode` hat **keine eigenen Commands** — wird über die bestehenden generischen
  `get_pref` / `set_pref` (Scope `Device`) gelesen/geschrieben, siehe data-model.md.
- `list_messages` (bestehend, `thread_commands.rs`) bekommt keinen neuen Parameter — `tool_call`/
  `tool_result`-Zeilen laufen einfach als zusätzliche Rollenwerte durch den bestehenden Rückgabetyp;
  Frontend muss neue `role`-Werte rendern können, das ist eine reine Erweiterung, kein Bruch.
