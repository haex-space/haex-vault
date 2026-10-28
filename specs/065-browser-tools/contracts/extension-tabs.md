# Contract: Übergebene Tabs im Browser des Nutzers (Stufe 2)

**Feature**: [../spec.md](../spec.md) | **Research**: R14 | Baut auf
[049 contracts](../../049-external-bridge/contracts/) (Protokoll 3, Umschläge, Zulassung) auf.

Neue Nachrichten im Protokoll der External Bridge. Sie setzen eine zugelassene Erweiterung mit der
neuen erklärten Fähigkeit `agentTabs` voraus; eine Erweiterung ohne sie bekommt keine dieser
Nachrichten, und holzi bietet Stufe 2 nicht an.

## Erweiterung → holzi

| Nachricht             | Inhalt                                   | Wirkung                                                                                  |
| --------------------- | ---------------------------------------- | ---------------------------------------------------------------------------------------- |
| `agentTab.handOver`   | `tabId`, `url`, `title`, gewählter Agent | Nutzer übergibt den Tab in der Erweiterung; holzi prüft Recht `user-browser` und Website |
| `agentTab.takeBack`   | `tabId`                                  | Nutzer nimmt den Tab zurück (= Stoppen für diesen Tab)                                   |
| `agentTab.userInput`  | `tabId`                                  | vertrauenswürdige Eingabe des Nutzers im Tab (FR-017)                                    |
| `agentTab.closed`     | `tabId`                                  | Tab geschlossen                                                                          |
| `agentTab.request`    | `tabId`, `method`, `url`, `formFields?`  | ändernde Anfrage im Zeitfenster aus R5; wartet auf `agentTab.requestDecision`            |
| `agentTab.navigation` | `tabId`, `url`                           | Dokument-Navigation vor dem Laden; wartet auf `agentTab.navigationDecision`              |
| `agentTab.newTab`     | `openerTabId`, `tabId`, `url`            | eine Seite öffnet einen Tab; holzi fragt (FR-034)                                        |

## holzi → Erweiterung

| Nachricht                                          | Inhalt                                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------------- |
| `agentTab.read`                                    | `tabId`, `from?`, `section?` → Seite wie in browser-tools.md                    |
| `agentTab.click` / `.type` / `.select` / `.scroll` | wie die Werkzeuge                                                               |
| `agentTab.navigate`                                | `tabId`, `to`                                                                   |
| `agentTab.screenshot`                              | `tabId` → Bild des sichtbaren Bereichs                                          |
| `agentTab.fill`                                    | `tabId`, `form?`, Felder mit Werten (nur in diesem Umschlag, nie protokolliert) |
| `agentTab.requestDecision` / `.navigationDecision` | `allow` / `deny`                                                                |
| `agentTab.marker`                                  | `tabId`, angegebener Name des Agenten, Zustand (`running`, `paused`)            |
| `agentTab.release`                                 | `tabId` (Ende der Steuerung)                                                    |

## Regeln

- Die Erweiterung führt nur in übergebenen Tabs aus; jede Nachricht für einen anderen `tabId`
  beantwortet sie mit `notHandedOver`.
- `chrome.debugger` nur für übergebene Tabs, gelöst bei `release`, `takeBack`, `closed`.
- Ohne `chrome.debugger` (Firefox): `agentTab.screenshot` und Absenden-Erkennung Stufe 2 fehlen;
  holzi bietet dann nur Lesen an (FR-035).
- Die Erweiterung trifft keine Entscheidung über Rechte; sie meldet und führt aus.
