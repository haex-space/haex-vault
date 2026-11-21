# Tasks: Agent-Rückfrage mit Auswahl

**Input**: Design documents from `/specs/046-agent-choice-prompt/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/choice-contract.md, quickstart.md

**Tests**: Test-first per task pair (write the failing check, then implement). Pure frontend logic in
`scripts/check-*.ts` (`node --test`, relative `.ts` imports, no `~/` alias); Rust unit tests in sibling
`*_tests.rs` files (never inline `mod tests { … }`), integration tests in `src-tauri/tests/` with
`tests/common/tool_loop_fixture.rs`. No new test framework, no new dependency.

**Conventions**: Pure modules under `src/lib/` import only relative `.ts` paths. i18n keys go into both
`src/i18n/locales/de.json` and `en.json`. Every changed file stays under 500 lines; files already above
(`useChat.ts`, `commands.rs`) only get the minimal hook, new logic goes into new files (plan, 500 LoC
row). Commit per phase with Conventional Commits (`feat(agent): …`, `refactor(chat): …`).

## Phase 1: Setup

**Purpose**: Build on PR #336 and satisfy the graphify-first rule before new named artifacts exist.

- [x] T001 Confirm PR #336 (`fix/agent-unknown-app`) and PR #337 (`refactor/radio-group`, haex-ui `ShadcnRadioGroup`) are merged, then `git fetch origin && git rebase origin/main` on branch `046-agent-choice-prompt`; verify `ActionInputError` exists in `src/lib/actions/runner.ts` and `unknownAppMessage` in `src/lib/wm/apps.ts`. verify `ShadcnRadioGroup` in `.nuxt/components.d.ts` after `nuxt prepare`. If either PR is not merged, stop and ask the operator
- [x] T002 Refresh the graph (`graphify` on the worktree) and run one bounded `graphify query "<intent>" --budget 1000` each for `src/lib/wm/appMatch.ts`, `src-tauri/src/chat/turn/choices.rs`, `src-tauri/src/chat/tools/ask_user.rs`, `src/components/chat/ChoicePrompt.vue`, `src/composables/useChatChoices.ts`; record candidates and why each did not match in the head section of `specs/046-agent-choice-prompt/research.md`. If a near-identical candidate appears, stop and ask the operator

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Two mechanical refactors as their own commits, no behaviour change (research R5, R9).

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T003 [P] Move `fold` from `src/lib/passwords/search.ts:19-24` to new `src/lib/search/fold.ts` (named export, same doc comment); `src/lib/passwords/search.ts` imports it relatively and re-exports nothing new; `pnpm check:passwords` stays green; commit `refactor(search): move fold to a shared module`
- [x] T004 [P] Rename the chat prompt queue: `PendingApproval` → `PendingPrompt` with a `kind: 'approval'` discriminant (type moves from `src/components/chat/PermissionPrompt.vue` to `src/composables/useChat.ts` next to `ToolPermissionRequestEvent`), `pendingApprovals` → `pendingPrompts`, `pendingApprovalsByThread` → `pendingPromptsByThread` in `src/components/apps/ChatApp.vue`, `src/components/chat/Composer.vue`, `src/components/chat/PermissionPrompt.vue`, `src/composables/useChatTranscript.ts`, `src/composables/useComposer.ts`, `src/composables/useThreadSidebar.ts`, `src/composables/useChatTab.ts`, `scripts/lib/chat-state-harness.ts`, `scripts/check-chat-state.ts`; `handleToolPermissionRequest` sets `kind: 'approval'`; `PermissionPrompt` only reads prompts of kind `approval`; `pnpm check:chat-state`, `pnpm check:templates`, `pnpm typecheck` green; commit `refactor(chat): one queue for pending prompts`

**Checkpoint**: Refactors merged into the branch; behaviour unchanged.

---

## Phase 3: User Story 1 - App per Name öffnen, auch mit Tippfehler (Priority: P1) 🎯 MVP

**Goal**: `wm.app.open` / `wm.tab.new` accept a name or a misspelled name and open a clear match directly
(FR-001–FR-003).

**Independent Test**: quickstart steps 1–4: „öffne haex-mail“, „öffne haex-mial“, „öffne die
Einstellungen“ open the right app without a question.

- [x] T005 [US1] Write `scripts/check-wm-app-match.ts` (fixtures: `WM_APPS` plus extension apps haex-mail, haex-notes, haex-files built with `extensionApps` from `src/lib/extensions/apps.ts`; `titleOf` stub mapping `wm.apps.settings` → „Einstellungen“ etc.): exact id → exact; `system.federation` alias → exact `system.settings`; „haex-mail“ and „HAEX-MAIL“ → exact; „haex-mial“ → exact haex-mail; „system.notes“ → exact haex-notes; „einstellungen“ → exact `system.settings`; „settings“ → exact via id rest; „haex“ → choice with the three haex apps; „Kalender“ → choice with ≤ 5 candidates (possibly empty); candidates ordered best first, never more than 5; `unavailableKey` carried on the candidate. Add the file to `check:wm-navigation` in `package.json`. Run it and confirm it fails (module missing)
- [x] T006 [US1] Implement `matchApp(input, apps, titleOf)` in new `src/lib/wm/appMatch.ts` per research R5 (exact id → `resolveAppAlias` → fold + strip `system.`/`extension.` prefix → exact folded title or id rest → Fuse over `[title, idRest]` with `ignoreLocation: true`, `includeScore: true`, `threshold: 0.4`; clear match = best score ≤ 0.25 and no second or gap ≥ 0.15; else `{ kind: 'choice', candidates }` capped at 5). Calibrate the two thresholds until T005 passes; document the final values and why in the module comment
- [x] T007 [US1] `src/stores/wmActionHandlers.ts`: `registerWmActionHandlers(wm, t)` (type `Translate` as in `src/stores/wmLayoutHandlers.ts:21,40`); `target(input)` calls `matchApp(String(input.appId), wm.apps(), (app) => app.title ?? t(app.titleKey))`; `exact` → use its `appId` (alias `at` as before); `choice` → keep throwing `ActionInputError(unknownAppMessage(...), 'appId')` until US2 replaces it
- [x] T008 [US1] `src/plugins/actions.client.ts:21-23`: pass `t` to `registerWmActionHandlers`
- [x] T009 [US1] `src/lib/actions/wmActions.ts` `APP_ID.description`: the app id from `wm.apps.list` or the app's name as the user said it; regenerate `src-tauri/src/chat/eval/tools.json` with `pnpm export:eval-tools` then `pnpm exec prettier --write` on it; `pnpm check:agent-actions` green
- [x] T010 [US1] Run `pnpm check:wm-navigation`, `pnpm typecheck`, `pnpm lint`; commit `feat(agent): open apps by name, tolerating typos`

**Checkpoint**: US1 works on its own; ambiguous names still return `invalid_input` with all ids.

---

## Phase 4: User Story 2 - Rückfrage mit Auswahl, wenn die App nicht eindeutig ist (Priority: P1)

**Goal**: An ambiguous app opens nothing; the tool round asks in the chat and re-runs the action with the
answer (FR-004–FR-012).

**Independent Test**: quickstart steps 5–12.

### Frontend transport

- [x] T011 [P] [US2] `scripts/check-wm-actions.ts`: a handler throwing `ActionChoiceError('no app matches haex unambiguously', 'appId', [{ value, label }])` becomes `{ ok: false, code: 'needs_choice', message, field: 'appId', options: [...] }`; run, confirm failure
- [x] T012 [US2] `src/lib/actions/types.ts`: `ActionErrorCode` + `'needs_choice'`, `ChoiceOption = { value: string; label: string; unavailable?: string }`, failure variant gets `options?: ChoiceOption[]`; `src/lib/actions/runner.ts`: export `ActionChoiceError extends Error { field; options }`, map it in the `catch` like `ActionInputError`; T011 passes
- [x] T013 [P] [US2] `scripts/check-agent-actions.ts`: `toOutcomeWire` keeps `options` for `needs_choice`; run, confirm failure
- [x] T014 [US2] `src/lib/actions/agentTools.ts:102-119`: `ActionOutcomeWire` failure gets `options?`, `toOutcomeWire` copies it; T013 passes
- [x] T015 [US2] `src/stores/wmActionHandlers.ts`: on `choice` throw `ActionChoiceError` with `field: 'appId'` and options `{ value: appId, label: title, unavailable: unavailableKey ? t(unavailableKey) : undefined }` (research R8); drop the now unused `unknownAppMessage` import only if nothing else uses it

### Rust transport

- [x] T016 [US2] Tests first in `src-tauri/src/chat/tools/action_bridge_tests.rs`: an outcome wire `{ ok: false, code: "needs_choice", field, message, options }` becomes `ActionReply::NeedsChoice` with the message unmasked; and in `action_tool_tests.rs`: `into_tool_result` of that reply yields a `ToolResult` whose `choice` has `field`, `value` = the call's input at `field`, `question: None`, the options, and whose `content` is `{"error":{"code":"needs_choice","message","field","options"}}`; run `cargo test`, confirm failure
- [x] T017 [US2] `src-tauri/src/chat/tools/mod.rs`: `ChoiceOption { value, label, unavailable: Option<String> }` and `ChoiceRequest { question: Option<String>, field: Option<String>, value: String, options: Vec<ChoiceOption> }` (both `Serialize`, `Clone`, `Debug`, camelCase), `ToolResult.choice: Option<ChoiceRequest>`, constructor `ToolResult::needs_choice(content, ChoiceRequest)`; replace every non-test `ToolResult { … }` struct literal (18 sites, `grep -rn "ToolResult {" src-tauri/src`) with `ToolResult::ok/error` or add `choice: None`
- [x] T018 [US2] `src-tauri/src/chat/tools/action_bridge.rs`: `ActionOutcomeWire.options: Option<Vec<ChoiceOption>>` (`#[serde(default)]`, `ChoiceOption` gets `Deserialize`), `ActionReply::NeedsChoice { field, message, options }`, `From` maps code `needs_choice`; `action_tool.rs` `into_tool_result` builds `ToolResult::needs_choice` from it (needs the call input: pass `&input` into `into_tool_result`); T016 passes

