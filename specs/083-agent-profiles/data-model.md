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
a profile id; absent or naming a profile that is gone means `STANDARD_PROFILE_ID`, a value this build cannot read
gives no right (FR-012), never a wider one.

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
  conflict. "Standard" is every subject's implicit membership and has no member row, with one exception: a **model** the
  user took out of "Standard" (FR-006b) has a member row in "Standard" (profile id of "Standard", id derived as for any
  other), which means "not in Standard", the exception to the rule; deleting the row puts it back, and a model that is
  added later is in "Standard" without a row. Exactly one profile is active.
- **Which models the chat offers** (`resolve::model_available`, `AgentSubjectView.available`): while "Standard" is active (or
  the active profile has no row, or its value cannot be read) every model without that row; while another profile is
  active only the models that have a member row in it. The chat refuses a turn with any other (`chat.error.modelNotInProfile`
  in `chat::send::start_turn`, before anything is stored); an Abo-Agent's session is not checked.
- **Member count**: the member rows of the profile in the vault. For "Standard": every agent row of every device
  (`haex_mcp_agents` is synced) and every listed model that did not leave it (below), and every agent that registers later.
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

## The group `delegate` (PR J3)

`grant_group` `delegate` (spec FR-030): a row `autonomy` at `standard`, `ungated` or `gated_permissive`, and a row
`deny:<category>` at `on` for each deny category (`workspace_escape`, `network_access`, `credential_paths`). Without a readable
`autonomy` row, or with a `deny:` row whose category this build does not know, the group is absent (reading skips less than the
user set, so an older build asks for every call instead). Absent means `standard`: every call asks. "Standard" without a row
of its own has the factory group `ungated` with no deny rules. `Rights.delegate: Option<DelegateRights { autonomy, deny }>`,
read by `agent_profiles::resolve::delegate_rights(model)`; the chat puts it into `ChatRequest.autonomy_mode` and
`autonomy_deny`, and the adapter's `DelegateChatContext.deny_rules` carries the deny rules to the approval bridge. The
maintenance step `agent_profiles::delegates::convert_autonomy_settings` moves the preferences `chat.autonomy_mode` and
`cli_delegate.deny_rules` into "Standard" once and deletes them.

## The group `contacts` (PR K1)

`grant_group` `contacts` (spec FR-032): a row `*` for all contacts and a row `group:<contact group id>` per group, at `read`,
`readWrite` or `denied`. A level this build does not know reads as `denied` (skipping the row would give more than the user
set); a key that is neither `*` nor a group is no right. `Rights.contacts: ContactsRights { all, groups }`; `rows()` gives
the `(target, status)` pairs the address book's checks (`contacts::agent::grants_from`, `asks`) have always read. Without an
entry the agent is asked. Replaces the rows of kind `contacts` of `agent_file_permissions`, which are no longer read.

## The group `files` (PR K2)

`grant_group` `files` (spec FR-033): the rows `device`, `shares` and `sync` at `on`, a row `storage:<storage id>` per storage and a
row `folder:<storage id>/<folder id>` per encrypted folder at `read`, `readWrite` or `denied` (a level this build does not know
reads as `denied`, a key it does not know is no right); a folder row for cloud models too carries the scope tags `["cloud"]`.
`Rights.files: FilesRights`, turned into the grants the file checks read by `FilesRights::grants()`; the device is granted or
refused there, for the chat's agent as for any other. Replaces `agent_file_permissions`, dropped by migration `0059`.
`Identity` (`agent_profiles/identity.rs`) reads the rights of a data action for the chat's agent (the model of the turn), a
registered agent and an Abo-Agent's model, and writes an answer into the profile it was asked in only. Removing a storage deletes
its `storage:` and `folder:` rows in every profile (`store::remove_storage_rights`).

## Provider memberships (PR M)

`haex_agent_profile_members.subject` may be `provider:<provider id>` (spec FR-034); no migration, the column is free text. A
row's id is `member_id(subject, profile)` as for any subject. `store::chain(subject)` is the subject and, for a model, the
provider of its row in `models`; `store::effective_memberships` adds the memberships of the chain: the profiles are the union,
`left_standard` and `unreadable` are true when either is. `resolve::applying` checks the deterministic id of every link in the
chain against the active profile, so a row that names another subject fails closed on the provider as on the model. A model that
has no row in `models` has only its own memberships. Deleting a provider's membership removes nothing else: the models simply
stop inheriting.

## Migration `0057` (PR H)

`haex_agent_profile_members` keeps its table. Its rows from PR B and C are one per subject with an id derived from the
subject alone; after PR H a new row's id derives from subject and profile. The reader accepts both ids: the id of
subject and profile and the legacy id of the subject alone (`legacy_member_id`); a row with any other id makes the subject
unreadable, so it gets no right while a profile other than "Standard" is active. **Legacy rows are never rewritten.** An
earlier design rewrote them on open (a delete and an insert with the new id), but that insert is a new row: a device that
upgrades offline after another device removed the membership would write it again, the sync would take it for newer than the
removal, and the removal's tombstone expires after 90 days, so no check against it lasts. Without a rewrite there is no such
insert, and a removal deletes the row that holds the membership (matched by subject and profile, whatever its id), which every
device applies as an ordinary delete. Adding checks for the membership by subject and profile too, so a legacy row is never
doubled. The migration adds the non-unique index `haex_agent_profile_members_subject` (SQL cannot compute a UUIDv5) and
nothing else. The active profile starts as "Standard". `HOLZI_TRIGGER_VERSION` rises to 37, so a device on an older version is refused by the
sync handshake, as for any schema change.

## The reset

There is no migration state. The old rows of `haex_mcp_agent_grants` are not read after PR B (dropped in PR E, research R8);
the local agent's two old preferences are deleted by an idempotent maintenance step in PR C (research R7). Nothing marks a
step as done, because nothing is converted.

## Bindings

`Rights`, `BrowserRights`, `PasswordRights`, `SiteLevel`, `OperateLevel` are unchanged. New exported types:
`AgentProfileView`, `AgentProfileDetail`, `AgentSubjectView`, and the arguments of the commands in
[contracts/commands-and-actions.md](./contracts/commands-and-actions.md).
