# Data model: sync rules for single files

Spec 061's model (`specs/061-sync-rules/data-model.md`, with the names of spec 075) is unchanged except for what is
listed here. A folder rule has exactly the fields and behaviour it had.

## New type: `SyncRuleSourceKind`

Exported to `src/types/bindings/SyncRuleSourceKind.ts`.

| Value    | Stored text | Meaning                                                          |
| -------- | ----------- | ---------------------------------------------------------------- |
| `folder` | `folder`    | the source place is a folder (every rule until now)              |
| `file`   | `file`      | the source place is one file; the target place is still a folder |

## `SyncRule` and `SyncRuleDraft`

| Field         | Type                                                                              | Rule                                                                                                   |
| ------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `source_kind` | `SyncRuleSourceKind` (`SyncRule`); `Option<SyncRuleSourceKind>` (`SyncRuleDraft`) | `SyncRule`: always present. `SyncRuleDraft`: `sourceKind?` on the wire, see below for a missing value. |

**What a missing `sourceKind` means** (resolved in `commands::create` and `commands::update` before the draft is validated,
so the agent tools and the window share it): on create it is `folder`; on update it keeps the kind the stored rule has. A
draft that names the kind sets it. The window always sends it. This keeps an agent's update of a file rule, which
replaces all settings, from turning it into a folder rule by leaving the field out.

For a file rule:

- `source.path` is the window path of the file, as the file browser names it (`/Dokumente/Haushalt.ods`, or a
  device path). The folder it lies in and its name are derived where the place is opened.
- `target.path` is a folder. The copy lies in it under the file's name.
- `delete_mode` is stored as `keep`; validation sets it. It has no effect, since a file rule deletes nothing.
- `direction` is `sourceToTarget` or `both`, as before.

## Validation (`rules::validate`)

| Check                                                                                                        | Error          |
| ------------------------------------------------------------------------------------------------------------ | -------------- |
| a device source of a file rule is a regular file, not a link, not a folder, not missing (`symlink_metadata`) | `invalidPlace` |
| a file rule's source path has a file name (is not a root)                                                    | `invalidPlace` |
| the claims of the rule overlap another rule's claims, or each other (research R6)                            | `overlap`      |
| everything spec 061 checks for a place (contact's share, holzi's own places, `..`, absolute device path)     | as before      |

**Claims of a rule** (for overlap): a folder rule claims `source.path` and `target.path`. A file rule claims
`source.path` and `target.path` joined with the source file's name. Two claims overlap when one is the other or
lies in it, by path components, for the same location.

## `SyncRuleOutcome`

New value `InvalidSource` (`invalidSource`): the run found no file at the source path of a file rule and did
nothing. Stored in `last_outcome`; the error text holds the reason. A build that does not know the value reads the
column as "no outcome" (as it does for any unknown value).

| Reason text (stored in `last_error`) | Meaning                                                                      |
| ------------------------------------ | ---------------------------------------------------------------------------- |
| `noFile`                             | the folder is there and no file is at the path (also when a folder is there) |
| `folderGone`                         | a folder above the file is missing (renamed, moved, deleted)                 |

The reasons are keys, not sentences; the window translates them (`files.syncRules.file.invalidReason.*`), as it
does for skipped files.

## `SyncRuleState` (status)

New value `InvalidSource` (`invalidSource`). `status_of` maps `last_outcome == InvalidSource` to it for an enabled,
idle rule. `reason` carries the reason key.

## `SyncRuleErrorCode`

Two new values. `FileRule` (`fileRule`): the action does not apply to a file rule (the exclusion editor). `InvalidSource` (`invalidSource`): the preview of a file rule found no file at the source path; the message is the reason key.

## Per-file state

Unchanged. For a file rule the table `haex_sync_rule_files_no_sync` holds at most one row, with `path` = the file's
name. Moves are never detected (there is one entry per side).

## State transitions of a file rule

```text
                 source file at the path
      ┌────────────────────────────────────────────┐
      ▼                                            │
   Active ── no file at the path ──▶ InvalidSource ┘  (a file at the path again: next run, no step by the user)
      │                                  │
      │ mount root / peer / storage      │ edit: pick another file → Active at the next run
      │ not reachable                    │ delete the rule → nothing else changes
      ▼
   Waiting ── place back ──▶ Active
```

`Paused` and `Running` are as for a folder rule and sit above these.
