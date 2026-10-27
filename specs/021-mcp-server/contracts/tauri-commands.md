# Contract: Tauri-Commands, Ereignisse und Katalogaktionen

**Feature**: [../spec.md](../spec.md) | **Research**: R4, R5, R6, R13, R16

## Commands (nur Fenster)

| Command                   | Eingabe                                                                             | Ergebnis / Wirkung                                                             |
| ------------------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `mcp_status`              | –                                                                                   | an/aus, Wege, Endpunkt, HTTP-Port, verbundene Agenten                          |
| `mcp_agents_list`         | –                                                                                   | Agenten mit Zustand, letzter Verbindung, Freigabestufe, Rechte-Zusammenfassung |
| `mcp_agent_get`           | `agentId`                                                                           | Agent mit allen Rechten                                                        |
| `mcp_agent_log`           | `agentId`, `before?`, `limit` (≤ 200)                                               | Protokolleinträge, neueste zuerst                                              |
| `mcp_configure`           | `enabled`                                                                           | schaltet den Zugang dieses Geräts; nur über `settings.agents.mcp.enable`       |
| `mcp_registration_answer` | `requestId`, `decision` (`register`/`deny`), `remember`, `grants`, `permissionMode` | beendet den Registrierungsdialog                                               |
| `mcp_call_answer`         | `requestId`, `decision` (`allow`/`deny`)                                            | beendet eine Rückfrage                                                         |
| `mcp_setup_snippet`       | `client` (`claude-code`, `codex`, `other`), `transport`                             | Text der Anleitung ohne Nachweis (FR-005)                                      |

Änderungen an Einstellungen und Rechten laufen nicht über eigene Commands, sondern über die
Katalogaktionen unten (ein Weg für Oberfläche und Tests).

## Ereignisse (Rust → Fenster)

| Ereignis                   | Inhalt                                                                                                                                                                                      |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcp-registration-request` | `requestId`, Slug, angegebener Name, Transport, vorausgefüllte Rechte, Frist                                                                                                                |
| `mcp-call-request`         | `requestId`, Agent-Id, angegebener Name, Werkzeug (lokalisiert), Wirkung, Ziel, Eingaben ohne Geheimnisse, Hinweis „Geheimnis geht an Agent und Anbieter“ bei `passwords.secret.get`, Frist |
| `mcp-prompt-closed`        | `requestId`, Grund (`answered`, `timeout`, `cancelled`)                                                                                                                                     |
| `mcp-agents-changed`       | –                                                                                                                                                                                           |

Delegates: `mcp-call-request` mit `delegateTurnId` wird vom Chat als Freigabe-Zeile gezeigt statt
als Dialog (R6).

`action-call-request` (vorhanden) bekommt das Feld `caller`: `{ "kind": "builtinAgent" }` oder
`{ "kind": "externalAgent", "agentId": "…" }`.

## Katalogaktionen (neu)

| Id                               | Bereich            | Wirkung       | agentCallable | builtinAgentCallable |
| -------------------------------- | ------------------ | ------------- | ------------- | -------------------- |
| `settings.agents.mcp.enable`     | `guardrails`       | `write`       | nein          | –                    |
| `settings.agents.mcp.grants.set` | `guardrails`       | `write`       | nein          | –                    |
| `settings.agents.mcp.mode.set`   | `guardrails`       | `write`       | nein          | –                    |
| `settings.agents.mcp.revoke`     | `guardrails`       | `destructive` | nein          | –                    |
| `settings.agents.mcp.unblock`    | `guardrails`       | `write`       | nein          | –                    |
| `settings.agents.mcp.log.clear`  | `guardrails`       | `destructive` | nein          | –                    |
| `passwords.secret.get`           | `passwords.secret` | `read`        | ja            | nein                 |

Neuer Bereich `passwords.secret` in `src/lib/actions/scopes.ts` mit lokalisiertem Namen.
`scripts/check-agent-actions.ts` prüft: alle `settings.agents.mcp.*` im Bereich `guardrails`;
`passwords.secret.get` nie für den eingebauten Agenten; kein Schema mit geheimer Eingabe.
