# Research: holzi als MCP-Server für externe Agenten

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-10-26

Jeder Abschnitt nennt Entscheidung, Begründung und verworfene Alternativen. Pfade sind
repo-relativ.

## R1 Transport: Hilfsprogramm über lokale IPC, dazu HTTP für Mobilgeräte

**Entscheidung**: Zwei Wege zu demselben Server im App-Prozess.

1. **Programm-Weg (Desktop)**: Der Agent startet `holzi --mcp-agent <slug> [--vault <id>]` (dieselbe
   ausführbare Datei, wie heute `--internal-cli-delegate-approval-bridge`). Dieses Hilfsprogramm
   reicht Bytes zwischen stdin/stdout und einem lokalen IPC-Endpunkt der laufenden App durch und
   spricht selbst kein MCP. Vor dem ersten MCP-Byte schickt es eine Begrüßung (Slug,
   Zugangsnachweis falls vorhanden, Protokollversion des Hilfsprogramms) und bekommt eine Antwort
   (angenommen, wartet auf Registrierung, Nachweis neu ausgegeben, abgewiesen mit Grund).
   - Linux/macOS: Unix-Socket in einem Verzeichnis mit Rechten `0700` (Linux
     `$XDG_RUNTIME_DIR/holzi/`, sonst das Temp-Verzeichnis des Benutzers; Pfadlänge ≤ 104 Byte
     wegen macOS), Name `mcp-<vault-id>.sock`. Zusätzlich prüft die App die UID der Gegenseite
     (`SO_PEERCRED` / `getpeereid`).
   - Windows: Named Pipe `\\.\pipe\holzi-mcp-<vault-id>` mit `first_pipe_instance(true)` gegen
     Hijacking und der Standard-DACL des Benutzers.
   - Der Server im App-Prozess ist `rmcp` mit Feature `server` und `transport-async-rw` auf dem
     IPC-Strom.
2. **HTTP-Weg (alle Plattformen, Pflicht auf Android/iOS)**: Streamable HTTP auf `127.0.0.1`/`[::1]`
   mit `rmcp` `transport-streamable-http-server`, Anmeldung nach der MCP-Autorisierung (OAuth 2.1,
   holzi ist Autorisierungs- und Ressourcenserver, dynamische Client-Registrierung). Der Schritt
   „Autorisieren“ ist der Registrierungsdialog im holzi-Fenster; die Autorisierungsseite im
   Browser sagt nur „In holzi bestätigen“ und leitet nach der Entscheidung zurück.

**Begründung**: Claude Code und Codex starten MCP-Server als Programm, ohne Anmeldung; ein
Unix-Socket oder eine Named Pipe ist für Webseiten unerreichbar, damit entfällt die ganze Klasse
der Angriffe aus US8 auf diesem Weg. Auf Android und iOS kann kein anderes Programm die holzi-Datei
starten; dort geht nur Loopback-HTTP, und dafür sieht MCP OAuth vor (Spec FR-003). Ein Server im
App-Prozess hält eine Implementierung der Rechte (ADR-0005).

**Verworfen**:

- Nur HTTP mit festem Bearer-Token in der Agent-Konfiguration: Ein vom Agenten ausgelöster Dialog
  (Klärung FR-006) ist damit nicht möglich, der Nutzer müsste vorher einen Nachweis kopieren.
- Ein eigener MCP-Server im Hilfsprogramm, der die App fernsteuert: zweite Implementierung von
  Werkzeugliste und Prüfungen.
- Loopback-TCP mit eigenem Protokoll für das Hilfsprogramm: für Webseiten erreichbar, braucht
  dieselben Abwehrmaßnahmen wie HTTP, ohne Gewinn gegenüber Socket/Pipe.

## R2 Registrierung, Zugangsnachweis und Warten

**Entscheidung**:

- Zugangsnachweis: 32 Zufallsbytes, Base64url; die App speichert nur `SHA-256`. Programm-Weg: Das
  Hilfsprogramm legt den Nachweis in `<Konfigurationsverzeichnis von holzi>/mcp-agents/<vault-id>/<slug>.token`
  (Rechte `0600`, unter Windows im Benutzerprofil). HTTP-Weg: OAuth-Access-Token (1 h) und
  Refresh-Token; der Refresh-Token ist der Zugangsnachweis im Sinn der Spec.
- Unbekannter Agent: Die App beendet `initialize` sofort mit leerer Werkzeugliste und einem
  `instructions`-Text „Warte auf Registrierung in holzi“; nach der Zustimmung schickt sie
  `notifications/tools/list_changed` (Spec US1 Szenario 1–2). Der Registrierungsdialog läuft nach
  5 Minuten mit Ablehnung ab.
- Angegebener Name: `clientInfo.name` aus `initialize`, gekürzt auf 64 Zeichen, nie geprüft;
  der Slug aus der Anleitung dient nur als Dateiname des Nachweises.
- Höchstens ein offener Dialog je Slug und Gegenstelle, höchstens 3 gleichzeitig (FR-011).

**Begründung**: `initialize` warten zu lassen, bis der Nutzer entscheidet, scheitert an den
Start-Zeitgrenzen der Clients (Claude Code wartet standardmäßig rund 30 s auf einen MCP-Server).
`tools/list_changed` beherrschen Claude Code und rmcp-Clients; ein Client ohne diese Benachrichtigung
sieht die Werkzeuge nach erneutem Verbinden.

**Verworfen**: Einmalcode, den der Nutzer abtippt (Klärung FR-006 wählt den Dialog); Nachweis
in der Agent-Konfiguration (der Agent kann sie nicht selbst schreiben).

## R3 Rechte: Rechtegruppen auf Bereiche und Datenrechte abbilden

**Entscheidung**: Eine Funktion `mcp_server::grants::offer(agent, definitions) -> Vec<Offer>`
entscheidet aus den Rechten, welche Werkzeuge ein Agent sieht, und dieselbe Funktion prüft jeden
Aufruf (FR-014).

| Rechtegruppe      | Bereiche des Katalogs (`src/lib/actions/scopes.ts`)                                                    | Zusätzliche Prüfung im Dienst                                                          |
| ----------------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| holzi bedienen    | `wm.*`, `chat.*`, `settings.read`, `settings.device`, `settings.models`; je Bereich keine/lesen/ändern | –                                                                                      |
| Dateien           | `files.read`, `files.write`, `files.shares`, `contacts.read`, `contacts.write`                         | `agent_file_permissions` (044/051/056) mit der Kennung des Agenten                     |
| Passwörter        | `passwords.read`, neu `passwords.secret`                                                               | `passwords::access` mit der `Grant` des Agenten (034 FR-028–FR-030)                    |
| haextensions      | Werkzeuge `x_<slug>_<tool>` je freigegebener Erweiterung                                               | Einstellung des Werkzeugs in der Erweiterung (018 `confirmation()`), strengere gewinnt |
| Befehle ausführen | `run_command` (Spec 003)                                                                               | `PlatformCapabilities.command_tool`; immer `RiskClass::Risky`                          |
| `guardrails`      | nie                                                                                                    | –                                                                                      |

Die Werkzeuge einer Datengruppe erscheinen, sobald der Agent in ihr irgendeine Berechtigung hat;
die Reichweite prüft der Dienst wie bisher. Eine spätere Rechtegruppe (Browser) kommt als neuer
Eintrag dieser Tabelle hinzu und ist für bestehende Agenten aus (FR-008).

**Begründung**: Bereiche und Datenrechte existieren schon und werden nicht verdoppelt
(one-implementation-per-rule). Die Spec-Gruppe „Dateien“ umfasst Kontakte, weil 044/051 beide in
`agent_file_permissions` (`AgentFileKind::Contacts`) führen.

