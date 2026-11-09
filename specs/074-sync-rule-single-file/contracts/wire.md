# Contract: what crosses the window and the agent boundary

All names are the camelCase serde names.

## Types (generated into `src/types/bindings/`)

| Type                 | Change                                                                                                       |
| -------------------- | ------------------------------------------------------------------------------------------------------------ |
| `SyncRuleSourceKind` | new: `"folder" \| "file"`                                                                                    |
| `SyncRule`           | `sourceKind: SyncRuleSourceKind`                                                                             |
| `SyncRuleDraft`      | `sourceKind?: SyncRuleSourceKind` (`Option` in Rust; missing: `folder` on create, the stored kind on update) |
| `SyncRuleOutcome`    | new value `"invalidSource"`                                                                                  |
| `SyncRuleState`      | new value `"invalidSource"`                                                                                  |
| `SyncRuleErrorCode`  | new value `"fileRule"`                                                                                       |

`cargo test` regenerates the bindings and leaves trailing spaces; run
`sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` afterwards (as the CI does).

## Tauri commands

No new command and no changed signature. Behaviour for a file rule:

| Command                          | File rule                                                                                                                    |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `sync_rules_create` / `_update`  | validates as in `data-model.md`; stores `deleteMode` as `keep`                                                               |
| `sync_rules_list`                | `view.status.state` may be `invalidSource`; `view.status.reason` is a reason key (`noFile`, `folderGone`)                    |
| `sync_rules_run_now`             | runs; an invalid source ends the run with outcome `invalidSource`, logged like any run                                       |
| `sync_rules_preview`             | shows the one step; an invalid source is the error `invalidSource` (the reason key is its message); never an exclusion count |
| `sync_rules_ignore_get` / `_set` | error `fileRule`                                                                                                             |
| `sync_rules_pending_*`           | unchanged; a file rule has no pending deletion                                                                               |

## Agent tools

`syncRules.create` and `syncRules.update` take the same draft; its schema gains the optional property:

```json
"sourceKind": {
  "type": "string",
  "enum": ["folder", "file"],
  "description": "file when source names one file; the target is then a folder. Default folder."
}
```

A create call without it makes a folder rule, as before; an update call without it keeps the stored rule's kind. Other tools are unchanged; `syncRules.list` and
`syncRules.status` return the new fields. The user's confirmation question names the kind (`SyncRuleConfirmDialog`).
`tools.json` is regenerated with `pnpm export:eval-tools`; `pnpm check:agent-actions` must pass.

## Translation keys (`en` and `de`)

| Key                                             | German text (draft)                                                    |
| ----------------------------------------------- | ---------------------------------------------------------------------- |
| `files.syncRules.state.invalidSource`           | Quelle ungültig                                                        |
| `files.syncRules.file.invalidReason.noFile`     | Am gespeicherten Pfad liegt keine Datei.                               |
| `files.syncRules.file.invalidReason.folderGone` | Ein Ordner über der Datei fehlt (umbenannt, verschoben oder gelöscht). |
| `files.syncRules.file.pickFile`                 | Datei wählen                                                           |
| `files.syncRules.file.copyFrom`                 | Kopie von {name}                                                       |
| `files.syncRules.file.noExclusions`             | Ausnahmen gelten nur für Ordner.                                       |
| `files.syncRules.error.invalidSource`           | Die Quelle ist ungültig: Am gespeicherten Pfad liegt keine Datei.      |
| `files.syncRules.error.fileRule`                | Das geht bei einer Regel für eine einzelne Datei nicht.                |

## Test ids

The "Quelle ungültig" badge is the existing `files-sync-rule-state-<name>`;
`files-picker-source-file-<name>` on a file row of the source picker.
