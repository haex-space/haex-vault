# Tasks: Small local models finish what they start

**Input**: Design documents from `specs/087-agent-chains/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: requested by the spec (every user story has an independent test) and by the constitution (tests in
`*_tests.rs` next to each module). Test tasks come before the code they test.

**Order**: the phases follow the plan's delivery order, not the spec's priority order. US1 (the measurement) comes
first because every step is judged by it. Then the deterministic steps (US4), then the wording steps (US5, US2, US3),
each measured on its own (spec Decision 3).

**Rules for every task**:

- Run `graphify query "<what the function does>"` before writing a new function, and reuse what exists.
- Build in the 10 GB scope with `-j 1`; run the lib tests with `--config 'profile.dev.package.holzi.debug=0'`; build
  the CUDA eval binary per spec 082's quickstart.
- Keep each file under 500 lines; tests in `*_tests.rs` next to the module.
- Tune wording only on `specs/087-agent-chains/analysis/chainset.json` (and the routing development set of spec 082),
  never on the everyday set (FR-012).
- **Measuring a step**: (1) the chain set for Qwen3.5 4B and Gemma 4 E4B per candidate, into
  `specs/087-agent-chains/runs/chain/<step>/`; (2) for the chosen candidate the everyday set for both target models
  and Gemma 4 E2B and Qwen3 1.7B (phone), into `runs/steps/<step>/`; (3) the hosted models (Opus, Sonnet, Haiku 5.5)
  when the step changes what every model reads (research R9). Compare request by request with the baseline
  (`requests[]`), and each target model request by request with the preceding kept state (the baseline when none was
  kept), recording that incremental result in `results.md`. Keep the step only if neither target model nor a hosted model passes fewer requests than in the
  baseline (FR-002); otherwise rework or drop it and note why in `results.md`. Report each step's effect on the
  preceding kept state as well, request by request, so a step that loses passes the earlier steps hid is seen; the
  keep threshold stays the baseline.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: the user story of the task (US1 to US6)

Paths are relative to the repository root. `chat/` stands for `src-tauri/src/chat/`.

---

## Phase 1: Setup

**Purpose**: the base is current and the tools to measure are ready.

- [x] T001 Confirm that #903 and #904 (a local model's context and request fitting, spec 086) are merged into
      `main` (`gh pr view <n> --json state`) before the baseline (T012); #853, #856 and #857 are merged. Rebase
      `087-agent-chains` onto `origin/main` (no force-push of a pushed branch; the branch is not pushed yet).
- [x] T002 Query the graph for the everyday report (`chat/eval/everyday/report.rs`), the runner's routing record
      (`chat/eval/everyday/runner.rs`), the set loading (`chat/eval/everyday/mod.rs`), `plan_calls`
      (`chat/turn/plan_calls.rs`), the clock line and the instruction (`chat/tools/prompt.rs`) and the routed offer
      (`chat/tools/offer.rs`); confirm the call sites of plan.md.
- [x] T003 Create `specs/087-agent-chains/runs/` with `baseline/`, `chain/`, `steps/` and `final/`, and copy the
      measuring script of spec 082 (`measure082.sh` pattern: one model per call, report copied as `<model>.json`)
      as a scratch script that takes `STEP`, `ONLY` and `HAEX_EVAL_SET`.

---

## Phase 2: User Story 1 - The baseline is the app as it is now (Priority: P1) 🎯 MVP

**Goal**: every report says where each request was routed, a chain set exists, and every model has a baseline on
the current app.

**Independent Test**: a report of a scripted run lists every request with `routed`, `routingHit` and `firstOffer`;
the chain set loads through `HAEX_EVAL_SET`; `runs/baseline/` holds a report per model.

### Tests for User Story 1

- [x] T004 [P] [US1] Add tests in `chat/eval/everyday/report_tests.rs`: the report has `set` and one `requests[]`
      entry per request in the set's order, passed and failed, each with `id`, `pass`, `routed`, `routingHit` and
      `firstOffer` (sorted, in the set's names `x_mail_<tool>`), per `contracts/report-routing.md`.
- [x] T005 [P] [US1] Add tests in `chat/eval/everyday/mod_tests.rs`: a set file in the everyday format loads from a
      path; a file whose expected tool is unknown fails before the first request, with the tool's name.
- [x] T006 [P] [US1] Add a test in `chat/eval/everyday/runner_tests.rs`: with routing, a scripted run's record for a
      mail request holds the routed domains and the offered `x_mail_find_messages` in the set's name.

### Implementation for User Story 1

- [x] T007 [US1] Write the routing record per request in `chat/eval/everyday/report.rs` (`requests[]`, `set`) from
      what `chat/eval/everyday/runner.rs` already keeps (`routed`, the hit, the first offer translated with
      `MailNames::in_set`); `runner.rs` only hands the record on (research R10).
- [x] T008 [US1] Load a set from a path in `chat/eval/everyday/mod.rs` with the same checks as the embedded set, and
      read `HAEX_EVAL_SET` in `src-tauri/tests/model_tool_eval.rs` (`measure_everyday_voice`); the report's `set` is
      `everyday` or the file name.
- [x] T009 [US1] Update spec 082's report contract `specs/082-agent-harness/contracts/everyday-report.md` to point to
      `specs/087-agent-chains/contracts/report-routing.md` for the per-request fields.
- [x] T010 [US1] Write the chain set `specs/087-agent-chains/analysis/chainset.json` per `contracts/chain-set.md`:
      about 36 requests (German and English, clean and spoken) on the fixture world, at least 4 each for looking up
      (a sender, subject or day named: Julia Wagner, IT-Support, Ingrid Berger, Stadtwerke), finishing a chain (show
      the Amazon, GitHub, WLAN or VPN password; answer Julia; move the Stadtwerke mail), deleting (the IT-Support mail
      into the trash; one "for good"), a day's bounds (today, the day before yesterday); expectations in the
      everyday set's form. Check that it loads (T008) and that a scripted run of its expected chains passes every
      request.
- [x] T011 [US1] Run the lib tests of `chat::eval` and `chat::tools`, the `chat_tool_loop_*` integration tests, clippy
      for the desktop and for Android, and `pnpm typecheck:scripts`.
- [x] T012 [US1] Take the baseline on the everyday set for Qwen3.5 4B, Gemma 4 E4B, Gemma 4 E2B, Qwen3 1.7B (phone)
      and the hosted models into `specs/087-agent-chains/runs/baseline/`, and the chain set for both target models
      into `runs/chain/baseline/`.
- [x] T013 [US1] Write `specs/087-agent-chains/results.md` "Baseline": totals, categories, the failure causes of
      `failures.py` per target model, and for Gemma 4 E4B's tool searches whether they were routed to mail (from
      `routed`); compare with spec 082's `runs/names/haex-mail/` and say which differences come from #853/#856 and from the context of #903/#904.

**Checkpoint**: every later step has a baseline to be compared with, request by request.

---

## Phase 3: User Story 4 - An argument counts as the tool means it (Priority: P2)

**Goal**: values in the wrong form run as meant (R5), and a day runs to its end (R4).

**Independent Test**: unit tests of the value forms pass without a model; the everyday set shows fewer `bad_args`
for Qwen3.5 4B.

### Tests for User Story 4

- [x] T014 [P] [US4] Add tests in `chat/turn/arg_forms_tests.rs` per `contracts/value-forms.md`: `"True"`/`"false"`
      for a boolean, `"5"` for an integer, `"2.5"` for a number become typed values; `"yes"`, `"1"` for a boolean,
      `"five"`, `"2.5"` for an integer, nested properties and properties without a single declared type stay as
      sent.
- [x] T015 [P] [US4] Add a test in `src-tauri/tests/chat_tool_loop_arg_forms.rs` (the permissions test file is past
      500 lines): a risky call with `"True"` for a
      boolean argument shows `true` in the permission request and stores the call with `true`.
- [x] T016 [P] [US4] Add tests in `chat/tools/prompt_tests.rs`: the clock line names today, yesterday and tomorrow
      with their dates and says that a day runs from its date to the next.

### Implementation for User Story 4

- [x] T017 [US4] Create `chat/turn/arg_forms.rs` (register it in `chat/turn/mod.rs`) and call it in
      `chat/turn/plan_calls.rs` before the guard, so the dialog, the tool and the stored call get the turned
      arguments; use the same function in the everyday runner (`chat/eval/everyday/runner.rs`) before `turn_away`.
- [x] T018 [US4] Measure the value forms (one everyday run per target model, phone models, hosted models) into
      `runs/steps/value-forms/`; keep or drop by the rule above; note it in `results.md`.
- [x] T019 [US4] Extend the clock line in `chat/tools/prompt.rs` (`now_line`) with yesterday's and tomorrow's dates
      and the sentence on a day's bounds; tune the sentence on the chain set (day requests of T010) with both target
      models, into `runs/chain/day-bounds/<candidate>/`.
- [x] T020 [US4] Measure the chosen clock line on the everyday set (target, phone and hosted models) into
      `runs/steps/day-bounds/`; keep or drop; note it in `results.md`.

**Checkpoint**: values and days are no longer a cause, or the step is recorded as dropped with its numbers.

---

## Phase 3b: User Story 1 - The world answers truthfully, the chain set covers every area (operator 2026-11-09)

**Goal**: the measurement tells a model the truth in every area (R12), the chain set tunes for every area (R13), and
every later step is compared with a baseline on that world.

**Independent Test**: no reading tool of holzi's areas is answered with a bare `{"ok": true}`; the chain set's
expected chains pass in a scripted run; `runs/baseline-world/` holds a report per model.

- [x] T048 [P] [US1] Add tests in `chat/eval/everyday/world_app_tests.rs`: every reading tool of `chat/eval/tools.json`
      gets an answer other than `{"ok": true}`; the conversations list holds no person's conversation; a
      conversation's messages carry no text; an unknown conversation id, file path or sync rule is an error; a known
      file's details match `files_search`; changing tools still answer a plain success.
- [x] T049 [US1] Answer holzi's other reading tools in a new `chat/eval/everyday/world_app.rs` (registered in `mod.rs`),
      called from `FixtureWorld::answer` before the plain success; add `conversations` and `models` to
      `chat/eval/everyday/world.json` (R12).
- [x] T050 [US1] Extend `specs/087-agent-chains/analysis/chainset.json` across areas (R13): contacts, files, passwords,
      settings and holzi's conversations, and requests that name only a person or a thing; at least a third not mail;
      nothing the everyday set asks; `chainset_tests.rs` passes.
- [x] T054 [US1] Score invented ids in every area (research R12): `chat/eval/everyday/ids.rs` reads a tool's id
      arguments from its schema as the guard does, tests first in `ids_tests.rs`; `scoring.rs` and `consent.rs` use
      it instead of their mail lists.
- [x] T055 [US1] A name in the user's message is no id (R14, FR-020): `chat/turn/guard.rs` `seen_text` keeps only
      the words of a user message that look like an id, tests first in `chat/turn/guard_tests.rs`; measure the
      everyday set for every model into `runs/steps/user-ids/`; note it in `results.md`.
- [x] T051 [US1] Run the lib tests of `chat::eval`, clippy for the desktop and for Android.
- [x] T052 [US1] Take the new baseline on the everyday set for every model of T012 into
      `specs/087-agent-chains/runs/baseline-world/`, and the chain set for both target models into
      `runs/chain/baseline-world/`; write `results.md` "Baseline on the truthful world" with the difference to T018
      request by request. Every later step is compared with this baseline.

**Checkpoint**: the world and the chain set no longer favour mail.

---

## Phase 4: User Story 5 - Deleting a mail moves it to the trash (Priority: P2)

**Goal**: "Delete the mail" leads to the move into the trash, for any mail extension (R8).

**Independent Test**: on the chain set and the everyday set, delete requests without "for good" end with the mail in
the trash; the same under the extension name `thunderbird`.

### Tests for User Story 5

- [x] T021 [P] [US5] Add tests in `chat/tools/offer_tests.rs` (or the new module's `*_tests.rs`): a final tool's
      description in an offer ends with holzi's sentence; a reversible tool's does not; holzi's own deleting actions
      are final too.
- [x] T022 [P] [US5] Add tests in `chat/tools/routed_offer_tests.rs`: for a message with "lösch"/"delete", a domain's
      final tools come after its reversible change tools; the reading tools still come first.

### Implementation for User Story 5

- [x] T023 [US5] Add holzi's sentence to the description of every final tool when it is offered (R8.2), in
      `chat/tools/offer.rs` or a new `chat/tools/final_tools.rs` if `offer.rs` passes 450 lines; the effect comes from
      the tool (`RiskClass`, the extension's declared effect), never from its name. Tune the sentence on the chain set
      (delete requests of T010) into `runs/chain/delete-sentence/<candidate>/`.
- [x] T024 [US5] If T023 does not make both target models pass the chain set's delete requests, order final tools
      after reversible ones in `routed_offer` (R8.1) and measure on the chain set into `runs/chain/delete-order/`.
- [x] T025 [US5] Only if T023 and T024 both fail: after a declined final call, the answer names the step's
      reversible move tool as what may have been meant (R8.3), in `chat/turn/guard.rs` (`next_step`) and the
      everyday runner's played decline; tests first in `chat/turn/guard_tests.rs`.
- [x] T026 [US5] (Not run: no lever was kept, results.md "Step: deleting into the trash".) Measure the kept delete step on the everyday set (target, phone and hosted models) into
      `runs/steps/delete/`, and the target models also under `HOLZI_EVAL_MAIL_NAME=thunderbird` with
      `HOLZI_EVAL_MAIL_TITLE=Thunderbird` into `runs/steps/delete-thunderbird/` (FR-011); note it in `results.md`.

**Checkpoint**: SC-007 can be read off the step's reports.

---

## Phase 5: User Story 2 - The model looks it up instead of asking (Priority: P1)

**Goal**: with the area's tools offered, the model searches with what the user said before asking or searching for
tools (R6).

**Independent Test**: on the chain set, look-up requests start with the offered search; on the everyday set, Gemma 4
E4B's failures of the first lever (research R1) go down.

### Tests for User Story 2

- [x] T027 [P] [US2] (Written with l1, removed with it.) Add tests in `chat/tools/prompt_tests.rs` for the new instruction sentence: it is in the
      instruction with and without the tool search, and only once.
- [x] T028 [P] [US2] (Written with s1, removed with it.) Add tests in `chat/tools/find_actions_tests.rs` (new, registered in `find_actions.rs`): when
      the step already offers a found tool, the result marks it as already offered and says to call it; tools not
      offered are listed as today.

### Implementation for User Story 2

- [x] T029 [US2] Decide from the baseline's `routed` (T013) whether Gemma 4 E4B's tool searches were routed to mail.
      Answered in `results.md` "Baseline": 15 of 17 were routed right, so the look-up levers (T030–T032) apply to
      Gemma 4 E4B; Qwen3.5 4B's routing misses get their own, general step (T044–T047, T053).
- [x] T030 [US2] Add the look-up sentence to `TOOL_INSTRUCTION` and `TOOL_INSTRUCTION_WITHOUT_SEARCH` in
      `chat/tools/prompt.rs` (R6.1); tune it on the chain set into `runs/chain/lookup-sentence/<candidate>/`.
- [x] T031 [US2] Make `find_actions` name already offered tools in its result (R6.2) in `chat/tools/find_actions.rs`
      (the step's offer passed in from `chat/turn/tool_round.rs` and the everyday runner); measure on the chain set
      into `runs/chain/lookup-search-answer/`.
- [x] T032 [US2] Only if neither T030 nor T031 moves Gemma 4 E4B on the chain set: offer no tool search in a routed
      first step (R6.3) in `chat/tools/route.rs`/`chat/tools/offer.rs`, tests first in
      `chat/tools/routed_offer_tests.rs`; measure on the chain set into `runs/chain/lookup-no-search/`.
- [x] T033 [US2] Measure the kept look-up step on the everyday set (target and phone models; hosted models if the
      instruction changed) into `runs/steps/lookup/`; watch the `clarify` category (FR-006); note it in `results.md`.

**Checkpoint**: SC-006's first half (asks instead of searching) can be read off the step's reports.

### A person or a thing, not an area (US2, research R11; operator 2026-11-09)

- [x] T044 [US2] Rebuild llama-server from the llama.cpp revision holzi pins (CUDA, `CUDAARCHS=86`) for the offline
      routing measurement, and run spec 082's `analysis/router_eval.py` on its `analysis/devset.json` for Qwen3.5 4B
      and Gemma 4 E4B as the routing baseline, into `specs/087-agent-chains/runs/route/baseline.txt`.
- [x] T045 [US2] Write `specs/087-agent-chains/analysis/routeset.json` (never the everyday set): requests from every
      area that name only a person or a thing ("Antworte Anna", "Was hat Tom geschickt?", "Wo ist der Mietvertrag?"),
      each with every area that may hold it; a routing is right when all of them are among the routed areas.
- [x] T046 [US2] Tune the routing prompt (`chat/tools/route.rs`, R11.1) and holzi's domain descriptions
      (`src/lib/actions/agentTools.ts` `AGENT_DOMAINS` and the export, R11.3) on the development sets of T044/T045,
      candidate by candidate into `runs/route/<candidate>.txt`; no wording names a particular area or extension
      (FR-019), an extension's line stays built from its tools (spec 082 #847); tests first in `route_tests.rs`.
- [x] T053 [US2] Add the sentence to look in the next offered area when a search finds nothing (R11.2, FR-018) to
      the instruction in `chat/tools/prompt.rs`, tests first in `chat/tools/prompt_tests.rs`; tune it on the chain
      set into `runs/chain/next-area/<candidate>/`. Kept c1 for routed turns only (`system_prompt(…, routed)`).
- [x] T047 [US2] Measure the kept routing and next-area steps on the everyday set (target and phone models; hosted
      models only if the instruction changed, since they do not route) into `runs/steps/routing/`; note it in
      `results.md`.

---

## Phase 6: User Story 3 - A started chain is finished (Priority: P1)

**Goal**: after finding what the request is about, the model does what was asked (R7).

**Independent Test**: on the chain set, show/answer/move requests end with the right last call; on the everyday set,
`incomplete_chain` failures go down while `clarify` holds.

### Tests for User Story 3

- [x] T034 [P] [US3] Add tests in `chat/tools/prompt_tests.rs` for the finishing sentence (present once, with and
      without the tool search).
- [x] T035 [P] [US3] (Not needed: R7.2 was not built.) Only for the candidate R7.2: tests in `chat/turn/guard_tests.rs` (or a new module's tests) that a
      reading call's successful result with exactly one match ends with one next-step sentence naming the step's
      offered tools that act on it, and that results with none or several matches stay unchanged.

### Implementation for User Story 3

- [x] T036 [US3] Add the finishing sentence to the instruction in `chat/tools/prompt.rs` (R7.1); tune it on the chain
      set into `runs/chain/finish-sentence/<candidate>/`.
- [x] T037 [US3] (Not run: c1 reduced the chains to finish, results.md.) Only if T036 does not reduce the chain set's stopped chains for both target models: add the next
      step after a successful result (R7.2) in `chat/turn/tool_round.rs` and the everyday runner; measure on the
      chain set into `runs/chain/finish-next-step/`.
- [x] T038 [US3] Measure the kept finishing step on the everyday set (target, phone and hosted models) into
      `runs/steps/finish/`; note it in `results.md`.

**Checkpoint**: SC-003 and SC-006 can be read off the step's reports.

---

## Phase 7: User Story 6 - Hosted models and phone models with every step, and the final run (Priority: P2)

**Goal**: the final numbers for every model, and the target models under another extension name.

**Independent Test**: `runs/final/` holds a report per model of the baseline; `results.md` checks every success
criterion.

- [x] T039 [US6] Run the everyday set on the final state for every model of the baseline into
      `specs/087-agent-chains/runs/final/`, and the target models under `HOLZI_EVAL_MAIL_NAME=thunderbird` with
      `HOLZI_EVAL_MAIL_TITLE=Thunderbird` into `runs/final/thunderbird/` (FR-015).
- [x] T040 [US6] Write `results.md` "Final": a table per model (baseline, each kept step, final), categories, the
      failure causes, and SC-001 to SC-008 each with met / not met and the numbers.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [x] T041 Update `specs/087-agent-chains/quickstart.md` with what changed during the steps (variables, scripts),
      and its in-app checks with what the final run shows.
- [x] T042 Run the full checks before the PR: all lib tests, the `chat_tool_loop_*`, `action_*`, `mcp_server_calls`
      and `chat_message_idempotency` integration tests, clippy for the desktop and for Android, `check:agent-actions`,
      `typecheck:scripts`, `typecheck`, ESLint and Prettier; strip the ts-rs binding drift.
- [ ] T043 Write the PR description (no Claude trailer): the steps kept and dropped with their numbers, the success
      criteria, the in-app checks for both target models; dispatch E2E with `gh workflow run ci.yml --ref
