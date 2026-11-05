# Research: Agent profiles

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Findings from reading the code on 2026-11-05 (worktree at the merge of PR #831). Paths are relative to the repository
root. Every decision below is open to the operator's review; the ones marked **(review)** change what a user sees.

## R1 Where rights live today

- `haex_mcp_agents` (`identity/migrations_mcp_agents.rs`, 0045): one row per registered agent **and device**
  (`vault_device_uuid`), synced, columns `id`, `kind`, `slug`, `client_name`, `status`
  (registered, expired, blocked), `permission_mode`, `token_hash`, `registered_at`, `last_seen_at`. No UNIQUE
  constraint, because a conflict halts the sync (ADR-0001).
- `haex_mcp_agent_grants` (same file): rows `agent_id`, `grant_group`, `grant_key`, `level`, `scope_tags`; the
  semantics are documented at the top of `mcp_server/grant_rows.rs`. Row ids are UUIDv5 of device, agent, group and
  key (`registry.rs`, `grant_id`). Rows are replaced as a whole by `save_rights` (DELETE, then INSERT).
- The built-in agent has no `haex_mcp_agents` row. Its browser rights are the device preference
  `agent_browser.builtin`, its password fills `agent_browser.builtin_passwords` (`agent_browser/env.rs`), both
  `PrefScope::Device`.
- Files and contacts: `agent_file_permissions` (vault wide, synced; the built-in agent is `agent_id = "builtin"`).
  Out of Stage 1 (spec Scope).

## R2 Decision: three new vault-wide tables; an assignment names a subject

Profiles must be the same on every device (spec Decisions), so they cannot use the device scoped agent tables.

- **Decision**: `haex_agent_profiles` (id, name), `haex_agent_profile_grants` (the rows of a profile, the shape of
  `haex_mcp_agent_grants`), `haex_agent_profile_members` (id, subject, profile id). All synced, no
  `vault_device_uuid`, no UNIQUE besides the primary key, no cascading foreign key.
- **Subject**: `agent:<id>` for a row of `haex_mcp_agents` (external agent or Abo-Agent; the row of a delegate has a
  deterministic id per device and kind, `mcp_server/delegates.rs`), `model:<model id>` for a model of the built-in
  agent. The model id is the composite `<provider_id>:<remote_id>` for API models and the catalog id for local ones
  (`chat/session.rs`, `ActiveSession.model_id`); both are the same on every device.
- **Consequence for the spec**: an agent registration exists on one device only (ADR-0001), so an assignment of
  `agent:<id>` is effectively that device's; the profile it points to is shared. `haex_mcp_agents` is synced, so
  every device sees the agent rows of every device and can count and assign them. The member count of a profile is
  the count of its assignment rows in the vault; for "Standard" it is the agent rows of **every** device without an
  assignment plus the models of the built-in agent without one (FR-016 says "how many agents are in it"; profiles are
  vault wide). "Standard" also holds every agent that registers later, so a widening change of "Standard" always
  asks first, even when the count is 0 (R11).
- **Alternatives**: a `profile_id` column on `haex_mcp_agents` (rejected: the built-in agent has no row, and a model
  assignment must outlive models); rights JSON in one column (rejected: `grant_rows` already round-trips rows, one
  implementation per rule).

## R3 Decision: "at most one profile" by primary key

`haex_agent_profile_members.id` is `UUIDv5(NS_PROFILE_MEMBER, subject)`. Two devices that assign the same subject write
the same row, the CRDT merges it column by column, and the subject ends up in one profile. There is no check that could
race. Precedent: the deterministic ids of `agent_file_permissions` (`files/permissions.rs`) and of the grant rows.
Assigning to "Standard" deletes the member row (no row means "Standard"); a delete leaves the usual marker in
`haex_deleted_rows`, and re-assigning later re-inserts the same id.

## R4 Decision: "Standard" is virtual until it is edited

Rows cannot be inserted from a migration (the HLC is not initialised there), and a seed step on every device would
write the same rows twice. So `STANDARD_PROFILE_ID` is a fixed UUID; as long as `haex_agent_profiles` has no row with
that id, the resolver answers with `grants::default_rights()` (spec 021 FR-009). The first edit of "Standard" writes the
profile row and its grant rows. "Standard" cannot be deleted or renamed in the commands (FR-001). Its name in the
user-facing texts is a translated label, not the stored name.

## R5 Enforcement and the per-connection cache

- `mcp_server/grants.rs` (`covers`, `offers`, `check`, the extension and command offers) stays the decision point
  (FR-010). It takes `Rights`; only the source changes.
- External agents and Abo-Agents: `SessionAgent.rights` (`mcp_server/session.rs`) is loaded at admission by
  `registry::load_rights` (`connection.rs`, `attach.rs`, `delegates.rs`) and used for the tool list and every call
  (`dispatch.rs`). A cached copy does not satisfy FR-011.
- **Decision**: keep the cache and refresh it from the table feed, not from each writer. `core.vault_changes`
  (`vault_events.rs`) reports the tables of every committed transaction: a local command, a remembered answer, and a
  batch received from another device alike. `attach::tell_catalog_changes` already follows it for
  `agent_file_permissions`. A change of `haex_agent_profiles`, `haex_agent_profile_grants` or
  `haex_agent_profile_members` (and a lagged feed, which means "anything may have changed") re-resolves the rights of
  **every** live session (a handful). Each re-resolve holds `live.synchronize(agent)`, as `attach::join` does, so it
  cannot race an admission. Sessions whose rights differ get the new `SessionAgent.rights` and
  `tools/list_changed`. One trigger serves every writer and FR-011's "on every device". No writer has to remember to
  refresh. The browser, password and file executors already read fresh on every call (`agent_browser/env.rs`,
  `password_env.rs`), so they see a change at once.
- **Known gap to close**: `agent_browser/env_rights.rs` (`change_browser_rights`) writes an external agent's rights
  without telling the live sessions, and no synced change of `haex_mcp_agent_grants` refreshes a session today. The
  table feed closes both (PR B).
- **Alternative**: resolve on every MCP call (rejected: the tool list must also change, which needs the push anyway).

## R6 Built-in agent **(review)**

- **Finding**: the built-in agent has no rights model beyond the browser and password fills. Its window, chat and
  settings tools are registered when the frontend marks an action `builtinAgentCallable`
  (`chat/action_commands.rs`, `chat/tools/action_tool.rs`); `run_command` when the device has the command tool;
  extension tools through the registry. The only limit is the approval mode, `chat.permission_mode` (vault
  preference). Passwords are refused outright (`passwords/access.rs`, `Caller::BuiltinAgent => Denied::Forbidden`);
  the agent only fills through the browser.
- **Consequence**: a profile for the built-in agent is a **new gate** on tools that were open. A "Standard" with
  reading only would take most of them away.
- **Decision**: (1) enforce the whole rights list for the built-in agent, so the profile editor never shows a row that
  silently does not apply; (2) the migration creates a full-rights profile "Eingebauter Agent" from today's device
  settings and assigns every model the user has used, so nothing narrows (FR-025); (3) a model that appears later
  starts in "Standard" like every agent, and the chat says so with the existing notice mechanism
  (`chat.*` notices, as `delegateUnregistered`), naming the profile and the missing right (R11), for example "Dieses
  Modell ist im Agentenprofil Standard und darf den Browser nicht verwenden."
- **Why not** a second always-present profile for built-in models: the spec says "Standard holds every unassigned
  agent"; one default is simpler to explain. If the notice proves annoying, the operator can set "Standard" wider or
  assign a profile in one step.
- **Subject at call time**: nothing passes the model into `BrowserEnv::rights(&caller)` today. The chat already sets
  a task-local `AgentTask` around the tool calls of a turn (`chat/tools/agent_task.rs`, spec 048 FR-030a). The file
  actions read its model and locality from there. **Decision**: `AgentTask` also carries the turn's
  `ActiveSession.model_id`, and `agent_profiles::subjects::builtin_subject()` reads it from the task-local. It never
  reads the global `ChatState.session`: a model switch replaces it between turns, and it says nothing about a call
  outside a turn. A call outside a turn has
  no task and resolves as "Standard". The `Caller` enum stays unchanged (many matches). One source of "which model
  acts" serves files, browser, passwords and tools. The voice assistant runs its turns through the same turn loop, so
  it gets the same task.
- **Where the built-in agent is gated**: `chat/action_commands.rs` registers the built-in tools once per catalog push
  into the shared `tool_registry`, for every model. A filter there cannot depend on the model, and it would miss a
  change made after registration. So the gate sits in the turn: the offer of each step (`chat/tools/offer.rs`,
  `availability.rs`) leaves out what the subject's rights do not cover. The dispatch of each call re-resolves and
  refuses an uncovered call with a denial, as `grants::check` does for MCP calls. The registry stays as it is, and no
  re-registration is needed on a profile change.
- **What the profile gates for the built-in agent in Stage 1**: the topics, the browser, the password fills, commands
  and extension tools, through the same `grants` functions. Files, file shares and contacts (`files.*`,
  `files.shares`, `contacts.*`) keep today's gate, `agent_file_permissions` with `agent_id = "builtin"` plus the
  approval mode (spec Scope). The built-in agent's `Reach` lists the native executors that serve it, not the ones of
  external agents. Otherwise `grants::covers` would hide `files.shares` (no grantable scope) and the native actions
  that only serve the built-in agent, and the migration (R7) would narrow it. `passwords/access.rs` keeps refusing
  password tools to the built-in agent. For a model, the password row of a profile governs the fills through the
  browser only, and the editor says so for built-in members.
- **A chat that runs on an Abo-Agent**: when the active session's provider is a subscription agent
  (`ProviderKind::CliDelegate`), the subject is that delegate's agent row (`agent:<id>`), not a `model:` entry, because
  the agent that holzi started is the one acting and it already has its registration (`mcp_server/delegates.rs`).
- **Approval mode** of the built-in agent stays the vault preference (FR-017).

## R7 Decision: the migration runs after open

A CRDT DELETE inside a migration fails (no HLC), and inserts need the same context. FR-024 therefore runs as an
idempotent step after the vault is open, the way `storage/maintenance.rs` folds preferences (steps 2 and 4):

1. For every agent row of this device: load its grant rows with the existing `grant_rows::rights_of`. Equal to what
   "Standard" resolves to **on this device at this moment** (`default_rights()` while it is virtual; its rows once
   another device's edit has arrived), after normalising level order and empty groups: no assignment. Otherwise
   create a profile named after the agent with those rows and assign the agent. A name that is taken gets the device
   alias appended. Comparing with the resolved "Standard", not with the factory rights, keeps an agent out of a
   "Standard" that another device already widened. The case where that edit has not arrived yet is accepted (Q2, decided).
2. Built-in agent (PR C): build `Rights` from `agent_browser.builtin` and `agent_browser.builtin_passwords`. Add
   "write" in every topic, commands on where the device has the command tool, and the new right "alle Erweiterungen" (Q3, decided: a row `extension` / `*`, skipped by older builds).
   Create "Eingebauter Agent" and assign the models the built-in agent has used. `chat_messages` carries no device,
   so `chat_messages.model_id` lists the models used on **any** device of the vault. `chat.last_active_model_id` is
   this device's. Only models of `ProviderKind::Local` and `ApiKey` count; a `CliDelegate` session is an Abo-Agent
   (R6). The member id comes from the model alone (R3), so two devices with different built-in settings would write
   the same member rows for a model both used, and the later write would decide for both devices, which can widen
   one of them. How to migrate a model used on several devices is decided in Q1: one fixed-id profile whose browser and password
   part is the intersection of every device's settings, re-run when a newer migrated row arrives by sync.
3. Markers make each step idempotent: `agent_profiles.migrated_v1` for step 1 (PR A) and
   `agent_profiles.builtin_migrated_v1` for step 2 (PR C). They are device-scoped preferences. With one marker, a
   device that ran PR A's step would skip PR C's. Every write is deterministic by id, so a crash in the middle repeats
   safely (FR-025). Two devices that migrate at the same time write disjoint agent subjects (agent rows are per
   device) and different profile ids (`UUIDv5` of device and agent), so step 1 cannot collide across devices.

The old rows stay (nothing is deleted in this stage); after the switch nothing reads them (R8).

## R8 Decision: old per-agent rows are dead data after PR A

After PR A `registry::load_rights` no longer reads `haex_mcp_agent_grants`; `save_rights` is replaced by an assignment
write. The rows are not deleted (rollback and audit), and a later cleanup spec may drop them. `manage::revoke` deletes
the agent's assignment row in the same write (explicit CRDT delete, the precedent of `passwords/trash.rs`).

**Between PR A and PR D** the windows still send a `Rights` value: the agent page editor (`mcp_agent_rights_set`),
the registration answer, the OAuth consent and the delegate registration. So does the remembered answer of
`env_rights.rs`. PR A routes every one of them through one function, `agent_profiles::set_own_rights(agent, rights)`.
It applies the migration's rule (R7 step 1). Rights equal to "Standard" leave the agent unassigned. Other rights go
into the agent's own profile (`UUIDv5("migrated-agent:{device}:{agent}")`, created on the first write). Until PR D
every assigned agent has a profile of its own, so the behaviour stays per agent as today and no submitted right is
dropped. PR D replaces these inputs with the profile field. PR E removes `set_own_rights` together with the old
commands.

## R9 Sync and older builds

`sync/handshake.rs` refuses a peer whose migration count or `HOLZI_TRIGGER_VERSION` differs (`RejectCode::Incompatible`).
Migration 0054 and the bump to 35 therefore partition the fleet until every device is updated; spec 059 already shows
the device versions. An older build never reads a profile row, so FR-012's risk is only inside one schema: a profile
row this build cannot read (unknown group, level or key) counts as no right, as in `grant_rows.rs`; a member row whose
profile does not exist counts as "Standard". Renumbering risk: on 2026-11-05 no open PR and no branch on `origin`
registers `0054` or later, and `origin/main` ends at `0053_sync_rules_source_target`. A branch that takes 0054 before
this one lands makes this one move to the next number. If 0054 is already on a device by then, the old name needs an
entry in `identity/migrations_legacy_names.rs`.

**`grant_rows::rights_of` today is not strict for every group**, and profile rows sync to every device of the vault.
It reads every `commands` row at `on` as command access, whatever the key. It also inserts every `operate` key into
`Rights.operate`. `grants::covers` refuses only `guardrails` before the operate lookup, so a row `operate` /
`files.shares` / `write` would open the file-share actions, which no agent can be granted. PR A tightens the shared
converter, for the old rows and the profile rows alike. `commands` counts only with the empty key (`rows_of` writes
nothing else), and `operate` keeps only keys of `OPERATE_SCOPES`. Every other row is skipped (FR-012). Tests in
`grant_rows_tests.rs` cover both cases.

## R10 Registration and prompts

- `registration.rs` builds `RegistrationQuestion { rights, mode, delegate, ... }` and the window answers with
  `RegistrationAnswer { decision, remember, rights, mode }` (`prompts.rs`). Today the window edits rights; with
  profiles the question carries the effective rights of the profile the agent would get (read only, FR-013) and the
  answer carries `profileId`. `registry::register` writes an assignment instead of rights (none for "Standard"). An
  answer whose `profileId` names no profile any more (deleted while the question was open) is refused with
  `profile_missing`, as `agent_assign` is; the question stays open so the user chooses again. A missing profile never
  silently resolves as "Standard".
- The second entry point is the OAuth consent of HTTP agents (`oauth_flow.rs`, `oauth_rows::previous` and
  `oauth_rows::authorize`). It asks with the same `RegistrationQuestion`, answers with the same `RegistrationAnswer`,
  and today calls `live.set_rights` for an earlier authorization's connections. It gets the same change: the profile
  field, `profile_missing`, an assignment write, and no `live.set_rights` (R5's table feed refreshes).
- `manage::renew` (expired agent connects again) keeps the assignment (FR-014): there is nothing to restore, because
  the rights were never on the agent. The question of a renewal preselects the agent's current profile
  (`registration.rs` preselects the previous rights today), and the answer's `profileId` is written like a new
  registration's.
- Delegates register in the chat through "Rechte festlegen" (`delegates::register`,
  `settings.agents.mcp.delegate.register`); the same field "Agentenprofil" replaces the rights editor there.
- Remembered answers: `env.rs` `remember`, `remember_fill`, `remember_request` and the built-in `switch_on` go through
  `change_browser_rights`; it writes the profile of the subject (PR B), and the prompts in
  `agent_browser/prompts.rs` and `PromptDialog.vue` name the profile (FR-015). A prompt is also a place where the user
  changes a profile, so an answer that widens it names the member count first, like the page (FR-016). For "Standard"
  that count includes every agent that registers later (R2). "Switch the browser on" for the built-in agent changes
  its profile, which may hold other models and external agents; the prompt says so.

## R11 UI

- Routes: `src/lib/settings/registry.ts` (`ai.profiles`, `ai.profiles.profile`, group `agents`), `components/wm/appRoutes.ts`,
  `scripts/check-settings.ts`, the e2e `settings-categories.test.ts`, i18n `settings.locations.*`.
- The profile editor is `RightsEditor` (`RightsFields`, `PasswordRightsFields`, `BrowserRightsFields`,
  `CommandRightsFields`, `ExtensionRightsFields`) with the profile's `Rights` as its draft, unchanged; it saves on
  selection (settings rule), errors as toasts.
- **Widening confirmation (FR-016)**: a pure function `widens(before, after)` in `src/lib/agents/profiles.ts` is true
  when `after` gives anything `before` does not, field by field over the whole `Rights`. That covers each topic
  (none < read < write), commands, and each extension id. For passwords it covers off < read < write, a tag reach that
  is wider (every entry, or a tag that was not listed), and a new default tag. For the browser it covers off < on,
  `user_browser`, the level for all websites, every per-site level (a site that is new, raised, or no longer
  blocked), `fill_remembered` on a site, and every request rule that is new at `allow` or turned from `deny` to
  `allow`. A test enumerates the fields of `Rights`, so a field added later fails the test until `widens` covers it.
  A widening change opens a confirm dialog naming the member count when the profile has members, and **always** for
  "Standard" (R2). Cancelling restores the control. A narrowing change saves at once. The same function serves the
  prompts (R10).
- The agent page keeps mode, log and revoke; its rights editor becomes one `SettingsSelect` "Agentenprofil" plus the
  profile's rights shown read only (the editor components already take `disabled`).
- Components are holzi's own settings components in `src/components/settings/` (`Group.vue`, `Row.vue`,
  `OptionRow.vue`, `Select.vue`, `MultiSelect.vue`, used as `SettingsGroup`, `SettingsRow`, ...) and
  `src/components/agents/RightsInset.vue`, not haex-ui. The pages follow the COSMIC/GNOME style of the other KI pages:
  toolbar row, big title, boxed lists. The new pages go to `src/components/settings/agentProfiles/` so no file is
  called just "Profile" next to the cookie profile of the agent browser.
- Chat notice for the built-in agent (`chat.agentProfile.restricted`): it names the profile and the right that is
  missing for what was just asked (browser, password fill, a topic at write, commands, an extension), one variant per
  kind of right. It never says "darf nur lesen" for a model that may do more.
- KI → Browser keeps the browser choice, trackers and the cookie profile (FR-021).
- Naming sweep ("Abo-Agent", never "Delegate"/"Delegat"): `src/i18n/locales/de.json` strings about delegates
  (the chat notices `delegate*`, the registration hint, the settings entries "Autonomie für Delegaten" and
  "Verbotsregeln für Delegaten", the keywords of the AI category), `en.json` likewise, and
  `RegistrationDialog.vue`. Keys keep their names; only the texts change. A text the sweep touches also says
  "HaexVault" where it says "holzi" today (`chat.delegateUnregistered` does). The provider-connection texts
  (`cliDelegate.*`, "Verbinden") describe a login, not an agent, and stay.

## R12 Alternatives rejected for the whole design

- **Profiles per domain** (the first idea): rejected by the operator for the one-step switch.
- **Profile as a template copied into each agent**: rejected, a change must reach every member (spec US2).
- **Approval mode inside the profile**: operator decided no (spec FR-017).

## R13 Decisions on the open questions (operator, 2026-11-05)

The options stay below for the reasoning; the chosen one is marked **Decided**.

- **Decided Q1: (a). Decided Q2: (a). Decided Q3: (b). Decided Q4: (a).** Consequences: Q1 gives the built-in profile a
  fixed id (`UUIDv5("builtin-profile")`, one per vault) and makes step 2 narrow, never widen; Q3 adds the right "alle
  Erweiterungen" (`Rights.allExtensions`, row `extension` / `*`) with a row in the extension fields, a check in
  `grants::extension_offers`, and `widens` ranks it above any single extension; Q4 amends FR-002 and the edge case
  "Names" of the spec.

- **Q1 A built-in model used on several devices (blocks PR C).** The built-in rights are device settings today, and a
  model's assignment is vault wide (R3, R7 step 2). When two devices had different `agent_browser.builtin` or
  `builtin_passwords`, one assignment cannot keep both, and the later write can widen the other device (FR-025).
  Options: (a) one vault-wide "Eingebauter Agent" with a fixed id, whose browser and password part every device's
  step narrows to the intersection with its own settings; the step re-runs when a newer migrated row arrives by sync,
  so the concurrent case converges to the narrowest; (b) migrate the built-in browser and password rights as off on
  every device and show a one-time notice to grant them in the profile (narrows, never widens); (c) accept that the
  last migration decides and amend FR-025 for this case.
- **Q2 "Standard" edited on another device before this device migrated, not yet synced (PR A).** Step 1 compares
  with the "Standard" this device sees (R7). If the edit arrives after this device's step, its factory-rights agents
  get the edited "Standard", which can be wider. Options: (a) accept; the edit was made for "every unassigned agent"
  (spec Decisions) after the update; (b) step 1 always puts factory-rights agents into a snapshot profile
  "Werksrechte", which narrows nothing but adds a profile the user did not make.
- **Q3 Extensions installed later and the built-in agent (PR C).** `Rights.extensions` lists ids; "a new extension
  is not among them" (`grants.rs`). The built-in agent sees every extension's tools today. After the migration it
  would not see an extension installed later until the user adds it to the profile. Options: (a) accept and say so
  on the install page; (b) a new right "alle Erweiterungen" (a row `extension` / `*`, skipped by older builds).
- **Q4 Profile names after concurrent creates (PR A).** FR-002 and the spec's edge case require unique names in the
  vault, and `name_taken` can only check what this device has seen. Two devices that create the same name before
  syncing (by hand, or the migration for agents with the same name) both succeed. Options: (a) amend the spec: such
  duplicates are possible, the list marks them, and the next edit asks for a new name; (b) make the profile id a
  function of the normalised name, so the two creates merge into one profile; a rename then becomes a new profile
  plus a move of its members and rows.
