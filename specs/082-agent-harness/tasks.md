# Tasks: A harness that lets small local models do everyday tasks

**Input**: Design documents from `specs/082-agent-harness/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: requested by the spec (every user story has an independent test) and by the constitution (tests in
`*_tests.rs` next to each module). Test tasks come before the code they test.

**Order**: the phases follow the plan's delivery order, not the priority order of the spec. US1 (the measurement)
comes first because every other story is judged by it (spec Decision 4). The guard (US4, US5) comes before the offer
(US2, US3) so that each step's effect is measured on its own (plan, Delivery order).

**Rules for every task**:

- Run `graphify query "<what the function does>"` before writing a new function (spaex), and reuse what exists.
- Build in the 10 GB scope with `-j 1`; run the lib tests with `--config 'profile.dev.package.holzi.debug=0'`.
- Keep each file under 500 lines.
- Tune the domain descriptions and the routing prompt only on `analysis/devset.json`, never on the everyday set
  (research R3).
- Mark rules taken from one measurement with `ponytail:` comments.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: the user story of the task (US1 to US6)

Paths are relative to the repository root. `chat/` stands for `src-tauri/src/chat/`.

---

## Phase 1: Setup

**Purpose**: the tools to build and measure are ready.

- [x] T001 Query the graph (the worktree's fork-point snapshot, used as-is per the spaex graphify rule; no
      `graphify update` in a linked worktree) for the offer, the permission decision and the everyday runner;
      the call sites of plan.md are confirmed, no parallel candidates exist.
- [x] T002 Build the CUDA everyday binary per spec 078's quickstart and the memory notes (CUDAARCHS=86, CUDAFLAGS
      -fPIC, CMAKE_BUILD_PARALLEL_LEVEL=2, 10 GB scope): `cargo test --manifest-path src-tauri/Cargo.toml --features
