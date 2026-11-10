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
(configure once) are P1; US3 (a profile for some agents), US4 (registration), US5 (local agent) and US6 (remembered
answers) are P2; US7 (the reset) is P3: there is no migration, the old rights simply stop counting (spec 083, 2026-11-06). The PR letters of plan.md are marked on the phases.

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

- [x] T001 Run `graphify update .` in the worktree root so later queries see the current tree.
- [x] T002 Check that migration number 0054 is free: list `src-tauri/src/identity/migrations*.rs` on `origin/main` and in
      every open worktree and branch (`git worktree list`, `git branch -r`); if another branch took 0054, take the next
      number everywhere below and add the entry to `src-tauri/src/identity/migrations_legacy_names.rs` for the later one.

---

## Phase 2: Foundation (PR A, blocks every story)

**Purpose**: the tables, the store, the resolver, the strict converter and the refresh of live sessions. Purely additive:
nothing reads profiles yet, so nothing changes for the user. The switch of the rights source is PR B (T010). There is no
migration of old rights (spec 083 FR-024, research R7): they stop counting in PR B and are dropped in PR E.

- [x] T003 Create `src-tauri/src/identity/migrations_agent_profiles.rs` with the SQL const for migration
      `0054_agent_profiles`: tables `haex_agent_profiles` (`id` TEXT PK, `name` TEXT NOT NULL),
      `haex_agent_profile_grants` (`id` TEXT PK, `profile_id` TEXT NOT NULL, `grant_group` TEXT NOT NULL, `grant_key` TEXT
      NOT NULL DEFAULT '', `level` TEXT NOT NULL, `scope_tags` TEXT) and `haex_agent_profile_members` (`id` TEXT PK,
      `subject` TEXT NOT NULL, `profile_id` TEXT NOT NULL). Per data-model.md: synced CRDT tables, no
      `vault_device_uuid`, no UNIQUE besides the primary key (a conflict halts the sync), no cascading foreign key. Header
      comment explains the vault-wide scope against the device-scoped agent tables (ADR-0001) and cites spec 083.
- [x] T004 Register the migration in `src-tauri/src/identity/migrations.rs` (`holzi_migration_source()`, after
      `0053_sync_rules_source_target`), bump `HOLZI_TRIGGER_VERSION` from 34 to 35 with a line in its history comment, and
      check how a new synced table gets its CRDT triggers (follow `migrations_mcp_agents.rs`); update every test that pins
      the migration count or the trigger version (`identity/migrations_tests.rs`, `sync/handshake_refusal_tests.rs`).
- [x] T005 [P] Create `src-tauri/src/identity/migrations_agent_profiles_tests.rs`: the three tables exist with exactly
      the columns of T003, a second member row with the same `id` is refused by the primary key, a delete leaves a marker
      in `haex_deleted_rows`; model it on `migrations_mcp_agents_tests.rs`.
- [x] T006 Create `src-tauri/src/agent_profiles/mod.rs` (declare the module in `src-tauri/src/lib.rs`): `Subject` with
      `Agent(Uuid)` and `Model(String)`, `Subject::key()` giving `agent:<uuid>` / `model:<id>` and `Subject::parse`;
      `STANDARD_PROFILE_ID` (one fixed UUID written as a constant, never generated at runtime); namespaces
      `NS_PROFILE_MEMBER` and `NS_PROFILE_GRANT`; `member_id(subject)` = UUIDv5 of the subject key,
      `grant_id(profile, group, key)` = UUIDv5 of `{profile}:{group}:{key}` (data-model.md).
- [x] T007 Create `src-tauri/src/agent_profiles/store.rs`: reads and writes of the three tables through the CRDT write
      path used by `mcp_server/registry.rs` (`save_rights` is the model): list and get profiles, insert, rename, replace
      the grant rows of a profile in one write (DELETE then INSERT, rows built by the existing
      `mcp_server/grant_rows.rs` `rows_of`), upsert a member row by `member_id`, delete a member row, delete a profile
      (its grant rows, its member rows and the profile row in one write, each an explicit delete so it leaves a marker),
      the name check (`name_empty` only; names may repeat, FR-002), "Standard" cannot be renamed or deleted.
- [x] T008 Create `src-tauri/src/agent_profiles/resolve.rs`: `rights_of(db, subject) -> Rights` as data-model.md
      "Resolution": the member row's profile if that profile exists, else "Standard"; "Standard" without a profile row
      answers `grants::default_rights()`; the rows go through the existing `grant_rows::rights_of`, which skips what it
      cannot read; a member row with an unreadable field gives no rights (FR-012). Make the shared converter strict where it
      is not today (research R9): in `mcp_server/grant_rows.rs` `rights_of`, `commands` counts only with the empty key and
      `operate` keeps only keys of `grants::OPERATE_SCOPES`; cases in `grant_rows_tests.rs` (a `commands`/`shell` row and an
      `operate`/`files.shares` row give no right).
- [x] T009 [P] Create `src-tauri/src/agent_profiles/store_tests.rs` and `resolve_tests.rs`: Standard is the factory
      rights while it has no row; the first stored Standard replaces them; a dangling profile id means Standard; unknown
      group or level rows are skipped; assigning the same subject twice leaves one member row; deleting a profile moves
      its members to Standard and leaves markers; two profiles may have the same name, an empty name is refused and "Standard" is fixed.
- [x] T010 (PR B, the switch) Make `mcp_server/registry.rs` `load_rights` resolve through
      `agent_profiles::resolve::rights_of(Subject::Agent(id))` keeping its signature, so the callers stay as they are:
      `connection.rs`, `attach.rs`, `delegates.rs`, `oauth_rows.rs`, `password_env.rs`, `manage.rs` (`list_with_rights`),
      `manage_commands.rs` (`mcp_agent_get`), `agent_browser/env.rs` (`rights`, `password_rights`). From here no code reads
      `haex_mcp_agent_grants`; every registered agent is in "Standard" until a profile is assigned (FR-024). Together with the
      writers of T022, T043 to T046 and T059, because the old inputs have nowhere to write any more.
