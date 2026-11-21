# Phase 1 Data Model: CLI Delegate Backend (Claude Code / Codex)

## Geänderte Entitäten (bestehendes Schema, keine neue Migration)

### `providers` (bestehend, `storage/providers.rs`)

Kein neues Feld, keine neue Migration nötig — `ProviderKind::CliDelegate` und `credentials:
Option<Vec<u8>>` existieren bereits als Etappe-2-Groundwork (unbenutzt seit Einführung). Dieses
Feature ist der erste Verbraucher:

| Spalte        | Bisheriges Verhalten für `cli_delegate`                                                   | Neues Verhalten                                                                                                                                                                                                                                           |
| ------------- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `credentials` | Immer `None` (Doc-Kommentar `providers.rs:13-17`)                                         | Ausgefüllt: für `adapter = "claude"` der `claude setup-token`-Output (OAuth-Token, UTF-8-Bytes); für `adapter = "codex"` der Inhalt von `auth.json` aus `codex login` (rohe Bytes). SQLCipher-verschlüsselt wie bei `api_key` (Constitution Principle I). |
| `adapter`     | `add_provider` lehnt `adapter` für `CliDelegate` explizit ab (`providers/mod.rs:112-123`) | Wird zum Vendor-Diskriminator: `"claude"` \| `"codex"` — dieselbe Spaltenrolle wie heute schon für `api_key`-Vendoren (`"anthropic"` etc.), nur jetzt auch für `cli_delegate` erlaubt.                                                                    |
| `base_url`    | Aktuell laut `add_provider`-Validierung als "cli command" gefordert                       | Bleibt so — trägt weiterhin den auszuführenden Binary-Namen/Pfad (`claude` bzw. `codex`), falls nicht im `PATH` mit dem erwarteten Namen auffindbar.                                                                                                      |

**Validierungsänderung** (`providers/mod.rs::add_provider`): die bestehende Ablehnung von `adapter`
für `CliDelegate` (Zeile ~112-123) wird durch eine Prüfung ersetzt, die `adapter ∈ {"claude",
"codex"}` verlangt — analog zur bereits vorhandenen Vendor-Validierung für `api_key`.

**Doc-Kommentar-Update**: `providers.rs:13-17`s "For local and cli_delegate providers it is None"
muss auf den neuen Zustand angepasst werden (`local` bleibt `None`, `cli_delegate` nicht mehr).

### `chat_messages` (bestehend, `tool_source`-Spalte)

Keine Migration — `tool_source` ist bereits `TEXT NULL`, freiform (`chat/tools/mod.rs:69`s eigener
Kommentar: "`mcp` or `cli` — persisted verbatim"). Neue zulässige Werte für Tool-Aktivität, die ein
Delegate-Backend selbst ausgeführt hat und die haex-vault nur nachrichtlich aufzeichnet:

| Wert                  | Bedeutung                                          |
| --------------------- | -------------------------------------------------- |
| `cli_delegate:claude` | Tool-Aufruf, den Claude Code selbst ausgeführt hat |
| `cli_delegate:codex`  | Tool-Aufruf, den Codex selbst ausgeführt hat       |

Validierungsregeln aus 003 (`storage/chat_messages.rs`) bleiben unverändert — diese Werte durchlaufen
denselben `tool_call`/`tool_result`-Zeilenmechanismus wie `mcp`/`cli` heute schon, nur mit anderem
`tool_source`-Wert. Anders als bei `mcp`/`cli` entscheidet für diese Zeilen **keine** eigene
`Decision`-Auswertung (§8.2 unten) — die Freigabe ist zum Zeitpunkt des Aufrufs bereits live erteilt
oder verweigert worden; die Zeile ist reine Aufzeichnung (FR-005), keine erneute Gate-Prüfung.

## Neue Rust-Typen (nicht persistiert, laufzeitintern)

### `DelegateVendor` (`adapters/cli_delegate/mod.rs`)

```rust
enum DelegateVendor { Claude, Codex }
```

