---
description: 'Task list for CLI tools per vault (spec 094)'
---

# Tasks: CLI tools per vault

**Input**: Design documents from `/specs/094-vault-cli-tools/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/tauri-commands.md](contracts/tauri-commands.md), [quickstart.md](quickstart.md)

**Tests**: Included. The spec's success criteria (SC-002 to SC-011) are measurable test series and the repository rules require tests for boundaries and failure behaviour. Rust tests live in sibling `*_tests.rs` files, never inline.

**Organization**: Grouped by user story so each can be implemented and tested on its own. US1 and US2 are both P1; the feature's purpose is only met when both are done (MVP = Phases 1 to 4).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 to US5 as in spec.md; Setup, Foundational and Polish tasks carry none
- Paths are relative to the repository root

## Commands used by the verification tasks

- Rust, through the host bridge: `nix develop --command scripts/with-nix-host-bridge.sh cargo <args>` (a plain `nix develop --command cargo` fails in the GTK/WebKit sys crates). On a memory-limited machine use `-j 1` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`; host builds outside the bridge need `env -u CC -u CXX -u AR`.
- Frontend and scripts: `nix develop --command pnpm <script>`; in a fresh worktree run `nix develop --command pnpm install --frozen-lockfile` first.
- After `cargo test` regenerates ts-rs bindings: `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.

---

## Phase 1: Setup

**Purpose**: dependencies and the module skeleton

- [x] T001 Add `tar = "0.4"` and `flate2 = { version = "1", default-features = false, features = ["rust_backend"] }` to `src-tauri/Cargo.toml` (both are already in `Cargo.lock` as transitive dependencies; keep the existing resolved versions) and run `cargo check` through the host bridge.
- [x] T002 Create `src-tauri/src/cli_tools/mod.rs` with the module documentation (spec 094, what the module owns, what stays outside) and declare `mod cli_tools;` in `src-tauri/src/lib.rs`; add the submodule list as `pub(crate) mod` lines as the files appear.
- [x] T003 [P] Teach the CI path check the new folder names: add `cli-tools` and `home` to the name list and both regexes in `scripts/ci/check-vault-paths.py`, and add cases (a `.join("cli-tools")` outside `vault_paths.rs` fails, inside passes, in `*_tests.rs` is exempt) to `scripts/ci/test_check_vault_paths.py`.

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: paths, platform key, catalog, containment, runtime, installed-state, records, sweeps

**CRITICAL**: no user story starts before this phase is done

- [x] T004 [P] Add the folder names and accessors to `src-tauri/src/instances/vault_paths.rs`: constants `CLI_TOOLS = "cli-tools"` and `HOME = "home"`, methods `cli_tools(name)`, `cli_tool_dir(name, vendor)`, `home(name)`, `run_root(name)` (`home/run`), `records_dir(name)` (`home/records`); they use `subfolder` and touch no disk. Tests in `src-tauri/src/instances/vault_paths_tests.rs`.
- [x] T005 [P] Create `src-tauri/src/cli_tools/platform.rs`: `PlatformKey` (`linux-x64`, `linux-arm64`, `darwin-x64`, `darwin-arm64`, `win32-x64`, `win32-arm64`) with `PlatformKey::host()` and `PlatformKey::from_parts(os, arch)` returning `None` for anything else. Tests with injected `(os, arch)` values in `src-tauri/src/cli_tools/platform_tests.rs`.
- [x] T006 [P] Create `src-tauri/src/cli_tools/catalog.rs`: types `VendorDefinition` (vendor, display_name, per-platform executable, `env_set`, `config_args`, `env_remove_prefixes`, versions), `CatalogVersion`, `Artifact` (url, sha256, size, kind `file`|`tar.gz`, `unpacked_size`) exactly as in `data-model.md`; `load()` parses the embedded `catalog.json` (`include_str!`) through `parse(&str)`, which tests call with their own JSON; validation rejects versions not strictly descending by semver, invalid sha256, non-https or foreign-host urls, vendors without a version, and `tar.gz` artifacts without `unpacked_size`. Tests (including one over the shipped file) in `src-tauri/src/cli_tools/catalog_tests.rs`.
- [x] T007 [P] Write the maintainer script `scripts/cli-tools/add-version.ts` (`--vendor claude|codex --version X`): for Claude it fetches `manifest.json` and `manifest.json.sig` from `downloads.claude.ai/claude-code-releases/<version>/`, imports the pinned key (fingerprint `31DDDE24DDFAB679F42D7BD2BAA929FF1A7ECACE`) into a temporary keyring, runs `gpg --verify` and refuses on failure, then takes `checksum` and `size` per platform; for Codex it fetches `codex-package_SHA256SUMS` and the GitHub API asset `digest` for each `codex-package-<target>.tar.gz`, refuses when they differ, hashes the download while it streams (so the hash the entry carries is the one measured, and equal to both published values), explicitly does not verify any `.sigstore` bundle, and measures `unpacked_size` by streaming a tar listing. It inserts the entry newest-first into `src-tauri/src/cli_tools/catalog.json`. Keep the pure parsing and comparison helpers in `scripts/cli-tools/lib/entry.ts` and cover them with a Node check `scripts/check-cli-tools-catalog.ts` wired as `check:cli-tools-catalog` in `package.json`.
- [x] T008 Create `src-tauri/src/cli_tools/catalog.json` with the vendor-level data by hand from `data-model.md` (Claude: `env_set` `DISABLE_UPDATES=1`, `env_remove_prefixes` `ANTHROPIC_`, `CLAUDE_CODE_`; Codex: `config_args` `-c check_for_update_on_startup=false --disable plugins --disable plugin_discovery_prefetch --disable recommended_plugins --disable remote_plugin`, `env_remove_prefixes` `OPENAI_`, `CODEX_`; executables per platform, Codex's from the package's `entrypoint`), then run the T007 script for Claude Code 2.1.311 and Codex 0.169.0 and keep the generated versions. Depends on T006, T007.
- [x] T009 [P] Create `src-tauri/src/cli_tools/containment.rs`: `trait Containment { fn contain(&self, command: &mut tokio::process::Command, run: &RunFolder, vendor: &VendorDefinition) -> IsolationLevel }`, `enum IsolationLevel { BestEffort, Enforced }`, and `EnvContainment` implementing the six steps of the environment contract in `data-model.md` (host environment unchanged; remove `env_remove_prefixes`; set `HOME` or `USERPROFILE`/`APPDATA`/`LOCALAPPDATA`, the temp variables, the vendor config variable, `CLAUDE_CODE_TMPDIR`, `env_set`; set an `XDG_*` variable only where the host defines it; never touch `PATH`, locale, proxy and certificate variables; pass nothing else implicitly). It always returns `BestEffort`. Tests in `src-tauri/src/cli_tools/containment_tests.rs`: pass-through list survives, removal prefixes, XDG only-if-defined, HOME points into the vault home, child-only (the test process's own environment is unchanged afterwards).
- [x] T010 [P] Create `src-tauri/src/cli_tools/runtime.rs`: `DelegateRuntime { vendor, executable, home, run_root, containment }`, `DelegateRuntime::for_stub(vendor, path, dir)` for tests (the vendor id selects the shipped definition), `RunFolder` (creates `run_root/<8 hex>/` with mode `0700` on Unix before any run files are stored, plus `tmp/`, removed when dropped and swept at vault open), and a `confirm()` handle returning `Ok(())` for now (implemented in T053). Tests in `src-tauri/src/cli_tools/runtime_tests.rs`.
- [x] T011 [P] Create `src-tauri/src/cli_tools/installed.rs`: `InstallMarker` (`install.json`, schema 1, with `executable_size` and every package file's relative path and size) read and write, `Installation` (vendor, version, dir, confirmed), completeness (every recorded file exists with its recorded size, including the executable's `executable_size`) and device-fit checks, `list(paths, name, vendor)`, `resolve(...)` (highest complete, fitting version), `sweep_staging(...)` removing `.*.staging` folders, `sweep_runs(...)` removing everything under `home/run/` without following symlinked roots or ancestors. Tests in `src-tauri/src/cli_tools/installed_tests.rs` (complete, missing marker, executable of another size, wrong platform, wrong folder name, staging sweep, run sweep, symlinked cleanup roots).
- [x] T012 [P] Create `src-tauri/src/cli_tools/record.rs`: append-only record at `<vault>/home/records/tools.log`, rotated at 64 kB, lines carry a timestamp, an event name and fixed text only (never a vault name, an absolute path or request text). Tests in `src-tauri/src/cli_tools/record_tests.rs`.
- [x] T013 Add one function `cli_tools::open_sweep(paths, name)` (staging sweep and run sweep of T011; T051 adds the retry of deferred deletions) and call it where a vault session starts: `session_services::start` in `src-tauri/src/instances/session_services.rs`, the one place that `open.rs`, `create.rs` and `import.rs` all call (so the sweep is not copied three times). Test: a vault folder with a stray `.staging` folder and a populated `home/run/` is clean after the sweep, and other vaults and the records stay, in `src-tauri/src/cli_tools/mod_tests.rs` (the wrapper needs the real app handle, so the one-line call is covered by the quickstart and the e2e run). Depends on T011.

**Checkpoint**: foundation ready; user stories can start

---

## Phase 3: User Story 1 - Install a tool into the vault and use it (Priority: P1) MVP

**Goal**: a user on a machine with no `claude`, `codex`, `node` or `npm` installs a tool from the UI into the vault, connects, and gets a delegate-answered reply with the vault's own copy.

**Independent Test**: quickstart steps 1, 2 and 5; plus the automated tests below.

### Tests for User Story 1

- [x] T014 [P] [US1] `src-tauri/src/cli_tools/unpack_tests.rs`: raw file placement (executable bit on unix), `tar.gz` unpack, refusal of `..` and absolute entries, refusal of symlink, hardlink and device entries, partial archive.
- [x] T015 [P] [US1] `src-tauri/src/cli_tools/install_tests.rs` with `wiremock`: success publishes `<version>/install.json` and no staging folder; checksum mismatch leaves nothing unpacked (`integrity_mismatch`); 500 answer (`server_error`); refused connection (`offline`); body cut off (no half version, `.part` removed); cancel through the token (`cancelled`); second install of the same vendor (`locked`); too little free space (`no_space`); no artifact for the platform (`unsupported_platform`); program that exits 1 at the start check (`smoke_failed`, nothing published); a kill injected after each step (download, verify, unpack, start check, marker write, rename), followed by `open_sweep`, leaves either no new version or a complete one, never a mixture (SC-004); a vendor that exists only in a test catalog (`catalog::parse`) installs, resolves and lists without any change to the manager's code (SC-007).
- [x] T016 [P] [US1] `src-tauri/src/cli_tools/status_tests.rs` and `commands_tests.rs`: status derivation for `not_installed`, `installed`, `installing`, `unsupported`, that `cli_tools_status` never touches the network, and that on a device without the `cli_delegates` capability every command returns the `platform` unavailable reason (FR-016).
- [x] T017 [P] [US1] Adapter tests in `src-tauri/src/adapters/cli_delegate/claude_tests.rs`, `codex_tests.rs`, `mod_tests.rs`: starting without a complete installation fails before any spawn with `AdapterError::Unavailable { reason: "not installed in this vault: <vendor>" }`; `build_command` uses `runtime.executable`.

### Implementation for User Story 1

- [x] T018 [US1] Create `src-tauri/src/cli_tools/unpack.rs` (raw file and `tar.gz` into a staging folder using `tar`'s `unpack_in`, refusing escaping paths and link or device entries; blocking work inside `spawn_blocking`). Makes T014 pass.
- [x] T019 [US1] Create `src-tauri/src/cli_tools/install.rs`: per-vendor-and-vault lock (the publication-lock pattern of `src-tauri/src/models/paths.rs`), free-space check (pattern of `src-tauri/src/models/download_check.rs`), download with `models::download::download_to_file` raced against a cancel token and the vault token (as `models/commands.rs` does), SHA-256 with `models::hash::sha256_file` in `spawn_blocking`, unpack into `.<version>.<uuid>.staging/`, start check (run `<executable> --version` through the runtime and containment from the staging folder; exit 0 and the version in the output), write `install.json` last (with `executable_size`, the size of the executable after unpacking), rename to `<version>/`, record lines through `record.rs`, progress callback with phases `downloading`, `verifying`, `unpacking`, `publishing`. No log line carries a path or the vault name. Keep each file under about 500 lines (split `fetch.rs` out if needed). Makes T015 pass. Depends on T018.
- [x] T020 [US1] Create `src-tauri/src/cli_tools/commands.rs` with `cli_tools_status`, `cli_tools_install`, `cli_tools_cancel` as in `contracts/tauri-commands.md`, the cancel registry (pattern of `src-tauri/src/models/cancel.rs`), the events `cli-tool-progress` and `cli-tool-changed`, the stable error codes, and the ts-rs types `CliToolStatus` (with `isolation`, and `download_bytes` and `unpacked_bytes` of the version an install would fetch, from the catalog), `CliToolProgress` (domain-prefixed names); register the commands and managed state in `src-tauri/src/lib.rs`; return the `platform` unavailable reason where `cli_delegates` is false. Makes T016 pass. Depends on T019.
- [x] T021 [US1] Regenerate and commit the bindings `src/types/bindings/CliToolStatus.ts` and `src/types/bindings/CliToolProgress.ts` (`cargo test`, then the trailing-whitespace `sed`).
- [x] T022 [US1] The MCP helper receives holzi's endpoint folder explicitly: `src-tauri/src/mcp_server/delegates.rs` adds `--endpoint-dir <path>` to the generated helper arguments, `src-tauri/src/mcp_server/helper.rs` reads it and falls back to `default_socket_dir()` only when absent, `src-tauri/src/mcp_server/endpoint.rs` stays unchanged otherwise. Tests in `src-tauri/src/mcp_server/helper_tests.rs`; extend `src-tauri/tests/mcp_server_delegates.rs` so the helper still connects when its `HOME` and `XDG_RUNTIME_DIR` differ from the app's.
- [x] T023 [US1] Switch the adapter entry to the runtime in `src-tauri/src/adapters/cli_delegate/mod.rs`: `CliDelegateAdapter::new(vendor, credentials, runtime, context)` replaces the `binary: String` argument; resolution failure returns the `Unavailable` reasons of T017; adjust `src-tauri/src/adapters/cli_delegate/process.rs` so a spawn failure no longer mentions `PATH`.
- [x] T024 [US1] `src-tauri/src/adapters/cli_delegate/claude.rs`: `build_command` runs `runtime.executable`; replace `TempDir::new()` (the per-run config and working folder) by a `RunFolder` under `home/run/`; apply the containment to the command; keep attachments, system prompt, MCP config and approval socket inside the run folder.
- [x] T025 [US1] `src-tauri/src/adapters/cli_delegate/codex.rs`: same change for `TempDir::new()` and `build_command`; pass the vendor's `config_args` before `app-server`; `auth.json` is written into the run folder and removed with it.
- [x] T026 [US1] `src-tauri/src/adapters/cli_delegate/connect_claude.rs`, `connect_codex.rs` and `src-tauri/src/providers/connect.rs`: both connect flows take the runtime (run folder under `home/run/`, containment applied); the hard-coded `"claude"` and `"codex"` names (connect.rs near lines 117 and 139) go away; a missing installation returns the `Unavailable` reason with the install action.
- [x] T027 [US1] Approval socket path in `src-tauri/src/adapters/cli_delegate/claude.rs` and `approval_bridge.rs`: the socket lives in `home/run/<8 hex>/`; when the full path exceeds `MAX_SOCKET_PATH_BYTES` (104) it falls back to a random-named socket in the system temp folder (a listed residual), removed at the end of the run. Tests with a 64-character vault name in `src-tauri/src/adapters/cli_delegate/approval_bridge_tests.rs`.
- [x] T028 [US1] Build the runtime where adapters are built: `src-tauri/src/providers/mod.rs` (`build_adapter`), `src-tauri/src/chat/model_loading.rs`, `src-tauri/src/model_jobs/model_source.rs`, `src-tauri/src/model_jobs/delegate.rs`; add `cli_tools::runtime::for_vault(paths, name, vendor)` so each caller passes the open vault's `VaultPaths` and name (find how `model_loading.rs` already reaches the open vault).
- [x] T029 [US1] Move the integration tests to `DelegateRuntime::for_stub`: `src-tauri/tests/cli_delegate_claude.rs`, `cli_delegate_approval.rs`, `cli_delegate_autonomy_audit_trail.rs`, `cli_delegate_autonomy_deny_rules.rs`, `cli_delegate_autonomy_gated_permissive.rs`, `cli_delegate_autonomy_ungated.rs`, `cli_delegate_autonomy_unavailable.rs`, `cli_delegate_connect.rs`, `cli_delegate_disconnect_in_flight.rs`, `mcp_server_delegates.rs`; all stay green (unix).
- [x] T030 [P] [US1] Create `src/composables/useCliTools.ts`: status list, `install(vendor)`, `cancel(vendor)`, listeners for `cli-tool-progress` and `cli-tool-changed`; every failed action goes to `toast.error` with a fixed id.
- [x] T031 [P] [US1] Create `src/components/settings/CliToolRow.vue`: state text, install button with the download size and the space it takes next to it (no confirmation dialog), progress bar, cancel; no inline success text (the new state is the feedback).
- [x] T032 [US1] Add the row to each vendor in `src/components/settings/ConnectDelegateProvider.vue` and guide the connect button when the tool is not installed. Depends on T030, T031.
- [x] T033 [P] [US1] `src/composables/useModelInventory.ts`: a "not installed in this vault" picker entry beside the "not connected" one: it cannot be chosen as a model, and selecting it opens Settings > Providers at that vendor's tool row (see what the "not connected" entry does on selection and do the same); `src/composables/useErrorString.ts`: map `not installed in this vault`, `needs reinstall` and the `cli_tools.*` codes to actionable messages.
- [x] T034 [P] [US1] Add the strings (`settings.cliTools.*`, `chat.model.delegateNotInstalled`, error texts) to `src/i18n/locales/en.json` and `src/i18n/locales/de.json`.
- [x] T035 [US1] Add `scripts/check-cli-tools.ts` (Node sandbox check of `useCliTools` and the pure state mapping, in the pattern of the other `scripts/check-*.ts`; give it a `vue-sonner` double) and `check:cli-tools` in `package.json`; run every `check:*` script, since composables also run in DOM-less sandboxes.
- [ ] T036 [US1] End-to-end (written, not yet run: e2e does not run on pull requests, see T069): add `src-tauri/src/cli_tools/stand_in.rs` (a base-URL override read from `HAEX_CLI_TOOLS_STAND_IN`, patterned on `src-tauri/src/models/stand_in.rs`, compiled only for e2e) and the scenario `scripts/e2e/scenarios/cli-tools-install.test.ts` with a tiny fake tool fixture: install from the UI, row shows the version, no `.staging` folder left.

**Checkpoint**: US1 works alone; a tool installs and answers

---

## Phase 4: User Story 2 - The host's own tools and folders play no part (Priority: P1)

**Goal**: with a different host `claude` on `PATH` and a configured host home, the vault's copy runs, nothing but the listed residuals lands outside the vault folder, and no device-wide file names the vault.

**Independent Test**: quickstart steps 3, 4 and 7 plus the tests below.

### Tests for User Story 2

- [x] T037 [P] [US2] `src-tauri/tests/cli_tools_containment.rs` (unix): a stub delegate prints its environment and writes to `$HOME`, `$TMPDIR` and the vendor config variable; assert the file tree outside the vault folder is unchanged except the listed residuals (SC-003), `HTTPS_PROXY`, `PATH` and `LANG` arrive unchanged, the host's `ANTHROPIC_API_KEY` does not arrive in a Claude run (the Codex run drops `OPENAI_`, covered by `containment_tests.rs`), `XDG_*` is redirected only where the host defined it.
- [x] T038 [P] [US2] In `src-tauri/tests/cli_tools_containment.rs`: a host copy named like the vendor placed first on `PATH` is never started (SC-002), with and without a vault copy installed.
- [x] T039 [P] [US2] In `src-tauri/tests/cli_tools_containment.rs` (install and run; the connect log lines are fixed text in `connect.rs` and `connect_codex.rs`): capture every `log::` record during install and run and assert none contains the vault name, a tool path or request text, and that `home/records/tools.log` exists in the vault folder (SC-009).
- [x] T040 [P] [US2] Provider tests: `src-tauri/src/providers/add_tests.rs` (a `cli_delegate` row without `base_url` is accepted), `connect_tests.rs` (a connected row stores no `base_url`), `src-tauri/src/adapters/cli_delegate/mod_tests.rs` (`build_adapter` ignores `base_url`).

### Implementation for User Story 2

- [x] T041 [US2] `src-tauri/src/providers/add.rs`, `src-tauri/src/providers/connect.rs`, `src-tauri/src/providers/mod.rs`: `base_url` is no longer required, written (`upsert_delegate_provider`) or read (`build_adapter`) for `cli_delegate`; fix the doc comments in `src-tauri/src/storage/providers.rs`. Makes T040 pass.
- [x] T042 [US2] Log hygiene (the `codex login` line was done in PR B): `src-tauri/src/adapters/cli_delegate/connect_codex.rs` (the `log::warn!` that prints the raw stderr of `codex login`) and `src-tauri/src/providers/connect.rs` (the `log::warn!` that interpolates a full error) log fixed text only; install, update, connect and run events go to `record.rs` at their call sites. Makes T039 pass.
- [ ] T043 [US2] Measure what device-wide files holzi writes today name a vault, a tool or request text: write `scripts/cli-tools/check-device-traces.py <app-local-data> <needle>...` (searches every file below the app's local-data folder outside `vaults/`, including `logs/` and the webview stores `storage`, `localstorage`, `WebKitCache`, `cookies`; prints each match with its file) with a test over a fake tree in `scripts/cli-tools/test_check_device_traces.py`; run it after a full series (install, connect, request, stop, update, removal) in a build with the file log on, record the baseline and the result in `research.md`. A hit in a file this feature touches is fixed here; any other hit goes on the follow-up list of T071. Closes SC-009 at file level (T039 covers the log records). Depends on T042.
- [x] T044 [US2] Remove the `PATH` era (done in PR B with the adapter switch, which needed it): delete `DelegateVendor::install_hint` and its two callers (`src-tauri/src/adapters/cli_delegate/mod.rs`, `process.rs`, `connect_claude.rs` `is_missing_binary` use), so no message tells the user to `npm install` anything.
- [x] T045 [US2] Write `scripts/cli-tools/check-containment.py` (the measurement of `research.md` §3: run a given tool in an empty tree under `strace -f -y` with the environment of `EnvContainment`, resolve directory fds, list every path created, written, renamed or removed outside the tree) and `scripts/cli-tools/allowed-residuals.json` (the two listed residuals); `add-version.ts` runs it on Linux and refuses a version whose residue is not on the list (FR-021). Test with a fake program in `scripts/cli-tools/test_check_containment.py`.
- [x] T046 [US2] Show the isolation level: `CliToolRow.vue` renders `best effort` or `enforced` from `CliToolStatus.isolation` (SC-010); strings in both locale files; assertion in `scripts/check-cli-tools.ts`.

**Checkpoint**: US1 and US2 together are the MVP

---

## Phase 5: User Story 3 - Update on request, never silently (Priority: P2)

**Goal**: one version in use (the newest), the older kept only until the new one is confirmed, a start check before activation, a manual way back.

**Independent Test**: quickstart step 6 plus the tests below.

### Tests for User Story 3

- [x] T047 [P] [US3] `src-tauri/src/cli_tools/installed_tests.rs`: `confirm()` creates `.confirmed` and deletes older versions; an unconfirmed newest keeps the previous; `revert()` deletes the newest; a deletion that fails is repeated at the next open; resolution picks the newest complete version. (Done; the running-use claim is `cli_tools/uses.rs` with `uses_tests.rs`: a version a run uses is kept until the last run on it ends.)
- [x] T048 [P] [US3] `src-tauri/src/cli_tools/install_tests.rs`: update installs side by side; a start-check failure leaves the previous version untouched and removes staging; the series of SC-011 (one folder after a failed update, two after a good one, one after the first confirming run).
- [x] T049 [P] [US3] `src-tauri/tests/cli_tools_update_in_flight.rs` (unix): a stub delegate run is in flight while an update installs; the run finishes normally and a run started afterwards uses the new version (SC-008). Also: closing the vault while a stub run is in flight stops the child, the run ends as cancelled, `confirm()` is not called, the run folder is gone and no child process is left (FR-010); if closing does not stop delegate children today, add that where the vault's session services stop (find it next to `src-tauri/src/instances/session_services.rs`). (Done. Closing is the reply cancelled plus `ChildRegistry::kill_all`, which the drain already does and the delegate children are registered with, so nothing had to be added; the test drives both, and the registry alone.)
- [x] T050 [P] [US3] Adapter tests in `claude_tests.rs`, `codex_tests.rs`, `connect_claude_tests.rs`, `connect_codex_tests.rs`: `confirm()` is called only at a regular end; a user stop, a missing login and a network error never call it. (Done, but the run-level cases are in `src-tauri/tests/cli_tools_confirm.rs` (a stub tool per vendor through the real adapters), the Claude connect cases in `connect_claude_tests.rs` and `providers/connect_tests.rs`; `claude_tests.rs` and `codex_tests.rs` only parse lines and spawn nothing.)

### Implementation for User Story 3

- [x] T051 [US3] `src-tauri/src/cli_tools/installed.rs`: `confirm`, `revert`, `prune_older`, retried from `cli_tools::open_sweep` (T013), which both `open.rs` and `import.rs` already call. Makes T047 pass.
- [x] T052 [US3] `src-tauri/src/cli_tools/install.rs`: choose the version (newest listed, or the given listed one), treat a version above the list as `untested` and keep starting it, and a version below the oldest listed entry as `outdated` (the row says so and offers the update), list offered versions without any network access; the update path never touches the version in use. Makes T048 pass. (Done in `commands.rs::plan_for`, where the catalog meets the vault: an install never switches to an older version, a second update waits with `unconfirmed_update` while the version in use has a way back, `version_in_use` while a fallback is pending. A first install that no run has confirmed may be updated, because it has no way back to protect.)
- [x] T053 [US3] Implement `DelegateRuntime::confirm()` (calls T051) and call it at the regular end of a run: `claude.rs` (the `result` event), `codex.rs` (completed turn), `connect_claude.rs` and `connect_codex.rs` (credential obtained). Makes T050 pass.
- [x] T054 [US3] `src-tauri/src/cli_tools/commands.rs`: add `cli_tools_revert`, extend `CliToolStatus` with `confirmed`, `previous_version`, `untested`, `outdated`, `latest_listed_version`, state `update_available`; refuse `revert` with `nothing_to_revert` and `in_use` as in the contract; regenerate the bindings (T021 procedure).
- [x] T055 [P] [US3] Frontend: update button and "go back to X" in `CliToolRow.vue`, an untested badge, `install(vendor)` and `revert(vendor)` in `src/composables/useCliTools.ts`, strings in both locale files, assertions in `scripts/check-cli-tools.ts`.
- [x] T056 [US3] E2E `scripts/e2e/scenarios/cli-tools-update.test.ts` with two fake versions on the stand-in host: update, go back, update again and confirm. (Written, not yet run, like T036. "Confirm" goes through the debug-only `e2e_confirm_cli_tool`, which takes the place of a request that ends normally; there is no model account in e2e.)

**Checkpoint**: updates are user-driven and recoverable

---

## Phase 6: User Story 4 - Remove a tool, remove a vault (Priority: P2)

**Goal**: removing a tool or a vault leaves nothing of it.

**Independent Test**: quickstart step 9 plus the tests below.

### Tests for User Story 4

- [x] T057 [P] [US4] `src-tauri/src/cli_tools/commands_tests.rs`: remove deletes all versions of one vendor and leaves the other vendor and `home/` alone; `in_use` while a delegate child of that vendor is live; the stored provider row is not touched by the command (the settings action removes it).
- [ ] T058 [P] [US4] `src-tauri/src/instances/cleanup_tests.rs`: `take_back` removes a vault folder that contains populated `cli-tools/` and `home/` (SC-006).

### Implementation for User Story 4

- [x] T059 [US4] `src-tauri/src/cli_tools/commands.rs`: `cli_tools_remove`, with a per-vendor live-use counter held by `DelegateRuntime` for the lifetime of a run or connect flow; emit `cli-tool-changed`. Makes T057 pass.
- [x] T060 [US4] Frontend remove action in `DelegateRow.vue` (one row per provider since 2026-11-18, replacing `CliToolRow.vue`): a bin button, a confirmation dialog, and the guardrail action `settings.delegate.remove`, which removes the program and the stored connection; strings in both locale files.

**Checkpoint**: removal and vault removal are complete

---

## Phase 7: User Story 5 - A copied vault says what is missing (Priority: P3)

**Goal**: a wrong-platform, damaged or unrunnable installation is a clear state and is never started.

**Independent Test**: quickstart step 8 plus the tests below.

### Tests for User Story 5

- [ ] T061 [P] [US5] `src-tauri/src/cli_tools/installed_tests.rs`: `install.json` for another platform gives `needs_reinstall` (`platform`); a missing executable, or one whose size differs from `install.json`, gives `needs_reinstall` (`incomplete`); a spawn that fails with permission denied gives `not_runnable` with the reason and no copy elsewhere on the host.
- [ ] T062 [P] [US5] `src-tauri/src/instances/import_tests.rs`: an imported or synced vault arrives with no `cli-tools/` content, and its tools show `not_installed`.

### Implementation for User Story 5

- [ ] T063 [US5] `src-tauri/src/cli_tools/installed.rs`, `commands.rs` and `src-tauri/src/adapters/cli_delegate/process.rs`: derive `needs_reinstall` and `not_runnable`, map the permission-denied spawn error to an `Unavailable` reason `not runnable: <reason>`; never start an installation that does not fit. Makes T061 pass.
- [ ] T064 [P] [US5] Frontend states and strings for `needs_reinstall` (`platform`, `incomplete`) and `not_runnable` in `CliToolRow.vue`, `useErrorString.ts` and both locale files; reinstall is the install action.

**Checkpoint**: every state a user can reach has a message and an action

---

## Phase 8: Polish and cross-cutting

- [ ] T065 [P] Update `CONTEXT.md`: the "Vault folder" entry names `cli-tools/` and `home/`; add entries for Tool installation, Vault home, Containment, Tested-version list.
- [ ] T066 [P] Add a "partly superseded by 094" note (tool location, `base_url`, per-run folders in the OS temp folder) to `specs/007-cli-delegate/spec.md` and `specs/007-cli-delegate/data-model.md`; no other change to those files.
- [ ] T067 Gate greps, fail the task if any hits remain: `Command::new("claude")`, `Command::new("codex")`, `install_hint`, `TempDir` in `src-tauri/src/adapters/cli_delegate/*.rs` production code (the one approval-socket fallback of T027 is the only allowed exception and carries a `// residual:` comment the grep excludes), `base_url` reads for delegates in `src-tauri/src/providers/mod.rs`.
- [ ] T068 Full verification of `src-tauri/` and the frontend through the bridge: `cargo fmt --check`, `cargo check`, `cargo clippy` (host and the `aarch64-linux-android` target, because of the cfg branches), `cargo test` (lib with `-j 1`, then the integration tests), `pnpm lint`, `pnpm typecheck`, `pnpm typecheck:scripts`, every `pnpm check:*`, `pnpm format:check`, prettier on every touched Markdown and JSON file; report what ran and what was unavailable.
- [ ] T069 Run the e2e suite on the branch (`gh workflow run ci.yml --ref <branch>`; e2e does not run on pull requests) and read the result.
- [ ] T070 Walk through `quickstart.md` on Linux with real Claude and Codex subscriptions; on macOS and Windows as far as machines are available; record the outcome per platform in `research.md` under section 3 (platforms not measured stay marked as such).
- [ ] T071 Record the follow-ups as issues in the repository's tracker (link `specs/094-vault-cli-tools/spec.md`, Assumptions) once the operator agrees: runtime debug-log switch, OS sandbox for Linux and macOS, audit of other device-wide files that name a vault (including every hit of the device-trace measurement that this spec did not fix), and spec 096 (Abacus delegate on this tool manager).

---

## Dependencies and execution order

- **Phase 1** has none. **Phase 2** depends on Phase 1 and blocks everything below.
- Inside Phase 2: T004, T005, T006, T007, T009, T010, T011, T012 are parallel; T008 needs T006 and T007; T013 needs T011.
- **US1** (Phase 3) needs Phase 2. Inside it: T018 then T019 then T020 then T021; T022 (the MCP helper's explicit endpoint folder) before the adapter switch T023, because moving `HOME` and the temp variables would otherwise cut the helper off from holzi's endpoint; then T023, then T024, T025, T026 (different files, parallel after T023), then T027 (approval socket path, after T024: same file), then T028, then T029; the frontend tasks T030, T031, T033, T034 are parallel, T032 needs T030 and T031, T035 needs the frontend, T036 needs T019 and the frontend.
- **US2** needs US1's adapter wiring (T023 to T028). T037 to T040 are parallel; T041 to T046 are mostly independent files (T043 needs T042, T045 needs T007, T046 needs T031).
- **US3** needs US1's install and adapters; it is independent of US2. **US4** and **US5** need US1 and are independent of US2 and US3 (T054 and T059 and T063 all edit `commands.rs`, so they follow one another).
- **Polish** needs the stories that are included.

## Parallel examples

```text
After Phase 1:   T004 vault_paths   T005 platform   T006 catalog   T007 add-version   T009 containment   T010 runtime   T011 installed   T012 record
US1 tests:       T014 unpack_tests   T015 install_tests   T016 commands_tests   T017 adapter tests
US1 frontend:    T030 useCliTools    T031 CliToolRow      T033 inventory/errors  T034 i18n
US2 tests:       T037 containment   T038 host PATH   T039 log search   T040 provider tests
```

## Implementation strategy

1. **MVP first**: Phases 1 to 4 (US1 and US2). Stop and run quickstart steps 1 to 5 and 7 before anything else.
2. **Incremental delivery** as stacked pull requests, in this order: A = Phases 1 and 2; B = US1; C = US2; D = US3; E = US4 and US5; F = Polish. The repository rules require rebase-merge or a merge commit, never squash.
3. After each slice run the checks of T068 that apply to it, and let the operator review before the next slice is stacked (the operator often pushes review fixes to open branches; fetch before pushing and never force-push).
4. Out of scope and tracked as T071: debug-log switch, OS sandbox, audit of other device-wide files, Abacus (spec 096).
