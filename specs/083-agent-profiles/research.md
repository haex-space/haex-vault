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
- The local agent has no `haex_mcp_agents` row. Its browser rights are the device preference
  `agent_browser.builtin`, its password fills `agent_browser.builtin_passwords` (`agent_browser/env.rs`), both
  `PrefScope::Device`.
- Files and contacts: `agent_file_permissions` (vault wide, synced; the local agent is `agent_id = "builtin"`).
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
  assignment plus the models of the local agent without one (FR-016 says "how many agents are in it"; profiles are
  vault wide). "Standard" also holds every agent that registers later, so a widening change of "Standard" always
  asks first, even when the count is 0 (R11).
- **Alternatives**: a `profile_id` column on `haex_mcp_agents` (rejected: the local agent has no row, and a model
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

## R6 Local agent **(review)**

- **Finding**: the local agent has no rights model beyond the browser and password fills. Its window, chat and
  settings tools are registered when the frontend marks an action `builtinAgentCallable`
  (`chat/action_commands.rs`, `chat/tools/action_tool.rs`); `run_command` when the device has the command tool;
  extension tools through the registry. The only limit is the approval mode, `chat.permission_mode` (vault
  preference). Passwords are refused outright (`passwords/access.rs`, `Caller::BuiltinAgent => Denied::Forbidden`);
  the agent only fills through the browser.
- **Consequence**: a profile for the local agent is a **new gate** on tools that were open. A "Standard" with
  reading only would take most of them away.
- **Decision**: (1) enforce the whole rights list for the local agent, so the profile editor never shows a row that
  silently does not apply; (2) there is no migration (R7): every model starts in "Standard" with the factory rights
  and the user gives it a profile; (3) the chat says so with the existing notice mechanism
  (`chat.*` notices, as `delegateUnregistered`), naming the profile and the missing right (R11), for example "Dieses
  Modell ist im Agentenprofil Standard und darf den Browser nicht verwenden."
- **Why not** a second always-present profile for built-in models: the spec says "Standard holds every unassigned
  agent"; one default is simpler to explain. If the notice proves annoying, the operator can set "Standard" wider or
  assign a profile in one step.
- **Subject at call time**: nothing passes the model into `BrowserEnv::rights(&caller)` today. The chat already sets
  a task-local `AgentTask` around the tool calls of a turn (`chat/tools/agent_task.rs`, spec 048 FR-030a). The file
  actions read its model and locality from there. **Decision**: `AgentTask` also carries the turn's
  `ActiveSession.model_id`, and `agent_profiles::local::builtin_subject()` reads it from the task-local. A task that a turn spawns (a page request or a new tab of the agent browser) does not inherit the task-local, so it falls back to the model of the latest turn that ran tool calls (a process-wide note, `agent_profiles::local`); the fallback exists so such a task does not judge a model in a profile as "Standard". It never
  reads the global `ChatState.session`: a model switch replaces it between turns, and it says nothing about a call
  outside a turn. A call outside any turn has
  no task and no latest turn and resolves as "Standard". The `Caller` enum stays unchanged (many matches). One source of "which model
  acts" serves files, browser, passwords and tools. The voice assistant runs its turns through the same turn loop, so
  it gets the same task.
- **Where the local agent is gated**: `chat/action_commands.rs` registers the built-in tools once per catalog push
  into the shared `tool_registry`, for every model. A filter there cannot depend on the model, and it would miss a
  change made after registration. So the gate sits in the turn: the offer of each step (`chat/tools/profile_gate.rs`, applied to the first offer in `chat/send.rs` and to each next step in
  `chat/turn/plan_calls.rs`) leaves out what the subject's rights do not cover. The dispatch of each call re-resolves and
  refuses an uncovered call with a denial, as `grants::check` does for MCP calls. The registry stays as it is, and no
  re-registration is needed on a profile change.
- **What the profile gates for the local agent in Stage 1**: the topics, the browser, the password fills, commands
  and extension tools, through the same `grants` functions. Files, file shares and contacts (`files.*`,
  `files.shares`, `contacts.*`) keep today's gate, `agent_file_permissions` with `agent_id = "builtin"` plus the
  approval mode (spec Scope). The local agent's `Reach` lists the native executors that serve it, not the ones of
  external agents. Otherwise `grants::covers` would hide `files.shares` (no grantable scope) and the native actions
  that only serve the local agent, and the local agent would lose them. `passwords/access.rs` keeps refusing
  password tools to the local agent. For a model, the password row of a profile governs the fills through the
  browser only, and the editor says so for built-in members.
- **A chat that runs on an Abo-Agent**: when the active session's provider is a subscription agent
  (`ProviderKind::CliDelegate`), the subject is that delegate's agent row (`agent:<id>`), not a `model:` entry, because
  the agent that holzi started is the one acting and it already has its registration (`mcp_server/delegates.rs`).
- **Approval mode** of the local agent stays the vault preference (FR-017).

## R7 Decision: no migration, the old rights stop counting

Operator decision 2026-11-06: what agents were allowed before agent profiles is **not converted**. A conversion would
need a step after open (a CRDT write cannot run inside a schema migration, no HLC exists there), a profile per agent
that contradicts "a profile belongs to many agents", and rules for devices that differ (the local agent's old settings
are per device). The reset needs none of that.

- After PR B nothing reads `haex_mcp_agent_grants`, so every registered agent (external agent or Abo-Agent) is in
  "Standard" and has the factory rights, the user sets profiles anew. Registrations stay: name, credential hash, status,
  blocks, approval mode are not rights.
- After PR C the local agent's models are in "Standard" too. A maintenance step deletes its two old preferences
  `agent_browser.builtin` and `agent_browser.builtin_passwords` for every device (a synced delete, idempotent, the
  precedent is step 2 of `storage/maintenance.rs`, which deletes `shell.active_workspace_id`).
- The reset is not a step that could run twice: the old rows are simply not read. A profile the user made after the update is
  never touched (FR-025).
- **Consequence the operator accepted**: an agent that was more restricted than "Standard" before (for example no chat
  right) gets the factory rights (reading) until the user sets a profile. The local agent loses what its tools had been
  open for (window, chat, settings, commands, extensions, browser) until a profile gives it back.
- Files and contacts (Stage 2) are handled the same way when that stage lands.

## R8 Decision: the old per-agent table is dead data until it is dropped

After PR B `registry::load_rights` resolves through profiles and no code reads `haex_mcp_agent_grants`; the old inputs
(`mcp_agent_rights_set`, `settings.agents.mcp.grants.set`, `registry::save_rights`) are removed in PR B, because they have
nothing to write to. The table is dropped in PR E by migration `0056_drop_agent_grants` (DDL only; a dropped table leaves no
delete markers, `migrations.rs`). `manage::revoke` deletes the agent's assignment row in the same write (explicit CRDT
delete, the precedent of `passwords/trash.rs`); the profile stays, it may have other members.

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
  that count includes every agent that registers later (R2). "Switch the browser on" for the local agent changes
  its profile, which may hold other models and external agents; the prompt says so.

