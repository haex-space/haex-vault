# Implementation Plan: Browser-Werkzeuge für Agenten

**Branch**: `065-browser-tools` | **Date**: 2026-10-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/065-browser-tools/spec.md`

## Summary

Agenten bedienen einen Webbrowser: holzis eingebauter Agent, externe Agenten über den MCP-Zugang
aus 021 und später die Delegates. Stufe 1 ist ein Agenten-Browser, den holzi selbst startet und
steuert; Stufe 2 sind Tabs, die der Nutzer im eigenen Browser über die Erweiterung aus 049
übergibt. Den Umfang bestimmen die Spec und die Klärungen vom 2026-10-28:

- Browser-Profil verschlüsselt mit einem Schlüssel nur dieses Geräts, über Neustarts erhalten,
  nie auf andere Geräte.
- Anmeldedaten eintragen fragt einmal je Agent und Website, mit „Für diese Website merken“.
- Im Browser des Nutzers nur übergebene Tabs; ein von einer Seite geöffneter Tab nach Rückfrage.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md),
Entscheidung über Steuerung, Isolation und Profil in
[ADR-0019](../../docs/adr/0019-agent-browser-over-cdp-pipe.md)):

- **Steuerung** (R1, R2): eigener schmaler CDP-Client über `--remote-debugging-pipe`, kein Port;
  Modul `src-tauri/src/agent_browser/`.
- **Profil** (R3): zwischen zwei Läufen als `profile.zip` in einem verschlüsselten Ordner aus 048;
  während des Laufs in einem nur für den Nutzer lesbaren Arbeitsverzeichnis, das am Ende und vor
  jedem Start gelöscht wird. Anmeldedaten nur aus dem Passwortmanager (FR-002, FR-036).
- **Lesen und Bedienen** (R4): Skript in einer eigenen Welt, nummerierte Elemente, echte
  Eingaben.
- **Absenden** (R5): Einordnung vor dem Klick plus Anhalten ändernder Anfragen danach.
- **Sichtbarkeit** (R6): Markierung, Sitzungsanzeige und „Stoppen“ gehören ab Lieferung C zum ersten
  nutzbaren Browser; Eingreifen und Zurückgeben folgen in Lieferung D.
- **Anmeldedaten** (R7): Dienst `PasswordsService::headers_for_page` (aus der Bridge gezogen),
  Werte direkt ins Feld, Rückfluss gefiltert, Bildschirmfotos bis zum Sitzungsende gesperrt.
- **Rechte und Rückfragen** (R8, R9): Rechtegruppe `browser` in 021, Werkzeuge als native
  Katalogaktionen mit `confirmsItself`, Website-Prüfung vor dem Laden jedes Haupt- oder
  Frame-Dokuments. Unbekannte Frame-Herkünfte werden vor dem Laden angehalten; erst nach
  Zustimmung wird der Inhalt geladen und damit für Lesen oder Bildschirmfotos sichtbar.
- **Prompt Injection** (R10): Seiteninhalt immer gekennzeichnet; was er bewirken kann, ist durch
  die Regeln begrenzt und mit einem festen Testkorpus geprüft.

## Technical Context

<!-- prettier-ignore -->
**Language/Version**: Rust (Edition 2021, Toolchain des Repos), TypeScript 6, Vue 3.5 / Nuxt 4

**Primary Dependencies**: vorhanden: `tokio`, `serde_json`, `zip`, `url`, `rmcp` (021),
verschlüsselte Ordner aus 048 (`files::encrypted`). Für die Pipe unter Windows `windows-sys` als direkte
Abhängigkeit nur für Windows (steht schon im Lockfile über Tauri), Funktion `CreateProcessW` mit
`STARTUPINFOW.lpReserved2`. Keine neue Crate.

**Storage**: `haex_mcp_agent_grants` (021, neue Gruppe `browser`), Geräteeinstellung
`agent_browser.builtin`, neue Tabelle `haex_agent_browser_no_sync`, Profil als `profile.zip` im
verschlüsselten Ordner unter `<AppLocalData>/agent-browser/<Instanz>/` (data-model.md).

**Testing**: `cargo test` (Einheiten in `*_tests.rs`), Integrationstests in `src-tauri/tests/`
gegen einen echten Chromium mit einem lokalen Testserver (`agent_browser_*.rs`, nur wo ein Browser
gefunden wird; CI installiert Chromium), Prüfskripte (`check:agent-actions`, `check:mcp-actions`),
e2e `scripts/e2e/scenarios/agent-browser.test.ts`.

**Target Platform**: Linux, macOS, Windows für den Agenten-Browser; Android/iOS ohne (R13).

**Project Type**: Desktop-App (Tauri 2) mit Rust-Kern und Nuxt-Oberfläche.

**Performance Goals**: „Stoppen“ < 1 s (SC-004); Seite lesen einer üblichen Seite < 1 s nach dem
Laden; Start des Agenten-Browsers mit Profil < 5 s.

**Constraints**: kein Port für die Steuerung (FR-004); kein Klartext des Profils außerhalb des
Arbeitsverzeichnisses; kein Geheimnis in Werkzeugergebnissen, Bildern oder Protokoll; 500 Zeilen je
Datei; keine blockierenden Aufrufe im Async-Teil (Archiv packen und verschlüsseln in
`spawn_blocking`).

**Scale/Scope**: 1 Agenten-Browser je Vault-Sitzung, bis 5 Tabs je Agent, wenige Agenten
gleichzeitig.

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` und die spaex-Constitution
`.spaex/constitution.md`.

