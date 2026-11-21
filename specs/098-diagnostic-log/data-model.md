# Data model: Diagnostic log

One new table that is not synced holds the level; the log is plain files.

## Log level

Two values, one per vault file. It is read at the unlock and applies only while the vault is unlocked.

| State      | Meaning                            | Global max level | Files              |
| ---------- | ---------------------------------- | ---------------- | ------------------ |
| `standard` | warnings, errors and information   | `Info`           | 4 files of 256 KiB |
| `verbose`  | adds detail lines; has an end time | `Debug`          | 5 files of 4 MiB   |

Third-party crates named in the damping list (research R2) stay at `Warn` in both states.

### Table `haex_diagnostic_log_no_sync` (not synced, one row per vault file)

| Column     | Type                          | Meaning                                             |
| ---------- | ----------------------------- | --------------------------------------------------- |
| `id`       | INTEGER PRIMARY KEY, always 1 | the single row                                      |
| `level`    | TEXT NOT NULL                 | `standard` or `verbose`                             |
| `until_ms` | INTEGER, null with `standard` | end time, Unix milliseconds                         |
| `run_id`   | TEXT, null with `standard`    | random id of the app run that switched to `verbose` |

No row means `standard`. The table is created by a migration (research R4) and is not tracked by the sync, so a change of the
level is no sync change and other devices never see it.

Validation: an unknown `level`, a `verbose` without `until_ms` or `run_id` counts as `standard` and is rewritten as `standard` at
the next `begin` (a damaged row never raises the level).

### State transitions

```text
          switch (user)                          end time passed (timer or at begin)
standard ───────────────► verbose ──────────────────────────────────────────────► standard
    ▲                        │   run id differs at begin (a new start of the app)      │
    └────────────────────────┴─────────────────────────────────────────────────────────┘
                              switch (user) back to standard
```

A switch to `verbose` sets `until_ms = now + 24 h` and `run_id = this run`. A switch back writes `standard` and clears both.

## Log

Plain UTF-8 text, one line per record, newest last, in the vault's folder (spec 084):

```text
vaults/<name>/
├── vault.db …              # unchanged
└── logs/
    ├── haex.log            # the current file
    └── haex.log.1 … .N     # older files, N = files kept − 1; the highest number is the oldest
```

The folder is created at `begin` and goes with the vault's folder when it is deleted or copied (FR-017). The sink never opens
a file outside `logs/` of the open vault, and never a file of another vault (FR-007).

## Log line

```text
[2026-11-21][07:02:02.123][WARN][haex_lib::passwords::import::legacy_haex_vault] passwords import: reading the file failed: …
[2026-11-21][07:02:03.004][ERROR][ui] TypeError: x is undefined\n    at …
```

| Part    | Content                                                                                        |
| ------- | ---------------------------------------------------------------------------------------------- |
| time    | local date and time with milliseconds                                                          |
| level   | `ERROR`, `WARN`, `INFO`, `DEBUG`                                                               |
| origin  | the module path of the backend, or `ui` for a record whose target starts with `WEBVIEW_TARGET` |
| message | the text; line breaks of a stack are kept and indented by the formatter                        |

A line is never written with an object's fields (FR-006) or a secret (FR-009). One extra line kind, written by the interface
plugin: `[ui] dropped N messages in the last second` (FR-014).
