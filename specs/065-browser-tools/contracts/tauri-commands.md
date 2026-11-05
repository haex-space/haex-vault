# Contract: Tauri-Commands, Ereignisse und Katalogaktionen

**Feature**: [../spec.md](../spec.md) | **Research**: R8, R11, R13

## Commands (nur Fenster)

| Command                       | Eingabe                                                                                                                             | Ergebnis / Wirkung                                                                                                                |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `agent_browser_status`        | –                                                                                                                                   | verfügbar, gewählter Browser und Version, läuft ja/nein, Profil vorhanden, Download angeboten                                     |
| `agent_browser_sessions`      | –                                                                                                                                   | laufende Sitzungen: Agent (angegebener Name), Browser, Tabs (Website, Titel), Zustand, letzte Handlung                            |
| `agent_browser_prompt_answer` | `requestId`, `decision` (`allow`/`deny`), `remember?` (`site-read`, `site-operate`, `fill`, `request`), `entryId?` (nur bei `fill`) | beendet eine Browser-Rückfrage                                                                                                    |
| `agent_browser_control`       | `sessionId`, `command` (`stop`, `takeover`, `return`)                                                                               | Stoppen, Eingreifen, Zurückgeben                                                                                                  |
| `agent_browser_cookies`       | –                                                                                                                                   | the profile's cookies (name, exact domain, website, path, expiry, flags), without their values (FR-046)                           |
| `agent_browser_trackers`      | –                                                                                                                                   | the filter lists with their state: time of the last download, error, whether the first download still runs (FR-045)               |
| `agent_browser_trackers_set`  | `settings` (`enabled`, `off`, `custom`)                                                                                             | behind `settings.agentBrowser.trackers.set`: checks, keeps and applies the choice, answers the state as `agent_browser_trackers`  |
| `agent_browser_cookie_value`  | cookie (`name`, `domain` as listed, `path`)                                                                                         | the value of one cookie, for the window only                                                                                      |
| `agent_browser_cookie_set`    | `cookie`, `value`, `expires?` (seconds, none = session)                                                                             | behind `settings.agentBrowser.profile.cookie.set`: new value and expiry, every other attribute kept                               |
| `agent_browser_cookie_delete` | `target` (`{ cookie }` or `{ site }`)                                                                                               | behind `settings.agentBrowser.profile.cookie.delete`: one cookie or every cookie of a website, answers how many went              |
| `agent_browser_dns`           | –                                                                                                                                   | the DNS over HTTPS choice and the presets (`id`, name, address) for "KI → Browser" (FR-048)                                       |
| `agent_browser_dns_set`       | `settings` (`mode`, `provider`, `custom?`)                                                                                          | behind `settings.agentBrowser.dns.set`: checks and keeps the choice, answers it as `agent_browser_dns`; applies at the next start |

Änderungen an Rechten und Einstellungen laufen über die Katalogaktionen unten.

## Ereignisse (Rust → Fenster)

| Ereignis                         | Inhalt                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent-browser-prompt`           | `requestId`, `kind` (`site`, `action`, `submit`, `request` (FR-044: `label` is the request, `site` the website it goes to), `fill`, `takeover`, `download`, `newTab`, `switchOn` (FR-040, only the agent name)), angegebener Name des Agenten, Website, je Art: `label` und `fields` (Geheimnisse als `•••`) bei `submit`, `entries` (nur Titel, nur für den Nutzer) bei `fill`, `reason` bei `takeover`, `size` bei `download`; Frist |
| `agent-browser-prompt-closed`    | `requestId`, Grund (`answered`, `timeout`, `cancelled`)                                                                                                                                                                                                                                                                                                                                                                                |
| `agent-browser-sessions-changed` | –                                                                                                                                                                                                                                                                                                                                                                                                                                      |

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
| `settings.agentBrowser.profile.cookie.set`                            | `guardrails` | `write`       | nein          |
| `settings.agentBrowser.profile.cookie.delete`                         | `guardrails` | `destructive` | nein          |
| `settings.agentBrowser.trackers.set`                                  | `guardrails` | `write`       | nein          |
| `settings.agentBrowser.dns.set`                                       | `guardrails` | `write`       | nein          |

Neuer Bereich `browser` in `src/lib/actions/scopes.ts` mit lokalisiertem Namen. Er gehört nicht zur
Gruppe „holzi bedienen“ (`grants::OPERATE_SCOPES`), sondern zur Rechtegruppe `browser`.
`scripts/check-agent-actions.ts` prüft: alle `browser.*` sind `runner: 'native'`, ändernde und
zerstörende haben `confirmsItself`; `settings.agentBrowser.*` liegen im Bereich `guardrails`.
