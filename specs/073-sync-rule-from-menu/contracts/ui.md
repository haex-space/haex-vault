# UI contract: sync rule from the context menu

## Menu

| Menu                                              | Item id    | Test id               | Offered when                                                                                                 |
| ------------------------------------------------- | ---------- | --------------------- | ------------------------------------------------------------------------------------------------------------ |
| entries (one entry selected or under the pointer) | `syncRule` | `files-menu-syncRule` | `syncRuleOffer` is true for that entry                                                                       |
| open folder (empty area)                          | `syncRule` | `files-menu-syncRule` | `syncRuleOffer` is true for the open folder, and the folder is not the list of places (`path` is not `null`) |

Not offered for several entries (`count !== 1`). Hidden, not disabled. No shortcut.

## Dialog

`FilesSyncRuleDialog` props: the existing `sources`, `rule`, `labels`, plus `initialSource?: PickedFolder | null`.
On open without a rule: `source` = `initialSource`, `target` empty, defaults as in spec 061 FR-005. Events: unchanged
(`saved`).

## After saving

A toast `files.syncRules.createdFromMenu` ("Sync rule “{name}” created." / "Sync-Regel „{name}“ angelegt."), the
sync rule store reloads, the browser keeps its folder, no navigation.

## i18n keys (en and de)

- `files.actions.syncRule`
- `files.syncRules.createdFromMenu`
