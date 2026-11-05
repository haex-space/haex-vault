---
description: 'Task list for spec 083, agent profiles'
---

# Tasks: Agent profiles

**Input**: Design documents from `specs/083-agent-profiles/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/commands-and-actions.md, quickstart.md

**Tests**: included. The spec's acceptance scenarios name what to check and the spaex constitution asks for one
runnable check per non-trivial piece of logic. Rust unit tests go in `*_tests.rs` next to their module, integration
tests in `src-tauri/tests/`, frontend logic in `scripts/check-*.ts`, UI flows in `scripts/e2e/scenarios/`.

**Organization**: by user story of spec.md, after a foundation that must land first. US1 (strict in one step) and US2
(configure once) are P1; US3 (a profile for some agents), US4 (registration), US5 (built-in agent) and US6 (remembered
answers) are P2; US7 (migration) is P3 but its core is in the foundation, because switching the source of the rights
without it would reset every agent. The PR letters of plan.md are marked on the phases.

**Working rules for every task**:

- **graphify first**: before a new named function, run `graphify query "<concept>"` and extend an existing candidate
  where one matches (spaex). Where a task says "find", that is the way.
- **Run tooling in the devShell**: `nix develop --command pnpm <script>` and
  `nix develop --command scripts/with-nix-host-bridge.sh cargo <args> -j 4`; a fresh worktree needs
  `nix develop --command pnpm install --frozen-lockfile` and a reflinked `src-tauri/target` (cargo notes of the
  project). Run only the targeted cargo tests named in a task.
- **Bindings**: `cargo test` regenerates `src/types/bindings/*.ts` with trailing whitespace; strip it
  (`sed -i 's/[ \t]*$//'`) and restore bindings you did not change (`git checkout -- <file>`) before committing.
- **Texts**: user-facing texts in German (`de.json`) and English (`en.json`), same keys in both; "Abo-Agent" and
  "Agentenprofil", never "Delegate"/"Delegat" or "Profil" alone; "HaexVault"; no bare "Relay". Errors are toasts, no
  success notices, no save buttons (settings rules).
- **Commits**: Conventional Commits, no agent trailer (constitution), one PR per letter of plan.md, rebase-merge or
  merge commit. Format every touched `specs/` file and frontend file with `pnpm exec prettier --write`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: the user story of spec.md

---

## Phase 1: Setup

- [ ] T001 Run `graphify update .` in the worktree root so later queries see the current tree.
- [ ] T002 Check that migration number 0054 is free: list `src-tauri/src/identity/migrations*.rs` on `origin/main` and in
      every open worktree and branch (`git worktree list`, `git branch -r`); if another branch took 0054, take the next
      number everywhere below and add the entry to `src-tauri/src/identity/migrations_legacy_names.rs` for the later one.

---

## Phase 2: Foundation (PR A, blocks every story)

**Purpose**: the tables, the resolver, external agents and Abo-Agents reading their rights from profiles, and the
migration that keeps every agent's rights. Nothing changes for the user.

- [ ] T003 Create `src-tauri/src/identity/migrations_agent_profiles.rs` with the SQL const for migration
      `0054_agent_profiles`: tables `haex_agent_profiles` (`id` TEXT PK, `name` TEXT NOT NULL),
      `haex_agent_profile_grants` (`id` TEXT PK, `profile_id` TEXT NOT NULL, `grant_group` TEXT NOT NULL, `grant_key` TEXT
      NOT NULL DEFAULT '', `level` TEXT NOT NULL, `scope_tags` TEXT) and `haex_agent_profile_members` (`id` TEXT PK,
      `subject` TEXT NOT NULL, `profile_id` TEXT NOT NULL). Per data-model.md: synced CRDT tables, no
      `vault_device_uuid`, no UNIQUE besides the primary key (a conflict halts the sync), no cascading foreign key. Header
      comment explains the vault-wide scope against the device-scoped agent tables (ADR-0001) and cites spec 083.
- [ ] T004 Register the migration in `src-tauri/src/identity/migrations.rs` (`holzi_migration_source()`, after
      `0053_sync_rules_source_target`), bump `HOLZI_TRIGGER_VERSION` from 34 to 35 with a line in its history comment, and
      check how a new synced table gets its CRDT triggers (follow `migrations_mcp_agents.rs`); update every test that pins
      the migration count or the trigger version (`identity/migrations_tests.rs`, `sync/handshake_refusal_tests.rs`).
- [ ] T005 [P] Create `src-tauri/src/identity/migrations_agent_profiles_tests.rs`: the three tables exist with exactly
      the columns of T003, a second member row with the same `id` is refused by the primary key, a delete leaves a marker
      in `haex_deleted_rows`; model it on `migrations_mcp_agents_tests.rs`.
- [ ] T006 Create `src-tauri/src/agent_profiles/mod.rs` (declare the module in `src-tauri/src/lib.rs`): `Subject` with
      `Agent(Uuid)` and `Model(String)`, `Subject::key()` giving `agent:<uuid>` / `model:<id>` and `Subject::parse`;
      `STANDARD_PROFILE_ID` (one fixed UUID written as a constant, never generated at runtime); namespaces
      `NS_PROFILE_MEMBER` and `NS_PROFILE_GRANT`; `member_id(subject)` = UUIDv5 of the subject key, `grant_id(profile, group,
    key)` = UUIDv5 of `{profile}:{group}:{key}` (data-model.md).
- [ ] T007 Create `src-tauri/src/agent_profiles/store.rs`: reads and writes of the three tables through the CRDT write
      path used by `mcp_server/registry.rs` (`save_rights` is the model): list and get profiles, insert, rename, replace
      the grant rows of a profile in one write (DELETE then INSERT, rows built by the existing
      `mcp_server/grant_rows.rs` `rows_of`), upsert a member row by `member_id`, delete a member row, delete a profile
      (its grant rows, its member rows and the profile row in one write, each an explicit delete so it leaves a marker),
      name checks (`name_empty`, `name_taken`, "Standard" cannot be renamed or deleted).
- [ ] T008 Create `src-tauri/src/agent_profiles/resolve.rs`: `rights_of(db, subject) -> Rights` as data-model.md
      "Resolution": the member row's profile if that profile exists, else "Standard"; "Standard" without a profile row
      answers `grants::default_rights()`; the rows go through the existing `grant_rows::rights_of`, which skips what it
      cannot read; a member row with an unreadable field gives no rights (FR-012).
- [ ] T009 [P] Create `src-tauri/src/agent_profiles/store_tests.rs` and `resolve_tests.rs`: Standard is the factory
      rights while it has no row; the first stored Standard replaces them; a dangling profile id means Standard; unknown
      group or level rows are skipped; assigning the same subject twice leaves one member row; deleting a profile moves
      its members to Standard and leaves markers; names are unique and "Standard" is fixed.
- [ ] T010 Make `mcp_server/registry.rs` `load_rights` resolve through `agent_profiles::resolve::rights_of(Subject::Agent(id))`
      keeping its signature, so the callers stay as they are: `connection.rs`, `attach.rs`, `delegates.rs`, `oauth_rows.rs`,
      `password_env.rs`, `manage.rs` (`list_with_rights`), `manage_commands.rs` (`mcp_agent_get`), `agent_browser/env.rs`
      (`rights`, `password_rights`).
- [ ] T011 Keep the old write paths working until PR D with a compatibility write: `registry::save_rights` (called by
      `register`, `register_delegate`, `manage::renew`, `manage::set_rights`, `oauth_rows.rs`) writes the rights into a
      profile of its own for that agent (created named after the agent on the first write, assigned by a member row) instead
      of `haex_mcp_agent_grants`. Mark it `// removed in PR E (spec 083 T060)`.
- [ ] T012 `mcp_server/manage.rs` `revoke`: in the same write also delete the agent's member row
      (`Subject::Agent(id)`), as an explicit delete with its marker.
- [ ] T013 Create `src-tauri/src/agent_profiles/migrate.rs`, step 1 (research R7): after the vault is open, for each agent
      row of this device load its legacy rows from `haex_mcp_agent_grants` with `grant_rows::rights_of`; equal to
      `default_rights()` (after normalising) means no assignment; otherwise create a profile named after the agent (a taken
      name gets the device alias appended) with deterministic id `UUIDv5("migrated-agent:{device}:{agent}")` holding those
      rows, and assign the agent. Idempotent through the device preference `agent_profiles.migrated_v1` (set last) and
      deterministic ids. Run it from the post-open path next to the `storage/maintenance.rs` steps (find the call site
      with `graphify query "post-open maintenance preferences folding"`).
- [ ] T014 [P] Create `src-tauri/src/agent_profiles/migrate_tests.rs`: a factory-rights agent gets no assignment; an agent
      with a browser right gets a profile with its name and the same rights; two agents with one name do not collide;
      running twice, and running after a simulated stop in the middle, changes nothing more; for every fixture agent the
      migrated `rights_of` equals the legacy rights (nothing widens, FR-025).
- [ ] T015 Run `cargo test -j 4 --lib -- agent_profiles identity::migrations mcp_server sync::handshake` and the
      `src-tauri/tests/mcp_server_*.rs` integration tests; all existing tests must still pass unchanged apart from
      T004. Open PR A.

**Checkpoint**: every agent has the rights it had; rights now come from profiles; nothing is visible.

---

## Phase 3: User Story 1 - Make an agent stricter in one step (Priority: P1)

**Goal**: assign an agent to a profile and see its next call limited, in every kind of access at once.

**Independent Test**: quickstart M1.

- [ ] T016 [US1] Create `src-tauri/src/agent_profiles/subjects.rs`: `list_subjects(db, device)` returning the agents of this
      device with `kind` (`external` for `AgentKind::External`, `subscription` for the two delegate kinds, per
      `mcp_server/registry.rs`), name, status and the profile id and name from the resolver.
- [ ] T017 [US1] Create `src-tauri/src/agent_profiles/commands.rs` with `agent_profiles_list`, `agent_subjects_list` and
      `agent_assign` as in contracts/commands-and-actions.md (`profile_missing` for an unknown profile; "Standard" deletes
      the member row); register them in the Tauri invoke handler (find with `graphify query "tauri invoke handler
    mcp_agent_rights_set"`); export `AgentProfileView` and `AgentSubjectView` with ts-rs and regenerate the bindings.
- [ ] T018 [US1] `mcp_server/live.rs`: add a refresh that re-resolves the rights of every live session through
      `registry::load_rights`, serialised by the existing `synchronize`, updates the session (as `set_rights` does) and sends
      `tools/list_changed` where the tools differ; call it after `agent_assign`, and emit `EVENT_AGENTS_CHANGED`.
- [ ] T019 [P] [US1] Add `src-tauri/tests/mcp_server_profiles.rs`: two live agents in "Standard"; assign one to a profile that
      grants nothing; its tool list shrinks and a browser or password call is refused, the other agent is unchanged; assign it
      back and it has the factory rights again (FR-006, FR-010, FR-011, M1).
- [ ] T020 [P] [US1] Add `disabled` to `src/components/agents/PasswordRightsFields.vue`, `CommandRightsFields.vue`,
      `ExtensionRightsFields.vue`, `RightsInset.vue` children and `RightsEditor.vue` (`RightsFields` and
      `BrowserRightsFields` already have it), so the rights can be shown read only.
- [ ] T021 [US1] Create `src/lib/actions/settingsAgentProfileActions.ts` with `settings.agents.profiles.assign` (guardrail:
      `scope: 'guardrails'`, `agentCallable: false`, `binding: 'global'`, input `{ subject, profileId }`), list it in
      `src/lib/actions/catalog.ts`, add its handler to `src/stores/settingsActionHandlers.ts` and create
      `src/stores/agentProfiles.ts` (list, subjects, assign) that reloads on `EVENT_AGENTS_CHANGED`.
- [ ] T022 [US1] `src/components/settings/agents/AgentDetailView.vue`: replace the rights editor by one `SettingsSelect`
      "Agentenprofil" (options from `agent_profiles_list`, value the agent's profile) and the profile's rights shown read
      only with the name of their profile (FR-019); mode, log and revoke stay. Test ids `agent-profile-select`.
- [ ] T023 [US1] `src/components/settings/agents/AgentsView.vue` and `src/components/agents/AgentRow.vue`: show the kind label
      (Externer Agent, Abo-Agent) and the profile name in the summary line (FR-007); update the summary of
      `mcpAgents.agents.rights` accordingly in `de.json` and `en.json`.
- [ ] T024 [P] [US1] i18n keys of T021 to T023 in `src/i18n/locales/de.json` and `en.json`.
- [ ] T025 [US1] Add `scripts/e2e/scenarios/agent-profiles.test.ts` with scenario M1 (register an agent, create a profile
      through the action, assign it, check the tool list and a refused call) using `scripts/e2e/lib/mcp-agent.ts`.

**Checkpoint**: US1 works for external agents and Abo-Agents.

---

## Phase 4: User Story 2 - Configure once, for every agent (Priority: P1)

**Goal**: the pages KI → Agentenprofile with the profile editor, "Standard" first, widening asks.

**Independent Test**: quickstart M2.

- [ ] T026 [US2] `agent_profiles/commands.rs`: `agent_profile_get` (profile, rights, members) and `agent_profile_rights_set`
      (run `grants::validate` first; for "Standard" the first edit inserts its profile row; then the store write of T007 and
      the live refresh of T018); export `AgentProfileDetail`; register in the invoke handler.
- [ ] T027 [P] [US2] Create `src/lib/agents/profiles.ts` with the pure `widens(before, after): boolean` (each topic ranks
      none < read < write; passwords by level, then by reach: all entries wider than tags, more tags wider; browser by level for
      all websites, an unblocked or raised exception, a lifted block; commands and each extension by presence), relative `.ts`
      imports only; create `scripts/check-agent-profiles.ts` with its cases and add `"check:agent-profiles": "node --test
    scripts/check-agent-profiles.ts"` to `package.json`.
- [ ] T028 [US2] Register the pages: `src/lib/settings/registry.ts` (`ai.profiles` with `overviewRow: true`, group `agents`, and
      the detail `ai.profiles.profile`, as `ai.agents` and `ai.agents.agent`), `src/components/wm/appRoutes.ts`
      (`defineAsyncComponent` entries), `scripts/check-settings.ts` (path map and group lists), i18n
      `settings.locations.ai.profiles.*` in both languages; extend `scripts/e2e/scenarios/settings-categories.test.ts`.
- [ ] T029 [US2] Create `src/components/settings/profiles/ProfilesView.vue`: a `SettingsGroup` list, "Standard" first, each row
      with name and member count, linking to the profile (member count of "Standard" is for this device and says so).
- [ ] T030 [US2] Create `src/components/settings/profiles/ProfileView.vue`: the profile's `RightsEditor` (draft from
      `rightsDraftOf`, `rightsOf` on change), saving on selection through `settings.agents.profiles.rights.set`; errors as
      toasts; shows the number of agents in the profile (FR-016).
- [ ] T031 [US2] Create `src/components/settings/profiles/WideningConfirm.vue` (dialog) and wire it in `ProfileView.vue`: when
      `widens(before, after)` and the profile has members, ask naming the number ("Das Agentenprofil {name} gilt für {n}
      Agenten.") before saving; cancel restores the control (FR-016, US2 scenario 2); a narrowing change saves at once.
- [ ] T032 [US2] Add the action `settings.agents.profiles.rights.set` to `settingsAgentProfileActions.ts`: input `{ id, rights }`
      with the schema object of the removed-later `settings.agents.mcp.grants.set` plus `requests` and the `blocked` site
      level; handler in `settingsActionHandlers.ts`.
- [ ] T033 [P] [US2] Extend `scripts/check-mcp-actions.ts`: everything `RightsEditor` can emit for a profile is accepted by
      the schema of `settings.agents.profiles.rights.set` (as the browser-row test does for the built-in action).
- [ ] T034 [P] [US2] i18n of T029 to T031 in `de.json` and `en.json`.
- [ ] T035 [US2] Extend `scripts/e2e/scenarios/agent-profiles.test.ts` with scenario M2 (edit "Standard", two agents and a later
      third have the change; a widening change shows the confirmation with the number).
- [ ] T036 [US2] Open PR D1 with T016 to T035 (the first visible PR); PR B (T018 and the remember path, US6) may land before.

**Checkpoint**: US1 and US2 give the MVP: stricter in one step and configured once.

---

## Phase 5: User Story 3 - A profile for some agents only (Priority: P2)

**Goal**: create, rename and delete profiles; add agents to a profile from its page.

**Independent Test**: quickstart M3.

- [ ] T037 [US3] `agent_profiles/commands.rs`: `agent_profile_create`, `agent_profile_rename`, `agent_profile_delete`
      (returns `{ moved }`, moves members to "Standard" in the same write, `standard_fixed`, `name_taken`, `name_empty`), with
      the live refresh after a delete; register in the invoke handler.
- [ ] T038 [P] [US3] Add the actions `settings.agents.profiles.create`, `.rename` and `.delete` (delete is `effect:
    'destructive'`) to `settingsAgentProfileActions.ts` and their handlers; tests in `scripts/check-agent-profiles.ts` that
      the catalog marks all five profile actions as guardrails no agent can call.
- [ ] T039 [US3] `ProfilesView.vue`: a field to create a profile (a name, Enter adds it; error toast for `name_taken`);
      `ProfileView.vue`: rename in the header, delete with a confirm that names how many agents move to "Standard" (FR-009);
      "Standard" shows neither.
- [ ] T040 [US3] Create `src/components/settings/profiles/ProfileMembers.vue` used in `ProfileView.vue`: the agents in the
      profile (name, kind label), and "Agent hinzufügen" listing every subject from `agent_subjects_list` with the name of the
      profile it is in now; choosing one moves it (`settings.agents.profiles.assign`); removing a member assigns it to "Standard".
- [ ] T041 [P] [US3] Rust tests in `agent_profiles/store_tests.rs` and `tests/mcp_server_profiles.rs`: delete moves members and
      returns the count; a name that exists is refused; "Standard" cannot be renamed or deleted; adding an agent that is in
      another profile moves it and the other profile no longer lists it (spec scenario 3).
- [ ] T042 [P] [US3] i18n of T039 and T040; extend `scripts/e2e/scenarios/agent-profiles.test.ts` with scenario M3.

---

## Phase 6: User Story 4 - A new agent registers into "Standard" (Priority: P2)

**Goal**: the registration question shows what the agent would get and lets the user pick a profile.

**Independent Test**: quickstart M4 and M9.

- [ ] T043 [US4] `mcp_server/prompts.rs` and `registration.rs`: `RegistrationQuestion` carries the effective rights of the profile
      the agent would get (the previous assignment for an expired agent, else "Standard"), `profiles: { id, name }[]` and
      `profileId`; `RegistrationAnswer { decision, remember, profileId, mode }` drops `rights`; export the bindings.
- [ ] T044 [US4] `mcp_server/registry.rs` `register`, `register_delegate` and `manage::renew`: write a member row when the chosen
      profile is not "Standard", none otherwise; `renew` keeps the existing assignment (FR-014); remove the compatibility write of
      T011 from these three.
- [ ] T045 [US4] `mcp_server/delegates.rs` `register` and the action `settings.agents.mcp.delegate.register`
      (`src/lib/actions/settingsMcpAgentActions.ts`, handler in `settingsActionHandlers.ts`): input `{ kind, profileId, mode }`
      instead of rights; Abo-Agents show the same field.
- [ ] T046 [US4] `src/components/agents/RegistrationDialog.vue` and `src/stores/mcpAgents.ts`: a `SettingsSelect` "Agentenprofil"
      (preselected from the question), the effective rights of the chosen profile read only (reuse T020), mode and "Ablehnung
      merken" as before; the delegate texts say "Abo-Agent". The answer sends `profileId`.
- [ ] T047 [P] [US4] Update the Rust tests that build a `RegistrationAnswer` or call `register` with rights
      (`mcp_server/prompts_tests.rs`, `registry_tests.rs`, `manage_tests.rs`, `hello_tests.rs`, `src-tauri/tests/mcp_server.rs`,
      `mcp_server_manage.rs`, `mcp_server_delegates.rs`, `scripts/check-mcp-delegate-calls.ts`) and add: registering with a
      profile assigns it, registering into "Standard" writes no member row, renewing an expired agent keeps its profile.
- [ ] T048 [US4] Update `scripts/e2e/scenarios/mcp-agent.test.ts` (it chooses `mcp-right-wm` in the dialog): register by choosing
      a profile that gives "Lesen und ändern" for the topics; extend `agent-profiles.test.ts` with M4 and M9.
- [ ] T049 [P] [US4] i18n of T043 to T046 in `de.json` and `en.json`.

---

## Phase 7: User Story 5 - The built-in agent has one home for its rights (Priority: P2)

**Goal**: the built-in agent's models are assigned to profiles; its tools follow the profile; its settings leave KI → Browser.
Research R6 holds the decisions, among them the full-rights profile of the migration and the notice. Open PR C first with
T050 to T055, then the UI part.

**Independent Test**: quickstart M5 and M7 (built-in part).

- [ ] T050 [US5] `agent_profiles/subjects.rs`: `builtin_subject(app)` from the active chat session (`chat/session.rs`
      `ChatState.session`, `ActiveSession.model_id` and `provider_kind`): a `CliDelegate` provider gives the delegate's
      `Subject::Agent`, any other `Subject::Model(model_id)`; none (no model) is "Standard". Add the models of the built-in agent
      to `list_subjects` (providers and the catalog on this device, plus assigned models that are not offered, marked
      `missing`).
- [ ] T051 [US5] `agent_browser/env.rs`: `rights(&Caller::BuiltinAgent)` and `password_rights` resolve through
      `builtin_subject`; stop reading `builtin_rights` and `builtin_password_rights`; keep the functions for the migration (T054)
      until PR E.
- [ ] T052 [US5] `chat/action_commands.rs` and `chat/tools/action_tool.rs`: register the built-in agent's action tools, its
      `run_command` host tool and its extension tools only where the resolved `Rights` cover them, using the same
      `grants::covers`, `command_tool::offer` and `extension_offers` as the MCP side; recompute when the model changes
      (`chat/send.rs` `select_model`) and on `EVENT_AGENTS_CHANGED`; the approval mode stays as it is (FR-017).
- [ ] T053 [US5] `chat/` notice `chat.agentProfile.restricted` (contracts): once per session when the active model's profile is
      "Standard" with its factory rights or does not cover what the chat asked for; reuse the mechanism of the existing
      `delegateUnregistered` notice (find with `graphify query "delegateUnregistered chat notice"`), texts in `de.json` and
      `en.json`.
- [ ] T054 [US5] `agent_profiles/migrate.rs` step 2: build the built-in `Rights` from `agent_browser.builtin` and
      `agent_browser.builtin_passwords`, "write" in every topic, commands on where the device has the command tool, every
      extension; create the profile "Eingebauter Agent" (`UUIDv5("migrated-builtin:{device}")`, with the device alias appended
      when another device already made one) and assign every model of `chat_messages.model_id` and `chat.last_active_model_id`
      of this device; same marker and idempotence as T013.
- [ ] T055 [P] [US5] Tests: `agent_profiles/migrate_tests.rs` for T054 (settings off gives browser off in the profile; models
      assigned; rerun changes nothing); `src-tauri/tests/mcp_server_profiles.rs` or `agent_browser` scene tests
      (`src-tauri/tests/common/agent_browser_scene.rs`): the local model in a profile with passwords on fills a login, the API
      model in "Standard" cannot and its tool list lacks the topics it has no right for (SC-006); a model that is gone keeps
      its assignment.
- [ ] T056 [US5] `src/components/settings/agentBrowser/AgentBrowserView.vue` and `src/stores/agentBrowser.ts`: remove the "Eingebauter
      Agent" rights and the password rights sections (FR-021); keep the browser choice, trackers and the cookie profile; trim
      `agent_browser_status` in `agent_browser/commands.rs` to what the view still reads and regenerate the bindings.
- [ ] T057 [US5] `AgentsView.vue`: list the built-in agent's models with the kind label "Eingebauter Agent" and their profile (a
      model assignment uses the same select as T022, on a small model row or detail view `/ai/agents/model/<id>`), and let
      `ProfileMembers.vue` add and move them; i18n.
- [ ] T058 [US5] Extend `agent-profiles.test.ts` with M5 and extend `agent-browser.test.ts` where it reads the removed built-in
      settings.

---

## Phase 8: User Story 6 - What a prompt remembers is written into the profile (Priority: P2)

**Goal**: "Für diese Website merken" and the other remembered answers change the profile, and say so. This phase can land as
PR B right after the foundation.

**Independent Test**: quickstart M6.

- [ ] T059 [US6] `agent_browser/env_rights.rs` `change_browser_rights`: resolve the subject (external agent from the caller, built-in
      from `builtin_subject`), apply the closure to the browser group of that subject's profile, write it (materialising
      "Standard" when it has no row), refresh the live sessions (T018) and emit `EVENT_AGENTS_CHANGED`; remove the write to the
      built-in device preference and to `haex_mcp_agent_grants`. This also closes the gap that external rights were written
      without refreshing live sessions.
- [ ] T060 [US6] `agent_browser/prompts.rs` (the question types) and `src/components/agentBrowser/PromptDialog.vue`: the website, fill
      and request-rule questions and the built-in "switch the browser on" question carry the profile name and say "gilt für alle
      Agenten im Agentenprofil {name}" next to "Für diese Website merken"; i18n in both languages (FR-015).
- [ ] T061 [P] [US6] Tests in `agent_browser/env_tests.rs` and `sites_tests.rs` and `src-tauri/tests/`: remembering a website for agent A
      lets agent B of the same profile open it without a question, an agent of another profile still asks, removing it on the
      profile page removes it for both; the same for a remembered fill and a request rule.
- [ ] T062 [US6] Extend `agent-browser.test.ts` or `agent-profiles.test.ts` with M6.

---

## Phase 9: User Story 7 - Everything that exists today keeps working (Priority: P3)

**Goal**: prove that the migration of T013 and T054 gives every agent exactly what it had. The migration code is in the
foundation and in US5; this phase is the evidence.

**Independent Test**: quickstart M7.

- [ ] T063 [US7] Add `src-tauri/tests/agent_profiles_migration.rs`: build a vault with legacy rows for a factory agent, an agent with
      browser, passwords and extensions, an Abo-Agent and built-in preferences, run the migration, and compare for each agent
      the tool list and `grants::check` of a fixed list of calls before and after (nothing more, nothing less); run it twice.
- [ ] T064 [P] [US7] Confirm in `sync/handshake_refusal_tests.rs` that a peer with the old trigger version or migration count is
      refused with `Incompatible` (T004 changed the numbers) and that `specs/059-device-versions` shows the mismatch; note the
      fleet partition in the PR text.
- [ ] T065 [US7] Run quickstart M7 on a copy of the operator's real vault (diagnose on a copy, never the live vault), and record
      the result in the PR.

---

## Phase 10: Polish and cleanup (PR E)

- [ ] T066 Remove the old per-agent rights path: `mcp_agent_rights_set` (`mcp_server/manage_commands.rs`) and the action
      `settings.agents.mcp.grants.set` with its handler; `registry::save_rights`, the compatibility write of T011 and the
      reads of `haex_mcp_agent_grants`; `agent_browser_builtin_set`, `agent_browser_builtin_passwords_set`,
      `builtin_rights`/`set_builtin_rights` and `builtin_password_rights`/`set_builtin_password_rights` in
      `agent_browser/env.rs` and `commands.rs`, the actions `settings.agentBrowser.builtin.set` and
      `settings.agentBrowser.builtinPasswords.set` (`settingsAgentBrowserActions.ts`); regenerate the bindings and drop the
      tests that only covered them. The tables stay (research R8).
- [ ] T067 Naming sweep (FR-022): in `de.json` and `en.json` change the user-facing texts about delegates to "Abo-Agent"
      (the chat notices `delegate*`, `mcpAgents.registration.delegateHint`, the settings entries "Autonomie für Delegaten" and
      "Verbotsregeln für Delegaten", the keywords of the AI category, the action title `register`), keep the keys; leave the
      provider-connection texts (`cliDelegate.*`). Add a case to `scripts/check-agent-profiles.ts` that fails when any of those
      keys contains "Delegat" or "Delegate".
- [ ] T068 [P] Docs: note in `specs/021-agent-control-mcp-server` (data model) and `specs/065-browser-tools/data-model.md` that
      the rights now come from agent profiles (spec 083), and mark T011's compatibility write removed in this tasks file.
- [ ] T069 Full CI parity: `cargo fmt --check`, `pnpm lint:rust` (clippy, both feature sets, `-D warnings`), `cargo test`,
      `pnpm lint`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm format:check`, `pnpm check:templates`,
      `pnpm check:mcp-actions`, `pnpm check:settings`, `pnpm check:agent-actions`, `pnpm check:agent-profiles`.
- [ ] T070 Run quickstart M1 to M9 by hand on desktop and Android (a real phone for the settings pages) and tick the results.

---

## Dependencies and order

- **Phase 1 then Phase 2** block everything. PR A is T003 to T015.
- **US1** needs the foundation. **US2** needs US1's commands and store (T017, T021) but not its agent-page work. **US3** needs US2's
  pages. **US4** needs the foundation only. **US5** needs the foundation, US1's subjects (T016, T017) and the live refresh (T018).
  **US6** needs the foundation and T018; it can ship as PR B before any UI. **US7** is evidence after T013 and T054.
- **Phase 10** is last; T066 must not land before US1 to US5 have replaced every caller.
- Delivery as in plan.md: A (T003 to T015), B (T018, T059 to T062), C (T050 to T055, T063), D (T016 to T049 minus the above, T056, T057),
  E (T066 to T070).

## Parallel opportunities

- Inside the foundation: T005, T009 and T014 (tests) beside T006 to T008 once the types exist.
- After the foundation: US4 (T043 to T049) and US6 (T059 to T062) are independent of the UI work of US1 to US3.
- Frontend and Rust of one story: the i18n tasks (T024, T034, T042, T049) and the `disabled` task (T020) touch other files than the
  Rust tasks.

## Implementation strategy

- **MVP**: Phases 1 and 2, then US1 and US2 (the two P1 stories): rights come from profiles, an agent can be moved to another profile
  in one step and "Standard" is edited once. Stop and validate with quickstart M1, M2 and M7.
- **Then**: US6 (small, closes the live-refresh gap), US4, US3, US5 (the largest risk, it gates the built-in agent; keep it its own PR with
  the migration test of T063 passing first), US7 evidence, cleanup.
- **Order of risk**: T010 and T011 (changing where rights come from) and T052 (gating the built-in agent) are the two places where a
  mistake changes what an agent may do; both have a test that compares before and after.
