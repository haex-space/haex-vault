# Implementation Plan: Werkzeuge von Erweiterungen für den Agenten

**Branch**: `018-extension-agent-tools` | **Date**: 2026-10-10 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/018-extension-agent-tools/spec.md`

## Summary

Eine Erweiterung erklärt Werkzeuge im signierten Manifest und stellt sie über MCP bereit; haex-vault bietet sie
dem eingebauten Agenten an. Weil kleine lokale Modelle Werkzeuge, die sie erst suchen müssen, nicht
verlässlich nutzen (046), bietet haex-vault passende Werkzeuge selbst im ersten Schritt an: ein deterministischer
Abgleich der Nachricht mit Titel, Beschreibung und Beispielsätzen.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Manifest** (R5): Block `tools` mit Name, Titeln, Beschreibung, Schema, Wirkungsart, Beispielsätzen;
  Format-Prüfung im Crate `haex-bundle` (vault-sdk), Schema-Teilmenge in haex-vault.
- **Bestätigung** (R3, R4): Berechtigungsart `agentTool` in `extension_permissions`; Dialog, Update,
  Sync und Einstellungen aus 017 gelten mit. Wirkungsart aus dem Manifest; Schalter = `*`-Zeile „denied“.
- **Transport** (R1, R2): `rmcp`-Client über ein Kanalpaar (`sink_stream`), Port-Nachricht `haexspace:mcp`,
  Relay im Frontend, Rahmen-Sitzung als Identität; keine Client-Fähigkeiten, Anfragen der Erweiterung
  auf dieser Verbindung abgelehnt, bis der Nutzer einen Weg zum Agenten festlegen kann (047 und später).
- **Aufruf** (R8–R11): `ExtensionTool` mit `origin()`, Name `x_<slug>_<tool>`, Rahmen im Hintergrund
  (`openAppInBackground`), Zeitlimit 60 s, Ergebnis ≤ 64 KiB als Daten der Erweiterung, `byAgent` bei
  Host-Abfragen, Spalte `tool_origin`.
- **Angebot** (R6, R7): neues Modul `chat/tools/extension_offer.rs` (IDF-gewichtete Wortüberlappung, ≤ 3
  Werkzeuge, Schwelle); `find_actions` sucht Erweiterungswerkzeuge mit.
- **vault-sdk** (R12): `sdk.tools.register(name, handler)`, minimaler MCP-Server ohne Fremdabhängigkeit,
  Mitschnitt-Tests gegen `rmcp`.
- **Prüfen** (R13, R14): signierte Test-Erweiterung, Eval-Set v4 mit 30 erfundenen Erweiterungen.

## Technical Context

**Language/Version**: Rust (Edition des Crates `src-tauri`), TypeScript (strict), Vue 3.5, Nuxt 4 (SPA);
vault-sdk: TypeScript + Rust-Crate `haex-bundle` (WASM für `haex`)

**Primary Dependencies**: vorhanden — `rmcp =3.5.0` (`client`; `sink_stream` ohne neues Feature),
`tokio`, `serde_json`, `haex-bundle` (Pin wird auf das gemergte vault-sdk erhöht). vault-sdk: keine neue
Laufzeitabhängigkeit

**Storage**: `extension_permissions` (neue Art `agentTool`, keine Schemaänderung); neue Spalte
`chat_messages.tool_origin` ([data-model.md](./data-model.md))

**Testing**: `cargo test` (neu `extensions/agent_tools/*_tests.rs`, `chat/tools/extension_offer_tests.rs`,
Erweiterungen in `offer_tests.rs`, `install_tests.rs`, `manifest_tests.rs`, `eval/*_tests.rs`; Integration
`tests/extension_agent_tools.rs`; Vertrag und Mitschnitte in `extensions/agent_tools/contract_tests.rs`
(T005: `HAEX_WRITE_MCP_TRANSCRIPTS=1 cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib agent_tools`),
`pnpm check:extensions`, `check:chat-state`, `check:wm-state`, `check:agent-actions`, E2E-Szenario
`extension-agent-tools.test.ts`; vault-sdk: vitest (Mitschnitte, Schema-Prüfer, Vektoren)

**Target Platform**: Linux, macOS, Windows, Android (kompaktes Layout, R9), iOS

**Project Type**: Desktop- und Mobil-App (Tauri) plus Bibliothek (vault-sdk)

**Performance Goals**: Passendes Angebot < 20 ms bei 30 Erweiterungen × 4 Werkzeugen; Aufruf ohne offenes
Fenster ≤ 3 s langsamer als mit (SC-004)

**Constraints**: Kein Weg von einer Erweiterung zum Modell über die Werkzeug-Verbindung (FR-015); Werkzeug-Angebot ≤ 14 im
ersten Schritt, ≤ 19 nach einer Suche (R6); Manifest ohne Gleitkommazahlen (RFC 8785); Dateien < 500
Zeilen; Rust-Tests in `*_tests.rs`

**Scale/Scope**: ≥ 30 Erweiterungen mit je bis zu 32 Werkzeugen; 2 Repos (haex-vault, vault-sdk); 5 Lieferungen
(R15)

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Prinzip                                       | Bewertung                                                                                                                                                                                                                                                                                                                      |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I No secrets in git                           | Erfüllt: Die Test-Erweiterung wird wie `probe.xt` (017) mit einem aus einem öffentlichen Seed abgeleiteten Testschlüssel signiert; [ADR-0012](../../docs/adr/0012-public-test-signing-keys.md) hält fest, dass solche Testschlüssel kein Schlüsselmaterial im Sinne von Prinzip I sind (vom Operator am 2026-10-10 bestätigt). |
| II Keine lokalen Pfade                        | Erfüllt: Verweise repo-relativ; vault-sdk über Repository + SHA.                                                                                                                                                                                                                                                               |
| III Geräteunabhängige Identität               | Nicht berührt.                                                                                                                                                                                                                                                                                                                 |
| IV Pins auf unveränderliche Revisionen        | Erfüllt: `haex-bundle` per `rev` auf den Merge-Commit von Lieferung A; Mitschnitte und Vertrag liegen in haex-vault und werden in vault-sdk per SHA referenziert.                                                                                                                                                              |
| V Externe Quellen opt-in                      | Nicht berührt.                                                                                                                                                                                                                                                                                                                 |
| VI Selbstmodifizierende Anweisungen           | ADR-0004 wird per PR geändert (Lieferung E), nicht stillschweigend.                                                                                                                                                                                                                                                            |
| VII Relay blockiert nichts                    | Erfüllt: alles lokal.                                                                                                                                                                                                                                                                                                          |
| VIII Keine Verschleierung                     | Erfüllt; Ergebnisse von Erweiterungen werden dem Modell als Daten gekennzeichnet (R10).                                                                                                                                                                                                                                        |
| Phasen                                        | 017 und 032 im täglichen Einsatz, 046 gemergt; 019 erst nach Lieferung D.                                                                                                                                                                                                                                                      |
| PR-Pflicht, kein Squash, Conventional Commits | Je Lieferung ein PR (haex-vault bzw. vault-sdk).                                                                                                                                                                                                                                                                               |

Ergebnis nach Phase 1: keine Verletzung. Die Abweichung zu FR-002 (research R4) hat der Operator am
2026-10-10 angenommen; die Spec ist angepasst.

## Project Structure

### Documentation (this feature)

```text
specs/018-extension-agent-tools/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── manifest-tools.md
│   ├── mcp-port.md
│   ├── tauri-commands.md
│   └── transcripts/          # Mitschnitte (Lieferung A/C)
└── tasks.md                  # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── extensions/
│   ├── bundle/manifest.rs            # + tools (ManifestTool), supported-Flag
│   ├── permissions/model.rs          # + PermissionKind::AgentTool
│   ├── permissions/manifest_map.rs   # tools → DeclaredPermission
│   ├── bridge/permissions.rs         # + byAgent
│   ├── bridge/frames.rs              # Zähler laufender Werkzeugaufrufe
│   ├── agent_tools/                  # NEU
│   │   ├── mod.rs                    # ExtensionToolDef aus Manifest + Berechtigungen
│   │   ├── link.rs                   # McpLink: rmcp über Kanalpaar, R2-Handler
│   │   ├── contract_tests.rs         # R2 + Mitschnitte aufzeichnen (C1)
│   │   ├── test_support.rs           # In-Memory-Server für Link- und Vertragstests
│   │   ├── tool.rs                   # ExtensionTool (impl Tool), Ergebnis, Zeitlimit
│   │   ├── frame_request.rs          # Rahmen im Hintergrund anfordern (R9)
│   │   └── commands.rs               # extension_mcp_send, extension_agent_frame_ready, extension_agent_tools_set
│   └── commands/…                    # Registrierung in lib.rs
├── chat/
│   ├── tools/mod.rs                  # Tool::origin(), ToolOrigin
│   ├── tools/extension_offer.rs      # NEU: passendes Angebot (R6)
│   ├── tools/offer.rs, find_actions.rs  # Suche über Erweiterungswerkzeuge (R7)
│   ├── commands.rs                   # Angebot im ersten Schritt mit Nachrichtentext
│   ├── turn/tool_round.rs            # toolOrigin in Event und Zeile
│   ├── events.rs                     # ToolPermissionRequestEvent.tool_origin
│   └── eval/                         # v4, extension_tools.json, offered/called
├── storage/chat_messages.rs          # tool_origin
└── identity/migrations.rs            # Spalte tool_origin

src/
├── composables/useExtensionFrame.ts  # haexspace:mcp relayen
├── lib/extensions/mcpRelay.ts        # NEU, rein (Check-Skript)
├── lib/wm/layoutState.ts             # openAppInBackground
├── stores/windowManager.ts           # Rahmen-Anforderung beantworten
├── components/extensions/InstallDialog.vue        # Werkzeuge mit Wirkungsart
├── components/settings/extensions/PermissionsView.vue  # Werkzeuge, Schalter
├── components/chat/PermissionPrompt.vue, MessageList.vue  # toolOrigin anzeigen
└── lib/chat/prompts.ts               # toolSource 'haextension', ToolOrigin

scripts/e2e/scenarios/extension-agent-tools.test.ts

vault-sdk (eigenes Repo, Lieferung A):
crates/haex-bundle/src/verify.rs      # tools-Regeln, Vektoren bad-tools-*.xt
src/types.ts                          # ExtensionManifest.tools
src/api/tools.ts                      # sdk.tools (MCP-Server, Schema-Prüfer)
src/client/events.ts, init.ts, client.ts   # haexspace:mcp
src/api/__tests__/tools.test.ts       # Mitschnitte abspielen
```

**Structure Decision**: Die MCP-Verbindung zu Erweiterungen lebt unter `extensions/agent_tools/`, nicht im
Chat; der Chat kennt nur das `Tool`. So bleibt die Prüfstelle der Erweiterungen an einem Ort, und der
Brücken-Vertragstest bleibt unverändert.

## Lieferungen

| Lieferung | Repo       | Inhalt                                                                                                                                   | Voraussetzung      |
| --------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ------------------ |
| A         | vault-sdk  | Format-Prüfung `tools`, Typen, `sdk.tools`, Mitschnitt-Tests, Release                                                                    | Mitschnitte aus C1 |
| B         | haex-vault | Pin auf A, Manifest `tools`, `agentTool`, Dialog, Einstellungen                                                                          | A gemergt          |
| C         | haex-vault | C1 Mitschnitte + R2-Vertrag; C2 Relay, `ExtensionTool`, Hintergrund-Rahmen, Herkunft, Verlauf, `byAgent`                                 | B                  |
| D         | haex-vault | Passendes Angebot, Suche, Test-Erweiterung, Eval v4, E2E                                                                                 | C                  |
| E         | haex-vault | ADR-0004 ändern: Wirkungsart aus dem Manifest, Transport über den Port, rmcp 3.5, Modell nur über einen vom Nutzer festgelegten Weg (R2) | mit B              |

C1 (Mitschnitte aufzeichnen) kann vor A laufen, weil sie nur `rmcp` gegen einen In-Memory-Server brauchen.

## Complexity Tracking

Keine Verletzung der Verfassung.
