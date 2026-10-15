# Tasks: Werkzeuge von Erweiterungen für den Agenten

**Input**: Design documents from `/specs/018-extension-agent-tools/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/manifest-tools.md,
contracts/mcp-port.md, contracts/tauri-commands.md, quickstart.md

**Tests**: Test-first per task pair (write the failing check, then implement). Pure frontend logic in
`scripts/check-*.ts` (`node --test`, relative `.ts` imports, no `~/` alias); Rust unit tests in sibling
`*_tests.rs` files (never inline `mod tests { … }`), integration tests in `src-tauri/tests/`. vault-sdk
tests in vitest. No new runtime dependency in holzi or vault-sdk.

**Conventions**: Pure modules under `src/lib/` import only relative `.ts` paths. i18n keys go into both
`src/i18n/locales/de.json` and `en.json`. Every changed file stays under 500 lines; files already above
(`chat/commands.rs`, `useChat.ts`) only get the minimal hook. Cross-repo references pin full commit SHAs
(constitution IV). Commit per task group with Conventional Commits. vault-sdk work happens in its own
worktree on a topic branch from `origin/main` (`40ea29ce5ee2c913a541e97033e9e885fa6372bb` or newer); its PR is
opened by the operator's account rules (haex-space repo: `gh auth switch --user haex-space`, switch back
afterwards).

**Deliveries** (plan, research R15): C1 = T003–T005; A = T006–T011 (vault-sdk); B = T012–T018 (Phase 2
holzi manifest model + Phase 3, US2); C2 + D = Phases 4–6; E = Phase 8. Each delivery is its own PR.

## Phase 1: Setup

- [x] T001 Rebase `018-extension-agent-tools` on `origin/main`; confirm PR #347 (spec 046) is merged and `src-tauri/src/chat/tools/offer.rs` has `MAX_ACTION_OFFER = 16` and `word_matches`; confirm `rmcp = "=3.5.0"` in `src-tauri/Cargo.toml` and that `rmcp-3.5.0/src/transport/sink_stream.rs` exists in the cargo registry
- [x] T002 Refresh the graph (`graphify` on the worktree) and run one bounded `graphify query "<intent>" --budget 1000` each for `src-tauri/src/extensions/agent_tools/link.rs`, `src-tauri/src/chat/tools/extension_offer.rs`, `src/lib/extensions/mcpRelay.ts`, `openAppInBackground` in `src/lib/wm/layoutState.ts`; record candidates and why each did not match in the head section of `specs/018-extension-agent-tools/research.md`. If a near-identical candidate appears, stop and ask the operator

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The MCP contract (C1, holzi), the vault-sdk building block and format checks (A), and the
manifest model in holzi. **⚠️ No user story work can begin until this phase is complete.**

### C1 — MCP contract and transcripts (holzi)

- [x] T003 Contract test as a unit test next to the link, `src-tauri/src/extensions/agent_tools/contract_tests.rs` (the in-memory server needs `rmcp`'s dev-only `server` feature, so link and contract tests share `agent_tools/test_support.rs`): holzi's client joined to an in-memory `rmcp` server (`echo`, `slow`, `fail`) through JSON like the frontend relay; records every message with direction into `specs/018-extension-agent-tools/contracts/transcripts/{handshake,list,call-ok,call-invalid-input,call-error,call-cancelled,server-request-rejected}.jsonl` when `HOLZI_WRITE_MCP_TRANSCRIPTS=1`, otherwise compares (fails when stale); protocol pinned to `2025-11-25`, `clientInfo` holzi, versions normalized, per `contracts/mcp-port.md`
- [x] T004 Add the client-side guard (research R2) in new `src-tauri/src/extensions/agent_tools/link.rs`: a `ClientHandler` that declares no capabilities and answers every server request except `ping` (`sampling/createMessage`, `roots/list`, `elicitation/create`, any other) with `-32601`; `connect(send, receive)` returns the running client. Test in `src-tauri/src/extensions/agent_tools/link_tests.rs` (server sends each request → error, nothing else happens); the transcript `server-request-rejected.jsonl` from T003 uses this handler
- [x] T005 Generate the transcripts (`HOLZI_WRITE_MCP_TRANSCRIPTS=1 cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib agent_tools`), commit them; open the C1 PR (`test(extensions): record the MCP exchange for extension tools`) and note its merge SHA for vault-sdk; done: merged as #357, `0ffd3a45ee5c398a023c55bd0483d3050b8fe604`

### A — vault-sdk (separate repo, separate PR)

- [x] T006 [P] In vault-sdk `crates/haex-bundle/src/verify.rs` `assert_valid_manifest`: validate the optional `tools` block per `contracts/manifest-tools.md` (rules 1–8) → `manifest_invalid`; tests in the crate's test module style; add vectors `test-vectors/bundles/{ok-tools,bad-tools-name,bad-tools-effect,bad-tools-duplicate,bad-tools-examples}.xt` via `scripts/generate-bundle-vectors.mjs` and `expected.json`
- [x] T007 [P] In vault-sdk `src/types.ts`: add `ManifestTool` and `ExtensionManifest.tools?: ManifestTool[]`; `readManifest` keeps the field (already spreads `...parsed`), add a typed test
- [x] T008 In vault-sdk new `src/api/tools/schema.ts`: input validator for holzi's schema subset (same keywords as `schema_in_subset` in holzi `src-tauri/src/chat/tools/action_tool.rs`); tests in `src/api/__tests__/toolsSchema.test.ts` (valid, missing required, wrong type, enum, nested object)
- [x] T009 In vault-sdk new `src/api/tools/server.ts`: minimal MCP server per `contracts/mcp-port.md` (`initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`, `notifications/cancelled`, `-32601` otherwise); tool metadata from `HaexHubConfig.manifest.tools`, handlers from a registry, `AbortSignal` per call, results/errors as in the contract. Test `src/api/__tests__/toolsTranscripts.test.ts` replays holzi's transcripts (copied into `src/api/__tests__/transcripts/` with a README naming holzi repo + C1 merge SHA + path)
- [x] T010 In vault-sdk `src/api/tools/index.ts` + `src/client.ts`: `sdk.tools.register(name, handler)` (`ToolsAPI`, constructed next to `actions`); a name not in the manifest throws; port messages `{type: "haexspace:mcp", message}` routed in `src/client/events.ts` `processEvent` (and the Tauri `listen()` list in `src/client/init.ts`); replies via `port.postMessage({type: "haexspace:mcp", message})`; extend `src/client/__tests__/portHandshake.test.ts` with an MCP round trip over a real `MessageChannel`
- [x] T011 vault-sdk: typecheck, lint, test, build green; open the PR (`feat(tools)!:` only if the API breaks, else `feat(tools): serve extension tools to the host over MCP`); after merge and release note the merge SHA and npm version; done: haex-space/vault-sdk#64, merge `280cebf63eba72422d1ddd62e1edeb5ad7e7d3cc`, released as `@haex-space/vault-sdk` 4.3.0

### holzi manifest model (after A is merged)

- [x] T012 Bump `haex-bundle` in `src-tauri/Cargo.toml` to the A merge SHA; refresh `src-tauri/tests/fixtures/extension_bundles/` from vault-sdk `test-vectors/bundles/` (update `SOURCE.md` with repo + SHA); `cargo test --test extension_bundle_format` green including the new `*-tools*` vectors
- [x] T013 [P] Test then implement `Manifest.tools: Vec<ManifestTool>` in `src-tauri/src/extensions/bundle/manifest.rs` for `from_verified`, `from_dev_file`, `from_stored` (fields per data-model.md, `supported` = `schema_in_subset(input_schema)`); tests in `src-tauri/src/extensions/bundle/manifest_tests.rs`
- [x] T014 [P] Test then implement `PermissionKind::AgentTool` (`as_str` `agentTool`, not device-scoped) in `src-tauri/src/extensions/permissions/model.rs` and the mapping tools → `DeclaredPermission {kind: AgentTool, action: effect, target: name}` in `src-tauri/src/extensions/permissions/manifest_map.rs`; tests in the sibling `*_tests.rs`

**Checkpoint**: The SDK can serve tools; holzi reads and maps them; nothing is offered to the agent yet.

---

## Phase 3: User Story 2 - Werkzeuge bei der Installation sehen und bestätigen (Priority: P1)

**Goal**: Tools appear in the install/update dialog and are stored as `agentTool` permissions.

**Independent Test**: Install the test extension (T030 builds it; until then a fixture built in the test)
→ dialog lists its tools with effect; update with one more tool → only that one is asked.

- [x] T015 [US2] Test in `src-tauri/src/extensions/registry/install_tests.rs`: install preview lists `agentTool` rows with effect; an update that adds a tool or changes an effect yields exactly those in `new_permissions`; an update that only changes `inputSchema` yields none (FR-002); a removed tool's row is deleted by `apply_declarations`; tests in `install_agent_tools_tests.rs` (`install_tests.rs` stays under 500 lines)
- [x] T016 [US2] Make T015 pass in `src-tauri/src/extensions/registry/install.rs` (titles for the preview: add `title_de`/`title_en` to `DeclaredPermissionView` for `agentTool`, ts-rs binding regenerated with a domain-prefixed name per memory "ts-rs name collision"); the view moved to `registry/declared_view.rs`
- [x] T017 [P] [US2] `src/components/extensions/InstallDialog.vue`: group `agentTool` rows under "Werkzeuge für den Agenten" with title and effect label; unticked = `ask` as for other kinds; i18n keys `extensions.install.agentTools.*`, `extensions.permissions.kind.agentTool`, `extensions.permissions.effect.{read,change,destructive}` in de/en; `pnpm check:templates`; the kind label went into the existing group `extensions.permissions.kinds`
- [x] T018 [US2] Extract the dialog's grouping and labels for `agentTool` rows (group, title per locale, effect label key, sort by title) into pure `src/lib/extensions/agentTools.ts`, used by `InstallDialog.vue` (T017) and `PermissionsView.vue` (T036); tests in new `scripts/check-extensions-agent-tools.ts`, added to `check:extensions` in `package.json`

**Checkpoint**: Tools are confirmed and stored per vault; still not offered.

---

## Phase 4: User Story 1 - Der Agent beantwortet eine Frage mit dem Werkzeug einer Erweiterung (Priority: P1) 🎯 MVP

**Goal**: A confirmed tool is offered when it fits the message, called over MCP, and answered.

**Independent Test**: quickstart steps 2 and 3.

### Transport and call (delivery C2)

- [x] T019 [P] [US1] Pure relay helpers in new `src/lib/extensions/mcpRelay.ts` (recognize `{type: "haexspace:mcp", message}` from the port, build the outgoing port message, drop malformed ones); tests in `scripts/check-extensions-mcp-relay.ts`, added to `check:extensions`
- [x] T020 [US1] `src/composables/useExtensionFrame.ts`: forward `haexspace:mcp` port messages to `invoke('extension_mcp_send', {frame, message})`; listen to `extension-mcp-message` for this frame and post to the port; `pnpm check:extensions`, `pnpm typecheck`
- [x] T021 [US1] New `src-tauri/src/extensions/agent_tools/commands.rs`: `extension_mcp_send` (frame session lookup like `extension_bridge_call`, unknown frame dropped) feeding the link's inbound channel; outbound channel emits `extension-mcp-message`; register in `src-tauri/src/lib.rs`; tests in `commands_tests.rs`; `tests/extension_bridge_contract.rs` stays unchanged and green
- [x] T022 [US1] `src-tauri/src/extensions/bridge/frames.rs`: per-session `McpLink` slot (created on first call, closed with the session) and an in-flight call counter; tests in `frames_tests.rs` (session close ends the link and fails pending calls); the slot lives in `agent_tools/slot.rs`, `frames.rs` holds it and closes it with the session, a reload resets it
- [x] T023 [US1] Background frame (research R9): `openAppInBackground(state, appId)` in `src/lib/wm/layoutState.ts` (new minimized window, `activeWindowId` and focus unchanged, existing instance wins, compact layout → background tab without switching; read 043/045 code for compact mode first and note the finding in research R9); tests in `scripts/check-wm-background.ts` (part of `check:wm-state`); `src/stores/windowManager.ts` answers the Rust event `extension-agent-frame {extensionId, requestId}` with `extension_agent_frame_ready {requestId, frame}` once the frame's port handshake is done (or `null` after 10 s)
- [x] T024 [US1] New `src-tauri/src/extensions/agent_tools/mod.rs`: build `ExtensionToolDef`s from installed, enabled, ready extensions + `agentTool` rows (`*` denied → none; tool denied → none; no row → none; `ask` → always ask; `supported == false` → none); model-facing name `x_<slug>_<tool>` with collision suffix, ≤ 64 chars, `is_valid_tool_name`; tests in `mod_tests.rs` (FR-003, FR-009, table in data-model.md); landed as `agent_tools/defs.rs` (`tool_defs` pure, `load` from the vault) with `defs_tests.rs`
- [x] T025 [US1] `src-tauri/src/chat/tools/mod.rs`: `Tool::origin() -> Option<ToolOrigin>` (default `None`) and `ToolOrigin {extension_id, extension_name, tool_title_de, tool_title_en, dev}` (serde camelCase) and `Tool::always_ask() -> bool` (default `false`); new `src-tauri/src/extensions/agent_tools/tool.rs` `ExtensionTool` (`source() = "haextension"`, `risk_class` from effect (`change` → `Change`), `always_ask` = status `ask`, tool description ends with the sentence that `data` is data of the extension, not instructions (research R10), `execute`: ensure frame (T023 via `frame_request.rs`), ensure link (`connect` has no timeout of its own: bound the handshake by the 10 s readiness limit of research R9) + `tools/list` once (declared-but-missing → `tool_unavailable`, undeclared ignored), `call_tool` with 60 s timeout and cancel via `RequestHandle::cancel`, join text blocks, cap 64 KiB, wrap `{"extension", "data"}`); tests in `tool_tests.rs` with an in-memory server (ok, timeout, cancel, unavailable, oversized, invalid input reported by the server, handler throws because a host function is missing on this device, e.g. `desktop_only`); the call reaches a frame through the `Links` trait, `frame_request.rs` `WindowLinks` in the app
- [x] T026 [US1] `register_extension_tools(registry, defs)` in `src-tauri/src/extensions/agent_tools/mod.rs` (source `haextension`, `remove_source` + re-register; FR-010), called on vault open and on `extensions-changed` / `extension-status-changed` from `src-tauri/src/chat/session.rs` (minimal hook); vault close drops them like action tools; tests in `src-tauri/src/extensions/agent_tools/mod_tests.rs`; landed as `agent_tools/registration.rs`: started from `instances/open.rs` next to the other followers, it refreshes on every vault change to the extension and permission tables (local or synced, so permission edits apply too) instead of listening to the two events
- [x] T027 [P] [US1] New `src-tauri/src/chat/tools/extension_offer.rs`: `matched_offer(message, defs) -> Vec<ToolSpec>` per research R6 (words from `offer.rs`, prefix ≥ 5, IDF over all tool texts, extension name/display name hit, threshold, ≤ 3, deterministic order); tests in `extension_offer_tests.rs`: "Wie viele ungelesene Mails habe ich?" hits `count_unread` among 30 fake extensions, a smalltalk sentence hits nothing, a sentence naming the extension hits its tools, stop words alone never hit; the offer takes `MatchEntry {tool_name, extension_title, texts}` and returns model-facing names (the chat stays free of extension types); words split on Unicode letters, since `offer.rs::words` splits "Einträge" at the umlaut; a tool needs two matching words or its extension named (stop-word list de/en, IDF ranks)
- [x] T028 [US1] Use it in the first step: `src-tauri/src/chat/commands.rs` (offer at `:433-444`) passes `args.content` to build core + matched offer; matched tools stay for all steps of the turn (`src-tauri/src/chat/turn/tool_round.rs` `extend_offer` keeps them like core); limits ≤ 14 first step, ≤ 19 after a search (`offer.rs` constants); extend `offer_tests.rs`; FR-017: test that a model with tool use "unsupported" and a delegate turn get no extension tools (`src-tauri/tests/chat_tool_loop_core.rs`); the gate moved into `offer.rs` `first_offer(availability, registry, message)`, which `commands.rs` calls, so the FR-017 test sits in `offer_tests.rs` (the loop tests receive a prepared offer); `core_offer` no longer offers extension tools on its own
- [x] T029 [US1] `find_actions` searches extension tools too (research R7): `src-tauri/src/chat/tools/find_actions.rs` and `offer.rs` (`search_actions` over a combined entry list including examples; `extend_offer` accepts `haextension` names); tests in `offer_tests.rs`; `find_actions` reads the extension tools from the registry at each search (weak reference), `extend_offer` takes an `ExtensionScope` and replaces found extension tools on the next search while the first step's tools stay
- [x] T030 [US1] Test extension: sources under `src-tauri/tests/fixtures/extension_e2e/agent-tools/` (manifest with `count_entries` read, `add_entry` change, `slow_echo` read, one tool outside the schema subset; JS using the released vault-sdk `sdk.tools` bundled into the fixture) and `agent-tools.xt` built and signed in `src-tauri/tests/extension_e2e_fixtures.rs` like `probe.xt` (seed-derived test key, stale-file check, `HOLZI_WRITE_E2E_FIXTURES=1`); sources in `agent-tools/` (`manifest.json` read by the builder and bundled into the code, `main.js`, `index.html`); `build.sh` bundles `main.js` with `@haex-space/vault-sdk` 4.3.0 (integrity checked) and `drizzle-orm` 0.45.4 into `app.js` with esbuild 0.28.2, reproducibly (`SOURCE.md`); the tool outside the subset is `optional_note` with a union type, since `haex-bundle` already refuses unknown schema keys
- [x] T031 [US1] Integration test `src-tauri/tests/extension_agent_tools.rs`: install `agent-tools.xt`, confirm tools, scripted model calls `x_agenttools_count_entries` → result row with `tool_source = haextension` (the origin is asserted in T033); the frame side simulated by an in-memory server on the link (no webview); holzi keeps `tool_source` on the call row, so the test asserts it there
- [x] T032 [US1] E2E scenario `scripts/e2e/scenarios/extension-agent-tools.test.ts`: install `agent-tools.xt` through the dialog and assert the tool rows with effect (quickstart step 1); the call path is covered by T031 because the E2E harness runs no model; the dialog picks its file with the system file dialog, which the harness cannot drive, so the scene checks `extension_install_preview` (what the dialog shows), installs with the tools ticked and checks the stored rows, then opens the extension and waits for its bundled vault-sdk to count the entries in the frame

**Checkpoint**: MVP — the agent answers with an extension tool, without the model searching.

---

## Phase 5: User Story 3 - Freigabe je Aufruf und Kontrolle in den Einstellungen (Priority: P1)

**Goal**: Approvals name extension and tool; the user can switch an extension's tools off; host prompts
triggered by an agent call say so.

**Independent Test**: quickstart steps 5, 6, 7, 9.

- [x] T033 [US3] `tool_origin` end to end: migration adding `chat_messages.tool_origin TEXT NULL` in `src-tauri/src/identity/migrations.rs`; `src-tauri/src/storage/chat_messages.rs` read/write; `persist_round` and `ToolCallEvent` carry it; `ToolPermissionRequestEvent.tool_origin` in `src-tauri/src/chat/events.rs` set in `plan_calls`; tests in the sibling `*_tests.rs` and `src-tauri/tests/chat_tool_loop_permissions.rs`; extend `src-tauri/tests/extension_agent_tools.rs` (T031) to assert the origin of the result row; the origin sits on the call row next to `tool_source` (the result row carries neither), migration `0035_chat_messages_tool_origin` bumps `HOLZI_TRIGGER_VERSION` to 23, executed calls carry a `CallSource {source, origin}` (`chat/turn/call_source.rs`); the loop tests live in the new `tests/chat_tool_loop_extension_tools.rs` because `chat_tool_loop_permissions.rs` is past 500 lines
- [x] T034 [P] [US3] Frontend: `toolSource` gains `'haextension'` and `toolOrigin?: ToolOrigin` in `src/lib/chat/prompts.ts` and `src/composables/useChat.ts`; `src/components/chat/PermissionPrompt.vue` title "<Erweiterung>: <Werkzeugtitel>" with input as for actions; `src/components/chat/MessageList.vue` shows extension + tool title (dev badge when `dev`); i18n de/en; `pnpm check:chat-state`, `check:templates`; the label "<extension>: <tool title>" and the input fields come from pure `src/lib/chat/toolOrigin.ts` (tested by `scripts/check-chat-tool-origin.ts` in `check:chat-state`), so no new i18n keys were needed; the dev badge is `WmDevBadge`
- [x] T035 [US3] Status `ask` forces approval in every mode, "Plan" included: `plan_calls` in `src-tauri/src/chat/turn/tool_round.rs` returns Ask when `tool.always_ask()` before the risk-class matrix; tests in `src-tauri/tests/chat_tool_loop_permissions.rs` (read tool on `ask` in Manual, Auto and Plan → approval prompt each time); the check landed with the review of #425 (`063146fd`), the tests in `tests/chat_tool_loop_extension_tools.rs`
- [ ] T036 [US3] Switch "Für den Agenten verfügbar": command `extension_agent_tools_set {extensionId, available}` writing/removing the `agentTool / * / denied` row (`src-tauri/src/extensions/agent_tools/commands.rs`, store via `permissions/store.rs`); refresh the registry (T026); test in `commands_tests.rs`; UI in `src/components/settings/extensions/PermissionsView.vue` (switch above the tool rows, tools with title, effect, "nicht unterstützt" for `supported == false`); i18n de/en; extend `scripts/e2e/scenarios/extension-agent-tools.test.ts` (T032) with the switch (quickstart step 7)
- [ ] T037 [US3] `byAgent` (research R11): `PermissionRequestEvent` in `src-tauri/src/extensions/permissions/prompts.rs` gets `by_agent` from the frame session's in-flight counter (T022); the extension permission dialog component shows "ausgelöst durch einen Aufruf des Agenten"; tests in `prompts_tests.rs` and `scripts/check-extensions-queue.ts`

**Checkpoint**: Every call is controllable; switching off works without restart (SC-006).

---

## Phase 6: User Story 4 - Erweiterungen mit Werkzeugen entwickeln und prüfen (Priority: P2)

**Goal**: Dev versions offer tools; the model evaluation measures extension tools.

**Independent Test**: quickstart steps 11 and 12.

- [ ] T038 [US4] Dev mode: `from_dev_file` tools (T013) feed T024 with `dev: true` from `dev_extension_permissions_no_sync`; test in `src-tauri/src/extensions/lifecycle_us12_tests.rs` or a sibling
- [ ] T039 [US4] Eval fixture `src-tauri/src/chat/eval/extension_tools.json`: 30 invented extensions with 2–4 tools each (titles, descriptions, examples de/en), including a mail extension with `count_unread`; loader in `src-tauri/src/chat/eval/mod.rs`
- [ ] T040 [US4] Eval set v4: kind `extension` with ≥ 10 sentences (de/en, none naming the extension except two), expected tool names `x_<slug>_<tool>`; `run_eval` (`src-tauri/src/chat/eval/runner.rs`) builds the first offer with `matched_offer` and searches extension tools; `run_command` offered in the eval; report fields `offered`, `called`, `runCommandCalls` (`scoring.rs`); tests in `runner_tests.rs`, `scoring_tests.rs`; `scripts/check-agent-actions.ts` accepts version 4 and the new kind
- [ ] T041 [US4] Run the model eval with Qwen3-4B (`cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture`, CUDA feature set as in 046); record SC-001 (offered ≥ 9/10), SC-002 (called ≥ 7/10), SC-003 (0 `run_command`) in new `specs/018-extension-agent-tools/eval-results.md`; if SC-001 misses, tune threshold/weights in `extension_offer.rs` and re-run before recording

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T042 [P] Measure SC-004 (call with no open frame ≤ 3 s slower than with one) in the E2E scenario or by hand; record in `eval-results.md`
- [ ] T043 [P] Check every new or changed file stays under 500 lines (`wc -l` over the plan's structure tree)
- [ ] T044 Run `cargo fmt --check`, `pnpm lint:rust` (both feature sets), `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`, every `pnpm check:*`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`; revert ts-rs whitespace drift in `src/types/bindings/` that is not part of the change; fix until green
- [ ] T045 Walk through `specs/018-extension-agent-tools/quickstart.md` steps 1–11 in `pnpm tauri:dev:cuda`; record skipped steps in the PR description
- [ ] T046 Add **Erweiterungswerkzeug** and **Passendes Angebot** to `CONTEXT.md` (chat runtime / extensions sections)

