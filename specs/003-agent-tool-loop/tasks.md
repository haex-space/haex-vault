---
description: 'Actionable, dependency-ordered task list for the agent-tool-loop feature'
---

# Tasks: Agent Tool Loop

**Input**: Design documents from `/specs/003-agent-tool-loop/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/tauri-commands.md
**Tests**: Backend tests are REQUIRED (repo convention: every module gets its own `*_tests.rs`, never
inline `#[cfg(test)] mod tests`, plus integration tests under `src-tauri/tests/`). Frontend
verification is manual per quickstart.md (kein Playwright in diesem Repo).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1..US4)

## Path Conventions

- Backend: `src-tauri/src/` and `src-tauri/tests/` (Rust)
- Frontend: `src/` (Nuxt 4 SPA)
- Docs: `specs/003-agent-tool-loop/` (this feature's design docs)

---

## Phase 1: Setup (Shared Infrastructure)

- [x] T001 [P] Add `rmcp = "=3.3.0"` to `[dependencies]` in `src-tauri/Cargo.toml` (research.md §3 — official Rust MCP SDK, exact pin)
- [x] T002 [P] Create `src-tauri/src/chat/tools/mod.rs` with the `Tool` trait (`name`, `description`, `input_schema`, `risk_class() -> RiskClass`, `async execute(Value) -> ToolResult`) using the existing `async-trait` pattern so `Box<dyn Tool>` is object-safe on Rust 1.77.2; add `RiskClass::{Safe, Risky}`, `ToolResult { content: String, is_error: bool }`, and an empty `ToolRegistry` struct (`Vec<Box<dyn Tool>>`, no tools registered yet); add `pub mod tools;` to `src-tauri/src/chat/mod.rs`

**Checkpoint**: Module skeleton and pinned dependency in place. No behavior changes yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Data model and adapter-interface changes every user story builds on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T003 Add Migration `0014_chat_messages_tool_columns` in `src-tauri/src/identity/migrations.rs`: `ALTER TABLE chat_messages ADD COLUMN` for `tool_name`, `tool_call_id`, `tool_input`, `tool_is_error`, `tool_source` (all nullable TEXT/INTEGER per data-model.md), each as its own `--> statement-breakpoint` step matching the `0013` migration's style
- [x] T004 Extend `MessageRole` in `src-tauri/src/storage/chat_messages.rs` with `ToolCall` and `ToolResult` variants; extend `ChatMessage` with `tool_name: Option<String>`, `tool_call_id: Option<String>`, `tool_input: Option<String>`, `tool_is_error: Option<bool>`, `tool_source: Option<String>`; update `insert_message`/row-mapping to read/write the new columns
- [x] T005 [P] Create `src-tauri/src/storage/chat_messages_tests.rs` covering: inserting a `ToolCall` row round-trips all tool fields; inserting a `ToolResult` row round-trips `tool_call_id`/`tool_is_error`; invalid rows are rejected when required role fields are missing, tool fields are present on non-tool roles, or fields from the wrong tool role are mixed; a pre-migration `User`/`Assistant` row still round-trips with all new fields `None`. Register `#[cfg(test)] mod chat_messages_tests;` in `src-tauri/src/storage/mod.rs`
- [x] T006 [P] Add `ToolSpec { name: String, description: String, input_schema: serde_json::Value }` and `ToolCall { id: String, name: String, input: serde_json::Value }` to `src-tauri/src/adapters/types.rs`; add `tools: Vec<ToolSpec>` to `ChatRequest`; extend `ChatRole`/`ChatMessage` with `ToolCall`/`ToolResult` variants (mirroring storage's roles, data-model.md); add `StreamChunk::ToolCalls(Vec<ToolCall>)` variant, emitted before `Done`
- [x] T007 Add `tool_registry: crate::chat::tools::ToolRegistry` and `pending_tool_approvals: Mutex<HashMap<Uuid, tokio::sync::oneshot::Sender<ApprovalDecision>>>` (new `ApprovalDecision::{Allow, Deny}` enum in `chat/tools/mod.rs`) to `ChatState` in `src-tauri/src/chat/session.rs`
- [x] T008 Add `FinishReason::ToolLimitReached` to `src-tauri/src/storage/chat_messages.rs`'s `FinishReason` enum (contracts/tauri-commands.md), including its `as_str`/parse mapping

**Checkpoint**: Data model, adapter types, and `ChatState` ready. User-story phases can begin.

---

## Phase 3: User Story 1 - Assistant completes a task using tools (Priority: P1) 🎯 MVP (part 1 of 2, ships with US2)

**Goal**: The assistant can call the host-CLI tool and MCP-provided tools, across multiple rounds within one response, with results grounded in real tool output.

**Independent Test**: Ask the assistant something requiring the CLI tool or an MCP tool with permission mode `auto`; verify tool calls/results appear in `chat_messages` and the final answer reflects the actual result (spec.md Acceptance Scenarios 1-3).

### Tests for User Story 1

- [x] T009 [P] [US1] Integration test in `src-tauri/tests/chat_tool_loop.rs`: a stubbed adapter emits one `ToolCalls` frame, the loop executes the tool, persists `tool_call`+`tool_result` rows, then a second step produces the final `assistant` row — assert row order and `parent_id` chain
- [x] T010 [P] [US1] Integration test in `src-tauri/tests/chat_tool_loop.rs`: a tool execution returns `is_error: true` — assert the response continues to a final answer instead of ending in `FinishReason::Error`
- [x] T011 [P] [US1] Integration test in `src-tauri/tests/chat_tool_loop.rs`: a stubbed adapter that always requests the same tool call exceeds the fixed round limit — assert `FinishReason::ToolLimitReached` and no further step runs
- [x] T011A [P] [US1] Adapter test in `src-tauri/src/adapters/anthropic_tests.rs`: two ordered `ToolCall` values reconstruct as exactly one assistant message with two `tool_use` blocks followed by one user message with matching `tool_result` blocks, preserving call order and IDs (data-model.md)

### Implementation for User Story 1

- [x] T012 [US1] Rewrite the `tauri::async_runtime::spawn` block in `send_message` (`src-tauri/src/chat/commands.rs`) into a turn/step loop: after `StreamChunk::ToolCalls`, execute each call via `ChatState.tool_registry`, persist `tool_call`+`tool_result` rows (T004), rebuild `ChatRequest.messages` history including them, issue the next step; stop on `Done` with no tool calls, on the fixed round limit (`FinishReason::ToolLimitReached`), or on error
- [x] T013 [US1] Extend `AnthropicAdapter::stream_chat` in `src-tauri/src/adapters/anthropic.rs` to send `ChatRequest.tools` as the request's `tools` array and buffer `content_block_start`/`content_block_delta` (`input_json_delta.partial_json`)/`content_block_stop` per block index into complete `ToolCall`s, emitted as `StreamChunk::ToolCalls` before `Done` when `stop_reason: "tool_use"` (research.md §1)
- [x] T014 [US1] Extend `LocalAdapter`/`LocalModel` (`src-tauri/src/adapters/local.rs`, `src-tauri/src/llm/local.rs`, under `llm-cpu`) to call `RequestBuilder::set_tools`/`set_tool_choice` with `ChatRequest.tools`, and map `ChatCompletionResponse.choices[0].message.tool_calls` / the streamed `Delta.tool_calls` (parsing `CalledFunction.arguments` as JSON) into `StreamChunk::ToolCalls` (research.md §2)
- [x] T015 [P] [US1] Create `src-tauri/src/chat/tools/cli.rs`: a `Tool` implementation spawning `tokio::process::Command`, `risk_class()` always returns `Risky`, no working-directory or command restriction (spec.md FR-015); register it statically in `ToolRegistry`
- [x] T016 [P] [US1] Create `src-tauri/src/chat/tools/cli_tests.rs`: executing a trivial command returns its stdout as `ToolResult.content`; a failing command returns `is_error: true` without panicking; register `#[cfg(test)] mod cli_tests;` in `chat/tools/mod.rs`
- [x] T017 [P] [US1] Create `src-tauri/src/chat/tools/mcp.rs`: using `rmcp`, connect to the user's configured MCP servers, call `tools/list`, wrap each returned tool as a `Tool` (`risk_class()` always `Risky` per design doc §4), forwarding `tools/call` in `execute()`
- [x] T018 [P] [US1] Create `src-tauri/src/chat/tools/mcp_tests.rs` against a minimal in-process test MCP server: tool discovery populates the registry and a tool whose server disconnects during execution returns an error result instead of panicking. Unspawnable servers are skipped with a warning during discovery; reconnection wiring remains T019.
- [ ] T019 [US1] Wire `ToolRegistry` population in `src-tauri/src/lib.rs`/`ChatState::new`: CLI tool is registered unconditionally and MCP refresh/discovery helpers disambiguate collisions, but no configured-server reconnection caller exists in this PR.
- [x] T020 [US1] Emit `chat-tool-call`/`chat-tool-result` events (contracts/tauri-commands.md) from the turn loop (T012) at the point each row is persisted; emit `chat-tool-call` even for a persisted Plan-mode-blocked call
- [ ] T021 [US1] Update `src/composables/useChat.ts` to render `tool_call`/`tool_result` message rows and subscribe to the new events; keep `chat-message-complete`/`chat-message-error` per-step, subscribe to `chat-turn-complete`, and clear `streamingMessageId`/`busy` only on that final-turn event. The production consumer and i18n strings are present, but this repo has no frontend test runner for the required consumer coverage.

**Checkpoint**: Tool calls execute and are visible end-to-end, but every tool call is unconditionally allowed — Phase 4 (US2) is required before this is safe to ship (spec.md User Story 2 rationale).

---

## Phase 4: User Story 2 - User controls which actions need approval (Priority: P1) 🎯 MVP (part 2 of 2)

**Goal**: Manual/Auto/Plan permission modes gate tool execution per spec.md FR-003–FR-006.

**Independent Test**: Toggle `chat.permission_mode` and trigger a tool use; verify the approval prompt appears/doesn't appear and blocks/doesn't block exactly as spec.md Acceptance Scenarios 1-5 describe.

### Tests for User Story 2

- [x] T022 [P] [US2] Unit test in `src-tauri/src/chat/tools/permission_tests.rs`: the decision matrix (`Manual`+`Safe`→Ask, `Manual`+`Risky`→Ask, `Auto`+`Safe`→Allow, `Auto`+`Risky`→Ask, `Plan`+`Safe`→Allow, `Plan`+`Risky`→Deny-no-prompt)
- [x] T022A [P] [US2] Add a test-only stub `Tool` (`risk_class() -> Safe`) in `src-tauri/tests/chat_tool_loop.rs`, used only to exercise the Auto/Plan-mode Safe pass-through path in integration tests — not shipped as a real tool (analysis finding E1: no production tool is ever classified `Safe` in this feature's scope, so the Auto-mode "safe tools don't interrupt" acceptance scenario has nothing else to exercise it end-to-end)
- [x] T023 [P] [US2] Integration test in `src-tauri/tests/chat_tool_loop.rs`: `Manual` mode emits `tool-permission-request` for a `Safe` tool too (using the T022A stub) and for the CLI tool (`Risky`); the loop does not proceed for either until `respond_tool_permission` is called
- [x] T024 [P] [US2] Integration test in `src-tauri/tests/chat_tool_loop.rs`: denying a request produces a `tool_result` with `is_error: true` and the response continues (spec.md Acceptance Scenario 4); an unanswered request leaves the turn waiting indefinitely, not auto-decided (Scenario 5 — assert no state change after a simulated delay); under `Plan` mode, assert zero `tool-permission-request` events fire when a `Risky` action is blocked (analysis finding E4)
- [x] T024A [P] [US2] Integration test in `src-tauri/tests/chat_tool_loop.rs`: two independent `Risky` tool calls within one turn each get their own `tool-permission-request`; approving one does not resolve or affect the other (spec.md Edge Case — analysis finding E2)
- [x] T024B [P] [US2] Integration test in `src-tauri/tests/chat_tool_loop.rs`: changing `chat.permission_mode` while a `tool-permission-request` is pending does not affect that pending request; only the next tool use in the same turn observes the new mode (spec.md Edge Case — analysis finding E3)

### Implementation for User Story 2

- [x] T025 [US2] Create `src-tauri/src/chat/tools/permission.rs` with `fn decide(mode: PermissionMode, risk: RiskClass) -> Decision::{Allow, Ask, Deny}` implementing the matrix from T022; add `PermissionMode::{Manual, Auto, Plan}` (parsed from the `chat.permission_mode` preference string, default `Manual` per spec.md Assumptions)
- [x] T026 [US2] Wire the gate into the turn loop (T012): before `execute()`, call `decide()`; on `Ask`, mint a `request_id`, store a `oneshot::Sender` in `ChatState.pending_tool_approvals`, emit `tool-permission-request` (contracts.md), and await the receiver racing against the existing abort signal via `tokio::select!`; on `Deny` (from `Plan`), skip execution and synthesize a `tool_result` row with `is_error: true` and a fixed reason string (no localized text from backend, per `CONTEXT.md` i18n boundary)
- [x] T027 [US2] Add `respond_tool_permission(request_id, decision)` command in `src-tauri/src/chat/commands.rs`, resolving the matching `oneshot::Sender` from `pending_tool_approvals`; register it in the `invoke_handler` in `src-tauri/src/lib.rs`
- [x] T028 [P] [US2] Add a `chat.permission_mode` key constant next to the existing `PREF_LAST_ACTIVE_MODEL`/`PREF_DEFAULT_MODEL` constants in `src-tauri/src/chat/commands.rs` — no new storage module, reuses `storage::preferences`/`PrefScope::Device` exactly like the existing model preferences
- [x] T029 [US2] Add a permission-mode switcher (Manual/Auto/Plan) and an approval-request dialog to the chat UI in `src/composables/useChat.ts` + a new component under `src/components/chat/`; add `de`/`en` i18n strings

**Checkpoint**: User Stories 1 and 2 together form the safe MVP — tool use exists and is user-controlled. Deployable/demoable here.

---

## Phase 5: User Story 3 - User stops a response mid-action (Priority: P2)

**Goal**: `abort_current_generation` halts a running tool process or an open approval wait immediately, ending the whole turn (spec.md FR-009–FR-011).

**Independent Test**: Start a response using the CLI tool, abort mid-execution; verify the OS process is gone and no further step runs (spec.md User Story 3 Acceptance Scenarios).

### Tests for User Story 3

- [x] T030 [P] [US3] Integration test in `src-tauri/tests/chat_tool_loop.rs`: aborting while a (simulated slow) tool is executing kills the process and yields `FinishReason::Cancelled` with no further `chat_messages` rows
- [x] T031 [P] [US3] Integration test in `src-tauri/tests/chat_tool_loop.rs`: aborting while a `tool-permission-request` is pending resolves the wait immediately as cancelled, not as a denial (distinguish `tool_is_error` reason from a user `Deny`)

### Implementation for User Story 3

- [x] T032 [US3] Extend `ChatState`/the turn loop (`src-tauri/src/chat/session.rs`, `chat/commands.rs`) to track the current tool execution's cancellation handle (`tool_cancellation: Mutex<Option<CancellationToken>>`) alongside the existing generation `AbortHandle`; for an in-flight `tokio::process::Command` (T015), the child now spawns in its own process group (`process_group(0)`, Unix) / is torn down via `taskkill /T /F` (Windows) so a killed shell cannot leave a grandchild running, then `Child::kill().await` + `wait().await` reaps it (`kill_on_drop` only as an unreached fallback). For MCP, `execute()` races `call_tool()` against the same signal and drops it on cancellation — sending a spec-correct `notifications/cancelled` was not implemented: `RunningService::call_tool()`'s MRTR convenience wrapper does not expose the raw JSON-RPC request id that notification needs, and no product surface can reach this path yet (MCP servers are not user-configurable in this feature, spec.md Assumptions). `abort_turn` (factored out of the `#[tauri::command]` for direct test use) also drains `pending_tool_approvals`, resolving any pending wait as cancelled rather than denied.
- [x] T033 [US3] Confirmed via T030/T031: both scripts exactly one adapter step, so a code path that incorrectly issued a further step would panic the stub adapter and fail `handle.await.unwrap()`. `run_turn` also checks `cancel.is_cancelled()` immediately before minting a new approval wait and right after a round's `executed` results resolve, ending the turn as `Cancelled` in both cases without persisting the interrupted round's rows.

**Checkpoint**: Cancellation works uniformly across plain generation, a running tool, and a pending approval.

---

## Phase 6: User Story 4 - Assistant recovers from temporary connection problems (Priority: P3)

**Goal**: A bounded, backed-off automatic retry at the LLM-request level, invisible to the user on success (spec.md FR-012–FR-014).

**Independent Test**: Simulate one transient failure then success; verify only the final answer is visible in the conversation (spec.md User Story 4 Acceptance Scenarios).

### Tests for User Story 4

- [x] T034 [P] [US4] Unit test in `src-tauri/src/adapters/anthropic_tests.rs` (extend existing file): classify which adapter errors are transient (timeout, 5xx, rate-limit) vs. terminal (4xx auth/validation). Added `StreamError::Transient` (Anthropic's `overloaded_error`/`rate_limit_error`/`api_error` SSE `error.type` values map here; everything else stays `StreamError::Model`, terminal) plus `StreamError::is_transient()`/`AdapterError::is_transient()` (`Http`/429/5xx → transient; `InvalidCredentials`/`Parse`/other 4xx → terminal).
- [x] T035 [P] [US4] Integration test in `src-tauri/tests/chat_tool_loop.rs`: a stubbed adapter fails once transiently then succeeds — assert exactly one `assistant` row is persisted, no trace of the failed attempt
- [x] T036 [P] [US4] Integration test in `src-tauri/tests/chat_tool_loop.rs`: a stubbed adapter fails transiently past the retry limit — assert `FinishReason::Error`, distinguishable from `Cancelled` and `ToolLimitReached`

### Implementation for User Story 4

- [x] T037 [US4] Implement a bounded retry-with-backoff wrapper (`run_step`, `MAX_RETRY_ATTEMPTS = 3`, exponential 500ms/1s/2s) around the per-step adapter call in the turn loop (`src-tauri/src/chat/commands.rs`), using the error classification from T034, covering both a `stream_chat` restart and a mid-stream failure under one shared attempt budget; emits `chat-retry` and persists nothing from a failed attempt (design doc §3 invariant). **Deviation from this task's original wording, confirmed with the operator**: tokens stream live via the existing `chat-token` event exactly as a non-retried step would, rather than being buffered server-side and replayed only after success — spec.md's Acceptance Scenarios are framed around the persisted conversation history ("no failed attempt appears in the conversation history"), which server-side buffering isn't needed to satisfy, and buffering would have also suppressed live streaming for the (dominant) non-retried case and changed the already-shipped Story 3 cancellation UX. On a transient failure `chat-retry` fires before the backoff/restart so the frontend can clear that attempt's now-discarded partial text (T039) instead.
- [x] T038 [US4] Emit the transient `chat-retry` event (contracts/tauri-commands.md) on each retry attempt; not persisted to `chat_messages`
- [x] T039 [P] [US4] Update `src/composables/useChat.ts` to show a subtle "retrying…" indicator on `chat-retry` without adding a message row; add `de`/`en` i18n strings. Implemented in `src/pages/chat/[instance].vue` (`retryingMessageId`, `handleRetry`): clears the currently-streaming message's shown partial text/reasoning and displays `chat.retrying` next to the message header until the next attempt's tokens arrive or the turn ends. Not wired into the `pendingStreamEvents` pre-setup-race buffer (unlike token/complete/error/turnComplete) — a retry landing in that sub-invoke() window has no rendered partial text to clear yet, and a retry only fires after at least one real network round-trip, so the window is not realistically reachable.

**Checkpoint**: All four user stories independently functional and tested.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T040 Run all quickstart.md scenarios manually against a dev build (`pnpm tauri:dev`)
- [x] T041 [P] Verify `de`/`en` i18n lockstep for every string added across T021/T029/T039 (`CONTEXT.md` i18n boundary requirement). Verified programmatically: `en.json`/`de.json` have byte-for-byte identical key trees, and every `chat.*` key referenced from `src/pages/chat/[instance].vue` + `src/components/chat/PermissionPrompt.vue` (14 keys, including `chat.retrying`) resolves in both locales.
- [x] T042 Run `cargo test --lib` and `cargo test --test chat_tool_loop` (and existing suites `chat_message_idempotency`, `local_inference`, `provider_models`) — all green. 76 lib tests, 15 `chat_tool_loop` (12 pre-existing + T030/T031/T035/T036 + one bonus terminal-not-retried test), 11 idempotency, 4 provider_models pass; `local_inference`'s 2 tests remain `ignored` (need `HAEX_TEST_GGUF`, unrelated to this feature). `cargo clippy --lib --tests --all-targets` clean. `pnpm typecheck` exit 0.
- [x] T043 Update `plans/001-desktop-mvp.md`'s `cli_delegate` host-authentication line if any of this feature's work touches provider rows shared with that track (expected: no overlap, since `cli_delegate` stays out of scope — verify, don't assume). Verified: Phase 5/6 touched `ChatState` (a new `tool_cancellation` field), `chat/tools/{cli,mcp}.rs` (cancellation), `chat/commands.rs` (`run_step`/`retry_or_bail`/`abort_turn`), the adapter error types, and the frontend retry indicator — none of it reads/writes the `providers` table or the `cli_delegate` temp-dir credential flow §8.3-8.4 describes. No overlap; no update needed.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup — BLOCKS all user stories
- **US1 (Phase 3)** and **US2 (Phase 4)**: both depend on Foundational; ship together as the MVP (spec.md explicitly ties them — tool execution without approval control is not an acceptable intermediate state)
- **US3 (Phase 5)**: depends on Foundational + the turn loop existing (T012 from US1) + US2's approval machinery (T026/T027), because cancellation also resolves pending approvals
- **US4 (Phase 6)**: depends on Foundational + the turn loop existing (T012 from US1); independent of US2/US3
- **Polish (Phase 7)**: depends on whichever stories are in scope for the release

### Within Each User Story

- Tests written first, expected to fail before implementation lands
- Data/adapter changes before loop wiring before UI

### Parallel Opportunities

- T001/T002 in parallel
- T005/T006 in parallel (different files); T004 blocks T005
- T015-T018 (CLI tool + MCP tool + their tests) in parallel with each other, after T006/T007
- T009-T011A (US1 tests) in parallel with each other before T012
- T022, T022A, T023, T024, T024A, T024B (US2 tests) in parallel; T034-T036 (US4 tests) in parallel; T030-T031 (US3 tests) in parallel
- US3 and US4 implementation phases can proceed in parallel with each other once US1's T012 and US2's T026/T027 land; US3 depends on US2 for approval cancellation, while US4 remains independent of US2/US3

---

## Parallel Example: User Story 1

```bash
# Tests together:
Task: "Integration test: multi-step tool loop persists ordered rows, in src-tauri/tests/chat_tool_loop.rs"
Task: "Integration test: tool error doesn't end the response, in src-tauri/tests/chat_tool_loop.rs"
Task: "Integration test: round limit reached, in src-tauri/tests/chat_tool_loop.rs"

# Tool implementations together (independent files):
Task: "Host-CLI tool in src-tauri/src/chat/tools/cli.rs"
Task: "MCP-client tool discovery in src-tauri/src/chat/tools/mcp.rs"
```

---

## Implementation Strategy

### MVP (User Stories 1 + 2 together)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks everything)
3. Phase 3: User Story 1
4. Phase 4: User Story 2
5. **STOP and VALIDATE**: run quickstart.md Szenario 1 + 2 — tool use works AND is user-controlled
6. This is the smallest safe, demoable increment — US1 alone (tool execution with no approval gate) is explicitly not an acceptable ship point per spec.md

### Incremental delivery after MVP

7. Phase 5 (US3, cancellation) and Phase 6 (US4, retry) can land in either order, or in parallel if staffed — each independently testable via its own quickstart scenario
8. Phase 7: Polish once all four stories are in
