---
description: 'Task list for spec 032-model-operates-holzi'
---

# Tasks: Modell bedient holzi über die Aktionen

**Input**: Design documents from `/specs/032-model-operates-holzi/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md), [contracts/eval-format.md](./contracts/eval-format.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic: the new `scripts/check-agent-actions.ts` (`pnpm check:agent-actions`, in CI), Rust unit tests in sibling `*_tests.rs` files, Rust integration tests in `src-tauri/tests/`, and the ignored model run `src-tauri/tests/model_tool_eval.rs`. Write each test task before its implementation and see it fail first.

**Organization**: Grouped by user story. The branch `032-model-operates-holzi` starts from `main` (specs 003, 012, 020, 023 merged). Modules under `src/lib/**` import siblings relatively with a `.ts` suffix so the Node check scripts can load them. Every file stays ≤ 500 lines (`src-tauri/src/chat/commands.rs` has a documented exception and only receives a few lines). Commits follow Conventional Commits and carry no agent attribution. Rust commands run as `nix develop --command scripts/with-nix-host-bridge.sh cargo …`; frontend commands as `nix develop --command bash -c '…'`. Code behind the `llm-cpu` feature is wrapped in `#[cfg(feature = "llm-cpu")]`; CI checks default and `--no-default-features`.

**Shipping note**: Everything lands in one PR. The app is usable after US1 + US2 (P1). US3–US5 follow in the same PR; the in-app self-test (T045, label US4) needs the evaluation runner of US5 and is therefore built in Phase 7.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup

- [x] T001 Prepare `.worktrees/032-model-operates-holzi`: real `pnpm install` (no symlinked `node_modules`), reflink the Rust build cache from the primary checkout if `src-tauri/Cargo.lock` matches (`cp -a --reflink=always ../../src-tauri/target src-tauri/target`); run `graphify query` for "action bridge", "select tools", "tool use capability" and "agent tools" and prefer any existing candidate over a new name; record baseline counts of `pnpm check:wm-navigation`, `check:chat-state`, `check:templates` and `cargo test` in this task's note
  - Done 2026-10-01: pnpm install real, Rust target reflinked (deps still rebuilt, so no cache hit); graphify found no existing `ActionBridge`, `core_offer`, `ToolUse` or `agentTools` candidates (the delegate `approval_bridge` is only the pattern). Frontend baseline on `main`: `check:wm-navigation` 77, `check:chat-state` 50, `check:wm-state` 66, `check:templates` 74 templates; no separate `cargo test` baseline (run after the change instead).
- [x] T002 [P] Write `docs/adr/0006-actions-as-builtin-agent-tools.md` (status accepted): one action definition, two entrances (in-process chat, MCP in spec 021); three-step risk mapping (`read`→`Safe`, `write`→`Change`, `destructive`→`Risky`); `builtinAgentCallable`; what spec 021 takes over; note that 0005 stays reserved for spec 021. Content per [research.md](./research.md) R1, R4, R5, R15

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Risk stage, catalog fields, name mapping, capability type and the check script that every story builds on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T003 Add the stage `Change` to `RiskClass` in `src-tauri/src/chat/tools/mod.rs` (`Safe | Change | Risky`, keep derives) and make `risk_class_str` in `src-tauri/src/chat/events.rs` return `"safe" | "change" | "risky"`. `CliTool` and MCP tools stay `Risky`. Only these two files here; the remaining non-exhaustive `match` sites are fixed in T005 (`cargo check` stays red until T005 is done)
- [x] T004 Update `decide()` in `src-tauri/src/chat/tools/permission.rs` to the matrix of [research.md](./research.md) R4 (Manual: Ask/Ask/Ask; Auto: Allow/Allow/Ask; Plan: Allow/Deny/Deny for Safe/Change/Risky) and extend `src-tauri/src/chat/tools/permission_tests.rs` to all nine cells plus the existing parse/default tests
- [x] T005 Adapt (after T003) the delegate approval bridge and fixtures to the new stage: add the `Change` arm in `src-tauri/src/adapters/cli_delegate/approval_bridge.rs` (same result as `Safe`, delegates classify themselves), fix `src-tauri/src/chat/session_tests.rs`, `src-tauri/tests/common/tool_loop_fixture.rs` and `src-tauri/tests/chat_tool_loop_{core,permissions,retry}.rs` so `cargo test` compiles and stays green
  - Done 2026-10-01: only `approval_bridge.rs` needed a change (it now uses `risk_class_str`); the existing fixtures and tests compile unchanged.
- [x] T006 [P] Teach the frontend the stage: `RiskClass = 'safe' | 'change' | 'risky'` in `src/composables/useChat.ts`; `src/components/chat/PermissionPrompt.vue` shows a label for `change`; add `chat.permission.change` ("Änderung" / "Change") to `src/i18n/locales/de.json` and `en.json` (keep both in lockstep)
- [x] T007 Extend `ActionDefinition` in `src/lib/actions/types.ts` with the optional field `builtinAgentCallable?: boolean` (default `true`, see [data-model.md](./data-model.md) §1) and make `createActionRunner` in `src/lib/actions/runner.ts` reject `{ kind: 'builtinAgent' }` for `builtinAgentCallable === false` with the existing code `forbidden_for_agents` before input validation; external agents are unaffected
- [x] T008 [P] Set the new field: `builtinAgentCallable: false` on `chat.message.send`, `chat.message.retry`, `chat.reply.cancel` in `src/lib/actions/chatActions.ts`
- [x] T009 Create `src/lib/actions/agentTools.ts` (pure TS, relative `.ts` imports): `toToolName(id)` (`.`→`_`, result must match `^[A-Za-z0-9_-]{1,64}$`), `fromToolName(name, catalog)` (reverse lookup), `toAgentActionDef(def, locale)` and `listAgentActions(catalog)` per [data-model.md](./data-model.md) §2. `listAgentActions` returns only `agentCallable && builtinAgentCallable !== false`; export the constant `CORE_AGENT_TOOLS` (`wm.state.get`, `wm.apps.list`, `wm.app.open`, `wm.tab.new`, `wm.tab.activate`, `wm.tab.close`, `settings.get`, `settings.appearance.setColorScheme`, `settings.models.list`) and set `core: true` on those definitions in `toAgentActionDef`; titles `{ de, en }` come from an injected `titleOf(key, locale)` so the module stays i18n-free
  - Done 2026-10-01: `toAgentActionDef(def, titleOf)` takes the title resolver for both locales instead of a single `locale`; also exports `toOutcomeWire` and `isBuiltinAgentAction`.
- [x] T010 [P] Add `tool_use: Option<ToolUse>` to `ModelCapabilities` in `src-tauri/src/model_capabilities.rs` with `ToolUse { support: Supported | Unsupported, basis: Provider | Curated | Template | SelfTest }` (camelCase, snake_case enum values, `#[serde(default)]`; `None` means unknown; a record without `toolUse` reads as `None`; `is_undetermined()` stays "equals `Default`"); add the TS type `ModelCapabilities.toolUse: { support, basis } | null` in `src/composables/useModels.ts`; switch struct-literal sites to `..Default::default()` (`adapters/anthropic_capabilities.rs`, `model_capabilities_tests.rs`, `adapters/request_tests.rs`, `adapters/attachments_tests.rs`, `chat/commands_tests.rs`, `storage/models_tests.rs`, `tests/model_capabilities_storage.rs`)
- [x] T011 Create `scripts/check-agent-actions.ts` (Vorbild `scripts/check-wm-actions.ts`; `node:test`, `.ts` imports, `import type`) with the catalog invariants: every `toToolName` result matches `^[A-Za-z0-9_-]{1,64}$` and is unique across `ALL_ACTIONS`; `listAgentActions` contains no `guardrails` action (FR-003); `builtinAgentCallable: false` is refused for `builtinAgent` and accepted for `externalAgent`; the three `builtinAgentCallable: false` fields of T008 are set; every id in `CORE_AGENT_TOOLS` exists, is builtin-callable and at most 9 of them exist (9 plus `find_actions` is the cap of 10, FR-011). Add `"check:agent-actions": "node --test scripts/check-agent-actions.ts"` to `package.json` and a step "Check agent actions" running `corepack pnpm check:agent-actions` after "Check window manager navigation" in `.github/workflows/ci.yml`

**Checkpoint**: `cargo test` (both feature legs) and `pnpm check:agent-actions`, `check:wm-navigation`, `typecheck` green; nothing is offered to a model yet.

---

## Phase 3: User Story 1 - Holzi per Chat bedienen: lesen und navigieren (P1) 🎯 MVP

**Goal**: A model (API key or local) calls actions like `wm_state_get` and `wm_app_open` through the existing tool loop; the action runs in the frontend with caller `builtinAgent`, the result returns to the model, and every call is a tool row in the transcript.

**Independent Test**: [quickstart.md](./quickstart.md) §3 (stub frontend) and §4 steps 1–2 with Claude per API key and with local Qwen3-4B: "Öffne die Sync-Einstellungen" and "Welche Tabs sind offen?" produce the right UI state and a correct answer; transcript shows the action rows.

### Tests for User Story 1

- [x] T012 [US1] Extend `src-tauri/tests/common/tool_loop_fixture.rs` with an `ActionBridge` helper: build a bridge with a channel-based emitter and `respond_action(bridge, request_id, outcome)` (resolves the pending map directly, as `respond` does for approvals) plus `register_action_tool(chat_state, def)`; keep the file ≤ 500 lines (split into `tests/common/action_fixture.rs` if needed)
  - Done 2026-10-01: the helpers live in the new `src-tauri/tests/common/action_fixture.rs` (keeps `tool_loop_fixture.rs` unchanged).
- [x] T013 [P] [US1] Write `src-tauri/src/chat/tools/action_tool_tests.rs` (wired via `#[path]`): `risk_class()` mapping `read`→`Safe`, `write`→`Change`, `destructive`→`Risky`; `name()`, `source() == "action"`, `description()` and `input_schema()` pass through unchanged
- [x] T014 [P] [US1] Write `src-tauri/tests/action_bridge.rs` (fails first): a scripted `StubAdapter` calls `wm_state_get`, the test answers `action-call-request` via the fixture → rows `tool_call`/`tool_result` with `tool_source = "action"`; result `ok`; error replies pass `code`/`field`/`message` and never a raw `error`; `code == "failed"` is replaced by `"The action failed."` (FR-006); no answer for 60 s (use a short injected timeout) → `action_timeout`; `reset_for_close` while pending → `tool_call_cancelled`; late and duplicate replies are a no-op; two calls in one round execute one after the other (run lock); emitter not set → `action_unavailable`

### Implementation for User Story 1

- [x] T015 [US1] Create `src-tauri/src/chat/tools/action_bridge.rs` per [data-model.md](./data-model.md) §4: `ActionBridge { emitter: OnceLock<EventEmitter>, pending: Mutex<HashMap<Uuid, oneshot::Sender<ActionReply>>>, run_lock: tokio::sync::Mutex<()> }`, `ActionReply = { ok: true, result } | { ok: false, code, field, message }`, `call(action_id, input, thread_id, cancel, timeout)` that emits `action-call-request`, waits in `select!` against the cancel token and the timeout (default 60 s, injectable), removes the entry on every exit, and `resolve(request_id, reply)` for the command; no `unwrap`/`expect` on inputs
- [x] T016 [US1] Create `src-tauri/src/chat/tools/action_tool.rs`: `ActionTool` implementing `Tool` per [data-model.md](./data-model.md) §3 (holds the whole `AgentActionDef` incl. `scope` and `titles` for the selection, `source() == "action"`, risk mapping from the effect, `execute` calls `ActionBridge::call` and turns `ActionReply` into `ToolResult`, replacing the message of `code == "failed"` with the fixed text); declare the modules in `src-tauri/src/chat/tools/mod.rs`
- [x] T017 [US1] Create `src-tauri/src/chat/action_commands.rs` with the commands `set_agent_actions` (validate: unique `toolName`, `^[A-Za-z0-9_-]{1,64}$`, schema inside the JSON-schema subset → `InvalidInput`; replace all registry tools with source `action`, keep `run_command` and MCP tools; returns `{ registered }`) and `respond_action_call` (unknown or late `requestId` is `Ok`); register both in `gate.wrap(generate_handler![...])` in `src-tauri/src/lib.rs` and keep them vault-scoped in the gate (not in `APP_SCOPED_COMMANDS`). Contract: [contracts/tauri-commands.md](./contracts/tauri-commands.md)
- [x] T018 [US1] Wire the bridge into the app state: add `action_bridge: ActionBridge` to `ChatState` in `src-tauri/src/chat/session.rs`; in `reset_for_close` drop pending senders and remove tools with source `action` but keep the bridge and its emitter; set the emitter from the `AppHandle` in `.setup(|app| …)` in `src-tauri/src/lib.rs` (same closure type as the delegate `EventEmitter`)
- [x] T019 [P] [US1] Create `src/plugins/agentActions.client.ts`: a global listener for `action-call-request` that calls `wm.runAction(actionId, input, { kind: 'builtinAgent' })` and answers with `respond_action_call` containing only `code`, `field`, `message` (or `result`), never the raw `error`; after the vault session starts (same hook the window-manager restore uses) and on locale change it builds `listAgentActions(ALL_ACTIONS)` with German and English titles from the i18n messages and invokes `set_agent_actions`
  - Done 2026-10-01 with a change: a composable `src/composables/useAgentActions.ts`, started from `pages/workspace/[instance].vue` once the vault is open, instead of a plugin (a plugin runs before the vault unlocks and both commands need an open vault). `threadId` dropped from `action-call-request` (`Tool::execute` has no thread).
- [x] T020 [P] [US1] Frontend types and transcript: extend `ToolCallEvent.toolSource` to `'mcp' | 'cli' | 'action'` in `src/composables/useChat.ts`; in `src/components/chat/MessageList.vue` show the localized action title (`actions.<id>` via `fromToolName`) instead of the raw tool name for source `action`, falling back to the name
- [x] T021 [US1] Share the test harness: move `sample(schema)` and `catalogRunner(calls)` from `scripts/check-wm-actions.ts` into `scripts/lib/actions-harness.ts` (exported, `.ts` imports) and import them in both scripts; add to `scripts/check-agent-actions.ts`: every agent-callable `read` action runs successfully for a `builtinAgent` caller with a sample input (targets named explicitly), and `wm.state.get` exposes the tab, window and workspace ids an agent needs as targets (FR-004); if its result lacks any of them, extend its result schema and handler in the catalog (`src/lib/actions/wmActions.ts` and the handler in `src/stores/wmLayoutHandlers.ts`) within this task
  - Done 2026-10-01: `wm.state.get` already returns the three ids; its description now names them (its result schema is an open object), and a check asserts that.
- [ ] T022 [US1] Checkpoint: run `cargo test` (both legs), `pnpm check:agent-actions`, `pnpm check:wm-navigation`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`; then [quickstart.md](./quickstart.md) §4 steps 1–2 by hand with Claude (API key) and with the local Qwen3-4B

**Checkpoint**: A model can read and navigate holzi. Mode "Manual" asks for every call through the existing dialog.

---

## Phase 4: User Story 2 - Änderungen nur mit Freigabe nach Modus, Leitplanken bleiben gesperrt (P1)

**Goal**: Changes follow the mode and the action's effect; guardrail actions are unreachable; the approval dialog speaks plain language.

**Independent Test**: [quickstart.md](./quickstart.md) §2 and §4 steps 3–5: in each mode one read, one change and one destructive action; then every guardrail action requested by a model is refused.

### Tests for User Story 2

- [x] T023 [P] [US2] Extend `src-tauri/tests/chat_tool_loop_permissions.rs` with `ActionTool` stubs: all nine mode × stage cells; Plan returns `blocked_by_plan_mode` for `Change` and `Risky` without emitting a request; a denied request reaches the model as the tool result `denied_by_user` (scenario 4; what the model does next is not testable with a scripted stub); a model requesting a guardrail tool name gets `unknown tool` because it is not in the registry
  - Done 2026-10-01: `src-tauri/tests/action_permissions.rs` (new binary instead of growing the 1000-line permissions file): nine cells, denied request, guardrail name unknown in every mode.
- [x] T024 [P] [US2] Extend `scripts/check-agent-actions.ts`: every `guardrails` action is refused for `builtinAgent` before its handler runs (reuse `catalogRunner`); secret scan over the input and result schemas of all builtin-callable actions — no property name matching `/secret|private|password|passphrase|token|apiKey|credential/i` (short, justified allow-list in the script); `settings.devices.identity` result schema exposes only public-key fields; `chat.messages.list` returns an agent-safe projection without `content`, verified with a secret-bearing message fixture (FR-010, SC-003, SC-004)
  - Done 2026-10-01: guardrail refusal for builtin and external callers, secret scan (one allow-list entry: `tokenizerRepo`), `settings.devices.identity` now has a result schema with `npub` and `hex` only, `chat.messages.list` returns `agentSafeMessages` (id, role, createdAt, status) for every non-user caller.

### Implementation for User Story 2

- [x] T025 [US2] Add `describeAction(def, input, resolveTarget)` to `src/lib/actions/agentTools.ts`: returns `{ titleKey, target?: { kind, label }, inputs: Array<[field, value]> }` for the approval dialog (pure; `resolveTarget` is injected so the module stays store-free); add tests to `scripts/check-agent-actions.ts` (known action, missing target, unknown tool name falls back)
  - Done 2026-10-01: `describeAction` and `describeToolCall` (the latter falls back to `undefined` for any name that is no offered action, e.g. `find_actions`).
- [x] T026 [US2] Make `src/components/chat/PermissionPrompt.vue` plain-language for source `action` (FR-008): localized action title, target label from the window-manager store (tab/window/workspace title), inputs as a "Feld: Wert" list, stage label for `change`; other tools keep today's layout. Add needed keys to `src/i18n/locales/de.json` and `en.json` in lockstep; `pnpm check:templates` stays green
  - Done 2026-10-01: the approval event carries `toolSource` so the dialog knows an action; target labels come from the window-manager store as the tab bar names them, workspaces as "Arbeitsbereich N".
- [ ] T027 [US2] Checkpoint: [quickstart.md](./quickstart.md) §2 and §4 steps 3–5 by hand (Manual, Auto, Plan; guardrail request); confirm Auto runs a color-scheme change without asking and asks before closing a tab

**Checkpoint**: P1 complete — the model reads, navigates and changes holzi under the existing modes.

---

## Phase 5: User Story 3 - Kleine Modelle bekommen nur die passenden Werkzeuge (P2)

**Goal**: Every model, local or cloud, gets the same small fixed core offer plus a search tool; the model finds the rest itself, and what it finds is offered from the next step of the same answer.

**Independent Test**: With the full catalog, in no step more than 15 action tools are offered (core ≤ 10 plus ≤ 5 unique non-core results); retained `run_command` and MCP tools are outside this action-only limit; the core offer is identical for every model and every answer; after `find_actions` the found tools are part of the next request and callable.

### Tests for User Story 3

- [x] T028 [P] [US3] Write `src-tauri/src/chat/tools/offer_tests.rs` (wired via `#[path]`): `core_offer` returns exactly the `core: true` definitions plus `find_actions` (≤ 10) and is identical for any input, provider and model; `search_actions` ranks by word hits over camelCase-split id segments, description and both titles, breaks ties by stable id order, returns ≤ 5, never returns a non-builtin-callable action, and gives an empty result with the hint `"no matching action"` for no hit; `extend_offer` replaces earlier found tools, filters a search result overlapping a core action such as `wm.tab.activate`, deduplicates by `toolName`, keeps at most 15 action tools, and leaves non-action tools untouched
  - Done 2026-10-01: tests live in `offer_tests.rs` (search ranking, pagination, `extend_offer`, `found_tools`, `core_offer`, `find_actions` limit) and `tests/action_bridge.rs`.

### Implementation for User Story 3

- [x] T029 [US3] Create `src-tauri/src/chat/tools/offer.rs` per [research.md](./research.md) R6: `core_offer(defs) -> Vec<ToolSpec>`, paginated `search_actions(defs, query?, cursor, max) -> (Vec<AgentActionDef>, next_cursor)` and `extend_offer(request_tools, found)`, all pure and working over the `AgentActionDef`s held by the `ActionTool`s (they carry `core` and `titles`); declare the module in `src-tauri/src/chat/tools/mod.rs`
  - Done 2026-10-01 (PR #184): `core_offer` takes the registry, `extend_offer` takes the request tools, the found tools and the action definitions; `found_tools` reads the `find_actions` result.
- [x] T030 [P] [US3] Create `src-tauri/src/chat/tools/find_actions.rs`: the search tool `find_actions` (`Safe`, source `action`, optional `query`/`cursor`/`limit`, result with `actions` and `nextCursor`, at most 5 entries per page, exhaustive pagination without `query`, empty result plus the hint `"no matching action"`) per [contracts/tauri-commands.md](./contracts/tauri-commands.md); `set_agent_actions` (T017) registers it when at least one action exists; unit tests in `action_tool_tests.rs`
  - Done 2026-10-01 (PR #184): `set_agent_actions` registers `find_actions` and rejects it as a pushed tool name.
- [x] T031 [US3] Use the core offer in `send_message` in `src-tauri/src/chat/commands.rs`: replace `tool_specs(&registry)` by a helper that keeps non-action tools (`run_command`, MCP) and uses `core_offer` for the action tools; no per-model cap, no dependence on the user's text; keep the change ≤ 15 lines in `commands.rs` (helper lives in `offer.rs`). Extend `src-tauri/tests/action_bridge.rs`: with a 60-action registry the first request offers the same core tools regardless of the user text and provider kind (SC-005)
  - Done 2026-10-01: `send_message` uses `core_offer`; `run_command` and MCP tools stay offered. A test with 52 registered actions shows the first request carries only the core, `find_actions` and `run_command`.
- [x] T031a [US3] Offer what `find_actions` finds: call `extend_offer` from the turn loop where a finished round is appended to the request (`append_round_to_request` in `src-tauri/src/chat/turn/tool_round.rs`, one call, keep the file ≤ 500 lines) so the tools returned by `find_actions` are part of `request.tools` for the next step; a later search replaces earlier matches. The path is the same for every model (research R6). Test in `src-tauri/tests/action_bridge.rs` with a `StubAdapter` that records each `ChatRequest`: the second step's `tools` contains the found tool and calling it runs, and a third step after a second search no longer contains the first matches; add a case in `src-tauri/src/adapters/request_tests.rs` (Anthropic) and in `src-tauri/src/llm/local/stream_tests.rs` (local) showing that a tool in `request.tools` is forwarded in that adapter's format (FR-012, FR-013)
  - Done 2026-10-01: the search of a round is picked by tool name (before, a later JSON result of another tool of the same round hid it). `StubAdapter::recording` logs each request; tests cover step two offering the found tool and calling it, step four dropping the first matches, and a search next to another tool. The Anthropic and the local format each have a test that a tool in `request.tools` is forwarded.

**Checkpoint**: [quickstart.md](./quickstart.md) §1 green; with local Qwen3-4B the same sentences as in US1 still work with the reduced offer.

---

## Phase 6: User Story 4 - Modelle ohne Werkzeugnutzung werden nicht damit überfordert (P2)

**Goal**: The capability "tool use" decides whether tools are offered; unknown models get tools and a hint; delegates get no tools and a notice. (Scenario 7, the background self-test, is built in Phase 7 because it needs the evaluation runner.)

**Independent Test**: [quickstart.md](./quickstart.md) §4 steps 6 and 8: a model with `Unsupported` chats normally without tools and with one notice; `Supported` gets tools; a delegate gets the delegate notice.

### Tests for User Story 4

- [x] T032 [P] [US4] Tests first: `src-tauri/src/model_capabilities_tests.rs` (serde round trip of `toolUse`, a record without the field reads as `None`, `is_undetermined()` unchanged); `src-tauri/tests/model_capabilities_storage.rs` (`set_tool_use` replaces only that field, keeps reasoning and attachments; precedence Provider/Curated over Template over SelfTest and never downward; re-importing the same model resets the field); `src-tauri/src/llm/local/probe_tests.rs` (probe decision logic with an injected renderer: identical output with and without the dummy tool → `Unsupported`/`Template`, tool-aware output → inconclusive, "does not handle tool usage" error → `Unsupported`); an `#[ignore]` probe test with `HOLZI_TEST_GGUF` in `src-tauri/tests/local_inference.rs`
  - Done 2026-10-01: the probe decision tests are in `llm/local/probe_tests.rs`; the `#[ignore]` probe test in `tests/local_inference.rs` ran against the local Qwen3-4B and reads its template as taking tools.
  - Done 2026-10-01: serde, precedence and storage tests (`model_capabilities_tests.rs`, `model_capabilities_storage.rs`, `availability_tests.rs`).

### Implementation for User Story 4

- [x] T033 [P] [US4] Anthropic reports support: `map_capabilities` in `src-tauri/src/adapters/anthropic_capabilities.rs` sets `tool_use = Some(Supported/Provider)`; extend the existing capability tests
  - Done 2026-10-01: every mapped Anthropic record carries `Supported/Provider`, also when the wire has no capabilities object; four tests that expected such a record to be undetermined now expect only reasoning and attachments to stay unknown.
- [x] T034 [US4] Storage and catalog: add `set_tool_use(tx, id, ToolUse)` to `src-tauri/src/storage/models.rs` (read-modify-write of `capabilities_json`, respects the precedence from T032); add the optional `tool_use` field to `CatalogEntry` in `src-tauri/src/catalog/mod.rs` and `src-tauri/src/catalog/model_catalog.json` (initially absent) with `catalog_tests.rs` coverage; `ModelCapabilities::local()` takes it from the catalog entry with basis `Curated`
  - Done 2026-10-01: `set_tool_use` (read-modify-write, rank from `ToolUse::may_replace`), `CatalogEntry.tool_use` (optional, empty), `ModelCapabilities::local` takes it as `Curated`.
- [x] T035 [US4] Create `src-tauri/src/llm/local/probe.rs` (`#[cfg(feature = "llm-cpu")]`): render a fixed test message with and without a dummy tool via `Model::tokenize` + `detokenize` (research R8); in `src-tauri/src/chat/model_loading.rs` run it once after loading a local model whose `tool_use` is `None`, write `Unsupported/Template` through `set_tool_use` when the template ignores tools, and emit `model-tool-use-updated { modelId }`; no mistralrs symbols outside the feature gate
  - Done 2026-10-01: `ProviderAdapter::probe_tool_template` (default `Inconclusive`, `LocalAdapter` asks the model); `llm/local/probe.rs` compares the rendering with and without a dummy tool. The `either` crate became a direct, feature-gated dependency because `Model::tokenize` takes an `Either`. The result is written by the check of T045, not at load time.
- [x] T036 [US4] Availability in `send_message` (`src-tauri/src/chat/commands.rs`, small; logic in a new `src-tauri/src/chat/tools/availability.rs` with its own `_tests.rs`): derive `ToolAvailability` from `(provider_kind, capabilities.tool_use)` per [data-model.md](./data-model.md) §6, pass no tools for `Delegate` and `Unsupported`, emit `chat-tool-availability { threadId, state }` once per turn (constants in `src-tauri/src/chat/events.rs`), leave `adapters/cli_delegate/` untouched
  - Done 2026-10-01: `chat/tools/availability.rs` (`ToolAvailability::of`, `offers_tools`, `ToolAvailabilityEvent`); `send_message` hands no tools to `Unsupported` and `Delegate` and emits `chat-tool-availability` once per turn; the constant sits in `events.rs`.
- [x] T037 [US4] Frontend notice and refresh: handle `chat-tool-availability` in `src/composables/useChatTranscript.ts` and show a dismissible banner in `src/components/chat/StatusBanners.vue` for `offeredUnverified`, `unsupported` and `delegate`, once per thread and state per app session (in-memory set, `ponytail:` comment about the restart); handle `model-tool-use-updated` by reloading the model lists in `src/composables/useModels.ts`; add `chat.toolNotice.unverified|unsupported|delegate` to `de.json` and `en.json` (the `unsupported` text also says which kind of model can operate holzi); add `scripts/check-chat-tool-notice.ts` (harness test: one notice per thread and state, none for `offered`) and append it to the `check:chat-state` script in `package.json`
  - Done 2026-10-01: `chat-tool-availability` handling (`useToolNotice`, `lib/chat/toolNotice.ts`), the banner, the keys, `model-tool-use-updated` model-list refresh and `check-chat-tool-notice.ts` in `check:chat-state`. The producer is supplied by T035/T045; the listener is ready before those probes land.
- [ ] T038 [US4] Checkpoint: [quickstart.md](./quickstart.md) §4 steps 6 and 8 by hand (model without tool template, Claude Code or Codex delegate); `pnpm check:chat-state` green

**Checkpoint**: Unsupported models and delegates never get tools; unknown models get tools with a hint.

---

## Phase 7: User Story 5 - Zuverlässigkeit je Modell messen (P3), incl. in-app self-test for US4

**Goal**: A versioned example set measures any model's tool calling without a vault; a small subset runs as a background self-test on first use.

**Independent Test**: [quickstart.md](./quickstart.md) §5: two runs against one model give comparable reports; a new local model with a tool-capable template moves from unknown to supported/unsupported within two minutes while the chat stays usable (§4 step 7).

### Tests for User Story 5

- [x] T039 [P] [US5] Write `src-tauri/src/chat/eval/scoring_tests.rs` (wired via `#[path]`): the six results `pass`, `wrong_tool`, `bad_args`, `missed`, `spurious`, `not_found` per [contracts/eval-format.md](./contracts/eval-format.md); `args` subset matching (extra schema-valid fields allowed); exact unordered matching of the complete expected and observed call batch, including missing, duplicate and unexpected calls; rates overall, per language and per kind; two-step sentences (search first, then the call) and the `reachRate` and `extraSteps` figures
  - Done 2026-10-01: `scoring_tests.rs` also covers the strict schema check, the report, and that the embedded set and snapshot parse.

### Implementation for User Story 5

- [x] T040 [US5] Create `src-tauri/src/chat/eval/mod.rs` and `scoring.rs` (pure scoring and `EvalReport` per [contracts/eval-format.md](./contracts/eval-format.md)); declare `chat::eval` in `src-tauri/src/chat/mod.rs`
  - Done 2026-10-01: the report also carries `deterministic`.
- [x] T041 [P] [US5] Create `src-tauri/src/chat/eval/eval_set.json` (`version: 2`, about 28 sentences, German and English, kinds `read`, `change`, `smalltalk` with at least six `smalltalk`; `selfTest: true` on five sentences covering German and English and including at least one `smalltalk`; expected tools only from `tools.json`, at least eight sentences whose target is outside the core offer (they need the search step); no runtime ids as expected args), embedded via `include_str!` from `eval/mod.rs`
  - Done 2026-10-01: 30 sentences (24 expecting calls, 6 `smalltalk`), 11 outside the core offer, five marked `selfTest` (three `de`/`en` reads and changes plus two smalltalk).
- [x] T042 [US5] Create `scripts/export-eval-tools.ts` (writes `src-tauri/src/chat/eval/tools.json` from `listAgentActions(ALL_ACTIONS)` with German/English titles) and extend `scripts/check-agent-actions.ts`: `tools.json` equals the export (stale snapshot fails), every `expect.tool` exists in `tools.json`, the `eval_set.json` shape and the `selfTest` composition hold
  - Done 2026-10-01: `pnpm export:eval-tools` writes `tools.json`; `check:agent-actions` compares the snapshot with the catalog, checks every expected tool, runtime ids in args, and the composition (>= 6 smalltalk, >= 8 outside the core, five or six self-test sentences in both languages).
- [x] T043 [US5] Add deterministic sampling to the request: a `sampling` field on `ChatRequest` in `src-tauri/src/adapters/types.rs` (default `Default`, variant `Deterministic`), honored by `src-tauri/src/llm/local/stream.rs:build_request` via `set_deterministic_sampler` (feature-gated), ignored by other adapters; fix the struct-literal sites with `..Default::default()`. Then create `src-tauri/src/chat/eval/runner.rs`: `run_eval(adapter, set, tools, sentences) -> EvalReport` — per sentence up to two `stream_chat` steps with the core offer: if the model calls `find_actions` the runner executes the pure search over `tools.json`, extends the offer with `extend_offer` and scores the complete action-call batch in the second step; for direct cases it scores the complete first-step batch or text; missing, duplicate and unexpected calls are failures; no action is ever executed (FR-021); the report carries `reachRate` and `extraSteps`, `deterministic` recorded in the report
  - Done 2026-10-01: `ChatRequest.sampling` (`Default` or `Deterministic`), honored by the local adapter. `run_eval` takes the adapter, model id, flag, set, tools, `Which` (`All` or `SelfTest`) and a cancellation token. A test model that does what each sentence expects passes all 30, so the set, the snapshot and the search agree.
- [x] T044 [US5] Create `src-tauri/tests/model_tool_eval.rs` (`#[ignore]`, `#![cfg(feature = "llm-cpu")]` for the local path): reads `HOLZI_TEST_GGUF` (and optional `HOLZI_TEST_GGUF_TOKENIZER`) or `HOLZI_EVAL_PROVIDER` + `HOLZI_EVAL_MODEL` + the provider key from the environment (never from files), runs the full set, writes `target/eval/<model>.json`; needs no vault (FR-022)
  - Done 2026-10-01: `tests/model_tool_eval.rs` (local via `HOLZI_TEST_GGUF`, Anthropic via `HOLZI_EVAL_PROVIDER=anthropic`, `HOLZI_EVAL_MODEL` and `ANTHROPIC_API_KEY`); not run yet, see T046.
- [x] T045 [US4] Create `src-tauri/src/chat/tools/selftest.rs` (+ `selftest_tests.rs` with a scripted stub adapter): a background `tokio::spawn` that runs the `selfTest` subset through `run_eval` after an inconclusive probe for a local model with `tool_use == None`, aborts when the session switches the model, writes `Supported|Unsupported` with basis `SelfTest` through `set_tool_use` and emits `model-tool-use-updated`; pass threshold as the constant `SELF_TEST_PASS = 0.6` with a `ponytail:` comment pointing to T046; trigger it from `src-tauri/src/chat/model_loading.rs` after T035's probe; the chat is never blocked (scenario 7, FR-018b, SC-002a)
  - Done 2026-10-01: `chat/tools/selftest.rs` (`find_tool_use`, `spawn_tool_use_check` as gate work, `SELF_TEST_PASS`), triggered after a local model is ready. `ChatState::begin_tool_check` ends the previous check; a message sent, a model change, an unload and the vault close end the running one, so a person who chats right after loading may leave the model unchecked until its next load.
- [ ] T046 [US5] First full measurement (manual): run T044 twice against the local Qwen3-4B (and Qwen3-1.7B/0.6B) and against one Claude model with an API key; save the reports' summary to `specs/032-model-operates-holzi/eval-results.md` (no keys, no paths); check SC-001, SC-002, SC-005 and SC-008 (two runs ≤ 10 percentage points apart); run the self-test of T045 on each measured model, time it (SC-002a, ≤ 2 minutes, CPU only, Qwen3-4B) and compare its verdict with the full run (≥ 90 % agreement); derive `SELF_TEST_PASS`, confirm or change the composition of the core offer (`CORE_AGENT_TOOLS`), check `reachRate` against SC-005 (≥ 95 % within two steps) and the extra-step cost for the local model, optionally run the control with all 51 tools offered for cloud models (runner switch) to see whether the search step pays off, and set the `tool_use` entries for `model_catalog.json`; apply them in `selftest.rs`, `offer.rs` and the catalog; decide whether constrained decoding (research R10) is needed and, if so, record it as a follow-up spec idea in `eval-results.md`
  - Partial 2026-10-01 for Qwen3-4B (Q4_K_M, CPU, release build, deterministic): 16 of 30 (53 %), de and en 8 of 15 each, smalltalk 6 of 6, reads 4 of 11, changes 6 of 13, `reachRate` 0.54, no search step at all (`extraSteps` 0), no spurious call. Two runs gave the same report (SC-008). The model never calls `find_actions`: for the 11 sentences outside the core offer it answers in text or calls a wrong core tool, and two core sentences (open tabs, apps) get text only. Self-test subset 4 of 5 (0.8); `SELF_TEST_PASS` stays 0.6 and the catalog entry of Qwen3-4B says `supported`. Not measured: Qwen3 0.6B and 1.7B (not installed here) and a Claude model (no key in this environment); 30 sentences took about 7.8 minutes in a release build, the five of the self-test about 1.3. The full task remains open until the missing model measurements and the required comparison are available. With the tool instruction of research R18 (PR #198) the same model reaches 20 of 30 (67 %), `reachRate` 0.67, three searches, all 13 core sentences pass, the self-test subset 5 of 5; outside the core offer one of 11 passes.

**Checkpoint**: [quickstart.md](./quickstart.md) §5 done; self-test moves a fresh local model from unknown to a verdict within two minutes without blocking chat.

---

## Phase 8: Polish & Cross-Cutting

- [ ] T047 [P] Add the glossary entries to `CONTEXT.md`: "Werkzeug" = an action offered to a model, source `action`; "Wirkungsart/Risikostufe" read/change/destructive ↔ `Safe`/`Change`/`Risky`; keep the rule that "command" means Tauri commands
- [ ] T048 [P] Keep the locale files in lockstep: `src/i18n/locales/de.json` and `en.json` contain every new `chat.permission.*`, `chat.toolNotice.*` and dialog key; run `pnpm check:templates` and `pnpm check:settings`
- [x] T049 Run the CI-equivalent commands of [quickstart.md](./quickstart.md) §1: `pnpm check:agent-actions`, `check:wm-navigation`, `check:wm-state`, `check:chat-state`, `check:settings`, `check:templates`, `typecheck`, `typecheck:scripts`, `lint`, `format:check`, `cargo fmt --check`, `cargo test` and `cargo clippy --all-targets -- -D warnings` on default and `--no-default-features`, and `python3 scripts/ci/check-docs.py`; fix findings
  - Done 2026-10-01 on the state of `main` after #198: `check:agent-actions`, `check:wm-navigation`, `check:wm-state`, `check:chat-state`, `check:settings`, `check:templates`, `typecheck`, `typecheck:scripts`, `lint`, `format:check`, `cargo fmt --check`, clippy `-D warnings` on default and `--no-default-features` and `check-docs.py` are green locally; the full `cargo test` runs in CI only (a local full run once took the editor down), where each merged PR of the spec passed it.
- [ ] T050 Complete [quickstart.md](./quickstart.md) §4 by hand with Claude (API key) and local Qwen3-4B, including step 7 (self-test) and step 8 (delegate), and check SC-009 (someone who does not know the feature reaches the three US1 examples in free wording on the first try); update `specs/032-model-operates-holzi/checklists/requirements.md` if any criterion changed
- [ ] T051 Prepare the PR text: call out the three points that go beyond the spec wording for review — the third risk stage (change to spec 003's gate), the catalog field `builtinAgentCallable`, and the notice that reappears after an app restart; state the measured values from T046. Conventional Commits, no agent attribution, rebase-merge or merge-commit (no squash)
- [ ] T052 [P] Add a row to `plans/README.md` for the follow-up spec "Download- und Sync-Steuerung (Datenvolumen)": pause large downloads (models) and file sync on mobile to save data volume; status idea; touches specs 005, 025, 029; not part of 032
- [x] T053 Tell the model the user's local date and time in every turn (FR-025): `now_line` in `src-tauri/src/chat/tools/prompt.rs`, the clock moved from `assistant/speakable.rs` to `src-tauri/src/local_time.rs` (also built without `voice`), `utcOffsetMinutes` on `send_message` from `src/composables/useChat.ts`, the voice turn passes its conversation's offset; tests in `prompt_tests.rs`

---

## Dependencies & Execution Order

- **Phase 1** → **Phase 2** (blocking) → **US1** → **US2**. US2 needs the tool from US1 to test the matrix end to end, but its checks (T024, T025) only need Phase 2.
- **US3** needs Phase 2 and US1 (`set_agent_actions`, registry); it is independent of US2 and US4. T031a needs T030 (`find_actions`) and T031.
- **US4** needs Phase 2 (`ToolUse`) and US1 (`send_message` edits land in the same function as US3's T031 — do T031 before T036 or merge the edits carefully).
- **US5** needs US3 (`core_offer`, `search_actions`) and Phase 2 (`listAgentActions`). **T045** (label US4, self-test) needs T035 (probe) and T043 (runner). **T046** needs T044 and T045.
- Within a story: tests first (they fail), then implementation; Rust tasks that touch `src-tauri/src/chat/commands.rs` (T031, T036) run one after the other.

## Parallel Opportunities

- Phase 2: T005, T006, T008, T010 touch different files and can run in parallel after T003/T004/T007 land.
- US1: T013 and T014 (tests) in parallel; T019 and T020 (frontend) in parallel with T015–T018 (Rust).
- US2: T023 and T024 in parallel.
- US3: T028 first, then T029 and T030 in parallel; T031 and then T031a (same files, one after the other).
- US4: T033 in parallel with T032; T034 and T035 are sequential (shared storage and loading code).
- US5: T039 and T041 in parallel; T040 and T042 in parallel after T041.

## Implementation Strategy

1. **MVP = Phases 1–3 (US1)**: a model can read and navigate holzi in Manual mode. Stop and run the quickstart.
2. **Add US2**: modes and plain-language approvals make it safe to use by default. P1 is now shippable as a slice of the PR.
3. **Add US3 and US4**: quality for small local models and honest notices.
4. **Add US5**: measure, set the real thresholds (T046), then finish with Phase 8.
5. Everything lands in one PR; do not merge before T049 and T050 are green.