## R11 UI

- Routes: `src/lib/settings/registry.ts` (`ai.profiles`, `ai.profiles.profile`, group `agents`), `components/wm/appRoutes.ts`,
  `scripts/check-settings.ts`, the e2e `settings-categories.test.ts`, i18n `settings.locations.*`.
- The profile editor is `RightsEditor` (`RightsFields`, `PasswordRightsFields`, `BrowserRightsFields`,
  `CommandRightsFields`, `ExtensionRightsFields`) with the profile's `Rights` as its draft, unchanged; it saves on
  selection (settings rule), errors as toasts.
- **Widening confirmation (FR-016), dropped 2026-11-07**: the user wants no confirmation for any change of a profile's
  rights or for making a profile active ("das ist offensichtlich"), so `widens`, `WideningConfirm.vue` and the question on
  activation are gone. What follows is the design as it was before, kept for the record: a pure function `widens(before, after)` in `src/lib/agents/profiles.ts` is true
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
- Chat notice for the local agent (`chat.agentProfile.restricted`): it names the profile and the right that is
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

- **Q1 and Q2 are void (2026-11-06)**: they concerned the migration of the old per-device settings and of an edit of
  "Standard" that has not synced. There is no migration (R7). **Decided Q3: (b). Decided Q4: no uniqueness.** Q3 adds the
  right "alle Erweiterungen" (`Rights.allExtensions`, row `extension` / `*`) with a row in the extension fields, a check
  in `grants::extension_offers`, and `widens` ranks it above any single extension; Q4 amends FR-002 and the edge case
  "Names" of the spec.

- **Q3 Extensions installed later and the local agent (PR C).** `Rights.extensions` lists ids; "a new extension
  is not among them" (`grants.rs`). The local agent sees every extension's tools today. A profile that lists extension ids
  would not see an extension installed later until the user adds it to the profile. Options: (a) accept and say so
  on the install page; (b) a new right "alle Erweiterungen" (a row `extension` / `*`, skipped by older builds).
