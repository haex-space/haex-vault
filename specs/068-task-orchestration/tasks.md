---
description: 'Task list for spec 068-task-orchestration'
---

# Tasks: Orchestrator und Modell-Routing

**Input**: Design documents from `/specs/068-task-orchestration/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/orchestrator.md](./contracts/orchestrator.md),
[quickstart.md](./quickstart.md), [ADR-0021](../../docs/adr/0021-parallel-turns-and-orchestrator.md)

**Tests**: Included (constitution). Unit tests in sibling `*_tests.rs`; integration tests in
`src-tauri/tests/` with scripted models (pattern of the existing turn tests with a recorder `emit`);
the routing measurement is `#[ignore]` and release-only (memory: debug mistralrs is too slow); rules in
`scripts/check-orchestration-*.ts`. Tests first.

**Organization**:

| Delivery | Phases     | Stories                              |
| -------- | ---------- | ------------------------------------ |
| PR A     | –          | documents, ADR-0021                  |
| PR B     | Phases 1–2 | Setup, Foundational (parallel turns) |
| PR C     | Phases 3–6 | US1, US3, US5, US6                   |
| PR D     | Phase 7    | US2                                  |
| PR E     | Phase 8    | US4                                  |
| PR F     | Phase 9    | US1 (delegate)                       |
| PR G     | Phase 10   | US7                                  |
| –        | Phase 11   | Polish                               |

**Shipping notes**: PR B changes no visible behaviour. The orchestration switch appears in PR C
(default off). ADR-0021 becomes "accepted" in PR B.

**Conventions** (as in 066 tasks.md): files ≤ 500 lines; test code separate; no secrets, briefs or
reports in logs; Rust only via `nix develop --command scripts/with-nix-host-bridge.sh cargo … -j 4`,
one test target at a time; `pnpm generate:ts-types` after Rust type changes; prettier on touched
Markdown/JSON; `graphify query` before new named artifacts; Conventional Commits without agent
attribution.

---

## Phase 1: Setup (PR B)

- [ ] T001 Create module `src-tauri/src/orchestrator/mod.rs` (empty submodules per plan) and register it in src-tauri/src/lib.rs
- [ ] T002 [P] Create `scripts/check-orchestration-settings.ts` and the script `check:orchestration` in package.json; wire into the CI job of `check:agent-actions`
- [ ] T003 [P] Add the routing fixture (40 tasks with human stage labels: 10 coding, 10 simple questions without task, 20 mixed) to the tool-eval fixtures of spec 032 (next to `src-tauri/tests/model_tool_eval.rs`)
- [ ] T004 Spike: two concurrent sequences on one `LocalModel` (Qwen3-4B, release) on the reference desktop; record memory, time to first token and throughput in research.md R2

---

## Phase 2: Foundational — parallel turns (PR B, blocking)

### Tests first

- [ ] T005 [P] Turn-slot tests in src-tauri/src/chat/turn_slots_tests.rs: insert/remove by turn id, lookup by thread and task, abort one slot leaves others running
- [ ] T006 [P] Integration test src-tauri/tests/chat_parallel_turns.rs: two scripted turns in different threads run at once; a model load waits for both (write lock); aborting one cancels only its generation, approvals and choices; the other completes
- [ ] T007 [P] Regression: all existing chat turn tests and `cli_delegate` tests pass unchanged with orchestration off
- [ ] T008 [P] Token-sum tests in src-tauri/src/chat/tokens_tests.rs: tokens of every round add up per turn; stored on the final row

### Implementation

- [ ] T009 Implement `TurnSlots` in src-tauri/src/chat/turn_slots.rs and switch `ChatState` (src-tauri/src/chat/session.rs): `operation` → `tokio::sync::RwLock<()>` (turns read, loads and vault transitions write); `current_generation` and `turn_cancellation` move into the slot
- [ ] T010 Add `turn_id` to pending approvals and choices (`pending_tool_approvals`, `pending_choices`) with the smallest change in src-tauri/src/chat/turn/tool_round.rs and src-tauri/src/chat/choices.rs; `abort_turn(turn_id)` drains only that turn
- [ ] T011 Command `abort_turn_by_id {turnId}`; `abort_current_generation` aborts the turn of the active thread (src-tauri/src/chat/commands.rs)
- [ ] T012 Model per turn: a turn may carry its own adapter from `model_jobs::ModelSource` / `Providers::adapter` instead of `chat.session`; local models share one `LocalModel` with up to two sequences per T004 (src-tauri/src/model_jobs/local_slot.rs)
- [ ] T013 Token sums per turn in src-tauri/src/chat/tokens.rs; columns `turn_tokens_in`, `turn_tokens_out` on `chat_messages` in the 068 migration (T024)
- [ ] T014 Events carry `turnId` (and `taskId` when set) in src-tauri/src/chat/events.rs
- [ ] T015 Frontend: several streams keyed by turn id in src/composables/useChatTranscript.ts and src/composables/useChat.ts; extend scripts/check-chat-state.ts
- [ ] T016 Set ADR-0021 to "accepted" in docs/adr/0021-parallel-turns-and-orchestrator.md