### Rust choice loop

- [x] T019 [US2] `src-tauri/src/chat/events.rs`: `EVENT_CHOICE_REQUEST = "chat-choice-request"`, `ChoiceRequestEvent { request_id: Uuid, thread_id: Uuid, tool_name: String, question: Option<String>, field: Option<String>, value: String, options: Vec<ChoiceOption> }` (`Serialize`, camelCase)
- [x] T020 [US2] New `src-tauri/src/chat/turn/choices.rs`: `ChoiceAnswer` (`Deserialize`, `#[serde(tag = "kind", rename_all = "snake_case")]`: `Option { value }`, `Text { text }`, `Cancel`); `PendingChoices { pending: Mutex<HashMap<Uuid, oneshot::Sender<ChoiceAnswer>>>, cancelled: Mutex<HashSet<Uuid>> }` with `insert`, `resolve(id, answer) -> Result<(), ChatError>` (unknown → `InvalidInput`, tombstoned → `Ok`), `cancel_all()` (move pending ids to tombstones, drop senders), `reset()`; `#[tauri::command] respond_choice(args: RespondChoiceArgs { request_id, answer })`. Wire up: `ChatState.pending_choices: Arc<PendingChoices>` in `src-tauri/src/chat/session.rs` (incl. manual `Clone` and `reset_for_close`), `abort_turn` in `src-tauri/src/chat/commands.rs:620-669` calls `cancel_all()`, register `respond_choice` in `src-tauri/src/lib.rs` next to `respond_tool_permission`; register the test module `#[cfg(test)] #[path = "choices_tests.rs"] mod tests;`
- [x] T021 [US2] Tests first: `src-tauri/src/chat/turn/choices_tests.rs` (resolve: unknown id error, tombstoned id ok, answer delivered once) and new integration test `src-tauri/tests/chat_tool_loop_choices.rs` (fixtures from `tests/common/tool_loop_fixture.rs`; a `ScriptedTool` with source `action` returning `needs_choice` for input `appId: "haex"` and ok otherwise): (a) option answer → tool re-executed with `appId` replaced, one `chat-choice-request` event with `threadId`, persisted result has the `{"result", "choice"}` envelope; (b) text answer that is again ambiguous → second request, then option → ok; (c) cancel → result `declined_by_user`, `is_error`; (d) mode Manual → exactly one `tool-permission-request` before the choice, none after (FR-012); (e) turn cancel while the choice is open → turn ends `Cancelled`, no tool rows persisted. Run, confirm failure
- [x] T022 [US2] `choices.rs`: `impl TurnRunner { async fn resolve_choices(&mut self, executed: &mut [ExecutedCall]) }` per research R1/R4: for each result with `choice`, loop: insert sender, `emit_event(EVENT_CHOICE_REQUEST, …)`, `select!{ biased; cancel → return; rx }`; with `field`: copy input, set field to option value or text, look the tool up in the registry, `execute` again (no `plan_calls`), repeat while the new result has a choice; finally wrap content as `{"result": <parsed content or string>, "choice": {"value", "answer"}}`; `Cancel` → `ToolResult::error("declined_by_user")`. Call it in `src-tauri/src/chat/turn/tool_round.rs` between `execute_plans` and the cancel check (`:84-91`); make `ExecutedCall` visible to `choices.rs`. T021 passes; `tool_round.rs` stays under 500 lines

