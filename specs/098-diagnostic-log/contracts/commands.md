# Contract: Tauri commands

All commands need an unlocked vault (`active_database`); all are `#[tauri::command]` in `src-tauri/src/diagnostic_log/commands.rs`.
Types that cross to the interface are exported with `ts-rs` and carry the `DiagnosticLog` prefix (no name collision with other
bindings).

| Command                 | Arguments                        | Result                | Notes                                                                                                                   |
| ----------------------- | -------------------------------- | --------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `diagnostic_log_get`    | none                             | `DiagnosticLogStatus` | the current level, the end time, whether the log can be written, whether this platform shares or reveals                |
| `diagnostic_log_set`    | `level: "standard" \| "verbose"` | `DiagnosticLogStatus` | applies at once: writes the row of the level, sets the global max level, starts or stops the end timer, emits the event |
| `diagnostic_log_reveal` | none                             | `()`                  | desktop only; reveals `logs/haex.log` in the file manager; `unsupported` elsewhere                                      |
| `diagnostic_log_share`  | none                             | `()`                  | Android only; copies the log to a temporary snapshot and starts the share sheet; `unsupported` elsewhere                |

```text
DiagnosticLogStatus {
  level: "standard" | "verbose",
  until_ms: number | null,        // Unix ms, only with "verbose"
  writable: boolean,              // false after a failed write or a failed begin
  can_reveal: boolean,            // desktop
  can_share: boolean,             // Android
}
```

Errors use the repository's `HaexError`; a missing vault gives the usual "no active vault" error. `diagnostic_log_set`
with the level it already has changes nothing and returns the status.

The catalog action `settings.diagnostics.logLevel.set` calls `diagnostic_log_set` and is marked as not callable by an agent
(guardrail, as `settings.privacy.screenCapture.set`).

The interface messages use no command of ours: they go through the log plugin's `plugin:log|log` (permission `log:default` in
`capabilities/default.json` and `mobile.json`, window `main`).
