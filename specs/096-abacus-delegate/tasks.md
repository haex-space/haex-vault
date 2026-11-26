---
description: 'Task list for the Abacus AI CLI delegate (spec 096)'
---

# Tasks: Abacus AI CLI delegate

**Input**: Design documents from `/specs/096-abacus-delegate/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/tauri-commands.md](contracts/tauri-commands.md), [quickstart.md](quickstart.md)

**Tests**: Included. The success criteria (SC-002 to SC-008) are measurable series and the repository rules require tests for boundaries and failure behaviour. Rust tests live in sibling `*_tests.rs` files, never inline; the real Abacus account is never used by a test (FR-015), a stub program and a `wiremock` double of the sign-in service stand in.

**Organization**: Grouped by user story so each can be implemented and tested on its own. US1, US2 and US3 are P1; the feature's purpose is only met when all three are done (MVP = Phases 1 to 5). Tasks marked **(operator)** need the operator's real Abacus account or devices and are never done by a test or by the implementing agent alone; each records its result in `research.md` as a "Measured" line.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 to US5 as in spec.md; Setup, Foundational and Polish tasks carry none
- Paths are relative to the repository root

## Commands used by the verification tasks

- Rust, through the host bridge: `nix develop --command scripts/with-nix-host-bridge.sh cargo <args>`. On a memory-limited machine use `-j 1` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0` inside a 10 GB scope; host builds outside the bridge need `env -u CC -u CXX -u AR`.
- Frontend and scripts: `nix develop --command pnpm <script>`; in a fresh worktree run `nix develop --command pnpm install --frozen-lockfile` first.
- After `cargo test` regenerates ts-rs bindings: `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.
- Prettier runs from the worktree folder (`prettier --check .` over everything before a push).

---

## Phase 1: Setup

**Purpose**: file skeletons and the signature fixture

- [x] T001 [P] Create the skeletons with module documentation and sibling empty `*_tests.rs` files (`#[cfg(test)] #[path = "..."] mod tests;`): `src-tauri/src/providers/delegate_credential.rs`, `src-tauri/src/providers/abacus_signin.rs`, `src-tauri/src/adapters/cli_delegate/abacus.rs`; declare them in `src-tauri/src/providers/mod.rs` and `src-tauri/src/adapters/cli_delegate/mod.rs`.
- [x] T002 [P] Record the signature fixture of research §1: copy the 10 411-byte bundle `abacusai-linux-x64.sigstore` of release 2.10.23 to `scripts/cli-tools/fixtures/abacusai-2.10.23-linux-x64.sigstore` and the program's SHA-256 (`108c4ba6e4076fc911959169bb7b94e64106af89ff83c3a8e32a050f91591d56`) to `scripts/cli-tools/fixtures/abacusai-2.10.23-linux-x64.sha256`, with a `README` line saying where they come from. The bundle verifies against the bare digest (`sigstore verify identity --bundle <bundle> --cert-identity <identity> --cert-oidc-issuer https://token.actions.githubusercontent.com sha256:<digest>`), so the 157 MB program is not committed.

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: the four manager additions, the vendor definition, the vendor enum everywhere, the credential module, the maintainer script. This is pull request A (plus the enum plumbing the later ones need).

**CRITICAL**: no user story starts before this phase is done