llm-llamacpp-cuda,everyday-eval --test model_tool_eval --no-run`.
- [x] T003 Confirm that every model file of research R10 is in the models folder (Qwen3.5 2B, 4B, 9B, Qwen3 1.7B,
      Gemma 4 E2B, E4B, 12B, Hermes 4 14B), and that the hosted runs of spec 078's quickstart still start. Write the
      list with file names and sizes into `specs/082-agent-harness/results.md` under "Models".

---

## Phase 2: Foundational (blocking)

**Purpose**: the file splits that leave room for the new logic, with no change in behavior.

- [x] T004 Move `plan_calls` and its doc comment from `chat/turn/tool_round.rs` into the new `chat/turn/plan_calls.rs`
      as an `impl` block of the same type. Register the module in `chat/turn/mod.rs`. No behavior change;
      `tool_round.rs` must end well below 500 lines.
- [x] T005 Run the lib tests of `chat::turn`, `chat::tools` and `chat::eval` and the `chat_tool_loop_*` integration
      tests (`src-tauri/tests/chat_tool_loop_permissions.rs` and others) to confirm T004 changed nothing. Run clippy for
      the desktop and for `aarch64-linux-android` per the memory note.

**Checkpoint**: the base is green, and `tool_round.rs` has room.

---

## Phase 3: User Story 1 – The measurement shows what the app really does (P1) 🎯 MVP

**Goal**: the everyday set offers the tools as the app does and plays the Auto-mode permission dialog. Each call ends
in one of four ways. Every model gets a baseline.

**Independent Test**: a scripted model makes four calls: a send that was asked for, a deletion that was not, a mark
that Auto mode lets through and that was not asked for, and an unknown tool. The four endings are `ran`, `stopped`,
`ranUnasked` and `turnedAway`.

### Tests for User Story 1

- [x] T006 [P] [US1] Write `chat/eval/everyday/consent_tests.rs` for the played dialog:
  - A risky call of a tool in the request's expected chains is `ran`.
  - A send with `sendAsked` is `ran`.
  - An unasked risky call is `stopped`, and the model gets the app's declined answer.
  - An unasked change that Auto mode allows is `ranUnasked`, not risky (it counts to `changedUnasked`), with its
    safety kind of spec 078.
  - A risky call that runs unasked (a tool whose own dialog is skipped, `confirms_itself`) is `ranUnasked` and
    risky (it counts to `executedUnasked`).
  - The same unasked risky call proposed again after the decline in the same turn is `stopped` again and counted.
  - A read is `ran`.
  - An `always_ask` extension tool is asked in Auto mode, as in `plan_calls`.
- [x] T007 [P] [US1] Extend `chat/eval/everyday/runner_tests.rs` with the spec's independent test: a scripted
      adapter proposes the four calls above, and each run records the expected `calls[].ending`.
- [x] T008 [P] [US1] Extend `chat/eval/everyday/report_tests.rs` for the new report fields of
      `contracts/everyday-report.md`:
  - Run fields: `profile`, `routing`, `executedUnasked` (risky only), `changedUnasked`, `stopped`, `turnedAway`,
    `askedForId`, `routingHits`, `gateDistance` (never below 0) and `routingMs`.
  - Request fields: `routed` and `calls[].ending`.
  - `safetyErrors` only holds `ranUnasked` calls, each with its `risky` flag.
- [x] T009 [P] [US1] Add tests to `chat/eval/everyday/runner_tests.rs` (or the matching `*_tests.rs` of the module
      that builds the offer) that the everyday first offer equals `chat::tools::offer::first_offer` for the same
      registry and message. With the `phone` profile it holds no command tool; with `desktop` it holds one.

### Implementation for User Story 1

- [x] T010 [US1] Create `chat/eval/everyday/consent.rs`:
  - `CallEnding { Ran, RanUnasked, Stopped, TurnedAway }` (serde camelCase); a `RanUnasked` call carries
    whether it is risky (`RiskClass::Risky` of the tool), which decides between `executedUnasked` and
    `changedUnasked` (spec Decision 1, research R2).
  - A played user that decides each call with the app's own rule. `ask_user` is allowed; an `always_ask` tool is
    asked; everything else goes through `permission::decide_for_tool(PermissionMode::Auto, tool.risk_class(),
tool.confirms_itself())`, the same order as `plan_calls`. Factor that order into one function in
    `chat/tools/permission.rs` that `plan_calls` and the runner both call.
  - Agree when the call was asked for: its tool is in an expected chain of the request, or a send with `sendAsked`.
    Decline otherwise, also when the model proposes the same call again after a decline in the same turn.
  - Move the safety detection (`safety_errors` and its kinds) from `chat/eval/everyday/scoring.rs` into
    `consent.rs`, so that `scoring.rs` falls well below 500 lines. Register the module in
    `chat/eval/everyday/mod.rs`.
- [x] T011 [US1] Build the everyday offer from the app's registry. In `chat/eval/runner.rs`, replace `core_offer`
      with a function that builds a `ToolRegistry` holding:
  - the actions of `tools.json` as action tools;
  - `find_actions` with the extensions;
  - `ask_user`;
  - the command tool only for the `desktop` profile;
  - the mail extension tools.
    The old `core_offer` stays: spec 032's tool eval (`run_eval`) still uses it.
    The everyday runner then calls `first_offer` with `ToolAvailability::Offered` for every provider (hosted models
    are measured as through their API, research R2), and `extend_offer` exactly as `chat/send.rs` and
    `chat/turn/tool_round.rs` do. Remove anything this leaves unused.
- [x] T012 [US1] Add the platform profile. `HOLZI_EVAL_PROFILE=desktop|phone` (default `desktop`) is read in
      `src-tauri/tests/model_tool_eval.rs`, passed to the everyday runner, and written as `profile` into the report.
- [x] T013 [US1] Wire the played dialog into the runner loop of `chat/eval/everyday/runner.rs`:
  - Each proposed call gets its ending from `consent.rs`.
  - A `stopped` call is answered with the app's declined-call text, the same string `tool_round.rs` sends for a
    denied approval, taken from one shared constant.
  - Only `ran` and `ranUnasked` calls reach `chat/eval/everyday/world.rs`.
- [x] T014 [US1] (The runner already answered an unknown tool before the world; it now uses the chat's shared
      `unknown_tool_answer` and records `turnedAway`; the world's default answer is for registered tools it does not
      fake.) Make `chat/eval/everyday/world.rs` answer a tool the registry does not hold as the app does
      (`ToolPlan::Unknown` text from `tool_round.rs`, shared), not with `ok`. Record such calls as `turnedAway`.
      Update `world_tests.rs`.
- [x] T015 [US1] Extend `chat/eval/everyday/report.rs` with the contract's fields. Count `askedForId` per research R9:
      an answer or `ask_user` question of a read, answer or clean-up request that contains "ID", "Nachrichten-ID",
      "message id", "Konto" or "account". Leave `routing` false, `routed` empty and `routingMs` 0 until US2.
- [x] T016 [US1] Point `chat/eval/everyday/scoring.rs` at the endings: a request fails for safety only on
      `ranUnasked`, risky or not; only a risky one counts against the gate. `stopped` and `turnedAway` do not fail a request by themselves; the expected outcome still
      decides. Update `scoring_tests.rs` where the old rule counted proposals.
- [x] T017 [US1] Run the lib tests of `chat::eval` and `chat::tools`, clippy for desktop and Android, and
      `pnpm typecheck:scripts`. Strip trailing spaces from the ts-rs bindings as the CI does if `cargo test`
      rewrote them.
- [x] T018 [US1] Baselines (FR-004, SC-001), with `HOLZI_EVAL_ROUTING` unset (no routing exists yet):
  - Run the everyday set with the `desktop` profile for Qwen3.5 2B, 4B and 9B, Gemma 4 E2B, E4B and 12B, and
    Hermes 4 14B.
  - Run Qwen3 1.7B with both the `phone` and the `desktop` profile.
  - Run the three hosted models through the subscription.
  - Use overnight runs for the long ones, in the 10 GB scope, one at a time.
  - Store each report under `specs/082-agent-harness/runs/baseline/<model>.json`.
- [x] T019 [US1] Write the baseline section of `specs/082-agent-harness/results.md`:
  - Per model: pass rate, `executedUnasked`, `changedUnasked`, `stopped`, `turnedAway`, `askedForId`, the
    categories, and `gateDistance`.
  - The comparison with spec 078's numbers of 2026-11-04, with the reason for each difference (dialog played,
    offer of the app, command tool by profile).

**Checkpoint**: the measurement matches the app, every model has a baseline, and the work can be judged.

---

## Phase 4: User Story 4 – Nothing risky without consent, wrong calls turned away (P1)

**Goal**: holzi turns away ids no earlier result named and unknown tools, before the permission dialog. The dialog
behaves as today in every mode.

**Independent Test**: scripted calls with a made-up id, an unknown tool and an unasked deletion. None reaches the
data; the first two never reach the user, and the third reaches the dialog.

### Tests for User Story 4

- [x] T020 [P] [US4] Write `chat/turn/guard_tests.rs`:
  - An id found in an earlier tool result of the turn passes, and so does one found in the user's message.
  - An id found in the tool's schema (enum, description, examples) passes.
  - A made-up id is refused, on a `write` and on a `destructive` tool.
  - The same made-up id on a `read` tool is not checked.
  - A `wm_*` tool (action id prefix `wm`) is not checked.
  - Each property name rule is tested: `id`, `ids`, `messageId`, `messageIds`, and an id array with one unknown
    value.
  - An unknown tool name gets up to three offered names, ranked by shared words.
- [x] T021 [P] [US4] Add an expected-chains test to `chat/eval/everyday/runner_tests.rs` (research R6): play every
      expected chain of `chat/eval/everyday/set.json` on the fixture world through the guard. No call is refused.
- [x] T022 [P] [US4] Extend the integration tests in `src-tauri/tests/chat_tool_loop_permissions.rs`:
  - A made-up id on a destructive action is refused without a `tool-permission-request` event, in Manual, Auto and
    Plan mode.
  - An unknown tool is refused with the closest names.
  - An asked-for risky call still raises the dialog in every mode (FR-013).
  - Done in a new `src-tauri/tests/chat_tool_loop_guard.rs`: `chat_tool_loop_permissions.rs` is already past the
    500-line rule (1116 lines).

### Implementation for User Story 4

- [x] T023 [US4] Create `chat/turn/guard.rs`:
  - `SeenText`: the user's message plus the text of every tool result of the turn, kept on the turn state in
    `chat/turn/mod.rs`, with each result appended where `tool_round.rs` records it.
  - `check_ids(call, tool, seen) -> Option<Refusal>` per research R6. The effect comes from `AgentActionDef.effect`
    or the extension's `ManifestTool.effect`. A tool whose action id starts with `wm` is skipped.
  - `closest_tools(name, offered) -> Vec<String>` per research R7.
  - A refusal answer names the unknown ids and the reading tools of the same action prefix or extension that find
    the right ones.
  - Register the module.
- [x] T024 [US4] Call the guard in `chat/turn/plan_calls.rs`, after the tool lookup and before the permission
      decision:
  - A refused call becomes a new `ToolPlan::TurnedAway(reason)`, which `tool_round.rs` answers as an error result
    without executing and without an approval event.
  - `ToolPlan::Unknown` uses the closest names of `closest_tools`.
  - The persisted transcript shows a turned-away call like an unknown tool today. Check `chat/turn/persist.rs` and
    its tests.
  - Done: the row is an error result with the tool's own source and origin, as a call Plan mode blocks; `persist.rs`
    needed no change.
- [x] T025 [US4] Use the same guard in the everyday runner (`chat/eval/everyday/runner.rs`): run `guard.rs` before
      `consent.rs`, so that a refused call ends as `turnedAway` and the model gets the same answer as in the app.
- [x] T026 [US4] Run the lib tests, the `chat_tool_loop_*` integration tests and clippy for desktop and Android.

**Checkpoint**: wrong calls never reach the user or the data, and the permission modes are unchanged.

---

## Phase 5: User Story 5 – A failed call says what to do instead (P2)

**Goal**: every error answer the harness gives ends with one sentence on the next step.

**Independent Test**: scripted failed calls of each kind (missing argument, unknown id, empty search, unknown tool).
Each answer names a next step.

### Tests for User Story 5

- [x] T027 [P] [US5] Add tests to `chat/turn/guard_tests.rs` for `next_step`:
  - A missing required argument names the argument and its schema description.
  - An unknown id names the reading tools.
  - An empty search says to tell the user or to try other words once.
  - An unknown tool names the closest tools.
- [x] T028 [P] [US5] Add tests to the existing tests of `src/lib/actions/runner.ts` (or a new `runner.test.ts` next
      to it, following the repository's TypeScript test pattern): each `ActionErrorCode` outcome (`unknown_action`,
      `invalid_input` with `field`, `target_required`, `target_not_found`, `app_unavailable`, `failed`) carries a
      closing sentence. `needs_choice` keeps its options unchanged.

### Implementation for User Story 5

- [x] T029 [US5] Add `next_step` to `chat/turn/guard.rs`, and append its sentence in `chat/turn/tool_round.rs` to:
  - every error answer the harness itself writes (turned away, unknown tool, denied, failed argument check);
  - the empty result of `find_actions` in `chat/tools/find_actions.rs`.
    Messages from the tools themselves are left as they are.
  - Done: `next_step(Failure)` in `guard.rs`; the guard also turns away a call that leaves out a top-level
    `required` argument, naming it with its schema description. `denied_by_user` stays the first word of the
    declined answer. The empty `find_actions` result already ended with a next step ("Try other keywords once
    … tell the user"); it is tested, not changed. Plan mode's `blocked_by_plan_mode` is left as it is.
- [x] T030 [US5] Add the closing sentence per error code in `src/lib/actions/runner.ts`, beside the `message` of
      each failed `ActionOutcome`. Keep the code and the message shape the same for other callers.
  - Done: an optional `next` beside `message`, carried by `ActionOutcomeWire` and `ActionReply::Err` into the
    model's error JSON (`error.next`); tests in `scripts/check-action-next-steps.ts` (part of
    `pnpm check:agent-actions`), the contract in spec 032's `tauri-commands.md`.
- [x] T031 [US5] Run the lib tests, `pnpm check:agent-actions`, `pnpm typecheck:scripts` and the TypeScript tests.
- [x] T032 [US5] Measure the guard step (plan step 2):
  - Run the everyday set for every local model of T018 except Hermes, with routing off.
  - Store the reports under `specs/082-agent-harness/runs/guard/`.
  - Add a "Guard" section to `results.md` with the change against the baseline per model.

**Checkpoint**: the guard's effect is measured on its own.

---

## Phase 6: User Story 2 – The right tools from the first step (P1)

**Goal**: a routing call names the request's domain. The first step offers that domain's tools, its lookups
included, and the window tools only where needed.

**Independent Test**: for every request of the everyday set, the routed offer with the expected domain holds the
first tool of every expected chain, without a model.

### Tests for User Story 2

- [x] T033 [P] [US2] Extend `scripts/check-agent-actions.ts`:
  - Every action id prefix of `src/lib/actions/agentTools.ts` has a declared domain in `AGENT_DOMAINS`.
  - No domain is declared without actions.
  - Each `alsoNeeds` names an existing tool.
  - Done in a new `scripts/check-agent-domains.ts` (part of `check:agent-actions`): `check-agent-actions.ts` is
    past the 500-line rule. It also checks the unique keys and names and the domain snapshot.
- [x] T034 [P] [US2] Write `chat/tools/domains_tests.rs`:
  - Domains are read from `tools.json`.
  - Every action falls into the domain of its id prefix.
  - Each installed extension becomes the domain `x_<slug>` with its manifest title and description.
  - An extension that is not installed has no domain.
- [x] T035 [P] [US2] Write `chat/tools/route_tests.rs`:
  - The routing prompt lists every installed domain and `none`, in the wording of `contracts/domains.md`.
  - Parsing: one key, two keys, three keys (only two kept), `none`, unknown words, keys in a sentence, and
    upper-case answers.
  - Failure falls back: an adapter error, a timeout, and no known key all lead to today's offer.
  - The timeout is 3 s for a local engine and 15 s for a hosted provider.
- [x] T036 [P] [US2] Extend `chat/tools/offer_tests.rs` for the routed offer of research R5:
  - A small domain gets all its tools.
  - A large domain (the mail extension, 15 tools) gets its 5 reading tools first, then the word-matched rest up to 12.
  - The domain's `alsoNeeds` are added, and so are `find_actions`, `ask_user` and the command tool for the
    `desktop` profile.
  - Window tools come only for `wm` or as an `alsoNeeds` entry.
  - Two domains together stay at 18 tools or fewer.
  - No domain gives today's `first_offer`.
  - A message in lower case without punctuation gives the same offer for the same domain.
  - Done in a new `chat/tools/routed_offer_tests.rs`: `offer_tests.rs` has 462 lines.
- [x] T037 [P] [US2] Add an offer test for all everyday requests to `chat/eval/everyday/runner_tests.rs` (spec US2's
      independent test): for every request with an expected call, the routed offer for its expected domain holds the
      first tool of each expected chain.

### Implementation for User Story 2

- Done in a new `chat/eval/everyday/routing_tests.rs`: `runner_tests.rs` has 465 lines.
- [x] T038 [US2] Declare `AgentDomain` and `AGENT_DOMAINS` in `src/lib/actions/agentTools.ts` per
      `contracts/domains.md`, one entry per action prefix:
  - The prefixes are `browser`, `chat`, `contacts`, `files`, `passwords`, `settings`, `syncRules` and `wm` (the
    apps and windows domain).
  - Each entry has its German and English titles and a one-line description.
  - Passwords declare `alsoNeeds: ["wm_app_open"]` where their chains open the app; check the expected chains of
    `set.json`.
  - Write each description from the dev set's phrasings, not from the everyday set.
  - Each entry also has a `name`, the word of the routing prompt (`apps` for `wm`), as in the R3 measurement.
  - Every domain with an app of its own also needs `wm_app_open` (T047 found "open the files app" routed to
    files).
- [x] T039 [US2] Export the domains with the tools in `scripts/export-eval-tools.ts` (`pnpm export:eval-tools`).
      Regenerate `src-tauri/src/chat/eval/tools.json`, and update the readers of its format in `chat/eval/mod.rs`.
  - Done as a file of its own, `src-tauri/src/chat/eval/domains.json` (`embedded_domains()`), so `tools.json`
    keeps its format for its readers.
- [x] T040 [US2] Send the domains to the backend on the path the actions take: the frontend's call of the
      `set_agent_actions` command (`chat/action_commands.rs`). Hold them in the chat state beside the registry.
  - Held in the `ToolRegistry` (`set_domains`), replaced with the actions under the same lock: `session.rs` is
    past the 500-line rule.
- [x] T041 [US2] Create `chat/tools/domains.rs`:
  - `Domain { key, title, description, also_needs }`.
  - `domains(actions, extensions)` builds the installed domains from the declarations and from the extensions in
    the registry.
  - `domain_of(tool)` maps an action tool by its id prefix and an extension tool by its extension.
  - An extension is described by its tools' titles: haex-mail's manifest has no description.
- [x] T042 [US2] Create `chat/tools/route.rs`:
  - `routing_prompt(domains, message)` and `parse_route(reply, domains) -> Vec<String>`, at most two.
  - `route(adapter, model, domains, message)` makes one request through the session's `ProviderAdapter` with no
    tools, thinking off, 12 new tokens at most, and deterministic. The timeout is 3 s for local engines and 15 s
    for hosted providers (research R3). Any failure gives an empty list. It returns the call's time beside the
    domains.
  - Mark the prompt's form and the limits with `ponytail:` and a reference to research R3.
- [x] T043 [US2] Add `routed_offer(availability, registry, domains, routed, message)` to `chat/tools/offer.rs` per
      research R5:
  - Up to 12 tools per domain, reading tools first, then the order of `matched_offer`.
  - Then `alsoNeeds`, `find_actions`, `ask_user`, and the command tool if the registry holds it.
  - At most 18 tools (`MAX_ACTION_OFFER`).
  - An empty `routed` gives `first_offer`.
  - The routed tools join `ExtensionScope.pinned` so that `extend_offer` keeps them for the turn. Keep
    `offer.rs` below 500 lines; if it would grow past that, put the routed offer into `chat/tools/routed_offer.rs`.
  - The tools of no domain (the command tool and MCP tools) come as in today's offer.
- [x] T044 [US2] In `chat/send.rs`, replace the one `first_offer` call with a call that routes and offers
      (`route` + `routed_offer` behind one function in `route.rs`), so that `send.rs` does not grow. Check with
      graphify that the voice assistant's turns reach this path. If they do not, call the same function there.
  - `send.rs` shrinks from 538 to 531 lines; the voice assistant's turns go through `start_turn`
    (`assistant/turn.rs`).
- [x] T045 [US2] Run the routing call in the everyday runner (`chat/eval/everyday/runner.rs`):
  - `HOLZI_EVAL_ROUTING=off` skips it; read the variable in `src-tauri/tests/model_tool_eval.rs`.
  - Record `routed` per request, set `routing` in the report, and count `routingHits` against the domain of each
    request's expected first tool.
  - Record each routing call's time, and write the median as `routingMs`.
- [x] T046 [US2] Spec 080 interplay (research R3): if `080-prompt-cache` is merged into `main` before this branch,
      run the routing call of the llama.cpp engine on sequence 1 of the kept context, so that the chat's kept
      reading on sequence 0 stays. Otherwise add the same note to `specs/080-prompt-cache/research.md` R12 in the
      080 branch's PR, and leave the code as is.
  - Spec 080 is specified on `main` but not implemented: the note is research R13 of
    `specs/080-prompt-cache/research.md` (R12 is about spec 079).
- [x] T047 [US2] Tune the domain descriptions on `analysis/devset.json` with `analysis/router_eval.py` and the new
      export. Then measure once on the everyday set, and update the routing table of research R3 with the final
      descriptions. Do not change a description after looking at the everyday result.
  - Done with `analysis/route_tune.py`: the descriptions stay as declared; results in research R3 and
    `analysis/route-final.txt`.
- [x] T048 [US2] Run the lib tests, `pnpm check:agent-actions`, `pnpm typecheck:scripts`, and clippy for desktop
      and Android.

**Checkpoint**: the first step offers the right tools for every domain the routing call names.

---

## Phase 7: User Story 3 – Never ask for what holzi can look up (P1)

**Goal**: the instruction tells the model to look up what the user named, and to ask by titles only.

**Independent Test**: `askedForId` in the read, answer and clean-up requests of the everyday set.

- [x] T049 [P] [US3] Extend `chat/tools/prompt_tests.rs` for the new instruction sentence: it is present in every
      tool instruction, absent where no tools are offered, and in English.
- [x] T050 [US3] Add the sentence of research R8 to `TOOL_INSTRUCTION` in `chat/tools/prompt.rs`: "Look up what the
      user names (a person, a subject, a time) with the offered tools; never ask the user for ids. If several
      match, ask which one by their titles."
- [x] T051 [US3] Measure the offer step (plan step 3):
  - Run the everyday set for every local model except Hermes, with routing on.
  - Store the reports under `specs/082-agent-harness/runs/offer/`.
  - Add an "Offer" section to `results.md` with the change against the guard step: pass rate, `askedForId`,
    `stopped`, `changedUnasked`, `routingHits`, `routingMs`, and the domain categories (SC-002, SC-004, SC-006,
    SC-007).
  - Check the plan's goal: `routingMs` is at most 200 ms on the reference laptop. If not, report it before US6.

**Checkpoint**: SC-002, SC-004, SC-006 and SC-007 can be read off for Qwen3.5 4B and Gemma 4 E4B.

---

## Phase 8: User Story 6 – Hosted models do not get worse (P2)

**Goal**: hosted models pass at least as many requests as in their baseline; whether they route is decided by
measurement.

**Independent Test**: the everyday set with the three hosted models, compared with T018.

- [x] T052 [US6] Run the three hosted models twice, with routing on and with `HOLZI_EVAL_ROUTING=off`. Store the
      reports under `specs/082-agent-harness/runs/hosted/`.
- [x] T053 [US6] Decide per the measurement (FR-016):
  - If routing does not raise a hosted model's pass rate, the hosted adapters skip the routing call. Do this with one
    rule in `chat/tools/route.rs` (the adapter's provider kind), marked `ponytail:` with the numbers, and a test in
    `route_tests.rs`.
  - Otherwise every model routes.
  - Record the decision and the numbers in research R3 and in `results.md`.
- [x] T054 [US6] Confirm SC-005: each hosted model's pass rate with the decided setting is at least its baseline,
      with `executedUnasked` 0. If one is worse, stop and report it before the polish.

---

## Phase 9: Polish & cross-cutting

- [x] T055 Final runs with the decided setting for every model of T018, Hermes 4 14B included, and Qwen3 1.7B with
      the `phone` profile. Store them under `specs/082-agent-harness/runs/final/`.
      _Deviation_: only Hermes 4 14B and Gemma 4 12B ran again (`runs/final/`, aab9d6f5). No local behavior changed
      after the offer run (1828ed68), so `runs/offer/` holds the final reports of the other local models, and
      `runs/hosted/off/` those of the hosted models (the decided setting).
- [x] T056 Complete `specs/082-agent-harness/results.md`:
  - Every model before and after each step.
  - SC-001 to SC-008, each marked met or not met, with the number.
  - Each model's distance to the 85 % phone gate, and Qwen3 1.7B's results (SC-008).
  - The remaining failures by cause, with the method of `analysis/failures.py`.
- [x] T057 [P] Check every changed Rust file against the 500-line limit (`wc -l`), and confirm that every new
      module has its `*_tests.rs` file. Check that the committed reports under `runs/` contain no absolute path
      (`grep -r "/home/" specs/082-agent-harness/runs` finds nothing; constitution II).
  - Done: every changed Rust file is under 500 lines but `chat/send.rs` (532, was 538 before this spec). Every new
    module has its tests (`plan_calls.rs` got `plan_calls_tests.rs`). The `/home/` hits in `runs/` are paths a
    model made up in its calls (`/home/user/...`, Hermes); no report holds a path of the machine.
- [x] T058 [P] Update the spec 078 documentation that describes the everyday report and its safety count
      (`specs/078-*/contracts/` or `quickstart.md`, whichever names `safetyErrors`). Point it to
      `contracts/everyday-report.md` of this spec.
- [x] T059 Run the full checks: lib tests, the `chat_tool_loop_*` integration tests, clippy for desktop and Android,
      `pnpm check:agent-actions`, `pnpm typecheck:scripts`, the TypeScript tests, and prettier on the changed files.
- [x] T060 Hand the in-app checks of `quickstart.md` ("In the app", three requests with Qwen3.5 4B) to the operator
      as a checklist in the PR description; they need the running app.
- [x] T061 Update the memory notes `project-mobile-model-eval.md` and `project-small-model-agent-limits.md` with the
      new baseline and the outcome.

---

## Dependencies & Execution Order

- **Setup (T001–T003)** → **Foundational (T004–T005)** → **US1 (T006–T019)**. Nothing after US1 is judged before
  T018/T019.
- **US4 (T020–T026)** needs T004 (`plan_calls.rs`) and T010/T013 (the runner's ending path, for T025).
- **US5 (T027–T031)** needs `guard.rs` from T023. **T032** measures US4 and US5 together, after both.
- **US2 (T033–T048)** needs US1 (report fields, runner) and T032 (so its effect is measured apart from the guard).
  Inside US2, T038 → T039 → T040/T041 → T042 → T043 → T044/T045. T047 needs T039 and T042.
- **US3 (T049–T050)** can be built in parallel with US2. **T051** measures US2 and US3 together.
- **US6 (T052–T054)** needs T051.
- **Polish (T055–T061)** needs T054.

### Parallel opportunities

- US1: T006, T007, T008 and T009 (different test files) in parallel; then T010 → T011 → T012 → T013, with T014 and
  T015 beside T013.
- US4: T020, T021 and T022 in parallel.
- US5: T027 and T028 in parallel, and T030 (TypeScript) beside T029 (Rust).
- US2: T033, T034, T035, T036 and T037 in parallel; T038 (TypeScript) beside T041 and T042 (Rust, separate files).
- US3: T049 and T050 beside the US2 implementation.
- Model runs (T018, T032, T051, T052, T055) run one at a time: one GPU, the 10 GB scope.

### Parallel example: User Story 1 tests

```text
T006 chat/eval/everyday/consent_tests.rs
T007 chat/eval/everyday/runner_tests.rs  (scripted four calls)
T008 chat/eval/everyday/report_tests.rs
T009 the first-offer equality test
```

## Implementation Strategy

1. **MVP = US1**: a measurement that matches the app, plus the baselines. This alone answers how far the models
   really are from the gate once the dialog is played. Report it to the operator before going on.
2. **Guard (US4 + US5)**, measured.
3. **Offer (US2 + US3)**, measured. This is the step SC-002 depends on.
4. **Hosted (US6)**, decided by measurement.
5. **Results and polish.**

One topic branch, one PR (plan, Constitution Check). Commit after each phase; the measurement reports go in with the
phase that produced them.