- [x] ~~T011~~ Dropped 2026-11-06: no interim write into a profile of the agent's own, and no migration. A profile belongs to
      many agents; nothing creates one per agent (research R7, R8).
- [x] T012 `mcp_server/manage.rs` `revoke`: in the same write also delete the agent's member row
      (`Subject::Agent(id)`), as an explicit delete with its marker; the profile stays. Done in PR A.
- [x] ~~T013~~ Dropped 2026-11-06: no post-open migration of agents into profiles (research R7).
- [x] ~~T014~~ Dropped 2026-11-06, with T013.
- [x] T015 Run `cargo test -j 4 --lib -- agent_profiles identity::migrations mcp_server` and the
      `src-tauri/tests/mcp_server_*.rs` integration tests; all existing tests must still pass unchanged apart from
      T004. Open PR A.

**Checkpoint**: the tables, store and resolver exist and are tested; nothing reads them yet.

---

## Phase 3: User Story 1 - Make an agent stricter in one step (Priority: P1)

**Goal**: assign an agent to a profile and see its next call limited, in every kind of access at once.

**Independent Test**: quickstart M1.

- [x] T016 [US1] Create `src-tauri/src/agent_profiles/subjects.rs`: `list_subjects(db)` returning the agents of every device
      (`haex_mcp_agents` is synced) with `kind` (`external` for `AgentKind::External`, `subscription` for the two delegate
      kinds, per `mcp_server/registry.rs`), name, device alias, status and the profile id and name from the resolver; and the
      member count of each profile, vault wide (data-model.md, Member count).
- [x] T017 [US1] Create `src-tauri/src/agent_profiles/commands.rs` with `agent_profiles_list`, `agent_subjects_list` and
      `agent_assign` as in contracts/commands-and-actions.md (`profile_missing` for an unknown profile; "Standard" deletes
      the member row); register them in the Tauri invoke handler (find with
      `graphify query "tauri invoke handler mcp_agent_rights_set"`); export `AgentProfileView` and `AgentSubjectView` with ts-rs and regenerate the bindings.
- [x] T018 [US1] (done in `attach::tell_catalog_changes`, which re-resolves per session under `synchronize`; `live.rs` stays unchanged) `mcp_server/live.rs`: add a refresh that re-resolves the rights of every live session through
      `registry::load_rights`, serialised per agent by the existing `synchronize`, updates the session (as `set_rights` does)
      and sends `tools/list_changed` where the rights differ. Trigger it from the table feed, not from each writer (research
      R5): `attach::tell_catalog_changes` (or one server-wide subscriber of `core.vault_changes`) runs it when a commit
      touches `haex_agent_profiles`, `haex_agent_profile_grants` or `haex_agent_profile_members`, or when the feed lagged.
      That covers local writes and rows that arrive by sync. The commands only emit `EVENT_AGENTS_CHANGED`. The trigger in
      `attach::tell_catalog_changes` landed with PR A (33e1ac1f); what is left is checked against this text in PR B.
- [x] T019 [P] [US1] Add `src-tauri/tests/mcp_server_profiles.rs`: two live agents in "Standard"; assign one to a profile that
      grants nothing; its tool list shrinks and a browser or password call is refused, the other agent is unchanged; assign it
      back and it has the factory rights again; a member row written straight into the database, as a synced commit
      arrives, refreshes the session too (FR-006, FR-010, FR-011, M1, M8b).
- [x] T020 [P] [US1] Add `disabled` to `src/components/agents/PasswordRightsFields.vue`, `CommandRightsFields.vue`,
      `ExtensionRightsFields.vue`, `RightsInset.vue` children and `RightsEditor.vue` (`RightsFields` and
      `BrowserRightsFields` already have it), so the rights can be shown read only.
- [x] T021 [US1] Create `src/lib/actions/settingsAgentProfileActions.ts` with `settings.agents.profiles.assign` (guardrail:
      `scope: 'guardrails'`, `agentCallable: false`, `binding: 'global'`, input `{ subject, profileId }`), list it in
      `src/lib/actions/catalog.ts`, add its handler to `src/stores/settingsActionHandlers.ts` and create
      `src/stores/agentProfiles.ts` (list, subjects, assign) that reloads on `EVENT_AGENTS_CHANGED`.
- [x] T022 [US1] `src/components/settings/agents/AgentDetailView.vue`: replace the rights editor by one `SettingsSelect`
      "Agentenprofil" (options from `agent_profiles_list`, value the agent's profile) and the profile's rights shown read
      only with the name of their profile (FR-019); mode, log and revoke stay. Test ids `agent-profile-select`. Rewrite the
      three tests of `scripts/check-mcp-actions.ts` that replay this page's rights editor ("the agent page can save every rights
      group through its action", "instant password saves revoke access and keep the empty draft across reloads" and the
      reload case of "revoking an agent shows no error toast", which use `onDraft` and `reloadAsync`) for the profile select.
- [x] T023 [US1] `src/components/settings/agents/AgentsView.vue` and `src/components/agents/AgentRow.vue`: show the kind label
      (Externer Agent, Abo-Agent) and the profile name in the summary line (FR-007); update the summary of
      `mcpAgents.agents.rights` accordingly in `de.json` and `en.json`.
- [x] T024 [P] [US1] i18n keys of T021 to T023 in `src/i18n/locales/de.json` and `en.json`.
- [x] T025 [US1] Add `scripts/e2e/scenarios/agent-profiles.test.ts` with scenario M1 (register an agent, create a profile
      through the action, assign it, check the tool list and a refused call) using `scripts/e2e/lib/mcp-agent.ts`.

**Checkpoint**: US1 works for external agents and Abo-Agents.

---

## Phase 4: User Story 2 - Configure once, for every agent (Priority: P1)

**Goal**: the pages KI → Agentenprofile with the profile editor, "Standard" first, widening asks.

**Independent Test**: quickstart M2.

