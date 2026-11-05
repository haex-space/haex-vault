# Data Model: Agent profiles

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

## Tables (migration `0054_agent_profiles`, synced, vault wide)

No `vault_device_uuid`, no UNIQUE besides the primary key (a conflict halts the sync), no cascading foreign key
(deletes are explicit CRDT deletes in the same write, so each leaves its marker; `passwords/trash.rs` is the
precedent). `HOLZI_TRIGGER_VERSION` 34 to 35, with a line in its history comment.

| Table                        | Column        | Type                     | Meaning                                                                        |
| ---------------------------- | ------------- | ------------------------ | ------------------------------------------------------------------------------ |
| `haex_agent_profiles`        | `id`          | TEXT PK                  | UUID; `STANDARD_PROFILE_ID` for "Standard"                                     |
|                              | `name`        | TEXT NOT NULL            | shown name; for "Standard" the label is translated, the stored name is ignored |
| `haex_agent_profile_grants`  | `id`          | TEXT PK                  | UUIDv5 of `{profile}:{group}:{key}`                                            |
|                              | `profile_id`  | TEXT NOT NULL            | the profile                                                                    |
|                              | `grant_group` | TEXT NOT NULL            | `operate`, `browser`, `passwords`, `extension`, `commands`                     |
|                              | `grant_key`   | TEXT NOT NULL DEFAULT '' | as in `haex_mcp_agent_grants`                                                  |
|                              | `level`       | TEXT NOT NULL            | as in `haex_mcp_agent_grants`                                                  |
|                              | `scope_tags`  | TEXT                     | as in `haex_mcp_agent_grants`                                                  |
| `haex_agent_profile_members` | `id`          | TEXT PK                  | UUIDv5 of the subject (`NS_PROFILE_MEMBER`)                                    |
|                              | `subject`     | TEXT NOT NULL            | `agent:<haex_mcp_agents.id>` or `model:<model id>`                             |
|                              | `profile_id`  | TEXT NOT NULL            | the profile the subject is in                                                  |

The columns of `haex_agent_profile_grants` after `profile_id` and their group, key and level semantics are those of
`mcp_server/grant_rows.rs` (header comment); the code that turns rows into `Rights` and back is shared, not copied.
Browser rows include the block list of spec 065 FR-047 (`blocked`), the level for all websites (`*`) and the request
rules (`request:<site>`).

## Rules

- **Standard**: `STANDARD_PROFILE_ID` is a fixed UUID. While `haex_agent_profiles` has no row with it, its rights are
  `grants::default_rights()`. The first edit inserts the profile row and the grant rows. It cannot be deleted or
  renamed.
- **Name**: unique among the profiles this device has seen, checked on every create and rename (`name_taken`). Two
  devices that create the same name before they sync can both succeed (decided, research R13 Q4; FR-002 says so):
  the list marks the duplicates and the next edit of one asks for a new name. The resolver never depends on names.
- **At most one profile per subject**: by the deterministic member id (R3). No member row means "Standard".
- **Member count**: the member rows of the profile in the vault. For "Standard": the agent rows of every device
  without a member row (`haex_mcp_agents` is synced) plus the offered built-in models without one, and every agent
  that registers later, so a widening change of "Standard" always asks (R2, R11).
- **Dangling profile**: a member row whose `profile_id` has no profile row counts as "Standard".
- **Unreadable rows**: a grant row with an unknown group, level or key is skipped (less, never more). PR A makes the
  shared converter strict where it is not today: `commands` only with the empty key, `operate` only with a key of
  `OPERATE_SCOPES` (research R9).
- **Delete a profile**: in one write, delete its grant rows, delete its member rows (the subjects fall back to
  "Standard"), delete the profile row. The command returns the number of subjects moved, and the page asks first (FR-009).
- **Delete an agent** (`manage::revoke`): also delete `agent:<id>`'s member row.
- **Model that is gone**: its `model:` member row stays until the user removes it (FR-008).

## Subject

```text
Subject = Agent(agent_id)        -- external agent or Abo-Agent, a row of haex_mcp_agents on this device
        | Model(model_id)        -- a model of the built-in agent (catalog id, or provider id : remote id)
kind label (UI) = "Externer Agent" | "Abo-Agent" | "Eingebauter Agent"
```

The label of an `Agent` follows `haex_mcp_agents.kind` (`external` is an Externer Agent, `claude_code_delegate` and
`codex_delegate` are Abo-Agents); a `Model` is always an Eingebauter Agent.

## Resolution

```text
rights_of(subject):
  profile = member(subject).profile_id if that profile exists, else STANDARD
  if profile == STANDARD and it has no profile row: return default_rights()
  return grant_rows::rights_of(rows of profile)      -- unknown rows skipped
```

Called by: the MCP connection admission and refresh (`connection.rs`, `attach.rs`, `delegates.rs`, `oauth_flow.rs`,
`live.rs`), `password_env.rs`, `agent_browser/env.rs` (`rights`, `password_rights`), the built-in agent's offer of each
step and dispatch of each call (`chat/tools/offer.rs`, `chat/turn/`), and `mcp_agent_get`/`list_with_rights` for the
pages. For the built-in agent the subject is the model of the turn, read from the task-local `AgentTask`
(`chat/tools/agent_task.rs`); a call outside a turn resolves as "Standard" (research R6).

## State changes

| Change                                 | Writes                                                                               | Then                                    |
| -------------------------------------- | ------------------------------------------------------------------------------------ | --------------------------------------- |
| create profile                         | profile row                                                                          | feed (below)                            |
| rename                                 | profile row                                                                          | feed                                    |
| set rights of a profile                | delete and insert its grant rows (and the profile row for "Standard" the first time) | feed; an agent change event for pages   |
| assign a subject to P                  | member row (id from the subject) with P; for "Standard" delete the member row        | feed; an agent change event for pages   |
| delete a profile                       | see Rules                                                                            | feed; an agent change event for pages   |
| remember (website, fill, request rule) | rows of the profile of the subject (browser group)                                   | feed                                    |
| agent registers                        | member row if the profile is not "Standard"                                          | feed                                    |
| any of the three tables by sync        | the rows another device wrote                                                        | feed; the pages reload from their event |

**Feed**: every commit that touches `haex_agent_profiles`, `haex_agent_profile_grants` or
`haex_agent_profile_members` reaches `core.vault_changes`, whether it was written here or arrived by sync. The MCP
server re-resolves the rights of every live session from it (research R5). No writer calls a refresh of its own.

## Migration state

`agent_profiles.migrated_v1` (PR A, the agents) and `agent_profiles.builtin_migrated_v1` (PR C, the built-in agent):
device-scoped preferences, each set after its step of the post-open migration (R7) has finished. A step is
repeatable: profiles it creates have deterministic ids (`UUIDv5("migrated-agent:{device}:{agent}")`; the built-in
profile has the fixed id `UUIDv5("builtin-profile")`, one per vault, research R13 Q1).

## Bindings

`Rights`, `BrowserRights`, `PasswordRights`, `SiteLevel`, `OperateLevel` are unchanged. New exported types:
`AgentProfileView`, `AgentProfileDetail`, `AgentSubjectView`, and the arguments of the commands in
[contracts/commands-and-actions.md](./contracts/commands-and-actions.md).
