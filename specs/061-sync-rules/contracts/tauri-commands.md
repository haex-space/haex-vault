# Vertrag: Tauri-Befehle und Ereignisse der Sync-Regeln

Alle Befehle stehen in `lib.rs` im `generate_handler!` hinter `gate.wrap(...)` (laufen nur bei geöffneter,
entsperrter Vault). Typen sind ts-rs-Typen mit Präfix `SyncRule`, `camelCase`. Fehler: `SyncRuleError { code,
message }` für Regelfehler, `FilesError` für Fehler eines Orts.

## Typen (Auszug)

```text
SyncRulePlace      { source: SourceRef, path: string }          // SourceRef wie in files/mod.rs
SyncRuleDirection  'aToB' | 'bToA' | 'both'
SyncRuleDeleteMode 'mirror' | 'keep' | 'ask'
SyncRuleDraft      { name, placeA, placeB, direction, deleteMode, intervalSecs, enabled }
SyncRule           SyncRuleDraft + { id, createdAt, updatedAt, status, lastRunAt, lastError }
SyncRuleStatus     { state: 'active'|'running'|'waiting'|'error'|'paused'|'inactive', reason?, pendingDeletes,
                     conflicts, conflictCopies: string[], errors: { path, reason }[], ignored,
                     ignoreErrors: { line, text }[], deviations: number }
SyncRulePlan       { steps: SyncRulePlanStep[], hashReads, ignored }
SyncRulePlanStep   { action, path, side?: SyncRuleSide | null, detail?: string | null }
SyncRulePending    { path, side }
SyncRuleRun        { id, startedMs, durationMs, cause, outcome, error?, copied, moved, deleted, conflicts,
                     errors, skipped }
SyncRuleRunItem    { seq, path, action, side, detail?, trashKind?, trashPath? }
SyncRuleIgnore     { content, writable, reason?, fixed: string[], ignoredNow: number,
                     lineErrors: { line, text }[] }
SyncRuleError      { code: 'overlap'|'invalidPlace'|'invalidInterval'|'invalidName'|'invalidDepth'|'notFound'|
                     'readOnly'|'unreachable'|'placeError'|'invalidIgnore'|'unavailable', message }
```

## Befehle

| Befehl                       | Eingabe                                              | Ausgabe              | Hinweis                                                                               |
| ---------------------------- | ---------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------- |
| `sync_rules_list`            | –                                                    | `SyncRule[]`         | Regeln dieses Geräts                                                                  |
| `sync_rules_create`          | `draft: SyncRuleDraft`                               | `SyncRule`           | prüft Überlappung (FR-004)                                                            |
| `sync_rules_update`          | `id`, `draft`                                        | `SyncRule`           | Orte oder Richtung geändert → Stand verworfen                                         |
| `sync_rules_delete`          | `id`                                                 | –                    | Dateien beider Orte unberührt (FR-006)                                                |
| `sync_rules_set_enabled`     | `id`, `enabled`                                      | `SyncRule`           | Pausieren und Fortsetzen                                                              |
| `sync_rules_run_now`         | `id`                                                 | –                    | fällig machen; Ergebnis über Ereignis                                                 |
| `sync_rules_preview`         | `id`, `draft` (je optional)                          | `SyncRulePlan`       | ändert nichts; `id` gespeichert, `draft` neu, beides eine Änderung                    |
| `sync_rules_cancel`          | `id`                                                 | –                    | bricht den laufenden Lauf ab                                                          |
| `sync_rules_pending_list`    | `id`                                                 | `SyncRulePending[]`  | Löschmodus „Nachfragen“                                                               |
| `sync_rules_pending_resolve` | `id`, `paths`, `choice: 'delete'\|'keep'\|'restore'` | `{ path, reason }[]` | sofort; eine inzwischen geänderte Datei bleibt dem nächsten Lauf                      |
| `sync_rules_runs`            | `id`                                                 | `SyncRuleRun[]`      | Lauf-Protokoll, jüngste zuerst                                                        |
| `sync_rules_run_items`       | `runId`, `offset`, `limit`                           | `SyncRuleRunItem[]`  | Dateien eines Laufs                                                                   |
| `sync_rules_ignore_get`      | `id`                                                 | `SyncRuleIgnore`     | Datei im Quellort, feste Ausnahmen, Zahl ausgenommener Dateien                        |
| `sync_rules_ignore_set`      | `id`, `content`                                      | `SyncRuleIgnore`     | schreibt `.haex-ignore` in Ort A; `readOnly` bzw. `invalidIgnore`; Regel läuft danach |
| `sync_rules_log_depth_get`   | –                                                    | `number`             | Vorgabe 10                                                                            |
| `sync_rules_log_depth_set`   | `depth: number`                                      | –                    | 0 bis 100; kürzt sofort                                                               |

## Ereignisse

| Ereignis              | Nutzlast                                                   | Wann                                                                                     |
| --------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `sync-rules-changed`  | `{ ruleId: string \| null }`                               | Statuswechsel, Lauf beendet, Regel angelegt/geändert/gelöscht (`null` = alles neu laden) |
| `sync-rules-progress` | `{ ruleId, itemsDone, itemsTotal, bytesDone, bytesTotal }` | höchstens alle 250 ms während eines Laufs                                                |

Das Frontend legt die Sicht der Regel vor `invoke` an und bindet frühe Ereignisse über die `ruleId` (kein Verlass
darauf, dass `invoke().await` vor dem ersten Ereignis zurückkehrt).

## Agent-Aktionen

| Aktion             | `effect`      | Abfrage                                                                                               |
| ------------------ | ------------- | ----------------------------------------------------------------------------------------------------- |
| `syncRules.list`   | `read`        | keine; erfordert die Berechtigung „Sync-Regeln verwalten“                                             |
| `syncRules.status` | `read`        | keine; dito                                                                                           |
| `syncRules.create` | `destructive` | `SyncRuleConfirm`: Orte, Richtung, Löschmodus, Intervall; je Aufruf, unabhängig von der Freigabestufe |
| `syncRules.update` | `destructive` | wie `create`, mit den alten Werten daneben                                                            |
| `syncRules.delete` | `destructive` | Name und Orte der Regel                                                                               |

Ein Agent kann weder Orte der eigenen Daten von holzi noch Freigaben von Kontakten wählen (FR-002, FR-036);
`sync_rules_create` lehnt sie unabhängig vom Aufrufer ab.