- [x] T026 [US2] `agent_profiles/commands.rs`: `agent_profile_get` (profile, rights, members) and `agent_profile_rights_set`
      (run `grants::validate` first; for "Standard" the first edit inserts its profile row; then the store write of T007 and
      the table feed refreshes the sessions, T018); export `AgentProfileDetail`; register in the invoke handler.
- [x] T027 [P] [US2] Create `src/lib/agents/profiles.ts` with the pure `widens(before, after): boolean`, true when `after`
      gives anything `before` does not, over every field of `Rights` (research R11): each topic ranks none < read < write;
      passwords by level, then by reach (all entries wider than tags, more tags wider) and a new default tag; browser by
      off < on, `userBrowser`, the level for all websites, every per-site level (new, raised or unblocked),
      `fillRemembered`, and every request rule new at `allow` or turned to `allow`; commands and each extension by
      presence. Relative `.ts` imports only; create `scripts/check-agent-profiles.ts` with its cases, including one that
      enumerates the fields of `Rights` and fails for a field `widens` does not compare, and add
      `"check:agent-profiles": "node --test scripts/check-agent-profiles.ts"` to `package.json`.
- [x] T028 [US2] Register the pages: `src/lib/settings/registry.ts` (`ai.profiles` with `overviewRow: true`, group `agents`, and
      the detail `ai.profiles.profile`, as `ai.agents` and `ai.agents.agent`), `src/components/wm/appRoutes.ts`
      (`defineAsyncComponent` entries), `scripts/check-settings.ts` (path map and group lists), i18n
      `settings.locations.ai.profiles.*` in both languages; extend `scripts/e2e/scenarios/settings-categories.test.ts`.
- [x] T029 [US2] Create `src/components/settings/agentProfiles/ProfilesView.vue`: a `SettingsGroup` list, "Standard" first,
      each row with name and member count (vault wide; for "Standard" the unassigned agents of every device and models),
      linking to the profile. The label "Standard" is translated (the stored name is ignored). Profiles with the same name show their member counts, so they can be told apart (FR-002).
- [x] T030 [US2] Create `src/components/settings/agentProfiles/ProfileView.vue`: the profile's `RightsEditor` (draft from
      `rightsDraftOf`, `rightsOf` on change), saving on selection through `settings.agents.profiles.rights.set`; errors as
      toasts; shows the number of agents in the profile (FR-016).
