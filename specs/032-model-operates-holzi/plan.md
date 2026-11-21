# Implementation Plan: Modell bedient haex-vault über die Aktionen

**Branch**: `032-model-operates-holzi` | **Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/032-model-operates-holzi/spec.md`

## Summary

Der Werkzeug-Ablauf im Chat (Spec 003, Rust) kennt heute nur ein Werkzeug,
`run_command`. Die Aktionen des Window Managers und der Einstellungen (Spec
020/023, TypeScript) sind beschrieben und für Agenten vorbereitet, aber kein
Modell erreicht sie. 032 verbindet beides und ergänzt, was kleine lokale
Modelle dafür brauchen. Es entsteht kein eigener Agent.

Technischer Ansatz (Begründungen und verworfene Alternativen in
[research.md](./research.md)):

- **Brücke** (R1, R2): Das Frontend bleibt einzige Quelle der Definitionen und
  schiebt sie per `set_agent_actions` nach Rust. Dort wird je Aktion ein
  `ActionTool` im bestehenden `ToolRegistry` angelegt. Ausgeführt wird über
  ein Ereignis `action-call-request` und den Command `respond_action_call`
  (Muster der Freigabe-Antworten) in einem **globalen** Frontend-Listener, der
  `runAction(…, { kind: 'builtinAgent' })` aufruft.
- **Freigabe** (R4): `RiskClass` bekommt die Stufe `Change`. `read` → `Safe`,
  `write` → `Change`, `destructive` → `Risky`; Auto lässt lesen und ändern
  laufen, Plan nur lesen. Das Katalogfeld `builtinAgentCallable` schließt die
  selbstbezüglichen Chat-Aktionen aus (R5). Downloads gelten wie jede
  Änderung; die Steuerung großer Downloads und des Dateisyncs auf Mobilgeräten
  kommt mit einer eigenen Spec.
- **Werkzeug-Angebot** (R6): ein einziges Verfahren für alle Modelle: ein
  festes Kernangebot (neun häufige Aktionen plus `find_actions`, zusammen
  höchstens 10), in jedem Schritt gleich und unabhängig vom Nutzertext. Alles
  andere sucht das Modell selbst mit `find_actions`; höchstens 5 eindeutige
  Nicht-Kern-Treffer werden ab dem nächsten Schritt derselben Antwort
  angeboten, sodass höchstens 15 Aktionswerkzeuge vorliegen. `run_command` und
  MCP-Werkzeuge bleiben zusätzlich erhalten und zählen nicht in dieses Limit.
  Kein Raten anhand des Nutzersatzes, keine Sonderwege je Anbieter.
- **Fähigkeit „Werkzeugnutzung“** (R7–R9): `ModelCapabilities.tool_use`
  (unbekannt / unterstützt / nicht unterstützt) ohne Migration; Anthropic
  setzt „unterstützt“, lokale Modelle durchlaufen eine Vorlagenprobe
  (`tokenize` mit/ohne Dummy-Werkzeug) und danach einen kurzen
  Hintergrund-Selbsttest.
- **Messlauf** (R9): `chat/eval/` mit eingebettetem, versioniertem
  Beispielsatz-Satz, reiner Bewertung und einem Runner gegen jeden
  `ProviderAdapter`; kein Tresor nötig, keine Aktion wird ausgeführt.
- **Hinweise** (R11, R14): `chat-tool-availability` je Zug, Banner im
  Frontend, einmal je Unterhaltung und Zustand pro App-Sitzung; CLI-Delegates
  bekommen keine Werkzeuge und den Delegate-Hinweis.
- **Keine neuen Abhängigkeiten**, keine Migration, keine MCP-Schicht im Chat.
  ADR-0006 hält „eine Definition, zwei Eingänge“ fest (R15).

## Technical Context

**Language/Version**: Rust (Tauri 2.11, Cargo-Features `llm-cpu` Standard,
`llm-cuda`/`llm-metal` optional); TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2
(SPA), Node 22.19 für die Prüfskripte

**Primary Dependencies**: nur Vorhandenes — `mistralrs` 0.8.1 (lokal, hinter
`llm-cpu`), `async-trait`, `tokio`, `serde_json`, `uuid`, `tokio-util`
(Abbruch-Token); Frontend: Pinia, `@nuxtjs/i18n`, haex-ui-Layer

**Storage**: keine Migration. Neues Feld `toolUse` in der vorhandenen
`models.capabilities_json` (CRDT-Tabelle, tresorweit, R7); Katalog
`model_catalog.json` bekommt das optionale Feld `tool_use`. Zwei eingebettete
JSON-Dateien (`eval_set.json`, `tools.json`)

**Testing**: Rust — Einheitstests in `*_tests.rs` (`permission_tests.rs`,
`offer_tests.rs`, `action_tool_tests.rs`, `scoring_tests.rs`), Integration in
`src-tauri/tests/` (`action_bridge.rs`, erweitertes
`chat_tool_loop_permissions.rs`), Messlauf `model_tool_eval.rs`
(`#[ignore]`, `HAEX_TEST_GGUF` bzw. `HAEX_EVAL_*`); Frontend — neues
`pnpm check:agent-actions` (Vorbild `check-wm-actions.ts`), Regression
`check:wm-navigation`, `check:chat-state`, `check:templates`, `typecheck`,
`typecheck:scripts`, `lint`, `format:check`; manuell nach
[quickstart.md](./quickstart.md)

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows); der Code hinter
`llm-cpu` ist mit `#[cfg(feature = "llm-cpu")]` zu klammern, CI prüft
Standard und `--no-default-features`

