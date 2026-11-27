---
description: "Task list for spec 102: delegates reach the web only through HaexVault's agent browser"
---

# Tasks: Delegates reach the web only through HaexVault's agent browser

**Input**: Design documents from `/specs/102-delegate-web-tools/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/web-block.md](contracts/web-block.md), [contracts/delegate-server.md](contracts/delegate-server.md),
[quickstart.md](quickstart.md)

**Tests**: Included. Each requirement is a boundary (a switch, a refused connection, a refused start) and the repository
rules require tests for those. Rust tests live in sibling `*_tests.rs` files, never inline; stub programs stand in for the
real delegates. Tasks marked **(operator)** need the operator's accounts and record their result in `research.md`.

**Organization**: by user story. US1 and US2 are P1, US3 is P2. Delivery: PR A = Phases 1 and 3, PR B = Phase 4, PR C =
Phases 5 and 6.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 to US3 as in spec.md; Setup and Polish tasks carry none

## Commands used by the verification tasks

- Rust on this machine only while the app is not running, with `-j 1`, `CARGO_PROFILE_DEV_DEBUG=line-tables-only
CARGO_INCREMENTAL=0` and `env -u CC -u CXX -u AR` inside a memory-limited scope; otherwise rely on the PR's CI.
- Frontend and scripts: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm check:chat-state`, Prettier from the
  worktree folder (`prettier --check .` before a push).
- After `cargo test` regenerates ts-rs bindings: `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.

---

## Phase 1: Setup

- [ ] T001 Create `src-tauri/src/adapters/cli_delegate/web_block.rs` with its module documentation (spec 102, research
      R1–R5) and the sibling `web_block_tests.rs` (`#[cfg(test)] #[path = "web_block_tests.rs"] mod tests;`); declare
      `mod web_block;` in `src-tauri/src/adapters/cli_delegate/mod.rs`.

---

## Phase 2: Foundational

No shared prerequisite beyond Phase 1: US1 and US2 touch different modules and can start in parallel.

---

## Phase 3: User Story 1 - A delegate opens websites only through HaexVault's agent browser (Priority: P1) — PR A

**Goal**: every delegate start carries its vendor's web block, confirmed where the program reports it.

**Independent Test**: stub delegates report an init line / app-server answers with and without the block; a start without
it ends with `AdapterError::Unavailable`; the builders' arguments match [contracts/web-block.md](contracts/web-block.md).

### Tests for User Story 1

- [ ] T002 [P] [US1] `web_block_tests.rs`: Claude's arguments are `--tools <allow-list>` and `--no-chrome`, the
      allow-list holds every name of contracts/web-block.md and neither `WebFetch` nor `WebSearch`; Codex' arguments are
      `-c web_search="disabled"` plus the five `--disable` pairs and never `computer_use`; Abacus' deny rules are the three
      `Bash(...)` rules; the arguments are the same for `Standard`, `Ungated` and `GatedPermissive`.
- [ ] T003 [P] [US1] `web_block_tests.rs`: the Claude confirmation accepts an init line whose `tools` lacks both web tools
      and whose `mcp_servers` has no `claude-in-chrome`; it refuses one with `WebFetch`, one with `WebSearch`, one with a
      connected `claude-in-chrome`, and one without a `tools` field, each with a reason naming what was found.
- [ ] T004 [P] [US1] `web_block_tests.rs`: the Codex confirmation accepts a `config/read` result with
      `web_search: "disabled"` and an `experimentalFeature/list` result with the five features `enabled: false`; it refuses
      `web_search` `"live"` or `"cached"` or missing, a feature `enabled: true`, a feature missing from the list, and an
      error answer. Answers are recorded from the 0.169.0 probe of research R2.

### Implementation for User Story 1