- [x] T031 [US2] Create `src/components/settings/agentProfiles/WideningConfirm.vue` (dialog) and wire it in `ProfileView.vue`:
      when `widens(before, after)` and the profile has members, and for "Standard" on every widening even without members (it
      also holds every agent that registers later), ask naming the number ("Das Agentenprofil {name} gilt für {n}
      Agenten.") before saving; cancel restores the control (FR-016, US2 scenario 2); a narrowing change saves at once, in "Standard" too.
- [x] T032 [US2] Add the action `settings.agents.profiles.rights.set` to `settingsAgentProfileActions.ts`: input `{ id, rights }`
      with the schema object of the removed-later `settings.agents.mcp.grants.set` plus `requests` and the `blocked` site
      level; handler in `settingsActionHandlers.ts`.
- [x] T033 [P] [US2] Extend `scripts/check-mcp-actions.ts`: everything `RightsEditor` can emit for a profile is accepted by
      the schema of `settings.agents.profiles.rights.set` (as the browser-row test does for the built-in action).
- [x] T034 [P] [US2] i18n of T029 to T031 in `de.json` and `en.json`.
- [x] T035 [US2] Extend `scripts/e2e/scenarios/agent-profiles.test.ts` with scenario M2 (edit "Standard", two agents and a later
      third have the change; a widening change shows the confirmation with the number).
- [x] T036 [US2] Open PR B with T010, T016 to T049 (US1 to US4 for external agents and Abo-Agents), T059 to T062 (US6 for
      external agents) and T066: the switch and the pages in one PR, because the old rights inputs have nowhere to write
      once T010 lands. T056 and T057 (the local agent) are PR C. PR A lands before it.

**Checkpoint**: US1 and US2 give the MVP: stricter in one step and configured once.

---

## Phase 5: User Story 3 - A profile for some agents only (Priority: P2)

**Goal**: create, rename and delete profiles; add agents to a profile from its page.

**Independent Test**: quickstart M3.

- [x] T037 [US3] `agent_profiles/commands.rs`: `agent_profile_create`, `agent_profile_rename`, `agent_profile_delete`
      (returns `{ moved }`, moves members to "Standard" in the same write, `standard_fixed`, `name_empty`); the
      table feed refreshes the sessions (T018); register in the invoke handler.
- [x] T038 [P] [US3] Add the actions `settings.agents.profiles.create`, `.rename` and `.delete` (delete is
      `effect: 'destructive'`) to `settingsAgentProfileActions.ts` and their handlers; tests in `scripts/check-agent-profiles.ts` that
      the catalog marks all five profile actions as guardrails no agent can call.
- [x] T039 [US3] `ProfilesView.vue`: a field to create a profile (a name, Enter adds it; an empty name is refused with a toast);
      `ProfileView.vue`: rename in the header, delete with a confirm that names how many agents move to "Standard" (FR-009);
      "Standard" shows neither.
- [x] T040 [US3] Create `src/components/settings/agentProfiles/ProfileMembers.vue` used in `ProfileView.vue`: the agents in the
      profile (name, kind label), and "Agent hinzufügen" listing every subject from `agent_subjects_list` with the name of the
      profile it is in now; choosing one moves it (`settings.agents.profiles.assign`); removing a member assigns it to "Standard".
- [x] T041 [P] [US3] Rust tests in `agent_profiles/store_tests.rs` and `tests/mcp_server_profiles.rs`: delete moves members and
      returns the count; a name another profile has is accepted; "Standard" cannot be renamed or deleted; adding an agent that is in
      another profile moves it and the other profile no longer lists it (spec scenario 3).
- [x] T042 [P] [US3] i18n of T039 and T040; extend `scripts/e2e/scenarios/agent-profiles.test.ts` with scenario M3.

---

## Phase 6: User Story 4 - A new agent registers into "Standard" (Priority: P2)

**Goal**: the registration question shows what the agent would get and lets the user pick a profile.

**Independent Test**: quickstart M4 and M9.

- [x] T043 [US4] `mcp_server/prompts.rs` and `registration.rs`: `RegistrationQuestion` carries the effective rights of the profile
      the agent would get (the previous assignment for an expired agent, else "Standard"), `profiles: { id, name }[]` and
      `profileId`; `RegistrationAnswer { decision, remember, profileId, mode }` drops `rights`; an answer whose `profileId`
      names no profile any more is refused with `profile_missing` and the question stays open; export the bindings. The OAuth
      consent of HTTP agents (`mcp_server/oauth_flow.rs`, `oauth_rows::previous`, `oauth_rows::authorize`) uses the same
      question and answer and gets the same change; drop its `live.set_rights` call (the feed refreshes).
- [x] T044 [US4] `mcp_server/registry.rs` `register`, `register_delegate`, `manage::renew` and `oauth_rows::authorize`: write a
      member row when the chosen profile is not "Standard", none otherwise; `renew` keeps the existing assignment unless the
      answer chose another (FR-014); the four no longer write rights.
- [x] T045 [US4] (done: the action keeps the input `{ kind }`; the command only opens the dialog and the profile is chosen there) `mcp_server/delegates.rs` `register` and the action `settings.agents.mcp.delegate.register`
      (`src/lib/actions/settingsMcpAgentActions.ts`, handler in `settingsActionHandlers.ts`): input `{ kind, profileId, mode }`
      instead of rights; Abo-Agents show the same field.
- [x] T046 [US4] `src/components/agents/RegistrationDialog.vue` and `src/stores/mcpAgents.ts`: a `SettingsSelect` "Agentenprofil"
      (preselected from the question), the effective rights of the chosen profile read only (reuse T020), mode and "Ablehnung
      merken" as before; the delegate texts say "Abo-Agent". The answer sends `profileId`.
- [x] T047 [P] [US4] Update the Rust tests that build a `RegistrationAnswer` or call `register` with rights
      (`mcp_server/prompts_tests.rs`, `registry_tests.rs`, `manage_tests.rs`, `hello_tests.rs`, `src-tauri/tests/mcp_server.rs`,
      `mcp_server_manage.rs`, `mcp_server_delegates.rs`, `mcp_server_passwords.rs` (it reads `registry::load_rights`) and the fixture
      `registered(slug, rights)` in `src-tauri/tests/common/mcp_fixture.rs`, which writes rights per agent and so changes with
      T010 (it creates a profile with the rights and assigns it), `scripts/check-mcp-delegate-calls.ts`) and add: registering with a
      profile assigns it, registering into "Standard" writes no member row, renewing an expired agent keeps its profile.
- [x] T048 [US4] (M9 is not an e2e scenario: the rig cannot age a registration; the Rust test `a_registration_without_a_connection_for_ninety_days_expires` covers it) Update `scripts/e2e/scenarios/mcp-agent.test.ts` (it chooses `mcp-right-wm` in the dialog): register by choosing
      a profile that gives "Lesen und ändern" for the topics; extend `agent-profiles.test.ts` with M4 and M9.
- [x] T049 [P] [US4] i18n of T043 to T046 in `de.json` and `en.json`.

---

## Phase 7: User Story 5 - The local agent has one home for its rights (Priority: P2)

**Goal**: the local agent's models are assigned to profiles; its tools follow the profile; its settings leave KI → Browser.
Research R6 holds the decisions, among them that every model starts in "Standard" (there is no migration) and the notice. Open PR C first with
T050 to T055, then the UI part.

**Independent Test**: quickstart M5 and M7 (the local agent part).

- [x] T050 [US5] (done as `agent_profiles/local.rs`: `subject_of_session`, `builtin_subject`, `current_rights`; a CLI-delegate session has no subject there, the delegate is judged over MCP) `chat/tools/agent_task.rs`: `AgentTask` also carries the turn's `ActiveSession.model_id` and `provider_kind`
      (`AgentTask::of_turn`). `agent_profiles/subjects.rs`: `builtin_subject()` reads the task-local `AgentTask`, never the
      global `ChatState.session` (research R6): a `CliDelegate` provider gives the delegate's `Subject::Agent`, any other
      `Subject::Model(model_id)`; no task (a call outside a turn) is "Standard". Add the models of the local agent
      to `list_subjects` (providers and the catalog on this device, plus assigned models that are not offered, marked
      `missing`).
- [x] T051 [US5] `agent_browser/env.rs`: `rights(&Caller::BuiltinAgent)` and `password_rights` resolve through
      `builtin_subject`; stop reading `builtin_rights` and `builtin_password_rights`, and remove them (T054). Also `agent_browser/env_rights.rs` `change_browser_rights` and the built-in "switch the browser on" prompt
      (`switch_on`, with its text in `agent_browser/prompts.rs` and `PromptDialog.vue`): for the local agent they resolve
      `builtin_subject`, write its profile and no longer write the device preference (the external part is T059, PR B); the
      prompts name the profile (FR-015).
- [x] T052 [US5] Gate the local agent in the turn, not at registration (research R6): `chat/action_commands.rs` keeps
      registering every built-in tool into the shared `tool_registry`. The offer of each step (`chat/tools/offer.rs`,
      `availability.rs`) leaves out the action tools, the `run_command` host tool and the extension tools the subject's
      resolved `Rights` do not cover, using the same `grants` functions, `command_tool::offer` and `extension_offers` as the
      MCP side. The dispatch of each call in `chat/turn/` re-resolves and refuses an uncovered call with a denial, as
      `grants::check` does. The local agent gets its own `Reach` (the native executors that serve it), and `files.*`,
      `files.shares` and `contacts.*` keep today's gate (`agent_file_permissions` for `builtin`, Stage 1). The approval mode
      stays as it is (FR-017).
- [x] T053 [US5] `chat/` notice `chat.agentProfile.restricted.*` (contracts): once per session and right when the profile of
      the turn's model does not cover what the chat asked for, naming the profile and the missing right (browser, password
      fill, a topic at write, commands, an extension); reuse the mechanism of the existing
      `delegateUnregistered` notice (find with `graphify query "delegateUnregistered chat notice"`), texts in `de.json` and
      `en.json`.
