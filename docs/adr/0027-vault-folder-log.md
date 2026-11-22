# ADR 0027: The diagnostic log is a plain-text file in the vault's folder

- **Status**: accepted
- **Date**: 2026-11-22
- **Spec**: [098-diagnostic-log](../../specs/098-diagnostic-log/spec.md), research R1, R3, R4

## Context

Before spec 098 the app logged only in debug builds, at a fixed level, through a plugin whose log folder is fixed when the plugin is built. A user whose import failed with "the file is damaged" had no way to give us the real cause: it stood in a log that release builds never wrote. The log must exist in every build, must be switchable to more detail by the user while a problem is being diagnosed, and must not leak what a vault protects.

A vault is an encrypted database. Its folder also holds files that are not encrypted. A log is useful only if a person can read and send it without the vault's passphrase.

## Decision

1. **One plain-text file per vault.** The log is `haex.log` (with `haex.log.1` … as archives) in the `logs/` folder of the vault's own folder. It is not encrypted and not synchronized. A line holds a time, a severity, an origin and a text, and never a secret (research R7).
2. **Nothing before the unlock.** The sink drops every record while no vault is open and buffers nothing. Lines of the app's start, the vault list and the passphrase prompt are not written. The log begins in the shared start of the session services and ends after the close has drained, so the lines of the close are in it (R3). The migrations that run inside the database open fall before that start; the spec accepts the gap.
3. **One swappable sink behind one logger.** The log plugin is created in every build with one dispatch that feeds our own `log::Log` sink. The sink holds the open vault's writer and is attached at the unlock and detached at the close. A debug build keeps its terminal output beside it (R1).
4. **One level per vault and device.** "Standard" or "Ausführlich" is a row in `haex_diagnostic_log_no_sync`, a `_no_sync` table, with an end time and the id of the app run that switched it. "Ausführlich" ends after 24 hours measured from the switch, or at the next start, whichever is first (R4).
5. **The interface writes through the backend.** Warnings and errors of the web view go to the sink through a command that carries the session token of the unlock, so a message that outlived its vault is dropped.

## Alternatives

- **A global log folder** (the plugin's own folder target, or a file next to the app data): it needs no vault, so it would also hold the start and the vault list. It was rejected because one folder would mix the lines of every vault, and the folder target is fixed for the life of the process. The operator decided that the log belongs to the vault (2026-11-21).
- **Buffering until the unlock** and then writing the held lines: it would keep lines of a locked vault in memory and would put a user's path or a failed passphrase attempt into a file of a vault they did not open yet. Rejected with decision 2.
- **The level as a device preference**: it needs no migration, but a preference is a tracked row. It replicates to other devices and every switch is a sync change, made exactly when something, maybe the sync, is being diagnosed. The `_no_sync` table costs one small migration.
- **The level in a plain file next to the log**: it would be readable before the unlock, which the spec dropped.
- **A second logger per vault**: a process has one `log` logger.
- **An encrypted log inside the vault database**: it would need the passphrase to be read, cannot be sent without opening the vault, and would add writes to the database that is being diagnosed.

## Consequences

- A person can open the folder (desktop) or share a joined copy of all files (Android) and send it without a passphrase. The price is that the file is readable by anyone with access to the vault's folder, so no line may hold a secret, an entry's field or a path of the home folder. Spec 098 task T037 reviews the existing log calls for this.
- A problem that happens before the unlock, or in the migrations of the open itself, is not in the log.
- Opening a second vault moves the writing to its folder; the first folder is left as it was.
- A vault file copied to another device carries the level row. The run id there is never that device's run, so the copy starts at "Standard".