**Project Type**: desktop-app (Nuxt-SPA-Frontend + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: Suche über ≈ 60 Aktionen < 5 ms (reine Wortsuche); Probe < 2 s; Selbsttest ≤ 2 Minuten (nur Prozessor, Qwen3-4B) und
ohne den Chat zu blockieren (SC-002a); ein Aktionsumlauf ohne Nutzerfreigabe
< 100 ms Overhead über dem Handler selbst

**Constraints**: Dateien ≤ 500 Zeilen (`chat/commands.rs` steht bei 733 mit
dokumentierter Ausnahme → neue Commands in eigener Datei
`chat/action_commands.rs`; `tool_round.rs` 456 bleibt unberührt);
Testcode in eigenen Dateien; `src/lib/actions/*` bleibt reines TS mit
relativen `.ts`-Importen (Node-Harness); keine `unwrap`/`expect` auf
Eingabedaten; Backend gibt keine lokalisierten Texte aus (Frontend
übersetzt Schlüssel); Werkzeugnamen `^[A-Za-z0-9_-]{1,64}$`

**Scale/Scope**: 64 Aktionen im Katalog, davon 54 für Agenten aufrufbar,
nach Ausschluss der drei selbstbezüglichen 51 für den eingebauten Agenten;
Beispielsatz-Satz ≈ 28 Sätze (Deutsch/Englisch), davon ≈ 5 für den
Selbsttest

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` und die spaex-Constitution
`.spaex/constitution.md`.

| Prinzip / Vorgabe                                                          | Status | Begründung                                                                                                                                              |
| -------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                                                 | ✅     | Messlauf liest Schlüssel aus der Umgebung, nie aus Dateien; Geheimnis-Stichprobe im Prüfskript (R13); keine Schlüssel im Schnappschuss                  |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration            | ✅     | Quickstart und Skripte nutzen Platzhalter und repo-relative Pfade                                                                                       |
| III Projektidentität geräteunabhängig                                      | ✅     | Berührt nicht                                                                                                                                           |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                   | ✅     | Keine neue Cross-Repo-Referenz                                                                                                                          |
| V Externe Quellen nur per Opt-in                                           | ✅     | Keine neue Quelle                                                                                                                                       |
| VI Selbstverändernde Anweisungen review-pflichtig                          | ✅     | Keine Änderung an Constitution, Skills oder Berechtigungen des Werkzeugs; die Leitplanken bleiben für den eingebauten Agenten gesperrt (FR-003, FR-010) |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                            | ✅     | Rein lokal; Cloud-Modelle sind Wahl des Nutzers                                                                                                         |
| VIII Keine Verheimlichung in Agent-Ausgaben                                | ✅     | Jede Aktion des Modells erscheint im Verlauf; Hinweise benennen Einschränkungen                                                                         |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits, kein Squash | ✅     | specify → clarify → plan → tasks → implement; Topic-Branch im Worktree                                                                                  |
| ADR bei prinzipienrelevanter Entscheidung                                  | ✅     | ADR-0006 „Aktionen als Werkzeuge des eingebauten Agenten“ (Aufgabe in tasks); 0005 bleibt für Spec 021 reserviert                                       |
| Test-Code in separaten Dateien                                             | ✅     | `*_tests.rs` per `#[path]`, `src-tauri/tests/`, `scripts/check-agent-actions.ts`                                                                        |
| Worktree je Änderung                                                       | ✅     | `.worktrees/032-model-operates-holzi`                                                                                                                   |
| 500-LoC-Grenze                                                             | ⚠️     | Neue Dateien bleiben darunter; `chat/commands.rs` (733, dokumentierte Ausnahme) bekommt nur wenige Zeilen, neue Commands liegen in `action_commands.rs` |
| Graphify vor neuen benannten Artefakten                                    | ✅     | Aufgabe T001: Abfragen für `ActionBridge`, `core_offer`, `ToolUse`, `agentTools` vor dem ersten neuen Namen                                             |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                          | ✅     | Geplant an der wortbasierten Suche (R6), Hinweis nach Neustart (R11), Platzhalter-Schwelle des Selbsttests (R9), Selbsttest ohne Abbruch bei Chat-Start |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                  | ✅     | `check:agent-actions`, Rust-Tests, `model_tool_eval`                                                                                                    |
| Keine Selbstreferenzen von Agenten in Artefakten/Commits                   | ✅     | Commits und PR ohne Agent-Zusätze (haex-vault-Regel)                                                                                                    |
| Phasen-Disziplin                                                           | ✅     | Baut auf 003 und 020, beide gemerged und im Einsatz; 021 folgt nach 032                                                                                 |

**Ergebnis vor Phase 0**: kein unbegründeter Verstoß; ein ⚠️ dokumentiert.

**Ergebnis nach Phase 1**: unverändert. Das Design fügt keine Abhängigkeit, keine
Migration und keine neue Tabelle hinzu. Drei neue Tauri-Commands/Ereignisse
folgen dem vorhandenen Freigabe-Muster; `RiskClass` wächst um eine Stufe, die
mehrere Stellen mitziehen muss (Complexity Tracking).

## Project Structure

### Documentation (this feature)

```text
specs/032-model-operates-holzi/
├── plan.md                        # dieser Plan
├── research.md                    # Phase 0: Entscheidungen R1–R16
├── data-model.md                  # Phase 1
├── quickstart.md                  # Phase 1
├── contracts/
│   ├── tauri-commands.md          # Commands, Ereignisse, Meta-Werkzeug
│   └── eval-format.md             # Beispielsatz-Satz, Bewertung, Bericht
├── checklists/requirements.md
└── tasks.md                       # Phase 2 (/speckit-tasks)
```

### Source Code (Repository-Wurzel)

```text
src-tauri/src/
├── chat/
│   ├── tools/
│   │   ├── mod.rs                 # RiskClass + Change, Quelle "action"          [ändern]
│   │   ├── permission.rs          # decide(): 3×3-Matrix                          [ändern]
│   │   ├── action_tool.rs         # ActionTool (Tool-Implementierung)             [neu]
│   │   ├── action_bridge.rs       # Umlauf, Zeitgrenze, Sperre                    [neu]
│   │   ├── find_actions.rs        # Suchwerkzeug find_actions                     [neu]
│   │   ├── offer.rs               # Kernangebot, Suche, extend_offer              [neu]
│   │   ├── selftest.rs            # Hintergrund-Selbsttest                        [neu]
│   │   └── *_tests.rs             # permission, offer, action_tool, selftest
│   ├── eval/
│   │   ├── mod.rs, scoring.rs, runner.rs, *_tests.rs                               [neu]
│   │   ├── eval_set.json          # versionierter Satz                            [neu]
│   │   └── tools.json             # Katalog-Schnappschuss                         [neu]
│   ├── action_commands.rs         # set_agent_actions, respond_action_call        [neu]
│   ├── commands.rs                # send_message: Angebot, Availability           [ändern, klein]
│   ├── events.rs                  # risk_class_str, neue Ereignisse               [ändern]
│   ├── session.rs                 # ChatState.action_bridge, reset_for_close      [ändern]
│   └── model_loading.rs           # Probe + Selbsttest beim ersten Laden          [ändern]
├── llm/local/probe.rs             # Vorlagenprobe (cfg llm-cpu)                   [neu]
├── model_capabilities.rs          # ToolUse                                        [ändern]
├── adapters/anthropic_capabilities.rs     # Supported/Provider                     [ändern]
├── adapters/cli_delegate/approval_bridge.rs   # match um Change ergänzt            [ändern]
├── storage/models.rs              # set_tool_use                                   [ändern]
├── catalog/mod.rs, model_catalog.json     # optionales tool_use                    [ändern]
└── lib.rs                         # Emitter in setup(), generate_handler           [ändern]

src-tauri/tests/
├── action_bridge.rs               # Umlauf mit Ersatz-Frontend                    [neu]
├── model_tool_eval.rs             # Messlauf (#[ignore])                          [neu]
├── chat_tool_loop_permissions.rs  # 3×3-Matrix                                     [ändern]
└── common/tool_loop_fixture.rs    # ActionTool-Hilfen                              [ändern]

src/
├── lib/actions/
│   ├── types.ts                   # builtinAgentCallable               [ändern]
│   ├── agentTools.ts              # toToolName, toAgentActionDef                  [neu]
│   ├── runner.ts                  # builtinAgentCallable-Prüfung                  [ändern]
│   ├── settingsActions.ts, chatActions.ts     # Felder setzen                     [ändern]
├── composables/useAgentActions.ts # Listener, set_agent_actions                   [neu]
├── composables/useChat.ts, useModels.ts       # Typen, Ereignisse                 [ändern]
├── components/chat/PermissionPrompt.vue, StatusBanners.vue   # Klartext, Hinweis [ändern]
└── i18n/locales/{de,en}.json      # chat.toolNotice.*, chat.permission.change    [ändern]

scripts/
├── check-agent-actions.ts         # neu
└── export-eval-tools.ts           # neu

docs/adr/0006-actions-as-builtin-agent-tools.md    # neu
package.json, .github/workflows/ci.yml             # check:agent-actions           [ändern]
```

**Structure Decision**: Erweiterung der vorhandenen Module; neue Dateien je
Verantwortung (Umlauf, Werkzeug, Angebot, Selbsttest, Messlauf), damit
`commands.rs` und `tool_round.rs` nicht wachsen. Das Frontend verändert den
Katalog nur um zwei optionale Felder, die Logik (Namen, Übergabe) liegt in
einer neuen reinen TS-Datei.

## Complexity Tracking

| Verstoß / Aufwand                                           | Warum nötig                                                                      | Einfachere Alternative verworfen, weil                                                                                             |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `RiskClass` um `Change` erweitern (≈ 10 Stellen, Rust + TS) | FR-007 verlangt „Auto erlaubt Ändern, Plan blockiert“; zwei Stufen reichen nicht | Zwei Stufen plus Sonderfall im Modus würden die Logik in `decide()` und im Dialog verstreuen                                       |
| Ein neues optionales Katalogfeld (`builtinAgentCallable`)   | Chat-Aktionen würden sich im laufenden Zug selbst aufrufen (R5)                  | `agentCallable` auf `false` sperrt sie auch für Spec 021; eine versteckte Ausschlussliste im Bridge-Code wäre eine zweite Wahrheit |
| `chat/commands.rs` bleibt über 500 Zeilen                   | Bestehende dokumentierte Ausnahme; 032 fügt nur wenige Zeilen hinzu              | Eine Aufspaltung gehört in eine eigene Änderung und ist kein Ziel von 032                                                          |

## Bewusste Grenzen (aus research.md)

- Constrained Decoding bleibt außen vor, bis der Messlauf zeigt, dass es nötig
  ist (R10).
- Liegt die Aktion nicht im Kernangebot, braucht die Antwort einen zusätzlichen
  Schritt (Suche), bei lokalen Modellen ein weiterer Modelllauf; der Messlauf
  weist das aus (`reachRate`, R6).
- Die Hinweise erscheinen nach einem App-Neustart erneut (R11, entschieden).
- Die Selbsttest-Schwelle ist ein Platzhalter bis zum ersten Messlauf (R9).
- Die Mindestquote, die endgültige Zusammensetzung des Kernangebots und die
  Aufnahme größerer lokaler
  Modelle in den Katalog entscheiden sich nach dem ersten Messlauf (R16).
