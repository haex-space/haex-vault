# Implementation Plan: Agent-Rückfrage mit Auswahl

**Branch**: `046-agent-choice-prompt` | **Date**: 2026-10-08 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/046-agent-choice-prompt/spec.md`

## Summary

Der eingebaute Agent öffnet Apps auch bei Tippfehlern und fragt nach, statt abzulehnen. Eine Aktion,
die ihre Eingabe nicht eindeutig zuordnen kann, meldet `needs_choice` mit Kandidaten; die Tool-Runde in
Rust stellt daraufhin selbst eine Rückfrage im Chat und führt die Aktion mit der Wahl erneut aus. Das
Modell muss dafür keinen weiteren Schritt machen. Für allgemeine Unklarheit bekommt das Modell das
Werkzeug `ask_user` über denselben Weg.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Rückfrage in der Tool-Runde** (R1, R2): neues Modul `chat/turn/choices.rs` mit `resolve_choices`
  zwischen `execute_plans` und dem Cancel-Check; `PendingChoices` auf `ChatState` nach dem Muster der
  Freigaben; Event `chat-choice-request`, Command `respond_choice`.
- **Transport** (R3, R4): `ToolResult.choice`, `ActionReply::NeedsChoice`, Code `needs_choice` mit
  `options` durch Runner, Wire und Bridge; Endergebnis mit Vermerk im gespeicherten `content`.
- **App-Auflösung** (R5, R8): reines Modul `src/lib/wm/appMatch.ts` (Editierdistanz, `fold`), `ActionChoiceError`
  in `wmActionHandlers.ts`, `t` für Titel der System-Apps.
- **`ask_user`** (R6): eingebautes Werkzeug, immer angeboten, nie freigabepflichtig, Satz im Systemprompt.
- **UI** (R9, R10): eine Warteschlange `pendingPrompts` für Freigaben und Rückfragen; neue Komponente
  `ChoicePrompt.vue` mit `ShadcnRadioGroup` aus haex-ui (PR #337).
- **Prüfung** (R11): Eval-Set v3 mit `Kind::Clarify`, `ask_user` im Eval-Angebot und in der Schema-Suche.

## Technical Context

**Language/Version**: Rust (Edition des Crates, `src-tauri`), TypeScript (strict), Vue 3.5, Nuxt 4 (SPA)

**Primary Dependencies**: vorhanden — `tokio` (oneshot, `select!`), `serde`, `uuid`,
haex-ui-Layer (`UiDrawerModal`, `UiButton`, `UiInput`); keine neue Abhängigkeit, keine neue
Tauri-Berechtigung

**Storage**: keine Änderung; Ergebnis im `content` der bestehenden `tool_result`-Zeile
([data-model.md](./data-model.md))

**Testing**: `cargo test` (neu `chat/turn/choices_tests.rs`, `chat/tools/ask_user_tests.rs`,
Erweiterungen in `action_bridge_tests.rs`, `action_tool_tests.rs`, `prompt_tests.rs`, `offer_tests.rs`,
`eval/scoring_tests.rs`; Integration `src-tauri/tests/chat_tool_loop_choices.rs` mit
`tests/common/tool_loop_fixture.rs`), `cargo clippy`, `cargo fmt --check`; `pnpm check:wm-navigation`
(Runner), `pnpm check:agent-actions` (Wire), neu `scripts/check-wm-app-match.ts` (in
`check:wm-navigation`), `pnpm check:chat-state` (Warteschlange), `pnpm check:templates`,
`pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`

**Target Platform**: Linux, macOS, Windows, Android, iOS (Chat-Drawer auf kleinen Bildschirmen)

**Project Type**: Desktop- und Mobil-App (Tauri)

**Performance Goals**: App-Auflösung < 10 ms bei ~50 Apps; Rückfrage erscheint ohne spürbare Verzögerung
nach dem Aufruf

**Constraints**: offline-fähig; keine Rückfrage ohne Zeitlimit-Abbruch durch die Bridge (FR-011);
abgebrochene Runde hinterlässt keine Zeilen (bestehende Invariante)

**Scale/Scope**: 1 neues Rust-Modul + 1 Tool, 1 neues TS-Modul, 1 Komponente, 1 Composable; Änderungen an
Tool-Runde, Bridge, Runner, WM-Handlern, Chat-Warteschlange, Eval

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

`.specify/memory/constitution.md` (I–VIII) und `.spaex/constitution.md`:

| Prinzip                                                      | Bewertung                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I No secrets in git                                          | ✅ keine Geheimnisse berührt; Rückfragen tragen nur App-Namen und -IDs, die das Modell über `wm_apps_list` ohnehin sieht                                                                                                                                                                                                       |
| II No local absolute paths                                   | ✅ keine                                                                                                                                                                                                                                                                                                                       |
| III–V Identity / pinned refs / opt-in                        | ✅ nicht berührt                                                                                                                                                                                                                                                                                                               |
| VI Self-modifying instructions                               | ✅ Der Systemprompt-Satz (R6) ist Produktcode des eingebauten Agenten, keine Agent-Konfiguration dieses Repos; er landet per PR                                                                                                                                                                                                |
| VII Relay unavailability                                     | ✅ rein lokal                                                                                                                                                                                                                                                                                                                  |
| VIII No concealment                                          | ✅ Rückfrage und Antwort bleiben im Verlauf sichtbar (FR-010)                                                                                                                                                                                                                                                                  |
| spaex: worktree on topic branch                              | ✅ `.worktrees/046-agent-choice-prompt`, Branch `046-agent-choice-prompt` von `origin/main`                                                                                                                                                                                                                                    |
| spaex: speckit-workflow-adherence                            | ✅ specify → (Review) → plan → tasks → implement                                                                                                                                                                                                                                                                               |
| spaex: pr-required-for-main, conventional commits, no squash | ✅ Topic-Branch, PR                                                                                                                                                                                                                                                                                                            |
| spaex: graphify-first-authoring                              | ⚠️ Graph vom 2026-09-15 liefert nur Fremdtreffer (research, Kopf); Kandidaten per Code-Suche geprüft und wiederverwendet: Freigabe-Muster, `ActionBridge`, `PermissionPrompt`, native Radios, `fold`/Fuse. Nachprüfung mit frischem Graphen vor `/speckit-implement`                                                           |
| spaex: ponytail / laziness ladder                            | ✅ keine neue Abhängigkeit; eine Warteschlange statt zwei (R9); kein neuer Trait-Hook für ein Tool (R6); keine neue Zeilenart in der Historie (R4)                                                                                                                                                                             |
| spaex: tests in separate files                               | ✅ `*_tests.rs`, `scripts/check-*.ts`                                                                                                                                                                                                                                                                                          |
| spaex: 500 LoC                                               | ✅ mit Ausweichen: `tool_round.rs` (462) → neues `choices.rs`; `session.rs` (493) → ein Feld `PendingChoices`; `useChat.ts` (591, schon darüber) → neues `useChatChoices.ts`; `ChatApp.vue` (495) → nur Registrierung; `commands.rs` (739, schon darüber) → `respond_choice` in `choices.rs` bzw. eigenem `choice_commands.rs` |
| spaex: 100/1000-Zeilen-Diff                                  | ⚠️ Gesamt deutlich > 100 Zeilen. Aufteilung in Commits je User Story; die Umbenennung `pendingApprovals` → `pendingPrompts` und das Verschieben von `fold` als eigene mechanische Commits                                                                                                                                      |
| spaex: phasing-discipline                                    | ✅ Erweiterung von Spec 032 (eingebauter Agent bedient haex-vault), das im täglichen Einsatz ist                                                                                                                                                                                                                               |
| ADR nötig?                                                   | Nein — keine Core-Principle-Änderung                                                                                                                                                                                                                                                                                           |

**Post-Design Re-Check**: ✅ bestanden. Eine dokumentierte Abweichung vom Spec: nicht verfügbare Apps
sind als Kandidat nicht wählbar statt „führen zum selben Fehler“ (R8, Spec angepasst). Abhängigkeit:
PR #336 (`ActionInputError`) und PR #337 (`ShadcnRadioGroup`, haex-ui-Pin) müssen vor der Umsetzung
gemergt sein.

## Project Structure

### Documentation (this feature)

```text
specs/046-agent-choice-prompt/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/choice-contract.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/chat/
├── turn/
│   ├── tool_round.rs               # resolve_choices aufrufen; ask_user in plan_calls immer Allow
│   ├── choices.rs                  # neu: PendingChoices, resolve_choices, respond_choice
│   └── choices_tests.rs            # neu
├── tools/
│   ├── mod.rs                      # ToolResult.choice, ChoiceRequest, ChoiceOption
│   ├── ask_user.rs                 # neu: AskUserTool
│   ├── ask_user_tests.rs           # neu
│   ├── action_bridge.rs            # ActionReply::NeedsChoice, Wire-Feld options
│   ├── action_tool.rs              # into_tool_result → needs_choice
│   ├── offer.rs                    # ask_user im Core-Angebot
│   └── prompt.rs                   # Satz „ask instead of refusing“
├── action_commands.rs              # AskUserTool registrieren, Name reservieren
├── commands.rs                     # abort_turn: offene Rückfragen verwerfen
├── session.rs                      # ChatState.pending_choices, reset_for_close
├── events.rs                       # EVENT_CHOICE_REQUEST, ChoiceRequestEvent
└── eval/{eval_set.json,scoring.rs,runner.rs}   # v3, Kind::Clarify, ask_user
src-tauri/src/lib.rs                # respond_choice registrieren
src-tauri/tests/chat_tool_loop_choices.rs        # neu