### Frontend UI

- [x] T023 [US2] `src/composables/useChat.ts`: `ChoiceRequestEvent` type and `PendingPrompt` variant `{ kind: 'choice', requestId, toolName, question?, field?, value, options }`. New `src/composables/useChatChoices.ts`: `onChoiceRequest(handler)` (listen `chat-choice-request`) and `respondChoiceAsync(requestId, answer)` (`invoke('respond_choice', { args: { requestId, answer } })`). `src/composables/useChatTranscript.ts`: `handleChoiceRequest` queues like `handleToolPermissionRequest` (per-thread parking, de-dup by `requestId`)
- [x] T024 [US2] `src/lib/actions/chatActions.ts`: action `chat.choice.answer` (scope `guardrails`, input `{ requestId, answer }`, mirrors `chat.approval.decide` at `:145-157`) with title i18n key; handler in `src/composables/useChatTab.ts` answers, removes the prompt, clears attention when the queue is empty. `src/components/apps/ChatApp.vue`: register `onChoiceRequest` in `registerChatSubscriptions` with `wmTab.requestAttention()`; file stays under 500 lines
- [x] T025 [US2] New `src/components/chat/ChoicePrompt.vue` (research R10): `UiDrawerModal` like `PermissionPrompt.vue:108-174`; title from `question` or `chat.choice.title {value}` („Ich habe „{value}“ nicht eindeutig gefunden. Meintest du …?“); `ShadcnRadioGroup` with one `ShadcnRadioGroupItem` per candidate inside a `<label>` (pattern `src/components/extensions/StorageDialog.vue` after PR #337), disabled item plus reason for `unavailable`; an item „Etwas anderes …“ revealing `UiInput`; footer `UiButton` „Abbrechen“ (outline) and „Bestätigen“ (disabled until a valid selection or non-empty text); closing the dialog = cancel answer; emits `answer: [requestId, ChoiceAnswer]`. `src/components/chat/Composer.vue`: render `PermissionPrompt` or `ChoicePrompt` by `pendingPrompts[0].kind`, forward the answer
- [x] T026 [P] [US2] i18n in `src/i18n/locales/de.json` and `en.json`: `chat.choice.{title, other, otherPlaceholder, confirm, cancel, unavailable, noCandidates}`, `actions.chat.choice.answer`, `chat.autonomy.audit.declined_by_user`; `src/components/chat/MessageList.vue:48-51` adds `declined_by_user` to `DENY_AUDIT_MARKERS`
- [x] T027 [US2] `scripts/check-chat-state.ts` (+ harness hooks in `scripts/lib/chat-state-harness.ts` if needed): a choice request is queued for the active thread; for another thread it is parked and restored on switch; answering calls `respond_choice` and removes it; turn completion clears it; the close guard counts it. Run `pnpm check:chat-state`, `pnpm check:templates`, `pnpm typecheck`, `pnpm lint`, `cargo test` for the chat modules; commit `feat(agent): ask the user when an app is ambiguous`

**Checkpoint**: US1 + US2 deliver the reported fix end to end.

---

## Phase 5: User Story 3 - Der Agent fragt nach, statt eine Anweisung abzulehnen (Priority: P2)

**Goal**: The model can ask with `ask_user`; the system prompt tells it to (FR-013–FR-017).

**Independent Test**: quickstart step 13; eval quotas (step 14).

**Depends on**: US2 (choice loop, event, command, `ChoicePrompt`).

- [x] T028 [US3] Tests first in new `src-tauri/src/chat/tools/ask_user_tests.rs`: schema has `question` and `options` (2–5); `execute` with valid input returns `needs_choice` with `question: Some`, `field: None`, options as `{value: label, label}`; fewer than 2 or more than 5 options or a missing question → `ToolResult::error("invalid_input")`; extend `choices_tests.rs`/`chat_tool_loop_choices.rs`: without `field` an option answer becomes `{"answer": …}`, text becomes `{"answer": …, "freeText": true}`, cancel `declined_by_user`; in Manual mode `ask_user` raises no `tool-permission-request`. Run, confirm failure
- [x] T029 [US3] New `src-tauri/src/chat/tools/ask_user.rs`: `ASK_USER_TOOL_NAME = "ask_user"`, `AskUserTool` (description and schema from `contracts/choice-contract.md`, `source()` = `ACTION_SOURCE`, `risk_class()` = Safe); declare the module and its test module in `src-tauri/src/chat/tools/mod.rs`
- [x] T030 [US3] `src-tauri/src/chat/action_commands.rs:54-80`: reserve the name `ask_user`, register `AskUserTool` next to `FindActionsTool`; `src-tauri/src/chat/tools/offer.rs:27-37` `core_offer` always keeps `ask_user` (extend `offer_tests.rs`); `src-tauri/src/chat/turn/tool_round.rs` `plan_calls`: `ask_user` is always `Allow`
- [x] T031 [US3] `src-tauri/src/chat/turn/choices.rs`: the no-`field` branch maps answers per T028; T028 passes
- [x] T032 [US3] `src-tauri/src/chat/tools/prompt.rs:10-18`: both instruction variants gain „If you cannot carry out an instruction unambiguously, call ask_user with the possible options instead of refusing.“; adjust `prompt_tests.rs`
- [x] T033 [US3] `src/components/chat/ChoicePrompt.vue`: when `question` is set show it as the title and no `value` line (otherwise unchanged); `pnpm check:templates`
- [x] T034 [US3] Eval (research R11): `src-tauri/src/chat/eval/scoring.rs` adds `Kind::Clarify` and resolves schemas for the built-in tools `ask_user` and `find_actions` besides `tools.json`; `src-tauri/src/chat/eval/runner.rs:54-58` offers `AskUserTool` in step 1; `src-tauri/src/chat/eval/eval_set.json` → version 3 with, per language, 3 typo sentences (`kind: change`, expect `wm_app_open` with `args: {}`) and 3 ambiguous sentences (`kind: clarify`, expect `ask_user` with `args: {}`), all `selfTest: false`; extend `scoring_tests.rs` / `runner_tests.rs`; `cargo test` green; commit `feat(agent): let the agent ask instead of refusing`

**Checkpoint**: All three stories work.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [x] T035 Check every new or changed file stays under 500 lines (`wc -l` on all files from the plan's structure tree); files already above 500 must not grow by more than their minimal hook
- [x] T036 Run `cargo fmt --check`, `pnpm lint:rust` (both feature sets), `cargo test --manifest-path src-tauri/Cargo.toml`, `pnpm check:wm-navigation`, `pnpm check:agent-actions`, `pnpm check:chat-state`, `pnpm check:passwords`, `pnpm check:templates`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`; fix until green
- [ ] T037 Walk through `specs/046-agent-choice-prompt/quickstart.md` steps 1–13 in `pnpm tauri:dev:cuda` with Qwen3-4B; record skipped steps in the PR description
- [x] T038 Run the model evaluation (`cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture`, setup as in `specs/032-model-operates-holzi/quickstart.md:76`) against Qwen3-4B; record overall, `change` and `clarify` quotas and the SC-005/SC-006 verdict in new `specs/046-agent-choice-prompt/eval-results.md`
- [x] T039 Add **Rückfrage** and **Kandidat** to the "Chat runtime" section of `CONTEXT.md` (`:89`)
- [x] T040 Let `find_actions` find `wm.apps.list` for „Erweiterungen“ (research R15): name the extensions in its description and titles, match words of five or more letters as prefixes in `search_score` (`offer.rs`), make the empty-result hint say it is about actions, not data; tests in `offer_tests.rs`, eval sentence `read-extensions-de-1`, `pnpm export:eval-tools`
- [x] T041 Put `wm.apps.list` back into `CORE_AGENT_TOOLS` (research R16): first offer at most 11 tools (`check-agent-actions.ts`, `eval/runner_tests.rs`), `MAX_ACTION_OFFER` 16, `pnpm export:eval-tools`
- [x] T042 Keep ids out of the agent's replies (research R17): the tool instruction in `prompt.rs` says to name things by their title and never show ids; `wm.apps.list` returns an extension's manifest `description` (`AppDefinition.description`, `extensionApps`); tests in `prompt_tests.rs` and `check-extensions-apps.ts`
- [x] T043 Limit `run_command`'s description to commands the user asks for on the device, not haex-vault's apps, extensions or their data, and never an invented command (research R18); test in `cli_tests.rs`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 blocks everything (needs PR #336 and #337); T002 before any new file
- **Foundational (Phase 2)**: T003 and T004 independent of each other; T003 blocks T006; T004 blocks T023–T027
- **US1 (Phase 3)**: after T003; T005 → T006 → T007/T008 → T009 → T010
- **US2 (Phase 4)**: after US1 (T015 replaces T007's fallback) and T004. Frontend transport T011–T015 and Rust transport T016–T018 can proceed in parallel; T019–T022 need T017/T018; UI T023–T027 needs T019/T020 for the event and command names
- **US3 (Phase 5)**: after US2
- **Polish (Phase 6)**: after all stories

### Within Each Story

Failing test first, then implementation, then i18n, then the story's commit.

### Parallel Opportunities

- T003 ∥ T004
- In US2: T011–T015 (TypeScript) ∥ T016–T018 (Rust); T026 (i18n) alongside T025
- In US3: T032 (prompt) and T033 (UI) alongside T029–T031

## Parallel Example: User Story 2

```text
T011 Test ActionChoiceError mapping in scripts/check-wm-actions.ts
T016 Test NeedsChoice in src-tauri/src/chat/tools/action_bridge_tests.rs
T026 i18n chat.choice.* in de.json / en.json
```

## Implementation Strategy

### MVP First

1. Phase 1 + Phase 2
2. Phase 3 (US1): typo-tolerant opening → validate quickstart 1–4 → already fixes most reports
3. Phase 4 (US2): the choice prompt → quickstart 5–12

### Incremental Delivery

US1 → US2 → US3, one commit per story after the two refactor commits. One PR for the feature; if the
diff passes ~1000 lines after US2, split US3 into a follow-up PR.