- [x] T003 [P] `src-tauri/src/cli_tools/platform.rs`: add `PlatformKey::LinuxX64Baseline` (`linux-x64-baseline`), `PlatformKey::candidates() -> Vec<PlatformKey>` (the variant first, then the base key), and `host()` choosing the variant when the host is Linux on x86_64 and AVX2 is missing (`std::is_x86_feature_detected!("avx2")`, wrapped in a function that takes the answer as a parameter so a test needs no CPU). `as_str`/`parse` know the new key. Tests in `platform_tests.rs`: every `(os, arch, avx2)` row, the candidate order, Claude/Codex keys unchanged.
- [x] T004 [P] `src-tauri/src/cli_tools/catalog.rs`: `ArtifactKind::Zip` (`"zip"`, needs `unpacked_size` like `tar.gz`); `Artifact.requires_avx2: bool` (default false, `deny_unknown_fields` kept); a platform or executable key may be `linux-x64-baseline`; `versions` may be empty; `artifact`, `executable_for`, `newest_for` look up through `PlatformKey::candidates()`. Tests in `catalog_tests.rs`: a zip entry validates and one without `unpacked_size` is refused, a variant key parses, an empty-version vendor parses, lookup falls back from the variant to the base key and a vendor without a variant entry is unaffected, the shipped catalog still validates.
- [x] T005 [P] `src-tauri/src/cli_tools/unpack.rs`: zip unpacking (the `zip` crate that is already a dependency) with the escape refusal, link and device refusal (zip Unix mode bits) and size accounting of the `tar.gz` path, in `spawn_blocking`; the executable bit on unix. Tests in `unpack_tests.rs` with zips built in the test: normal unpack, `..` and absolute entries refused, symlink entry refused, unpacked size above the catalog's refused.
- [x] T006 `src-tauri/src/cli_tools/install.rs`, `status.rs`, `installed.rs`: take the `Zip` branch where `TarGz` is chosen (`install.rs` around lines 237 and 409 to 415, `status.rs` around 125); look the entry up through the candidates; "fits this device" holds when the recorded key is one of the host's candidates; an artifact with `requires_avx2` on a host without AVX2 reads `unsupported_platform`; a vendor with no tested version reads `no_tested_version` and `cli_tools_install` refuses with that reason. Tests in `install_tests.rs`, `status_tests.rs`, `installed_tests.rs`, `commands_tests.rs`: zip install through `wiremock`, the avx2 rule (Abacus entries only), `no_tested_version`, a recorded `linux-x64-baseline` installation fits a host whose candidates include it. Needs T003, T004, T005.
- [x] T007 Add the vendor definition `abacusai` to `src-tauri/src/cli_tools/catalog.json` as in `data-model.md` §1 (`download_hosts`, `executable` for every key including `abacusai.exe`, `env_set` `ABACUS_AUTO_UPDATE=off` and `ABACUS_DISABLE_AUTO_MEMORY=1`, `env_remove_prefixes` `ABACUS_`, `ABACUSAI_`, `CODELLM_`, `config_env` `ABACUSAI_CODING_AGENT_DIR`, and the measured credential binding mode) with `"versions": []`: no version is installable until the operator's measurement (plan D8). Extend `catalog_tests.rs`: the vendor parses, has the listed variables, and `Catalog::shipped().vendor("abacusai")` reports no installable version. Needs T004.
- [x] T008 `DelegateVendor::Abacus` (`as_str` `"abacusai"`) and every match that names the vendors, compile-driven (research §9): `src-tauri/src/adapters/cli_delegate/mod.rs` (`as_str`, `parse`, `list_models`, `stream_chat`), `autonomy.rs`, `haex_mcp.rs`, `process.rs`, `src-tauri/src/providers/connect.rs`, `providers/add.rs`, `providers/mod.rs` (`build_adapter`), `src-tauri/src/chat/model_loading.rs`, `src-tauri/src/agent_profiles/delegates.rs`, `src-tauri/src/mcp_server/delegates.rs`, `mcp_server/registry.rs` (`AgentKind` gets an Abacus delegate kind and its string). The arms that run a request, list models or connect return a typed `AdapterError` "not implemented yet" for now; T020, T019 and T021 replace them. Extend `mod_tests.rs`: `parse`/`as_str` round trip, an unknown vendor is still refused in `providers/add.rs`.
- [x] T009 [P] Frontend vendor lists: `src/lib/cli-tools.ts` (`CliToolVendor`), `src/composables/useProviders.ts` (`DelegateVendor`), `src/composables/useModelInventory.ts` (`DELEGATE_VENDORS`), `src/composables/useToolNotice.ts`, `src/lib/actions/settingsActions.ts` (the enum), `src/components/chat/MessageList.vue` and the i18n key `chat.model.delegate.abacusai` in `src/i18n/locales/en.json` and `de.json`; add the vendor name to the key list that `scripts/check-cli-tools.ts` checks. `pnpm typecheck` and `pnpm check:cli-tools` pass.
- [x] T010 `src-tauri/src/providers/delegate_credential.rs`: `FEATURE = "delegate"`, `CALLER = Caller::Internal { feature: FEATURE }`, the fixed `ReadWrite`/`Scope::All` grant, the tag `haex-delegate-<vendor>`, `save(db, vendor, kind, secret, device_scope)` (title `Delegate: <display name>`, secret in `password`, custom fields `kind` and `device`, tag), `read(db, vendor, device_scope) -> Ready { kind, secret } | Missing`, and `delete(db, vendor)` for vault-wide disconnect. `device_scope` is `vault` for a shared credential and the installation ID for a device-bound credential. `read` filters the headers an Internal caller sees by tag and exact scope; duplicates are resolved by newest `updated_at` within that scope and `save` purges only duplicates in that scope. A save from device B must preserve device A's entry. Disconnecting the provider deletes all its scopes; duplicate provider-row cleanup does not delete credentials while a provider row survives. `Missing` covers both a deleted entry and one the sync has not brought yet: no row names the entry's id, so the deleted-row marker of `marketplace/credentials.rs` cannot be used, and the `not_connected` text says so. Tests in `delegate_credential_tests.rs`: save/read/delete by scope, same-scope duplicates resolved, cross-device entries preserved on save, global disconnect removes all scopes, missing entry is `Missing`, an entry that arrives later reads `Missing` before and `Ready` after, the entry is invisible to `Caller::BuiltinAgent` and visible to `Caller::User`.
- [x] T011 Provider-row rules of the review (contracts, "exactly one row"): in `src-tauri/src/storage/providers.rs` and `providers/connect.rs` (`write_delegate_provider`), at most one `cli_delegate` row with `adapter = "abacusai"` per vault: an upsert reuses it; duplicates from sync are reduced to the row with the lexicographically smallest id and the credential of the survivor is kept. `providers/mod.rs` `delete_provider` deletes the password-manager entry only when it removes the last Abacus row. Tests in `connect_tests.rs` and `providers/mod_tests.rs`: two rows reduce to one, deleting a duplicate keeps the entry, deleting the last deletes it, a missing entry is not an error, `claude` and `codex` rows are untouched.
- [x] T012 [P] `scripts/cli-tools/add-version.ts` and `scripts/cli-tools/lib/entry.ts`: `--vendor abacusai --version X --platform <key>` accepts an explicit platform selector (repeatable); Abacus requires at least one selector and processes only selected platforms. Other vendors retain their existing all-platform default. `--measure-only` downloads, verifies and unpacks the selected platform into an operator-only temporary folder for pre-publication measurements, then exits without writing the catalog. For each selected platform, measure SHA-256 and size on the stream, list archive entries (`tar -t` / zip listing) for unpacked size and refuse links, devices and escaping paths, unpack the bundle and program into a throw-away folder, and verify the bundle with `sigstore verify identity` (or `cosign verify-blob`, one must be on `PATH`, and the script says which it used) against the program's SHA-256 with the pinned identity `^https://github\.com/abacusai/codellm/\.github/workflows/release-cli-native\.yml@refs/tags/cli-v<version>$` and issuer `https://token.actions.githubusercontent.com`. Refuse to write an entry whose bundle does not verify and say that a signature by a private repository's workflow shows who released the file, not what it contains. Write `requires_avx2` for keys that need it. Header comment updated. Tests in `scripts/check-cli-tools.ts`: the identity pattern accepts the tag of its version only, platform selection excludes unselected keys, measure-only leaves the catalog untouched, command builder and entry shape, and, when `sigstore` is on `PATH`, the fixture of T002 verifies and a changed digest fails.
- [x] T013 [P] `scripts/cli-tools/check-containment.py` (+ `test_check_containment.py`): know the vendor `abacusai` (version query `--version`, the variables of the vendor definition, the same `strace` measurement and `allowed-residuals.json`); the unauthenticated series of research §4 passes with no entry added to the residual list.

