# Sync rules: keeping folders equal across locations

**Status**: Design agreed with the operator on 2026-10-26; to be specified as its own spec (number open).

**Replaces** the draft [025](../../specs/025-own-device-file-sync/spec.md) (file index, content-addressed
objects, per-file keys, conflict copies at Bereich level). Builds on spec
[056](../../specs/056-file-access-sharing/spec.md): another own device appears in the file browser as a
storage, so a rule needs no transport of its own.

## Context

The operator wants to define a rule that keeps files in two places equal, one-way or two-way,
permanently. Both places can be anything the file browser shows: a local folder, another own device
(056), S3, WebDAV, SFTP or an unlocked encrypted folder (spec 048). Device-to-device mirroring and
cloud backup are the same mechanism.

## Decisions

| #   | Question                  | Decision                                                                                                                                                                                                                                                                                                         |
| --- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Which places?             | Any place of the file browser on both sides. Contacts' shares (foreign vaults) are excluded: sync and revocation collide.                                                                                                                                                                                        |
| 2   | Who runs a rule?          | One device per rule: the device that created it. The rule is device-local and not synced (spec 024). The other side is reached through the live access of 056. No shared file index, no reconciliation between devices.                                                                                          |
| 3   | Concurrent changes        | Conflict copy: the newer version keeps the name, the older one is stored beside it as `name (Konflikt <device> <date>).ext`; both sides get both. Changed on one side and deleted on the other: the changed file stays and is copied back.                                                                       |
| 4   | Deletions                 | Per rule: mirror, keep or ask. Mirror moves to the system trash where one exists, else to a `.holzi-trash` folder of the rule. Defaults: mirror for two-way, keep for one-way. "Ask" lists pending deletions in the rule status.                                                                                 |
| 5   | When does it run?         | While holzi runs with an unlocked vault: file watcher on local folders, interval for remote places (default 15 min), "sync now". Android needs the background service, a separate spec. The status shows "paused, holzi not active".                                                                             |
| 6   | Encryption in the cloud   | None of its own. Whoever wants ciphertext at the provider picks an encrypted folder (048) as the place. The rule sees plaintext because the folder is unlocked.                                                                                                                                                  |
| 7   | Change and move detection | Size plus modification time against the remembered state of the same side; clocks of different devices are never compared (only to pick the newer side in a conflict). A vanished and a new file with equal size and time are a move. A checksum is computed only for ambiguous candidates and to verify a copy. |

## Model

**Rule** (device-local): name, active or paused, place A and place B (each storage plus folder path),
direction (A→B, B→A, both), deletion mode, interval. Two rules may not overlap; creating one checks
that.

**State** per rule and file: relative path, size and modification time on side A and on side B at the
last successful reconciliation. It tells what is new, changed, moved or deleted since.

**Engine** (Rust, module `file_sync` beside `files`): works only against the storage interface of the
file browser (`StorageFiles`) and knows no backend. A run:

1. List both sides.
2. Compare with the state and build a plan (copy, move, delete, conflict copy).
3. Execute the plan, copying through the streaming paths of `files::transfer` so large files resume.
4. Write the state for a file only after its step succeeded; an aborted run continues cleanly.

Plan and execution are separate. The plan is pure logic, testable without backends, and also feeds the
preview and the confirmation for "ask".

## Plan per file

| Side A                                  | Side B    | Action                                |
| --------------------------------------- | --------- | ------------------------------------- |
| new                                     | –         | copy to B (A→B or both)               |
| changed                                 | unchanged | copy to B                             |
| changed                                 | changed   | conflict copy, both sides get both    |
| deleted                                 | unchanged | deletion mode: mirror, keep or ask    |
| deleted                                 | changed   | changed file stays and is copied back |
| vanished + new with equal size and time | –         | execute as a move on B                |

One-way considers only the source direction. A change at the target is ignored and reported in the
status by default; resetting the target is not the default because it would overwrite data.

## Failure handling

- **Mass-delete guard**: a side that lists empty or whose root is missing (mount gone, device offline)
  aborts the run with "place unreachable". An empty list never means "everything was deleted".
- A failing file (locked, no space, network) does not stop the run. It shows with its reason in the
  rule status and is retried on the next run.
- An unreachable place sets the rule to "waiting", not to an error.
- Copies go to a temporary file at the target first and are renamed afterwards, so no half file
  counts as complete.

## UI

- New area "Sync rules" in the files: list with name, places, direction and status (active, waiting,
  error, paused, last run).
- Create dialog: two place pickers (the folder picker of the file browser), direction, deletion mode,
  interval. For cloud targets a hint that an encrypted folder protects the content from the provider.
- Per rule: sync now, pause, preview of the plan, errors and pending deletions or conflicts to confirm.
- Missing UI primitives go to haex-ui, not rebuilt locally.

## Agents

A permission "manage sync rules". Reading rules and status is allowed; creating, changing and
deleting go through the approval gate. A running rule needs no agent and has only the rights the user
confirmed when creating it. holzi's own data stays blocked as in spec 044.

## Tests

- Plan logic as pure unit tests: every table row, move, empty place, overlapping rules.
- Engine against an in-memory store and a real local folder, including abort mid-run and resume.
- One E2E: two local folders, two-way, with a conflict. Device to device is a hand check like 056.
- Rust tests in `*_tests.rs` beside the module.

## Out of scope

Background service (Android foreground service, desktop autostart), contacts' shares as a rule place,
rule-owned keys, a cross-device file index.

## Open

- Checked 2026-10-26: spec 048 keeps size, modification time and a plaintext checksum per file in its sidecar (decision 6 holds). A write into an encrypted folder sets the write time as modification time.
- Number of the new spec (next free is 061 unless another branch took it; 045 is the dock).
- Update the references to 025 and the "sync rules" wording in 056 and the sync architecture once the
  spec exists.
