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
| `haex_agent_profile_members` | `id`          | TEXT PK                  | UUIDv5 of `{subject}:{profile}` (revised: was of the subject alone, FR-006)    |
|                              | `subject`     | TEXT NOT NULL            | `agent:<haex_mcp_agents.id>` or `model:<model id>`                             |
|                              | `profile_id`  | TEXT NOT NULL            | a profile the subject is a member of; one row per membership                   |

**The active profile** is not a table: it is the vault preference `agent_profile.active` (synced, `PrefScope::Vault`) holding
a profile id; absent, unreadable or naming a profile that is gone means `STANDARD_PROFILE_ID`.

The columns of `haex_agent_profile_grants` after `profile_id` and their group, key and level semantics are those of
`mcp_server/grant_rows.rs` (header comment); the code that turns rows into `Rights` and back is shared, not copied.
Browser rows include the block list of spec 065 FR-047 (`blocked`), the level for all websites (`*`) and the request
rules (`request:<site>`).

## Rules

- **Standard**: `STANDARD_PROFILE_ID` is a fixed UUID. While `haex_agent_profiles` has no row with it, its rights are
  `grants::default_rights()`. The first edit inserts the profile row and the grant rows. It cannot be deleted or
  renamed.
- **Name**: free text, not unique (decided, research R13 Q4; FR-002): two profiles may have the same name. Only `name_empty`
  is refused. The profile id is the identity; the resolver and every row refer to ids, never to names.
- **Any number of profiles per subject** (revised 2026-11-06, FR-006): one member row per `{subject, profile}`, with an id
  derived from both (R3), so two devices that add the same membership write the same row and different memberships never
  conflict. "Standard" is every subject's implicit membership and has no member row. Exactly one profile is active.
- **Member count**: the member rows of the profile in the vault. For "Standard": every agent row of every device
  (`haex_mcp_agents` is synced) and every listed model (below), and every agent that registers later, so a widening
  change of "Standard" always asks (R2, R11).
- **Dangling profile**: a member row whose `profile_id` has no profile row counts as "Standard".
- **Unreadable rows**: a grant row with an unknown group, level or key is skipped (less, never more). PR A makes the
  shared converter strict where it is not today: `commands` only with the empty key, `operate` only with a key of
  `OPERATE_SCOPES` (research R9).
- **Delete a profile**: in one write, delete its grant rows, delete its member rows (the subjects keep their other
  memberships), delete the profile row, and set the active profile to "Standard" when it was the active one. The command returns the number of subjects moved, and the page asks first (FR-009).
- **Delete an agent** (`manage::revoke`): also delete all of `agent:<id>`'s member rows.
- **Model that is gone**: its `model:` member row stays until the user removes it (FR-008).
- **Which models are listed** (`agent_profiles::subjects`): every model row of the local chat provider, whatever the state
  of its file on this device (the row exists once a file was downloaded or imported on any device); a model row whose
  provider row is not here yet counts as local when it has a file hash; the chat models of API providers; every model with a
  member row. `AgentSubjectView.local` marks the first group, so the page lists those as rows and the API models in a pick list.

## Subject

```text
Subject = Agent(agent_id)        -- external agent or Abo-Agent, a row of haex_mcp_agents on this device
        | Model(model_id)        -- a model of the local agent (catalog id, or provider id : remote id)
kind label (UI) = "Externer Agent" | "Abo-Agent" | "Lokaler Agent"
```

The label of an `Agent` follows `haex_mcp_agents.kind` (`external` is an Externer Agent, `claude_code_delegate` and
`codex_delegate` are Abo-Agents); a `Model` is always a local agent (UI label: "Lokaler Agent").

## Resolution

```text
rights_of(subject):
  active  = the preference agent_profile.active, else STANDARD
  profile = active if subject has a member row with it (or active is STANDARD) and that profile exists, else STANDARD
  if profile == STANDARD and it has no profile row: return default_rights()
  return grant_rows::rights_of(rows of profile)      -- unknown rows skipped
```

Called by: the MCP connection admission and refresh (`connection.rs`, `attach.rs`, `delegates.rs`, `oauth_flow.rs`,
`live.rs`), `password_env.rs`, `agent_browser/env.rs` (`rights`, `password_rights`), the local agent's offer of each
step and dispatch of each call (`chat/tools/offer.rs`, `chat/turn/`), and `mcp_agent_get`/`list_with_rights` for the
pages. For the local agent the subject is the model of the turn, read from the task-local `AgentTask`
(`chat/tools/agent_task.rs`); a call outside a turn resolves as "Standard" (research R6).

## State changes

| Change                                 | Writes                                                                               | Then                                    |
| -------------------------------------- | ------------------------------------------------------------------------------------ | --------------------------------------- |
| create profile                         | profile row                                                                          | feed (below)                            |
| rename                                 | profile row                                                                          | feed                                    |
| set rights of a profile                | delete and insert its grant rows (and the profile row for "Standard" the first time) | feed; an agent change event for pages   |
| add a subject to P / remove it         | member row (id from subject and P); remove deletes only that row                     | feed; an agent change event for pages   |
| make P active                          | the preference `agent_profile.active`                                                | feed (preference); every live session   |
| delete a profile                       | see Rules                                                                            | feed; an agent change event for pages   |
| remember (website, fill, request rule) | rows of the profile of the subject (browser group)                                   | feed                                    |
| agent registers                        | member rows of the profiles the user chose (none beyond "Standard" by default)       | feed                                    |
| any of the three tables by sync        | the rows another device wrote                                                        | feed; the pages reload from their event |

**Feed**: every commit that touches `haex_agent_profiles`, `haex_agent_profile_grants` or
`haex_agent_profile_members` reaches `core.vault_changes`, whether it was written here or arrived by sync. The MCP
server re-resolves the rights of every live session from it (research R5). No writer calls a refresh of its own. A
change of the preference `agent_profile.active` (table `preferences`, key `agent_profile.active`) feeds the same
re-resolution; the feed filter of the MCP server takes that one key as well.

## Migration `0057` (PR H)

`haex_agent_profile_members` keeps its table. Its rows from PR B and C are one per subject with an id derived from the
subject alone; after PR H the id derives from subject and profile (the validator of a row checks it). The migration
rewrites the ids of the rows that exist (`UPDATE` with the id built from `subject` and `profile_id`), so every existing
assignment stays as a membership; the active profile starts as "Standard". `HOLZI_TRIGGER_VERSION` rises by one.
Devices on an older version are refused by the sync handshake as for any schema change.

## The reset

There is no migration state. The old rows of `haex_mcp_agent_grants` are not read after PR B (dropped in PR E, research R8);
the local agent's two old preferences are deleted by an idempotent maintenance step in PR C (research R7). Nothing marks a
step as done, because nothing is converted.

## Bindings

`Rights`, `BrowserRights`, `PasswordRights`, `SiteLevel`, `OperateLevel` are unchanged. New exported types:
`AgentProfileView`, `AgentProfileDetail`, `AgentSubjectView`, and the arguments of the commands in
[contracts/commands-and-actions.md](./contracts/commands-and-actions.md).