**Checkpoint**: manager additions, vendor data, enum and credential module in place; user stories can start

---

## Phase 3: User Story 1 - Install the Abacus tool and connect (Priority: P1) MVP

**Goal**: a user with an Abacus account on a machine with no Abacus CLI and no Node installs the tool into the vault, connects with their normal credentials from the UI, and gets an Abacus-answered reply from the vault's copy.

**Independent Test**: quickstart steps 2, 4 and 5 (operator), plus the automated tests below.

### Tests for User Story 1

- [x] T014 [P] [US1] `src-tauri/src/providers/abacus_signin_tests.rs` with `wiremock`: the first call sends `{email, deviceId}` with a random UUID `deviceId` and returns the prompt; polling returns `success: false` twice then the device code; a `success: false` on the first call ends with the vendor's `error` text; ten minutes without confirmation ends with `expired` (paused tokio time); cancelling stops polling at once and sends no further request; no `Debug` or log output of the client, the prompt or the error contains the e-mail address, the code or the device code.
- [x] T015 [P] [US1] `src-tauri/src/providers/connect_tests.rs`: the Abacus branch end to end against the double: `connect_started`, then `delegate-connect-progress` `awaiting_browser` with the address and code, then `success`; the device code is saved once, one provider row exists with empty credentials; a failure of either write restores the prior credential and row and returns an error (review of the contract); a missing e-mail is `InvalidInput`; a vault without the installed tool fails before any network call; the record lines of the flow carry fixed text only.
- [x] T016 [P] [US1] `src-tauri/src/adapters/cli_delegate/abacus_tests.rs` with a shell stub that speaks the protocol of `data-model.md` §4: the command line equals the contract; the child has `CODELLM_DEVICE_CODE` and never `ABACUS_API_KEY`, even when the host has one; the host's `ABACUS_MODEL`, `ABACUSAI_*` and `CODELLM_*` are removed; the `user` line is written; text deltas, tool use and the `result` line become the chat stream; a `fatal` `not_authenticated`/`access_denied` ends as "credential refused" (leads back to connecting, no retry, no other backend); `network_unreachable` ends as "network"; `Missing` ends as `not_connected`; the run folder is removed; unknown message types are ignored.
- [x] T017 [P] [US1] `src-tauri/tests/cli_delegate_abacus_run.rs`: a request through `CliDelegateAdapter` with `DelegateRuntime::for_stub("abacusai", ...)`, a stub program and a vault with a stored credential: reply labelled with the backend, nothing started from the host's `PATH` (the program is spawned by its own absolute path from the runtime, and the test asserts that the program that ran is exactly that path).