- [x] T054 [US5] The local agent's old settings are removed, not migrated (research R7): a maintenance step in
      `storage/maintenance.rs` `run_after_open`, next to step 2 (the legacy `shell.active_workspace_id` delete is the precedent),
      deletes the preferences `agent_browser.builtin` and `agent_browser.builtin_passwords` for every device (a synced
      delete, idempotent, each leaves a marker). From T051 on every model resolves to its profile, "Standard" for a model
      in none. Remove `builtin_rights`, `set_builtin_rights`, `builtin_password_rights` and `set_builtin_password_rights` from
      `agent_browser/env.rs`, `agent_browser_builtin_set`, `agent_browser_builtin_passwords_set` from `agent_browser/commands.rs` and
      the actions `settings.agentBrowser.builtin.set` and `settings.agentBrowser.builtinPasswords.set`
      (`settingsAgentBrowserActions.ts`); regenerate the bindings.
- [x] T055 [P] [US5] Tests: the maintenance step of T054 (the two preferences of two devices are gone, a second run changes
      nothing, other preferences stay); `src-tauri/tests/mcp_server_profiles.rs` or `agent_browser` scene tests
      (`src-tauri/tests/common/agent_browser_scene.rs`): the local model in a profile with passwords on fills a login, the API
      model in "Standard" cannot and its tool list lacks the topics it has no right for (SC-006); a model that is gone keeps
      its assignment.
- [x] T056 [US5] `src/components/settings/agentBrowser/AgentBrowserView.vue` and `src/stores/agentBrowser.ts`: remove the "Lokaler Agent" rights and the password rights sections (FR-021); keep the browser choice, trackers and the cookie profile; trim
      `agent_browser_status` in `agent_browser/commands.rs` to what the view still reads and regenerate the bindings.
- [x] T057 [US5] `AgentsView.vue`: list the local agent's models with the kind label "Lokaler Agent" and their profile (a
      model assignment uses the same select as T022, on a small model row or detail view `/ai/agents/model/<id>`), and let
      `ProfileMembers.vue` add and move them; i18n.
- [x] T058 [US5] Extend `agent-profiles.test.ts` with M5 and extend `agent-browser.test.ts` where it reads the removed built-in
      settings.
- [x] T071 [US5] The right "alle Erweiterungen" (plan Q3): `Rights.allExtensions` (Rust `mcp_server/grants.rs`, row `extension` /
      key `*` at level `on` in `grant_rows.rs`, skipped by older builds), `grants::extension_offers` and `check_extension_tool`
      cover every extension while it is on, a row "Alle Erweiterungen" at the top of `src/components/agents/ExtensionRightsFields.vue`
      that greys out the single ones, `src/lib/agents/rights.ts` and `widens` (T027: on is wider than any single extension),
      bindings regenerated, the schema of `settings.agents.profiles.rights.set`, i18n; tests in `grants_tests.rs`,
      `grant_rows_tests.rs`, `scripts/check-agent-profiles.ts`. Lands in PR C.

---

## Phase 8: User Story 6 - What a prompt remembers is written into the profile (Priority: P2)

**Goal**: "Für diese Website merken" and the other remembered answers change the profile, and say so. This phase can land as
PR B right after the foundation.

**Independent Test**: quickstart M6.

- [x] T059 [US6] `agent_browser/env_rights.rs` `change_browser_rights`: for an external agent apply the closure to the browser group of its profile (the local agent follows in T051, PR C; until
      then it keeps writing its device preference), write it (materialising
      "Standard" when it has no row; the table feed refreshes the live sessions, T018) and emit `EVENT_AGENTS_CHANGED`;
      remove the write to `haex_mcp_agent_grants` for external agents. This also closes the gap that external rights were written
      without refreshing live sessions.
- [x] T060 [US6] `agent_browser/prompts.rs` (the question types) and `src/components/agentBrowser/PromptDialog.vue`: the website, fill
      and request-rule questions carry the profile name and say "gilt für alle
      Agenten im Agentenprofil {name}" next to "Für diese Website merken"; an answer that widens the profile names its member
      count first, for "Standard" always (FR-016; the local agent's prompts follow in T051); i18n in both languages (FR-015).
- [x] T061 [P] [US6] Tests in `agent_browser/env_tests.rs` and `sites_tests.rs` and `src-tauri/tests/`: remembering a website for agent A
      lets agent B of the same profile open it without a question, an agent of another profile still asks, removing it on the
      profile page removes it for both; the same for a remembered fill and a request rule.
- [ ] T062 [US6] (open: the e2e rig cannot yet drive a website prompt for an external agent in a profile; the Rust tests of T061 cover the behaviour) Extend `agent-browser.test.ts` or `agent-profiles.test.ts` with M6.

---

## Phase 9: User Story 7 - The update resets what agents may do (Priority: P3)

**Goal**: prove that after the update nothing from before counts: every agent is in "Standard", registrations stay, and the
reset is not applied twice. There is no migration code; this phase is the evidence.

**Independent Test**: quickstart M7.

- [ ] T063 [US7] Add `src-tauri/tests/agent_profiles_reset.rs`: a vault with old rows in `haex_mcp_agent_grants` for an agent with
      browser, passwords and extensions and for an Abo-Agent, and the two old preferences of the local agent; after PR B and PR C:
      each agent has exactly the factory rights of "Standard", its registration (credential hash, mode, status) is unchanged, a
      profile the user made after the update is not touched by a second start, and the old preferences are gone.
- [ ] T064 [P] [US7] Confirm in `sync/handshake_refusal_tests.rs` that a peer with the old trigger version or migration count is
      refused with `Incompatible` (T004 changed the numbers) and that `specs/059-device-versions` shows the mismatch; note the
      fleet partition in the PR text.
- [ ] T065 [US7] Run quickstart M7 on a copy of the operator's real vault (diagnose on a copy, never the live vault): agents are
      registered and in "Standard", the local agent's models too; record the result in the PR.

---

## Phase 10: Polish and cleanup (PR E)

- [x] T066 (PR B) Remove the old per-agent rights inputs, which have nothing to write to after T010: `mcp_agent_rights_set`
      (`mcp_server/manage_commands.rs`) and the action `settings.agents.mcp.grants.set` with its handler; `registry::save_rights`
      and its callers' rights parameters (`register`, `register_delegate`, `manage::renew`, `oauth_rows::authorize` take a profile
      instead, T044); regenerate the bindings and drop the tests that only covered them. The fixture `registered(slug, rights)`
      in `src-tauri/tests/common/mcp_fixture.rs` then creates a profile with the rights and assigns it. The table
      `haex_mcp_agent_grants` stays until T072.
