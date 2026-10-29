# Contract: Orchestrator-Werkzeuge, Commands, Ereignisse

## Werkzeuge nur für den Orchestrator

Keines ändert Nutzerdaten (FR-003). Angeboten nur in Antworten des Orchestrators.

| Werkzeug        | Eingabe                                                                         | Ergebnis                                                                                                |
| --------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `task_dispatch` | `{label, goal, context, answers?, targets?, stage, reason, workdir?, waitFor?}` | `{taskId, stage, model}` \| `{conflict: {taskId, label, targets}}` \| `{unavailable: stage, fallback?}` |
| `task_cancel`   | `{taskId}`                                                                      | `{cancelled: bool}`                                                                                     |
| `task_status`   | `{taskId?}`                                                                     | Liste `{taskId, label, state, stage, startedAt, lastStep?}`                                             |
| `task_answer`   | `{questionId, answer}`                                                          | `{delivered: bool}`; nur für Rückfragen der Art `info`                                                  |
| `ask_user`      | vorhanden (046)                                                                 | vorhanden                                                                                               |

Regeln:

- `stage` ∈ `local`, `small`, `large`, `delegate`; nennt der Nutzer eine Stufe oder ein Modell, gilt
  diese (FR-009). `task_dispatch` prüft Verfügbarkeit und wählt bei `unavailable` die nächste
  verfügbare passende Stufe (Reihenfolge aufwärts, dann abwärts; `delegate` nur für Programmierarbeit).
- `targets` werden vor dem Start mit den Zielen laufender Aufgaben verglichen. Bei Überschneidung
  ohne `waitFor` startet nichts; das Werkzeug gibt die kollidierende Aufgaben-Id, ihren Namen und
  die kollidierenden Ziele zurück. Der Orchestrator zeigt diese Daten samt dem neuen Auftrag an und
  fragt, ob die neue Aufgabe warten oder die laufende beendet werden soll. Bei „warten“ wiederholt
  er `task_dispatch` mit `waitFor` auf die kollidierende Aufgaben-Id; die neue Aufgabe wird dann
  eingereiht und startet erst, wenn deren Ziele frei sind. Bei „beenden“ ruft er `task_cancel` für
  die kollidierende Aufgabe auf und dispatcht danach erneut. Eine wartende Entscheidung führt
  niemals schon Aktionen aus (FR-021).
- Vor Speicherung prüft `label`, `goal`, `context`, jedes Element aus `answers`, `reason` und
  `workdir` auf Zugangsdaten, private Schlüssel, Sitzungstoken und andere Geheimnisse. Der eigene
  fail-closed Detektor erkennt bekannte Schlüssel- und Tokenformate, Zugangsdaten in URLs,
  credential-artige Bezeichner mit zugehörigem Wert (etwa `password=...`, `password: ...`,
  `the password is ...`, `token: ...` oder `Bearer ...`) sowie hochentropische geheime
  Zeichenfolgen. `mcp_server::redact` schwärzt nur bekannte strukturierte
  Felder und gilt nicht als Detektor. Bei Treffer oder uneindeutigem Ergebnis wird der Auftrag
  abgelehnt, ohne den erkannten Wert zu loggen.
- Freigaben haben kein Werkzeug: `task_answer` lehnt Fragen der Art `approval` ab (FR-014).
- Die Rückfrage zu einer Freigabe zeigt den vom Orchestrator formulierten Satz zusammen mit dem
  ursprünglichen Werkzeugnamen und dessen Eingabe unverändert. Nur `permission::decide` und die
  Nutzerantwort entscheiden (FR-014).

## Werkzeug nur für Ausführer

| Werkzeug      | Eingabe                                                      | Wirkung                         |
| ------------- | ------------------------------------------------------------ | ------------------------------- |
| `task_report` | `{status: "done"\|"partial"\|"failed", result, open?: [..]}` | beendet die Antwort mit Bericht |

## Tauri-Commands

| Command                       | Eingabe           | Ergebnis         | Spec   |
| ----------------------------- | ----------------- | ---------------- | ------ |
| `chat_tasks_list`             | `{threadId}`      | `TaskWire[]`     | FR-022 |
| `chat_task_get`               | `{taskId}`        | `TaskWire`       | FR-022 |
| `chat_task_cancel`            | `{taskId}`        | –                | FR-020 |
| `chat_task_retry`             | `{taskId, stage}` | `TaskWire`       | FR-023 |
| `abort_turn_by_id`            | `{turnId}`        | –                | R1     |
| `orchestration_stage_options` | –                 | Modelle je Stufe | FR-007 |

`TaskWire` = Zeile aus `chat_tasks` plus `questions[]` und `executorThreadId`.

## Ereignisse

| Ereignis             | Nutzlast                                      |
| -------------------- | --------------------------------------------- |
| `chat-task-state`    | `TaskWire`                                    |
| `chat-task-question` | `{taskId, questionId, kind, text}`            |
| Chat-Ereignisse      | wie bisher, zusätzlich `turnId` und `taskId?` |

## Einstellungen

Aktionen `settings.orchestration.*` über den Helfer `setting({...})` im Bereich `guardrails` (FR-025).