**Checkpoint (PR B)**: two turns can run at once; nothing visible changes.

---

## Phase 3: User Story 1 - Aufgabe an den passenden Agenten (Priority: P1) 🎯 MVP (PR C)

**Goal**: The orchestrator hands a task to one stage and reports the result.

**Independent Test**: Scripted orchestrator dispatches to a scripted executor of stage "klein"; the
report reaches the orchestrator and the user; stage and reason are stored.

- [ ] T017 [P] [US1] Dispatch tests in src-tauri/src/orchestrator/tools_tests.rs: `task_dispatch` validates `stage` ∈ "'local'|'small'|'large'|'delegate'", stores `reason`, rejects a brief containing a secret (redaction of `mcp_server::redact`), returns `unavailable` with fallback order (up, then down; `delegate` only for coding)
- [ ] T018 [P] [US1] Report tests in src-tauri/src/orchestrator/report_tests.rs: `task_report` ends the turn; missing report is built from the last answer and `finish_reason` (`tool_limit_reached` → "partial")
- [ ] T019 [P] [US1] Integration test src-tauri/tests/orchestrator_tasks.rs: dispatch → executor in hidden thread `origin = 'task'` → report → orchestrator turn with the report → user message; executor tools, approval level and limits unchanged (FR-012)
- [ ] T020 [US1] Implement `orchestrator::tools` (`task_dispatch`, `task_status`, `task_report`) in src-tauri/src/orchestrator/tools.rs; register them only for orchestrator and executor turns respectively
- [ ] T021 [US1] Implement `orchestrator::stages` in src-tauri/src/orchestrator/stages.rs: availability (key present, online, delegate installed, tool capability per 012) and fallback (FR-010)
- [ ] T022 [US1] Implement `orchestrator::scheduler` (single task for now) and `orchestrator::relay` (executor `emit` sink → task state, report → next orchestrator turn) in src-tauri/src/orchestrator/{scheduler,relay}.rs
- [ ] T023 [US1] Implement `orchestrator::prompt` in src-tauri/src/orchestrator/prompt.rs: role, date/time, stages with mapping and "needs haex tools?", running tasks, rules (ask before dispatch when ambiguous, at most three options, coding → delegate, report in own words, never act)
- [ ] T024 [US1] Migration (next free number): `chat_tasks`, `chat_task_questions` exactly as data-model.md (`state` "'queued'|'running'|'waiting'|'done'|'partial'|'failed'|'cancelled'"), `chat_messages.turn_tokens_in/out`, `chat_threads.origin` if not yet on `main`; `HOLZI_TRIGGER_VERSION` +1; storage in src-tauri/src/orchestrator/storage.rs with tests
- [ ] T025 [US1] Route messages: with `orchestration.enabled`, `chat::send::start_turn` starts an orchestrator turn (FR-001); quick acknowledgement on dispatch (FR-005)

---

## Phase 4: User Story 3 - Einfaches beantwortet der Orchestrator selbst (Priority: P1) (PR C)

- [ ] T026 [P] [US3] Integration test in src-tauri/tests/orchestrator_tasks.rs: a scripted orchestrator answering without tools creates no task; `task_status` answers "how far is X" without touching the executor
- [ ] T027 [US3] Measurement src-tauri/tests/orchestrator_routing_eval.rs (`#[ignore]`, release): the 10 simple questions of T003 create no task (SC-002) and answer time per SC-002; record in research.md R7

---

## Phase 5: User Story 5 - Stufen und Orchestrator einstellen (Priority: P2) (PR C)

- [ ] T028 [P] [US5] Rules in scripts/check-orchestration-settings.ts: all `settings.orchestration.*` actions in `guardrails` (FR-025); only models with tool capability offered
- [ ] T029 [US5] Command `orchestration_stage_options` and device preferences `orchestration.*` (data-model.md; `maxParallel` "int 1–6", default 3) with defaults from installed models, keys and delegates in src-tauri/src/orchestrator/settings.rs
- [ ] T030 [US5] Settings page src/components/settings/OrchestrationSetting.vue under KI (route `ai.orchestration`): on/off, orchestrator model, four stages, parallel limit, project folders (from PR F), note of FR-027 when local orchestrator and local stage differ; saved on selection
- [ ] T031 [US5] Cloud orchestrator marker in the chat composer and the 066 indicator; offline fallback to the local model with a one-time note (FR-006)

---

## Phase 6: User Story 6 - Nachvollziehen (Priority: P2) (PR C)

- [ ] T032 [US6] Commands `chat_tasks_list`, `chat_task_get`; event `chat-task-state` in src-tauri/src/orchestrator/commands.rs
- [ ] T033 [US6] Task card src/components/chat/TaskCard.vue in src/components/chat/MessageList.vue: state, stage, model, reason; expandable to brief, executor steps (from the hidden thread), questions with answers, report, times, tokens split by orchestrator and executor (FR-022); store src/stores/tasks.ts

