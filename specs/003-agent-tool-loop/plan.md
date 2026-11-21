# Implementation Plan: Agent Tool Loop

**Branch**: `003-agent-tool-loop` | **Date**: 2026-09-11 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/003-agent-tool-loop/spec.md`

## Summary

Der bestehende `send_message`-Ablauf (`src-tauri/src/chat/commands.rs`) ist heute linear: ein
LLM-Call, ein Stream, fertig — kein Tool-Calling, kein Retry, keine Turn/Step-Struktur. Diese
Erweiterung führt eine Turn/Step-Loop ein (ein Turn = eine oder mehrere Steps aus LLM-Request +
optionalen Tool-Aufrufen), eine Tool-Registry (MCP-Client-Tools und ein Host-CLI-Tool),
ein Freigabe-Gate mit drei Modi (Manual/Auto/Plan, analog Claude Codes Permission-Modes),
automatische Retries auf LLM-Request-Ebene bei transienten Fehlern, und eine auf die ganze Turn/Loop
ausgeweitete Cancellation. Scope ist ausdrücklich auf die `local`- und `api_key`-Provider begrenzt;
`cli_delegate` (Claude Code/Codex als Backend) ist zurückgestellt (siehe spec.md "Out of scope" und
[docs/plans/2026-09-11-agent-tool-loop-design.md](../../docs/plans/2026-09-11-agent-tool-loop-design.md)
§8-9 für die dort dokumentierten, noch offenen Compliance-Fragen).

## Technical Context

**Language/Version**: Rust 1.77.2, edition 2021 (backend, `src-tauri`), TypeScript 5 (frontend, Nuxt
4 SPA)
**Primary Dependencies**: haex-crdt (git rev `1c069ef`), Tauri 2, tokio (`sync`, `rt-multi-thread`),
reqwest 0.12 (Anthropic-Adapter), mistralrs 0.8.1 (`llm-cpu`/`llm-cuda`/`llm-metal`, optional) — Tool-
Calling über dessen native Rust-API (`RequestBuilder::set_tools`/`ChatCompletionResponse.tool_calls`,
siehe [research.md](research.md) §2), Anthropic-Tool-Use über das native Messages-API-Streaming-Format
(siehe [research.md](research.md) §1). Neu: `rmcp` 3.3.0 (offizielles Rust-MCP-SDK, gepinnt, siehe
[research.md](research.md) §3) — dieses Repo hatte zuvor keinerlei MCP-Code.
**Storage**: SQLite via SQLCipher durch haex-crdt; Erweiterung der bestehenden, sync-getrackten
`chat_messages`-Tabelle (neue Migration nach `0013_chat_messages_add_idempotency_key`) um
`tool_name`, `tool_call_id`, `tool_input`, `tool_is_error`, `tool_source` (alle nullable) — `role` ist
bereits eine ungeprüfte TEXT-Spalte, neue Rollenwerte (`tool_call`/`tool_result`) brauchen keine
Schema-Änderung. Neue Device-Preference `chat.permission_mode` über die bestehende
`preferences`-Tabelle/`PrefScope`, kein neues Storage-Konzept.
**Testing**: `cargo test --lib` / `cargo test --test <name>`; Konvention dieses Repos: Tests in
eigenen `*_tests.rs`-Dateien neben dem Modul, nie inline `#[cfg(test)] mod tests`. `pnpm typecheck`
für die Frontend-Seite der neuen Freigabe-UI.
**Target Platform**: Desktop (Linux primär, macOS/Windows mitgedacht). Kein Mobile-Scope — das
Host-CLI-Tool ist ohnehin nur unter `#[cfg(...)]`-artigem Desktop-Gate sinnvoll, analog zum
bestehenden `llm-cpu`-Feature-Split.
**Project Type**: Desktop-App (Tauri: Rust-Backend als static lib + Nuxt-4-SPA-Frontend)
**Performance Goals**: Cancellation einer laufenden Tool-Ausführung im selben Zeitfenster wie das
heutige `abort_current_generation` für reine Text-Generierung (SC-005) — kein neuer,
eigenständiger Performance-Ziel-Wert.
**Constraints**: Bestehende `parent_id`-Message-Kette bleibt einzige Quelle für History-
Rekonstruktion (kein Zweit-Log); Freigabe-Warten darf niemals still auto-entscheiden (FR-005);
Turn-Loop MUST auf eine feste Rundenzahl begrenzt sein (FR-016); haex-crdt-kompatible
Schema-Änderungen (keine CRDT-Metadatenspalten anfassen, korrekte `haex_hlc_no_sync`-Injektion, wie
bei jeder bisherigen Migration in `identity/migrations.rs`).
**Scale/Scope**: Single-Nutzer, ein aktives Modell gleichzeitig (bestehende Invariante aus
`ChatState`/`ActiveSession`), Tool-Registry-Größe klein (Host-CLI-Tool + Tools aus vom Nutzer
konfigurierten MCP-Servern).

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Bewertung gegen haex-vault Constitution (`.specify/memory/constitution.md`, hard-pinned von haex-hive,
Revision `336eaf1e`):

