# Data Model: holzi als MCP-Server für externe Agenten

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md) (R8–R10)

Alle Tabellen sind geräteeigen nach ADR-0001: `vault_device_uuid` verweist auf `known_devices`
mit `ON DELETE CASCADE`, und der Dienst liest nur Zeilen des eigenen Geräts. Keine UNIQUE-Bedingung
(ein Konflikt hielte den Sync an, wie bei `haex_browser_clients`); die Kennungen leiten sich
eindeutig ab.

## haex_mcp_agents

| Spalte              | Typ     | Regel                                                                                      |
| ------------------- | ------- | ------------------------------------------------------------------------------------------ |
| `id`                | TEXT PK | Zufällige UUID; ist die `agentId` für `Caller::ExternalAgent` und `agent_file_permissions` |
| `vault_device_uuid` | TEXT FK | Gerät der Registrierung                                                                    |
| `kind`              | TEXT    | `claude_code_delegate`, `codex_delegate`, `external`                                       |
| `slug`              | TEXT    | Aus der Anleitung (`claude-code`, `codex`, frei); nur für den Programm-Weg und die Anzeige |
| `client_name`       | TEXT    | Angegebener Name aus `clientInfo.name`, ≤ 64 Zeichen, nie geprüft                          |
| `client_version`    | TEXT    | Aus `clientInfo.version`, nur Anzeige                                                      |
| `transport`         | TEXT    | `ipc` oder `http`                                                                          |
| `oauth_client_id`   | TEXT    | Nur `http`: Kennung aus der dynamischen Client-Registrierung                               |
| `token_hash`        | TEXT    | `SHA-256` des Zugangsnachweises (Base64url); `NULL` bei `blocked`                          |
| `status`            | TEXT    | `registered`, `expired`, `blocked`                                                         |
| `permission_mode`   | TEXT    | `manual`, `auto`, `plan`; ab Werk `manual`                                                 |
| `registered_at`     | INTEGER | Unix-ms                                                                                    |
| `last_seen_at`      | INTEGER | Unix-ms der letzten angenommenen Verbindung                                                |

Delegates: höchstens eine Zeile je `kind` und Gerät; die Kennung leitet sich aus Gerät und `kind`
ab, damit zwei Geräte nie kollidieren.

### Zustände

```text
(unbekannt) --Dialog: Registrieren--> registered
(unbekannt) --Dialog: Ablehnen + merken--> blocked
registered --90 Tage ohne Verbindung (geprüft beim Verbinden)--> expired
expired --Dialog: Registrieren (Rechte vorausgefüllt)--> registered (neuer Nachweis)
blocked --Nutzer hebt Sperre auf--> (Zeile gelöscht)
jeder Zustand --Widerruf--> (Zeile und Rechte gelöscht, Protokoll bleibt bis zum Ablauf)
```

Vorläufige Ablehnungen (ohne „merken“) liegen nur im Speicher bis zum Schließen der Vault.

## haex_mcp_agent_grants

| Spalte              | Typ     | Regel                                                                                     |
| ------------------- | ------- | ----------------------------------------------------------------------------------------- |
| `id`                | TEXT PK | Abgeleitet aus `agent_id`, `group`, `key`                                                 |
| `vault_device_uuid` | TEXT FK | wie Agent                                                                                 |
| `agent_id`          | TEXT    | → `haex_mcp_agents.id` (Löschen des Agenten löscht seine Rechte im Dienst)                |
| `group`             | TEXT    | `operate`, `extension`, `passwords`, `commands`                                           |
| `key`               | TEXT    | `operate`: Bereichs-Id (`wm.read`, …); `extension`: Erweiterungs-Id; sonst leer           |
| `level`             | TEXT    | `operate`: `read`/`write`; `extension`/`commands`: `on`; `passwords`: `read`/`read_write` |
| `scope_tags`        | TEXT    | Nur `passwords`: JSON-Liste der Tags, `NULL` = alle Einträge                              |

Regeln:

- `guardrails` ist als `key` nie zulässig; der Dienst weist es beim Schreiben ab.
- Ein fehlender Eintrag heißt „keine“/„aus“.
- Neue Gruppen späterer Specs (Browser) bekommen einen neuen `group`-Wert; fehlende Zeilen = aus
  (FR-008).
- Ab Werk beim Registrieren: `operate`/`read` für alle Bereiche der Gruppe „holzi bedienen“
  (research R3), sonst nichts (FR-009).

Dateien und Kontakte: unverändert `agent_file_permissions` (`migrations_agent_files.rs`) mit
`agent_id = haex_mcp_agents.id`. Ein Widerruf löscht dort die Zeilen des Agenten.

## haex_mcp_agent_log_no_sync

| Spalte              | Typ     | Regel                                                                                                |
| ------------------- | ------- | ---------------------------------------------------------------------------------------------------- |
| `id`                | TEXT PK | UUID                                                                                                 |
| `vault_device_uuid` | TEXT    | Gerät                                                                                                |
| `agent_id`          | TEXT    | bleibt nach Widerruf stehen; Anzeige „widerrufener Agent“                                            |
| `created_at`        | INTEGER | Unix-ms                                                                                              |
| `tool`              | TEXT    | Werkzeugname (`settings_appearance_set`, `x_mail_list_new`, …)                                       |
| `effect`            | TEXT    | `read`, `write`, `destructive`                                                                       |
| `target`            | TEXT    | Lesbares Ziel, ≤ 200 Zeichen                                                                         |
| `input`             | TEXT    | JSON der Eingaben, Felder mit `x-holzi-secret` als `•••`, Werte > 200 Zeichen gekürzt                |
| `outcome`           | TEXT    | `ok`, `denied_rights`, `denied_user`, `denied_plan`, `no_answer`, `error`, `unavailable`, `too_many` |
| `asked`             | INTEGER | 1, wenn eine Rückfrage gezeigt wurde                                                                 |

Grenzen: 30 Tage, höchstens 10 000 Einträge je Agent und Gerät; Bereinigung beim Schreiben. Kein
Ergebnis, kein Dateiinhalt, kein Geheimnis (FR-028).

## Einstellungen (Geräteeinstellungen)

| Schlüssel       | Wert           | Ab Werk                                       |
| --------------- | -------------- | --------------------------------------------- |
| `mcp.enabled`   | bool           | `false`                                       |
| `mcp.http_port` | Port, zufällig | beim ersten Einschalten des HTTP-Wegs gewählt |

Beide sind Leitplanken (für Agenten gesperrt, FR-001).

## Im Speicher (nicht persistent)

- **Verbindung**: Agent-Id oder „unbekannt“, Transport, Gegenstelle (UID/Pipe-Client), laufende
  Aufrufe (≤ 4), Sitzungs-Token für `tools/list_changed`.
- **Rückfrage**: Kennung, Agent-Id, Werkzeug, Ziel, Eingaben ohne Geheimnisse, Frist 5 Minuten,
  Ergebnis `allow`/`deny`/`timeout`/`cancelled` (über `PromptPark`).
- **Registrierungsdialog**: Slug, angegebener Name, vorausgefüllte Rechte, Frist 5 Minuten;
  höchstens einer je Slug und Gegenstelle, höchstens 3 insgesamt.
- **Vorläufige Sperren**: bis zum Schließen der Vault.
