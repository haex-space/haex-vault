# Contracts: commands and actions of agent profiles

**Feature**: [spec.md](../spec.md) | **Data model**: [data-model.md](../data-model.md)

Tauri commands live in `src-tauri/src/agent_profiles/commands.rs`; their argument and result types are exported by
`ts-rs` to `src/types/bindings/`. Catalog actions live in `src/lib/actions/settingsAgentProfileActions.ts`. Every
action is a guardrail (`scope: 'guardrails'`, `agentCallable: false`, `binding: 'global'`): no agent can change a
profile or an assignment (spec 021 FR-016). Errors reach the user as toasts; the commands return typed error codes as the reason of an invalid input: `name_empty`, `standard_fixed`, `profile_missing`, `subject_unknown`.

## Commands

| Command                    | Arguments                        | Result               | Notes                                                                                                                                                                                               |
| -------------------------- | -------------------------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent_profiles_list`      | none                             | `AgentProfileView[]` | `{ id, name, isStandard, isActive, memberCount }`, the count vault wide (data-model.md, Member count); "Standard" first, then by name                                                               |
| `agent_profile_get`        | `{ id }`                         | `AgentProfileDetail` | `{ id, name, isStandard, rights: Rights, members: AgentSubjectView[] }`                                                                                                                             |
| `agent_profile_create`     | `{ name }`                       | `AgentProfileView`   | rights start empty; error `name_empty`; a name may repeat (Q4)                                                                                                                                      |
| `agent_profile_rename`     | `{ id, name }`                   | none                 | error `standard_fixed`, `name_empty`                                                                                                                                                                |
| `agent_profile_delete`     | `{ id }`                         | `{ moved: number }`  | error `standard_fixed`; removes its memberships (`moved` counts them) and makes "Standard" active if it was; one write                                                                              |
| `agent_profile_rights_set` | `{ id, rights: Rights }`         | none                 | `grants::validate` first (guardrails, plain http, blocked all); refreshes live sessions                                                                                                             |
| `agent_membership_set`     | `{ subject, profileId, member }` | none                 | adds or removes one membership (replaces `agent_assign`, PR H); "Standard": agents have none (`standard_fixed`), a model can leave it and come back (FR-006b); unknown profile is `profile_missing` |
| `agent_profile_active_get` | none                             | `{ profileId }`      | the active profile; "Standard" when unset or gone                                                                                                                                                   |
| `agent_profile_active_set` | `{ profileId }`                  | none                 | makes a profile active; unknown profile is `profile_missing`; refreshes live sessions                                                                                                               |
| `agent_subjects_list`      | none                             | `AgentSubjectView[]` | the agents of every device and the built-in models, each `{ subject, kind, name, device, profileId, profileName, status? }`                                                                         |

`AgentSubjectView` carries `profiles: { id, name }[]` (the memberships, "Standard" not among them), `profileId`/`profileName` (the profile that applies to it now), `inStandard` and `available` (PR I: whether a model is in "Standard", and whether the chat offers it now, FR-006b; always `true` for an agent). `AgentSubjectView.kind` is `external`, `subscription` (Abo-Agent) or `builtin`. `device` is the alias of the device
an agent is registered on (absent for a model). `status` is present for registered agents (`registered`, `expired`,
`blocked`). A model that is assigned but not offered any more is listed with `missing: true` so it can be removed
(FR-008). `local: true` marks a model of the local provider (downloaded or imported on any device of the vault, whatever the
state of its file here); the agent page lists those as rows.

## Changed commands

| Command                                 | Change                                                                                                                                                                                                                                     |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mcp_agents_list`, `mcp_agent_get`      | carry `profileId` and `profileName`; `rights` is the effective rights of the profile (read only)                                                                                                                                           |
| `mcp_registration_answer`               | `RegistrationAnswer { decision, remember, profileId, mode }`; `rights` is dropped; `grants::validate` is no longer needed on the answer; an unknown `profileId` is `profile_missing` and the question stays open                           |
| `RegistrationQuestion`                  | `rights` is the effective rights of the profile the agent would get; adds `profiles: { id, name }[]` and `profileId` (the preselected one: "Standard", or the current profile on a renewal)                                                |
| OAuth consent (`oauth_flow.rs`)         | the same question and answer as the registration, the same `profile_missing`; `oauth_rows::authorize` writes an assignment instead of rights                                                                                               |
| `settings.agents.mcp.delegate.register` | input `{ kind, profileId, mode }` instead of rights                                                                                                                                                                                        |
| `mcp_agent_revoke`                      | also deletes the agent's member row                                                                                                                                                                                                        |
| `BrowserQuestion` (agent browser)       | adds `profile: BrowserProfileNote \| null` (`{ id, name, isStandard, memberCount }`), set for an external agent so the dialog can say that a remembered answer holds for every agent in the profile; `null` for the local agent until PR C |
| `agent_browser_status`                  | `builtin` and `builtinPasswords` are removed (PR C)                                                                                                                                                                                        |

