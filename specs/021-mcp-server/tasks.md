---
description: 'Task list for spec 021-mcp-server'
---

# Tasks: holzi als MCP-Server für externe Agenten

**Input**: Design documents from `/specs/021-mcp-server/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/mcp-surface.md](./contracts/mcp-surface.md),
[contracts/agent-helper.md](./contracts/agent-helper.md),
[contracts/http-auth.md](./contracts/http-auth.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md), [quickstart.md](./quickstart.md),
[ADR-0005](../../docs/adr/0005-mcp-server-transport-and-agent-identity.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic:

- Rust unit tests go in sibling `*_tests.rs` files, registered with
  `#[cfg(test)] #[path = "…_tests.rs"] mod tests;`.
- Rust integration tests go in `src-tauri/tests/` (`mcp_server.rs`, `mcp_server_contract.rs`) and
  drive the server with an `rmcp` client over a socket in a temp directory, with an injected clock.
- Catalog rules go in `scripts/check-mcp-actions.ts` (`pnpm check:mcp-actions`).
- e2e scenarios go in `scripts/e2e/scenarios/`.
- Tests come first inside each story and fail before the code exists.
- Real Claude Code and Codex are manual (quickstart M1–M10).

**Organization**: Grouped by user story and shipped in the plan's deliveries. Each delivery is one
PR from its own worktree off `main`.

| Delivery | Phases     | Stories                                                     |
| -------- | ---------- | ----------------------------------------------------------- |
| PR A     | –          | spec, plan, ADR-0005 (proposed), this file                  |
| PR B     | Phases 1–5 | Setup, Foundational, US1, US2, US8 (program way)            |
| PR C     | Phase 6    | US3, plus the call log of US7                               |
| PR D     | Phase 7    | US7 (management, revocation, expiry)                        |
| PR E     | Phases 8–9 | US4, US5 (extensions, files, contacts, passwords, commands) |
| PR F     | Phase 10   | US6 (delegates)                                             |
| PR G     | Phase 11   | HTTP way with OAuth (US1, US8 on Android/iOS)               |
| –        | Phase 12   | Polish, manual checks                                       |

Priorities: P1 US1, US2, US3, US8; P2 US4, US5, US6, US7.

**Shipping notes**:

- PR B lands the migration with all three tables, so C–G add no migration.
- The access is off by default (FR-001). PR B has no call prompts yet, so the approval mode
  "Manuell" (FR-021) cannot be honoured. PR B therefore keeps the on/off switch out of the
  settings page (the page shows only the snippets, marked "bald verfügbar"); only the integration
  tests switch the access on. PR C adds prompts and shows the switch. In addition, `grants::offer`
  withholds writing and destructive actions until PR C (T050).
- ADR-0005 changes from "proposed" to "accepted" in PR B.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US8)

**Conventions for every task**:

- **File size**: Files stay ≤ 500 lines. `claude.rs` (653), `codex.rs` (585), `lib.rs` (753),
  `migrations.rs` (748) and `check-agent-actions.ts` (527) get only the smallest hook; new code
  goes into new files (plan, Constraints).
- **Test code**: lives in separate files.
- **Rust**:
  - No `unwrap`/`expect` on input.
  - No localized text in Rust.
  - No credential, secret or tool result in a log line, error string, event or `Debug` output.
  - The caller comes only from the entrance, never from request content (ADR-0005).
- **Commits**: Conventional Commits without agent attribution.
- **Running Rust**: only through `nix develop --command scripts/with-nix-host-bridge.sh cargo …`
  with `CARGO_BUILD_JOBS=4` and `-j 4`, one test target at a time.
- **Before each commit**:
  - Run `pnpm generate:ts-types` after every Rust type change and commit `src/types/bindings/`.
  - Run `pnpm exec prettier --write` on touched Markdown/JSON.
- **New named artifacts**: Before the first new function, module, component or store of a
  delivery, run `graphify query` for it. Record reused names in the PR description.

---

## Phase 1: Setup (PR B)

**Purpose**: Dependencies, module skeleton, catalog scope.

- [x] T001 Enable the `rmcp` features `server`, `transport-async-rw` and `macros` for the main crate (keep `=3.5.0`) in src-tauri/Cargo.toml; confirm with `cargo tree -e features -i rmcp` that no new crate enters the lockfile apart from rmcp's own feature deps
- [x] T002 Create the module `src-tauri/src/mcp_server/mod.rs` with empty submodules `endpoint`, `hello`, `helper`, `server`, `grants`, `dispatch`, `registry`, `prompts`, `log`, `commands`, and register `pub mod mcp_server;` in src-tauri/src/lib.rs (done: `log.rs` comes with PR C, where it is first written; `connection.rs` and `session.rs` were added for the greeting, the pump and the rmcp handler)
- [x] T003 [P] Add the scope `passwords.secret` with a description ("Read one secret of an entry after the user confirms each time; never the built-in agent.") to `ACTION_SCOPE_IDS` in src/lib/actions/scopes.ts and its localized names under `actions.scopes.passwords.secret` in src/i18n/locales/de.json and src/i18n/locales/en.json
- [x] T004 [P] Create `scripts/check-mcp-actions.ts` (empty rule set, runs like `check-agent-actions.ts`) and add the script `check:mcp-actions` to package.json; wire it into the same CI job as `check:agent-actions` in .github/workflows/

---

