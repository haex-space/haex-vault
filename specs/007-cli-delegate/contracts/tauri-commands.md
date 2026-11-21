# Contracts: Tauri Commands & Events (CLI Delegate Backend)

Alle Kommandos sind async, laufen im Tauri-`invoke_handler` und geben `Result<T, HaexError>`
zurück. `snake_case` ↔ `camelCase` per `#[serde(rename_all = "camelCase")]`, unverändert.

## Neue Commands

### `connect_cli_delegate(args) -> ConnectCliDelegateResult`

Startet den einmaligen Verbindungs-Flow (spec.md FR-001, User Story 2). Läuft in einem frischen,
temporären Verzeichnis (`CLAUDE_CONFIG_DIR`/`CODEX_HOME` + `cwd`, research.md §3). **Die beiden
Vendoren haben unterschiedliche Mechanik** (research.md §5, live verifiziert 2026-09-16 — nicht wie
ursprünglich angenommen ein einheitlicher "Kindprozess gibt URL aus"-Flow):

- **`claude`**: `claude setup-token` läuft in einer PTY (`portable-pty` — echtes TUI, kein
  zeilenorientiertes stdout; per-`--ax-screen-reader` piped stdio liefert nachweislich 0 Bytes). Die
  OAuth-URL wird aus einer OSC-8-Hyperlink-Sequenz im PTY-Output extrahiert. Der Command selbst löst
  bereits nach dem Emittieren der URL auf, er wartet NICHT bis zum Login-Abschluss — der eigentliche
  Abschluss läuft danach im Hintergrund weiter (live verifiziert 2026-09-16/17, research.md §5-Addendum):
  Anthropics aktuelle OAuth-Seite zeigt entgegen der ursprünglichen Annahme **keinen** Code zum
  Zurückkopieren mehr an, daher liest ein Hintergrund-Task den weiterlaufenden PTY-Output selbständig
  und schließt den Flow ab, sobald der Token von selbst erscheint. `submit_cli_delegate_code` (unten)
  bleibt als Fallback bestehen, falls eine andere `claude`-CLI-Version doch einen Code anzeigt.
- **`codex`**: `codex login --device-auth` läuft über normales piped stdio (kein PTY nötig, verifiziert
  plain-text). URL **und** Einmalcode werden aus stdout extrahiert und beide in
  `delegate-connect-progress` emittiert. Kein zweiter Command nötig — der Prozess pollt selbst bis der
  Nutzer den Code auf der Webseite eingegeben hat, schreibt dann `auth.json` und beendet sich mit Exit
  0; `connect_cli_delegate` wartet intern darauf und schließt den Provider-Upsert direkt ab.

**Args**:

```typescript
{ vendor: 'claude' | 'codex', name: string }
```

**Returns**:

```typescript
{ status: 'awaiting_code', vendor: 'claude' }               // claude: Code-Eingabe aussteht
| { status: 'connected', provider: Provider }                 // codex: sofort fertig
```

**Fehler**:

- `HaexError::InvalidInput` wenn der Kindprozess (`claude setup-token`/`codex login --device-auth`)
  nicht startet (Binary fehlt) — spec.md FR-008-artige Behandlung, aber am Setup- statt am Chat-Pfad.
- Ein Fehlschlag beim Extrahieren der URL (unerwartetes CLI-Output-Format) liefert `InvalidInput` mit
  einer erklärenden `reason`; es wird keine Provider-Zeile angelegt oder verändert.

**Verhalten**: emittiert `delegate-connect-progress` (unten) während des Flows. Für `codex` läuft der
gesamte Flow innerhalb dieses einen Commands; für `claude` hält der Command den laufenden PTY-Kindprozess
in In-Memory-State (`ChatState`-artig, siehe data-model.md), bis `submit_cli_delegate_code` oder ein
Timeout/Abbruch ihn beendet. Das temporäre Verzeichnis wird garantiert entfernt (RAII/`finally`), auch
bei Fehschlag (research.md §3, spec.md FR-003).

### `submit_cli_delegate_code(code: string) -> Provider` (nur Claude)