| Prinzip / Vorgabe                                 | Status | Begründung                                                                                 |
| ------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------ |
| I Keine Geheimnisse in Git                        | ✅     | Profilschlüssel zur Laufzeit; Testserver-Zertifikat wird im Test erzeugt                   |
| II Keine lokalen absoluten Pfade                  | ✅     | Browser-Pfade als Suchliste und Umgebungsvariablen                                         |
| III Projektidentität geräteunabhängig             | ✅     | Nicht berührt                                                                              |
| IV Cross-Repo-Referenzen gepinnt                  | ✅     | Stufe 2 nennt die Erweiterung in haextension; die Version wird in Lieferung F gepinnt      |
| V Externe Quellen nur per Opt-in                  | ✅     | Download von „Chrome for Testing“ nur nach Zustimmung, mit Prüfsumme                       |
| VI Selbstverändernde Anweisungen review-pflichtig | ✅     | Seiten können keine Rechte ändern (R10); Leitplanken-Aktionen nicht für Agenten            |
| VII Relay-Ausfall blockiert lokale Arbeit nicht   | ✅     | Alles lokal                                                                                |
| VIII Keine Verheimlichung                         | ✅     | Markierung, Anzeige in holzi, Protokoll jedes Aufrufs                                      |
| Phasendisziplin                                   | ✅     | 021 B–D gemergt; Lieferung B übernimmt die nötigen Teile aus 021 PR E                      |
| Workflow: speckit, PR, Conventional Commits       | ✅     | specify → Klärung (2026-10-28) → plan → tasks; Worktree je PR                              |
| ADR bei prinzipienrelevanter Entscheidung         | ✅     | ADR-0019 (Steuerung, Isolation, Profil) als „proposed“, „accepted“ mit Lieferung C         |
| Test-Code in separaten Dateien                    | ✅     | `*_tests.rs`, `src-tauri/tests/`                                                           |
| Async nicht blockieren                            | ✅     | Siehe Constraints                                                                          |
| Keine neue Crate ohne Not                         | ✅     | Eigener CDP-Client statt `chromiumoxide` (R1)                                              |
| 500-LoC-Grenze                                    | ✅     | Ein Modul je CDP-Domäne und je Regel (Struktur unten)                                      |
| Graphify vor neuen benannten Artefakten           | ✅     | Kandidaten unten                                                                           |
| `ponytail:`-Kommentar bei bewusster Vereinfachung | ✅     | Geplant an: Wortliste für Absenden (R5), gesperrte Bildschirmfotos nach dem Eintragen (R7) |
| Nicht-triviale Logik hinterlässt einen Check      | ✅     | Injektionskorpus (R10), Absenden-Testseiten (SC-003), Prüfskripte                          |
| Jede Plattform gleich (Memory „every OS“)         | ✅     | Pipe auch unter Windows, eigener Windows-Test; Android/iOS mit Gegenfall                   |

### Kandidaten (Erweitern statt neu bauen)

| #   | Neues Artefakt                              | Vorhandener Kandidat                                                     | Entscheidung                                 |
| --- | ------------------------------------------- | ------------------------------------------------------------------------ | -------------------------------------------- |
| K1  | Rückfragen des Browsers                     | `src-tauri/src/prompt_park.rs`                                           | Wiederverwenden, eigene Instanz              |
| K2  | Rechte der Agenten                          | `mcp_server::grants` (`Rights`, `covers`, `validate`)                    | Um Gruppe `browser` erweitern                |
| K3  | Werkzeuge ohne Fenster                      | `chat/tools/native_action.rs` (ADR 0011)                                 | Erweitern um `NativeCall { caller, mode }`   |
| K4  | Eigene Rückfrage statt allgemeiner Freigabe | `confirmsItself` (060), `permission::decide_for_tool`                    | Wiederverwenden; im MCP-Zugang nachziehen    |
| K5  | Passwort ↔ Website                          | `browser_bridge/methods/items.rs` Filter über `passwords::site::site_of` | In den Passwortdienst ziehen (ponytail dort) |
| K6  | Verschlüsselung                             | `files/encrypted` (048, `put_file`, `EncryptedFile`)                     | Wiederverwenden: Profil als Datei darin      |
| K7  | Protokoll                                   | `mcp_server::agent_log`, `redact.rs`                                     | Wiederverwenden; zwei neue Ergebnisse        |
| K8  | Aufrufdialog                                | `components/agents/CallRequestDialog.vue`, `stores/mcpAgents.ts`         | Muster; eigener Dialog mit Arten             |
| K9  | Plattformfähigkeiten                        | `platform::capabilities()`                                               | Neues Feld `agent_browser`                   |
| K10 | Erweiterung im Browser                      | `browser_bridge` (049, Protokoll 3)                                      | Stufe 2 als neue Nachrichten                 |

