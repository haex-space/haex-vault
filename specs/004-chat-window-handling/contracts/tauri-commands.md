# Contracts: Chat Runtime und Tauri-Schnittstelle

Alle neuen bzw. erweiterten Kommandos liefern `Result<T, HolziError>`. Rust-
Felder werden wie bisher per `camelCase` serialisiert.

## Bestehende Kommandos mit geänderter Nutzung

### `send_message(args)`

Die Chat-Seite sendet für einen neuen Einstieg weiterhin:

```typescript
{
  threadId: null,
  content: string,
  systemPrompt?: string,
  maxNewTokens?: number,
  idempotencyKey: string
}
```

`threadId: null` MUSS einen neuen Thread erzeugen. Ein explizit ausgewählter
Historien-Thread sendet dagegen seine konkrete ID. Idempotenz- und
Retry-Semantik aus Spec 002/003 bleibt unverändert.

### `resolve_default_model()`

Bleibt unverändert und bleibt die autoritative Fallback-Kette aus Spec 002. Der
neue Preload verwendet dieselbe Funktion intern nach dem Vault-Open. Die
passive Auswahl schreibt weiterhin kein `last_active_model_id`.

### `load_model(modelId)`

Bleibt für manuelle Modellwechsel verfügbar. Seine gemeinsame interne
Load-Funktion wird zusätzlich vom Preload verwendet. Ein manueller Wechsel
invalidiert einen älteren Preload über dessen `loadId`.

## Neuer Snapshot-Command

### `model_load_status() -> ModelLoadStatusPayload`

Liefert den aktuell bekannten Load-Zustand für den aktiven Vault.

```typescript
type ModelLoadStatusPayload =
  | { status: 'idle'; vaultGeneration: number }
  | {
      status: 'loading'
      vaultGeneration: number
      loadId: number
      modelId: string
      modelName: string
      phase: 'connecting' | 'loading' | 'cuda-jit-warmup'
      providerName?: string
    }
  | {
      status: 'ready'
      vaultGeneration: number
      loadId: number
      modelId: string
      modelName: string
    }
  | {
      status: 'error'
      vaultGeneration: number
      loadId: number
      modelId?: string
      modelName?: string
      code: string
    }
```

Der Command ist read-only und liefert `idle`, wenn keine aktive Vault oder kein
aktiver Load vorhanden ist.

## Lifecycle-Status-Event

### `model-load-status`

Bei einer Vault-Transition, beim Abbruch eines Preloads oder beim Entladen des
aktiven Modells emittiert das Backend zusätzlich einen vollständigen
`ModelLoadStatusPayload`-Snapshot. Chat und Workspace abonnieren dieses Event,
damit ein sichtbarer Ladezustand sofort auf `idle` zurückgesetzt wird und nicht
bis zum nächsten Snapshot-Aufruf bestehen bleibt. Der Snapshot wird mit
denselben `vaultGeneration`-/`loadId`-Regeln wie die Fortschritts-Events gegen
veraltete Zustände geschützt.

`vaultGeneration` erhöht sich bei jedem Vault-Open oder -Wechsel und ist von
`loadId` unabhängig: `loadId` ordnet Loads innerhalb derselben Vault-Generation,
`vaultGeneration` grenzt eine Vault-Instanz gegen ihre Vorgänger ab. Ein Snapshot
oder Event mit einer nicht mehr aktuellen `vaultGeneration` gehört zu einer
bereits verlassenen Vault und wird verworfen, unabhängig davon, ob seine
`loadId` numerisch neuer wäre als die zuletzt gesehene. Das schließt die Lücke,
die reines `loadId`-Filtern beim Übergang über den `idle`-Status hätte: Ein neu
gemounteter Client, der `idle` ohne bekannten `loadId`-Referenzwert sieht, kann
sich trotzdem an `vaultGeneration` orientieren, statt ein verspätetes Event der
vorherigen Vault fälschlich zu übernehmen.

## Bestehendes Event mit Korrelations-ID

### `model-load-progress`

Der bestehende Payload wird um `loadId` und `vaultGeneration` ergänzt:

```typescript
{
  vaultGeneration: number,
  loadId: number,
  modelId: string,
  modelName: string,
  phase: 'connecting' | 'loading' | 'cuda-jit-warmup' | 'ready',
  providerName?: string
}
```

Das Frontend verwirft Events, deren `vaultGeneration` nicht der aktuell
aktiven Vault entspricht, sowie — innerhalb derselben Generation — Events mit
einer älteren `loadId` als dem zuletzt bekannten Load.

## Neuer Fehler-Event

### `model-load-error`

```typescript
{
  vaultGeneration: number,
  loadId: number,
  modelId?: string,
  modelName?: string,
  code: string
}
```

`code` ist ein stabiler Backend-Code, kein lokalisierter UI-Text. Das Frontend
übersetzt ihn via `@nuxtjs/i18n`. Der Fehler beendet nicht den aktiven Vault-
Runtime und erlaubt eine erneute Modellwahl.

## Interner Preload

Nach erfolgreicher Veröffentlichung einer neuen aktiven Vault starten
`create_instance` und `open_instance` intern:

```rust
start_default_model_preload(app_handle, app_state, chat_state)
```

Jede aktive Vault trägt eine `vaultGeneration`, die beim Veröffentlichen der
neuen aktiven Vault erhöht wird (`create_instance`/`open_instance`, vor dem
Start des Preloads). Der Helper:

1. liest die aktuelle `vaultGeneration` und erzeugt eine neue `loadId`;
2. führt `resolve_default_model` aus — dieselbe Fallback-Reihenfolge
   (`LastActive` → `DefaultDevice` → `DefaultVault` → `FirstAvailable`) wie für
   den manuellen Fall, aber lokal eingeschränkt: Liefert eine Präferenzstufe
   eine `model_id`, die kein lokaler Kandidat ist (z. B. ein Anbieter-Modell
   aus `LastActive`), gilt diese Stufe für den Preload als nicht erfüllt und
   die Prüfung geht zur nächsten Stufe über, statt auf das Anbieter-Modell zu
   laden oder abzubrechen. `FirstAvailable` bleibt dabei bereits lokal-first
   (`loadable_local` vor `loadable_api_key_ids`);
   - **Amended 2026-11-02**: the preload loads only the last model picked on this device, when it
     is a local one that can load; otherwise nothing (spec 002 FR-014 as amended).
3. lädt den so gefundenen lokalen Kandidaten über die gemeinsame Load-Funktion;
   ein Anbieter-Kandidat wird nicht proaktiv verbunden;
4. veröffentlicht nur bei weiterhin gültiger `vaultGeneration` und `loadId` das
   Ergebnis;
5. setzt bei Fehlern den strukturierten Error-Status und lässt die Vault offen.

Liefert Schritt 2 auf jeder Präferenzstufe ausschließlich Anbieter-Kandidaten
oder gar keinen Kandidaten, startet kein Preload und der Status bleibt `idle`
(kein `error`) — das deckt sowohl "kein ladbares Modell" (FR-015) als auch "nur
ein Anbieter-Modell verfügbar" (Acceptance Scenario 7 in User Story 2) ab.

Der Helper ist kein zusätzlicher Frontend-Command. Dadurch können Workspace und
Chat nicht versehentlich mehrere konkurrierende Preloads anfordern.

## Frontend-Event-Wrapper

`useChat` ergänzt:

```typescript
async function modelLoadStatusAsync(): Promise<ModelLoadStatusPayload | null>
async function onModelLoadProgress(
  handler: (event: ModelLoadProgressEvent) => void,
): Promise<UnlistenFn>
async function onModelLoadStatus(
  handler: (event: ModelLoadStatusPayload) => void,
): Promise<UnlistenFn>
async function onModelLoadError(
  handler: (event: ModelLoadErrorEvent) => void,
): Promise<UnlistenFn>
```

Die Chat-Seite registriert Listener vor dem Snapshot-Aufruf, damit zwischen
Listener-Registrierung und Statusabfrage kein Übergang verloren geht.