- **Q4 Profile names (PR A). Decided: no uniqueness.** A name check could only see what one device has seen, so two
  devices could create the same name anyway. The operator's rule is simpler: a profile is its unique id and the name may
  repeat if the user wants that. Consequences: no `name_taken`, no duplicate marking;
  the lists show member counts so same-named profiles can be told apart. (The option "id as a function of the name"
  is dropped.)

## R14 One kind for Abo-Agents and local models (PR J, 2026-11-07)

**Decision (operator):** the separation of Abo-Agents and the local agent is lifted; the permission system is the same for
both, and different treatment is two profiles. Rights per model, autonomy and deny rules into the profile.

**Today.** A delegate's models are rows of a `cli_delegate` provider (`<provider id>:<remote id>`), `subjects.rs` skips them,
`chat::send` exempts them from FR-006b. holzi's tools reach a delegate through the helper process of a pass
(`delegates::DelegateHub::issue`, `DelegatePass { kind, thread }`), the session is a `SessionAgent` keyed by the delegate row
(`delegate_id(device, kind)`), whose rights `registry::load_rights(device, id)` resolves as `Subject::Agent(id)`. The delegate
must be registered through "Rechte festlegen" (`delegates::register`). Autonomy is the device preference
`chat.autonomy_mode`, the deny list `DelegateDenyRulesSetting`.

**Design.**

1. **The pass names the model.** `HolziAccess::server_for(vendor, thread)` becomes `server_for(vendor, thread, model)`; the
   adapter knows the model of the request. `DelegatePass { kind, thread, model }`.
2. **The session carries its subject.** `SessionAgent` gets `subject: Subject` (`Agent(id)` for an external agent,
   `Model(model)` for a pass). Every place that today loads rights by agent id for a session (`attach::join`,
   `attach::tell_catalog_changes`, `connection.rs`, `dispatch.rs`, `agent_browser::env` for `Caller::ExternalAgent`,
   `password_env`, `remember`) takes the subject instead, so one function, `resolve::rights_of(subject)`, decides. A delegate's
   `Caller` carries the model subject; the agent browser and the password fills follow through it.
3. **No registration.** The pass is admitted while the MCP access is on and the model is offered (FR-006b). The delegate row in
   `haex_mcp_agents` stays as a technical identity (the browser sessions, the password and file grants and the call log are keyed
   by agent id); the start writes it (`delegates::server_for` → `register_delegate`, no question) with the approval mode of the
   chat (`chat.permission_mode`). `DelegateStatus` is `off` or `available`, `delegates::register` and its command and action go;
   the chat banner keeps only "MCP access is off". The row's rights are those of the model of its live pass:
   `registry::load_rights` looks the model up in the passes of this process (`delegates::live_model`), so every place that loads
   rights by agent id follows without a change, and a delegate with no start running has none.
4. **Rights gain `autonomy` and `deny`.** Two new rights groups in `grants::Rights` and `grant_rows` (rows skipped by older
   builds, so a mixed vault never widens): `autonomy` (`ungated` or `gated`) and `deny` (the list of rules). The adapter
   (`adapters/cli_delegate`) reads them from the resolver for the model of the request instead of the preference; the editor
   shows them in an "Abo-Agent" group of `RightsEditor`. They only matter for an Abo-Agent's model, and say so.
5. **Update.** A maintenance step (an ordinary write on open, not a rewrite of rows that sync could resurrect, see data-model.md "Migration 0057"): for each delegate row with memberships, add the same memberships to
   every model of its provider, then delete the row's member rows and mark the row removed (explicit delete, so the other
   devices converge). "Standard" gets the factory autonomy and deny rules from the device preference once, when it is first
   written; the preference stays as the factory value only.

**Stages (own PRs, each testable alone).** J1 spec and tasks (this). J2 the pass and the session subject, rights by subject
everywhere, no registration, the chat and `subjects.rs` list Abo-Agent models (FR-027 to FR-029, FR-006b without the exemption).
J3 autonomy and deny rules in the profile (FR-030). J4 the conversion of existing rows (FR-031), removal of dead code, texts,
e2e. J2 changes what an Abo-Agent may do, so it ships with a test that a model in "Standard" has the same rights as before
(`default_rights`) and one that a profile without browser rights refuses an Abo-Agent's browser call.

**Risks.** (a) `load_rights(device, agent)` has many call sites; a missed one would keep resolving a delegate by its old row,
so J2 removes the function for sessions and the compiler finds them. (b) The pass of a start whose model is later taken out
of the active profile keeps running to its end; the next start is refused (the chat checks first, FR-006b). (c) A vault on a
device that still has the old build writes delegate rows; the maintenance step converts them again on the next open.