**Ergebnis nach Phase 1**: keine Verletzung.

## Project Structure

### Documentation (this feature)

```text
specs/065-browser-tools/
├── spec.md
├── plan.md              # diese Datei
├── research.md          # R1–R15
├── data-model.md
├── quickstart.md        # M1–M9
├── contracts/
│   ├── browser-tools.md
│   ├── agent-browser.md
│   ├── tauri-commands.md
│   └── extension-tabs.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── agent_browser/
│   ├── mod.rs            # AgentBrowser (Managed State), Start bei Bedarf, Ende, Leerlauf
│   ├── locate.rs         # Browser finden, Version prüfen (contracts/agent-browser.md)
│   ├── process.rs        # Start mit Pipe (Unix), Ende
│   ├── process_windows.rs# Pipe über lpReserved2
│   ├── cdp/
│   │   ├── mod.rs        # Verbindung, ids, Sitzungen, Ereignisse
│   │   ├── target.rs  page.rs  runtime.rs  input.rs  fetch.rs  browser.rs  dom.rs
│   ├── profile.rs        # Arbeitsverzeichnis, Packen, Entpacken
│   ├── profile_store.rs  # profile.zip im verschlüsselten Ordner (048)
│   ├── tab.rs            # Agenten-Tab: eigene Welt, Elementnummern, gefüllte Felder
│   ├── page_script.js    # Leseskript + Markierung (eingebunden mit include_str!)
│   ├── read.rs           # Seite lesen, kürzen, kennzeichnen (R4, R10, R12)
│   ├── act.rs            # Klick, Tippen, Auswahl, Scrollen (R4)
│   ├── submit.rs         # Absenden erkennen, Stufe 1 und 2 (R5)
│   ├── sites.rs          # Website, Prüfung vor dem Laden, Sperren (R9)
│   ├── session.rs        # Browser-Sitzung, Pause, Stopp (R6)
│   ├── prompts.rs        # Browser-Rückfragen (PromptPark)
│   ├── credentials.rs    # Eintragen, Rückfluss-Filter (R7)
│   ├── downloads.rs      # Downloads, Hochladen (R11)
│   ├── executor.rs       # NativeExecutor für browser.*
│   ├── commands.rs       # Tauri-Commands
│   └── *_tests.rs
├── chat/tools/native_action.rs   # NativeCall { caller, mode } (Lieferung B)
├── mcp_server/grants.rs          # Gruppe browser (Lieferung B)
├── mcp_server/dispatch.rs        # native Aktionen direkt, confirmsItself (Lieferung B)
├── passwords/service/by_site.rs  # headers_for_page (Lieferung B)
└── identity/migrations_agent_browser.rs  # 0047, haex_agent_browser_no_sync
src-tauri/tests/
├── agent_browser_read.rs  agent_browser_act.rs  agent_browser_profile.rs  agent_browser_injection.rs
└── fixtures/agent_browser/{site,injection}/
src/
├── lib/actions/browserActions.ts           # browser.* (native)
├── lib/actions/settingsAgentBrowserActions.ts
├── components/agentBrowser/PromptDialog.vue  SessionsBar.vue
├── components/settings/agentBrowser/AgentBrowserView.vue   # KI → Browser
└── components/agents/BrowserRightsFields.vue                # Rechtegruppe Browser je Agent
scripts/e2e/scenarios/agent-browser.test.ts
```

**Structure Decision**: Ein neues Rust-Modul `agent_browser`, Erweiterungen in `mcp_server`,
`chat/tools` und `passwords`; Oberfläche in eigenen Komponentenordnern. Stufe 2 ergänzt
`browser_bridge` und die Erweiterung in haextension.

## Lieferungen

Siehe research.md R15: A (dieser PR, Dokumente), B (Grundlage in 021 und Passwörtern), C
(Agenten-Browser, Lesen, sichtbare Sitzung und Stoppen, US1/US3), D (Bedienen, Absenden und
Eingreifen, US2/US3), E (Anmelden, Downloads, US4/US5), F (Browser des Nutzers, US6, mit Erweiterung).

## Complexity Tracking

Keine Verletzung, die begründet werden müsste.
