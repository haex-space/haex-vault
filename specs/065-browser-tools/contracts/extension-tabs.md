# Contract: Übergebene Tabs im Browser des Nutzers (Stufe 2)

**Feature**: [../spec.md](../spec.md) | **Research**: R14 | Baut auf
[049 contracts](../../049-external-bridge/contracts/) (Protokoll 3, Umschläge, Zulassung) auf.

Neue Nachrichten im Protokoll der External Bridge. Sie setzen eine zugelassene Erweiterung mit der
neuen erklärten Fähigkeit `agentTabs` voraus; eine Erweiterung ohne sie bekommt keine dieser
Nachrichten, und holzi bietet Stufe 2 nicht an.

## Erweiterung → holzi

| Nachricht             | Inhalt                                               | Wirkung                                                                                              |
| --------------------- | ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `agentTab.handOver`   | `tabId`, `url`, `title`, gewählter Agent             | Nutzer übergibt den Tab in der Erweiterung; holzi prüft Recht `user-browser` und Website             |
| `agentTab.takeBack`   | `tabId`                                              | Nutzer nimmt den Tab zurück (= Stoppen für diesen Tab)                                               |
| `agentTab.userInput`  | `tabId`, `sessionId`                                 | vertrauenswürdige Eingabe des Nutzers im Tab (FR-017)                                                |
| `agentTab.return`     | `tabId`, `sessionId`                                 | Nutzer gibt die Steuerung nach `userInput` zurück                                                    |
| `agentTab.closed`     | `tabId`                                              | Tab geschlossen                                                                                      |
| `agentTab.request`    | `tabId`, `requestId`, `method`, `url`, `formFields?` | ändernde Anfrage; wartet auf Entscheidung mit demselben `requestId`                                  |
| `agentTab.navigation` | `tabId`, `frameId`, `navigationId`, `url`            | Top-Level- oder Frame-Navigation vor dem Laden; wartet auf Entscheidung mit derselben `navigationId` |
| `agentTab.newTab`     | `openerTabId`, `tabId`, `url`                        | eine Seite öffnet einen Tab; holzi fragt (FR-034)                                                    |

## holzi → Erweiterung

| Nachricht                                          | Inhalt                                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------------- |
| `agentTab.read`                                    | `tabId`, `from?`, `section?` → Seite wie in browser-tools.md                    |
| `agentTab.click` / `.type` / `.select` / `.scroll` | wie die Werkzeuge                                                               |
| `agentTab.navigate`                                | `tabId`, `to`                                                                   |
| `agentTab.screenshot`                              | `tabId` → Bild des sichtbaren Bereichs                                          |
| `agentTab.fill`                                    | `tabId`, `form?`, Felder mit Werten (nur in diesem Umschlag, nie protokolliert) |
| `agentTab.requestDecision`                         | `requestId`, `allow` / `deny`                                                   |
| `agentTab.navigationDecision`                      | `navigationId`, `allow` / `deny`                                                |
| `agentTab.newTabDecision`                          | `openerTabId`, `tabId`, `allow` / `deny`                                        |
| `agentTab.marker`                                  | `tabId`, angegebener Name des Agenten, Zustand (`running`, `paused`)            |
| `agentTab.release`                                 | `tabId` (Ende der Steuerung)                                                    |

## Zulassung und Regeln

Der Handshake muss für den passenden Eintrag unter `requestedExtensions` die Fähigkeit `agentTabs`
deklarieren. holzi routet `agentTab.*` nur, wenn die Erweiterung zugelassen ist, `agentTabs`
deklariert hat und der Nutzer das Recht `user-browser` gewährt hat. Eine fehlende Erklärung oder
Freigabe führt zu `NOT_SUPPORTED`; ein anderes Extension-Ziel wird nie angenommen.

- Die Erweiterung führt nur in übergebenen Tabs aus; jede Nachricht für einen anderen `tabId`
  beantwortet sie mit `notHandedOver`.
- `chrome.debugger` nur für übergebene Tabs, gelöst bei `release`, `takeBack`, `closed`.
- Ohne `chrome.debugger` (Firefox): `agentTab.screenshot` und Absenden-Erkennung Stufe 2 fehlen;
  holzi bietet dann nur Lesen an (FR-035).
- Die Erweiterung trifft keine Entscheidung über Rechte; sie meldet und führt aus.
- Nach `agentTab.userInput` bleibt die Sitzung pausiert; nur `agentTab.return` mit derselben
  `tabId` und `sessionId` setzt sie fort. `takeBack` beendet die Steuerung.
- Nach Zustimmung zu `newTab` gilt das Kind als übergeben; Ablehnung schließt es und entfernt seine
  Zuordnung. Jede wartende Anfrage wird anhand ihrer eigenen ID beantwortet; verspätete oder fremde
  IDs werden abgelehnt.