| Prinzip                                                 | Status | Begründung                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No Secrets in Git                                    | ✓ PASS | Kein Secret-Material in dieser Erweiterung. Vault-Credential-Handling für `cli_delegate` ist explizit außerhalb dieses Scopes (siehe Summary).                                                                                                                                                                                                                          |
| II. No Local Absolute Paths in Versioned Config         | ✓ PASS | Keine neuen versionierten Config-Dateien; MCP-Server-Konfiguration ist Laufzeit-/Vault-Zustand, kein Git-Artefakt.                                                                                                                                                                                                                                                      |
| III. Project Identity Is Device-Independent             | ✓ PASS | Neue `chat.permission_mode`-Preference nutzt die bestehende `PrefScope`/`vault_device_uuid`-Konvention unverändert.                                                                                                                                                                                                                                                     |
| IV. Cross-Repo References Pin Immutable Revisions       | ✓ PASS | `rmcp` wird per exaktem `crates.io`-Versions-Pin (`rmcp = "=3.3.0"`, per `Cargo.lock` reproduzierbar) referenziert. Prinzip IV betrifft laut Konstitutionstext "External harness content" — die `.haex-hive`/`.spaex`-Atom-Ebene, nicht gewöhnliche Cargo-Dependencies — analog zu `mistralrs`/`serde`, die ebenfalls per Versions-Pin statt Git-SHA referenziert sind. |
| V. External Sources Are Opt-in Per Project              | ✓ PASS | N/A — kein externer Harness-Content betroffen.                                                                                                                                                                                                                                                                                                                          |
| VI. Self-Modifying Instructions Are Always Review-Gated | ✓ PASS | Änderungen an `plans/001-desktop-mvp.md` (Revision der `cli_delegate`-Host-Auth-Zeile, siehe Design-Doku §8.3) laufen über PR-Review, nicht in dieser Spec selbst.                                                                                                                                                                                                      |
| VII. Relay Unavailability Never Blocks Local Work       | ✓ PASS | Tool-Loop, Freigabe-Gate und Retries sind rein lokale/Provider-API-Vorgänge, unabhängig vom Sync-Relay.                                                                                                                                                                                                                                                                 |
| VIII. No Concealment Instructions in Agent Output       | ✓ PASS | Freigabe-Anfragen und Tool-Ergebnisse sind laut FR-007 vollständig sichtbar; kein verstecktes Verhalten vorgesehen.                                                                                                                                                                                                                                                     |

**Result**: Alle Gates PASS. Kein Complexity-Tracking-Eintrag nötig.

## Project Structure

### Documentation (this feature)