- [x] T072 (PR E; migration number 0056, because 0055 became the profile indexes) Drop the table `haex_mcp_agent_grants`: migration `0056_drop_agent_grants` (DDL only, a drop leaves no markers;
      register it in `migrations.rs`, `HOLZI_TRIGGER_VERSION` 36 with its history line; migration `0045` stays as it is). Check
      with `git grep haex_mcp_agent_grants` that no code reads it; migration test next to
      `migrations_agent_profiles_tests.rs`.

---

## Phase 11: Fixes after the first manual check (PR G)

- [x] T073 The list of profiles: a "+" button beside the name field (disabled while the name is empty), the hints "name may repeat"
      and "allows nothing at first" removed, the introduction says that "Standard" applies to all agents and exactly one profile
      applies to an agent (FR-018). `ProfilesView.vue`, `de.json`, `en.json`.
- [x] T074 Every local model is listed (FR-007): `agent_profiles/subjects.rs` lists all rows of the local chat provider whatever the state
      of the file here (a row without a provider counts as local with a file hash), `AgentSubjectView.local`, the agent page lists them as
      rows and keeps API models in the pick list; the test in `subjects_tests.rs`.
- [x] T075 "Alle Erweiterungen" stays usable without installed extensions (FR-020a): the switch is not disabled in `ExtensionRightsFields.vue`
      (an earlier draft of this PR disabled it; the user wants to configure it beforehand).
- [x] T076 `ProfileView.vue`: keep the draft while a save is queued, and ignore the closing report of the widening dialog after an answer
      (the switch went back to off after the confirmation).
- [x] T077 Decided 2026-11-07 for autonomy mode and deny rules: into the profile (PR J, T101). The file and contact grants:
      decided 2026-11-08, see phase 15 (PR K).

---

## Phase 12: Several profiles per agent, one active (PR H)

**Goal**: an agent or model is a member of any number of profiles; exactly one profile is active for the vault, and the
user sets it from the list of profiles (FR-006, FR-006a, FR-023, SC-003). Decided 2026-11-06 after the first manual check.

- [x] T078 [US3] Migration `0057_agent_memberships` (the non-unique index by subject, `HOLZI_TRIGGER_VERSION` 37) and the reading of legacy ids (`legacy_member_id`): the rows that exist keep their ids and stay memberships, never rewritten, so a stale
      device cannot bring back a removed membership (data-model.md, Migration 0057); tests that an existing assignment stays a
      membership, that adding it again writes nothing and that two memberships of one subject are two rows.
- [x] T079 [US3] `agent_profiles/store.rs`: `add_member` and `remove_member` per `{subject, profile}` (replacing `assign`),
      `members_of`, `memberships_of(subject)`, deletion of a profile and of an agent removes all their rows; the vault preference
      `agent_profile.active` (get, set, "Standard" when unset, unreadable or gone). Tests in `store_tests.rs`.
- [x] T080 [US3] `agent_profiles/resolve.rs`: `rights_of(subject)` and `profile_of(subject)` per data-model.md Resolution (active
      profile if the subject is a member of it or it is "Standard", else "Standard"); `local.rs` and the browser and password
      environment follow through the resolver only. Tests: a member of the active profile, a non-member, the active profile
      deleted, the preference unreadable.
- [x] T081 [US3] Commands and actions: `agent_membership_set`, `agent_profile_active_get`, `agent_profile_active_set`,
      `settings.agents.profiles.activate`, `.assign` with `member`; `agent_assign` removed (the command is `agent_membership_set`); `AgentProfileView.isActive`;
      `AgentSubjectView.profiles` and `activeProfileId`; bindings; schemas and `scripts/check-mcp-actions.ts`. All
      guardrails, no agent can call them.
- [x] T082 [US3] The live refresh: the MCP session re-resolution and the local agent's gate take the preference key
      `agent_profile.active` from the feed (`attach::tell_catalog_changes`); a test that a change of the active profile changes
      a running session's tool list and gives `Notice::BrowserRightLost` where the browser right goes.
- [x] T083 [US3] The list of profiles (`ProfilesView.vue`): the active one marked, a way to make another active in every row,
      the question when the change gives members more (names how many); the page of a profile (`ProfileView.vue`) the same,
      and "Aktiv" next to its name.
- [x] T084 [US3] `ProfileMembers.vue` and the page of an agent (`AgentDetailView.vue`, `AgentModelView.vue`): the profiles of an
      agent as a list of switches (members), the active profile marked, the effective rights read-only; adding an agent never
      shows "jetzt in {profile}" and never removes it elsewhere. The registration question (`RegistrationDialog.vue`): a
      multiple choice of profiles. i18n of all of it.
- [x] T085 [US3] Remembered answers (`remember.rs`) go into the profile that applies to the agent (the active one it is a member
      of, else "Standard"); the prompts name it. Tests.
