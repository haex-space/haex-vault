# Contracts: Tauri Commands (Onboarding + Preferences)

Alle Kommandos sind async, laufen im Tauri-`invoke_handler` und geben `Result<T, HolziError>` zurück. Rust-`snake_case` wird beim JSON-Payload zu `camelCase` per `#[serde(rename_all = "camelCase")]`.

## Neue Commands

### `get_pref(scope, key) -> Option<String>`

Liest genau einen Preference-Wert für den angegebenen Scope+Key.

**Args**:

```typescript
{ scope: { kind: 'vault' } | { kind: 'device'; uuid: string }, key: string }
```

**Returns**: `string | null` (Frontend), `Option<String>` (Rust). `null` bedeutet "row nicht vorhanden ODER value ist NULL" — der Wrapper faltet beide zusammen.

**Fehler**: `HolziError::InvalidInput` wenn `key` leer oder ohne `.`-Namespace.

**Beispiel**:

```typescript
const value = await invoke<string | null>('get_pref', {
  args: {
    scope: { kind: 'device', uuid: '3f7c-...' },
    key: 'chat.default_model_id',
  },
})
```

### `set_pref(scope, key, value) -> ()`

Setzt oder aktualisiert einen Preference-Eintrag.

**Args**:

```typescript
{ scope: PrefScope, key: string, value: string }
```

**Returns**: `void`.

**Fehler**:

- `HolziError::InvalidInput` wenn Key-Format ungültig.
- `HolziError::CrdtInit` wenn DB-Zugriff scheitert.

**Verhalten**:

- Sync-tracked via `haex_hlc_no_sync = current_hlc()`.
- FK-Prüfung: wenn `scope.kind === 'device'` und diese UUID nicht in `known_devices` existiert → SQLite-FK-Fehler → propagiert als `HolziError::CrdtInit` (theoretisch nie erreichbar, weil Bootstrap den Sentinel + eigenes Device sichergestellt hat).

### `clear_pref(scope, key) -> ()`

Löscht einen Preference-Eintrag (nicht Update-auf-NULL).

**Args**: `{ scope: PrefScope, key: string }`

**Returns**: `void`. Idempotent — löschen einer nicht-existierenden Row ist kein Fehler.

### `resolve_default_model() -> ResolveDefaultModelResult`

Führt die Session-Resolver-Kette (FR-014) aus und gibt zurück, welches Modell zu laden ist plus warum.

**Args**: (keine — nutzt intern die aktive Device-UUID).

**Returns**:

```typescript
{
  modelId: string | null,             // null = Onboarding-Panel zeigen (FR-014 Pos. 5)
  source: 'last_active' | 'default_device' | 'default_vault' | 'first_available' | 'none'
}
```

**Fehler**: `HolziError::CrdtInit` bei DB-Zugriffsfehlern.

**Verhalten**:

- Reine Read-Operation, KEIN Preference-Write.
- Wenn `source === 'first_available'`, ist das eine passive Wahl — der Aufrufer (Frontend) schreibt kein `last_active_model_id` auf Basis dieses Ergebnisses. Der Write erfolgt ausschließlich nach einem erfolgreichen `send_message`; ein manueller Picker-Wechsel ohne Nachricht bleibt folgenlos.

### `current_device_info() -> DeviceInfoPayload`

Liefert Metadaten über das aktuelle Gerät im Kontext des aktiven Vaults.

**Args**: (keine).

**Returns**:

```typescript
{
  installationUuid: string,     // aus <AppLocalData>/installation-id
  vaultDeviceUuid: string,      // aus known_devices[installation_uuid]
  alias: string | null,         // null = Onboarding fällig
  hostname: string | null       // OS-Hostname wenn ermittelbar, sonst null
}
```

**Fehler**: `HolziError::CrdtInit` wenn kein aktiver Vault geöffnet oder Bootstrap noch nicht gelaufen.

**Verwendung**:

