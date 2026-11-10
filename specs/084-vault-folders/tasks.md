# Tasks: Vault folders

Spec [spec.md](./spec.md), plan [plan.md](./plan.md). Each PR below is made from its own worktree off `main`; no PR stacks on
another. Tests live in `*_tests.rs`; files stay under 500 lines. Cargo only inside `nix develop` with
`scripts/with-nix-host-bridge.sh`.

## PR A — one place for the paths (no behaviour change)

- [x] T001 Add `src-tauri/src/instances/vault_paths.rs` with `VaultPaths::new(app_local_data)` and, for the **old** layout, `database(name)`, `pending_marker(name)`, `databases_dir()`, `profile_dir(name)` (`agent-browser/<name>`), `chat_files_dir(name)` (`chat-files/<name>`); name check stays in `paths.rs` and is called here (R1)
- [x] T002 Tests in `vault_paths_tests.rs`: every path of the old layout equals what `paths.rs`, `profile_store::dir` and `chat/peer/files/store::dir` build today for a normal name and for `filters`, `models`, `agent-browser`
- [x] T003 `instances/vault_lock.rs`: `probe(db_path) -> Result<Probe::{Free(DatabaseLock), Held}, String>` on top of `haex_crdt::DatabaseLock::try_acquire` (R5); unit tests that hold the lock from a second handle, release it by dropping, and fail on a lock file that cannot be made
- [x] T004 Make `get_instance_path`, `get_instances_directory`, `get_pending_marker_path` delegate to `VaultPaths`; move `agent_browser/profile_store::dir` and `chat/peer/files/store::dir` onto `VaultPaths` and update their callers (`agent_browser/env.rs`, `chat/peer/service.rs`) and the existing tests
- [x] T005 [P] Move the two test fixtures (`tests/common/sync_fixture.rs`, `tests/sync_link.rs`) and `browser_bridge/commands.rs` onto `VaultPaths`
- [x] T006 `scripts/ci/check-vault-paths.py`: fails when a Rust file outside `instances/vault_paths.rs` joins a vault name onto `instances`, `agent-browser` or `chat-files`; wire it into the docs/lint CI step next to the other `check:*` scripts and into `package.json`
- [x] T007 Run the vault, agent browser, chat and sync test targets (`-j 4`), clippy `--all-targets`, rustfmt, `pnpm lint`; open PR A

## PR B — the new layout and the move of the database (US1, US2)

- [x] T010 [US1] `VaultPaths`: `folder(name)` = `vaults/<name>`, `database(name)` = `vaults/<name>/vault.db`, `staging(name)`, `root()` (`subfolder(name, purpose)` comes with T030); the old-layout functions of T001 stay for the move only (`legacy_*`)
- [x] T011 [US1] `instances/layout_move.rs`: exclusive device lock `vaults.lock` (no shared-lock gate in the commands: the move runs in `setup` before any command, and the per-vault lock probe covers a vault another process holds); scan `instances/*.db`; per vault the staged move of R4 and the lock probe of R5; skip with a log line when `vaults/<name>/` exists, when the per-vault lock is held, or when a rename fails; never delete or overwrite
- [x] T012 [US1] Run the move from `lib.rs::setup` before `cleanup_orphans_on_startup` and before the first command (a second process waits on the device lock); remove `instances/` when empty
- [x] T013 [US1] Tests in `layout_move_tests.rs`: normal move incl. `-wal`, `-shm`, `.vault-id`; a vault with `.pending`; target exists; lock held; rename failure injected at each step; the move run twice; a vault named `filters`
- [x] T014 [US1] Kill-at-every-step test (SC-003): a step counter in the move that stops after N renames, N from 0 to the last; after each stop run the move again and compare the result with the uninterrupted run
- [x] T015 [US2] `list_instances` scans `vaults/*/vault.db` (skips names starting with `.` and folders with `vault.db.pending`); `open_instance`, `create_instance`, `link_vault`, `passphrase_change`, `browser_bridge` use `VaultPaths::database`
- [x] T016 [US2] `create`/`import`: reserve the name with `create_dir` on `vaults/<name>/`, remove the folder on failure; `import_instance_core` copies to `vault.db`; `vault_id::owner_of` scans `vaults/*/vault.db.vault-id`
- [x] T017 [US2] Orphan clean-up: a folder with `vault.db.pending` is removed as a whole, an empty folder (a creation that stopped before its marker) goes too; a legacy `.pending` marker moves with its vault and is cleaned up after the move (no second routine for `instances/`)
- [x] T018 [US2] Drop `get_instances_directory`/`TRASH_DIRECTORY` and the old-layout helpers that nothing uses; update `paths_tests.rs`, `create_tests.rs`, `import_tests.rs`, `startup_tests.rs`, `cleanup_tests.rs`, `vault_id_tests.rs`, `sync_link.rs`, `sync_fixture.rs`
- [x] T019 Integration test `tests/layout_move.rs`: an old-layout data folder with five vaults (one `filters`), the move, each vault opened with its passphrase, data equal (SC-001, SC-002)
- [ ] T020 Full local run of the vault, sync and e2e-relevant targets; open PR B; do not merge before M1–M3 of the quickstart are done by hand