### Implementation for User Story 1

- [x] T018 [US1] `src-tauri/src/providers/abacus_signin.rs`: the client of the two calls (`start(email, device_id) -> Prompt { uri, code, interval }`, `poll(code, cancel) -> DeviceCode`) over `reqwest` with one client per runtime (see the CI flake note of spec 094), the fixed base address `https://apps.abacus.ai` with a test override, a fixed user agent constant (adjusted if quickstart 3.0 shows the service needs one), five seconds default interval, ten minutes limit, a `Secret` newtype with redacted `Debug` for the e-mail, the code and the device code, and no logging of bodies or answers. Makes T014 pass.
- [x] T019 [US1] `src-tauri/src/providers/connect.rs`: `ConnectCliDelegateArgs.email: Option<String>`; the Abacus branch of `connect_cli_delegate` as in the contract (installed check, `connect_started`, start, `awaiting_browser` event, poll, save the credential in the vendor's measured scope with `delegate_credential::save`, provider upsert as one logical operation, `connect_finished`, `success`); `DelegateConnectState` keeps a cancel token per vendor; `cancel_cli_delegate_connect(vendor)`; register the command in `src-tauri/src/lib.rs`; regenerate and commit the bindings (`cargo test`, then the trailing-whitespace `sed`). Makes T015 pass; needs T010, T011, T018.
- [x] T020 [US1] `src-tauri/src/adapters/cli_delegate/abacus.rs`: `spawn_abacus_invocation`: reads the credential through `delegate_credential::read` using the vendor's measured binding scope (shared `vault` or this installation ID), builds the command of the contract (`-p --input-format stream-json --output-format stream-json --permission-mode supervise --no-agents-md`, `--mcp-config` with the HaexVault MCP server as `haex_mcp.rs` builds it for the other vendors), runs it through `runtime.contain(...)` and only then sets `CODELLM_DEVICE_CODE`; the JSON-lines writer and reader of `data-model.md` §4 with the mapping to stream deltas, the fatal-reason mapping and the "ignore unknown types" rule; `list_models` returns the single account-default entry (plan D7); replaces the placeholder arms of T008. Makes T016 and T017 pass.
- [x] T021 [US1] `src-tauri/src/providers/mod.rs` (`build_adapter`) and `src-tauri/src/chat/model_loading.rs`: the Abacus arm builds the adapter with no credential bytes and the open vault's `VaultDb` so the adapter can read the entry at request time; the legacy-id check in `model_loading.rs` leaves Abacus alone.
- [x] T022 [US1] Frontend connect: `src/components/settings/ConnectDelegateProvider.vue` (the per-vendor state maps and the Abacus row): the install action as for the other tools, then Connect asks for the e-mail address, shows the address and the code from `delegate-connect-progress` the way the Codex row does, offers to open the address with the existing opener, calls `cancel_cli_delegate_connect` when the dialog closes, shows errors as toasts (`expired` has its own text); `src/composables/useProviders.ts` passes `email`; i18n keys in `en.json` and `de.json`. Needs T009, T019.
- [x] T023 [US1] `src/composables/useModelInventory.ts`: the Abacus entry cannot be chosen as a model while the tool is not installed or the backend is not connected and takes the user to the install or connect action in Settings (spec 094 US1 and FR-004); `src/composables/useErrorString.ts` maps the new reasons (`credential_refused`, `expired`, `no_tested_version`, `not_connected`).
- [x] T024 [US1] End-to-end: a stand-in sign-in service next to the stand-in release host (debug builds only, patterned on `src-tauri/src/cli_tools/stand_in.rs` and its `HAEX_CLI_TOOLS_STAND_IN`), and the scenario `scripts/e2e/scenarios/abacus-connect.test.ts`: install from the stand-in host, connect (the double confirms the code), disconnect. The request through the stub program is covered by `src-tauri/tests/cli_delegate_abacus_run.rs` (T017), since this suite has no delegate chat. Written, not run locally; it runs on a manual dispatch (e2e does not run on pull requests).
- [ ] T025 [US1] **(operator)** Quickstart §3.0 (the `curl` check of the sign-in service), §2 (install) and §4 (connect), §5 (a real request). Before a catalog entry exists, run `node scripts/cli-tools/add-version.ts --vendor abacusai --version 2.10.23 --platform linux-x64 --measure-only`; install/run only from its operator-only temporary folder. This path verifies and unpacks the measured artifact but does not publish it or make it installable to normal users. Record under "Measured" in `research.md`: whether the service answers a plain client or needs a user agent (adjust T018), the time to confirm, the expiry, and install time for SC-001. If the service refuses other clients, stop and report: the plan's D1 gate applies. **Partly measured 2026-11-24 (research §12):** the sign-in service answers a plain client and its not-confirmed answer differs from the quickstart (fixed in `abacus_signin.rs`); install and connect through the app wait for T044; the cost shows that a request spends plan credits.

**Checkpoint**: US1 works end to end with the stand-ins; the real run is the operator's.

---

## Phase 4: User Story 2 - The credential belongs to the vault, not to the device (Priority: P1)

**Goal**: connect once, carry the credential with the vault to every device, nothing in a file, disconnect and refusal behave as spec 007 asks.

**Independent Test**: quickstart step 6 (operator) and step 7's search for the device code, plus the automated tests below.

### Tests for User Story 2

- [x] T026 [P] [US2] `src-tauri/src/providers/delegate_credential_tests.rs` (additions): the entry saved through one `VaultDb` handle is read through a second handle after the sync step of the test harness (`src-tauri/tests/passwords_sync.rs` shows the pattern); no file in the vault folder or the temp folder contains the device code after connect, a request and a stop (a search of both trees).
- [x] T027 [P] [US2] `src-tauri/src/providers/connect_tests.rs` and `src-tauri/tests/cli_delegate_abacus_run.rs` (additions): disconnecting during a running response does not disturb it (the stub keeps its own `CODELLM_DEVICE_CODE`); afterwards a request is refused with `not_connected`; a stub that answers `not_authenticated` yields the "connect again" result and nothing is retried.

### Implementation for User Story 2

- [x] T028 [US2] `src-tauri/src/providers/mod.rs` (`delete_provider`): the last-Abacus-row rule of T011 wired to the command, with the confirmation the settings page already asks for; `cli_tools_remove` stays tool-only (the page deletes the provider with the tool, spec 094 US4).
- [x] T029 [US2] Frontend states in `src/components/settings/ConnectDelegateProvider.vue`: "connect again" after a refused credential, a "may still be syncing" hint for `not_connected`, the disconnect action with its confirmation; `src/composables/useErrorString.ts` texts in `en.json` and `de.json`.
- [ ] T030 [US2] **(operator)** Quickstart §6: first the device-binding check (take the code of 3.0 and run 3.2 with it on a second machine), then the ten alternating runs of SC-002 after sync. Record the result under "Measured". If the vendor binds the code to its device, retain one credential entry per installation: connecting on B must not replace A's entry, each device reads only its own entry and connects once. Disconnect is vault-wide: it deletes every Abacus credential scope and the single provider row; cleanup of a duplicate provider row while one survives keeps all credential scopes. If the code works on all devices, use the shared `vault` scope. Same-scope duplicate resolution and vault-wide disconnect rules are in T010 and `data-model.md` §3.

**Checkpoint**: US1 and US2 together are the connect-and-carry story; run quickstart 4 to 7 before US3.

---

## Phase 5: User Story 3 - Abacus acts only within the user's approval posture (Priority: P1)

**Goal**: sensitive actions wait for the user or are blocked, as with Claude Code and Codex; if the tool cannot be held to that, Abacus is not offered.

**Independent Test**: quickstart step 3.4 and 5 (operator), plus the automated tests below.

### Tests for User Story 3

- [x] T031 [P] [US3] `src-tauri/src/adapters/cli_delegate/abacus_tests.rs` (additions): the stub emits `can_use_tool` for a read-only and for a changing tool; each is forwarded to `approval_bridge::request_approval` and answered `accept` or `reject` (the approval bridge carries no message of the user, so `reject_with_message` is not sent); the stub fails if it ever receives `allowAlways`, `allowYolo`, `allow_always_with_rules` or `question_answers` the user did not give; a malformed `request`, an `error` control response and a closed input end in a rejection; a `system/init` whose `permission_mode` is not `supervise` refuses the run; the stream continues after a refusal.
- [x] T032 [P] [US3] `src-tauri/src/adapters/cli_delegate/abacus_tests.rs` and `autonomy_tests.rs` (additions): under the posture that blocks sensitive actions the stub's changing tool receives no `accept`; under `AutonomyMode::Standard` ("always ask") with the approval-coverage flag of T034 false the request ends with the reason `approvals_unverified` and nothing is spawned; with the flag true it runs; stop kills the process and a closed vault stops a running request as `cancelled` (094 FR-010; the run holds a use lease, `src-tauri/src/cli_tools/uses.rs`).

### Implementation for User Story 3

- [x] T033 [US3] `src-tauri/src/adapters/cli_delegate/abacus.rs`: the approval mapping with the payload fields of `can_use_tool.request` (`tool_name`, `tool_input`; extraction tolerant, falling back to the raw JSON text for display and never to an automatic accept), `request_approval` with the workspace root and thread as the Codex path passes them, the answers of T031, the `system/init` check, and the use lease for the stop and vault-close paths.
- [x] T034 [US3] The release gate of plan D6 as code: a documented constant `READ_ONLY_TOOLS_REACH_APPROVAL_BRIDGE: bool = false` in `abacus.rs`, read in `stream_chat`; while it is false and the user's posture is `AutonomyMode::Standard`, a request ends with `approvals_unverified`, and `src/composables/useModelInventory.ts` shows the entry as unavailable with that reason (i18n text in `en.json` and `de.json`). The flag is flipped only by T035.
- [x] T035 [US3] **(operator)** Quickstart §3.4 and §5: demonstrate an approval round trip for a read-only tool and for a changing tool (`accept`, then `reject_with_message`), record the exact `can_use_tool.request` fields and the transcript. If both reach the bridge, flip the flag of T034 in its own commit that quotes the recorded evidence; if read-only tools bypass the bridge, leave it false and report: the backend stays unavailable under "always ask" until an enforcement path exists (spec FR-010, plan D6). **Done 2026-11-24 (research §12):** changing tools (`bash`, `write`) reach the bridge and `reject_with_message` stops them; read-only tools (`read`, `bash ls`) do not ask on their own; with the ask rule `*` the adapter now hands over in `--settings` they do, so the flag of T034 is flipped in its own commit.

**Checkpoint**: MVP (Phases 1 to 5) complete; run quickstart steps 2 to 6 before anything else.

---

## Phase 6: User Story 4 - Leave no trace on the host (Priority: P2)

**Goal**: the Abacus tool installs, connects, runs and is removed with nothing outside the vault's folder except listed residuals.

**Independent Test**: quickstart step 7 (operator), plus the automated tests below.

### Tests for User Story 4

- [x] T036 [P] [US4] `src-tauri/src/cli_tools/containment_tests.rs` (in the crate, because the release gate of T034 is closed to a test outside it, and the containment is what the task is about): under `EnvContainment` the stub sees `HOME`, `TMPDIR`, the XDG variables, `ABACUSAI_CODING_AGENT_DIR`, `ABACUS_AUTO_UPDATE=off` and `ABACUS_DISABLE_AUTO_MEMORY=1` below the vault's home, and none of the host's `ABACUS_*`/`ABACUSAI_*`/`CODELLM_*`; a stub that writes like the real tool (a log under `~/.abacusai/logs`, a pid file under `~/.abacusai/cli`, a config folder under `XDG_CONFIG_HOME`, a 13 MB file named `.bun-<uid>-<hash>.node` in `TMPDIR`) leaves nothing outside the vault's folder after a run, and the run folder is removed; the host's own `abacusai` earlier on `PATH` is never started (094 SC-002 for the third vendor; the adapter test `the_command_names_the_vaults_program_and_never_a_host_lookup` holds that part). The test passes the host's variables as a snapshot to the containment, so no process-wide variable is set.
- [x] T037 [P] [US4] Log-search test (the pattern of spec 094 SC-009; in `src-tauri/src/adapters/cli_delegate/abacus_tests.rs` with the shared `test_log` capture, because a request runs only inside the crate while the gate is closed): after connect, a request and a stop, no device-wide file HaexVault writes contains the e-mail address, the code, the device code, the vault's name or the request's text; `record.rs` lines of the connect and run events carry fixed text only.
- [x] T038 [US4] **(operator)** Run the pre-catalog containment measurement for Linux x64 with a real device code (Quickstart §3.1 to §3.7, plan D8). Record the files created in the throwaway home, `tmpdir`, `xdgcfg` and the agent directory, the external hosts, whether an update check is observed, and what is left after `kill -9`. Add an entry to `scripts/cli-tools/allowed-residuals.json` only for a residual the maintainer agrees to, with its reason; anything else is a change to the vendor definition or a reason not to ship the version (spec FR-013). **Measured 2026-11-24 and 2026-11-26 (research §§12–13):** the authenticated containment check reported no unlisted writes, so no Abacus residual was added. The app lifecycle check in §7 is part of T050 after T044.

---

## Phase 7: User Story 5 - Update, remove and repair like the other tools (Priority: P2)

**Goal**: Abacus appears next to Claude Code and Codex in Settings and behaves like them: version, update, go back, remove, reinstall, copied vault, damaged installation.

**Independent Test**: the automated tests below with a stub release; quickstart steps 8 and 9 (operator).

### Tests for User Story 5

- [x] T039 [P] [US5] `src-tauri/src/cli_tools/install_tests.rs`, `installed_tests.rs`, `commands_tests.rs` (additions) with a stub `abacusai` release served by `wiremock` (archive holds a stub program and a `.sigstore` file): install, the start check parses `Abacus.AI CLI version <semver>` and requires the installed version, update, confirmation by the first normally ended run, go back, remove; a damaged installation and one made for another platform read "reinstall"; the extra `.sigstore` file does not break the completeness check; a version outside the list is shown as untested.
- [x] T040 [P] [US5] `src-tauri/src/cli_tools/status_tests.rs` (additions; written with PR A, where the rule landed, and the musl case is `platform_tests.rs` plus `a_program_that_cannot_start_fails_the_start_check`): a Linux host without AVX2 selects the `linux-x64-baseline` entry, a macOS x64 or Windows x64 host without AVX2 reads `unsupported_platform` for Abacus (the `requires_avx2` rule of plan D3) while Claude Code and Codex stay installable there; a musl host is not detected and the start check is what reports it (`start_failed`).
- [x] T041 [P] [US5] `src-tauri/src/cli_tools/install_tests.rs` (additions): a Windows zip stand-in installs and runs the executable `abacusai.exe` resolved by the platform key; unpack refusals of T005 hold on the install path.

### Implementation for User Story 5

- [x] T042 [US5] Settings (checked, no change needed: the row is data-driven and shows `no_tested_version` with its own text and no install action, sizes come from the catalog entry): confirm `src/components/settings/CliToolRow.vue` and `src/composables/useCliTools.ts` need no change beyond the vendor name (they are data-driven); the Abacus row shows download size and unpacked size from the catalog entry; the `no_tested_version` state shows its own text ("no tested version yet") and no install action; i18n in `en.json` and `de.json`.
- [ ] T043 [US5] **(operator)** Quickstart §8 (copy a vault folder to another OS or architecture; remove the tool and disconnect; the files, the provider and the entry are gone) and §9 (Windows: install the zip build on `win32-x64`, run 2, 3.1 and 3.4, record the folders written under the account folders). Windows is its own pull request (plan D4); until it is measured the Windows keys stay out of the catalog.

---

## Phase 8: Polish and cross-cutting

- [ ] T044 **(operator)** After T038 passes and T025 and T035 are recorded: run `node scripts/cli-tools/add-version.ts --vendor abacusai --version 2.10.23 --platform linux-x64` for Linux only until other platforms have their own measurements (plan D3 and D4); after each platform's measurements, add its explicit `--platform <key>`. Review the generated entries and commit them to `src-tauri/src/cli_tools/catalog.json`. This is the first moment a version is installable (plan D8); until then the vendor reads `no_tested_version`.
- [x] T045 [P] `CONTEXT.md`: add entries for the delegate credential entry (owned entry of the password manager, tag `haex-delegate-<vendor>`), the Abacus sign-in (device code, the two private calls) and the vendors' credential places; mention in the Passwortmanager entry that an owned `delegate` entry exists. Keep the Vault folder entry as is.
- [x] T046 [P] Add a short "partly extended by 096" note to `specs/094-vault-cli-tools/spec.md` (the third vendor, the four manager additions) without changing its requirements, and extend its T071 follow-up line in `specs/094-vault-cli-tools/tasks.md` with the follow-ups of this spec once the operator agrees: moving the Claude Code and Codex credentials to the password manager (plan D9), the model list from the account (`get_model_catalog`, plan D7), a baseline archive for macOS x64 (plan D3).
- [x] T047 Gate greps, fail the task if any hits remain: `ABACUS_API_KEY` set anywhere in production code (it is only ever removed), a `Command::new("abacusai")` outside the adapter's use of the runtime's executable, `std::env::set_var` anywhere in the new code, the device code or e-mail address in a `log::`/`tracing` call, a `.join("cli-tools")` or `.join("home")` outside `vault_paths.rs` (`scripts/ci/check-vault-paths.py`).
- [ ] T048 Full verification of `src-tauri/` and the frontend through the bridge: `cargo fmt --check`, `cargo check`, `cargo clippy` (host and the `aarch64-linux-android` target, because of the cfg branches), `cargo test` (lib with `-j 1`, then the integration tests), `pnpm lint`, `pnpm typecheck`, `pnpm typecheck:scripts`, every `pnpm check:*`, `pnpm format:check`, `python3 -I scripts/ci/check-old-name.py`, `python3 -I scripts/ci/check-docs.py`, Prettier on every touched Markdown and JSON file; regenerate the bindings and run the trailing-whitespace `sed`; report what ran and what was unavailable.
- [ ] T049 Run the e2e suite on the branch (`gh workflow run ci.yml --ref <branch>`; e2e does not run on pull requests) and read the result, naming whether `abacus-connect` is among the scenarios that ran.
- [ ] T050 **(operator)** After T044, walk through `quickstart.md` on Linux with the real account and record the outcome per step and per platform in `research.md` under "Measured". Include the §7 host file listings after install, connect, request, stop, update (when two versions exist), disconnect and removal, plus the checks for the device code and device-wide log contents. Tick SC-001 to SC-008 one by one, each with its evidence; platforms not measured stay marked as such.

---

## Dependencies and execution order

- **Phase 1** has none. **Phase 2** depends on Phase 1 and blocks everything below.
- Inside Phase 2: T003, T004, T005, T009, T010, T012, T013 are parallel; T006 needs T003, T004 and T005; T007 needs T004; T008 first compiles everything else (do it early; the arms it leaves are replaced by T019, T020 and T021); T011 needs T010.
- **US1** (Phase 3) needs Phase 2. Inside it: T018, then T019 (needs T010, T011, T018), then T021; T020 needs T008 and T010; T022 needs T009 and T019; T023 needs T009; T024 needs T019 and T020; T025 needs all and is the operator's.
- **US2** needs US1's connect and adapter (T019 to T021). T026 and T027 are parallel; T028 follows T011; T029 needs T022; T030 is the operator's.
- **US3** needs US1's adapter (T020); it is independent of US2. T034 needs T033; T035 is the operator's and the only task that flips the flag.
- **US4** and **US5** need US1 and are independent of US2 and US3. T036 and T037 are parallel; T039 to T041 are parallel; T038 and T043 are the operator's.
- **Polish** needs the stories that are included; T044 needs the operator's results (T025, T035, T038) and is the only task that makes a version installable.

## Parallel examples

```text
After Phase 1:   T003 platform   T004 catalog   T005 unpack   T009 frontend lists   T010 credential   T012 add-version   T013 check-containment
US1 tests:       T014 signin_tests   T015 connect_tests   T016 abacus_tests   T017 run integration test
US2 tests:       T026 credential portability   T027 disconnect and refusal
US3 tests:       T031 approval mapping   T032 posture and gate
US4 tests:       T036 containment   T037 log search
US5 tests:       T039 install/update/remove   T040 variants   T041 windows zip
```

## Implementation strategy

1. **MVP first**: Phases 1 to 5 (US1, US2 and US3). Stop and run quickstart steps 2 to 6 with the operator before anything else; the gates of plan D1 (T025), D6 (T035) and the device-binding check (T030) decide whether the design stands.
2. **Incremental delivery** as stacked pull requests, in this order: A = Phases 1 and 2; B = US1 and US2 (the sign-in client, connect flow, credential, adapter and frontend; gated by T025); C = US3; D = US4 and US5 without Windows; E = Windows (T041, T043); F = Polish with T044. The repository rules require rebase-merge or a merge commit, never squash.
3. After each slice run the checks of T048 that apply to it, and let the operator review before the next slice is stacked (the operator often pushes review fixes to open branches and rebases them; fetch before pushing and never force-push).
4. Out of scope and tracked under T046: moving Claude Code and Codex to the password manager, the account's model list, the macOS x64 baseline archive, the OS sandbox, encryption of the vault subfolders.