Aus `provider.adapter` geparst (analog zu `ProviderKind::parse`), bestimmt Dispatch zwischen
`claude.rs`/`codex.rs`.

### `CliDelegateAdapter` (`adapters/cli_delegate/mod.rs`)

Implementiert `ProviderAdapter`. **Korrigiert während der Implementierung**: `list_models` liefert
nicht `Ok(vec![])`, sondern genau ein synthetisches `ProviderModel` (`remote_id = vendor.as_str()`,
z.B. `"claude"`) — läuft dadurch durch den bestehenden `do_refresh`/`replace_provider_models`-Cache-
Pfad wie ein `api_key`-Provider und bekommt eine echte, gecachte `models`-Zeile mit der üblichen
`<provider_id>:<remote_id>`-Composite-ID. Das Frontend (`stores/models.ts`) verlässt sich auf genau
diese Zeile, statt selbst eine `:delegate`-ID zu synthetisieren — einfacher als der ursprünglich hier
beschriebene Plan, da kein separater Frontend-Sonderfall für "verbunden" nötig ist, nur noch für
"nicht verbunden" (kein `providers`-Zeile vorhanden).

| Feld                     | Typ                                                            | Herkunft                                                                                                                                                    |
| ------------------------ | -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `vendor`                 | `DelegateVendor`                                               | aus `provider.adapter`                                                                                                                                      |
| `credentials`            | `Vec<u8>`                                                      | entschlüsselt aus `provider.credentials`, wie bei `AnthropicAdapter`s `api_key`-Handling heute schon                                                        |
| `binary`                 | `String`                                                       | aus `provider.base_url`, Default `"claude"`/`"codex"` falls leer                                                                                            |
| `pending_tool_approvals` | `Arc<Mutex<HashMap<Uuid, oneshot::Sender<ApprovalDecision>>>>` | **Neu benötigt**: derselbe `ChatState`-Handle, den `turn.rs` bereits für den eingebauten Tool-Loop hält (`session.rs:120`) — siehe Signatur-Änderung unten. |
| `app_handle`             | `tauri::AppHandle`                                             | zum Emittieren von `tool-permission-request` während `stream_chat` noch läuft (dieselbe Event-Konstante wie 003, `events.rs:28`)                            |

**Signatur-Änderung, `build_adapter`/`load_api_key_model`** (`providers/mod.rs`,
`chat/model_loading.rs`): `build_adapter(provider: &Provider)` bekommt für den `CliDelegate`-Zweig
zusätzlich Zugriff auf `pending_tool_approvals` und `app_handle`. Beides ist am Aufrufort bereits
vorhanden — `load_model_inner` empfängt schon `app: &AppHandle` (`model_loading.rs:132`), und
`ChatState.pending_tool_approvals` ist `Arc<Mutex<...>>`, günstig zu klonen (`session.rs:120,134`).
Kein neuer globaler Zustand, nur eine zusätzliche Parameterdurchreichung von `load_model_inner` →
`load_api_key_model` → `build_adapter` → `CliDelegateAdapter::new(...)`. Für `local`/`api_key` bleibt
die bestehende, schmalere Signatur ungenutzt-kompatibel (die zusätzlichen Parameter werden für diese
Zweige schlicht ignoriert bzw. per Overload/Default nicht gebraucht — exakte Rust-Signaturform ist
Implementierungsdetail von tasks.md, nicht dieses Dokuments).

### `claude.rs` — Prozess- und Stream-Modell

**Wichtige Korrektur (2026-09-16, während der Implementierung entdeckt)**: MCPs Stdio-Transport
bedeutet, dass `claude` den in `--mcp-config` benannten `"command"` **als eigenen Kindprozess**
startet — nicht ein In-Prozess-Objekt innerhalb des schon laufenden Tauri-Prozesses (verifiziert im
research.md-§1-Testaufbau: der Test-MCP-Server lief als eigener `node`-Prozess). Ein separater
Prozess hat keinen direkten Zugriff auf `ChatState`/`pending_tool_approvals`. Zusätzliche
Randbedingung (Betreiber-Vorgabe): keine HTTP-Server-Lösung (auch wenn `cli_delegate` ohnehin
Desktop-only ist, da Subprozess-Spawning auf Mobile generell unmöglich ist) — reine Rust-Lösung ohne
neue schwere Abhängigkeit. Codex braucht diese Brücke **nicht**: haex-vault hält dessen
`app-server --stdio`-Pipe bereits selbst (siehe `codex.rs` unten), keine zusätzliche IPC-Hop nötig.

