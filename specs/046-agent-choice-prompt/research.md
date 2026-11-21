# Research: Agent-Rückfrage mit Auswahl

Alle Pfadangaben beziehen sich auf `main` @ `25c7a763` plus PR #337 (`refactor/radio-group`:
`ShadcnRadioGroup`, `SettingsGroup radio`) plus PR #336 (`fix/agent-unknown-app`:
`ActionInputError`, `unknownAppMessage`). Die Umsetzung beginnt erst, wenn #336 und #337 gemergt sind.

Graphify-Abfrage („agent asks user pending approval prompt oneshot“, „fuzzy search fuse threshold“):
Der Graph stammt vom 2026-09-15 und liefert nur Fremdtreffer (CLI-Delegates, Anthropic-Tests). Die
Kandidaten wurden per Code-Suche geprüft: `pending_tool_approvals` (Muster), `ActionBridge` (Muster),
`PermissionPrompt.vue` (Muster), `ShadcnRadioGroup` (haex-ui, PR #337),
`src/lib/passwords/search.ts` (`fold`, Fuse-Optionen). Nachprüfung mit frischem Graphen vor
`/speckit-implement`.

**Nachprüfung (T002, 2026-10-08, `graphify update .` → 19 694 Knoten, je Abfrage Budget 1000)**:

| Neues Artefakt                         | Abfrage                                                      | Kandidaten und warum keiner passt                                                                                                                                                                                                                                        |
| -------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/lib/wm/appMatch.ts`               | „fuzzy match app name title resolve appId“                   | nur E2E-Hilfen (`scripts/e2e/lib/*`), `SettingsApp.vue`; keine App-Namenssuche vorhanden                                                                                                                                                                                 |
| `chat/turn/choices.rs`                 | „pending user answer oneshot request id resolve cancel turn“ | `sync/link/pending.rs` (Kopplungs-Wartezustand, kein Turn), `passwords/clipboard.rs` (`Pending` = Leeren der Zwischenablage); beides andere Domäne                                                                                                                       |
| `chat/tools/ask_user.rs`               | „built-in tool question options ask user“                    | `extensions/permissions/prompts.rs` (`Question`, `PermissionState`) und `src/lib/extensions/queue.ts`: Berechtigungsfragen von Erweiterungen mit Frames, 1004-Wiederholung, gemerkter Entscheidung Erlauben/Verweigern — anderer Lebenszyklus, keine Optionen, kein Turn |
| `src/components/chat/ChoicePrompt.vue` | „chat prompt dialog radio choose answer“                     | nur E2E-Hilfen; Muster bleibt `PermissionPrompt.vue` (R10)                                                                                                                                                                                                               |
| `src/composables/useChatChoices.ts`    | „listen tauri event chat request respond invoke composable“  | `vault_gate/invoke.rs`, Android-Plugin; Muster bleibt `onToolPermissionRequest` in `useChat.ts` (R9)                                                                                                                                                                     |

## R1 — Wo die Rückfrage gestellt wird: in der Tool-Runde, nicht im Tool

**Decision**: Die Rückfrage stellt `TurnRunner` in einem neuen Schritt `resolve_choices` zwischen
`execute_plans` und dem Cancel-Check in `tool_round` (`src-tauri/src/chat/turn/tool_round.rs:84-91`).
Er läuft über die ausgeführten Aufrufe der Runde, nacheinander. Trägt ein Ergebnis eine Rückfrage
(`ToolResult.choice`, R3), sendet er `chat-choice-request` mit `threadId` über `self.emit_event`, parkt
auf einem oneshot (Map `pending_choices` auf `ChatState`) und wartet mit
`select!{ biased; cancel, rx }`. Danach ersetzt er das Ergebnis des Aufrufs (R4). Der Code liegt in einem
neuen Modul `src-tauri/src/chat/turn/choices.rs`, weil `tool_round.rs` schon 462 Zeilen hat.

**Rationale**: `Tool::execute(input, cancel)` (`chat/tools/mod.rs:82-110`) kennt weder Thread noch
Emitter; die Freigabe-Abfrage sitzt aus demselben Grund in `plan_calls`. Nach `join_all` laufen keine
Aufrufe mehr parallel, die Rückfragen erscheinen also von selbst nacheinander (Edge Case „mehrere
Rückfragen“). Der Cancel-Check direkt danach (`tool_round.rs:91`) gilt unverändert: Eine abgebrochene
Runde hinterlässt keine Zeilen, auch wenn eine Rückfrage offen war (Edge Case „Turn abgebrochen“).
Das 60-s-Timeout der `ActionBridge` (`action_bridge.rs:29`) betrifft nur die Ausführung im Webview,
nicht das Warten auf den Nutzer (FR-011, ohne Zeitlimit).

**Alternatives considered**: Rückfrage im `ActionTool` selbst — verworfen, das Tool müsste Thread-ID und
Emitter bekommen (Trait-Änderung für alle Tools). Rückfrage im Frontend-Handler von `wm.app.open` —
verworfen, der Handler weiß nicht, aus welchem Chat er aufgerufen wird, und das Modell bekäme nicht das
Endergebnis.

## R2 — Zustand und Abbruch: eigene Map nach dem Muster der Freigaben

**Decision**: `ChatState` bekommt `pending_choices: Arc<Mutex<HashMap<Uuid, oneshot::Sender<ChoiceAnswer>>>>`
und `cancelled_choices: Arc<Mutex<HashSet<Uuid>>>`. `abort_turn` (`chat/commands.rs:620-669`) verschiebt
offene Rückfragen in die Tombstones wie bei `pending_tool_approvals`; `reset_for_close`
(`session.rs:~414-452`) leert beides. Um `session.rs` (493 Zeilen) unter 500 zu halten, fassen beide
Felder in einem Typ `PendingChoices` in `turn/choices.rs` zusammen (ein Feld auf `ChatState`).

**Rationale**: Gleiches Lebenszyklus-Verhalten wie Freigaben (spät eintreffende Antwort auf
abgebrochene Rückfrage ist kein Fehler). Ein gemeinsamer Typ mit den Freigaben würde deren Wert-Typ
ändern und alle Freigabe-Tests berühren.

**Alternatives considered**: Freigabe-Map generisch machen (`oneshot::Sender<PromptAnswer>`) — verworfen,
großer Diff in fremdem, getestetem Code für ~10 gesparte Zeilen.

## R3 — Wie eine Rückfrage zum Tool-Ergebnis kommt

**Decision**: `ToolResult` (`chat/tools/mod.rs:56-76`) bekommt `choice: Option<ChoiceRequest>`;
`ToolResult::ok/error` setzen `None`, neu `ToolResult::needs_choice(ChoiceRequest)`.
`ChoiceRequest { question: Option<String>, field: Option<String>, value: String, options: Vec<ChoiceOption> }`,
`ChoiceOption { value, label, unavailable: Option<String> }`.

- `ActionTool`: `ActionReply` bekommt die Variante `NeedsChoice { field, message, options }`
  (`action_bridge.rs:38-47`); `ActionOutcomeWire` das Feld `options` (`:62-74`); `From` bildet Code
  `needs_choice` darauf ab. `into_tool_result` (`action_tool.rs:122-145`) erzeugt
  `ToolResult::needs_choice` mit `field`, `value = input[field]`, `question: None`, `content` = die
  bisherige JSON-Fehlerform (für den Fall, dass niemand fragt, R7).
- `ask_user` (R6): `execute` erzeugt `ToolResult::needs_choice` mit `question: Some(...)`, `field: None`.

**Rationale**: Ein Weg für beide Herkünfte; `resolve_choices` unterscheidet nur „Feld vorhanden →
Aktion erneut ausführen“ und „kein Feld → Antwort ist das Ergebnis“.

**Alternatives considered**: Rückfrage als Fehlercode im `content`-String parsen — verworfen, stringly
typed.

## R4 — Antwort verarbeiten

**Decision**: `ChoiceAnswer = Option(value) | Text(text) | Cancel`.

- Mit `field` (Aktion): Option/Text → Eingabe kopieren, `input[field] = value|text`, Tool aus der
  Registry holen und `tool.execute(input, cancel)` erneut aufrufen, ohne `plan_calls` (FR-012: die Wahl
  ist die Zustimmung). Trägt das neue Ergebnis wieder eine Rückfrage, Schleife (FR-008, Acceptance US2/5).
  Cancel → `ToolResult::error("declined_by_user")`.
- Ohne `field` (`ask_user`): Option → `ok({"answer": value})`, Text → `ok({"answer": text, "freeText": true})`,
  Cancel → `error("declined_by_user")`.
- Das Endergebnis bekommt einen Vermerk `{"choice": {"value": <ursprüngliche Angabe>, "answer": ...}}`
  im `content` (bei Aktionen als Hülle `{"result": ..., "choice": ...}`), damit FR-010 aus dem
  gespeicherten Tool-Ergebnis lesbar ist.

**Rationale**: Die Tool-Zeilen werden erst nach der Runde gespeichert (`persist_round`,
`tool_round.rs:314-371`); das ersetzte Ergebnis landet also mit Vermerk in der Historie, ohne neue
Zeilenart. `declined_by_user` kommt in `DENY_AUDIT_MARKERS` (`MessageList.vue:48-51`) und bekommt einen
i18n-Text wie `denied_by_user`.

**Alternatives considered**: Synthetische `ask_user`-Aufrufzeilen in die Historie schreiben — verworfen,
der laufende Request müsste dazu Tool-Aufrufe enthalten, die das Modell nie gemacht hat
(`append_round_to_request`), was Anbieter mit strenger `tool_use`/`tool_result`-Paarung ablehnen.

## R5 — Tolerante App-Auflösung

**Decision**: Neues reines Modul `src/lib/wm/appMatch.ts`:
`matchApp(input, apps, titleOf): { kind: 'exact', appId } | { kind: 'choice', candidates }`.

1. Exakte ID, dann `resolveAppAlias`.
2. Normalisieren mit `fold` (Diakritika weg, klein); ein Präfix `system.`/`extension.` wird abgeschnitten,
   damit „system.notes“ wie „notes“ sucht (US1/4).
3. Exakter Treffer auf gefalteten Titel oder ID-Rest (`chat`, `settings`, …) → eindeutig.
4. Score je App über Titel und ID-Rest (0 = gleich, 1 = fern): steckt das eine im anderen,
   0,05–0,15 (kürzere Namen zuerst); sonst Editierdistanz (Vertauschung = ein Fehler) geteilt durch die
   längere Länge. Kandidaten bis 0,5. **Klarer Treffer**: bester Score ≤ 0,2 und (kein Zweiter oder
   Abstand zum Zweiten ≥ 0,1). Sonst `choice` mit den besten 5 (FR-004), auch leer.

   _Beim Umsetzen geändert (T006):_ Geplant war Fuse (`threshold: 0.4`, klar bei ≤ 0,25 / Abstand
   0,15). Gemessen gab Fuse „haex-mial“ für haex-mail und haex-files denselben Score 0,471, weil es den
   besten Teil-Treffer irgendwo im Text bewertet, nicht den Abstand der ganzen Namen. Die
   Editierdistanz trennt beide (0,11 gegen 0,4).

Titel: `app.title ?? t(app.titleKey)` in der aktuellen Sprache; der ID-Rest deckt die englischen Namen
der System-Apps ab. `registerWmActionHandlers(wm, t)` bekommt `t` wie `registerWmLayoutHandlers`
(`src/plugins/actions.client.ts:21-23`). `fold` wandert aus `src/lib/passwords/search.ts:19-24` nach
`src/lib/search/fold.ts` (Passwörter importieren von dort; mechanischer eigener Commit).

Die Schwellen werden in `scripts/check-wm-app-match.ts` an festen Fällen kalibriert: „haex-mial“ →
haex-mail eindeutig; „haex“ bei haex-mail/-notes/-files → Auswahl; „system.notes“ → haex-notes;
„einstellungen“ → system.settings; „Kalender“ ohne passende App → Auswahl mit ≤ 5 oder leer.

**Rationale**: App-Namen sind kurz und ein Wort; für Tippfehler zählt der Abstand der ganzen Namen.
~20 Zeilen Editierdistanz ohne neue Abhängigkeit. Reines Modul ⇒ mit `node --test` testbar.

**Alternatives considered**: Fuse wie in der Passwortsuche — verworfen nach Messung (siehe oben).
Wortweise Suche wie bei Passwörtern — verworfen, App-Namen sind ein Wort.

## R6 — `ask_user` als eingebautes Werkzeug

**Decision**: `src-tauri/src/chat/tools/ask_user.rs`, `AskUserTool`, Name `ask_user`, Eingabe
`{ question: string, options: string[2..5] }`, `source() = "action"`. Registriert in
`register_agent_actions` neben `FindActionsTool` (`action_commands.rs:78-80`), Name reserviert
(:54-58). `core_offer` (`offer.rs:27-37`) nimmt `ask_user` immer auf (FR-015). In `plan_calls`
(`tool_round.rs:156-220`) gilt für `ask_user` immer `Allow`, unabhängig vom Modus (FR-015).
`TOOL_INSTRUCTION` und `TOOL_INSTRUCTION_WITHOUT_SEARCH` (`prompt.rs:10-18`) bekommen den Satz: „If you
cannot carry out an instruction unambiguously, call ask_user with the possible options instead of
refusing.“

**Rationale**: Wie `find_actions` ein Werkzeug ohne Bridge. Die Ausnahme in `plan_calls` ist ein
Namensvergleich wie in `offer.rs` für `find_actions`; ein neuer Trait-Hook nur für ein Tool wäre
Abstraktion ohne zweiten Nutzer.

**Alternatives considered**: Neue `RiskClass::None` — verworfen, zieht Frontend-Typen und Audit-Texte nach
sich.

## R7 — Rückfrage ohne Chat-UI

**Decision**: Externe Agenten über MCP gibt es noch nicht (Spec 021 ist nicht angelegt); die
`ActionBridge` bedient nur den eingebauten Agenten. Die Modellprüfung (`chat/eval/`) führt keine
Aktionen aus. Es gibt also heute keinen Aufrufer, der `needs_choice` ohne UI bekommt. Für später trägt
`ToolResult.content` bei einer Aktions-Rückfrage schon die JSON-Form
`{"error":{"code":"needs_choice","message","field","options"}}`.

## R8 — Nicht verfügbare Apps als Kandidaten

**Decision**: Ein Kandidat mit `unavailableKey` erscheint mit dem übersetzten Grund
(`ChoiceOption.unavailable`) und ist in der Rückfrage nicht wählbar, wie im Launcher. Spec-Edge-Case
entsprechend präzisiert (vorher: „führt zum selben Fehler“).

**Rationale**: `wm.app.open` prüft `unavailableKey` heute nicht; der Launcher deaktiviert den Eintrag.
Gleiches Verhalten an beiden Stellen.

## R9 — Frontend: eine Warteschlange für Freigaben und Rückfragen

**Decision**: Die Liste `pendingApprovals` (State in `ChatApp.vue:56,73`, Pflege in
`useChatTranscript.ts:297-345`, `useComposer.ts:104-111,259-265`, `useThreadSidebar.ts:283-311`,
Close-Guard `useChatTab.ts:91-103`) wird zu `pendingPrompts` mit
`PendingPrompt = { kind: 'approval', ... } | { kind: 'choice', ... }`. Umbenennen ist ein eigener
mechanischer Commit. `Composer.vue` zeigt für `pendingPrompts[0]` je nach `kind` `PermissionPrompt` oder
`ChoicePrompt`. Listener `onChoiceRequest` in einer neuen Datei `src/composables/useChatChoices.ts`
(`useChat.ts` hat schon 591 Zeilen); `ChatApp.vue` (495 Zeilen) bekommt nur die Registrierung in
`registerChatSubscriptions`.

**Rationale**: Die Behandlung von Thread-Wechsel, Rennen zwischen Event und `invoke` und Turn-Ende
existiert schon einmal; eine zweite Liste müsste sie an fünf Stellen duplizieren.

**Alternatives considered**: Eigene Liste `pendingChoices` — verworfen, Duplikation der Race-Logik.

## R10 — UI-Bausteine

**Decision**: `src/components/chat/ChoicePrompt.vue` nach `PermissionPrompt.vue`: `UiDrawerModal`,
Frage, `ShadcnRadioGroup` mit `ShadcnRadioGroupItem` je Kandidat (haex-ui, eingeführt mit
haex-space/haextension#75; haex-vault stellt in PR #337 alle Radio-Stellen darauf um), Option „Etwas anderes …“
als weiteres Item mit `UiInput`, Footer mit `UiButton` „Abbrechen“ (outline) und „Bestätigen“. Schließen des Dialogs =
Abbrechen der Rückfrage (nicht des Turns). Antwort über die Aktion `chat.choice.answer`
(`chatActions.ts`, Scope `guardrails` wie `chat.approval.decide`) → `invoke('respond_choice', ...)`.
i18n unter `chat.choice.*`.

## R11 — Modellprüfung

**Decision**: `eval_set.json` Version 3: neue `Kind::Clarify`; je Sprache 3 Tippfehler-Sätze
(`kind: change`, erwartet `wm_app_open` ohne Argument-Vorgabe) und 3 mehrdeutige Sätze
(`kind: clarify`, erwartet `ask_user`), `selfTest: false`. Der Runner (`eval/runner.rs:54-58`) bietet
`ask_user` im Core-Angebot an; die Schema-Suche beim Bewerten (`scoring.rs`) kennt die eingebauten Tools
`ask_user` und `find_actions`, sonst würde ein erwarteter `ask_user`-Aufruf als `BadArgs` zählen.

**Rationale**: FR-017, SC-005/SC-006 brauchen eigene Quoten; `Kind` ist ein geschlossenes Enum.

## R12 — Agent-Kontextdatei

**Decision**: Entfällt. `CLAUDE.md` wurde mit `c082fe89` („migrate haex-vault to spaex v4“) entfernt; es gibt
keine Datei mit `SPECKIT`-Markern.

## R13 — Kernangebot: `wm.apps.list` raus, `ask_user` rein (beim Umsetzen entschieden)

**Decision**: `ask_user` gehört zum Erstangebot jedes Turns (FR-015). Damit das Erstangebot bei
höchstens 10 Werkzeugen bleibt (Spec 032, R6), verlässt `wm.apps.list` die Kernaktionen
(`CORE_AGENT_TOOLS`, `src/lib/actions/agentTools.ts`); `find_actions` findet es weiter. Nach einer Suche
reserviert `extend_offer` je einen Platz für `find_actions` und `ask_user` (höchstens 15 Werkzeuge).

**Rationale**: Seit `wm.app.open` App-Namen auflöst (R5), braucht das Modell die App-Liste zum Öffnen
nicht mehr. Vom Operator am 2026-10-08 so gewählt.

**Alternatives considered**: Grenze auf 11 anheben; eine andere Kernaktion (z. B.
`settings.models.list`) herausnehmen.

## R14 — Viele Apps (beim Umsetzen geprüft)

Nutzer werden 30 und mehr Erweiterungen installieren; der Agent muss jede finden. `matchApp` durchsucht
immer alle installierten Apps, ohne Obergrenze und ohne Liste im Prompt; nur die Rückfrage zeigt
höchstens 5 Kandidaten plus „Etwas anderes …“. `scripts/check-wm-app-match.ts` prüft 40 Erweiterungen
plus die System-Apps: jede App öffnet sich mit ihrem Namen in jeder Schreibweise; bei einem vertauschten
Buchstabenpaar öffnen 37 von 41 direkt (≥ 90 %, SC-002), die übrigen (sehr kurze Namen wie „pdf“,
„git“) stehen in der Rückfrage unter den Kandidaten.

## R15 — „Welche Erweiterungen sind installiert?“ (beim Testen gefunden)

Seit R13 erreicht der Agent `wm.apps.list` nur noch über `find_actions`. Die Suche verglich ganze
Wörter aus Id, Beschreibung und Titeln; „erweiterungen“ stand nirgends, das leere Ergebnis mit dem
Hinweis „no matching action“ las das Modell als „keine Erweiterungen installiert“. Drei Änderungen:

- `wm.apps.list` nennt in Beschreibung und Titeln die installierten Erweiterungen („Apps und
  Erweiterungen auflisten“).
- Die Suche lässt ab fünf Buchstaben ein Wort als Präfix eines anderen gelten, sodass Einzahl und
  Mehrzahl einander finden („Erweiterung“ ↔ „Erweiterungen“, „extension“ ↔ „extensions“); kürzere
  Wörter zählen weiter nur ganz („set“ findet nicht „settings“).
- Der Hinweis bei leerem Ergebnis sagt, dass er nur Aktionen betrifft, nicht die Daten des Nutzers,
  und schlägt andere Stichworte oder die Liste aller Aktionen vor.

Das Eval-Set bekommt den Satz „Welche Erweiterungen sind installiert?“ (`read-extensions-de-1`).

## R16 — `wm.apps.list` zurück ins Kernangebot (beim Testen entschieden, ersetzt R13 teilweise)

**Decision**: `wm.apps.list` gehört wieder zu den Kernaktionen. Das Erstangebot darf dafür 11 statt
10 Werkzeuge haben (Kern höchstens 9, `find_actions`, `ask_user`), abweichend von Spec 032, R6; nach
einer Suche höchstens 16 statt 15 (`MAX_ACTION_OFFER`), damit weiter alle fünf Treffer Platz haben.

**Rationale**: Mit Qwen3-4B scheiterte „Welche Erweiterungen sind installiert?“ zweimal am Umweg über
die Suche: erst las das Modell die leere Suche als „keine Erweiterungen“ (R15), nach der Korrektur
fand es `wm_apps_list`, rief es aber nicht auf und erfand zwei Erweiterungen mit Platzhalter-Ids. Die
Frage nach dem Installierten ist zu grundlegend, um sie vom Suchschritt eines kleinen Modells
abhängig zu machen. Vom Operator am 2026-10-09 so gewählt.

**Alternatives considered**: `settings.models.list` gegen `wm.apps.list` tauschen (Grenze bliebe
bei 10); `wm.apps.list` nur über die Suche lassen.

## R17 — Keine Ids in Antworten (beim Testen gefunden)

Nach R16 listete der Agent die Erweiterungen korrekt, nannte aber zu jeder ihre App-Id
(`extension.<UUID>`). Ids braucht das Modell für Werkzeugaufrufe, dem Nutzer sagen sie nichts. Die
Werkzeug-Instruktion (`prompt.rs`, mit und ohne Suche) verlangt jetzt, Dinge bei ihrem Titel zu nennen
und nie Ids wie App-Ids oder UUIDs zu zeigen; das gilt auch für Unterhaltungen, Geräte und andere
Listen. Damit der Agent eine Erweiterung stattdessen beschreiben kann, gibt `wm.apps.list` die
Beschreibung aus ihrem Manifest mit (`description`, nur wenn vorhanden; System-Apps haben keine).

## R18 — `run_command` ist kein Ausweg für haex-vault-Daten (beim Testen gefunden)

Auf „Wie viele ungelesene Mails habe ich?“ wollte Qwen3-4B `run_command` mit dem erfundenen Befehl
`mail count unread` ausführen; nur die Freigabe hielt es auf. An die Daten einer Erweiterung kommt der
Agent erst mit den geplanten Specs 018 (Werkzeuge über MCP) und 019 (Werkzeuge in den haextensions).
Bis dahin grenzt die Beschreibung von `run_command` (`chat/tools/cli.rs`) das Werkzeug ein: nur wenn
der Nutzer etwas auf dem Gerät ausführen lassen will, es erreicht Programme und Dateien des Geräts,
nicht haex-vaults Apps oder Erweiterungen oder deren Daten, und nie einen erfundenen Befehl. Das Eval-Set
bietet `run_command` nicht an; ob das Modell der Beschreibung folgt, zeigt nur der Test in der App.
