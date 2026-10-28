# Research: Orchestrator und Modell-Routing

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-10-28

## Ausgangslage im Code

- **Eine Antwort zur Zeit**: `ChatState` (`chat/session.rs`) ist global: eine Sperre `operation`
  (gehalten für die ganze Antwort, `try_lock` ohne Warten), ein geladenes Modell `session`, ein
  `current_generation`, ein `turn_cancellation`. Eine zweite Antwort scheitert mit „a chat or vault
  operation is still in progress“. `abort_turn` bricht alles ab, auch alle offenen Freigaben und
  Auswahlen. Das Frontend kennt nur einen laufenden Strom (`useChatTranscript.ts`).
- **Modellwahl**: Eine Antwort nutzt das gerade geladene Modell; `chat_threads.last_model_id` wird
  nur geschrieben. Ein lokales Modell ist zur Zeit geladen; ein Ersetzen entlädt das vorige.
  `LocalModel` (mistralrs) kann mehrere Sequenzen gleichzeitig bedienen, haex serialisiert sie aber
  bewusst (`model_jobs/chat_slot.rs`).
- **Modell-Jobs (055)**: `model_jobs/` baut Adapter pro Modell ohne die Chat-Sitzung
  (`ModelSource`, `Providers::adapter`), leiht den lokalen Platz aus (`local_slot.rs`,
  `chat_slot.rs`) und kapselt Delegates (`delegate.rs`) — ohne Werkzeuge (055 FR-026).
- **Delegates**: `CliDelegateAdapter` startet `claude -p` bzw. `codex app-server` pro Aufruf in einem
  Wegwerf-Ordner (`TempDir`); Freigaben pro Werkzeugaufruf über `approval_bridge` in dieselbe
  Freigabe-Logik und dieselbe `pending_tool_approvals`-Tabelle. Einen Projektordner gibt es nicht.
- **Freigaben und Rückfragen**: Einträge in `pending_tool_approvals` / `pending_choices` sind nach
  Request-Id geschlüsselt; Ereignisse gehen global über `emit`; das Frontend ordnet sie nach `threadId`
  zu. Ein anderer Empfänger ist also über eine eigene `emit`-Senke möglich.
- **Verbrauch**: nur `prompt_tokens`/`completion_tokens` der letzten Runde in der letzten
  Assistenten-Zeile; keine Summen, keine Kosten.
- **Werkzeuge**: `trait Tool`, `ToolRegistry`; `ask_user` als eingebautes Werkzeug; native
  Katalogaktionen nach ADR-0011; Angebot pro Antwort in `tools/offer.rs`. Aktionen im Webview laufen
  über `ActionBridge` mit einer globalen Sperre `run_lock` nacheinander.
- **Fähigkeit „Werkzeuge“** (012): `ModelCapabilities.tool_use`, abgefragt über
  `ToolAvailability::of`.

## R1 — Parallele Antworten: Antwort-Plätze statt globaler Zustand

**Entscheidung**: `ChatState` bekommt `TurnSlots`: pro laufender Antwort ein `TurnSlot { turn_id,
thread_id, task_id?, cancel, generation, model }`. Die Sperre `operation` wird ein `RwLock`: Antworten
nehmen den Lese-Anteil, Modell-Laden und Vault-Übergänge den Schreib-Anteil. `abort_turn(turn_id)`
bricht nur diese Antwort ab; offene Freigaben und Auswahlen tragen die `turn_id` und werden nur für sie
verworfen. Der bisherige Command `abort_current_generation` bricht die Antwort der aktiven
Unterhaltung ab. Das Frontend verwaltet Ströme pro Unterhaltung und pro Aufgabe.

**Begründung**: Ohne diesen Umbau gibt es keine gleichzeitigen Aufgaben (US4) und keinen Orchestrator,
der antwortet, während ein Ausführer arbeitet. Der Umbau ist ohne sichtbare Änderung lieferbar (PR B)
und durch die vorhandenen Turn-Tests abgesichert.

**Alternative**: Ausführer nur als Modell-Jobs (055) neben dem Chat — verworfen, Jobs haben keine
Werkzeuge und keine Freigaben.