## Phase 2: Foundational (PR B, blocking)

**Purpose**: Storage, caller propagation, one tool definition, endpoint and lifecycle. Nothing of
any story works before this phase.

### Tests first

- [x] T005 [P] Migration test in src-tauri/src/identity/migrations_mcp_agents_tests.rs: the three tables exist with the columns of data-model.md; `haex_mcp_agents` and `haex_mcp_agent_grants` are CRDT-tracked (listed in `SYNCED_TABLES`), `haex_mcp_agent_log_no_sync` is not; deleting a `known_devices` row cascades
- [x] T006 [P] Registry tests in src-tauri/src/mcp_server/registry_tests.rs: register stores only the `SHA-256` of a 32-byte credential (base64url); a lookup with the right credential returns the agent, a wrong one returns none; rows of another `vault_device_uuid` are never returned; status values exactly `registered`, `expired`, `blocked`; `permission_mode` defaults to `manual`; `client_name` is cut to "≤ 64 Zeichen"
- [x] T007 [P] Hello tests in src-tauri/src/mcp_server/hello_tests.rs: parse the greeting of contracts/agent-helper.md (`hello`, `slug` matching `[a-z0-9-]{1,32}`, `token`, `delegate`), reject greetings over 4 KiB, a bad slug or a missing field; control lines are JSON objects with the single key `holzi_control` and are never valid JSON-RPC
- [x] T008 [P] Endpoint tests in src-tauri/src/mcp_server/endpoint_tests.rs: the Unix socket path is `mcp-<vault-id>.sock` in a directory with mode `0700`, the path stays ≤ 104 bytes, a peer with a different UID is refused (inject the UID check); on Windows the pipe name is `\\.\pipe\holzi-mcp-<vault-id>` and is created with `first_pipe_instance(true)` (cfg(windows) test)
- [x] T009 [P] Action bridge test in src-tauri/src/chat/tools/action_bridge_tests.rs: `action-call-request` carries `caller` `{kind:'builtinAgent'}` for the chat and `{kind:'externalAgent', agentId}` for an MCP call
- [x] T010 [P] Action registration test in src-tauri/src/chat/action_commands_tests.rs (new file if missing): `set_agent_actions` with all `agentCallable` actions registers for the built-in agent only those with `builtinAgentCallable !== false` and keeps the full list for the MCP server (research R4)

### Implementation

- [x] T011 Write migration `0045_mcp_agents` in src-tauri/src/identity/migrations_mcp_agents.rs with the tables `haex_mcp_agents`, `haex_mcp_agent_grants`, `haex_mcp_agent_log_no_sync` exactly as in data-model.md (FK `vault_device_uuid → known_devices ON DELETE CASCADE` on the first two, no UNIQUE constraint), register it in src-tauri/src/identity/migrations.rs and raise `HOLZI_TRIGGER_VERSION` 28 → 29
- [x] T012 Implement `registry.rs` in src-tauri/src/mcp_server/registry.rs: `register`, `find_by_credential`, `block`, `touch`, credential generation with `getrandom` (32 bytes, base64url) and `SHA-256` hashing; device-scoped reads via the own `vault_device_uuid`; blocking DB work through `VaultGate`/`spawn_blocking`. Revoke/remove is deferred.
- [x] T013 [P] Implement `hello.rs` in src-tauri/src/mcp_server/hello.rs: greeting and reply types (`accepted`, `pending`, `rejected` with `reason` `blocked`/`registration_busy`/`disabled`) and the control lines `{"holzi_control":{"token":…}}` and `{"holzi_control":{"closing":…}}` per contracts/agent-helper.md
- [x] T014 Implement `endpoint.rs` in src-tauri/src/mcp_server/endpoint.rs: listener on a Unix socket (Linux `$XDG_RUNTIME_DIR/holzi/`, otherwise the user temp dir, dir `0700`, `SO_PEERCRED`/`getpeereid` UID check) and on Windows a named pipe; one accepted connection yields an `AsyncRead + AsyncWrite` stream; remove a stale socket file only after a failed connect to it
- [x] T015 Extend the action bridge with the caller: add `caller` to the `action-call-request` payload in src-tauri/src/chat/tools/action_bridge.rs and pass it from `ActionTool` (builtin) and from `mcp_server::dispatch` (external)
- [x] T016 Handle the caller in the frontend: read `caller` from `action-call-request` and pass it to `runAction` in src/lib/actions/agentTools.ts; `runAction` in src/lib/actions/runner.ts keeps refusing non-`agentCallable` and guardrail actions for every agent caller (done in src/composables/useAgentActions.ts, which listens to `action-call-request`)
- [x] T017 Send the full list: `agentTools.ts` pushes every `agentCallable` action with `builtinAgentCallable` to `set_agent_actions`; src-tauri/src/chat/action_commands.rs registers only built-in-callable ones in the chat `ToolRegistry` and stores the full list in a shared `ActionCatalog` the MCP server reads (src-tauri/src/mcp_server/mod.rs) (done: `listPushedActions` in src/lib/actions/agentTools.ts; `listAgentActions` keeps the built-in share for the eval snapshot; the catalog lives in src-tauri/src/chat/tools/action_catalog.rs)
- [x] T018 Implement lifecycle in src-tauri/src/mcp_server/mod.rs: `McpServer` state managed by Tauri; start when the vault opens and the device setting `mcp.enabled` is true; stop through the `VaultGate` token on close/lock and when the setting turns off (end connections, `PromptPark::cancel_all`, remove the socket); wire start/stop in src-tauri/src/lib.rs next to `browser_bridge`
- [x] T019 Add the device settings `mcp.enabled` (default `false`) and `mcp.http_port` (unset) as guardrail device settings in the settings store used by `browser_bridge/settings.rs` (src-tauri/src/mcp_server/settings.rs, new file) (done for `mcp.enabled`; `mcp.http_port` moves to PR G with the HTTP way)