**Verworfen**: Rechte je einzelner Aktion (zu fein für den Dialog); eine eigene Kontaktgruppe (die
Datenrechte liegen bereits in derselben Tabelle).

## R4 Eine Definition je Werkzeug

**Entscheidung**: Das Frontend schickt mit `set_agent_actions` künftig alle Aktionen mit
`agentCallable: true`, mit dem Feld `builtinAgentCallable`. Rust registriert für den eingebauten
Agenten nur die mit `builtinAgentCallable !== false` (heute filtert das Frontend) und hält die volle
Liste für den MCP-Server. Werkzeuge von Erweiterungen kommen aus derselben `ToolRegistry`-Quelle
wie für den eingebauten Agenten (`extensions/agent_tools`), gefiltert je Agent.

**Begründung**: ADR-0006 verlangt eine Definition und zwei Eingänge; `scripts/check-agent-actions.ts`
prüft schon, dass genau drei Chat-Aktionen dem eingebauten Agenten fehlen und für `EXTERNAL` laufen.

**Verworfen**: Ein zweiter Katalog in Rust; die MCP-Werkzeugliste aus dem Frontend pro Aufruf
abfragen (braucht für `tools/list` eine laufende Webview ohne Not).

## R5 Aufrufer bis in jeden Dienst

**Entscheidung**:

- `action-call-request` bekommt das Feld `caller` (`{kind:'externalAgent', agentId}`); `runAction`
  läuft damit. Das Frontend prüft weiter nur `agentCallable`/Leitplanken; die Rechteprüfung liegt
  allein in Rust vor dem Senden (R3).
- Native Aktionen (ADR-0011) und Dienste bekommen `Caller::ExternalAgent { id }` (`src-tauri/src/caller.rs`
  hat ihn schon; `files/access.rs`, `contacts/access.rs`, `passwords/access.rs` behandeln ihn schon).
- Werkzeuge von Erweiterungen bekommen den Aufrufer im Aufrufkontext; der Frame sieht den Namen des
  Agenten nicht.

**Begründung**: Der Aufrufer kommt aus dem Eingang, nie aus der Anfrage (Kommentar in `caller.rs`).

## R6 Freigabestufe und Rückfragen

**Entscheidung**:

- `chat::tools::permission::decide(mode, risk)` wird mit der Freigabestufe des Agenten aufgerufen
  (nicht mit `chat.permission_mode`). `read`→`Safe`, `write`→`Change`, `destructive`→`Risky`
  (ADR-0006). `run_command` ist `Risky` (Auto fragt, Plan lehnt ab; Spec FR-012).
  `passwords.secret.get` ist lesend (`Safe`) und `always_ask`: Es fragt in jeder Stufe, auch in
  Auto und Plan (Spec FR-019, FR-021); `Tool::always_ask` aus 018 macht genau das.
- Rückfragen parken auf `src-tauri/src/prompt_park.rs` (Frist 5 Minuten, Abbruch über das Token
  der Vault) und gehen als Ereignis `mcp-agent-call-request` an einen Dialog im Fenster. Ist das
  Fenster nicht im Vordergrund, zeigt `tauri-plugin-notification` einen Hinweis.
- Für einen Delegate im Chat geht dieselbe Rückfrage in die Freigabe-Zeile des Chats
  (vorhandene Delegate-Freigaben aus 009), nicht in den Dialog.

**Verworfen**: Die globale Chat-Freigabestufe für alle Agenten (Spec FR-021 verlangt sie je Agent).

## R7 Delegates bekommen den Zugang

**Entscheidung**:

- Claude: `mcp-config.json` im temporären `CLAUDE_CONFIG_DIR` bekommt neben `holzi-approve` den
  Server `holzi` mit `--mcp-agent claude-code-delegate --delegate-token <nachweis>`; auch im Modus
  „Ungated“ wird die Konfiguration übergeben (heute dort ausgelassen). `bypassPermissions` betrifft
  nur Claudes eigene Freigabe; holzi prüft seine Werkzeuge selbst.