- [x] T086 [US3] `scripts/e2e/scenarios/agent-profiles.test.ts`: M3 and M8 as in quickstart.md; update `check-agent-profiles.ts`.
- [x] T087 No question for extensions and commands (FR-016); superseded by T092: `widens` leaves `extensions`, `allExtensions` and `commands` out
      (`UNCONFIRMED_RIGHTS` in `src/lib/agents/profiles.ts`, `check-agent-profiles.ts`). Part of PR H.
- [x] T088 The remove button of a member row is a bin icon (FR-020b): `ProfileMembers.vue` uses `DeleteButton`. Part of PR H.

---

## Phase 13: Models of the active profile in the chat, no confirmations (PR I)

**Goal**: the chat offers only the models of the active profile, "Standard" included, with a quick profile select next to the
model select, and no change of rights asks for a confirmation (FR-006b, FR-006c, FR-016). Decided 2026-11-07.

- [x] T089 [US5] `agent_profiles/store.rs`, `resolve.rs`: `set_in_standard` (a member row in "Standard" says a model left it),
      `Memberships.left_standard`, `resolve::model_available`; tests in `resolve_tests.rs` (Standard active, another profile
      active, the active profile gone).
- [x] T090 [US5] `subjects.rs` and `commands.rs`: `AgentSubjectView.inStandard` and `available`, the member count and member list of
      "Standard" without the models that left it, `agent_membership_set` for "Standard" with a model; test in `subjects_tests.rs`.
- [x] T091 [US5] `chat/send.rs`: `start_turn` refuses a model that is not offered (`chat.error.modelNotInProfile`, de/en).
- [x] T092 No confirmation for any change of rights or activation (FR-016): `widens`, `WideningConfirm.vue`, the questions in
      `ProfileView.vue`, `ProfilesView.vue` and `useAgentProfileActivation.ts` removed; `check-agent-profiles.ts` loses its
      `widens` tests.
- [x] T093 [US5] The composer: `ComposerProfileSelect.vue` in the popover and the drawer, the model list filtered by
      `unavailableModelIds` (`Composer.vue`, `stores/agentProfiles.ts`).
- [x] T094 [US5] The pages: a switch "Standard" for a model on its page (`Memberships.vue`), a bin and an add field for the models of
      "Standard" (`ProfileMembers.vue`), models that left "Standard" or are not offered marked in the agent list (`AgentsView.vue`).

---

## Phase 14: One kind for Abo-Agents and local models (PR J, R14)

**Goal**: an Abo-Agent's model is a model like any other: profiles, availability in the chat, rights by profile, autonomy and
deny rules in the profile, no registration (FR-027 to FR-031). Decided 2026-11-07 (operator: Abo-Agents and local agents are
treated alike; different treatment is two profiles). Delivery J1 to J4 as in plan.md; J1 is this documentation.

- [x] T095 [J1] Spec, research R14, plan and tasks for PR J.
- [x] T096 [J2] `DelegatePass.model` and `HolziAccess::server_for(vendor, thread, model)`; the adapter (`adapters/cli_delegate`) passes the
      model of the request; test that the pass names it.
- [x] T097 [J2] `SessionAgent.subject`; rights for a session, for `Caller::ExternalAgent` and for a delegate's `Caller` through
      `resolve::rights_of(subject)` only (`attach.rs`, `connection.rs`, `dispatch.rs`, `agent_browser/env.rs`, `password_env.rs`,
      `agent_profiles/remember.rs`); `registry::load_rights` for sessions removed so a missed call site does not compile.
- [x] T098 [J2] No registration: `delegates::admit` admits a pass while the MCP access is on and the model is offered; remove
      `delegates::register`, `DelegateRegistration`, `DelegateStatus::{Unregistered, Expired}`, `settings.agents.mcp.delegate.register`,
      `RegistrationQuestion.delegate`, the chat banner texts for them (FR-029); tests that a delegate gets tools without a row, and none
      with the MCP access off.
- [x] T099 [J2] `subjects.rs` lists the models of a `cli_delegate` provider (kind builtin, no longer skipped) and `chat::send` checks
      them like any model (FR-006b without the exemption); `SubjectKind::Subscription` goes; the agent list shows them under
      "Lokaler Agent"; the registration dialog and the agent pages lose their Abo-Agent branches; i18n, bindings.
- [x] T100 [J2] Tests: a model in "Standard" has `default_rights` (same as before for an Abo-Agent registered with the preset); a
      profile without browser rights refuses an Abo-Agent's browser call and drops its tool; taking the model out of the active
      profile refuses the next start; `mcp_server_delegates` integration tests rewritten for the pass with a model.
- [x] T104 Prompts and hints: the browser prompt no longer says that a remembered answer holds for every agent in the profile and how
      many (`agentBrowser.prompt.profileNote*` removed; the backend keeps the profile to refuse an answer after a change), the hint of
      the profile page keeps only the sentence on files and contacts (FR-015, 2026-11-08).
- [x] T101 [J3] `autonomy` and `deny` in `grants::Rights`, `grant_rows` (skipped by older builds), `validate`, the schema of
      `settings.agents.profiles.rights.set`, `RightsEditor` group "Abo-Agent", bindings; the adapter reads them for the model of the
      request; "Standard" starts from the device preference once (FR-030); `AutonomyModeSetting` and `DelegateDenyRulesSetting` move
      into the profile page or point to it.
- [x] T102 [J2, moved up from J4: without it a delegate in a restrictive profile would become models in "Standard"] Maintenance
      step `agent_profiles::delegates::convert_memberships`: memberships of delegate rows become memberships of every model of the
      provider and the row's memberships are deleted; a delegate whose provider has no models yet keeps them until the next open; test with two devices' rows and with a vault that has no delegate (FR-031).
