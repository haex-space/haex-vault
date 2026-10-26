# Implementation Plan: holzi als MCP-Server für externe Agenten

**Branch**: `021-mcp-server` | **Date**: 2026-10-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/021-mcp-server/spec.md`

## Summary

holzi bekommt einen MCP-Server im App-Prozess, über den registrierte externe Agenten (Claude Code,
Codex, andere MCP-Clients, die Delegates aus 007/009) holzi bedienen: Aktionen aus dem Katalog,
Werkzeuge von haextensions, Dateien, Kontakte, Passwörter und Befehle, jeweils nur mit den Rechten,
die der Nutzer dem Agenten bei der Registrierung gibt, und mit einer eigenen Freigabestufe je
Agent. Den Umfang bestimmen die Spec und die Klärungen vom 2026-10-26:

- Der Agent darf den Registrierungsdialog selbst auslösen.
- Geheimnisse aus dem Passwortmanager nur nach Rückfrage bei jedem Abruf, auch in Auto.
- Läuft holzi nicht, bekommt der Agent nur eine Meldung; nichts startet holzi.
- Protokoll mit Ziel und Eingaben, ohne Geheimnisse.
- Ablauf einer Registrierung nach 90 Tagen ohne Verbindung.
- „Befehle ausführen“ als eigene Rechtegruppe, ab Werk aus, jeder Befehl zerstörend.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md),
Entscheidung über Weg und Identität in
[ADR-0005](../../docs/adr/0005-mcp-server-transport-and-agent-identity.md)):

- **Server** (R1): `rmcp` 3.5 mit `server`, `transport-async-rw` und später
  `transport-streamable-http-server` im App-Prozess; Modul `src-tauri/src/mcp_server/`.
- **Programm-Weg** (R1, R2): `holzi --mcp-agent <slug>` reicht stdio an einen Unix-Socket
  (Verzeichnis `0700`, UID-Prüfung) oder eine Named Pipe durch; Begrüßung mit Zugangsnachweis,
  Steuerzeilen für Nachweis und Schließen ([contracts/agent-helper.md](./contracts/agent-helper.md)).
- **HTTP-Weg** (R1, R14): Streamable HTTP auf Loopback mit OAuth 2.1, Pflicht für Android/iOS
  ([contracts/http-auth.md](./contracts/http-auth.md)).
- **Rechte** (R3): eine Funktion `mcp_server::grants` für Werkzeugliste und Prüfung jedes Aufrufs;
  Rechtegruppen auf die vorhandenen Bereiche und Datenrechte abgebildet.
- **Eine Definition** (R4): Das Frontend schickt alle agentenaufrufbaren Aktionen; Rust filtert
  für den eingebauten Agenten.
- **Aufrufer** (R5): `Caller::ExternalAgent { id }` (gibt es schon) bis in jeden Dienst; das Feld
  `caller` in `action-call-request`.
- **Rückfragen** (R6): `permission::decide` mit der Freigabestufe des Agenten; Parken auf
  `prompt_park.rs`; Dialog im Fenster, für Delegates im Chat.
- **Delegates** (R7): holzi trägt sich in die temporäre Konfiguration von Claude Code und Codex
  ein, auch im Modus „Ungated“.
- **Speicher** (R8–R10): drei Tabellen, zwei geräteeigen nach ADR-0001, das Protokoll `_no_sync`
  ([data-model.md](./data-model.md)).

## Technical Context

**Language/Version**: Rust (Tauri 2.12), TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA), Node 22.19
für Prüfskripte.

**Primary Dependencies**:

- **Vorhanden**: `rmcp` =3.5.0 (bisher `client`, `transport-child-process`), `tokio` (net, sync),
  `tokio-util` (`CancellationToken` über `VaultGate`), `sha2`, `base64`, `getrandom` 0.4, `serde_json`, `ts-rs`,
  `tauri-plugin-notification`; im Frontend Pinia, `@nuxtjs/i18n`, haex-ui (`Group`, `Row`,
  `OptionRow`, `ShadcnDialog`).
- **Neue Features einer vorhandenen Crate**: `rmcp` `server`, `transport-async-rw`, `macros`;
  in PR G zusätzlich `transport-streamable-http-server`, `auth`.
- **HTTP-Server für PR G**: `rmcp` bringt einen `tower`-Dienst. `hyper` 1 mit `hyper-util` steht
  heute nur als Test-Abhängigkeit im Manifest; PR G hebt es in die normalen Abhängigkeiten statt
  `axum` aufzunehmen (steht nicht im Lockfile).
- **Keine neue Frontend-Abhängigkeit.**

**Storage**: Migration mit der nächsten freien Nummer (bei Planung `0042_mcp_agents`): CRDT-Tabellen
`haex_mcp_agents`, `haex_mcp_agent_grants`; Tabelle `haex_mcp_agent_log_no_sync`;
`HOLZI_TRIGGER_VERSION` 28 → 29; zwei Geräteeinstellungen. `agent_file_permissions` unverändert.

**Testing**:

- **Rust, Einheit** (`*_tests.rs`): `mcp_server/{grants,hello,registry,log,expiry,endpoint}`.
- **Rust, Integration**: `src-tauri/tests/mcp_server.rs` (rmcp-Client gegen Test-Vault über
  Socket im Temp-Verzeichnis, injizierte Uhr) und `mcp_server_contract.rs` (je Rechtegruppe ein
  Fall; Leitplanken nie gelistet).
- **Prüfskript**: `pnpm check:agent-actions` plus neue Datei `scripts/check-mcp-actions.ts`
  (`check-agent-actions.ts` hat 527 Zeilen).
- **End-to-End**: Szenario `mcp-agent` mit dem Hilfsprogramm als Agent; Android-Szenario über den
  HTTP-Weg in PR G.
- **Von Hand**: [quickstart.md](./quickstart.md) M1–M10.
- **Regression**: `extension_agent_tools`, `passwords_access`, `files` und `contacts` Zugriffstests,
  `cli_delegate`-Tests, `typecheck`, `lint`, `format:check`.

**Target Platform**: Linux, macOS, Windows (beide Wege), Android und iOS (HTTP-Weg). Delegates und
„Befehle ausführen“ nur, wo `PlatformCapabilities` sie anbietet.

**Project Type**: desktop-app mit Mobil-Zielen (Nuxt-SPA + Rust-Backend in einem Tauri-Projekt).

**Performance Goals**: Werkzeugliste < 2 s nach dem Verbinden; lesender Aufruf ohne Rückfrage
höchstens 0,5 s langsamer als im eingebauten Agenten (SC-005); Widerruf < 1 s (SC-004).

**Constraints**:

- **Geheimnisse**: nie in Protokoll, Fehlertext, Ereignis, `Debug`-Ausgabe oder Nachweis-Anzeige;
  Nachweise nur gehasht gespeichert.
- **Aufrufer**: kommt nur aus dem Eingang (ADR-0005).
- **Rechteprüfung**: nur in Rust (`mcp_server::grants`), eine Implementierung.
- **Async**: Dateizugriffe des Hilfsprogramms und Dienstaufrufe über `spawn_blocking` bzw.
  `VaultGate`; kein blockierender Aufruf auf dem Executor.
- **Code-Regeln**: Dateien ≤ 500 Zeilen. `claude.rs` (653) und `codex.rs` (585) bekommen die
  holzi-Konfiguration in einer neuen Datei `cli_delegate/holzi_mcp.rs`; `lib.rs` (753) bekommt nur
  den Zweig für `--mcp-agent`, der Rest liegt in `mcp_server/helper.rs`.
- **Testcode**: in eigenen Dateien.

**Scale/Scope**: 1 neues Rust-Modul (rund 14 Dateien plus Tests), 3 Tabellen, 7 Commands,
7 Katalogaktionen, 2 Dialoge, 1 Einstellungsseite mit Detailseite, 2 Integrationstests,
1–2 e2e-Szenarien.

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die spaex-Constitution
`.spaex/constitution.md`.

| Prinzip / Vorgabe                                        | Status | Begründung                                                                                                                   |
| -------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                               | ✅     | Nachweise entstehen zur Laufzeit; Tests erzeugen sie zufällig                                                                |
| II Keine lokalen absoluten Pfade                         | ✅     | Verträge nennen Verzeichnisse symbolisch; die Anleitung setzt den Pfad zur Laufzeit ein                                      |
| III Projektidentität geräteunabhängig                    | ✅     | Nicht berührt                                                                                                                |
| IV Cross-Repo-Referenzen gepinnt                         | ✅     | Keine Cross-Repo-Referenz                                                                                                    |
| V Externe Quellen nur per Opt-in                         | ✅     | Keine neue Harness-Quelle                                                                                                    |
| VI Selbstverändernde Anweisungen review-pflichtig        | ✅     | Agenten können ihre Rechte nicht ändern (Leitplanken, FR-016); keine Änderung an Constitution oder Skills                    |
| VII Relay-Ausfall blockiert lokale Arbeit nicht          | ✅     | Alles lokal                                                                                                                  |
| VIII Keine Verheimlichung                                | ✅     | Jeder Aufruf steht im Protokoll; Rückfragen sind sichtbare Dialoge; `instructions` an Agenten enthalten keine Verheimlichung |
| Phasendisziplin                                          | ✅     | 020, 023, 032, 018, 034, 044 im Einsatz (`plans/README.md`)                                                                  |
| Workflow: speckit, PR, Conventional Commits, kein Squash | ✅     | specify → clarify (2026-10-26) → plan → tasks → implement; Worktree je PR                                                    |
| ADR bei prinzipienrelevanter Entscheidung                | ✅     | ADR-0005 (Weg und Identität), in diesem PR als „proposed“, „accepted“ mit PR B                                               |
| Test-Code in separaten Dateien                           | ✅     | `*_tests.rs`, `src-tauri/tests/`                                                                                             |
| Async nicht blockieren                                   | ✅     | Siehe Constraints                                                                                                            |
| Keine neue Crate ohne Not                                | ✅     | PR B–F: nur Features von `rmcp`; PR G nutzt `hyper` (schon im Lockfile), kein `axum`                                         |
| 500-LoC-Grenze                                           | ✅     | Siehe Constraints                                                                                                            |
| Graphify vor neuen benannten Artefakten                  | ✅     | Abfragen zu Rückfragen, Aufrufern, Rechten und Delegates gelaufen; Kandidaten unten                                          |
| `ponytail:`-Kommentar bei bewusster Vereinfachung        | ✅     | Geplant an: Ablauf nur beim Verbinden geprüft (R9); Protokoll-Bereinigung beim Schreiben (R10)                               |
| Nicht-triviale Logik hinterlässt einen Check             | ✅     | Siehe Testing                                                                                                                |

### Kandidaten (Erweitern statt neu bauen)

| #   | Neues Artefakt                         | Vorhandener Kandidat                                                                      | Entscheidung                                                                                                |
| --- | -------------------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| K1  | Parken von Registrierung und Rückfrage | `src-tauri/src/prompt_park.rs` (`PromptPark`, Frist, `cancel_all`)                        | Wiederverwenden, zwei Instanzen                                                                             |
| K2  | Entscheidung nach Freigabestufe        | `src-tauri/src/chat/tools/permission.rs` `decide(mode, risk)`                             | Wiederverwenden mit der Stufe des Agenten; `PermissionMode` bekommt `FromStr` für die Spalte                |
| K3  | Aufrufer                               | `src-tauri/src/caller.rs` `Caller::ExternalAgent`                                         | Unverändert benutzen                                                                                        |
| K4  | Passwort-Freigabe                      | `passwords::access::Grant` (034), Muster `browser_bridge/approvals.rs` `Approval → Grant` | `haex_mcp_agent_grants`-Zeile → `Grant` nach demselben Muster                                               |
| K5  | Dateirechte                            | `agent_file_permissions`, `files/agent/prompt.rs` `PermissionPrompt`                      | Unverändert mit der Agent-Id; die Anfrage aus 044 zeigt den angegebenen Namen                               |
| K6  | Werkzeuge von Erweiterungen            | `extensions/agent_tools` (`defs.rs` `confirmation()`, `registration.rs`)                  | Dieselben Tools, gefiltert je Agent; `confirmation()` bleibt die Regel der Erweiterung                      |
| K7  | Hilfsprogramm-Muster                   | `adapters/cli_delegate/permission_mcp_server.rs` (stdio + Socket, nur Unix)               | Vorbild; das neue Hilfsprogramm ist plattformübergreifend und reicht nur Bytes durch, kein gemeinsamer Code |
| K8  | Abwehr eines lokalen Ports             | `browser_bridge/origin.rs`                                                                | Muster für PR G; HTTP-Prüfung von `Host`/`Origin` als eigene kleine Datei, da anderes Protokoll             |

**Ergebnis nach Phase 1**: keine Verletzung.

## Project Structure

### Documentation (this feature)

```text
specs/021-mcp-server/
├── spec.md
├── plan.md              # diese Datei
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── mcp-surface.md
│   ├── agent-helper.md
│   ├── http-auth.md
│   └── tauri-commands.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
docs/adr/0005-mcp-server-transport-and-agent-identity.md
```

### Source Code (repository root)

```text
src-tauri/src/
├── mcp_server/
│   ├── mod.rs            # Zustand, Start/Stopp mit VaultGate
│   ├── endpoint.rs       # Unix-Socket / Named Pipe, UID-Prüfung
│   ├── hello.rs          # Begrüßung, Steuerzeilen
│   ├── helper.rs         # --mcp-agent: stdio ↔ Endpunkt, Nachweis-Datei
│   ├── server.rs         # rmcp ServerHandler: list_tools, call_tool, list_changed
│   ├── grants.rs         # Rechtegruppen → Angebot und Prüfung (R3)
│   ├── dispatch.rs       # Aufruf an ActionBridge / native / Erweiterung / run_command
│   ├── registry.rs       # haex_mcp_agents, Nachweis, Ablauf (R9)
│   ├── prompts.rs        # Registrierung und Rückfrage über PromptPark
│   ├── log.rs            # haex_mcp_agent_log_no_sync, Schwärzung (R10)
│   ├── commands.rs       # Tauri-Commands
│   ├── http.rs           # PR G: Streamable HTTP, Host/Origin
│   └── oauth.rs          # PR G: Autorisierung, Token
├── identity/migrations_mcp_agents.rs
├── adapters/cli_delegate/holzi_mcp.rs
└── chat/tools/action_bridge.rs, chat/action_commands.rs   # caller, volle Aktionsliste

