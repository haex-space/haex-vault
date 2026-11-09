# Implementation Plan: Vault folders

**Branch**: `084-vault-folders` | **Date**: 2026-11-06 | **Spec**: [spec.md](./spec.md)

## Summary

One module owns the layout of a vault's files (`instances::vault_paths`). Every module that builds a path from a vault name
asks it. Then the layout changes to `<AppLocalData>/vaults/<name>/` with an automatic, restartable move at start, the
agent browser profile and the chat files go into subfolders, and the tracker and DNS choices move from files into the
vault's `preferences` table. Delivery is in stages, each its own PR from `main`, so the risky step (the move) is small and
reviewable on its own.

## Technical Context

- **Language/Stack**: Rust (Tauri 2), `haex-crdt` for the database and its lock, Vue only for the two settings pages that
  already exist (no new UI).
- **Storage**: SQLCipher vault; `preferences` table for the two choices (no migration, R7).
- **Testing**: `cargo test` unit tests in `*_tests.rs` next to the modules, one integration test that builds an old-layout
  data folder and starts the move (T0xx), a kill-at-every-step test for the move (SC-003).
- **Platforms**: Linux, Windows, macOS, Android (FR-011). Renames inside one volume only; the move is written against
  `std::fs` and tested on Linux with the failure cases injected (a rename that fails, a lock that is held), so the Windows
  and Android behaviour is the same code path.
- **Constraints**: no stacked PRs; files ≤ 500 lines; tests in separate files; one implementation per rule.

## Constitution Check

- _One implementation per rule_: `VaultPaths` is the single place that knows the layout (FR-003); a check script (T008)
  fails on a vault-named join outside it.
- _Credentials_: nothing here touches them.
- _Every OS_: see Platforms; no `cfg` gate for the layout.
- _Spec files in English_: yes.

## Stages and pull requests

| PR  | Stage | What                                                                                                                                                                        | Behaviour change            |
| --- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- |
| A   | 1     | `VaultPaths` resolver on the **old** layout; all call sites move to it; guard script                                                                                        | none                        |
| B   | 2     | New layout for the database, `vault.db`, listing, create, open, import, orphan clean-up; the move of database and sidecars (staging folder, lock probe, device lock); tests | vaults move                 |
| C   | 3     | `agent-browser/` and `chat-files/` into the vault folder; the move adopts them                                                                                              | profile and chat files move |
| D1  | 4a    | `filter-lists/` for the list cache; `agent-browser/filters/` split off (R6)                                                                                                 | lists move                  |
| D2  | 4b    | Tracker and DNS choice in `preferences`; legacy files imported once; engine takes its settings at vault open                                                                | choices move into the vault |
| E   | 5     | Quickstart M1–M6 by hand, memory and docs                                                                                                                                   | none                        |

Each PR keeps `main` working: after A nothing differs; after B the profile and chat files are still found at their legacy
places (they are keyed by name, not by the database path); after C they follow; D1 and D2 touch only device-wide files.

## Project Structure

```text
src-tauri/src/instances/
  vault_paths.rs          VaultPaths: root, folder(name), database(name), subfolder(name, purpose), staging(name)
  vault_paths_tests.rs
  layout_move.rs          the move at start: device lock, per-vault staging, lock probe, adoption of folders
  layout_move_tests.rs
  paths.rs                name check, get_app_local_data; get_instance_path & co. delegate to VaultPaths (A), then go (B)
  list.rs create.rs open.rs import.rs link_vault.rs passphrase_change.rs startup.rs cleanup.rs vault_id.rs   use VaultPaths
src-tauri/src/agent_browser/{profile_store,env,filter_lists,filter_settings,dns}.rs
src-tauri/src/chat/peer/files/store.rs, chat/peer/service.rs
src-tauri/src/browser_bridge/commands.rs
src-tauri/tests/layout_move.rs, tests/common/sync_fixture.rs, tests/sync_link.rs
scripts/ci/check-vault-paths.py
```

## Design notes

- **`VaultPaths`** takes the `AppLocalData` root (not an `AppHandle`), so tests build one for a temp folder and the Tauri
  entry (`get_*`) stays a thin wrapper. It never touches the disk except `ensure_folder`.
- **Move algorithm** (R4, R5): the move runs in `setup`, before any command can list, open, create or import a vault, and
  takes the exclusive device lock `vaults.lock` before scanning, so a second process starting at the same time waits and then
  finds nothing to do. No command needs a shared form of that lock: the code knows only `vaults/`, so nothing opens a vault
  of the old layout while the move is under way. The per-vault database lock protects a vault already open in another process.
  For each legacy vault, while the exclusive device lock is held:
  1. skip when `vaults/<name>/` exists (the move is done or a conflict; a conflict is logged and left alone);
  2. try the vault's lock non-blocking; skip when held;
  3. create `vaults/.moving-<name>/` (or reuse it after a crash);
  4. rename each existing legacy file (`.pending` first, so an interrupted creation stays recognisable; then `-wal`, `-shm`,
     `.vault-id`, then the database last, then the lock file) into it under the new names;
  5. rename the staging folder to `vaults/<name>/`; remove `instances/` when empty.
     An error at any step leaves the state a later start can continue from; nothing is deleted or overwritten (an existing
     target makes the step stop for that vault).
- **Adoption of folders** (stage 3) is the same idempotent pattern for `agent-browser/<name>` and `chat-files/<name>` into
  the existing vault folder: rename if the target is missing, skip otherwise.
- **Orphan clean-up** (startup): a `vaults/<name>/` with `vault.db.pending` is removed as a whole (`cleanup::take_back`); an
  empty folder goes too. A legacy marker moves with its vault and is cleaned up after the move; there is no second routine for
  `instances/`.
- **Import** reserves a name with `create_dir` on `vaults/<name>/` (atomic), copies the picked file to `vault.db`, and on
  failure removes the folder.
- **Settings at vault open** (D2): the open path of `instances` calls one function that reads both rows and hands the tracker
  choice to `TrackerLists::set_settings` and the DNS choice to the browser's start configuration. `StartConfig.dns` is read from
  the database at each start, not cached.

## Risks

| Risk                               | Answer                                                                                                                                                   |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A bug in the move loses a vault    | rename only, never delete or overwrite; staging; kill-at-every-step test; the user's real vaults are copied before the first run on them (quickstart M1) |
| WAL separated from its database    | files of a vault are never split across visible places (R4)                                                                                              |
| Another process has the vault open | lock probe (R5)                                                                                                                                          |
| A caller still joins `instances/`  | guard script T008 and the compile error when `get_instances_directory` goes (B)                                                                          |
| `EXDEV`/read-only data folder      | vault stays in `instances/` (not listed meanwhile), retried at next start                                                                                |