| Typ/Funktion                                             | Beschreibung                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `spawn_claude_invocation(...)`                           | Baut `tokio::process::Command`: `claude -p <prompt> --output-format stream-json --verbose --include-partial-messages --permission-mode default --mcp-config <tmp>/mcp-config.json --permission-prompt-tool mcp__haex-approve__approve --append-system-prompt <context>`, `env(CLAUDE_CONFIG_DIR, <tmp>)`, `env(CLAUDE_CODE_OAUTH_TOKEN, <credentials>)`, `current_dir(<tmp>)` — Muster identisch zu `chat/tools/cli.rs`s bestehendem Prozess-Setup (Prozessgruppe, Timeout, Kill-on-cancel), nur mit anderem Binary/Argumenten. Vor dem Spawn: startet den Approval-Socket-Listener (siehe `approval_bridge.rs`) und schreibt `<tmp>/mcp-config.json` mit `"command"` = `std::env::current_exe()` (der eigene haex-vault-Binary-Pfad) und `"args"` = `["--internal-cli-delegate-approval-bridge", "--socket", <socket-pfad>]`. |
| NDJSON-Parser                                            | Liest `stream-json`-Zeilen (`{"type":"assistant"/"stream_event"/"result"/...}`, research.md §1 zeigt reale Beispiel-Events); `content_block_delta`/`text_delta` → `StreamChunk::Delta`; abschließende `result`-Zeile → `StreamChunk::Done`. Kein `StreamChunk::ToolCalls` (research.md §4).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `permission_mcp_server::run_bridge_process(socket_path)` | Läuft **im separaten Kindprozess** (siehe `lib.rs`-Einstiegspunkt unten). Ein rmcp-`server`+`transport-io`-basierter Stdio-MCP-Server (eigenes geerbtes stdin/stdout — das ist, womit `claude` tatsächlich spricht) mit genau einem Tool (`approve`). Dessen `tools/call`-Handler verbindet sich seinerseits als Client zum `socket_path` (Unix-Domain-Socket unter Unix, Named Pipe unter Windows — `tokio::net`, Feature `net`, kein neues Crate), schickt `{tool_name, input}` als eine JSON-Zeile, liest eine JSON-Zeile mit der Entscheidung zurück, formt daraus die MCP-Tool-Antwort.                                                                                                                                                                                                                                   |
| `lib.rs`/`main.rs`-Einstiegspunkt                        | Ganz am Anfang von `main()`, vor `tauri::Builder::default()...run()`: wenn `std::env::args().nth(1) == Some("--internal-cli-delegate-approval-bridge")`, läuft stattdessen nur `permission_mcp_server::run_bridge_process(socket_path)` und der Prozess beendet sich danach — kein Tauri-/GUI-Start. Analog zu bekannten Rust-CLI-Mustern (verstecktes internes Subcommand über den eigenen Binary-Pfad), plattformunabhängig auf allen Desktop-Targets.                                                                                                                                                                                                                                                                                                                                                                       |

### `codex.rs` — Prozess- und Session-Modell