## R2 — Modell pro Antwort

**Entscheidung**: Eine Antwort bekommt ihr Modell beim Start: Der Orchestrator nutzt sein Modell,
ein Ausführer das Modell seiner Stufe. Adapter kommen über `model_jobs::ModelSource` /
`Providers::adapter` (eine Implementierung für Jobs und Aufgaben). Die bisherige globale `session`
bleibt das Modell des direkten Chats (Orchestrierung aus).

**Lokale Modelle**: Es ist höchstens ein lokales Modell geladen. Orchestrator und Stufe „lokal“ teilen
es, wenn beide lokal sind; `LocalModel` bedient dafür bis zu zwei Sequenzen gleichzeitig. Wählt der
Nutzer für beide verschiedene lokale Modelle, weist die Einstellung darauf hin, dass dann lokale
Aufgaben warten, bis der Orchestrator nicht spricht, und das Modell gewechselt wird (Leihe nach
`chat_slot.rs`). Der Spike (T004) misst Speicher und Wartezeit zweier Sequenzen auf dem
Referenz-Desktop; reicht es nicht, gilt eine Sequenz und die Wartezeit wird angezeigt.

## R3 — Der Orchestrator ist eine Antwort mit eigenem Werkzeugsatz

**Entscheidung**: Der Orchestrator läuft als normale Antwort (`run_turn`) mit seinem Modell, aber
einem festen, kleinen Werkzeugsatz statt der Aktionen:

- `task_dispatch {goal, context, stage, reason, answers, targets, workdir?}`
- `task_cancel {taskId}`
- `task_status {taskId?}`
- `task_answer {questionId, answer}` (Rückfrage eines Ausführers aus dem Auftrag beantworten)
- `ask_user` (vorhanden)

Kein Werkzeug ändert Nutzerdaten; das sichert FR-003 technisch statt nur per Anweisung. Systemtext:
Rolle, Uhrzeit, Datum, Liste der Stufen mit Belegung und Hinweis „braucht haex-Werkzeuge?“, Zustand der
laufenden Aufgaben, Regeln für Stufenwahl und Rückfragen. Einfache Fragen beantwortet er mit Text ohne
Werkzeug (FR-002).

**Begründung**: Nutzt den vorhandenen Werkzeug-Ablauf; strukturierte Entscheidungen als
Werkzeugaufrufe statt Freitext-Parsing; misst sich mit dem Tool-Eval aus 032.

## R4 — Ausführer und Rückkanal

**Entscheidung**: Ein Ausführer ist eine Antwort im eigenen `TurnSlot` mit `task_id`, Modell der Stufe
und dem Auftrag als Nutzernachricht in einer **eigenen, verborgenen Unterhaltung der Aufgabe**
(`chat_threads.origin = 'task'`, verknüpft mit der Gesprächs-Unterhaltung). Seine `emit`-Senke leitet
Ereignisse an die Aufgabe weiter:

- `tool-permission-request` → Rückfrage Art „Freigabe“: Der Orchestrator formuliert sie (Aufgabe
  benannt, Wirkung, Ziel) und stellt sie in der Gesprächs-Unterhaltung; die Antwort geht über
  `respond_tool_permission` (gleiche Request-Id). Entschieden wird wie bisher von
  `permission::decide` und dem Nutzer; der Orchestrator hat dafür kein Werkzeug (FR-014).
- `chat-choice-request` (`ask_user` des Ausführers) → Rückfrage Art „Auskunft“: Der Orchestrator
  bekommt sie als Ereignis in seiner nächsten Antwort und beantwortet sie mit `task_answer`, wenn der
  Auftrag die Antwort enthält, sonst mit `ask_user` an den Nutzer (FR-015).
- `chat-turn-complete` → Bericht.

Delegates: `DelegateChatContext` bekommt dieselbe Senke; ihre Freigaben laufen schon über
`approval_bridge` mit Request-Ids.

