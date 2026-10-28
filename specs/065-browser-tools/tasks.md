---
description: 'Task list for spec 065-browser-tools'
---

# Tasks: Browser-Werkzeuge für Agenten

**Input**: Design documents from `/specs/065-browser-tools/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/browser-tools.md](./contracts/browser-tools.md),
[contracts/agent-browser.md](./contracts/agent-browser.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md),
[contracts/extension-tabs.md](./contracts/extension-tabs.md), [quickstart.md](./quickstart.md),
[ADR-0019](../../docs/adr/0019-agent-browser-over-cdp-pipe.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic:

- Rust unit tests go in sibling `*_tests.rs` files, registered with
  `#[cfg(test)] #[path = "…_tests.rs"] mod tests;`.
- Rust integration tests go in `src-tauri/tests/agent_browser_*.rs`. They drive a real Chromium
  against the local fixture site in `src-tauri/tests/fixtures/agent_browser/`; a test without a
  browser on the machine is skipped with a printed reason, never passed silently. CI installs
  Chromium for these jobs.
- Catalog rules go in `scripts/check-agent-actions.ts` and `scripts/check-mcp-actions.ts`.
- e2e scenarios go in `scripts/e2e/scenarios/`.
- Tests come first inside each story and fail before the code exists.
- Real Claude Code and the user's own browser are manual (quickstart M1–M9).

**Organization**: Grouped by user story and shipped in the deliveries of research R15. Each
delivery is one PR from its own worktree off `main`.