Schließt einen laufenden, per `connect_cli_delegate(vendor: 'claude', ...)` gestarteten Flow ab: schreibt
`code` (plus Zeilenumbruch) in die PTY-stdin des wartenden `claude setup-token`-Prozesses, liest dessen
weiteren Output, extrahiert den finalen Langzeit-Token (Anthropics `sk-ant-oat`-Präfix, research.md §5 —
das exakte Erfolgsbild ist nicht live verifiziert, siehe dortige Einschränkung) und schließt den
Provider-Upsert ab.

**Returns**: die neu angelegte oder aktualisierte `Provider`-Zeile (`kind: 'cli_delegate'`,
`adapter: 'claude'`). **Upsert-Semantik**: genau eine `cli_delegate`-Provider-Zeile pro Vendor, analog
zum bestehenden `local`-Provider-Singleton-Muster (`providers/local.rs`) — ein erneuter `connect_cli_
delegate`+`submit_cli_delegate_code`-Durchlauf für denselben Vendor aktualisiert `credentials` an Ort
und Stelle statt eine zweite Zeile anzulegen (das bedient auch FR-014: reconnect ist derselbe
Command-Paar, keine separate "reconnect"-Route). Dieselbe Upsert-Semantik gilt für `codex`, dort aber
bereits am Ende von `connect_cli_delegate` selbst, da kein zweiter Command existiert.

**Fehler**: `HaexError::InvalidInput` wenn kein Flow für `claude` aussteht (z. B. doppelter Aufruf,
abgelaufener/bereits beendeter Prozess, **oder der Hintergrund-Task hat den Flow bereits automatisch
abgeschlossen** — in diesem Fall hat das Frontend das `success`-Event bereits erhalten oder erhält es
kurz danach; bei erfolgreichem Provider-Upsert ist das ein `success`-Event, bei einem Upsert-Fehler ein
`error`-Event), oder wenn der Code vom Kindprozess abgelehnt wird (falscher/abgelaufener Code) — keine
Provider-Zeile wird in diesem Fall angelegt oder verändert, der Flow bleibt offen für einen erneuten
`submit_cli_delegate_code`-Versuch.

## Geänderte Commands

### `add_provider(args) -> Provider`

**Verhalten geändert**: die bestehende Ablehnung von `adapter` für `kind: 'cli_delegate'`
(`providers/mod.rs:112-123`) entfällt zugunsten einer Prüfung `adapter ∈ {'claude', 'codex'}`
(data-model.md). In der Praxis ruft die Frontend-UI für `cli_delegate` aber `connect_cli_delegate`
auf, nicht `add_provider` direkt — `add_provider` bleibt der generische, von `connect_cli_delegate`
intern genutzte Insert-Pfad, kein zweiter öffentlich beworbener Weg, dieselbe Provider-Art anzulegen.

### `delete_provider(providerId) -> ()`

**Unverändert, wiederverwendet für "disconnect"** (spec.md FR-013): eine `cli_delegate`-Provider-Zeile
zu löschen _ist_ Disconnect. `storage::delete_provider` löscht nur die `providers`-Zeile, ohne
Cascade auf `models` — die eine gecachte Modell-Zeile pro Vendor (korrigiert während der
Implementierung, data-model.md) bleibt als verwaiste Zeile zurück, exakt wie es heute schon für
gelöschte `api_key`-Provider der Fall ist (bereits bestehendes, nicht `cli_delegate`-spezifisches
Verhalten). Harmlos in der Praxis: `modelGroups` (`stores/models.ts`) iteriert immer über die
aktuelle `providerList`, sodass eine verwaiste Zeile ohne zugehörigen Provider nie im Picker
auftaucht. Kein neuer `disconnect_cli_delegate`-Command nötig.

### `abort_current_generation() -> ()`

**Verhalten erweitert**: bricht — wie schon für den Host-CLI-Tool-Prozess in 003 — auch einen
laufenden Delegate-Subprozess (`claude -p`/`codex app-server`) hart ab, inklusive Prozessgruppen-Kill
analog zu `chat/tools/cli.rs`s bestehendem Muster (spec.md FR-011, User Story 5). Ein offener
`tool-permission-request` aus einer laufenden Delegate-Freigabe wird wie jeder andere beim Abbruch
aufgelöst (bestehendes Verhalten aus 003, unverändert).

