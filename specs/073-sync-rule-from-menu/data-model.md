# Data Model: Create a sync rule from the file browser's context menu

No stored data changes: no table, no column, no migration, no new backend command, no binding. A rule created from the
menu is a `SyncRuleDraft` like any other and goes through `sync_rules_create`.

## Transient values (frontend)

| Value                       | Shape                                                                               | Where                                                           |
| --------------------------- | ----------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `PickedFolder` (exists)     | `{ source: SourceRef, path: string, encrypted: boolean, kind: 'folder' \| 'file' }` | `FolderPicker.vue` exports it; the dialog's source place        |
| `initialSource` (new prop)  | `PickedFolder \| null`                                                              | `SyncRuleDialog.vue`; read when the dialog opens without a rule |
| `syncRule` (new menu input) | `boolean`, `undefined` = not offered                                                | `EntriesMenuInput`, `FolderMenuInput` in `lib/files/menus.ts`   |
| `FilesCommand`              | adds `'syncRule'`                                                                   | `lib/files/menus.ts`                                            |

## Pure functions (new, tested without Vue)

- `syncRuleOffer(input) -> boolean`, `lib/files/registry.ts`: the rule of research R2.
  Input: `{ source: SourceRef, holziOwned: boolean, symlink: boolean, noAccess: boolean, encrypted: EncryptedInfo-like | null }`.
- `pickedFromEntry(source, path, kind, encrypted) -> PickedFolder`, `lib/files/syncRules.ts`: research R3,
  including the window form of a storage path.

## Validation

Unchanged and on the backend (spec 061 FR-002, FR-004, FR-030a, FR-031; spec 074 FR-001 to FR-004). The menu
only hides the entry where the backend would refuse the place.