src/
├── lib/
│   ├── search/fold.ts              # neu (aus passwords/search.ts verschoben)
│   ├── passwords/search.ts         # importiert fold
│   ├── wm/appMatch.ts              # neu: matchApp
│   └── actions/{types.ts,runner.ts,agentTools.ts,chatActions.ts}  # needs_choice, ActionChoiceError, Wire, chat.choice.answer
├── stores/wmActionHandlers.ts      # matchApp, t-Parameter
├── plugins/actions.client.ts       # t an registerWmActionHandlers
├── composables/
│   ├── useChatChoices.ts           # neu: Listener, respondChoice
│   ├── useChatTranscript.ts / useComposer.ts / useThreadSidebar.ts / useChatTab.ts  # pendingPrompts
│   └── useChat.ts                  # Typen PendingPrompt
├── components/
│   ├── chat/ChoicePrompt.vue       # neu
│   ├── chat/Composer.vue           # Prompt nach kind
│   ├── chat/PermissionPrompt.vue   # nimmt PendingPrompt (kind approval)
│   ├── chat/MessageList.vue        # declined_by_user als Audit-Marker
│   └── apps/ChatApp.vue            # Registrierung onChoiceRequest, pendingPrompts
└── i18n/locales/{de,en}.json       # chat.choice.*, chat.autonomy.audit.declined_by_user

scripts/
├── check-wm-app-match.ts           # neu
├── check-wm-actions.ts             # ActionChoiceError → needs_choice
├── check-agent-actions.ts          # Wire mit options; tools.json neu exportiert
└── check-chat-state.ts             # Rückfrage in der Warteschlange, Thread-Wechsel, Turn-Ende
package.json                        # check:wm-navigation um check-wm-app-match.ts erweitern
```

**Structure Decision**: Bestehende Aufteilung — Turn-Logik unter `chat/turn/`, Werkzeuge unter
`chat/tools/`, reine Frontend-Logik in `src/lib/`, Chat-Zustand als Composables, Chat-Komponenten in
`src/components/chat/`.

## Complexity Tracking

Keine Verstöße. Der Gesamtumfang überschreitet die 100-Zeilen-Warnschwelle; die Aufteilung in Commits je
User Story und mechanische Vor-Commits steht im Constitution Check.