- Chat-/Workspace-/Settings-Route-Guard-Middleware ruft dies und prüft `alias === null`.
- Onboarding-Wizard nutzt `hostname` als initialen Wert des Alias-Inputs; wenn `null`, füllt das Frontend den Input mit dem lokalisierten Fallback-Wert aus `$t('onboarding.alias.defaultPlaceholder')` vor. Der Fallback ist damit ein echter Eingabewert und kein bloßer Placeholder.
- Settings-Screen zeigt `alias` als Kontext-Titel (`$t('settings.header.forDevice', { alias })`).

**Grund für nullable `hostname`**: Konsistent mit FR-020 (i18n-Boundary) — Backend liefert die rohe OS-Info, Frontend entscheidet über den lokalisierten Fallback-Text. Kein lokalisierter String im Backend.

### `update_device_alias(alias) -> ()`

Setzt oder ändert den Alias des aktuellen Geräts.

**Args**: `{ alias: string }`

**Returns**: `void`.

**Fehler**: `HolziError::InvalidInput` wenn `alias` leer oder nur Whitespace.

**Verhalten**: Ruft intern `storage::known_devices::update_alias`, das bereits existiert und `haex_hlc_no_sync` korrekt injiziert.

### `catalog_recommend_tiers() -> [TierRecommendation; 3]`

Liefert drei Hardware-passende Modell-Vorschläge (Easy/Sweet/Max) für den Onboarding-Wizard.

**Args**: (keine — ruft intern `hardware::detect` und wendet den Fit-Klassifikator auf den Katalog an).

**Returns**:

```typescript
[
  { tier: 'easy', entry: CatalogEntry, fit: 'fits' | 'tight' | 'too_big' | 'unknown' },
  { tier: 'sweet', entry: CatalogEntry, fit: ... },
  { tier: 'max', entry: CatalogEntry, fit: ... }
]
```

**Fehler**: `HolziError::CatalogEntryNotFound` nur bei einem leeren oder anderweitig ungültigen Katalog, aus dem keine gültige Empfehlung erzeugt werden kann. Ein nicht-leerer gültiger Katalog ist immer erfolgreich und liefert genau drei Empfehlungen; bei weniger als drei Einträgen werden fehlende Tiers nach dem Algorithmus mit dem besten verfügbaren Kandidaten wiederverwendet (bei zwei Einträgen z. B. Max = Sweet-Fallback, bei einem Eintrag wird dieser für alle Tiers verwendet). Das Frontend behandelt nur diesen echten Fehler als "Katalog leer" und rendert gültige wiederverwendete Empfehlungen normal.

**Algorithmus**:

1. Katalog-Einträge laden, per Fit klassifizieren und einmalig aufsteigend nach `(approx_size_bytes, id)` sortieren. Diese Reihenfolge ist die totale Ordnung für alle Ties und Fallbacks.
2. Der Median ist bei `n` Einträgen der untere mittlere Eintrag an Index `(n - 1) / 2`; damit ist auch ein gerader Katalog eindeutig.
3. Easy = kleinster `Fits`-Kandidat. Fallback: kleinster Kandidat aus `Tight`, dann `Unknown`, dann `TooBig`, jeweils in der totalen Ordnung.
4. Sweet = größter `Fits`-Kandidat. Fallback: der deterministische Median aus Schritt 2.
5. Max = größter `Fits`- ODER `Tight`-Kandidat. Fallback: der bereits bestimmte Sweet-Kandidat, niemals eine unabhängige Auswahl nach `read_dir`- oder Katalog-Eingangsreihenfolge.
6. Falls ein Tier keinen eigenen Kandidaten hat, wird sein bestimmter Fallback-Kandidat wiederverwendet; dadurch bleibt die Rückgabe auch für Kataloge mit einem oder zwei Einträgen bei drei Elementen.

## Geänderte Commands

### `list_installed_models` (bestehend)

