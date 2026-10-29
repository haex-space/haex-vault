# Contract: Tauri-Commands, Ereignisse und Katalogaktionen

**Feature**: [../spec.md](../spec.md) | **Research**: R8, R11, R13

## Commands (nur Fenster)

| Command                       | Eingabe                                                                                                                  | Ergebnis / Wirkung                                                                                     |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| `agent_browser_status`        | –                                                                                                                        | verfügbar, gewählter Browser und Version, läuft ja/nein, Profil vorhanden, Download angeboten          |
| `agent_browser_sessions`      | –                                                                                                                        | laufende Sitzungen: Agent (angegebener Name), Browser, Tabs (Website, Titel), Zustand, letzte Handlung |
| `agent_browser_prompt_answer` | `requestId`, `decision` (`allow`/`deny`), `remember?` (`site-read`, `site-operate`, `fill`), `entryId?` (nur bei `fill`) | beendet eine Browser-Rückfrage                                                                         |
| `agent_browser_control`       | `sessionId`, `command` (`stop`, `takeover`, `return`)                                                                    | Stoppen, Eingreifen, Zurückgeben                                                                       |

Änderungen an Rechten und Einstellungen laufen über die Katalogaktionen unten.

## Ereignisse (Rust → Fenster)

| Ereignis                         | Inhalt                                                                                                                                                                                                                                                                                                  |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent-browser-prompt`           | `requestId`, `kind` (`site`, `submit`, `fill`, `takeover`, `download`, `newTab`), angegebener Name des Agenten, Website, je Art: `label` und `fields` (Geheimnisse als `•••`) bei `submit`, `entries` (nur Titel, nur für den Nutzer) bei `fill`, `reason` bei `takeover`, `size` bei `download`; Frist |
| `agent-browser-prompt-closed`    | `requestId`, Grund (`answered`, `timeout`, `cancelled`)                                                                                                                                                                                                                                                 |
| `agent-browser-sessions-changed` | –                                                                                                                                                                                                                                                                                                       |

Die Rückfragen erscheinen als Dialog im Fenster (wie der Aufrufdialog aus 021 PR C), eine nach der
anderen; holzi kommt nach vorn oder meldet sich per Systembenachrichtigung.

Bei `kind = "fill"` enthält `entries` für den Dialog nur opaque `entryId`-Werte und Titel. Der
Nutzer wählt eine ID; nur diese ID darf mit `agent_browser_prompt_answer` zurückkommen. Geheimnis,
Benutzername und Feldwerte werden nie an das Frontend übergeben.

## Katalogaktionen (neu)

| Id                                                                    | Bereich      | Wirkung       | agentCallable |
| --------------------------------------------------------------------- | ------------ | ------------- | ------------- |
| `browser.*` (contracts/browser-tools.md)                              | `browser`    | je Werkzeug   | ja            |
| `settings.agentBrowser.builtin.set`                                   | `guardrails` | `write`       | nein          |
| `settings.agents.mcp.grants.set` (021, erweitert um `rights.browser`) | `guardrails` | `write`       | nein          |
| `settings.agentBrowser.path.set`                                      | `guardrails` | `write`       | nein          |
| `settings.agentBrowser.download`                                      | `guardrails` | `write`       | nein          |
| `settings.agentBrowser.profile.clear`                                 | `guardrails` | `destructive` | nein          |

Neuer Bereich `browser` in `src/lib/actions/scopes.ts` mit lokalisiertem Namen. Er gehört nicht zur
Gruppe „holzi bedienen“ (`grants::OPERATE_SCOPES`), sondern zur Rechtegruppe `browser`.
`scripts/check-agent-actions.ts` prüft: alle `browser.*` sind `runner: 'native'`, ändernde und
zerstörende haben `confirmsItself`; `settings.agentBrowser.*` liegen im Bereich `guardrails`.