## Removed (one implementation per rule)

- **PR B**: `mcp_agent_rights_set` and the action `settings.agents.mcp.grants.set`; `registry::save_rights`. They have nothing to
  write to once `load_rights` resolves through profiles.
- **PR C**: `agent_browser_builtin_set`, `agent_browser_builtin_passwords_set` and the actions
  `settings.agentBrowser.builtin.set`, `settings.agentBrowser.builtinPasswords.set`; `agent_browser_status` loses `builtin` and
  `builtinPasswords`.
- **PR E**: the table `haex_mcp_agent_grants` (migration `0056_drop_agent_grants`).

## Catalog actions

| Id                                    | Input                                                                                                        | Effect      | Handler                    |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ----------- | -------------------------- |
| `settings.agents.profiles.create`     | `{ name }`                                                                                                   | write       | `agent_profile_create`     |
| `settings.agents.profiles.rename`     | `{ id, name }`                                                                                               | write       | `agent_profile_rename`     |
| `settings.agents.profiles.delete`     | `{ id }`                                                                                                     | destructive | `agent_profile_delete`     |
| `settings.agents.profiles.rights.set` | `{ id, rights }`, `rights` with the schema of today's `grants.set` plus `requests` and `blocked` site levels | write       | `agent_profile_rights_set` |
| `settings.agents.profiles.activate`   | `{ profileId }`                                                                                              | write       | `agent_profile_active_set` |
| `settings.agents.profiles.assign`     | `{ subject, profileId, member }` (PR H: adds or removes one membership)                                      | write       | `agent_assign`             |

The schema of `rights.set` reuses the schema object of the removed `grants.set` and of
`settings.agentBrowser.builtin.set`; `scripts/check-mcp-actions.ts` keeps its test that everything the editor emits is
accepted by the schema (the "unknown property" bug of 2026-11-05).

## Events

`EVENT_AGENTS_CHANGED` (existing) is also emitted after every profile or assignment write, so the agent list, the agent
page and the profile pages reload; rows that arrive by sync reach the pages through `vault-data-changed`. MCP sessions
are refreshed from `core.vault_changes` for the three tables, local and synced commits alike, and get
`notifications/tools/list_changed` when their rights differ (research R5); `attach::tell_catalog_changes` does this per
session. An agent that loses its browser right through a profile gets `Notice::BrowserRightLost`, which ends its
browser session, as a change of its rights by hand did before.

## Chat

`chat::send::start_turn` refuses a turn whose model is not offered (FR-006b) with `InvalidInput` and the language key
`chat.error.modelNotInProfile` as its reason, before it stores anything; `useErrorString` shows the text. The composer's settings
(`ComposerSettingsPopover`, `ComposerSettingsDrawer`) get the select `ComposerProfileSelect` ("Agentenprofil", FR-006c), which
reads the store `agentProfiles` and runs `settings.agents.profiles.activate`; the model list is filtered by
`unavailableModelIds` of the same store.

## Chat notice

`chat.agentProfile.restricted.*` (new, de/en): shown once per session and right when the model of the turn is in a
profile that does not give what the chat just asked for. It names the profile and the missing right, one variant per
kind: `browser`, `passwords`, `topic` (a topic the profile does not list, with the topic), `topicWrite` (a topic it only reads, with the topic), `commands`, `extension` (with its name). The event is `chat-profile-restricted` with `{ threadId, profileId, profileName, missing }`; the backend sends it at most once per turn, the window shows it once per conversation. Example for
`browser`: "Dieses Modell ist im Agentenprofil {profile} und darf den Browser nicht verwenden." It never claims the
model may only read. Wording is final at implementation; it follows the pattern of the existing
`delegateUnregistered` notice.
