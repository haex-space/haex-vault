# Contract: Orchestrator-Werkzeuge, Commands, Ereignisse

## Werkzeuge nur für den Orchestrator

Keines ändert Nutzerdaten (FR-003). Angeboten nur in Antworten des Orchestrators.

| Werkzeug        | Eingabe                                                               | Ergebnis                                                                              |
| --------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `task_dispatch` | `{label, goal, context, answers?, targets?, stage, reason, workdir?}` | `{taskId, stage, model}` \| `{conflict: taskId}` \| `{unavailable: stage, fallback?}` |
| `task_cancel`   | `{taskId}`                                                            | `{cancelled: bool}`                                                                   |
| `task_status`   | `{taskId?}`                                                           | Liste `{taskId, label, state, stage, startedAt, lastStep?}`                           |
| `task_answer`   | `{questionId, answer}`                                                | `{delivered: bool}`; nur für Rückfragen der Art `info`                                |
| `ask_user`      | vorhanden (046)                                                       | vorhanden                                                                             |

Regeln:

- `stage` ∈ `local`, `small`, `large`, `delegate`; nennt der Nutzer eine Stufe oder ein Modell, gilt
  diese (FR-009). `task_dispatch` prüft Verfügbarkeit und wählt bei `unavailable` die nächste
  verfügbare passende Stufe (Reihenfolge aufwärts, dann abwärts; `delegate` nur für Programmierarbeit).
- `context` und `answers` werden vor dem Speichern auf Geheimnisse geprüft (gleiche Schwärzung wie
  `mcp_server::redact`); ein Treffer lehnt den Auftrag ab.
- Freigaben haben kein Werkzeug: `task_answer` lehnt Fragen der Art `approval` ab (FR-014).

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