- [ ] T005 [US1] `web_block.rs`: `CLAUDE_TOOLS` (the allow-list of contracts/web-block.md as one comma-separated value),
      `claude_args()`, `CODEX_DISABLED_FEATURES`, `codex_args()`, `ABACUS_DENY_RULES`, and the pure confirmations
      `confirm_claude_init(&Value) -> Result<(), String>` and `confirm_codex(config_read: &Value, features: &Value) ->
Result<(), String>`; the reason text of contracts/web-block.md ("{vendor} started without its web block ({what});
      nothing was run."). Makes T002–T004 pass.
- [ ] T006 [US1] `src-tauri/src/adapters/cli_delegate/claude.rs` `build_command`: add `web_block::claude_args()` after
      the catalog's `config_args`, in every autonomy mode; `claude_tests.rs` asserts the arguments for each mode, that
      `--allowedTools mcp__haex` stays for a start with HaexVault's server, and that the block sets no environment variable
      (the command's `get_envs()` equals that of a start without the block; spec FR-005).
- [ ] T007 [US1] `claude.rs` `spawn_claude_invocation`: read the first `system/init` line before any chunk reaches the
      chat and run `confirm_claude_init`; on refusal terminate the child (`classify_exit_error` path) and return
      `AdapterError::Unavailable` with the reason. Extend the stub of `src-tauri/tests/cli_delegate_claude.rs` to print an
      init line with and without `WebFetch`: with it the request ends with the reason and no answer text, without it the
      answer arrives.
- [ ] T008 [US1] `src-tauri/src/adapters/cli_delegate/codex.rs` `build_command`: add `web_block::codex_args()` after
      the catalog's `config_args` and before `app-server`; `codex_tests.rs` asserts the order and that the block sets no
      environment variable (spec FR-005).
- [ ] T009 [US1] `codex.rs` `spawn_codex_app_server`: after `initialize`/`initialized` and before `thread/start`, send
      `config/read` (`includeLayers: false`) and `experimentalFeature/list`, following `nextCursor` until it is `null` and
      joining the pages' `data`, pass both results to `confirm_codex`, and on refusal stop the app-server and return
      `AdapterError::Unavailable`. The request/answer plumbing stays in `codex.rs`, the judgement in `web_block.rs` (the
      file's maintainability exception). Extend the stub app-server used by `src-tauri/tests/cli_delegate_codex_live.rs`
      (or its unit stub) to answer both requests: once with the block, once with `browser_use` enabled, once with the list
      split over two pages, and once exiting at start with `Error: Unknown feature flag: browser_use` on stderr, whose text
      must reach the chat as the reason (spec FR-007, SC-005).
- [ ] T010 [US1] `src-tauri/src/adapters/cli_delegate/abacus.rs`: replace `approval_settings()` by
      `run_settings(asks: bool) -> Value` holding `permissions.deny = ABACUS_DENY_RULES` always and `permissions.ask = ["*"]`
      only when `asks`; write the file and pass `--settings` in "Standard" and "Ungated" (today only when `asks`); keep
      `APPROVAL_SETTINGS_FILE` as the file name. `abacus_models.rs` gets the same file through `build_command`. Update
      `abacus_tests.rs`: both modes pass `--settings`, the file of "Ungated" has no `ask`, the file of "Standard" has both,
      and the block sets no environment variable (spec FR-005).
      Update `src-tauri/tests/cli_delegate_abacus_run.rs` where it reads the settings path.
- [ ] T011 [US1] `src-tauri/src/adapters/cli_delegate/haex_mcp.rs`: `with_instruction` takes the vendor; for
      `DelegateVendor::Abacus` every start gets one sentence naming `web_search_text_only` ("WebSearch") and
      `scrape_url_content` ("Fetch") as tools not to use, with or without HaexVault's server; with the server it adds
      HaexVault's agent browser as the way to the web (spec FR-009). Callers in `claude.rs`, `codex.rs`, `abacus.rs` pass
      their vendor. `haex_mcp_tests.rs`: the Abacus text names both tools with and without the server, Claude's and Codex'
      text is unchanged.
- [ ] T012 [US1] Model jobs (spec 055) use the same builders: add one test per vendor in the job adapter's tests (find the
      call sites of `claude::build_command`, `codex::build_command`, `abacus::build_command` outside the chat path) that a
      job start carries the block.
- [ ] T013 [US1] PR A gate: `cargo clippy --all-targets -- -D warnings`, `cargo test` for `adapters::cli_delegate` and the
      `cli_delegate_*` integration tests (CI), Prettier; push branch `feat/102-pr-a-web-block` (from `origin/main`,
      `--no-track`), PR as haex-space.

**Checkpoint**: every delegate start has its web block; US1 is testable with the stubs.

---

## Phase 4: User Story 2 - A delegate has HaexVault's tools whether or not the external access is on (Priority: P1) — PR B

**Goal**: HaexVault's server runs with the vault; the external access only opens it to programs outside
([contracts/delegate-server.md](contracts/delegate-server.md)).

**Independent Test**: with `mcp.enabled` off, a delegate start gets a `HaexServer` and its pass connects; a registered
program's credential is refused; the HTTP endpoint is closed; turning the access on opens both without a vault restart.

### Tests for User Story 2

- [ ] T014 [P] [US2] `src-tauri/tests/mcp_server_delegates.rs`: with the access off, `DelegateHub::server_for` returns a
      server and a connection with that start's pass is admitted; a pass of an ended start is refused.
- [ ] T015 [P] [US2] `src-tauri/tests/mcp_server.rs` (local endpoint) and `mcp_server_http.rs`: with the access off, a hello
      with a registered program's credential and a registration request are closed before `tools/list` answers; the HTTP
      port is not bound; after turning the access on, both work without restarting the server's core; after turning it off
      again, an open program connection is closed and a running delegate's connection stays.

### Implementation for User Story 2

- [ ] T016 [US2] `src-tauri/src/mcp_server/server.rs`: `StartArgs.external: bool`; the HTTP listener only when `external`;
      `Running::set_external(bool)` that opens or closes the HTTP listener and, when turned off, closes every live session
      that is not a delegate (`Live`/`CloseReason::Disabled`).
- [ ] T017 [US2] `src-tauri/src/mcp_server/connection.rs` (hello handling around the `greeting.delegate` branch): while
      `!external`, any hello that is not a delegate pass gets the unknown-client answer and is closed; the delegate branch is
      unchanged.
- [ ] T018 [US2] `src-tauri/src/mcp_server/mod.rs` (`apply`/`apply_locked` and the vault-open start around line 332): the
      server starts when a vault opens whatever `mcp.enabled` says, with `external = enabled`, on platforms with delegates
      (`capabilities().cli_delegates`); elsewhere (Android, iOS) it keeps starting only while `mcp.enabled` is on.
      `mcp_configure` changing the access calls `set_external` instead of stopping and starting; a start error is kept in
      the slot with its text, for T019.
- [ ] T019 [US2] `src-tauri/src/adapters/cli_delegate/haex_mcp.rs` (`HaexAccess::server_for`, `server_for`) and
      `src-tauri/src/mcp_server/delegates.rs` (`DelegateHub::server_for`): return `Result<HaexServer, String>`; each of
      the five paths that today log and return `None` (no running server with the kept start error, model not offered by
      the active profile, write error, no helper path, no pass) returns its reason. The three drivers (`claude.rs`,
      `codex.rs`, `abacus.rs`) emit `chat-delegate-tools-unavailable` `{ threadId, delegateKind, reason }` through
      `DelegateChatContext.emit` for a chat start and run the turn without HaexVault's server; a job start only logs.
      Remove `mcp_delegate_status`, `DelegateStatus` and `src/types/bindings/DelegateStatus.ts`. Tests: each reason in
      `src-tauri/tests/mcp_server_delegates.rs`, the event in one driver test per vendor.
- [ ] T020 [US2] `src/lib/chat/toolNotice.ts`, `src/composables/useToolNotice.ts`, `scripts/check-chat-tool-notice.ts`:
      drop `delegateNoticeKey` and the `mcp_delegate_status` call; listen to `chat-delegate-tools-unavailable` and show
      `chat.toolNotice.delegateUnavailable` with `{name}` and `{reason}` once per conversation and reason. The check
      `pnpm check:chat-state` covers the once-per-reason rule.
- [ ] T021 [P] [US2] `src/i18n/locales/de.json` and `en.json`: remove `chat.toolNotice.delegateOff` and
      `delegateStatusUnknown`; add `chat.toolNotice.delegateUnavailable` ("{name} hat für diese Antwort keine
      HaexVault-Werkzeuge: {reason}"); rewrite
      `settings.locations.ai.access.description`, its `keywords` and `actions.settings.agents.mcp.enable` so they say the
      access is for programs outside HaexVault (an editor such as VS Code) and that the agents in the chat do not need it;
      the same sentence in `src/components/settings/agents/AccessView.vue` where it explains the switch.
- [ ] T022 [US2] `scripts/e2e/scenarios/mcp-agent.test.ts` and `android-mcp-agent.test.ts`: check whether a scenario
      relies on the server being down with the access off (e.g. "no socket while off") and adjust it to "refused while
      off"; run the e2e workflow on the branch before merge (`gh workflow run ci.yml --ref <branch>`).
- [ ] T023 [US2] PR B gate: `cargo clippy`, `cargo test` for `mcp_server` and the `mcp_server_*` integration tests (CI),
      `pnpm typecheck`, `pnpm check:chat-state`, Prettier; push branch `feat/102-pr-b-delegate-server`, PR as
      haex-space.

**Checkpoint**: with the access off, a delegate uses HaexVault's agent browser under its profile; programs outside stay
out.

---

## Phase 5: User Story 3 - The person can see that the block holds (Priority: P2) — PR C

**Goal**: the block is recorded per vendor with the versions it was checked against, and the Settings text names it and
its residuals.

**Independent Test**: adding a version to `catalog.json` without adding it to `checked_versions` fails a test; the
Settings view of the autonomy modes shows the block and the residuals.

- [ ] T024 [P] [US3] `web_block_tests.rs`: for each vendor of `Catalog::shipped()`, every listed version is in that
      vendor's `checked_versions`; a test catalog with an extra version fails the check with a message pointing at
      quickstart §4.
- [ ] T025 [US3] `web_block.rs`: `checked_versions` per vendor (`claude` 2.1.311, `codex` 0.169.0, `abacusai` 2.10.23)
      and the check function T024 calls.
- [ ] T026 [P] [US3] `src/i18n/locales/de.json`, `en.json`: `settings.autonomyMode.webBlock` — in every mode the agents'
      own web search and browser are off and websites go through HaexVault's agent browser; through their shell, through
      computer use and, for Abacus, through its server-side web search they can still reach the web. Show it under the
      autonomy mode choice (find the component that renders `settings.autonomyMode.*Description`).
- [ ] T027 [P] [US3] `specs/096-abacus-delegate/research.md`: a §16 pointer — "Ungated" passes a settings file with deny
      rules since spec 102 (research R3 there).

---

## Phase 6: Operator runs and polish

- [ ] T028 (operator) quickstart §1 per vendor and mode with the external access off, then once per vendor with it on
      (SC-002 "on and off"); record per vendor in research a "Measured" line (SC-001, SC-002, SC-004, SC-003 traces).
- [ ] T029 (operator) quickstart §2: a program outside refused while off, connecting while on; profile without browser.
- [ ] T030 (operator, credits) quickstart §3: Abacus' browser refused in both modes; count own web tool use vs.
      HaexVault's browser after T011; record in research R4.
- [ ] T031 PR C gate and polish: `cargo clippy`, `cargo test` (CI), `pnpm typecheck`, Prettier; tick the tasks, update
      the plan's delivery notes; push branch `feat/102-pr-c-version-check`, PR as haex-space.

---

## Dependencies & Execution Order

- Phase 1 → Phase 3 (US1) and Phase 4 (US2) in parallel up to T018; they touch different modules
  (`adapters/cli_delegate/` vs. `mcp_server/` and the chat notice). T019 changes the drivers' call of `server_for` and
  meets PR A there.
- Within US1: T002–T004 before T005; T005 before T006–T012; T006 before T007, T008 before T009.
- Within US2: T014–T015 before T016–T018; T016 before T017 and T018; T018 before T019 (the kept start error); T019
  before T020; T021 any time. T019 touches the three drivers that PR A changes: base PR B on PR A or rebase after it.
- US3 needs T005 (the module) only; T024 before T025.
- Operator runs need PR A and PR B merged (T028, T029) and T011 (T030).

## Parallel Example: User Story 1

```text
T002, T003, T004  (all in web_block_tests.rs: write together, one commit)
then T005
then T006 → T007  ||  T008 → T009  ||  T010  ||  T011
```

## Implementation Strategy

- **MVP**: PR A (US1) alone already closes the observed gap (Abacus' own browser) in every mode; without PR B a delegate
  still has no web while the access is off, now without falling back to its own browser.
- **Then** PR B gives every delegate HaexVault's agent browser regardless of the access.
- **Then** PR C records the versions and names the residuals in Settings; the operator runs close the spec.