**Checkpoint**: Tables, endpoint, caller and the full action list exist; no agent can connect yet.

---

## Phase 3: User Story 1 - Claude Code mit holzi verbinden und registrieren (Priority: P1) 🎯 MVP (PR B)

**Goal**: An agent started by the setup snippet connects, triggers the registration dialog, gets
its credential and the read tools of "holzi bedienen".

**Independent Test**: Integration test with an rmcp client through the helper's protocol; manual
M1–M2 with Claude Code.

### Tests first

- [x] T020 [P] [US1] Integration test in src-tauri/tests/mcp_server.rs: unknown agent → `initialize` succeeds with zero tools and the waiting `instructions`; answering the registration with `register` sends the control line with the credential, then `notifications/tools/list_changed`, then `tools/list` returns the read actions of "holzi bedienen" (FR-006, FR-009, US1 scenarios 1–2)
- [x] T021 [P] [US1] Integration test in src-tauri/tests/mcp_server.rs: reconnect with the stored credential → `accepted`, no dialog (US1 scenario 3); `deny` with `remember` → row `blocked`, next connect `rejected`/`blocked` without dialog; `deny` without `remember` → refused until the vault closes (US1 scenario 4)
- [x] T022 [P] [US1] Helper tests in src-tauri/src/mcp_server/helper_tests.rs: no endpoint → answers the first `initialize` with JSON-RPC error `-32000` "holzi ist nicht offen" and exits with code 2 (FR-002 clarification); `rejected` → exit 3; control line `token` writes the credential file with mode `0600` and is not forwarded; `closing: revoked` deletes the file; several endpoints without `--vault` → error naming the choice
- [x] T023 [P] [US1] Prompt tests in src-tauri/src/mcp_server/prompts_tests.rs: registration prompt times out after 5 minutes with "deny"; at most one open registration per slug and peer, at most 3 in total, the 4th unknown agent gets `registration_busy` (FR-011)

### Implementation

