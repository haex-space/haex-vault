# Implementation Plan: Orchestrator und Modell-Routing

**Branch**: `068-task-orchestration` | **Date**: 2026-10-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/068-task-orchestration/spec.md`

## Summary

Ein Orchestrator-Modell (Standard lokal) führt das Gespräch, beantwortet Einfaches selbst, fragt vor
der Übergabe nach, ordnet jede Aufgabe einer Stufe zu (lokal, klein, groß, Delegate), überbringt
Rückfragen und Freigaben der Ausführer, ohne selbst freizugeben, lässt mehrere Aufgaben parallel laufen
und berichtet jedes Ergebnis. Gilt für Chat, Sprachassistent (066) und Meeting-Auswertung (067).

Technischer Ansatz ([research.md](./research.md)):

- **Parallele Antworten** (R1): `TurnSlots` statt globalem Antwort-Zustand; `operation` als `RwLock`;
  Abbruch, Freigaben und Auswahlen pro Antwort; Frontend mit mehreren Strömen.
- **Modell pro Antwort** (R2): Adapter über `model_jobs::ModelSource`; ein lokales Modell, von
  Orchestrator und Stufe „lokal“ mit zwei Sequenzen geteilt.
- **Orchestrator** (R3): eine Antwort mit eigenem Werkzeugsatz `task_dispatch`, `task_cancel`,
  `task_status`, `task_answer`, `ask_user`; keine Aktionen.
- **Ausführer und Rückkanal** (R4): eigene verborgene Unterhaltung pro Aufgabe, eigene `emit`-Senke,
  Freigaben über die vorhandenen Request-Ids, Bericht über `task_report`.
- **Delegates** (R5): Projektordner als Arbeitsverzeichnis.
- **Verbrauch** (R6), **Messung** (R7), **gleiches Ziel** (R8), **Router geprüft** (R9).

## Technical Context

**Language/Version**: Rust (Tauri 2.12), TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA).

**Primary Dependencies**: vorhanden — `mistralrs` 0.8.1 (`llm-cpu`), `reqwest` + `eventsource-stream`
(Anbieter), `tokio`, `tokio-util`, `model_jobs` (055), `cli_delegate` (007/009). **Keine neue Crate,
keine neue Frontend-Abhängigkeit.**

**Storage**: eine Migration (nächste freie Nummer): `chat_tasks`, `chat_task_questions`; ggf.
`chat_threads.origin`, falls 066 noch nicht gemergt ist; Spalten für Token-Summen an `chat_messages`
(`turn_tokens_in`, `turn_tokens_out`); `HOLZI_TRIGGER_VERSION` +1. Geräteeinstellungen
`orchestration.*` ([data-model.md](./data-model.md)).

**Testing**:

- **Rust, Einheit**: `chat/turn_slots`, `orchestrator/{dispatch,scheduler,relay,report,prompt,
targets,stages}` je mit `*_tests.rs`.
- **Rust, Integration**: `tests/chat_parallel_turns.rs` (zwei Antworten gleichzeitig mit Skript-Modellen;
  Abbruch und Freigaben pro Antwort), `tests/orchestrator_tasks.rs` (Skript-Orchestrator und
  Skript-Ausführer: Übergabe, Rückfrage aus Auftrag, Freigabe über Nutzer, Bericht, drei parallele
  Aufgaben, Abbruch einer, Vault-Schließen, Geheimnis im Auftrag abgelehnt).
- **Messung (Release, `--ignored`)**: `tests/orchestrator_routing_eval.rs` (40 Aufgaben, SC-002,
  SC-003) auf Basis des Tool-Eval aus 032.
- **Prüfskripte**: `scripts/check-orchestration-settings.ts` (Einstellungen in `guardrails`, nur
  Modelle mit Werkzeugen), `scripts/check-chat-state.ts` erweitert um mehrere Ströme.
- **End-to-End**: Szenario `orchestrator` mit zwei Skript-Modellen über einen lokalen
  OpenAI-kompatiblen Testserver.
- **Von Hand**: [quickstart.md](./quickstart.md) M1–M10.
- **Regression**: alle Chat-Turn-Tests, `cli_delegate`-Tests, `model_jobs`-Tests, Tool-Eval 032.

**Target Platform**: alle; Delegates nur, wo `PlatformCapabilities` sie anbietet (Desktop).

**Performance Goals**: Quittung ≤ 2 s lokal / ≤ 1 s Cloud (SC-001); Antwort ohne Aufgabe wie 066
SC-002; Stufenwahl ≥ 85 % (SC-003); Abbruch einer Aufgabe ≤ 1 s (SC-005); Mehrkosten ≤ 20 % (SC-006).

**Constraints**:

- **Freigaben**: Entscheidung nur `permission::decide` + Nutzer; kein Orchestrator-Werkzeug dafür.
- **Geheimnisse**: Auftrag, Bericht und Rückfragen ohne Geheimnisse; Prüfung vor dem Speichern.
- **Async**: Antworten über `VaultGate::spawn`; keine blockierende Arbeit auf dem Executor.
- **Code-Regeln**: Dateien ≤ 500 Zeilen. `chat/commands.rs` (743) gibt Code ab; `tool_round.rs` (499)
  bekommt nur die `turn_id` an Freigaben und Auswahlen, Logik in neue Dateien.
- **Kompatibilität**: Orchestrierung aus = Verhalten wie heute (FR-001), durch die vorhandenen Tests
  abgesichert.

**Scale/Scope**: Umbau `chat` (Antwort-Plätze), neues Modul `orchestrator` (≈ 10 Dateien), 1 Migration,
6 Commands, 2 Ereignisse, 5 Werkzeuge, Aufgabenkarte, Einstellungsseite, 2 Integrations- und 1 Messtest.

## Constitution Check

| Prinzip / Vorgabe                                 | Status | Begründung                                                                                        |
| ------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------- |
| I–V                                               | ✅     | Keine Geheimnisse, Pfade, Identitäten oder Quellen berührt                                        |
| VI Selbstverändernde Anweisungen review-pflichtig | ✅     | Orchestrator-Systemtext ist Code im Repo; Einstellungen für Agenten gesperrt                      |
| VII Relay-Ausfall blockiert lokale Arbeit nicht   | ✅     | Standard lokal; Cloud-Orchestrator fällt auf lokal zurück                                         |
| VIII Keine Verheimlichung                         | ✅     | Jede Aufgabe mit Stufe, Modell, Begründung und Verbrauch sichtbar; Cloud dauerhaft gekennzeichnet |
| Phasendisziplin                                   | ✅     | 032, 012, 007/009, 055 im Einsatz                                                                 |
| ADR                                               | ✅     | ADR-0021 (proposed): parallele Antworten und Orchestrator ohne Freigabe-Recht                     |
| Keine neue Crate, 500-LoC, Tests, Async           | ✅     | Siehe oben                                                                                        |
| MCP-over-agent-Linie                              | ✅     | Kein eigener Programmier-Agent; Programmierarbeit an Delegates                                    |

### Kandidaten

| #   | Neues Artefakt        | Vorhandener Kandidat                            | Entscheidung                                 |
| --- | --------------------- | ----------------------------------------------- | -------------------------------------------- |
| K1  | Modell pro Antwort    | `model_jobs::ModelSource`, `Providers::adapter` | Wiederverwenden                              |
| K2  | Lokalen Platz teilen  | `model_jobs/{local_slot,chat_slot}.rs`          | Erweitern um zwei Sequenzen                  |
| K3  | Freigabe-Entscheidung | `permission::decide`, `approval_bridge`         | Unverändert                                  |
| K4  | Rückfrage an Nutzer   | `ask_user`, `pending_choices`                   | Unverändert, mit `turn_id`                   |
| K5  | Schwärzung            | `mcp_server::redact`                            | Für Auftrag und Bericht                      |
| K6  | Werkzeug-Eval         | `tests/model_tool_eval.rs` (032)                | Fixture-Satz für die Stufenwahl              |
| K7  | Aufgaben-Registry     | `model_jobs::state::JobRegistry`                | Muster für `TaskScheduler`, aber persistiert |

## Project Structure

```text
specs/068-task-orchestration/ spec.md, plan.md, research.md, data-model.md, quickstart.md,
                              contracts/orchestrator.md, checklists/requirements.md, tasks.md