- Codex: `config.toml` im temporären `CODEX_HOME` mit `[mcp_servers.holzi]` (command, args).
- Der Nachweis eines Delegates bleibt in der App (kein Token-File) und wird je Start als Argument
  übergeben; die Gegenstelle ist das von holzi gestartete Kind. Umsetzung (PR F): Jeder Start bekommt
  einen eigenen Nachweis, der nur im Speicher des Prozesses liegt, Art und Chat-Thread nennt und mit
  dem Start endet (`mcp_server/delegates.rs`). Die Zeile des Delegates hat keinen `token_hash`.
- Claude fragt bei holzi-Werkzeugen nicht selbst nach (`--allowedTools mcp__holzi`), sonst gäbe es
  zwei Rückfragen; holzi fragt nach der Freigabestufe des Delegates.

**Begründung**: holzi startet die Delegates selbst und kennt sie; der Nutzer richtet nichts ein
(SC-006).

## R8 Speicherung

**Entscheidung**: Nach ADR-0001 geräteeigene CRDT-Tabellen mit Fremdschlüssel auf
`known_devices(vault_device_uuid)`, wie `haex_browser_clients`: `haex_mcp_agents`,
`haex_mcp_agent_grants`. Das Protokoll in `haex_mcp_agent_log_no_sync` (wie `extension_logs_no_sync`,
mit `vault_device_uuid`). Dateirechte externer Agenten bleiben in `agent_file_permissions` (dort
schon mit `agent_id`). Migration mit der nächsten freien Nummer; `HOLZI_TRIGGER_VERSION` 28 → 29.
Einstellung `mcp.enabled` als Geräteeinstellung.

**Begründung**: ADR-0001 erklärt, warum geräteeigene Daten nicht `_no_sync` sein sollen (Kopie der
Datei); für das Protokoll ist „wandert nicht zu anderen Geräten“ gewollt und die Größe begrenzt.

## R9 Ablauf nach 90 Tagen

**Entscheidung**: Beim Verbinden prüft die App `last_seen_at`; älter als 90 Tage → Zustand
`expired`, Nachweis ungültig, Dialog mit den alten Rechten vorausgefüllt. Kein Hintergrund-Job.

## R10 Protokoll ohne Geheimnisse

**Entscheidung**: Ziel und Eingaben werden aus dem Eingabeschema geschrieben; Felder mit
`"x-holzi-secret": true` im Schema werden durch `•••` ersetzt, Werte über 200 Zeichen gekürzt.
Zusätzlich werden credential-tragende Werte in rohen Befehlen und Argumentlisten von `run_command`
vor dem Persistieren und vor einer Rückfrage redigiert, auch wenn das Schemafeld selbst kein
`x-holzi-secret` trägt; ein kurzer Wert wie `--token=abc` darf nie im Protokoll erscheinen.
`check-agent-actions.ts` prüft schon, dass kein Schema ein Geheimnis benennt; neu prüft es, dass
`passwords.secret.get` keine geheimen Eingaben hat. Ergebnisse werden nie geschrieben. Höchstens
10 000 Einträge je Agent und Gerät; ältere als 30 Tage werden beim Schreiben und beim Lesen
entfernt, sodass sie auch ohne weitere Werkzeugaufrufe nicht sichtbar bleiben. Der Log-Test deckt
ein Secret innerhalb eines Command-Arguments ab.

## R11 Grenzen

**Entscheidung**: höchstens 4 gleichzeitige Aufrufe je Agent (weitere warten bis 30 s, dann
`too_many_calls`), Ergebnisgröße wie für den eingebauten Agenten, Aktionen eines Agenten laufen
nacheinander über die Sperre der `ActionBridge`.

## R12 Plattformen

