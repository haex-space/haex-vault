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
  `agent:<id>` is effectively that device's; the profile it points to is shared. The member count of a profile is the
  count of its assignment rows in the vault, for "Standard" the agents of this device without an assignment (FR-016
  says "how many agents are in it"; the page names which device the count of "Standard" is for).
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
- **Decision**: keep the cache and refresh it by push, as `manage_commands::set_rights` does today through
  `live.set_rights` and `live.synchronize`: on every change of a profile's rows or of an assignment, re-resolve the
  rights of **every** live session (a handful) and send `tools/list_changed` to those whose rights differ. The browser,
  password and file executors already read fresh on every call (`agent_browser/env.rs`, `password_env.rs`), so they
  see a change at once.
- **Known gap to close**: `agent_browser/env_rights.rs` (`change_browser_rights`) writes an external agent's rights
  without telling the live sessions. With profiles it writes the profile and refreshes (PR B).
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
  (`chat.*` notices, as `delegateUnregistered`): "Dieses Modell ist im Agentenprofil Standard und darf nur lesen."
- **Why not** a second always-present profile for built-in models: the spec says "Standard holds every unassigned
  agent"; one default is simpler to explain. If the notice proves annoying, the operator can set "Standard" wider or
  assign a profile in one step.
- **Subject at call time**: the model is in `ChatState.session` (`ActiveSession.model_id`, `provider_kind`); nothing
  passes it into `BrowserEnv::rights(&caller)` today. The resolver takes it from the active session at call time
  (`agent_profiles::subjects::builtin_subject(app)`), without changing the `Caller` enum (many matches). A model job or
  the voice assistant that runs without a chat session uses the model of the job; one with no model is in "Standard".
- **A chat that runs on an Abo-Agent**: when the active session's provider is a subscription agent
  (`ProviderKind::CliDelegate`), the subject is that delegate's agent row (`agent:<id>`), not a `model:` entry, because
  the agent that holzi started is the one acting and it already has its registration (`mcp_server/delegates.rs`).
- **Approval mode** of the built-in agent stays the vault preference (FR-017).

## R7 Decision: the migration runs after open

A CRDT DELETE inside a migration fails (no HLC), and inserts need the same context. FR-024 therefore runs as an
idempotent step after the vault is open, the way `storage/maintenance.rs` folds preferences (steps 2 and 4):

1. For every agent row of this device: load its grant rows with the existing `grant_rows::rights_of`. Equal to
   `default_rights()` (after normalising level order and empty groups): no assignment. Otherwise create a profile
   named after the agent (a name that is taken gets the device alias appended) with those rows and assign the agent.
2. Built-in agent: build `Rights` from `agent_browser.builtin` and `agent_browser.builtin_passwords` plus "write" in
   every topic, commands on where the device has the command tool, all extensions; create "Eingebauter Agent" and
   assign the models of this device that have been used (`chat_messages.model_id`, `chat.last_active_model_id`). A
   second device with other settings creates "Eingebauter Agent (<device alias>)" so no device overwrites another's.
3. A marker preference (`agent_profiles.migrated_v1`, device scope) makes the step idempotent; every write is
   deterministic by id, so a crash in the middle repeats safely (FR-025).

The old rows stay (nothing is deleted in this stage); after the switch nothing reads them (R8).

## R8 Decision: old per-agent rows are dead data after PR A

After PR A `registry::load_rights` no longer reads `haex_mcp_agent_grants`; `save_rights` is replaced by an assignment
write. The rows are not deleted (rollback and audit), and a later cleanup spec may drop them. `manage::revoke` deletes
the agent's assignment row in the same write (explicit CRDT delete, the precedent of `passwords/trash.rs`).

## R9 Sync and older builds

`sync/handshake.rs` refuses a peer whose migration count or `HOLZI_TRIGGER_VERSION` differs (`RejectCode::Incompatible`).
Migration 0054 and the bump to 35 therefore partition the fleet until every device is updated; spec 059 already shows
the device versions. An older build never reads a profile row, so FR-012's risk is only inside one schema: a profile
row this build cannot read (unknown group, level or key) counts as no right, as in `grant_rows.rs`; a member row whose
profile does not exist counts as "Standard". Renumbering risk: other open branches may take 0054; the later one needs an
entry in `identity/migrations_legacy_names.rs`.

## R10 Registration and prompts

- `registration.rs` builds `RegistrationQuestion { rights, mode, delegate, ... }` and the window answers with
  `RegistrationAnswer { decision, remember, rights, mode }` (`prompts.rs`). Today the window edits rights; with
  profiles the question carries the effective rights of the profile the agent would get (read only, FR-013) and the
  answer carries `profileId`. `registry::register` writes an assignment instead of rights (none for "Standard").
- `manage::renew` (expired agent connects again) keeps the assignment (FR-014): there is nothing to restore, because
  the rights were never on the agent.
- Delegates register in the chat through "Rechte festlegen" (`delegates::register`,
  `settings.agents.mcp.delegate.register`); the same field "Agentenprofil" replaces the rights editor there.
- Remembered answers: `env.rs` `remember`, `remember_fill`, `remember_request` and the built-in `switch_on` go through
  `change_browser_rights`; it writes the profile of the subject (PR B), and the prompts in
  `agent_browser/prompts.rs` and `PromptDialog.vue` name the profile (FR-015). "Switch the browser on" for the built-in
  agent changes its profile, which may hold other models; the prompt says so.

## R11 UI

- Routes: `src/lib/settings/registry.ts` (`ai.profiles`, `ai.profiles.profile`, group `agents`), `components/wm/appRoutes.ts`,
  `scripts/check-settings.ts`, the e2e `settings-categories.test.ts`, i18n `settings.locations.*`.
- The profile editor is `RightsEditor` (`RightsFields`, `PasswordRightsFields`, `BrowserRightsFields`,
  `CommandRightsFields`, `ExtensionRightsFields`) with the profile's `Rights` as its draft, unchanged; it saves on
  selection (settings rule), errors as toasts.
- **Widening confirmation (FR-016)**: a pure function `widens(before, after)` in `src/lib/agents/profiles.ts` ranks
  each topic (none < read < write), passwords by level and tag reach, browser by level and blocked sites, commands and
  extensions by presence. A widening change on a profile with members opens a confirm dialog naming the member count;
  cancelling restores the control. A narrowing change saves at once.
- The agent page keeps mode, log and revoke; its rights editor becomes one `SettingsSelect` "Agentenprofil" plus the
  profile's rights shown read only (the editor components already take `disabled`).
- KI → Browser keeps the browser choice, trackers and the cookie profile (FR-021).
- Naming sweep ("Abo-Agent", never "Delegate"/"Delegat"): `src/i18n/locales/de.json` strings about delegates
  (the chat notices `delegate*`, the registration hint, the settings entries "Autonomie für Delegaten" and
  "Verbotsregeln für Delegaten", the keywords of the AI category), `en.json` likewise, and
  `RegistrationDialog.vue`. Keys keep their names; only the texts change. The provider-connection texts
  (`cliDelegate.*`, "Verbinden") describe a login, not an agent, and stay.

## R12 Alternatives rejected for the whole design

- **Profiles per domain** (the first idea): rejected by the operator for the one-step switch.
- **Profile as a template copied into each agent**: rejected, a change must reach every member (spec US2).
- **Approval mode inside the profile**: operator decided no (spec FR-017).