src-tauri/tests/mcp_server.rs, mcp_server_contract.rs

src/
├── lib/actions/{scopes.ts,runner.ts,agentTools.ts,passwordsActions.ts}
├── lib/actions/settingsMcpAgentActions.ts
├── components/settings/agents/  # Liste, Detail, Protokoll, Anleitung
├── components/agents/{RegistrationDialog.vue,CallRequestDialog.vue}
└── stores/mcpAgents.ts

scripts/check-mcp-actions.ts
```

**Structure Decision**: ein neues Rust-Modul `mcp_server` neben `browser_bridge`, Frontend nach den
vorhandenen Ordnern für Einstellungen und Aktionen.

## Lieferung (jede ein PR, jede auslieferbar)

| PR  | Inhalt                                                                                                                                                                                                                                                                | Stories              | Berührt                                                                                           |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------- |
| A   | Spec, Klärungen, Plan, Research, Datenmodell, Verträge, Quickstart, ADR-0005 (proposed), Tasks                                                                                                                                                                        | –                    | `specs/021-mcp-server/`, `docs/adr/`, `plans/README.md`                                           |
| B   | Migration, `registry`, `endpoint`, `hello`, `helper`, `server` mit Aktionen „holzi bedienen“, Registrierungsdialog, Ein/Aus, Anleitungen, `caller` in der ActionBridge, volle Aktionsliste (R4), Rechteprüfung `operate`, Vertragstest-Grundgerüst; ADR-0005 accepted | US1, US2, US8        | Rust `mcp_server/`, `identity/`, `chat/`; Frontend Store, Dialog, Einstellungsseite (Grundgerüst) |
| C   | Freigabestufe je Agent, Rückfrage-Dialog, Systembenachrichtigung, Grenzen (R11), Protokoll mit Schwärzung                                                                                                                                                             | US3, US7 (Protokoll) | `mcp_server/{prompts,log}.rs`, Frontend Dialog und Protokollansicht                               |
| D   | Verwaltung: Detailseite, Rechte und Stufe ändern, Widerruf, Sperre aufheben, Ablauf nach 90 Tagen, `list_changed` bei Änderungen                                                                                                                                      | US7, US2             | `registry.rs`, Katalogaktionen, Einstellungsseite                                                 |
| E   | Erweiterungen, Dateien/Kontakte, Passwörter inkl. `passwords.secret.get` (always_ask), Befehle ausführen                                                                                                                                                              | US4, US5             | `grants.rs`, `dispatch.rs`, `passwordsActions.ts`, Passwortdienst, `scripts/check-mcp-actions.ts` |
| F   | Delegates: Konfiguration für Claude Code (auch Ungated) und Codex, Rückfragen im Chat, „Rechte festlegen“, Ersatz des Hinweises aus 032 FR-023                                                                                                                        | US6                  | `cli_delegate/holzi_mcp.rs`, Chat-Frontend                                                        |
| G   | HTTP-Weg mit OAuth 2.1, Abwehr (R14), Android/iOS, Android-e2e                                                                                                                                                                                                        | US1, US8 (mobil)     | `mcp_server/{http,oauth}.rs`, Plattform, e2e                                                      |

PR B bis D liefern zusammen den nutzbaren Kern (Claude Code und Codex bedienen holzi mit Rechten);
E bis G bauen darauf auf und sind untereinander unabhängig.

## Complexity Tracking

Keine Verletzung, die begründet werden müsste.