| Typ/Funktion                  | Beschreibung                                                                                                                                                                                                                             |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `spawn_codex_app_server(...)` | `tokio::process::Command`: `codex app-server --stdio`, `env(CODEX_HOME, <tmp>)` vorbefüllt mit `auth.json` aus `credentials`, `current_dir(<tmp>)`.                                                                                      |
| JSON-RPC-Session              | Hand-gerollte Newline-JSON-RPC-Framing (kein `rmcp` — Codex spricht kein MCP hier, siehe research.md §2). Verschickt die Session-Start-/Turn-Requests, liest `ServerRequest`s.                                                           |
| Approval-Routing              | `item/commandExecution/requestApproval`/`item/fileChange/requestApproval`/`item/permissions/requestApproval` (research.md §2) → dieselbe `approval_bridge.rs`-Logik → Antwort mit `CommandExecutionApprovalDecision::{accept, decline}`. |
| Textausgabe                   | Codex' eigene Antwort-/Text-Events → `StreamChunk::Delta`/`Done`, analog zu `claude.rs`. Exaktes Event-Schema ist Implementierungsdetail von tasks.md (Codex-Live-Spike, research.md §2).                                                |

### `approval_bridge.rs` — gemeinsame Logik beider Backends, läuft im Hauptprozess

```rust
async fn request_approval(
    pending: &Mutex<HashMap<Uuid, oneshot::Sender<ApprovalDecision>>>,
    app: &AppHandle,
    thread_id: Uuid,
    tool_name: String,
    tool_input: serde_json::Value,
) -> ApprovalDecision
```

Zusätzlich (neu, siehe Korrektur oben): `start_approval_socket_listener(pending, app, thread_id) ->
(SocketPath, JoinHandle)` — legt einen frischen temporären Socket-/Pipe-Pfad an, `bind`et/`listen`et
darauf (`tokio::net::UnixListener` unter Unix, `tokio::net::windows::named_pipe` unter Windows), und
akzeptiert für die Dauer eines Delegate-Aufrufs Verbindungen vom Bridge-Kindprozess. Für jede
eingehende `{tool_name, input}`-Zeile: ruft zuerst `chat/tools/permission.rs::decide()` auf; bei
`Allow`/`Deny` antwortet sofort ohne `request_approval` (data-model.md-Edge-Case
FR-007/analyze-Finding G4); bei `Ask` ruft `request_approval` und schickt dessen Ergebnis als
JSON-Zeile zurück. Wird nach Abschluss des Delegate-Aufrufs geschlossen, der temporäre Socket-Pfad
entfernt (RAII, wie die übrigen temporären Verzeichnisse).

Erzeugt eine neue `Uuid`, einen `oneshot::channel`, registriert den Sender in `pending` (exakt wie
`turn.rs`s bestehender `Ask`-Zweig, `turn.rs:669-719`), emittiert `tool-permission-request`
(`events.rs:28`, unverändertes Payload-Schema `ToolPermissionRequestEvent`), und wartet auf die
Antwort — mit `PermissionMode`/`RiskClass` → `Decision` weiterhin aus `chat/tools/permission.rs`s
unverändertem `decide()` bestimmt, bevor `request_approval` überhaupt aufgerufen wird (bei
`Decision::Allow`/`Deny` entfällt der Live-Request ganz, identisch zum bestehenden Verhalten). Wenn
die Bridge selbst ausfällt (MCP-Server/Prozess-Crash), wird der Sender gedroppt — der wartende
Aufrufer interpretiert das fail-safe als Deny (spec.md Edge Cases).

**Wichtig**: Dies ist derselbe `pending_tool_approvals`/`respond_tool_permission`-Mechanismus wie in
003, nicht ein Duplikat. Das Frontend braucht **keine** neue UI für Delegate-Freigaben — dieselbe
`PermissionPrompt`-Komponente/derselbe `respond_tool_permission`-Command bedient beide Pfade.

## State-Transitions

Keine neuen — ein `Delegate Invocation` (spec.md Key Entities) ist aus `turn.rs`s Sicht ein einzelner
Step wie jeder andere (research.md §4); sein internes Leben (Prozess-Start → ggf. mehrere
Live-Approval-Runden → Prozess-Ende) ist intern zu `adapters/cli_delegate/` und für `turn.rs`
unsichtbar.

## Beziehungen

Unverändert — `providers`/`models`/`chat_messages` Fremdschlüssel-Struktur bleibt exakt wie 003 sie
hinterlassen hat.
