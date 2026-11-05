# Contracts: commands and actions of agent profiles

**Feature**: [spec.md](./spec.md) | **Data model**: [data-model.md](../data-model.md)

Tauri commands live in `src-tauri/src/agent_profiles/commands.rs`; their argument and result types are exported by
`ts-rs` to `src/types/bindings/`. Catalog actions live in `src/lib/actions/settingsAgentProfileActions.ts`. Every
action is a guardrail (`scope: 'guardrails'`, `agentCallable: false`, `binding: 'global'`): no agent can change a
profile or an assignment (spec 021 FR-016). Errors reach the user as toasts; the commands return typed error codes.

## Commands

| Command                    | Arguments                | Result               | Notes                                                                                                                           |
| -------------------------- | ------------------------ | -------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `agent_profiles_list`      | none                     | `AgentProfileView[]` | `{ id, name, isStandard, memberCount, memberCountScope }`; "Standard" first, then by name                                       |
| `agent_profile_get`        | `{ id }`                 | `AgentProfileDetail` | `{ id, name, isStandard, rights: Rights, members: AgentSubjectView[] }`                                                         |
| `agent_profile_create`     | `{ name }`               | `AgentProfileView`   | rights start empty; error `name_taken`, `name_empty`                                                                            |
| `agent_profile_rename`     | `{ id, name }`           | none                 | error `standard_fixed`, `name_taken`, `name_empty`                                                                              |
| `agent_profile_delete`     | `{ id }`                 | `{ moved: number }`  | error `standard_fixed`; moves members to "Standard" in the same write                                                           |
| `agent_profile_rights_set` | `{ id, rights: Rights }` | none                 | `grants::validate` first (guardrails, plain http, blocked all); refreshes live sessions                                         |
| `agent_assign`             | `{ subject, profileId }` | none                 | `profileId` of "Standard" deletes the member row; unknown profile is `profile_missing`                                          |
| `agent_subjects_list`      | none                     | `AgentSubjectView[]` | the agents of this device and the models of the built-in agent, each `{ subject, kind, name, profileId, profileName, status? }` |

`AgentSubjectView.kind` is `external`, `subscription` (Abo-Agent) or `builtin`. `status` is present for registered
agents (`registered`, `expired`, `blocked`). A model that is assigned but not offered any more is listed with
`missing: true` so it can be removed (FR-008).

## Changed commands

| Command                                 | Change                                                                                                                                     |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `mcp_agents_list`, `mcp_agent_get`      | carry `profileId` and `profileName`; `rights` is the effective rights of the profile (read only)                                           |
| `mcp_registration_answer`               | `RegistrationAnswer { decision, remember, profileId, mode }`; `rights` is dropped; `grants::validate` is no longer needed on the answer    |
| `RegistrationQuestion`                  | `rights` is the effective rights of the profile the agent would get; adds `profiles: { id, name }[]` and `profileId` (the preselected one) |
| `settings.agents.mcp.delegate.register` | input `{ kind, profileId, mode }` instead of rights                                                                                        |
| `mcp_agent_revoke`                      | also deletes the agent's member row                                                                                                        |
| `agent_browser_status`                  | `builtin` and `builtinPasswords` are removed (PR E)                                                                                        |

## Removed (PR E, one implementation per rule)

`mcp_agent_rights_set` and the action `settings.agents.mcp.grants.set`; `agent_browser_builtin_set`,
`agent_browser_builtin_passwords_set` and the actions `settings.agentBrowser.builtin.set`,
`settings.agentBrowser.builtinPasswords.set`; `registry::save_rights`.

## Catalog actions

| Id                                    | Input                                                                                                        | Effect      | Handler                    |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ----------- | -------------------------- |
| `settings.agents.profiles.create`     | `{ name }`                                                                                                   | write       | `agent_profile_create`     |
| `settings.agents.profiles.rename`     | `{ id, name }`                                                                                               | write       | `agent_profile_rename`     |
| `settings.agents.profiles.delete`     | `{ id }`                                                                                                     | destructive | `agent_profile_delete`     |
| `settings.agents.profiles.rights.set` | `{ id, rights }`, `rights` with the schema of today's `grants.set` plus `requests` and `blocked` site levels | write       | `agent_profile_rights_set` |
| `settings.agents.profiles.assign`     | `{ subject, profileId }`                                                                                     | write       | `agent_assign`             |

The schema of `rights.set` reuses the schema object of the removed `grants.set` and of
`settings.agentBrowser.builtin.set`; `scripts/check-mcp-actions.ts` keeps its test that everything the editor emits is
accepted by the schema (the "unknown property" bug of 2026-11-05).

## Events

`EVENT_AGENTS_CHANGED` (existing) is also emitted after every profile or assignment write, so the agent list, the agent
page and the profile pages reload. MCP sessions get `notifications/tools/list_changed` through `live.rs` when their
tools differ.

## Chat notice

`chat.agentProfile.restricted` (new, de/en): shown once per session when the active model of the built-in agent is in
a profile that does not give what the chat just asked for, or is in "Standard" with its factory rights: "Dieses Modell
ist im Agentenprofil {profile} und darf nur lesen." Wording is final at implementation; it follows the pattern of the
existing `delegateUnregistered` notice.