## Neue Events (Backend → Frontend)

### `delegate-connect-progress`

Fortschritt während `connect_cli_delegate`/`submit_cli_delegate_code` läuft.

```typescript
{
  vendor: 'claude' | 'codex',
  status: 'awaiting_browser' | 'awaiting_code' | 'success' | 'error',
  url?: string,      // bei 'awaiting_browser' sowie bei 'awaiting_code' für vendor: 'claude' — extrahierte OAuth-URL
  code?: string,     // nur bei 'awaiting_browser' und vendor: 'codex' — Einmalcode zur Eingabe auf der Webseite
  message?: string,  // nur bei 'error'
}
```

`awaiting_code` ist Claude-spezifisch und markiert den Punkt, an dem das Frontend die
Code-Eingabe als Fallback anzeigen kann — für `codex` wird dieser Status nie emittiert, da der Prozess
selbst pollt statt einen eingegebenen Code entgegenzunehmen. Für `claude` folgt darauf normalerweise
direkt ein `success`- (oder `error`-)Event vom Hintergrund-Task, ganz ohne `submit_cli_delegate_code`;
das Frontend muss auf `success`/`error` hören statt sich allein auf die Rückgabe von
`submit_cli_delegate_code` zu verlassen.

## Neue Tauri-Capability

### `opener:allow-open-url`

Die im UI angezeigte OAuth-URL (`url`-Feld von `delegate-connect-progress`, oben) wird vom Frontend über
`@tauri-apps/plugin-opener`s `openUrl()` im System-Browser geöffnet statt per `<a target="_blank">` —
letzteres bleibt in der Webview ohne diese Capability wirkungslos (Standard-CSP `default-src 'self'`
blockiert externe Navigation ohnehin). Erfordert `tauri-plugin-opener` registriert in `lib.rs` sowie
`opener:allow-open-url` in `capabilities/default.json`. Betrifft beide Vendoren gleichermaßen (`codex`
zeigt seine Geräte-Code-URL genauso an), ist aber durch den `claude`-Flow hier aufgefallen.

## Nicht geänderte Contracts (zur Klarstellung)

- **Freigabe-Gate**: `tool-permission-request` / `respond_tool_permission` sind **unverändert** und
  werden von beiden Delegate-Backends genauso genutzt wie vom eingebauten Tool-Loop (research.md §1,
  data-model.md `approval_bridge.rs`) — keine separate Delegate-Freigabe-UI, kein neues Event-Paar.
- **Welches Backend geantwortet hat** (spec.md FR-005): läuft über die bestehende
  `provider_id`/`model_id`-Verknüpfung auf `chat_messages`, die für `api_key`-Provider bereits existiert
  — keine neue Spalte, kein neues Feld in den Message-Events. Ob die aktuelle Frontend-Darstellung
  daraus schon einen Backend-Namen rendert oder das noch verdrahtet werden muss, ist eine
  tasks.md-Detailfrage, kein Contract-Neuentwurf.
- **`chat-tool-call`/`chat-tool-result`** (aus 003): bleiben in Form unverändert; ein Delegate-Backend
  erzeugt dieselben Zeilenarten mit neuem `toolSource`-Wert (`cli_delegate:claude`/`cli_delegate:codex`,
  data-model.md) statt `mcp`/`cli` — reine Werterweiterung, kein Schema- oder Event-Bruch.
- **`list_providers`/`list_provider_models`/`refresh_provider_models`**: unverändert in Form.
  `refresh_provider_models` liefert für `cli_delegate` **ein** synthetisches Modell pro verbundenem
  Vendor zurück (korrigiert während der Implementierung — `CliDelegateAdapter::list_models()`, nicht
  `Ok(vec![])`, data-model.md), fließt durch denselben `do_refresh`/`replace_provider_models`-Cache-Pfad
  wie bei `api_key`-Providern — kleine Verhaltensänderung gegenüber dem heutigen `build_adapter`-
  Fehlerfall, aber keine Signatur-/Contract-Änderung.