## PR C — profile and chat files in the vault folder (US2)

- [x] T030 [US2] `VaultPaths::subfolder(name, purpose)` with `agent-browser` and `chat-files` as the only places for `profile_store::dir` and `chat/peer/files/store::dir`
- [x] T031 [US1] The move adopts the two legacy folders into an existing vault folder (`instances/layout_adopt.rs`: rename if the target is missing, else skip; skipped while another process holds the vault); idempotent, no staging needed; runs for every vault at every start until nothing is left. The profile of a vault named `filters` is adopted entry by entry and the list files (`settings.json`, `*.txt`, `*.part`) stay in `agent-browser/filters/` until PR D1
- [x] T032 [US1] Tests: adoption with and without a profile, with a target present, twice; the profile of a moved vault is found by `ProfileStore` and the chat files by `ChatFileStore` (use their existing round-trip tests)
- [x] T033 [US2] `agent_browser_profile_clear` and anything else that removed or listed `agent-browser/<name>` use the subfolder; a test that clearing the profile leaves `vault.db` and the other subfolders alone
- [x] T034 PR C: checks as T007; guard script T006 must pass without exceptions

## PR D1 — the list cache leaves the agent-browser folder (US3)

- [x] T040 [US3] `VaultPaths::filter_lists()` = `<AppLocalData>/filter-lists`; `agent_browser/env::filter_lists_dir` uses it; `device_dir` is no longer needed for lists
- [x] T041 [US3] Move at start: `agent-browser/filters/{settings.json,*.txt,*.part}` → `filter-lists/` (R6); for a vault named `filters` the profile is already adopted by T031, so only the list files are left in that folder
- [x] T042 [US3] Tests: lists moved; a vault named `filters` keeps its profile and the lists end up in `filter-lists/`; running twice changes nothing; `profile clear` of that vault no longer removes the lists
- [x] T043 PR D1: checks as T007

## PR D2 — the choices belong to the vault (US3)

- [x] T050 [US3] `agent_browser/choices.rs`: read/write `preferences` rows `agentBrowser.trackers` (nil UUID) and `agentBrowser.dns` (device UUID) as JSON; invalid value = absent; unit tests with an in-memory vault
- [x] T051 [US3] `TrackerLists`: no settings file; `set_settings` from the row when the vault opens; `apply` writes the row (`agent_browser_trackers_set`), the list cache stays in `filter-lists/`; update `filter_lists` tests
- [x] T052 [US3] DNS: `agent_browser_dns`, `agent_browser_dns_set` and `env::start_config` read and write the device row; `dns::load`/`dns::save` and the file go
- [x] T053 [US3] Legacy import at the first open of a vault with neither row: write the rows from `filter-lists/settings.json` and `agent-browser/dns.json` if present, then remove the files (R7); a vault with a row never reads them; tests for both orders
- [x] T054 [US3] Where the open path calls the settings: one function `apply_vault_choices(app, db)` called by `open_instance`, `create_instance` and `import_instance`
- [x] T055 [US3] Integration test: two vaults with different choices in turn (separate runs), the sync of the tracker row between two databases, the DNS row not read by another device
- [x] T056 PR D2: checks as T007; browser integration tests `agent_browser_dns`, `agent_browser_filters`, `agent_browser_profile`

## PR E — closing

- [ ] T060 Quickstart M1–M6 by hand (operator), on a copy of the real data folder first
- [x] T061 Update `docs/` and `CONTEXT.md` where they name `instances/`, `agent-browser/<name>` or `chat-files/<name>`; memory notes (only `CONTEXT.md` described the layout; the other hits in `docs/` are dated plans and reviews and stay as they were)
- [x] T062 Mark the spec Done. The follow-up spec "private mode" is not created: it stays under "Later" in the spec until the operator starts it with `/speckit-specify`

## Dependencies

A → B → C → D1 → D2 → E, each merged before the next starts (no stacking). T003 (lock probe) is in A, because B needs it.
Within a PR the order is the task order; [P] marks tasks on different files.