**Checkpoint (PR C)**: orchestration usable with one task at a time.

---

## Phase 7: User Story 2 - Rückfragen vor und während der Ausführung (Priority: P1) (PR D)

**Goal**: Executor approvals and information questions reach the user through the orchestrator;
answers known from the brief are given without the user; the orchestrator never approves.

- [ ] T034 [P] [US2] Relay tests in src-tauri/src/orchestrator/relay_tests.rs: `tool-permission-request` of an executor becomes a question of kind `approval` with task label; `task_answer` on an approval is refused; `chat-choice-request` becomes kind `info`; `answered_by` recorded
- [ ] T035 [P] [US2] Integration test in src-tauri/tests/orchestrator_tasks.rs: executor asks for a folder that the brief contains → orchestrator answers via `task_answer` without user; executor needs approval → user answers → action runs only if `permission::decide` and the user agree (SC-004); unanswered question → task `waiting`, nothing runs
- [ ] T036 [US2] Implement relay of approvals and choices in src-tauri/src/orchestrator/relay.rs and `task_answer` in tools.rs; questions shown in the conversation thread with the task label (frontend prompts in src/composables/useChatChoices.ts route by `taskId`)
- [ ] T037 [US2] Delegates: `DelegateChatContext` gets the task sink so `approval_bridge` requests are relayed the same way (src-tauri/src/adapters/cli_delegate/mod.rs)

---

## Phase 8: User Story 4 - Mehrere Aufgaben gleichzeitig (Priority: P2) (PR E)

- [ ] T038 [P] [US4] Scheduler tests in src-tauri/src/orchestrator/scheduler_tests.rs: up to `maxParallel` running, more queued in order; cancel one by id; vault close cancels all
- [ ] T039 [P] [US4] Target tests in src-tauri/src/orchestrator/targets_tests.rs: overlapping `targets` → `conflict` result (FR-021)
- [ ] T040 [US4] Integration test: three scripted tasks run at once; every event, question and report attributed to its task (SC-005); cancel of one ends only it within 1 s
- [ ] T041 [US4] Implement queue, limit, per-task cancel (`task_cancel`, `chat_task_cancel`) and `orchestrator::targets`; questions of several tasks asked oldest first (US4 scenario 5)
- [ ] T042 [US4] 066 integration: an active end of a voice conversation cancels all its tasks (FR-020); stop word alone cancels the most recent task (spec Edge Cases)

---

## Phase 9: User Story 1 - Delegates im Projektordner (Priority: P1) (PR F)

- [ ] T043 [P] [US1] Tests in src-tauri/src/adapters/cli_delegate/workdir_tests.rs: with `workdir` the CLI runs there, config dir stays temporary; without it the old temp dir is used; a folder without a file grant (044) is refused
- [ ] T044 [US1] Pass `workdir` through `task_dispatch` to `claude.rs` and `codex.rs` (current dir only; `CLAUDE_CONFIG_DIR`/`CODEX_HOME` unchanged)
- [ ] T045 [US1] Project folders: device preference `orchestration.projectFolders` ("JSON [{path, label, lastUsedAt}]"), add/remove in OrchestrationSetting.vue via the folder picker; the orchestrator prompt lists them and asks when unclear (FR-026)

---

## Phase 10: User Story 7 - Wahl übersteuern (Priority: P3) (PR G)

- [ ] T046 [P] [US7] Integration test: a named stage or model is used; "nochmal mit Opus" starts a new task with `retry_of` and the same brief
- [ ] T047 [US7] Command `chat_task_retry {taskId, stage}` and a "Mit anderer Stufe wiederholen" menu on TaskCard.vue

---

## Phase 11: Polish

- [ ] T048 Routing measurement over all 40 tasks with local orchestrator and Haiku (SC-003); record in research.md R7; adjust prompt.rs if below 85 %
- [ ] T049 Cost comparison (SC-006): the same "klein" task with and without orchestration; record in research.md
- [ ] T050 [P] e2e scenario scripts/e2e/scenarios/orchestrator.test.ts with two scripted models behind a local OpenAI-compatible test server
- [ ] T051 [P] Update plans/README.md with a row for 068 after each merged PR
- [ ] T052 Manual quickstart M1–M10; record results in the last PR description

---

## Dependencies & Execution Order

- PR B (Phases 1–2) blocks everything. PR C (Phases 3–6) next. PR D needs C. PR E, F, G need C and are
  independent of each other.
- 066 PR E can use 068 tasks after 068 PR C; 067 PR F uses stage routing after 068 PR C.

### Parallel Opportunities

- Phase 2 tests T005–T008 in parallel.
- PR E, F, G in parallel worktrees after PR C.

## Implementation Strategy

1. PR B: invisible core change, all existing tests green.
2. PR C: orchestrator with one task at a time (MVP), switch default off.
3. PR D: back channel; PR E: parallel tasks; PR F: delegates in project folders; PR G: overrides.