```text
specs/003-agent-tool-loop/
├── plan.md                    # This file
├── spec.md                    # Feature specification (existing, from /speckit.specify + /speckit.clarify)
├── research.md                # Phase 0 output (this command)
├── data-model.md              # Phase 1 output
├── quickstart.md              # Phase 1 output
├── contracts/                 # Phase 1 output
│   └── tauri-commands.md      # Neue/geänderte Tauri-Command- und Event-Verträge
├── checklists/
│   └── requirements.md        # Spec-Quality-Checklist (aus /speckit.specify)
└── tasks.md                   # Phase 2 output (/speckit.tasks — NOT this command)
```

### Source Code (repository root)

Erweitert die bestehende Tauri-Desktop-Struktur; kein neues Top-Level-Projekt.

```text
src-tauri/src/
├── adapters/
│   ├── types.rs                  # + ToolSpec, ToolCall auf ChatRequest; ToolCalls-Variante auf StreamChunk;
│   │                              #   ChatMessage-Rollen ToolCall/ToolResult
│   ├── anthropic.rs               # + tools-Feld im Request, tool_use-Content-Block-Parsing im Stream
│   ├── local.rs                   # + Tool-Calling nur falls Phase-0-Research mistralrs 0.8.1 das freigibt,
│   │                              #   sonst: lokale Modelle bekommen leere tools-Liste (kein Fehler, nur kein Tool-Zugriff)
│   └── mod.rs                     # ProviderAdapter-Trait unverändert (stream_chat-Signatur bleibt stabil)
├── chat/
│   ├── commands.rs                # send_message wird zur Turn/Step-Loop; + respond_tool_permission-Command
│   ├── session.rs                 # ChatState: + tool_registry, + pending_tool_approvals
│   ├── tools/                     # NEU
│   │   ├── mod.rs                 # Tool-Trait (name/description/input_schema/risk_class/execute), Registry
│   │   │                          # (kein builtin.rs in diesem Feature — spec.md Assumptions erlauben
│   │   │                          #  explizit null Built-in-Tools außer dem CLI-Tool)
│   │   ├── cli.rs                 # NEU: Host-CLI-Tool (tokio::process::Command, immer Risky)
│   │   ├── mcp.rs                 # NEU: MCP-Client-Discovery + Tool-Wrapper
│   │   ├── permission.rs          # NEU: Freigabe-Gate (Manual/Auto/Plan-Auswertung, oneshot-Channel-Verwaltung)
│   │   └── *_tests.rs             # je Modul, Repo-Konvention (keine inline #[cfg(test)] mod tests)
│   └── thread_commands.rs         # unverändert
├── storage/
│   ├── chat_messages.rs           # + tool_*-Spalten, MessageRole-Erweiterung
│   └── preferences.rs             # unverändert (chat.permission_mode nutzt bestehende API)
├── identity/
│   └── migrations.rs              # + Migration 0014 (chat_messages tool_* Spalten)
└── lib.rs                         # + respond_tool_permission im invoke_handler, + ChatState-Init

src/
├── composables/
│   └── useChat.ts                 # + Tool-Call/Tool-Result-Rendering, + tool-permission-request-Event-Handling
├── components/chat/                # + PermissionPrompt-Komponente (Manual/Auto/Plan-Umschalter, Freigabe-Dialog)
└── i18n/{de,en}/*.json             # + Strings für Freigabe-Dialog, Tool-Ergebnis-Anzeige, Rundenlimit-/Retry-Meldungen
```

**Structure Decision**: Erweiterung der bestehenden Single-Tauri-App-Struktur. Neues Modul
`chat/tools/` bündelt die komplett neue Tool-Registry/Freigabe-Logik statt sie in `commands.rs`
aufzublähen; `commands.rs` selbst wird nur um die Loop-Steuerung erweitert. Keine neue
Top-Level-Struktur, kein separates Backend/Frontend-Splitting jenseits des Bestehenden.

## Complexity Tracking

_Keine Einträge — Constitution Check hat keine Verstöße ergeben._