**Verhaltens-Änderung**: statt DB-Query gegen `device_downloaded_models_no_sync` × `models`, wird jetzt `<AppLocalData>/models/` per `tokio::fs::read_dir` gescannt. Ein Sub-Dir wird nur berücksichtigt, wenn es eine vollständige reguläre `.gguf`-Datei enthält; Downloads und Importe schreiben zunächst in eine temporäre Datei und veröffentlichen sie erst per atomarem Rename unter dem finalen `.gguf`-Namen. Temporäre Endungen wie `.part` oder `.tmp`, leere Verzeichnisse und unvollständige Download-Dirs werden übersprungen. Enthält ein Sub-Dir mehrere vollständige `.gguf`-Dateien, ist die lexikografisch kleinste UTF-8-Datei nach ihrem Dateinamen kanonisch. Danach wird der Slug gegen `models::get_model(&slug)` gelookupt; nur Slugs mit passender `models`-Row werden zurückgegeben. Jeder `relativePath` muss auf diese tatsächlich verfügbare kanonische Modelldatei zeigen.

**Gemeinsamer Selector**: `models::paths::canonical_model_file(app, slug)` ist die einzige Auswahlroutine für eine lokale GGUF-Datei. Sie filtert im Slug-Verzeichnis reguläre finale `.gguf`-Dateien, ignoriert temporäre Dateien, sortiert gültige UTF-8-Dateinamen lexikografisch und liefert die kleinste Datei. `list_installed_models` baut daraus `relativePath` und `sizeBytes`; `load_local_model_by_id` verwendet denselben Pfad direkt. Ein gespeicherter relativer Dateipfad wird nicht als zweite Auswahlquelle verwendet.

**Return-Signatur** (bleibt gleich):

```typescript
{
  id: string,
  name: string,
  providerId: string,
  contextWindow: number | null,
  relativePath: string,       // <slug>/<lexikografisch kleinste vollständige .gguf-Datei>
  sizeBytes: number           // aus fs::metadata
}[]
```

**Fehler**: Wenn `AppLocalData/models/` noch nicht existiert, wird `[]` zurückgegeben (`tokio::fs::read_dir` liefert `NotFound` auf einer frischen Installation). Andere Fehler beim Lesen des Verzeichnisses werden als `HolziError::Io` weitergegeben.

> **Amended 2026-11-02** (spec 006 FR-021, FR-022): `chat.last_active_model_id` is written only by
> the new `select_model {modelId}` (a pick in the model menu: load, then remember), no longer by
> `send_message`. The new `use_session_model {threadId | null}` loads the model of a session (its
> last model while it can load, otherwise the chain of `resolve_default_model`); its result gets
> `source: 'session'` for the first case.

### `send_message` (bestehend)

**Args**:

```typescript
{
  threadId: string | null,
  content: string,
  systemPrompt?: string,
  maxNewTokens?: number,
  idempotencyKey: string // nicht-leer, stabil über alle Retries desselben Sends
}
```

Der `idempotencyKey` schützt ausschließlich gegen einen Retry des Frontends **seines eigenen** `invoke()`-Aufrufs (z.B. IPC-Hänger, App kurz reconnected) — nicht gegen eine ausbleibende LLM-Antwort. Scheitert die Generierung, bleibt es dabei: der Nutzer schickt eine neue Nachricht mit neuem `idempotencyKey`; ein automatisches Wiederaufnehmen/Neustarten eines fehlgeschlagenen Streams über denselben Key gibt es nicht (bewusst vereinfacht gegenüber einer früheren Contract-Fassung).

Der Client erzeugt pro Nutzer-Sendevorgang einen neuen, opaken `idempotencyKey` und verwendet bei jedem Retry exakt denselben Key. `user_message_id` und `assistant_message_id` werden deterministisch aus dem Key abgeleitet (UUID v5) — derselbe Key liefert also unabhängig von DB-Zustand immer dieselben beiden IDs, ganz ohne zusätzlichen Cache oder eine zweite Spalte.