**Entscheidung**: Programm-Weg auf Linux, macOS, Windows; HTTP-Weg überall, auf Android/iOS der
einzige. „Befehle ausführen“ nur, wo `command_tool` gilt; Delegates nur, wo `cli_delegates` gilt.
Auf iOS ist der Zugang nur im Vordergrund erreichbar (Spec-Annahme). Linux-Tests decken den
Windows-Pfad über die plattformneutrale Begrüßung und Rechteprüfung; die Pipe selbst hat einen
eigenen Windows-Test im CI-Build.

## R13 Geheimnis eines Passworts

**Entscheidung**: neue Katalogaktion `passwords.secret.get` (Bereich `passwords.secret`, Wirkung
`read`, `builtinAgentCallable: false`, `runner: 'native'`) mit Eingabe `entryId` und `field`
(`password`, `totp`, `custom:<name>`). Ausführung über den Passwortdienst mit der `Grant` des
Agenten; `always_ask`. Schreibende Passwortaktionen für Agenten gibt es weiterhin nicht; die Art
„Lesen und Schreiben“ wird wirksam, sobald es sie gibt.

## R14 Sicherheit des HTTP-Wegs

**Entscheidung**: nur Loopback; `Host` muss `127.0.0.1:<port>`, `[::1]:<port>` oder
`localhost:<port>` sein; ein vorhandener `Origin`-Header führt zu 403 außer für die eigene
Autorisierungsseite; Port zufällig beim ersten Start, geräteeigen gespeichert, in der Anleitung
angezeigt. Grenzen: 1 MiB je Anfrage, 16 Sitzungen.

**Begründung**: Die MCP-Spezifikation verlangt die Prüfung von `Origin` gegen DNS-Rebinding; das
Muster folgt 049.

## R15 Lebenszyklus

**Entscheidung**: Start beim Öffnen der Vault, wenn `mcp.enabled`; Stopp über das Token des
`VaultGate` beim Schließen oder Sperren oder beim Ausschalten. Laufende Aufrufe enden mit
`unavailable`, Rückfragen über `PromptPark::cancel_all`. Läuft holzi nicht, findet das Hilfsprogramm
keinen Endpunkt und schreibt eine MCP-Fehlerantwort „holzi ist nicht offen“ auf `initialize`,
dann endet es (Klärung FR-002).

## R16 Oberfläche

**Entscheidung**: Seite `ai.agents` in der Kategorie „KI“, Gruppe „Agenten“, mit Ein/Aus,
Anleitungen, Liste, Detailseite je Agent (Rechte, Freigabestufe, Protokoll, Widerruf). Dialoge:
Registrierung, Rückfrage. Alle Änderungen über neue Katalogaktionen im Bereich `guardrails`
(`settings.agents.mcp.*`), gespeichert mit der Auswahl (023 FR-021). Bauteile: `Group`, `Row`,
`OptionRow` aus haex-ui.

## R17 Tests

**Entscheidung**:

- Rust Einheit (`*_tests.rs`): `grants` (alle Gruppen × Wirkungsarten × Freigabestufen), Begrüßung,
  Ablauf, Protokoll-Schwärzung, IPC-Pfade.
- Rust Integration: `src-tauri/tests/mcp_server.rs` mit einem rmcp-Client (Feature `client` ist schon
  da) gegen eine Test-Vault über einen Socket im Temp-Verzeichnis: Registrierung, Werkzeugliste,
  Ablehnungen, Widerruf, Ablauf mit injizierter Uhr.
- Vertragstest `mcp_server_contract.rs`: Jede Gruppe in R3 hat einen Fall; jede Leitplanken-Aktion
  ist nie gelistet.
- `pnpm check:agent-actions` erweitert (R4, R10, R13).
- E2E: Szenario `mcp-agent` mit dem Hilfsprogramm aus dem e2e-Binary als Agent.
- Von Hand: [quickstart.md](./quickstart.md) mit echtem Claude Code und Codex.
