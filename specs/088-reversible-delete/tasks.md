# Tasks: The reversible way first

**Input**: Design documents from `specs/088-reversible-delete/` (plan.md, spec.md, research.md, data-model.md,
contracts/, quickstart.md)

**Tests**: included; holzi keeps unit tests in `*_tests.rs` next to each module.

**Build rule**: every cargo build/test inside the 10 GB scope with `-j 1` (quickstart.md); never rebuild the eval
binary while a measurement runs. `graphify query` before each new function.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [x] T001 Merge `main` (spec 087, PR #939, and the routing descriptions of PR #942 have landed) and note the merge
      commit in specs/088-reversible-delete/results.md; run the baseline without the check on it (chain set for both
      target models into `runs/chain/baseline/`, everyday set for the four local profiles into `runs/baseline/`), the
      numbers SC-002 to SC-004 compare against

---

## Phase 2: Foundational (blocking)

- [x] T002 Move the per-call planning of the everyday runner (registry lookup, `callable`, `turn_away`, played
      dialog, the `TraceCall` it pushes) out of src-tauri/src/chat/eval/everyday/runner.rs into a new
      src-tauri/src/chat/eval/everyday/plan_call.rs, with no behaviour change; runner.rs below 470 lines
- [x] T003 Run the everyday lib tests (`chat::eval::everyday`) and confirm they pass unchanged after T002
- [x] T004 [P] Write src-tauri/src/chat/tools/reversible_tests.rs: candidates (same `domain_of`, risk `Change`,
      offered only; empty when none), the question text (contracts/check-question.md: tool names and English titles, no
      area word), the messages (earlier user messages kept; the assistant's last answer included only when it ends with a
      question; tool calls and results left out), parsing (`final`, a candidate name with and without the `x_<slug>_` prefix, `none`, junk, a name that is
      not a candidate)
- [x] T005 Create src-tauri/src/chat/tools/reversible.rs: `Candidate`, `Verdict { Final, Use(String), NoVerdict }`,
      `candidates(tool, registry, offered)`, `check_prompt(...)`, `parse_check(reply, candidates)`, and the async
      `check(call: &RouteCall, messages, tool, candidates) -> Check` built like `route::route_in` (no tools, thinking off,
      16 tokens, `call.timeout`); register the module in src-tauri/src/chat/tools/mod.rs; T004 passes
- [x] T006 [P] Add `Failure::NotFinal { tool, candidate, title }` and its next step (research R6) to
      src-tauri/src/chat/turn/guard.rs, with a test in src-tauri/src/chat/turn/guard_tests.rs (done as
      `Failure::NotFinal { candidate, title }`; the whole answer is tested in reversible_tests.rs, since guard_tests.rs
      is at 481 lines)

**Checkpoint**: shared check and answer exist; nothing calls them yet.

---

## Phase 3: User Story 1 - An ordinary delete takes the reversible way (P1) 🎯 MVP

**Goal**: a final proposal the user did not ask for is turned away with the reversible tool named.

**Independent Test**: unit test with a fake adapter answering a candidate name: the plan is `TurnedAway` with the
NotFinal answer; in the app, "Lösch die Mail von Bernd" ends in the trash.

- [x] T007 [US1] Add a per-turn set of checked tool names to `TurnRunner` in src-tauri/src/chat/turn/mod.rs
- [x] T008 [US1] In src-tauri/src/chat/turn/plan_calls.rs, after the guard and the `callable` check and before
      `decide_call`: for a `Risky` tool that does not confirm itself, with mode not Plan, `RouteCall::of(session,
model_id)` present, not yet checked this turn and with candidates, run `reversible::check` on the conversation's messages
      (research R4); on `Use(name)` push `ToolPlan::TurnedAway` with the NotFinal answer; mark the tool checked
- [x] T009 [US1] Test in src-tauri/tests/chat_tool_loop_reversible.rs (`plan_calls` needs a vault and a chat state,
      which the tool-loop fixture has): a fake adapter answering a candidate turns the call away with the candidate's
      name in the answer; the same tool proposed again in the turn goes to the dialog without a second check (FR-006);
      the candidate tool is never executed by holzi, only the turn-away answer comes back (FR-007); a conversation
      whose assistant asked "delete for good?" and whose user answered "yes" passes the question with both messages
      (FR-002)
- [x] T010 [US1] Log one line per check (verdict, ms, no user text) in src-tauri/src/chat/tools/reversible.rs, like
      `route::log_route`

**Checkpoint**: the app turns ordinary final proposals away.

---

## Phase 4: User Story 2 - "For good" stays for good (P1)

**Goal**: a proposal the user asked for reaches the dialog unchanged.

**Independent Test**: fake adapter answering `final`: the plan is `Ask` as today.

- [x] T011 [US2] Test in src-tauri/tests/chat_tool_loop_reversible.rs: answer `final` leads to `Ask` in Auto mode;
      a decline still yields the `Denied` answer and nothing else runs

---

## Phase 5: User Story 3 - No reversible way, no change (P1)

**Goal**: no candidates, `none`, junk, error, timeout, Plan mode, a self-confirming tool or a hosted model: today's
behaviour, and no model call where the check does not apply.

**Independent Test**: unit tests where the fake adapter must not be called, or answers junk / times out.

- [x] T012 [US3] Tests in src-tauri/tests/chat_tool_loop_reversible.rs and
      src-tauri/src/chat/tools/reversible_tests.rs: no candidates → no adapter call, `Ask`; reply `none` / junk / timeout → `Ask`; Plan mode → `Deny` without a call; a hosted session (`RouteCall::of` is `None`) →
      no call; a `confirms_itself` tool → no call

---

## Phase 6: User Story 4 - Measured as the app runs it (P2)

**Goal**: the everyday runner runs the check and reports it (contracts/report-checks.md).

**Independent Test**: runner test with a scripted model: a final proposal is turned away, `checks` lists it, the
next call of the named tool runs and passes the request.

- [x] T013 [US4] In src-tauri/src/chat/eval/everyday/plan_call.rs run `reversible::check` with the runner's
      `RouteCall` (routing runs only) under the same conditions as T008; a turned-away call ends `TurnedAway`
- [x] T014 [US4] Add `checks` per request and `turnedToReversible` per run to
      src-tauri/src/chat/eval/everyday/report.rs, set names in `checks[].tool` / `checks[].use`; tests in
      src-tauri/src/chat/eval/everyday/report_tests.rs
- [x] T015 [US4] Test in src-tauri/src/chat/eval/everyday/plan_call_tests.rs: `mail_tidy-de-1` with a scripted
      delete proposal and a check answering the move tool passes once the model moves the mail; `mail_tidy-de-4` with
      `final` reaches the played dialog, which agrees
- [x] T016 [US4] Update `contracts/everyday-report.md` references: add a pointer to
      specs/088-reversible-delete/contracts/report-checks.md in specs/082-agent-harness/contracts/everyday-report.md

**Checkpoint**: unit tests, clippy (desktop and aarch64-linux-android), `pnpm typecheck:scripts` green.

---

## Phase 7: Measurement

- [x] T017 Chain set, Qwen3.5 4B and Gemma 4 E4B, deterministic, into specs/088-reversible-delete/runs/chain/c1/;
      compare request by request with spec 087's final chain run; read every `checks` entry
- [x] T018 If SC-002 is missed: tune only the question or the answer wording (contracts/check-question.md) on the
      chain set, one candidate per run into `runs/chain/c<n>/`; record each in specs/088-reversible-delete/results.md
- [x] T019 Keep or drop: keep if SC-002 holds and no chain-set area falls below spec 087's final chain run; otherwise
      record the numbers and remove the wiring (T008, T013), keeping results.md
- [x] T020 Chain set under `thunderbird` for both target models (SC-002, second half)
- [x] T021 Everyday set for Qwen3.5 4B, Gemma 4 E4B, Gemma 4 E2B and the phone profile into
      specs/088-reversible-delete/runs/final/; check SC-001, SC-003, SC-004, SC-005
- [x] T022 Read the answers of every request with a turned-away proposal (SC-006) and record any false "done"
- [x] T023 Measure the check's time on the desktop reference machine from `checks[].ms` (SC-007; the time of whole
      checked turns is not recorded, see results.md Time: open)

---

## Phase 8: Polish

- [x] T024 Write specs/088-reversible-delete/results.md: per-step table, SC table, what was tuned and dropped
- [x] T025 Note in specs/087-agent-chains/results.md that SC-007 is replaced by spec 088 SC-001, with the numbers
- [x] T026 Update specs/088-reversible-delete/quickstart.md "In the app" with what the final run shows; mark tasks
      done; PR

---

## Dependencies & Execution Order

- T001 before any measurement (T017+); code tasks may start before it.
- T002 → T003 → T013 (runner split before the measurement wiring).
- T004 ∥ T006; T004 → T005 → T007–T015.
- US1 (T007–T010) before US2/US3 tests (same file, plan_calls_tests.rs).
- US4 (T013–T016) after T005 and T006.
- Phase 7 after all code tasks; T018 only if T017 misses SC-002; T020–T023 only if T019 keeps the check.

## Parallel Opportunities

- T004 (reversible tests) and T006 (guard failure) touch different files.
- T014 (report) can run beside T013 once T005 exists.

## Implementation Strategy

MVP is Phases 1–3 plus the US3 tests: the app takes the reversible way and changes nothing elsewhere. The
measurement (US4) then decides on the chain set whether the check is kept (T019).
