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
- **Name**: unique among profiles, checked on every create and rename. Two devices that create the same name at the
  same time can both succeed; the page then shows both and the user renames one. The resolver never depends on names.
- **At most one profile per subject**: by the deterministic member id (R3). No member row means "Standard".
- **Dangling profile**: a member row whose `profile_id` has no profile row counts as "Standard".
- **Unreadable rows**: a grant row with an unknown group, level or key is skipped (less, never more), as today.
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

Called by: the MCP connection admission and refresh (`connection.rs`, `attach.rs`, `delegates.rs`, `live.rs`),
`password_env.rs`, `agent_browser/env.rs` (`rights`, `password_rights`, built-in via the active model), the built-in
agent's tool registration (`chat/action_commands.rs`), and `mcp_agent_get`/`list_with_rights` for the pages.

## State changes

| Change                                 | Writes                                                                               | Then                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| create profile                         | profile row                                                                          | none                                                                |
| rename                                 | profile row                                                                          | none                                                                |
| set rights of a profile                | delete and insert its grant rows (and the profile row for "Standard" the first time) | refresh every live MCP session; an agent change event for the pages |
| assign a subject to P                  | member row (id from the subject) with P; for "Standard" delete the member row        | refresh live sessions of that subject                               |
| delete a profile                       | see Rules                                                                            | refresh every live session                                          |
| remember (website, fill, request rule) | rows of the profile of the subject (browser group)                                   | refresh live sessions                                               |
| agent registers                        | member row if the profile is not "Standard"                                          | none                                                                |

## Migration state

`agent_profiles.migrated_v1`: device scoped preference; set after the post-open migration (R7) has finished. The step
is repeatable: profiles it creates have deterministic ids (`UUIDv5("migrated-agent:{device}:{agent}")`,
`UUIDv5("migrated-builtin:{device}")`).

## Bindings

`Rights`, `BrowserRights`, `PasswordRights`, `SiteLevel`, `OperateLevel` are unchanged. New exported types:
`AgentProfileView`, `AgentProfileDetail`, `AgentSubjectView`, and the arguments of the commands in
[contracts/commands-and-actions.md](./contracts/commands-and-actions.md).
