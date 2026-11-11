# Research: Create a sync rule from the file browser's context menu

Spec 073. The feature is a frontend change; the engine, the commands and the stored rules do not change (spec 074 is
merged, so a file is a valid source place). Each decision below was checked against the code of `main`.

## R1: Where the dialog lives

**Decision**: `Panel.vue` mounts one `FilesSyncRuleDialog` next to `FilesShareDialog`. `useFilesManage` gets an
option `syncRule?: (place: PickedFolder) => void` (like `share`), the panel sets a `starting` ref that opens the dialog.

**Why**: The menu belongs to a panel. In the split view each panel has its own `useFilesManage` and its own dialog,
so the entry acts on the panel whose menu was opened (FR-008) without any extra rule. The share dialog is wired the
same way.

**Rejected**: a dialog in a store opened from anywhere (a second place that owns window state for no gain); opening
the sync rule list with the dialog on top (the user would leave the folder, against FR-007).

## R2: When the menu offers the entry

**Decision**: One pure function `syncRuleOffer` decides, in `src/lib/files/registry.ts` next to `shareableSource`.
Input: the source, whether the entry is a symlink, has no access, is holzi's own, and the encryption state of the
entry (or of the open folder). It returns `true` when all hold:

- the source is this device, one of the vault's own cloud storages, or another own device (`peer:`); not a contact's
  share (`share:`), not a `device:` storage (those only name an encrypted folder's store);
- the entry is not holzi's own data, not a symlink, not without access (the folder picker hides the same entries,
  so SC-003 holds: the menu offers what the list's dialog could pick);
- the entry is not in an encrypted folder that is not readable (`otherVault`, `newerFormat`, `damaged`); a readable
  one is a usable place (spec FR-004, edge case "locked encrypted folder").

The menu input gets `syncRule?: boolean`. `entriesMenu` offers the item for one entry only; `folderMenu` for the open
folder. The backend still checks everything on save (spec 061 FR-002), so the menu is a courtesy, not the guard.

**Why**: The rule "hidden, not greyed out" (Assumptions) and the same pure-function pattern as `pin` and `shareable`,
which `scripts/check-files-*.ts` already test without Vue.

## R3: From an entry to the dialog's source place

**Decision**: A pure function `pickedFromEntry` (same file as R2's helper module, `lib/files/syncRules.ts`) builds
the dialog's `PickedFolder` `{ source, path, encrypted, kind }`:

- `source` is the panel's `SourceRef`; it is already the rule's `location` (the folder picker uses the same type and
  the same `peer:` ids);
- `kind` is `folder` for a folder entry or the open folder, `file` for a file entry;
- `path` is the entry's path; for a storage it is brought into the window form the rules store (leading `/`, no
  trailing `/`; the entry's key ends in `/` for folders), a device path stays as it is;
- `encrypted` is `entry.encrypted !== null` (for a file: the folder it lies in), for the open folder the panel's
  `folderEncrypted`.

**Why**: The dialog needs `encrypted` for the consent and the cloud hint (spec 061 FR-030a, FR-031), and the backend
normalizes storage paths only on save. A wrong form would break `keepsStored`, the automatic name and the overlap
hint in the dialog.

## R4: Prefilling the dialog

**Decision**: `SyncRuleDialog.vue` gets the optional prop `initialSource?: PickedFolder | null`. When the dialog
opens without a rule, `source` starts as `initialSource` (else `null`, as today); everything else keeps its defaults
(name from the source's last name through the existing `watch(source)`, direction one way, delete mode `keep`,
15 minutes). The source stays replaceable through the picker (FR-005). The list's dialog does not pass the prop, so
it works as before (FR-010).

**Why**: The smallest change that keeps one dialog and one save path (SC-003).

## R5: Confirmation and the list

**Decision**: `saved` closes the dialog (it does already). The panel shows `toast.success` with the rule's name
(`files.syncRules.createdFromMenu`) and reloads `useSyncRulesStore` so the list is current when opened. Nothing
navigates, so the browser keeps its folder (FR-007).

**Why**: `vue-sonner` toasts are the file browser's confirmation (`useFilesManage`, transfers). The store reload is
the one line the list's own dialog also does (`@saved="store.loadAsync()"`).

## R6: Keyboard, touch, focus

**Decision**: No new input path. The item is part of the existing right-click menu, which opens by mouse, long press
and the context menu key or Shift+F10 (spec 044 FR-017a), so FR-009 holds for opening. The item has no shortcut of
its own (the selection bar and shortcuts are out of scope). `Menu.vue` already leaves the focus to a dialog the
command opened; whether the focus returns to the list after the dialog closes is checked by hand, and fixed in the
panel if it does not (task T021).

**Why**: Rename and share dialogs behave the same, so a regression there would show first.

## R7: The selection bar

**Decision**: Unchanged. `ActionBar.vue` lists its buttons itself; the new command id is added only to the menu input.
T012 verifies that no bar button or keyboard map picks up `syncRule`.

## R8: Label and order

**Decision**: Command id `syncRule`, label key `files.actions.syncRule`: "Sync-Regel erstellen …" (de), "Create sync
rule…" (en). In the entries menu it follows `pin` (open, system, open with, share, pin, **sync rule**, select); in the
open folder's menu it follows `pin` after the separator, or stands alone behind it when the folder cannot be pinned.

**Why**: The spec puts the entry next to "Zu Orten hinzufügen" (spec 071) as another command on a folder.
