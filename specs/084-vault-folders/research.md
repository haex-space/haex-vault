# Research: Vault folders

Facts were read from the code on 2026-11-06 (main at `0f0bb085` plus #850).

## R1 — Where vault files are built today

`instances/paths.rs` builds `<AppLocalData>/instances/<name>.db` (`get_instance_path`), the marker `<name>.db.pending`
(`get_pending_marker_path`) and the directory (`get_instances_directory`). Sidecars are derived from the database path by
suffix: `vault_id_path` (`.vault-id`), `cleanup::SIDECAR_SUFFIXES` (`.lock`, `-wal`, `-shm`, `.vault-id`), temporary copies
`<name>.db.*.tmp`. `haex-crdt` takes the lock and creates `-wal`/`-shm` beside the path it is given.

Other places build vault-named paths themselves: `agent_browser/profile_store::dir` (`agent-browser/<name>`),
`chat/peer/files/store::dir` (`chat-files/<name>`). `agent_browser/env::device_dir` is `agent-browser/` and holds
`filters/` and `dns.json`.

Callers of the instance paths: `instances/{create,open,import,link_vault,passphrase_change,list,startup,cleanup,vault_id}`,
`browser_bridge/commands`, and the test fixtures `tests/common/sync_fixture.rs`, `tests/sync_link.rs`.

**Decision:** one module, `instances::vault_paths`, owns the layout. `paths.rs` keeps the name check and `get_app_local_data`;
everything that today joins a vault name asks `VaultPaths` (FR-003).

## R2 — Names inside the folder

The database keeps one fixed name in the folder: `vault.db`. Every helper that derives a sidecar from the database path
(`vault_id_path`, `get_pending_marker_path`, `remove_vault_files`, the lock) keeps working unchanged, because it appends to
the path it is given. The folder name carries the vault's name; nothing else does.

**Alternative rejected:** `vaults/<name>/<name>.db`. Same effect, but the name would live in two places and a rename of the
folder would have to rename the file.

## R3 — No delete function, no trash

`TRASH_DIRECTORY` (`.trash`) is only skipped by `list_instances`; nothing writes to it, and no command deletes a vault. The only
code that removes a vault's files is `startup::cleanup_orphans_in_dir` (an interrupted creation, marked by
`<name>.db.pending`) and the error paths of `create`/`import`. The spec therefore speaks of "removing a vault's files", not of
a user-facing delete. In the new layout the orphan clean-up removes the folder, and `reserve_name` becomes `create_dir`
(atomic, fails if the name is taken) instead of "check, then create".

## R4 — The move must keep the database, its WAL and its shm together

SQLite WAL mode: committed frames live in `-wal` until a checkpoint. A move that leaves the WAL behind loses them. So the
files of one vault are never split across two visible places: they are renamed one by one into a hidden staging folder
`vaults/.moving-<name>/`, and only when all are there the staging folder is renamed to `vaults/<name>/`. The list ignores names
starting with a dot, so a half moved vault is never listed. After a crash the next start finds `.moving-<name>/` and continues:
every file still at its legacy place is renamed into it, then the folder is renamed. Every rename is within
`<AppLocalData>` (one volume); if it fails (`EXDEV`, permissions, read-only), the vault stays and works.

**Which place do callers use during a half move?** None: the move runs in `setup` before any command, and the list and
`open` look only at `vaults/`. A vault in staging is invisible until complete; it cannot be opened in the meantime because
nothing runs yet.

## R5 — A vault another process holds open

`haex-crdt` takes an exclusive file lock (`<db>.lock`) while a vault is open. The move takes the same lock with a
non-blocking attempt before renaming anything and keeps it until the last rename: if it is held, the vault is skipped and
tried again at the next start (FR-006). On Windows a rename of an open file fails anyway; on Unix it would succeed and leave
the other process writing into a moved file, which is why the probe is required and not optional.

Two holzi processes starting at once would both run the move. A device-wide lock file `<AppLocalData>/vaults.lock` is taken
for the whole move; the second process waits, then finds nothing left to do.

The probe uses `haex_crdt::DatabaseLock::try_acquire`, the same call `Database::open` makes (it is public, an `fs2`
advisory lock on `<db>.lock` with the path canonicalised), so the move and the open can never disagree about what "held" means. No
new dependency.

## R6 — `filters` and the profile folder

Legacy `agent-browser/filters/` is two things when a vault is named `filters`: the tracker lists (`settings.json`, `*.txt`,
`*.part`) and that vault's profile store. The move of device-wide files runs first and takes exactly those names out
(to `filter-lists/`); what remains of `agent-browser/filters/` is the vault's profile and moves like any other. For every other
name the folder holds no list file, and the step does nothing there.

## R7 — Tracker and DNS choices go into `preferences`

`preferences (vault_device_uuid, key, value)` exists, is CRDT tracked and needs no migration. ADR 0001: a row with the nil
UUID is vault-wide, any other UUID is one device. Keys: `agentBrowser.trackers` (nil UUID; JSON of `TrackerSettings`),
`agentBrowser.dns` (the device's UUID; JSON of `DnsSettings`). The table `haex_agent_browser_no_sync` stays as it is (browser
path); a column per setting would need a migration for a value that fits a key.

The tracker engine (`TrackerLists`) is created in `setup`, before any vault is open, because the browser tools are
registered there. Its settings are therefore applied when the vault opens (one process serves one vault, ADR 0003): until then
the engine runs on the defaults and the browser is not usable anyway (no vault, no rights). The list cache is a plain
download cache of public files and stays device-wide in `<AppLocalData>/filter-lists/`.

**Legacy import:** at the first open of a vault that has neither row, the values of `agent-browser/filters/settings.json` and
`agent-browser/dns.json` (or, after PR D1, `filter-lists/settings.json`) are written as rows and the files removed. A vault that
already has the row never reads them. With two vaults on one device the first one opened takes the choice; the second
starts from the defaults. This is accepted: it is the first vault the user used after the update.

## R8 — Nothing here encrypts

The root of a vault folder holds only SQLite files and three small bookkeeping files. Subfolders get no encryption in this
spec. When it comes (private mode, later), the key source is the opened vault, as with encrypted folders (spec 048, ADR 0013);
the layout needs no change for it, which is the only property this spec has to guarantee.
