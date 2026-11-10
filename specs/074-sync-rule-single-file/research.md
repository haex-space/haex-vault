# Research: sync rules for single files

Each decision answers one open point of the spec or one risk found while reading the `file_sync` module, the
folder picker and the rule dialog on 2026-11-09. Nothing here needed a web source.

## R1 - How the engine sees a file place

**Decision**: A file rule runs on the existing engine. For a file rule, each side is a `OneFilePlace`: a wrapper
that holds the place of the folder the file lies in (the source file's parent, the target folder) and the file's
name. Its `list_tree` is one `stat` of that name: a listing with one file, or none. Every other `Place` method
passes through to the inner place unchanged, with the name as the relative path. `read_text` answers "no file" for
the exclusion file (R5).

The places in the code that open a rule's two places (the run in `service.rs`; in `commands.rs` the pending
deletion's resolution, the preview, and `open_source` for the exclusion editor) go through one new function
`open_rule_place(app, rule, side, want)` that does the wrapping, so no call site forgets it.

**Rationale**: `plan::build`, `execute`, `sync_copy`, the per-file state, conflicts and the run log all work on
listings of relative paths. Two one-entry listings are valid input for all of them, so none of that code learns
about files. The stored path stays the window path of the file (`source.path = /Dokumente/Haushalt.ods`); the
parent and name are derived where the place is opened.

**Alternatives considered**: Teaching `plan`, `execute` and `scan` about "a rule over one path". Rejected: it
spreads a special case over about 3,000 lines of tested engine code. A separate small engine for files. Rejected:
it would duplicate conflict handling, hashing, state and logging, and the two would drift.

## R2 - Where the kind of the source is stored

**Decision**: `SyncRule` gets `source_kind: SyncRuleSourceKind` (`folder` | `file`), stored in a new column
`source_kind TEXT NOT NULL DEFAULT 'folder'` of `haex_sync_rules_no_sync`. `SyncRuleDraft` gets
`source_kind: Option<SyncRuleSourceKind>`: `None` on create is `folder`, `None` on update keeps the stored rule's kind
(resolved in `commands::create` and `commands::update`, before `validate`). The target
has no kind: it is always a folder (FR-001).

**Rationale**: Without the stored kind the engine cannot tell the two missing-source cases apart: for a folder rule
a missing root is "place unreachable, wait" (FR-022); for a file rule a missing path is "source invalid" (FR-006).
Guessing from the path (a trailing `/`, a dot in the name) is wrong for files without an extension and folders
with a dot. A column with a default keeps every existing rule a folder rule without touching its row.

**Old readers**: An older build that reads a vault of this version ignores the new column (its `SELECT` lists the
columns by name) and treats a file rule as a folder rule at the file's path. That would list "a folder" that is a
file, find the root unreadable and wait; it never copies or deletes. A vault is not opened by older builds in the
normal flow, and holzi has no users yet, so this is noted and not engineered around.

**Why `Option` and not a serde default**: An agent's `update` replaces all settings of a rule. With a default of `folder`,
an update of a file rule that leaves `sourceKind` out would silently make it a folder rule whose source path is a file;
it would wait and delete nothing, but it would be wrong, and the confirmation question would not show it as a change.
With `None` meaning "keep", leaving the field out changes nothing.

**Alternatives considered**: A kind on `SyncRulePlace`, so both places carry one. Rejected: the target is a folder
by FR-001, a second field would only hold `folder`, and the picker and the validation would have to forbid
`file` there. Inferring the kind with a `stat` at save time and storing nothing. Rejected: the file may be gone at
the next run, which is exactly the case to detect.

## R3 - Invalid, waiting and error: who decides

**Decision**: Before a file rule scans, the run checks the source. The check lives in `place_file.rs`
(`source_check`) and returns one of: `Valid`, `Invalid(reason)`, or an `Unreachable` error.

| What the source place says                                                                                       | Result                   | Run outcome       |
| ---------------------------------------------------------------------------------------------------------------- | ------------------------ | ----------------- |
| `stat` finds a regular file                                                                                      | `Valid`                  | continues         |
| `stat` finds nothing, the parent folder exists (a folder at the path counts here: `stat` answers only for files) | `Invalid("no file")`     | `InvalidSource`   |
| `stat` finds nothing and the parent folder (or one above it) is missing, the mount root exists                   | `Invalid("folder gone")` | `InvalidSource`   |
| the path lies under a mount root that is not there (a removable drive that is not mounted)                       | `Unreachable`            | `Waiting`         |
| a remote place (peer, storage) answers "unreachable" or fails                                                    | as for a folder rule     | `Waiting`/`Error` |

For a place on a remote side the `Place` already tells missing from unreachable: its `stat` returns `None` for a
missing key and an `Unreachable` error when the peer or the storage cannot be reached. For the device the
distinction is a heuristic, `mount_root(path)`: the top directory where an external medium appears
(`/media/<user>/<label>`, `/run/media/<user>/<label>`, `/mnt/<name>`, `/Volumes/<name>`, `X:\`, `/storage/<id>`).
When that directory is missing the check is `Unreachable`; otherwise a missing path is `Invalid`.

The preview (`manage::preview`) makes the same check and returns the error `invalidSource`, so it never shows a plan that deletes. The target is not checked for validity: its folder must exist, or the rule waits (FR-022). A new `Place::check_root`
(default: `Ok`) answers it without listing the folder: the device place reads the folder's metadata, the storage
place asks for the folder. A missing target folder is `Unreachable` and the run ends `Waiting`.

**Rationale**: The two failures look alike on disk (nothing at the path), but the safe answer is the same for
both: copy nothing, delete nothing. Only the label differs, and the next run re-checks (FR-007), so a wrong
guess about an unusual mount point (a drive mounted at `/data`) shows "Quelle ungültig" instead of "wartet" and
corrects itself when the drive returns. That is why a heuristic is enough here and a registry of drives is not
needed. [Likely]: the paths above cover the mounts that the drive code of spec 064 itself recognises; not
verified against every Linux setup.

**Alternatives considered**: Asking the drive registry of spec 064 whether the path lies on a known drive.
Rejected: an unmounted drive has no path to match, and the extra dependency buys only a label.

## R4 - A missing copy, and no deletion

**Decision**: `plan::build` has a branch for `rule.source_kind == File`. It runs before the folder planner:

- when the source file is there and the target lacks it, the plan is one `Copy { from: Source, replaces: false }`,
  in either direction and whatever the state says;
- otherwise the folder planner runs as it does, on the two one-entry listings. With both files present it can only
  adopt, copy, conflict or report; it never reaches its deletion branches.

The delete mode of a file rule is stored as `keep` (validation sets it), and the dialog does not show the field.
In `run.rs`, the guard "the place lists no files although the state knew some" (`emptied`) is skipped for the
target side of a file rule: a deleted copy is not an emptied place, it is a copy to make again. The source side
cannot trigger it, since an empty source listing ended the run as invalid before. As a second line of defence the
branch plans no step at all when the source listing holds no file, so a caller that skipped the check still deletes nothing.

**Rationale**: The folder planner would read a deleted copy as "deleted at the target": a one-way rule leaves it
(FR-010), a two-way rule deletes the source. Both contradict FR-004. A branch in front of the planner states the
file rule's table in one place and leaves the folder planner untouched. Because the branch only fires when the
target lacks the file and the source has it, the "source gone" case can never reach the planner, so no deletion
is ever planned.

**Alternatives considered**: A new `Step` for "restore". Rejected: `Copy` with `replaces: false` already does it.
Letting the planner see a flag everywhere. Rejected: more branches inside a tested table.

## R5 - Exclusions do not apply

**Decision**: `OneFilePlace::read_text` returns `Ok(None)` for the exclusion file, so `scan` builds empty
exclusions and no step of `plan` or `execute` changes. The one place that treats the exclusion file's name
specially is the two-way planner (`both_ways`: `path == IGNORE_FILE` becomes `MergeIgnore`). A file rule skips
that branch, so a source that is itself named `.haex-ignore` is copied as a plain file (spec edge case). The
exclusion editor commands refuse a file rule with the error code `fileRule`; the list does not offer the action.

**Rationale**: FR-010. Using the wrapper keeps `scan.rs` free of a rule-kind parameter.

## R6 - What overlap means for a file rule

**Decision**: `rules::overlap` stays a "is one path a prefix of the other, by path components" test. What changes
is which paths a rule claims. A folder rule claims its two folders. A file rule claims the source file's path and
the path of the copy (`target.path` joined with the file's name). A new rule is rejected when any claim of it
overlaps any claim of another rule or of itself.

| Case                                                                    | Claims overlap?                         | Result   |
| ----------------------------------------------------------------------- | --------------------------------------- | -------- |
| the same file is the source of two rules                                | the source paths are equal              | rejected |
| the file lies in a folder that is a place of another rule               | the folder is a prefix of the file      | rejected |
| two rules would write the same copy                                     | the copy paths are equal                | rejected |
| the target folder is the file's own folder                              | copy path equals the source path        | rejected |
| two file rules from different files into the same target folder         | the copy paths differ                   | allowed  |
| a file rule whose copy lies in a folder that is a place of another rule | the folder is a prefix of the copy path | rejected |

**Rationale**: Claiming the copy's path, and not the whole target folder, is what lets Anna keep several files in
one cloud folder, one rule each. The other rows keep what spec 061 FR-004 protects: no two rules write or read
the same file.

**Validation of the source**: For a device source, `validate` reads the path with `symlink_metadata` and refuses
what is not a regular file (a link, a folder, nothing there) with `InvalidPlace`. For a storage or peer source it
cannot look (the function is synchronous and has no handle to open the storage), and the picker only offers
files; a path that is a folder there is caught by the first run as "a folder" (R3).

## R7 - Watching one file

**Decision**: `watch_file(parent, name, notify)` watches the parent folder non-recursively and calls `notify` only
for a batch with a change whose path is the file's path. The source is watched this way; for a two-way rule the
copy is watched the same way in the target folder. A storage place is not watched, as for a folder rule.

**Rationale**: A recursive watch of the parent would wake the rule for every change in the folder (a Downloads
folder), and watching the file's own inode misses editors that save by replacing the file (spec US2 scenario 7).
Watching the parent and filtering by path catches both. The filter takes the path from each `Change`
(`files/local/watch.rs`).

## R8 - The picker and the dialog

**Decision**: `FolderPicker.vue` gets a prop `allowFiles` (true for the source picker only). With it, the list shows
files as well as folders (not links, not holzi's own entries); a folder row is entered as today, a file row is
taken at once as the choice. The picked value `PickedFolder` gets `kind: 'folder' | 'file'`; `choose()` ("Choose
this folder") sets `folder`. The dialog stores the kind with the draft, shows the file icon for a file choice,
hides the delete mode while the source is a file, and keeps the name suggestion from the file's name. The target
picker is unchanged and offers folders only (spec US1 scenario 4).

Editing a rule: `pickedOf(rule.source)` takes the kind from `rule.sourceKind`. Picking another file replaces the
source (spec US2 scenario 5); picking a folder turns the rule into a folder rule and the other way round (spec FR-007,
clarified 2026-11-09). `update_rule` then clears the per-file state, as it does for any moved place.

The cloud hint and the decrypt consent depend on `encrypted` and the storage kind of the picked place, which the
picker already reports; for a file they come from the folders above it, which the picker has walked through.

## R9 - The agent schema and the translation keys

**Decision**: The draft schema of `syncRules.create` and `syncRules.update` gets an optional `sourceKind` (`folder`
or `file`, default `folder`), described as "`file` when `source` names one file; the target is then a folder".
There is no new tool (spec assumption). `tools.json` is regenerated with `pnpm export:eval-tools`. The question the
user is asked before an agent creates or changes a rule (`SyncRuleConfirmDialog.vue`) names the kind.

Translation keys live under a new group `files.syncRules.file` (`invalid`, `invalidReason.noFile`,
`invalidReason.folderGone`, `pickFile`, `copyFrom`, `noExclusions`) and one error key
`files.syncRules.error.fileRule`. The names were checked against the existing `files.syncRules.*` keys: free.

## R10 - Migration number

**Decision**: `0060_sync_rules_source_kind`. Main's last migration on 2026-11-09 is `0059_drop_agent_files`; the
number must be rechecked before the merge (spec 075 had to move twice). If another branch took it, rename the
constant, the registration and the tests, and add the old name to `migrations_legacy_names.rs` only if a vault
already ran this migration under it.

**Statement**: `ALTER TABLE haex_sync_rules_no_sync ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'folder'`. The table
is `_no_sync`, so the trigger version does not change.