- [x] T024 [US1] Implement `helper.rs` in src-tauri/src/mcp_server/helper.rs: `--mcp-agent <slug> [--vault <id>] [--delegate-token <c>]`, endpoint discovery, greeting, line relay stdin↔endpoint, interception of `holzi_control` lines, credential file `<holzi config dir>/mcp-agents/<vault-id>/<slug>.token` (`0600`), exit codes 0–4 of contracts/agent-helper.md; file I/O via `spawn_blocking`
- [x] T025 [US1] Branch for `--mcp-agent` at the top of `run()` in src-tauri/src/lib.rs that calls `mcp_server::helper::run` on a current-thread tokio runtime without starting Tauri (pattern of `--internal-cli-delegate-approval-bridge`)
- [x] T026 [US1] Implement `prompts.rs` in src-tauri/src/mcp_server/prompts.rs: a `PromptPark` (src-tauri/src/prompt_park.rs) for registrations with a 5-minute timeout, the limits of FR-011, event `mcp-registration-request` with slug, reported name (marked as the agent's claim), transport, preset rights and deadline; in-memory temporary denials until the vault closes
- [x] T027 [US1] Implement `server.rs` in src-tauri/src/mcp_server/server.rs: an rmcp `ServerHandler` per connection on the endpoint stream; `serverInfo` `holzi` + app version; capabilities `tools.listChanged`; `instructions` for registered and waiting agents (contracts/mcp-surface.md); `list_tools` from `grants::offer`; record `clientInfo.name`/`version` (cut to 64 chars); on registration send the token control line, then `tools/list_changed`
- [x] T028 [US1] Implement `commands.rs` in src-tauri/src/mcp_server/commands.rs: `mcp_status`, `mcp_registration_answer` (`requestId`, `decision`, `remember`, `grants`, `permissionMode`), `mcp_setup_snippet` (`client` `claude-code`/`codex`/`other`, `transport`) per contracts/tauri-commands.md; register them in src-tauri/src/lib.rs; snippet uses `current_exe` or `$APPIMAGE` and never contains a credential (FR-005)
- [x] T029 [P] [US1] Catalog action `settings.agents.mcp.enable` (scope `guardrails`, effect `write`, `agentCallable: false`) in the new file src/lib/actions/settingsMcpAgentActions.ts, registered in src/lib/actions/catalog.ts, with handler in src/stores/settingsActionHandlers.ts or a new src/stores/mcpAgentActionHandlers.ts (done: registered in src/lib/actions/catalog.ts because settingsActions.ts is at its size limit)
- [x] T030 [P] [US1] Store src/stores/mcpAgents.ts: status, agent list, listening to `mcp-registration-request`, `mcp-prompt-closed`, `mcp-agents-changed`, calling the commands of T028
- [x] T031 [US1] Registration dialog src/components/agents/RegistrationDialog.vue: reported name with the label "Angabe des Agenten", transport, rights groups preset to "holzi bedienen: lesen" for all non-locked scopes and everything else off (FR-009), approval mode Manuell, "Ablehnung merken", Registrieren/Ablehnen; mount it in the app shell next to the browser bridge dialogs (done in src/components/agents/RegistrationDialog.vue, mounted by src/components/agents/Prompts.vue in src/components/wm/Desktop.vue)
- [x] T032 [US1] Settings page "KI → Agenten": add `subView('ai.agents', 'ai/agents', 'ai', …)` in src/lib/settings/registry.ts and src/components/settings/agents/AgentsView.vue with status and the setup snippets for Claude Code, Codex and "anderer Agent" with a copy button (FR-005); the on/off switch (via T029) is built but hidden until PR C (shipping note); COSMIC/GNOME layout with `Group`/`Row`/`OptionRow`
- [x] T033 [P] [US1] German and English texts for the dialog, page and snippets in src/i18n/locales/de.json and src/i18n/locales/en.json (no `<…>` in messages; vue-i18n refuses them)

**Checkpoint**: Claude Code registers and lists the read tools (quickstart M1–M2 without prompts).

---

## Phase 4: User Story 2 - Ein Agent darf nur, was ihm erlaubt ist (Priority: P1) (PR B)

**Goal**: Tool list and every call follow the agent's rights; guardrails are never reachable.

**Independent Test**: Contract test over all scopes and effects; manual M3.

### Tests first

- [x] T034 [P] [US2] Grants unit tests in src-tauri/src/mcp_server/grants_tests.rs: for each scope of "holzi bedienen" and each level (`none`, `read`, `write`) the offer contains exactly the matching effects (`read` → read only; `write` → read, write, destructive); `guardrails` never appears and is refused when stored as a grant key; actions with `builtinAgentCallable: false` are offered to external agents (FR-012, FR-013, FR-016)
- [x] T035 [P] [US2] Contract test src-tauri/tests/mcp_server_contract.rs: one case per rights group of research R3 (operate, files, passwords, extensions, commands) and a case asserting that no `guardrails` action and no `settings.agents.mcp.*` action is ever listed or callable
- [x] T036 [P] [US2] Integration test in src-tauri/tests/mcp_server.rs: calling a tool that is not in the agent's list returns `isError` with `code` `not_allowed` (never "unknown tool"), the action is not executed, and the message does not reveal whether the target exists (FR-014, contracts/mcp-surface.md)
- [x] T037 [P] [US2] Rule in scripts/check-mcp-actions.ts: every `settings.agents.mcp.*` action has scope `guardrails` and `agentCallable: false`

### Implementation

- [x] T038 [US2] Implement `grants.rs` in src-tauri/src/mcp_server/grants.rs: load an agent's rights from `haex_mcp_agent_grants` (`grant_group` `operate`/`extension`/`passwords`/`commands`, `grant_key`, `level`, `scope_tags`), `offer(agent, catalog) -> Vec<Offer>` and `check(agent, tool) -> Result<Offer, NotAllowed>` from the same table of research R3; until PR C, writing and destructive offers are withheld (shipping note)
- [x] T039 [US2] Implement `dispatch.rs` in src-tauri/src/mcp_server/dispatch.rs: `call_tool` re-checks with `grants::check` before anything else, validates input against the schema (`invalid_input` with `field`), runs actions through the `ActionBridge` (T015) or `NativeActionTool` with `Caller::ExternalAgent { id }`, maps `ActionReply` to MCP results (`structuredContent`, error codes `failed`, `needs_choice` with `field`/`options`, `unavailable`) per contracts/mcp-surface.md
- [x] T040 [US2] Refuse writes of a `guardrails` key or of any grant by an agent caller in the registry write path (src-tauri/src/mcp_server/registry.rs), so no agent can change its own or another agent's rights (FR-016)

**Checkpoint**: Contract test green; M3 passes.

---

## Phase 5: User Story 8 - Fremde Programme kommen nicht durch (Priority: P1) (PR B, program way)

**Goal**: Only processes of the same OS user reach the endpoint; bad credentials and floods get
nothing. The HTTP part follows in Phase 11.

**Independent Test**: Integration tests with wrong credentials, revoked credentials and 100
connects in a row.

- [x] T041 [P] [US8] Integration test in src-tauri/tests/mcp_server.rs: an invalid or unknown credential gets no tools and no data; 100 connects of the same unknown slug in a row produce exactly one registration prompt (US8 scenarios 3–4)
- [x] T042 [P] [US8] Unit test in src-tauri/src/mcp_server/endpoint_tests.rs: the endpoint is never bound to a network interface (only a socket path or a pipe name)
- [x] T043 [US8] Enforce greeting limits in src-tauri/src/mcp_server/server.rs: greeting ≤ 4 KiB within 5 s, one MCP message ≤ 1 MiB, otherwise close the connection without a prompt
- [x] T044 [US8] Mark ADR-0005 as accepted in docs/adr/0005-mcp-server-transport-and-agent-identity.md (status line and date)

**Checkpoint (end of PR B)**: The integration tests register an agent, list and call read tools
and refuse everything else; the access is not yet switchable in the UI.

---

## Phase 6: User Story 3 - Rückfrage je Aufruf nach Freigabestufe (Priority: P1) (PR C)

**Goal**: Each agent has its own approval mode; prompts show in the window; every call lands in
the call log.

**Independent Test**: Same test agent in Manuell, Auto and Plan with one read, write and
destructive action each; manual M2, M4, M8 (log part).

### Tests first

- [x] T045 [P] [US3] Unit tests in src-tauri/src/mcp_server/prompts_tests.rs: with mode `manual` every call asks; `auto` runs read and write and asks for destructive; `plan` runs read and refuses others with `plan_mode` without asking; a prompt ends after 5 minutes with `no_answer`; `cancel_all` ends open prompts as `unavailable` (FR-021, FR-023) (done: the decision table and the secret rule as unit tests of `call_gate`; the 5-minute end and the end on closing run as integration tests in src-tauri/tests/mcp_server_calls.rs, where a prompt the server stops ends the call as `unavailable`)
- [x] T046 [P] [US3] Log tests in src-tauri/src/mcp_server/log_tests.rs: an entry holds time, tool, effect, target, input, outcome (`ok`, `denied_rights`, `denied_user`, `denied_plan`, `no_answer`, `error`, `unavailable`, `too_many`) and `asked`; input fields with `"x-holzi-secret": true` and credential-bearing values in raw command arguments are written as `•••`; values over 200 characters are cut; results are never written; entries older than 30 days and beyond 10 000 per agent and device are removed on write and read (FR-028, research R10) (done in src-tauri/src/mcp_server/agent_log_tests.rs; the module is `agent_log.rs`, because a module `log` would hide the `log` crate inside `mcp_server`, and the redaction lives in `redact.rs`)
- [x] T047 [P] [US3] Integration test in src-tauri/tests/mcp_server.rs: more than 4 concurrent calls of one agent wait up to 30 s and then get `too_many_calls` (FR-029, research R11) (done in src-tauri/tests/mcp_server_calls.rs with a shortened wait; the slot is held while a prompt waits)

### Implementation

- [x] T048 [US3] Call prompts in src-tauri/src/mcp_server/prompts.rs: second `PromptPark` (5 minutes) for calls; decision through `chat::tools::permission::decide(mode, risk)` with the agent's `permission_mode` (add `FromStr` for `PermissionMode` in src-tauri/src/chat/tools/permission.rs); event `mcp-call-request` with reported name, localized action name, effect, target and input without secrets; `mcp_call_answer` command in src-tauri/src/mcp_server/commands.rs (done: `call_gate` in prompts.rs takes `AgentMode::permission_mode()` from PR B instead of a `FromStr`; the event carries the catalog titles in German and English and the window picks one)
- [x] T049 [US3] System notification when the window is not focused, via `tauri-plugin-notification`, in src-tauri/src/mcp_server/prompts.rs (FR-023) (done in the window, like the registration dialog of PR B: holzi comes to the front and, when it stays in the back, notifies through `browser_bridge_notify`)
- [x] T050 [US3] Lift the PR B restrictions: offer writing and destructive actions for `write` levels in src-tauri/src/mcp_server/grants.rs, and show the on/off switch in src/components/settings/agents/AgentsView.vue now that prompts exist
- [x] T051 [US3] Concurrency limit (4 per agent, 30 s wait) in src-tauri/src/mcp_server/dispatch.rs; actions keep running one after the other through the existing `ActionBridge` lock
- [x] T052 [US3] Implement `log.rs` in src-tauri/src/mcp_server/log.rs and write one entry per call from `dispatch.rs`, including refused calls; add `mcp_agent_log` (`agentId`, `before?`, `limit` ≤ 200) to src-tauri/src/mcp_server/commands.rs (done in src-tauri/src/mcp_server/agent_log.rs)
- [x] T053 [US3] Call prompt dialog src/components/agents/CallRequestDialog.vue: reported name marked as the agent's claim, action, target, inputs, Erlauben/Ablehnen, queue oldest first (FR-022); mount it next to RegistrationDialog (done; the registration dialog goes first when both wait)
- [x] T054 [P] [US3] Approval mode selector in the registration dialog (src/components/agents/RegistrationDialog.vue) and texts in src/i18n/locales/de.json and src/i18n/locales/en.json (done in PR B; PR C adds the texts of the call prompt)
- [x] T055 [P] [US3] e2e scenario scripts/e2e/scenarios/mcp-agent.test.ts: the e2e binary starts the helper as a fake agent, registers through the dialog, calls a read action, a write action in Auto, and a destructive action that the test declines (done; on Android the counter case is `android-not-available`)

**Checkpoint (end of PR C)**: Claude Code and Codex register and use holzi with prompts; M1–M4,
the log part of M8 and the program-way part of M10 pass.

---

## Phase 7: User Story 7 - Agenten verwalten, Protokoll und Widerruf (Priority: P2) (PR D)

**Goal**: Overview, change of rights and mode, revocation, unblocking, 90-day expiry, log view.

**Independent Test**: Register two agents, block one; change rights and mode, revoke, unblock;
injected clock for expiry.

### Tests first

- [ ] T056 [P] [US7] Integration test in src-tauri/tests/mcp_server.rs: revoking a connected agent closes its connection after the control line `closing: revoked`, ends waiting prompts and calls with a refusal, invalidates the credential, and the next connect shows a new registration (FR-027); measure < 1 s (SC-004)
- [ ] T057 [P] [US7] Integration test with an injected clock in src-tauri/tests/mcp_server.rs: an agent whose `last_seen_at` is older than 90 days gets status `expired` on connect, the old credential is refused, and the registration prompt carries the previous rights and approval mode prefilled (FR-007, data-model.md states)
- [ ] T058 [P] [US7] Integration test in src-tauri/tests/mcp_server.rs: changing the rights of a connected agent applies from the next call and sends `notifications/tools/list_changed` (FR-017, US2 scenario 4)

### Implementation

- [ ] T059 [US7] Expiry check on connect in src-tauri/src/mcp_server/registry.rs with the transition `registered → expired → registered` of data-model.md; `ponytail:` comment that expiry is checked only on connect (research R9)
- [ ] T060 [US7] Catalog actions `settings.agents.mcp.grants.set`, `settings.agents.mcp.mode.set`, `settings.agents.mcp.revoke`, `settings.agents.mcp.unblock`, `settings.agents.mcp.log.clear` (scope `guardrails`, effects per contracts/tauri-commands.md, `agentCallable: false`) in src/lib/actions/settingsMcpAgentActions.ts with handlers calling new commands in src-tauri/src/mcp_server/commands.rs; revoke also deletes the agent's rows in `agent_file_permissions`
- [ ] T061 [US7] Live updates in src-tauri/src/mcp_server/server.rs: on any rights change send `tools/list_changed` to the connected agent; on revoke send `closing: revoked` and close; emit `mcp-agents-changed`
- [ ] T062 [US7] Commands `mcp_agents_list` and `mcp_agent_get` in src-tauri/src/mcp_server/commands.rs per contracts/tauri-commands.md
- [ ] T063 [US7] Agent list in src/components/settings/agents/AgentsView.vue (reported name, registered at, last connection, connected yes/no, approval mode, rights summary, blocked agents with "Sperre aufheben") and detail page src/components/settings/agents/AgentDetailView.vue (rights per group, approval mode, revoke) registered as `subView` in src/lib/settings/registry.ts; changes apply on selection without a save button (023 FR-021)
- [ ] T064 [US7] Log view src/components/settings/agents/AgentLogView.vue: entries newest first with target and input ("Farbschema → dunkel"), outcome, prompt yes/no, "Protokoll leeren"; paging via `before`
- [ ] T065 [P] [US7] Texts in src/i18n/locales/de.json and src/i18n/locales/en.json

**Checkpoint (end of PR D)**: M8 passes; the core (PR B–D) is usable.

---

## Phase 8: User Story 4 - Werkzeuge von haextensions für externe Agenten (Priority: P2) (PR E)

**Goal**: Extensions granted to an agent appear as its tools; the extension's own setting still
applies.

**Independent Test**: Two test extensions with one read and one write tool each, only one granted.

- [ ] T066 [P] [US4] Integration test in src-tauri/tests/mcp_server.rs with the agent-tools test fixture (src-tauri/tests/fixtures/extension_e2e/agent-tools/): only tools of the granted extension are listed with the same name `x_<slug>_<tool>`, description and schema as for the built-in agent; a newly installed extension is missing until granted; uninstalling sends `tools/list_changed` and a later call gets `unavailable` (US4)
- [ ] T067 [P] [US4] Unit test in src-tauri/src/mcp_server/grants_tests.rs: the stricter rule wins between the agent's approval mode and the extension tool's `confirmation()` (`granted`/`ask`/hidden) from src-tauri/src/extensions/agent_tools/defs.rs (FR-018)
- [ ] T068 [US4] Offer extension tools per agent in src-tauri/src/mcp_server/grants.rs from the same registry source as the built-in agent (src-tauri/src/extensions/agent_tools/registration.rs), filtered by `extension` grants; listen to the registry rebuild and send `tools/list_changed`
- [ ] T069 [US4] Call extension tools from src-tauri/src/mcp_server/dispatch.rs through the existing tool path (`extensions/agent_tools/tool.rs`) with the external caller in the call context; the frame never sees the agent's name (research R5)
- [ ] T070 [US4] Extension rights in the registration dialog and the detail page: one switch per installed extension, default off (src/components/agents/RegistrationDialog.vue, src/components/settings/agents/AgentDetailView.vue)

---

## Phase 9: User Story 5 - Dateien, Kontakte, Passwörter und Befehle (Priority: P2) (PR E)

**Goal**: The data rights of 034/044/051/056 apply to external agents; secrets only after a prompt
per retrieval; commands as their own group.

**Independent Test**: A test agent with one file, storage, contact and password grant; a second
agent without them.

### Tests first

- [ ] T071 [P] [US5] Integration test in src-tauri/tests/mcp_server.rs: without file rights a file tool is refused like an agent without permission in 044, holzi's own data stays locked; with "Dateien des Geräts" a search and read work (US5 scenario 1, FR-020)
- [ ] T072 [P] [US5] Integration test in src-tauri/tests/mcp_server.rs: grant "Lesen, Tag dev" → `passwords.read` lists only headers of `dev` entries; `passwords.secret.get` asks in every mode including `auto` and `plan`, the prompt says the secret goes to the agent and its model provider, the secret is returned only after "allow"; without a grant the call fails without revealing whether the entry exists (FR-019, 034 FR-029)
- [ ] T073 [P] [US5] Integration test in src-tauri/tests/mcp_server.rs: `run_command` is listed only with `commands` granted and where `PlatformCapabilities.command_tool` is true, counts as destructive (Auto asks, Plan refuses) (FR-012)
- [ ] T074 [P] [US5] Rules in scripts/check-mcp-actions.ts: `passwords.secret.get` has scope `passwords.secret`, effect `read`, `builtinAgentCallable: false`, `runner: 'native'`, and no input marked as a secret

### Implementation

- [ ] T075 [US5] Catalog action `passwords.secret.get` (input `entryId`, `field` `password`/`totp`/`custom:<name>`) in src/lib/actions/passwordsActions.ts and a native executor in src-tauri/src/passwords/agent_secret.rs (new file) calling `PasswordService::read_secret_item`/`totp_code` with `Caller::ExternalAgent` and the agent's `Grant`; the tool reports `always_ask() == true`
- [ ] T076 [US5] Build the agent's password `Grant` (`read`/`read_write`, `scope_tags` or all entries) from `haex_mcp_agent_grants` in src-tauri/src/mcp_server/grants.rs, following `browser_bridge/approvals.rs` `Approval → Grant`
- [ ] T077 [US5] Offer the tools of `files.*` and `contacts.*` only when the agent's rows in `agent_file_permissions` cover the tool's kind and actions, and the tools of `passwords.*` when it has a password grant (research R3) in src-tauri/src/mcp_server/grants.rs; test contact-only and file-only grants separately, and keep service checks with `Caller::ExternalAgent`
- [ ] T078 [US5] Show a 044 permission request for an external agent with its reported name in the existing prompt (src-tauri/src/files/agent/prompt.rs and its Vue component), not a second dialog
- [ ] T079 [US5] Offer and dispatch `run_command` (src-tauri/src/chat/tools/cli.rs) for agents with `commands` granted, as `RiskClass::Risky`, only where `command_tool` is available, in src-tauri/src/mcp_server/{grants,dispatch}.rs
- [ ] T080 [US5] Rights groups "Dateien", "Passwörter" (kind and tags) and "Befehle ausführen" in the registration dialog and the detail page, all off by default; file rights write into `agent_file_permissions` via the existing actions `settings.agents.files.set/remove` (src/components/agents/RegistrationDialog.vue, src/components/settings/agents/AgentDetailView.vue)
- [ ] T081 [P] [US5] Texts, including the secret warning of the prompt, in src/i18n/locales/de.json and src/i18n/locales/en.json

**Checkpoint (end of PR E)**: M5 and M6 pass.

---

## Phase 10: User Story 6 - Delegates im Chat bedienen holzi (Priority: P2) (PR F)

**Goal**: Registered Claude Code and Codex delegates get holzi's tools in the chat without any
setup; prompts show in the chat; "Ungated" never bypasses holzi.

**Independent Test**: Chat with a test delegate without and with registration; Ungated mode.

### Tests first

- [ ] T082 [P] [US6] Unit tests in src-tauri/src/adapters/cli_delegate/holzi_mcp_tests.rs: for a registered Claude delegate the `mcp-config.json` contains `holzi-approve` (outside Ungated) and `holzi` with `--mcp-agent claude-code-delegate --delegate-token …` in every mode including Ungated; for Codex the `config.toml` in `CODEX_HOME` contains `[mcp_servers.holzi]`; an unregistered delegate gets no `holzi` server
- [ ] T083 [P] [US6] Integration test in src-tauri/tests/mcp_server.rs: a connection with `delegate: true` and a valid delegate credential is accepted without a file; its call prompts carry `delegateTurnId`

### Implementation

- [ ] T084 [US6] New file src-tauri/src/adapters/cli_delegate/holzi_mcp.rs: build the holzi server entries for Claude and Codex; hook it into src-tauri/src/adapters/cli_delegate/claude.rs (also pass `--mcp-config` in Ungated mode) and src-tauri/src/adapters/cli_delegate/codex.rs with one call each
- [ ] T085 [US6] Delegate registrations in src-tauri/src/mcp_server/registry.rs: kinds `claude_code_delegate` and `codex_delegate`, at most one row per kind and device, id derived from device and kind; the credential stays in the app and is passed per start
- [ ] T086 [US6] Route call prompts of a delegate turn into the chat: `mcp-call-request` with `delegateTurnId` is shown by src/components/chat/PermissionPrompt.vue instead of CallRequestDialog
- [ ] T087 [US6] "Rechte festlegen" in the chat for an unregistered delegate (replacing the notice of 032 FR-023) that opens RegistrationDialog for that delegate: the notice key `chat.toolNotice.delegate` shown by src/components/chat/StatusBanners.vue (driven from src/components/apps/ChatApp.vue) gets the button; registered delegates show no notice
- [ ] T088 [P] [US6] Texts in src/i18n/locales/de.json and src/i18n/locales/en.json

**Checkpoint (end of PR F)**: M7 passes.

---

## Phase 11: HTTP way with OAuth 2.1 (US1, US8 on Android/iOS) (PR G)

**Goal**: Clients that speak only HTTP, and every client on Android/iOS, connect over loopback
with MCP authorization.

**Independent Test**: rmcp HTTP client test; Android e2e; manual M9 and the HTTP part of M10.

### Tests first

- [ ] T089 [P] [US8] Unit tests in src-tauri/src/mcp_server/http_tests.rs: `Host` other than `127.0.0.1:<port>`, `[::1]:<port>`, `localhost:<port>` → 421; any `Origin` header on `/mcp` and `/oauth/token` → 403; body over 1 MiB refused; more than 16 sessions or 10 open authorizations refused (contracts/http-auth.md)
- [ ] T090 [P] [US1] Unit tests in src-tauri/src/mcp_server/oauth_tests.rs: dynamic client registration creates no agent; PKCE S256 required; `redirect_uri` only loopback or native schemes and must match a registered client URI (loopback may vary only by port); the authorization code binds the accepted URI and `/oauth/token` rejects a mismatch; the refresh token is the credential (`token_hash`), rotates on use, and is invalid after revoke or 90 days; access tokens live 1 h in memory only
- [ ] T091 [P] [US1] Integration test src-tauri/tests/mcp_server_http.rs: an rmcp streamable-HTTP client completes 401 → metadata → registration → authorize (answered through `mcp_registration_answer`) → token → `tools/list`

### Implementation

- [ ] T092 [US1] Enable `transport-streamable-http-server` and `auth` of `rmcp`, move `hyper`/`hyper-util` from dev-dependencies to dependencies in src-tauri/Cargo.toml (no `axum`)
- [ ] T093 [US8] Implement src-tauri/src/mcp_server/http.rs: loopback listener on `mcp.http_port` (random on first enable, stored per device), `Host`/`Origin` checks, limits, serving rmcp's streamable HTTP service on `/mcp`
- [ ] T094 [US1] Implement src-tauri/src/mcp_server/oauth.rs: protected-resource and authorization-server metadata, `/oauth/register`, `/oauth/authorize` (page "In holzi bestätigen" that waits for the registration prompt) with exact registered-URI matching (loopback port exception), bind the accepted URI to the authorization code, and enforce the same value at `/oauth/token` with PKCE and refresh rotation; agents with `transport = 'http'` and `oauth_client_id`
- [ ] T095 [US1] HTTP setup snippets for Claude Code, Codex and "anderer Agent" in `mcp_setup_snippet` (src-tauri/src/mcp_server/commands.rs) and in src/components/settings/agents/AgentsView.vue
- [ ] T096 [US1] Start the HTTP way on Android and iOS (only way there) and on desktop alongside the program way, through `PlatformCapabilities` in src-tauri/src/platform/mod.rs
- [ ] T097 [P] [US1] Android e2e scenario scripts/e2e/scenarios/android-mcp-agent.test.ts: an HTTP MCP client in the test driver registers and lists tools

**Checkpoint (end of PR G)**: M9 and M10 pass.

---

## Phase 12: Polish & Cross-Cutting Concerns

- [ ] T098 [P] Update plans/README.md row 021 with the merged deliveries after each PR
- [ ] T099 [P] Document the MCP access for users (what it is, setup snippets, rights groups, revocation) in docs/ next to the existing user docs, if such a page exists for the browser bridge; otherwise only the in-app texts
- [ ] T100 Manual quickstart M1–M10 with real Claude Code and Codex (specs/021-mcp-server/quickstart.md); record results in the last PR description

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** → **Foundational (Phase 2)** → US1 (Phase 3) → US2 (Phase 4) → US8 (Phase 5): all PR B.
- **US3 (Phase 6, PR C)** depends on PR B.
- **US7 (Phase 7, PR D)** depends on PR C (log, prompts).
- **US4/US5 (Phases 8–9, PR E)**, **US6 (Phase 10, PR F)** and **HTTP (Phase 11, PR G)** depend on PR C and D; they do not depend on each other.
- **Polish (Phase 12)** runs alongside; T100 last.

### Within Each Story

- Tests first and failing, then implementation.
- Rust types before `pnpm generate:ts-types`, then frontend.
- `grants.rs` before `dispatch.rs`; `prompts.rs` before the dialogs.

### Parallel Opportunities

- Phase 1: T003 and T004 parallel to T001–T002.
- Phase 2: all test tasks T005–T010 in parallel; T013 parallel to T012 and T014.
- US1: T020–T023 in parallel; T029, T030, T033 in parallel once T028 is merged in the branch.
- US2: T034–T037 in parallel.
- PR E, PR F and PR G can be built in parallel worktrees after PR D.

## Parallel Example: User Story 1

```text
Task: "T020 Integration test registration flow in src-tauri/tests/mcp_server.rs"
Task: "T022 Helper tests in src-tauri/src/mcp_server/helper_tests.rs"
Task: "T023 Prompt limit tests in src-tauri/src/mcp_server/prompts_tests.rs"
Task: "T030 Store src/stores/mcpAgents.ts"
Task: "T033 Texts in src/i18n/locales/{de,en}.json"
```

## Implementation Strategy

### MVP First (PR B)

1. Phases 1–2 (Setup, Foundational).
2. US1 (register) and US2 (rights) and the program-way part of US8 (PR B, not yet switchable).
3. US3 (prompts, approval mode, log) in PR C.
4. **Stop and validate**: Claude Code and Codex register and use holzi with prompts (M1–M4).

### Incremental Delivery

1. PR B → server, registration and rights, tested but hidden.
2. PR C → approvals and log; the access becomes switchable (MVP).
3. PR D → management, revocation, expiry: the core is complete.
4. PR E → extensions, files, passwords, commands.
5. PR F → delegates in the chat.
6. PR G → HTTP and mobile.

Each PR leaves the access off by default and adds value without breaking the earlier ones. The
browser tools spec plugs in after PR D as a new rights group (spec FR-008).
