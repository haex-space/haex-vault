# Data Model: Orchestrator und Modell-Routing

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

## Geräteeinstellungen (`PrefScope::Device`, ADR-0001)

| Schlüssel                      | Typ                                | Standard                        | Spec   |
| ------------------------------ | ---------------------------------- | ------------------------------- | ------ |
| `orchestration.enabled`        | bool                               | `false`                         | FR-001 |
| `orchestration.model`          | Modell-Id                          | Standard-Chatmodell, wenn lokal | FR-006 |
| `orchestration.stage.local`    | Modell-Id \| null                  | installiertes lokales Modell    | FR-007 |
| `orchestration.stage.small`    | Modell-Id \| null                  | kleines Modell eines Schlüssels | FR-007 |
| `orchestration.stage.large`    | Modell-Id \| null                  | großes Modell eines Schlüssels  | FR-007 |
| `orchestration.stage.delegate` | Delegate-Anbieter-Id \| null       | installierter Delegate          | FR-007 |
| `orchestration.maxParallel`    | int 1–6                            | `3`                             | FR-018 |
| `orchestration.projectFolders` | JSON `[{path, label, lastUsedAt}]` | `[]`                            | FR-026 |

Nur Modelle mit `tool_use.support != "no"` sind für Orchestrator und die Stufen lokal/klein/groß
wählbar (012). Fehlt eine Belegung, ist die Stufe „nicht verfügbar“ (FR-010).

## Unterhaltungen

- Gespräch mit dem Orchestrator: normale Unterhaltung (`origin` NULL, `'voice'` oder `'meeting'`).
- Ausführer einer Aufgabe: eigene Unterhaltung `chat_threads.origin = 'task'`, in Listen verborgen,
  über `chat_tasks.executor_thread_id` erreichbar. (Spalte `origin` kommt aus 066; liegt 068 vorher
  auf `main`, bringt 068 sie mit.)

PR B ergänzt `turn_tokens_in` und `turn_tokens_out` in einer eigenen Migration an `chat_messages`,
damit die Tokenzählung vor den Aufgaben der Orchestrierung funktioniert. PR C ergänzt die folgenden
CRDT-Tabellen in einer eigenen Migration; jede Migration erhöht `HOLZI_TRIGGER_VERSION`.

## Neue CRDT-Tabellen (PR C)

```sql
CREATE TABLE chat_tasks (
  id TEXT PRIMARY KEY NOT NULL,
  thread_id TEXT NOT NULL REFERENCES chat_threads(id) ON DELETE CASCADE,   -- Gespräch
  executor_thread_id TEXT REFERENCES chat_threads(id) ON DELETE SET NULL,  -- Ausführer-Verlauf
  label TEXT NOT NULL,              -- kurzer Name für Meldungen („Mail an Jonas“)
  goal TEXT NOT NULL,
  context TEXT NOT NULL,            -- Auftrag ohne Geheimnisse (FR-011)
  answers TEXT NOT NULL,            -- JSON: vorab geklärte Antworten
  targets TEXT NOT NULL,            -- JSON-Array (R8)
  stage TEXT NOT NULL,              -- 'local'|'small'|'large'|'delegate'
  model TEXT NOT NULL,              -- Modell- oder Delegate-Id beim Start
  reason TEXT NOT NULL,             -- ein Satz (FR-008)
  workdir TEXT,                     -- nur Delegate (FR-026)
  state TEXT NOT NULL,              -- 'queued'|'running'|'waiting'|'done'|'partial'|'failed'|'cancelled'
  report TEXT,                      -- JSON {status, result, open}
  orchestrator_tokens_in INTEGER, orchestrator_tokens_out INTEGER,
  executor_tokens_in INTEGER, executor_tokens_out INTEGER,
  retry_of TEXT,                    -- frühere Aufgabe bei Wiederholung (FR-023)
  created_at INTEGER NOT NULL, started_at INTEGER, ended_at INTEGER
);

CREATE TABLE chat_task_questions (
  id TEXT PRIMARY KEY NOT NULL,     -- = Request-Id der Freigabe bzw. Auswahl
  task_id TEXT NOT NULL REFERENCES chat_tasks(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,               -- 'approval'|'info'
  text TEXT NOT NULL,               -- ohne Geheimnisse
  answer TEXT,
  answered_by TEXT,                 -- 'user'|'orchestrator'
  asked_at INTEGER NOT NULL, answered_at INTEGER
);
```

## Zustände einer Aufgabe

```text
queued ──Platz frei──▶ running ⇄ waiting (Rückfrage offen)
                         │
         Bericht ────────┼──▶ done | partial | failed
         Abbruch ────────┴──▶ cancelled   (auch Vault-Schließen, aktives Ende in 066)
```

## Nur im Speicher

- `TurnSlots`: laufende Antworten mit `turn_id`, `thread_id`, `task_id?`, Abbruch, Erzeugung, Modell
  (R1).
- `TaskScheduler`: Warteschlange, Grenze `maxParallel`, Zuordnung Request-Id → Aufgabe.