**Bericht**: Der Ausführer bekommt im Systemtext die Pflicht, mit einem Bericht im Format
`{status, result, open}` zu enden (Werkzeug `task_report`, nur für Ausführer). Fehlt er, baut haex
einen Bericht aus der letzten Antwort und `finish_reason` (Rundengrenze → „teilweise“).

## R5 — Delegates im Projektordner

**Entscheidung**: Ein Auftrag an die Stufe „Delegate“ trägt `workdir`. Der Orchestrator fragt nach dem
Ordner, wenn er nicht klar ist, aus einer Liste bekannter Projektordner pro Gerät (vom Nutzer
hinzugefügt; Rechte über die Datei-Freigaben aus 044). `claude.rs`/`codex.rs` nutzen `workdir` als
Arbeitsverzeichnis; Konfiguration (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) bleibt im Wegwerf-Ordner. Ohne
Ordner arbeitet der Delegate wie bisher im Wegwerf-Ordner (für Aufgaben ohne Projekt). Spec-Korrektur:
FR-026.

## R6 — Verbrauch

**Entscheidung**: `run_turn` summiert die Tokens aller Runden (`StreamChunk::Done`) und speichert die
Summe pro Antwort; eine Aufgabe zeigt Orchestrator- und Ausführer-Summen getrennt. Kosten zeigt haex
nur, wenn ein Anbieter sie meldet (heute keiner); eine eigene Preistabelle ist nicht Teil dieser Spec.

## R7 — Stufenwahl messen

**Entscheidung**: 40 Aufgaben mit menschlicher Zuordnung (davon 10 Programmierung, 10 einfache
Fragen ohne Aufgabe) als Fixture des Tool-Eval aus 032; gemessen mit lokalem Orchestrator (Qwen3-4B)
und Haiku (SC-002, SC-003). Ergebnis in dieser Datei.

## R8 — Gleiches Ziel

**Entscheidung**: `task_dispatch` trägt `targets` (Dateipfade, Ids aus Werkzeug-Ergebnissen, Namen),
vom Orchestrator gefüllt. Überschneidet sich eine neue Aufgabe mit einer laufenden, liefert
`task_dispatch` statt des Starts „Konflikt mit Aufgabe X“; der Orchestrator fragt den Nutzer (FR-021).
Eine Heuristik, kein Beweis; vermerkt als `ponytail:`-Kommentar.

## R9 — Router: RouteLLM und Jev geprüft, nicht übernommen

- **RouteLLM** (LMSYS): Python, entscheidet zwischen zwei Modellen (stark/schwach), braucht Training
  oder eigene Laufzeit. Vier Stufen und Delegates deckt es nicht ab.
- **Jev** (`thejevai.com`, laut Community-Artikel auf Hugging Face vom 2026-09-22; Hersteller im
  Artikel nicht genannt): gehostete API für strukturierte Entscheidungen (Auswahl, Bewertung,
  Ja/Nein-Wahrscheinlichkeit); keine lokalen Gewichte, keine Lizenz- oder Messangaben. Jede Nachricht
  ginge an einen unbekannten Dienst; das widerspricht dem lokalen Standard-Orchestrator.
- **Entscheidung**: Die Stufenwahl trifft der Orchestrator mit `task_dispatch`. Reicht SC-003 nicht,
  kann ein Router als optionaler Schritt vor `task_dispatch` folgen (eigene Spec), dann lokal oder mit
  Kennzeichnung wie jeder Cloud-Dienst.

## R10 — Daten

**Entscheidung**: CRDT-Tabellen `chat_tasks` und `chat_task_questions` ([data-model.md](./data-model.md));
Einstellungen als Geräteeinstellungen `orchestration.*`; Projektordner als Gerätezeilen.

## Spec-Korrekturen aus der Planung

- **FR-026 (neu)**: Aufgaben der Stufe „Delegate“ laufen in einem Projektordner, den der Nutzer nennt
  oder bestätigt; ohne Ordner im bisherigen Wegwerf-Ordner.
- **FR-027 (neu)**: Sind Orchestrator und Stufe „lokal“ verschiedene lokale Modelle, warten lokale
  Aufgaben, während der Orchestrator spricht; die Einstellungen sagen das.