**Verhaltens-Änderung**: Vor dem Insert prüft `send_message` per `idempotencyKey`, ob bereits eine User-Message mit diesem Key existiert:

- **Kein Treffer**: normaler Ablauf — User-Message (inkl. `idempotencyKey`) und Thread werden mit den abgeleiteten IDs angelegt, danach `stream_chat` gestartet; scheitert der Adapter-Start, werden Message und ggf. neu angelegter Thread zurückgerollt (unverändert). Erst nach erfolgreichem Stream-Start wird `preferences[('<my_device_uuid>', 'chat.last_active_model_id')] = <current_model_id>` geschrieben (FR-009, unverändert).
- **Treffer mit identischem Thread (oder `threadId: null`) und identischem Inhalt**: kein zweiter Insert, kein zweiter `stream_chat`-Aufruf, kein Preference-Write — es wird direkt dasselbe `SendMessageResult` wie beim ersten Mal zurückgegeben.
- **Treffer mit abweichendem Thread oder Inhalt**: `HolziError::InvalidInput`.

**Kein Return-Change**.

### `load_model` (bestehend)

**Verhaltens-Änderung**: KEINE Contract-Änderung. `load_model` schreibt WEDER bei Auto-Load NOCH bei manuellem Picker-Wechsel in `chat.last_active_model_id`. Der Preference-Write erfolgt ausschließlich in `send_message` (siehe unten).

Für lokale Modelle löst `load_local_model_by_id` die Datei über den gemeinsamen `canonical_model_file`-Selector auf; ein veralteter oder nicht deterministischer Pfad aus einer Registry wird nicht verwendet.

**Grund** (Post-Clarify/Analyze-Korrektur FR-009): Ausprobier-Klicks im Dropdown sollen keine Persistenz-Wirkung haben. Der Nutzer kann bedenkenlos verschiedene Modelle laden und wieder wechseln; erst wenn er tatsächlich mit einem Modell chattet (via `send_message`) wird der Wechsel als "letzte aktive Nutzung" gemerkt.

## Neue Events

### `model-load-progress`

Emittiert vom Backend während `load_model`-Ausführung. **Strukturierter Payload — keine lokalisierten Strings** (FR-020):

```typescript
{
  modelId: string,
  modelName: string,               // display name aus models-Tabelle
  phase: 'connecting' | 'loading' | 'cuda-jit-warmup' | 'ready',
  providerName?: string            // nur bei phase === 'connecting' gesetzt
}
```

**Frontend-Übersetzung** (Beispielhaft für deutsche Locale):

| Phase             | i18n-Key                     | Beispiel (de)                                                                        |
| ----------------- | ---------------------------- | ------------------------------------------------------------------------------------ |
| `connecting`      | `chat.loading.connecting`    | "Verbinde mit {providerName}…"                                                       |
| `loading`         | `chat.loading.loading`       | "Lade {modelName}…"                                                                  |
| `cuda-jit-warmup` | `chat.loading.cudaJitWarmup` | "Optimiere GPU für erste Nutzung von {modelName}, dauert einmalig etwa 30 Sekunden…" |
| `ready`           | `chat.loading.ready`         | "Bereit" (Signal: Ladepanel ausblenden)                                              |

Englische Locale-Einträge in derselben Struktur (`src/i18n/en/chat.json`).

**CUDA-Erkennung**: Backend prüft `~/.nv/ComputeCache/` für Modell-Signatur; wenn kein Cache-Eintrag existiert UND der Build `cfg(feature = "llm-cuda")` hat, wird `cuda-jit-warmup` emittiert. Auf CPU/Metal-Builds wird stattdessen `loading` gesendet.

## Nicht-Regression

Alle bestehenden Commands außer den drei oben aufgeführten (`list_installed_models`, `send_message`, `load_model`) bleiben unverändert. Alle bestehenden Events (`chat-token`, `chat-message-complete`, `chat-message-error`, `model-download-progress`, `model-download-complete`) bleiben strukturell unverändert.
