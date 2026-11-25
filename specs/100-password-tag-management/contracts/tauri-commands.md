# Contract: commands

Extends `specs/034-password-manager/contracts/tauri-commands.md` (tags). Rename, color and delete
commands take `tagIds: string[]`, the complete set of ids represented by the selected folded-name
tag, and apply the change atomically to every id.

## `passwords_merge_tags`

```text
args:   { sourceIds: string[], targetIds: string[], name: string | null }
result: null
errors: InvalidInput { reason: 'same' }      // source and target are the same tag
        InvalidInput { reason: 'empty' | 'too_long' | 'exists' }   // only for a given name
        PasswordsNotFound                    // either tag does not exist (also: already merged)
```

`sourceIds` and `targetIds` contain every id in the respective folded-name display group. All source
ids merge into the first target id; the target group remains a single displayed tag. `name` names the
merged tag; `null` keeps the name of the target. A given name is trimmed and follows
the rules of `passwords_rename_tag` (not empty, at most 64 characters, not the name of a tag that
stays by `fold`), except that the name of the source is free because the source goes. A name equal to
the target's name changes nothing. Every refusal is raised before anything is written.

Caller: the user only (`Caller::User`). Agents, extensions and the browser bridge never reach it: it is a
frontend command like its siblings, and the service refuses any other caller (`require_user`).

Atomic: the merge and the rename are one write. Every removed row (each moved link, the source) carries
a delete marker.

## `passwords_tag_agent_profiles`

```text
args:   { tagId: string }
result: string[]        // names of agent profiles whose password rights name the tag, sorted, unique
errors: PasswordsNotFound                    // no such tag
```

Read only. A profile names the tag through its tag list or its default tag, compared by `fold`.