087-agent-chains` before the merge.

---

## Dependencies & Execution Order

- **Setup (T001–T003)** first; T001 needs #903 and #904 merged before the baseline (T012); the code
  tasks T004–T011 do not wait for them.
- **US1 (T004–T013)** blocks every other story: no step is judged without its baseline. **T048–T052** follow the
  value forms (T018) and give the baseline for every later step.
- **US4 (T014–T020)**, then **US5 (T021–T026)**, then **US2 (T027–T033, then the routing step T044–T047 and T053)**, then **US3 (T034–T038)**: each step is
  measured against the baseline with the steps before it kept, so the order is fixed; within a story, tests come
  before code and candidates are tried in the order given.
- **US6 (T039–T040)** after the last kept step; **Polish (T041–T043)** last.

## Parallel Opportunities

- T004, T005 and T006 (different test files); T014, T015 and T016; T021 and T022; T027 and T028; T034 and T035.
- Writing the chain set (T010) while the code of T007/T008 builds.
- Chain-set runs of a candidate for the two target models run one after the other on the one GPU, but writing the
  next candidate's tests can go on meanwhile.

## Implementation Strategy

1. **MVP**: Setup and US1. The report and the chain set alone already answer research R2 (were Gemma's tool
   searches routed to mail?), which decides the look-up step.
2. **Deterministic first**: US4's value forms need no tuning and are cheap to measure.
3. **Wording steps one at a time** (day bounds, delete, look-up, finishing), each tuned on the chain set and kept by
   the everyday numbers.
4. **Final run and results**, then the PR.