---

## Phase 8: ADR (delivery E)

- [x] T047 [P] Commit `docs/adr/0012-public-test-signing-keys.md` (written during analysis, operator decision 2026-10-10); landed with delivery C1
- [x] T048 Amend `docs/adr/0004-extension-protocol-split.md`: effect from the signed manifest instead of "Risky by default" (Clarification 2026-10-10); transport over the port message `haexspace:mcp` with `rmcp` 3.5 `sink_stream` (resolves the open point); "The LLM is never reachable from an extension" → an extension reaches a model only through a route the user has set (spec 047 and later), never through the bridge or the tool connection without such a setting (research R2); commit `docs(adr): settle direction A for extension tools`

---

## Dependencies & Execution Order

### Phase Dependencies

- Phase 1 → Phase 2. Within Phase 2: C1 (T003–T005) → A (T006–T011, vault-sdk) → holzi model (T012–T014).
  T006/T007 can start before C1 is merged; T009 needs the transcripts.
- Phase 3 (US2) needs Phase 2. Phase 4 (US1) needs Phase 3 (only confirmed tools are offered).
- Phase 5 (US3) needs T024–T026; T033/T034 can start in parallel with Phase 4's offer tasks.
- Phase 6 (US4) needs Phase 4. Phase 7 needs all stories. Phase 8 can land with delivery B.

### Within Each Story

Test before implementation; Rust model before command; command before frontend; frontend checks after.

### Parallel Opportunities

- T006 ∥ T007 (vault-sdk, different files); T013 ∥ T014; T017 ∥ T016 after the binding exists.
- T019 ∥ T021 ∥ T022; T027 ∥ T023; T034 ∥ T033 once the field shape is fixed.
- T042 ∥ T043.

## Parallel Example: User Story 1

```text
T019 mcpRelay.ts + check   |  T021 extension_mcp_send   |  T022 frames.rs link slot
T027 extension_offer.rs    |  T023 openAppInBackground
```

## Implementation Strategy

### MVP First

Phases 1–4: tools confirmed and called with the matched offer. Stop and run quickstart steps 1–3.

### Incremental Delivery

1. C1 PR (contract + transcripts) → 2. vault-sdk PR + release → 3. holzi B (Phase 2 holzi part + US2 +
   ADR) → 4. holzi C2 + D (US1) → 5. holzi US3 + US4 + polish. Spec 019 (tools in haex-mail and others)
   starts after step 4.