| Delivery | Phases     | Stories                                                       |
| -------- | ---------- | ------------------------------------------------------------- |
| PR A     | –          | spec, plan, ADR-0019 (proposed), this file                    |
| PR B     | Phases 1–2 | Setup, Foundational (021 groundwork, rights group, passwords) |
| PR C     | Phase 3    | US1 (agent browser, profile, reading)                         |
| PR D     | Phases 4–5 | US2, US3 (operating, submitting, marker, take over, stop)     |
| PR E     | Phases 6–7 | US4, US5 (credentials, downloads, uploads)                    |
| PR F     | Phase 8    | US6 (the user's browser, with a release of the extension)     |
| –        | Phase 9    | Polish, manual checks                                         |

Priorities: P1 US1, US2, US3; P2 US4; P3 US5, US6.

**Shipping notes**:

- PR B lands the migration of `haex_agent_browser_no_sync`, so C–F add none.
- The rights group `browser` is off by default (021 FR-008). Until PR D, the browser tools that
  change a page are not in the catalog, so an agent with the group on can only read (the same
  approach as 021 PR B).
- PR B overlaps with 021 PR E (native actions over MCP with a caller). Whichever lands first
  carries T005–T010; the other drops them.
- ADR-0019 changes from "proposed" to "accepted" in PR C.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: Which user story (US1 … US6); Setup, Foundational and Polish have none

---

## Phase 1: Setup (PR B)

- [x] T001 Create the module `src-tauri/src/agent_browser/mod.rs` with empty submodules of plan.md ("Source Code") and register `pub mod agent_browser;` in src-tauri/src/lib.rs (done: only `sites.rs` so far; the other submodules come with the tasks that fill them, so no empty files land)
- [x] T002 [P] Enable the `stream` feature of `chacha20poly1305` and add `windows-sys` (features `Win32_System_Threading`, `Win32_Foundation`, `Win32_Security`) as a Windows-only dependency in src-tauri/Cargo.toml; no other crate (plan.md Constraints) (moved to PR C with T030 and T032, where they are first used)
- [x] T003 [P] Add `agent_browser: bool` to `PlatformCapabilities` in src-tauri/src/platform/mod.rs (true on Linux, macOS, Windows; false on Android, iOS) with a test in src-tauri/src/platform/mod_tests.rs
- [x] T004 [P] Add the scope `browser` with localized names to src/lib/actions/scopes.ts and src/i18n/locales/de.json, src/i18n/locales/en.json

---

## Phase 2: Foundational (PR B, blocking)

**Purpose**: What every story needs: native actions that know their caller and mode, the rights
group `browser`, the password lookup by website, the device table.

### Tests first

- [x] T005 [P] Unit tests in src-tauri/src/chat/tools/native_action_tests.rs: an executor receives `NativeCall { caller, mode }` with `caller = builtinAgent` from the chat and the chat's permission mode; an executor never reads a caller from its input (ADR 0011)
- [x] T006 [P] Integration test in src-tauri/tests/mcp_server_calls.rs: a native catalog action called by an external agent runs in its Rust executor with `caller = externalAgent { id }` and the agent's mode, not through the window; an action with `confirmsItself` skips the call prompt of 021 in Manuell and Auto and is refused in Plan when it is not read-only (done in src-tauri/tests/mcp_server_native.rs and as a unit test of `call_gate` in src-tauri/src/mcp_server/prompts_tests.rs)
- [x] T007 [P] Unit tests in src-tauri/src/mcp_server/grants_tests.rs for the group `browser`: off without the row with the empty key; `offers` lists `browser.*` read tools only with the group on; `validate` refuses `file:`, `javascript:`, `data:`, an address with user info, `http` with `operate`, and normalizes host case, IDN and default ports (data-model.md)
- [x] T008 [P] Unit tests in src-tauri/src/mcp_server/registry_tests.rs: the group `browser` round-trips through `haex_mcp_agent_grants` (`grant_group = 'browser'`, empty key, `user-browser`, website keys with `level` and `scope_tags = ["fill"]`)
- [x] T009 [P] Unit tests in src-tauri/src/passwords/site_tests.rs: `headers_for_page` returns the headers of a grant's scope whose address has the same site as the origin, nothing for another site, nothing for a lookalike host; the bridge's `get_items` keeps its answers (existing tests in src-tauri/src/browser_bridge stay green) (done in src-tauri/src/passwords/service/by_site_tests.rs and src-tauri/src/passwords/site_tests.rs; the site is the registrable domain of 049, so `login.shop.example.de` and `bad-shop.example.de` share it, `bad-example.de` does not)
- [x] T010 [P] Migration test in src-tauri/src/identity/migrations_agent_browser_tests.rs: fresh vault and upgrade create `haex_agent_browser_no_sync` with the columns of data-model.md, and the table is not tracked by the CRDT (migration `0047_agent_browser`)

### Implementation

- [x] T011 Add `NativeCall { caller: ActionCaller, mode: PermissionMode }` to `NativeExecutor::run` in src-tauri/src/chat/tools/native_action.rs and pass it from `NativeActionTool::execute`; update the files executors (src-tauri/src/files/agent/) to take the caller from it (done; the built-in agent reads the chat's mode through a `ModeSource` set in `setup()`; the files, share and contact executors refuse other callers with `builtin_only` until 021 PR E gives external agents their own)
- [x] T012 Run native actions over MCP in src-tauri/src/mcp_server/dispatch.rs through `ChatState.native_actions` with `ActionCaller::ExternalAgent` and the agent's mode, and honor `confirmsItself` in `prompts::call_gate` (skip the call prompt in Manuell and Auto, refuse non-read in Plan); `AgentActionDef` already carries `runner` and `confirms_itself`
- [x] T013 Extend `Rights` with `browser: Option<BrowserRights>` (`user_browser`, `sites: BTreeMap<Site, SiteGrant>`) in src-tauri/src/mcp_server/grants.rs, `Site` parsing and normalization in a new src-tauri/src/agent_browser/sites.rs, `covers` for scope `browser`, and `validate` per data-model.md
- [x] T014 Load and save the group in src-tauri/src/mcp_server/registry.rs (`load_rights`, `save_rights`) per data-model.md (the rows are built by `grants::browser_rows` and read by `grants::browser_from_rows`)
- [x] T015 Move the site filter of src-tauri/src/browser_bridge/methods/items.rs into `PasswordsService::headers_for_page` in src-tauri/src/passwords/site.rs and call it from the bridge; remove the ponytail comment there (done as `PasswordsService::headers_for_page` in src-tauri/src/passwords/service/by_site.rs, since `site.rs` stays free of the service; `parse_address` and `site_of_address` moved to `passwords::site`. Finding for PR E: the built-in agent lists passwords only through its own agent view (034 FR-027), so filling for it needs a decision in T063)
- [x] T016 Migration `haex_agent_browser_no_sync` in src-tauri/src/identity/migrations_agent_browser.rs, registered in src-tauri/src/identity/migrations.rs (no CRDT trigger, so `HOLZI_TRIGGER_VERSION` stays)
- [x] T017 [P] Rights fields for the group in the registration dialog and the agent page: src/components/agents/BrowserRightsFields.vue (on/off, websites with read/operate, add and remove a website), used by src/components/agents/RegistrationDialog.vue and src/components/settings/agents/AgentDetailView.vue; texts in src/i18n/locales/de.json and src/i18n/locales/en.json (done: on/off and websites with read/operate; the switch for the user's browser follows in T074, withdrawing a remembered fill in T064; `rightsOf` now keeps the browser rights when a scope of "holzi bedienen" changes)
- [x] T018 [P] Extend `settings.agents.mcp.grants.set` in src/lib/actions/settingsMcpAgentActions.ts with `rights.browser` and add the rule "every `browser.*` action is native and every changing one has `confirmsItself`" to scripts/check-agent-actions.ts (and `siteOf` tests in scripts/check-mcp-actions.ts)

**Checkpoint**: Native actions know their caller over both ways; agents can be granted websites;
nothing in the catalog uses it yet.

---

## Phase 3: User Story 1 - Ein Agent erledigt eine Recherche im Agenten-Browser (Priority: P1) 🎯 MVP (PR C)

**Goal**: The agent browser starts on demand, keeps an encrypted profile per device, opens pages
on allowed websites and reads them.

**Independent Test**: An agent with "read" for the fixture site opens a page, reads it, follows a
link and takes a screenshot; the log shows the calls (spec US1).

### Tests first

- [ ] T019 [P] [US1] Unit tests in src-tauri/src/agent_browser/locate_tests.rs: the search order of contracts/agent-browser.md per OS (with a fake file system root), a chosen path wins, a version below 120 counts as not found
- [ ] T020 [P] [US1] Unit tests in src-tauri/src/agent_browser/cdp/mod_tests.rs over an in-memory pipe: NUL framing, replies matched by `id`, events routed by `sessionId`, a reply after cancel is dropped, a closed pipe ends every waiting call with `browser_unavailable`
- [ ] T021 [P] [US1] Unit tests in src-tauri/src/agent_browser/profile_crypto_tests.rs: HXBP round trip for 0 bytes, one block and several blocks; a flipped byte, a cut tail, a reordered block and a wrong `key_id` are refused; the archive never contains plaintext of a cookie fixture
- [ ] T022 [P] [US1] Unit tests in src-tauri/src/agent_browser/profile_tests.rs: only the lasting parts of contracts/agent-browser.md go into the archive; leftovers of a crashed run are removed before a new start; the working directory is created with owner-only access; replacement of the archive is atomic
- [ ] T023 [P] [US1] Unit tests in src-tauri/src/agent_browser/read_tests.rs: the page result has the shape of contracts/browser-tools.md with `untrusted_page_content` and the note; text over 16 000 characters and more than 300 elements are cut with `truncated: true`; `from` and `section` return the right slice
- [ ] T024 [P] [US1] Unit tests in src-tauri/src/agent_browser/sites_tests.rs: navigation checks for an allowed website, a website outside the rights (asks, or refuses in Plan), blocked schemes and internal pages, user info in the address, a redirect to another website
- [ ] T025 [US1] Integration test in src-tauri/tests/agent_browser_read.rs against a real Chromium and the fixture site: start over the pipe (no listening port: check that the process has no TCP listener), `browser.open`, `browser.page.read`, follow a link, `browser.scroll`, `browser.page.screenshot` (image of the tab only), `browser.tabs.list`, page permissions (camera, location) denied
- [ ] T026 [US1] Integration test in src-tauri/tests/agent_browser_profile.rs: a cookie set by the fixture site survives stopping and restarting the browser; after stopping, the working directory is gone and only `profile.hxbp` remains; a vault on another device (other `vault_device_uuid`) starts without the cookie
- [ ] T027 [P] [US1] Fixture site in src-tauri/tests/fixtures/agent_browser/site/ (news page, links, a partner page on a second host, a page that sets a cookie, a slow page) and a small HTTPS server for the tests in src-tauri/tests/common/agent_browser_fixture.rs with a certificate made at test time

### Implementation

- [ ] T028 [US1] Find and check the browser in src-tauri/src/agent_browser/locate.rs per contracts/agent-browser.md
- [ ] T029 [US1] Start over the pipe on Linux and macOS in src-tauri/src/agent_browser/process.rs (descriptors 3 and 4 set before `exec`, start flags of contracts/agent-browser.md, 15 s for `Browser.getVersion`)
- [ ] T030 [US1] Start over the pipe on Windows in src-tauri/src/agent_browser/process_windows.rs (`CreateProcessW` with `STARTUPINFOW.lpReserved2`), with a test in src-tauri/src/agent_browser/process_windows_tests.rs that runs in the Windows CI job
- [ ] T031 [US1] CDP connection in src-tauri/src/agent_browser/cdp/mod.rs and the domains `target.rs`, `page.rs`, `runtime.rs`, `browser.rs`, `fetch.rs`, `dom.rs` (only the methods named in research R1–R11)
- [ ] T032 [US1] Profile key, working directory, packing and unpacking in src-tauri/src/agent_browser/profile.rs and src-tauri/src/agent_browser/profile_crypto.rs (R3), packing in `spawn_blocking`
- [ ] T033 [US1] `AgentBrowser` managed state in src-tauri/src/agent_browser/mod.rs: start on the first call (FR-003), end after 15 minutes without a session, on vault close and on the user closing the window; registered in src-tauri/src/lib.rs and ended with the vault session
- [ ] T034 [US1] Agent tab in src-tauri/src/agent_browser/tab.rs: create target, isolated world `holzi`, script on new documents, element numbers mapped to `backendNodeId`, tab ids `t1`… per session (contracts/agent-browser.md "Je Agenten-Tab")
- [ ] T035 [US1] Reading script in src-tauri/src/agent_browser/page_script.js (main text first, numbered elements with kind, name, value except secret fields, state, visibility) and the reader in src-tauri/src/agent_browser/read.rs (R4, R10, R12)
- [ ] T036 [US1] Website check before loading in src-tauri/src/agent_browser/sites.rs through `Fetch.requestPaused` for documents (R9), with the website prompt of FR-007 parked in src-tauri/src/agent_browser/prompts.rs (5 minutes)
- [ ] T037 [US1] Browser session in src-tauri/src/agent_browser/session.rs (agent, tabs ≤ 5, state, last action) and the executor for the read tools (`browser.open`, `.page.read`, `.page.screenshot`, `.navigate`, `.scroll`, `.tabs.list`, `.wait`) in src-tauri/src/agent_browser/executor.rs, registered in `NativeExecutors`
- [ ] T038 [US1] Log entries in src-tauri/src/agent_browser/executor.rs through `mcp_server::agent_log` with website and label as target; new outcomes `denied_site` and `paused` in src-tauri/src/mcp_server/agent_log.rs
- [ ] T039 [P] [US1] Catalog entries for the read tools in src/lib/actions/browserActions.ts (native, scope `browser`), registered in src/lib/actions/catalog.ts
- [ ] T040 [P] [US1] Settings page "KI → Browser" in src/components/settings/agentBrowser/AgentBrowserView.vue (state, chosen browser and version, rights of the built-in agent, "Profil löschen"), `subView('ai.browser', …)` in src/lib/settings/registry.ts, route in src/components/wm/appRoutes.ts, actions `settings.agentBrowser.builtin.set`, `.path.set`, `.profile.clear` in src/lib/actions/settingsAgentBrowserActions.ts with Tauri commands in src-tauri/src/agent_browser/commands.rs
- [ ] T041 [P] [US1] Website prompt dialog in src/components/agentBrowser/PromptDialog.vue with the store src/stores/agentBrowser.ts (events of contracts/tauri-commands.md), mounted next to the agents' prompts in src/components/agents/Prompts.vue
- [ ] T042 [US1] Android counter case: `browser.*` are not offered and "KI → Browser" says "auf diesem Gerät nicht verfügbar"; check in scripts/e2e/scenarios/android-not-available.test.ts and an exclusion in scripts/e2e/platform-exclusions.ts
- [ ] T043 [US1] Set ADR-0019 to accepted in docs/adr/0019-agent-browser-over-cdp-pipe.md

**Checkpoint (end of PR C)**: quickstart M1, M2 and M8 pass.

---

## Phase 4: User Story 2 - Nur erlaubte Websites und Rückfrage vor dem Absenden (Priority: P1) (PR D)

**Goal**: Agents type, choose and click; holzi asks before anything is submitted.

**Independent Test**: In Auto, filling runs without a prompt and submitting asks; in Plan
submitting is refused; a read-only website refuses typing (spec US2).

### Tests first

- [ ] T044 [P] [US2] Unit tests in src-tauri/src/agent_browser/submit_tests.rs: step 1 of R5 marks submit buttons, default buttons in forms, image inputs, Enter in a form field and the word list (de, en) as submitting, and a plain link or a disclosure button as not
- [ ] T045 [P] [US2] Unit tests in src-tauri/src/agent_browser/prompts_tests.rs: the decision table of contracts/browser-tools.md "Freigabe" for every row and mode; one prompt per action at most
- [ ] T046 [US2] Integration test in src-tauri/tests/agent_browser_act.rs with fixture pages for 20 forms and buy or delete buttons (SC-003): every submit asks in Manuell and Auto; the fixture server receives nothing after "Ablehnen"; a script that posts after an unnamed button (step 2 of R5) is held and asked; changing requests outside the window after an action pass
- [ ] T047 [US2] Integration test in src-tauri/tests/agent_browser_injection.rs with the corpus in src-tauri/tests/fixtures/agent_browser/injection/ (20 pages, SC-002): no call skips a prompt, opens another website without a prompt, changes a right or calls holzi's binding; every page result carries `untrusted_page_content`

### Implementation

- [ ] T048 [US2] Input domain in src-tauri/src/agent_browser/cdp/input.rs and the actions in src-tauri/src/agent_browser/act.rs (`browser.click`, `.type`, `.select`, `.tabs.close` with real input events, R4)
- [ ] T049 [US2] Submit detection in src-tauri/src/agent_browser/submit.rs: step 1 from the reading script, step 2 by holding changing requests through `Fetch` for 3 s after a changing agent action (R5), with a `ponytail:` comment on the word list
- [ ] T050 [US2] Prompts for actions and submitting in src-tauri/src/agent_browser/prompts.rs per contracts/browser-tools.md "Freigabe", including the fields that would be sent with secrets as `•••` (`mcp_server::redact`)
- [ ] T051 [US2] Read-only websites refuse typing, choosing and clicking that is not following a link or opening a disclosure (`read_only_site`, FR-008) in src-tauri/src/agent_browser/act.rs
- [ ] T052 [P] [US2] Catalog entries for the changing tools with `confirmsItself` in src/lib/actions/browserActions.ts
- [ ] T053 [P] [US2] Prompt kinds `submit` and `newTab` in src/components/agentBrowser/PromptDialog.vue with texts in src/i18n/locales/de.json and src/i18n/locales/en.json

---

## Phase 5: User Story 3 - Der Nutzer sieht zu, greift ein oder stoppt (Priority: P1) (PR D)

**Goal**: A visible marker in each agent tab, sessions shown in holzi, take over and stop.

**Independent Test**: "Stoppen" ends the running call within a second; "Eingreifen" pauses the
agent until "Zurückgeben"; user input in a steered tab pauses it (spec US3).

### Tests first

- [ ] T054 [P] [US3] Unit tests in src-tauri/src/agent_browser/session_tests.rs: stop ends the running call and the session, takeover answers `paused` until return, user input pauses, closing the window ends the session, revoking an agent (021) ends its sessions
- [ ] T055 [US3] Integration test in src-tauri/tests/agent_browser_act.rs: stop within 1 s (SC-004) while a page is slow; the marker exists in each document of an agent tab and its buttons react only to trusted events (a synthetic `click()` from the page does nothing); `browser.takeover.request` waits until return

### Implementation

- [ ] T056 [US3] Marker in a closed shadow root from the isolated world, binding `__holzi` in that world only, title prefix of the tab (R6) in src-tauri/src/agent_browser/page_script.js and src-tauri/src/agent_browser/tab.rs
- [ ] T057 [US3] Stop, takeover, return, the tool `browser.takeover.request` (FR-018, catalog entry in src/lib/actions/browserActions.ts) and detection of user input in src-tauri/src/agent_browser/session.rs; command `agent_browser_control` and `agent_browser_sessions` in src-tauri/src/agent_browser/commands.rs; event `agent-browser-sessions-changed`
- [ ] T058 [US3] End all sessions of an agent on revocation (021 `live.rs`) and on vault close in src-tauri/src/agent_browser/session.rs
- [ ] T059 [P] [US3] Sessions bar in src/components/agentBrowser/SessionsBar.vue (agent, tab, website, last action, "Eingreifen", "Stoppen"), mounted in the desktop (src/components/wm/Desktop.vue) while a session runs, and the prompt kind `takeover` in src/components/agentBrowser/PromptDialog.vue
- [ ] T060 [US3] e2e scenario scripts/e2e/scenarios/agent-browser.test.ts: the stand-in agent of 021 (`scripts/e2e/lib/mcp-agent.ts`) with the group `browser` reads a fixture page, fills a form in Auto, is asked before submitting and declined, and is stopped from the sessions bar

**Checkpoint (end of PR D)**: quickstart M1–M5 pass.

---

## Phase 6: User Story 4 - Anmelden, ohne dass ein Modell das Passwort sieht (Priority: P2) (PR E)

**Goal**: `browser.credentials.fill` fills logins from the password manager; nothing flows back.

**Independent Test**: With a marker value in the password, no tool result, screenshot or log entry
contains it (SC-005); the first fill per agent and website asks, a remembered one does not.

### Tests first

- [ ] T061 [P] [US4] Unit tests in src-tauri/src/agent_browser/credentials_tests.rs: filled values are removed from text, element names and replies, also URL-, Base64- and HTML-entity-encoded; screenshots are refused while filled fields exist and allowed after a new document; `http` and a non-matching website fill nothing
- [ ] T062 [US4] Integration test in src-tauri/tests/agent_browser_act.rs with a login fixture that copies the password into visible text after filling: the agent never sees the marker value; first fill asks, "Für diese Website merken" stores `scope_tags = ["fill"]`, the next fill does not ask; several matching entries ask every time; the agent needs the password group (021) for the entry

### Implementation

- [ ] T063 [US4] Fill in src-tauri/src/agent_browser/credentials.rs with `PasswordsService::headers_for_page`, a `Grant` of the agent's password group, writing in the isolated world, marking filled fields (R7) and a `ponytail:` comment on the blocked screenshots
- [ ] T064 [US4] Prompt kind `fill` with the entries' titles for the user only and "Für diese Website merken" in src-tauri/src/agent_browser/prompts.rs and src/components/agentBrowser/PromptDialog.vue; withdrawing the remembered fill in src/components/agents/BrowserRightsFields.vue
- [ ] T065 [P] [US4] Catalog entry `browser.credentials.fill` in src/lib/actions/browserActions.ts

---

## Phase 7: User Story 5 - Herunterladen und Hochladen (Priority: P3) (PR E)

**Goal**: Downloads into "Agenten-Downloads", uploads only from paths the file rights reach.

**Independent Test**: A fixture download lands in the folder and in the log; a download over
100 MB asks; an upload without a file right is refused (spec US5).

### Tests first

- [ ] T066 [P] [US5] Unit tests in src-tauri/src/agent_browser/downloads_tests.rs: names are made safe and unique in the folder, the 100 MB limit from `Content-Length` and while running, an upload path outside the agent's file rights (044) is refused without saying whether it exists
- [ ] T067 [US5] Integration test in src-tauri/tests/agent_browser_act.rs: `browser.download` of a fixture invoice, a 150 MB fixture asks, `browser.upload` asks as submitting and sets the file

### Implementation

- [ ] T068 [US5] Downloads and uploads in src-tauri/src/agent_browser/downloads.rs (R11), prompt kind `download`, the folder "Agenten-Downloads" in the instance's own storage reachable through 044
- [ ] T069 [P] [US5] Catalog entries `browser.download`, `browser.upload` in src/lib/actions/browserActions.ts

**Checkpoint (end of PR E)**: quickstart M6, M7 pass.

---

## Phase 8: User Story 6 - Den eigenen Browser des Nutzers steuern lassen (Priority: P3) (PR F)

**Goal**: Handed-over tabs in the user's browser, through the 049 extension, with the same rules.

**Independent Test**: A handed-over tab is read and operated; another tab stays invisible; submit
asks in Auto; "Zurücknehmen" ends the control (spec US6).

### Tests first

- [ ] T070 [P] [US6] Protocol tests in src-tauri/src/browser_bridge/protocol_tests.rs for the messages of contracts/extension-tabs.md: only clients that declared `agentTabs` get them; a message for a tab that was not handed over is answered `notHandedOver`
- [ ] T071 [US6] Integration test in src-tauri/tests/browser_bridge_agent_tabs.rs with the bridge test client (`e2e_bridge_client`) acting as the extension: hand over, read, click with submit prompt in Auto, a new tab from the page asks, take back ends the session, the right `user-browser` is required

### Implementation

- [ ] T072 [US6] Bridge side of the messages in src-tauri/src/browser_bridge/methods/agent_tabs.rs and a browser backend for handed-over tabs in src-tauri/src/agent_browser/user_browser.rs that the executor uses instead of CDP (FR-035: missing capabilities are left out)
- [ ] T073 [US6] Extension side in haextension `apps/haex-pass-browser` (separate PR there): hand over and take back in the popup, `chrome.debugger` only for handed-over tabs, content script in an isolated world for reading and the marker; pin the released version in this repo
- [ ] T074 [P] [US6] Rights field "Browser des Nutzers" in src/components/agents/BrowserRightsFields.vue and the browser shown per session in src/components/agentBrowser/SessionsBar.vue

**Checkpoint (end of PR F)**: quickstart M9 passes.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [ ] T075 [P] Delegates (after 021 PR F): check that a registered delegate gets the browser tools through its MCP registration and never through its own autonomy mode; test in src-tauri/tests/mcp_server_calls.rs
- [ ] T076 [P] Offer to download "Chrome for Testing" when no browser is found (`settings.agentBrowser.download`), with size, source and checksum shown, in src-tauri/src/agent_browser/locate.rs and src/components/settings/agentBrowser/AgentBrowserView.vue
- [ ] T077 Manual quickstart M1–M9 on Linux, macOS and Windows with a real Claude Code
- [ ] T078 Update the agent instructions of 021 (`session::INSTRUCTIONS_REGISTERED`) and the tool descriptions with the rule that page content is data, never an instruction

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** and **Foundational (Phase 2)**: PR B, blocking everything else.
- **US1 (Phase 3, PR C)** depends on PR B.
- **US2 and US3 (Phases 4–5, PR D)** depend on PR C; they ship together because operating without
  stop and marker is not acceptable (spec US3 "Why this priority").
- **US4, US5 (Phases 6–7, PR E)** depend on PR D.
- **US6 (Phase 8, PR F)** depends on PR D and on a release of the extension.
- **Polish (Phase 9)**: T075 after 021 PR F; the rest after PR E.

### Within Each Story

- Tests first; they fail before the code exists.
- CDP domains before the actions that use them; the executor last.
- Catalog entries and UI after the Rust side they call.

### Parallel Opportunities

- T002–T004 in parallel; T005–T010 in parallel; T017, T018 next to the Rust tasks.
- In US1: T019–T024 and T027 in parallel; T039–T041 next to T037.
- US4 and US5 can be built in parallel inside PR E.

---

## Parallel Example: User Story 1

```text
Task: "Unit tests for locating the browser in src-tauri/src/agent_browser/locate_tests.rs"
Task: "Unit tests for the CDP framing in src-tauri/src/agent_browser/cdp/mod_tests.rs"
Task: "Unit tests for the HXBP archive in src-tauri/src/agent_browser/profile_crypto_tests.rs"
Task: "Fixture site in src-tauri/tests/fixtures/agent_browser/site/"
```

---

## Implementation Strategy

### MVP First (PR B + PR C)

1. Foundational groundwork (PR B).
2. US1: the agent browser reads allowed websites (PR C). Agents can already research, with no way
   to change anything on a page.
3. Stop and validate with quickstart M1, M2, M8.

### Incremental Delivery

1. PR C → reading (MVP).
2. PR D → operating with prompts before submitting, marker, take over and stop.
3. PR E → logins without the model seeing passwords, downloads and uploads.
4. PR F → handed-over tabs in the user's browser.