- [x] T103 [J4] The delegate row stays as the technical identity (browser sessions, grants and the call log are keyed by agent id), so
      `delegate_id` and `registry::register_delegate` stay; amendments in `specs/021-mcp-server` and `specs/009-autonomous-delegate-mode`,
      legacy autonomy and deny rules seed Standard and existing Abo-Agent model profiles without overwriting an explicit
      delegate group; updated MCP quickstarts and the Spec 032 command contract; the Spec 083 quickstart has the scenario
      "M-Abo" (two profiles
      "Lokal" and "Abo"); the settings e2e scenarios were edited in J3 and `mcp-agent.test.ts` has no delegate flow to change.

---

## Phase 15: Contacts and files in the profile (PR K, FR-032, FR-033)

**Goal**: what an agent reaches in the address book and in files is a right of the profile of its model or agent, so profiles
can differ in it (decided 2026-11-08: also file shares and sync rules; no migration, there are no users). Delivery K1 contacts,
K2 files.

- [x] T105 [K1] Spec, plan and tasks for PR K.
- [x] T106 [K1] `Rights.contacts` (`ContactsRights { all, groups }`, `AccessLevel`), rows in group `contacts` (an unknown level reads
      as `denied`), `ContactsAgent` reads the profile of the subject (`local::builtin_subject` for the chat, `registry::load_rights`
      for a registered agent; `ContactsAgentEnv::device`), "Immer erlauben" and a kept refusal written by
      `remember::change_contacts_of` into the profile that was asked (otherwise for the call only); `mcp_server::dispatch::reach`
      reads the group; `AgentFileKind::Contacts`, `contacts_of`, `group_target` removed.
- [x] T107 [K1] The group "Kontakte" in `RightsEditor` (`ContactsRightsFields.vue`), the contacts part of `AgentFilesSetting` and
      `agentPermissions.ts` removed, `RIGHTS_SCHEMA`, i18n, tests (`agent_tests`, `grant_rows_tests`, `mcp_server_data`,
      `check-mcp-actions`, e2e `agent-contacts`).
- [x] T108 [K2] Files: the group `files` (device, storages, encrypted folders with `reach`, file shares, sync rules) in `Rights`,
      `grant_rows`, the resolver for the caller of `files::agent`, `file_sync::agent`, `agent_browser/env.rs`; the file part of the
      agent page moves into the profile editor; `agent_file_permissions` dropped by a migration.
- [x] T109 [K2] The built-in agent's default "files of the device" is gone: the factory rights of "Standard" have the device off for every
      agent (decided 2026-11-08). `agent_profiles::identity::Identity` is the one place that reads and writes the rights of a data
      action (contacts and files); the page KI → Dateien, its actions `settings.agents.files.*` and the commands
      `files_agent_permission*` are removed, `files_folder_names` names remembered folders; migration `0059_drop_agent_files`;
      e2e `settings-agent-files`, `files-agent`, `files-encrypted-agent`, `agent-sync-rules` and `agent-contacts` set the rights in
      "Standard".

---

## Phase 16: Providers as members (PR M, FR-034)

- [x] T110 [M] Spec FR-034, data-model, plan and tasks.
- [x] T111 [M] `Subject::Provider`, `store::chain` and `store::effective_memberships` (own plus provider memberships, unreadable on
      either side fails closed), `resolve::applying` and `model_available` use them; `subjects.rs` lists providers as subjects, gives
      each model `providerSubject` and `inherited`, counts agents and models only; `agent_membership_set` lets a provider leave
      "Standard"; tests in `provider_tests.rs`.
- [x] T112 [M] Frontend: the member list of a profile heads a provider's group with a row for the provider's own membership and a bin,
      models in only through it have none; the add field offers providers; `AgentProviderView` (`ai.agents.provider`) with the
      switches, the model page shows inherited profiles; i18n; `check-agent-profiles`.
- [x] T113 [M2] Navigation (FR-035): "Externe Agenten" becomes "Agenten und Modelle" (external agents and models by provider, each with its agent profile); the MCP switch, its state and the setup snippets move to the new page "Externer Zugang" (`ai.access`, `AccessView.vue`); registry, route, i18n (de/en), `check-settings`, the e2e scenarios that switch the access on.

---

## Dependencies and order

- **Phase 1 then Phase 2** block everything. PR A is T003 to T015.
- **US1** needs the foundation. **US2** needs US1's commands and store (T017, T021) but not its agent-page work. **US3** needs US2's
  pages. **US4** needs the foundation only. **US5** needs the foundation, US1's subjects (T016, T017) and the live refresh (T018).
  **US6** needs the foundation and T018 for external agents (T059, T060) and can ship as PR B before any UI; its built-in part is in T051 (PR C). **US7** is evidence after T010 and T054.
- **Phase 10** is last, except T066, which is part of PR B; T072 must not land before PR B and PR C.
- Delivery as in plan.md: A (T003 to T009, T012, T015, and the trigger of T018 that already landed), B (T010, T016 to T049, T059 to T062,
  T066: the switch and the pages), C (T050 to T058, T063, T071: the local agent), E (T067 to T070, T072), G (T073 to T076: fixes after the first manual check), H (T078 to T088: several profiles per agent, one active), I (T089 to T094: models of the active profile in the chat, no confirmations), J (T095 to T103: Abo-Agents and local models are one kind).

## Parallel opportunities

- Inside the foundation: T005 and T009 (tests) beside T006 to T008 once the types exist.
- After the foundation: US4 (T043 to T049) and US6 (T059 to T062) are independent of the UI work of US1 to US3.
- Frontend and Rust of one story: the i18n tasks (T024, T034, T042, T049) and the `disabled` task (T020) touch other files than the
  Rust tasks.

## Implementation strategy

- **MVP**: Phases 1 and 2, then US1 and US2 (the two P1 stories) in PR B: rights come from profiles, an agent can be moved to another
  profile in one step and "Standard" is edited once. Stop and validate with quickstart M1, M2 and M7.
- **Then**: US5 (PR C, the largest risk, it gates the local agent; keep it its own PR), US7 evidence, cleanup.
- **Order of risk**: T010 (changing where rights come from: every agent drops to "Standard") and T052 (gating the local agent) are the two places
  where a mistake changes what an agent may do; both have a test.