docs/adr/0021-parallel-turns-and-orchestrator.md

src-tauri/src/chat/turn_slots.rs, chat/send.rs (oder Erweiterung von 066), chat/tokens.rs
src-tauri/src/orchestrator/
├── mod.rs, commands.rs, settings.rs, stages.rs
├── prompt.rs        # Systemtext, Stufenliste, Aufgabenstand
├── tools.rs         # task_dispatch, task_cancel, task_status, task_answer, task_report
├── scheduler.rs     # Warteschlange, Grenze, Start der Ausführer
├── relay.rs         # emit-Senke der Ausführer, Rückfragen, Bericht
├── targets.rs       # gleiches Ziel (R8)
└── storage.rs       # chat_tasks, chat_task_questions
src-tauri/src/adapters/cli_delegate/{claude.rs,codex.rs}   # workdir
src-tauri/tests/chat_parallel_turns.rs, orchestrator_tasks.rs, orchestrator_routing_eval.rs

src/composables/useChatTranscript.ts            # mehrere Ströme
src/components/chat/TaskCard.vue, MessageList.vue
src/components/settings/OrchestrationSetting.vue
src/stores/tasks.ts
```

## Lieferung

| PR  | Inhalt                                                                                                    | Stories            |
| --- | --------------------------------------------------------------------------------------------------------- | ------------------ |
| A   | Dokumente, ADR-0021 (proposed)                                                                            | –                  |
| B   | Antwort-Plätze, `RwLock`, Abbruch/Freigaben pro Antwort, Modell pro Antwort, Token-Summen, mehrere Ströme | – (unsichtbar)     |
| C   | Orchestrator, Stufen, eine Aufgabe, Bericht, eigene Antworten, Einstellungen, Aufgabenkarte, Migration    | US1, US3, US5, US6 |
| D   | Rückkanal: Freigaben und Auskünfte über den Orchestrator, Antwort aus dem Auftrag                         | US2                |
| E   | Mehrere Aufgaben: Grenze, Warteschlange, Abbruch einzeln, gleiches Ziel                                   | US4                |
| F   | Delegates im Projektordner                                                                                | US1 (Delegate)     |
| G   | Übersteuern und Wiederholen                                                                               | US7                |

**Abhängigkeiten**: B → C → D; E, F, G nach C. 066 PR E nutzt nach 068 C die Aufgaben; 067 PR F die
Stufenwahl für die Auswertung.

## Complexity Tracking

| Abweichung                        | Warum nötig                                                   | Einfachere Alternative verworfen, weil                         |
| --------------------------------- | ------------------------------------------------------------- | -------------------------------------------------------------- |
| Umbau des Chat-Kerns (R1)         | Parallele Aufgaben und ein antwortender Orchestrator          | Globaler Zustand erlaubt nur eine Antwort                      |
| Verborgene Ausführer-Unterhaltung | Ausführer-Verlauf getrennt vom Gespräch, aber nachvollziehbar | Ausführer im Gespräch würde den Orchestrator-Kontext überladen |
