---
description: 'Task list for End-to-End Tests Across Several Vaults and Devices'
---

# Tasks: End-to-End Tests Across Several Vaults and Devices

**Input**: Design documents from `specs/033-multi-device-e2e/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/group.md, contracts/driver-layer.md, contracts/failure-material.md, quickstart.md

**Tests**: Included as mandatory. The scenarios are the product of this feature and are `scripts/e2e/scenarios/*.test.ts`, run only by `pnpm test:e2e`. The helpers' own logic has display-free checks in `scripts/e2e/lib/*.test.ts` (`pnpm check:e2e-lib`). Test-only doubles are files ending in `.testlib.ts`. Test code never sits in a production file (spaex rule). No new runner and no new dependency: Node's `node:test`.

**Organization**: Phases follow the user stories of the spec in priority order after the shared foundations. Pull requests follow the plan's stages: PR A = Phase 1 and 2; PR B = Phase 3 and 4 (M1 to M4, M9); PR C = Phases 5 to 7 (M5 to M8); PR D = Phases 8 to 10 and Polish. Ask before every push or PR.

**Commands**: through the host bridge, for example `nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --grep sync-`. Build only the relevant Rust targets (`-j 4`); building all integration targets at once has crashed this machine.

## Format: `- [ ] Txxx [P] [USn] Description with file path`

`[P]` = can run in parallel (different files, no dependency on an unfinished task). `[USn]` = the user story of the spec.

## Phase 1: Setup

- [x] T001 In the new worktree run the baseline once and keep the result for the PR text: `pnpm check:e2e-lib`, `pnpm typecheck:scripts` and `pnpm test:e2e --grep sync-`; all must pass before any change (baseline of main `4ea5292`).
- [x] T002 Consult graphify before authoring named artifacts (spaex rule): `graphify query` for the candidates `linkDevice`, `startFirstDevice`, `restartDevice`, `closeDevice`, `onlyServers`, `expectThreads` in `scripts/e2e/lib/sync-flows.ts`, `captureFailure` in `scripts/e2e/lib/artifacts.ts`, `startNostrRelay` in `scripts/e2e/lib/nostr-relay.ts`; record what was evaluated and why the group layer extends rather than duplicates them in the PR text. If a query fails, say so and continue.
- [x] T003 Confirm line counts of the files this work touches (`scripts/e2e/lib/scenario.ts` 482, `artifacts.ts` 92, `sync-flows.ts` 238, `nostr-relay.ts` 99) and note that `scenario.ts` may gain only a wiring change (500-line rule).

## Phase 2: Foundational (blocks every user story)

**Goal**: the group layer, the driver-layer seam, per-device failure material, a relay that returns on its URL, and the checks for them. All existing scenarios keep passing unchanged.

- [x] T004 [P] Create `scripts/e2e/lib/platform/host.ts` with `DeviceHost`, `RunningDevice` (extends the `Page` of `lib/page.ts` with `stop()`, `kill()`, `alive()`, `step()` and `screenshot()` returning PNG bytes) and `DataHandle` (`copyVaultFile(vaultName, to)`, `keep(folder)`, `dispose()`), exactly as in `contracts/driver-layer.md`. Types only, no platform names.
- [x] T043 **Gate G4** (before T005): check whether a running device can be taken offline without restarting it: network namespace per device and a firewall rule per process, on this machine and for the `ubuntu-24.04` runner. Result 2026-10-02: an in-place cut works in a nested user and network namespace here, but it needs every device, driver, screen and the relay to start inside that namespace, and unprivileged user namespaces are restricted by default on `ubuntu-24.04`; a firewall rule per process needs root. Decision: the Linux implementation is the restart with servers none; `research.md` R2, `spec.md` FR-013, `contracts/group.md`, `contracts/driver-layer.md`, `data-model.md`, `plan.md` and this file were amended in the same commit.
- [x] T005 Create `scripts/e2e/lib/platform/linux.ts` implementing `DeviceHost` over `lib/instance.ts`, `lib/processes.ts` and `lib/webdriver.ts`. `kill()` is SIGKILL of the process group without shutdown. `copyVaultFile` copies the vault file and its companion files (`<name>.db`, `-wal`, `-shm`, not `.lock`) from `<data>/space.haex.vault/instances/` and MUST refuse a running source and MUST report an error instead of a partial copy. Depends on T004 and T043.
- [x] T006 [P] Add `scripts/e2e/lib/platform/linux.test.ts` (with a `linux.testlib.ts` double where needed): copy includes the companion files, refuses a running source, reports failure on a missing file, `kill` skips shutdown. Depends on T005.
- [x] T007 Change `src-tauri/src/bin/e2e_nostr_relay.rs` to read an optional port (environment variable `HAEX_E2E_RELAY_PORT`) and start `nostr_sdk::local_relay::LocalRelay::builder().port(p)`; without it behave as today. No `unwrap` or `expect` on the parsed value: an unparsable port exits with a message. Build only this bin: `cargo build -j 4 --manifest-path src-tauri/Cargo.toml --features e2e --bin e2e_nostr_relay` through the host bridge.
- [x] T008 Change `scripts/e2e/lib/nostr-relay.ts`: choose a free port with `lib/ports.ts`, pass it by T007's variable, return a relay with `url`, `state`, `stop()` and `start()` that returns on the same URL. A second `start()` while up is an error. Depends on T007.
- [x] T009 [P] Add `scripts/e2e/lib/nostr-relay.test.ts`: the pure part (command and environment construction, state transitions, double start refused) against a fake spawn. Depends on T008.
- [x] T010 Create `scripts/e2e/lib/group.ts`: `ctx.group({ users: { name: [deviceNames] }, relay: 'shared' })` returning a Group per `data-model.md`. Constraints quoted: device names unique across users; more devices than `E2E_MAX_DEVICES` (default 6) refused before any device starts with a message that names the limit; passphrases random per run, never a literal; the first device of each user creates the vault with servers set to the group relay only (`onlyServers`); other devices of a user link as `linkDevice` does today, or through the form when `link: 'form'` is given. Everything started ends with the scenario in reverse order. Depends on T005, T008.
- [x] T011 Extend `scripts/e2e/lib/group.ts` (or a sibling file if it would pass about 350 lines) with the device operations of `contracts/group.md`: `start`, `stop`, `kill`, `restart`, `goOffline` (servers none, restart), `goOnline` (servers back, restart), `setNostrRelays`, `deviceList`, `status`, `identity`, `copyVaultTo`. State machine exactly as in `data-model.md`: `stopped -> running`, `running -> stopped|killed|offline`, `offline -> running`, `killed -> running`; every state ends `stopped` at scenario end. Depends on T010.
- [x] T012 [P] Add `scripts/e2e/lib/group.test.ts` with `group.testlib.ts` (a fake `DeviceHost`): planning of users and devices, unique names, the device limit message, passphrase randomness, each state transition and the refused ones, teardown order after a failure. Depends on T010, T011.
- [x] T013 Create `scripts/e2e/lib/group-expect.ts`: `expectThreads` (reuse of `sync-flows.ts`), `expectOnline`, `expectLastSeen`, `expectRole`, all with `fixed: true` deadlines that `E2E_TIME_SCALE` does not stretch. Depends on T011.
- [x] T014 Change `scripts/e2e/lib/artifacts.ts` and add `artifacts.test.ts` cases: per-device folder `<scenario>/<encoded-user-length>-<encoded-user>-<encoded-device-length>-<encoded-device>/` with `driver.log`, `screenshot.png` (or `screenshot-note.txt` when the device is gone) and `data/` kept on failure or `--keep`; timeline steps name their device; layout exactly as `contracts/failure-material.md`. Depends on T005.
- [x] T015 Wire `ctx.group` into `scripts/e2e/lib/scenario.ts` and pass the device list to artifact capture; the file MUST stay at or under 500 lines. If it would exceed 500, first extract `ScenarioContext` into a new file in a separate, behavior-free commit. Depends on T010, T014.
- [x] T016 Add `scripts/e2e/lib/seam.test.ts`: scans `scripts/e2e/scenarios/*.test.ts` and every helper in `scripts/e2e/lib/` except `lib/platform/` and tests for imports of `instance.ts`, `processes.ts`, `webdriver.ts`, `build.ts`, `node:child_process`, `node:os`, `node:fs` and for `process.kill`, `xvfb`, `tauri-driver`, `/proc`. It starts with an explicit `KNOWN_VIOLATIONS` list of today's offenders (`sync-copy-notice.test.ts` and any helper found), each with a reason; the list shrinks to empty by T034. (`ponytail:` ceiling: a text scan, not a type-level guarantee; upgrade path is an ESLint rule.)
- [x] T017 Prove nothing regressed: `pnpm check:e2e-lib`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check` and `pnpm test:e2e --grep sync-` all pass with the existing scenarios unchanged. Depends on T004 to T016.

**Checkpoint** (PR A): the group layer, the seam and the relay port exist and are checked; the old suite is green.

## Phase 3: User Story 1 - Linking, sync and identity through the real interface (P1) 🎯 MVP

**Goal**: M1, M2, M3 as scenarios driven through the start page, the chat and the device view.

**Independent Test**: `pnpm test:e2e --grep sync-link`, `--grep sync-two-devices`, `--grep sync-identity` pass; breaking linking or sync on purpose makes the matching one fail with per-device material.

- [x] T018 [US1] Add `data-testid="link-submit"` to the submit button of the form in `src/components/onboarding/LinkSheet.vue` (the only application change besides the relay binary), following the hook rules of `specs/016-e2e-testing/contracts/test-hooks.md`; add it to the hook table in `scripts/e2e/README.md`.
- [x] T019 [US1] **Gate G2** and M1: rewrite `scripts/e2e/scenarios/sync-link.test.ts` so B joins through the form: open `landing-link`, type the code into `link-code`, `link-device-name`, `link-vault-name`, the passphrase fields `#link-passphrase` and `#link-passphrase-confirm`, add the group relay in `link-servers-nostr-input`/`-add` and switch the three defaults off with `link-servers-nostr-toggle`, click `link-submit`. On A: the device name shows, `link-role-linked` stays selected, `link-confirm`. On B: `link-progress` reaches `data-state="done"`, `link-open-vault`, and B shows all of A's data. Both device views list B with `data-role="linked"` and `data-online`. Keep the existing refusal path (`link-reject`). Measure whether the join works on loopback with default iroh relays and write the result into `research.md` under G2; if it does not, stop and amend the plan (Complexity Tracking) before continuing. Depends on T018, T017.
- [x] T020 [US1] M2: rewrite `scripts/e2e/scenarios/sync-two-devices.test.ts` so a chat is created through the chat interface on A and appears on B; a setting changed on B (through the settings view) applies on A; B is stopped, A works, B is started and catches up. Where the interface lacks a stable hook for creating the chat or the setting, add the smallest `data-testid` and record it in `scripts/e2e/README.md`. Waits use `fixed: true` deadlines. Depends on T017.
- [x] T021 [P] [US1] M3: add `scripts/e2e/scenarios/sync-identity.test.ts`: both devices show the same npub in `settings-identity-npub`, `settings-identity-copy` puts it on the clipboard (label becomes "Kopiert"), the npub element is not an input and not editable, and on the linked device `settings-link-device` and `settings-device-remove` are absent and `settings-main-only` shows. Depends on T017.

**Checkpoint**: US1 independently complete and demonstrable.

## Phase 4: User Story 2 - Online state and the servers devices find each other through (P1)

**Goal**: M4 and M9, plus the relay-returns edge case.

**Independent Test**: `--grep sync-presence`, `--grep sync-servers-off`, `--grep sync-relay-return` pass.

- [x] T022 [US2] M4: add `scripts/e2e/scenarios/sync-presence.test.ts`: stop B gracefully and check that A's row for B drops `data-online` and its `settings-device-status` reads in the "zuletzt online" family with "gerade eben" or at most "vor 1 Min."; then start B and kill it (`kill()`), and check the same within the fixed 60 s deadline (research R1). The deadline is `fixed: true`; the scenario ends as soon as the expectation holds and MUST end within 90 s (SC-004). Depends on T013, T017.
- [x] T023 [US2] M9: add `scripts/e2e/scenarios/sync-servers-off.test.ts`: on both linked devices switch all Nostr relays off through the servers view (`settings-servers-nostrRelays-*` hooks), restart both, check that they do not show each other online within a bounded window (positive control: the same pair starts online with the relay), that local work on each succeeds, then switch the relays on again, restart, and check that they find each other and exchange both sides' work. (`ponytail:` ceiling: "do not find each other" is a bounded wait, not a proof; upgrade path is a counter of presence meetings in the application.) Depends on T013, T017.
- [x] T024 [US2] **Gate G1** and the relay edge case: add `scripts/e2e/scenarios/sync-relay-return.test.ts`: stop the group relay, check that both devices stay up and local work succeeds, start it on the same URL, and check that the devices find each other again within a fixed deadline. Record in `research.md` under G1 whether and how fast the application's client reconnects. A failed reconnect is a failed scenario and blocks G1 and the dependent Stage 2 work; keep the scenario and the measured finding, then amend the plan before proceeding. CI stays red until the gate is resolved. Depends on T008, T013.

**Checkpoint** (PR B ready: T018 to T024): M1 to M4 and M9 are automatic.

## Phase 5: User Story 3 - Removing devices and mutual removal (P2)

**Goal**: M6 and M7.

**Independent Test**: `--grep sync-remove-device`, `--grep sync-mutual-removal` pass.

- [x] T025 [US3] M6: add `scripts/e2e/scenarios/sync-remove-device.test.ts` with devices A (main), B (linked), C (main), D (linked): on A open `settings-device-remove` for D, check `remove-device-view` states the consequences before `remove-device-confirm`, confirm; D shows `settings-federation-notice` (removed) and `sync_status.thisDevice == 'removed'`, receives no new thread; A, B, C keep syncing a new thread. Depends on T017.
- [x] T026 [US3] M7: add `scripts/e2e/scenarios/sync-mutual-removal.test.ts` with A and C main, B linked: `goOffline` A and C, remove C on A and A on C through the interface, `goOnline` both, then check for either outcome: exactly one of A and C reports `main` and the other `removed` with the notice, B lists only the winner, B still has a main device. The tie rule itself stays covered by the unit tests `on_a_tie_the_smallest_hash_wins` and `two_main_devices_removing_each_other_leave_exactly_one_main_device` in `src-tauri/src/sync/device_list_tests.rs`. Depends on T011, T017.

## Phase 6: User Story 4 - Copies of the vault file (P2)

**Goal**: M5.

**Independent Test**: `--grep sync-copy` passes.

- [x] T027 [US4] M5: replace `scripts/e2e/scenarios/sync-copy-notice.test.ts` by `scripts/e2e/scenarios/sync-copy.test.ts`: stop A (main), `copyVaultTo` a new device C, start C: `copy-notice` shows, C is main and syncs both ways with A; copy linked B's file to D: D shows awaiting admission and neither sends nor receives; D makes a change; on A `admission-requests` lists it, `admission-admit` admits; D syncs and its change arrives everywhere; a second copy E is refused with `admission-reject` and stays outside. No injected store state, no `node:fs`. Depends on T005, T011, T017.
- [x] T028 [US4] Delete `sync-copy-notice.test.ts` and remove it from the seam allowlist in `scripts/e2e/lib/seam.test.ts`. Depends on T027.

## Phase 7: User Story 5 - Locking during a large sync (P2)

**Goal**: M8.

**Independent Test**: `--grep sync-lock-during-sync` passes.

- [x] T029 [US5] **Gate G3** and M8 (amended after G3: the receiving device is locked, N = 10,000 made by a loop on the page; see research.md R7): add `scripts/e2e/scenarios/sync-lock-during-sync.test.ts`: stop B, create N threads on A with `create_thread`, start B, wait until B holds some but not all, press `lock-instance` on A; check on B that A turns offline within the close promise of spec 013 plus detection time; start A again and check that B ends with exactly N threads, none missing and none duplicated. Calibrate N so the window is at least 2 s on a maintainer's machine and on the CI runner; record N and the measured window in `research.md` under G3. If no N gives 2 s, report the achieved overlap and amend the plan (bulk helper) before merging. Depends on T013, T017.

**Checkpoint** (PR C ready: T025 to T029): M5 to M8 are automatic.

## Phase 8: User Story 6 - Writing a multi-vault scenario takes few lines (P2)

**Goal**: the helpers are documented and a newcomer can use them in 60 lines.

**Independent Test**: a reviewer who has not seen the helpers writes the example from the README alone.

- [x] T030 [US6] Add `scripts/e2e/scenarios/sync-two-users.test.ts`, 60 lines or fewer, two users with two devices each: Anna's second device is made unreachable and restored (amended after CI: stopped and started, because a device after `goOffline` still dials the devices it knew at their last address); Ben's vault never shows Anna's data; it is also the isolation check (US6 scenario 4) and the template. Depends on T017.
- [x] T031 [US6] Extend `scripts/e2e/README.md` with "Scenarios with several vaults and users": the group call, the device operations table of `contracts/group.md`, the waiting rules, the forbidden list, and `sync-two-users` as the example. Depends on T030.
- [x] T032 [US6] Run the newcomer test (SC-003; run 2026-10-03 with a fresh agent that saw only the README and `sync-two-users`: it wrote the relay-pause variant in 42 lines, asked 12 questions, and its scenario failed on a wrong belief that `g.relay.stop()` cuts established connections; the README now has the imports, the helper signatures, the relay semantics and a note on waiting for absence): ask a reviewer (or a fresh agent session with only the README and the template) to write a two-user, two-device, unreachable-and-restore scenario; record its length and the questions asked; fix the README where they stumbled.

## Phase 9: User Story 7 - The platform stays out of the scenarios (P3)

**Goal**: SC-005 and SC-007.

**Independent Test**: `pnpm check:e2e-lib` fails when a scenario imports a platform file; `scripts/e2e/PLATFORMS.md` has all four entries.

- [x] T033 [P] [US7] Write `scripts/e2e/PLATFORMS.md` with an entry each for Windows, macOS, Android and iOS: driver, runner or device type, how several devices would be connected, known limits, from `research.md` R9; state at the top that the facts are to be confirmed in each follow-up spec and nothing is implemented.
- [x] T034 [US7] Empty `KNOWN_VIOLATIONS` in `scripts/e2e/lib/seam.test.ts` except `scenarios/relaunch-after-lock.test.ts` (it reads the virtual screen's framebuffer; a process-level scenario of spec 013 that stays Linux specific, with its reason in the list) and prove the scan works by a negative case (a temporary scenario that imports `../lib/processes.ts` makes the check fail; remove it again). Depends on T028.
- [x] T035 [P] [US7] Correct the stale sentence in `specs/016-e2e-testing/spec.md` if it still names two-process scenarios as out of scope (done in #199; verify, change nothing if correct).

## Phase 10: User Story 8 - The new scenarios run in the existing CI job (P3)

**Goal**: FR-024, SC-002, SC-008.

**Independent Test**: a branch with a broken sync fails the `e2e` job, names the scenario and keeps material per device.

- [x] T036 [US8] The default whole-run limit of `pnpm test:e2e` rose from 600 s to 1800 s in PR B, because the sync scenarios alone now run longer than 600 s (a full local run failed at the limit). In `.github/workflows/ci.yml` check that the `e2e` job's `timeout-minutes` (45) still holds the whole suite and raise it only if the first CI run shows it is needed; keep every scenario name starting with `sync-` so `--grep sync-` runs them alone.
- [x] T037 [US8] Deliberate-failure trial (V7 of `quickstart.md`): on a throw-away branch make `device_remove` do nothing and push; the job fails, names `sync-remove-device`, and the uploaded `e2e-failure-material` has `screenshot.png` and `driver.log` in each encoded device folder. Record the run in the PR text.
- [x] T038 [US8] Measure SC-002: over the 20 most recent runs of the `e2e` job on `main`, each of the nine scenarios passes in at least 19; list every failure of one of them with its cause, and merge a fix before T040 for each failure caused by the scenario or the rig (a failure from a defect of the product does not count against the scenario). Tracked after merge; this task stays open until the 20 most recent runs meet this. (Reworded 2026-10-04 from "20 consecutive green runs".) Met on 2026-10-04 over the 20 runs on `main` from 1218f38 (#223) to 8e1d86f (#247): eight of the nine scenarios pass in all 20; `sync-two-devices` passes in 19, its failure at f023e7a came from the rig (the helpers opened windows before the session restore had run) and has its fix merged in #243. The two failures of `sync-relay-return` (71ea159, 4df2df7) do not count, it is not one of the nine; its scenario was fixed in #240.

## Phase 11: Polish and cross-cutting

- [x] T039 [P] Run the whole CI set locally (the Rust steps are not needed here, this PR changes no Rust file): `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `pnpm lint:rust`, `cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib sync::`, `pnpm check:settings`, `pnpm check:templates`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`, `pnpm check:e2e-lib`; revert regenerated `src/types/bindings/` whitespace with `git checkout -- src/types/bindings/`.
- [x] T040 After T038: replace the manual section of `specs/024-own-device-sync/quickstart.md` by a pointer to `pnpm test:e2e --grep sync-` and close T081 in `specs/024-own-device-sync/tasks.md` with the scenario run as its record (FR-011).
- [x] T041 [P] Check every touched file stays at or under 500 lines (`wc -l`), tests are in dedicated files, and no agent reference appears in any file, comment, commit or PR text.
- [x] T042 Run `quickstart.md` V1 to V8 once end to end and record the result in the PR text. V1 to V3, V7 and V8 are recorded in #230, V4 and V5 run in the `e2e` job, V6 ran on a fresh build of `main` (#249, #253).

## Dependencies and order

- Phase 1 first. Phase 2 blocks everything else; inside it: T004 before T005 and T010; T007 before T008 before T010; T010 before T011 before T012, T013; T014 before T015; T016 can follow T015; T017 closes the phase.
- Phase 3 and Phase 4 are independent of each other after T017; T018 before T019. Phases 5, 6, 7 are independent of each other and of 3 and 4 after T017. T026 and T027 also need T011 and T005.
- Phase 8 needs T017 (and benefits from T011's finished operations). Phase 9: T034 after T028. Phase 10: T036 can start after T017; T037 after at least one new scenario exists; T038 after the PRs are merged.
- T040 after T038.

## Parallel opportunities

- T004, T006 (after T005), T009 (after T008), T012 (after T011), T016 once the layout is fixed.
- After T017: T020, T021, T022, T023, T025, T026, T027, T029 and T030 each touch their own scenario file and can be written in parallel; run them one at a time (the rig runs scenarios sequentially and the machine is limited).
- T033 and T035 any time.

## Implementation strategy

- **MVP**: Phase 1, Phase 2 and Phase 3 (US1): the group layer with linking, sync and identity through the real interface. It already retires M1 to M3 and proves the seam.
- **Increment 2**: Phase 4 (M4, M9, relay return). **Increment 3**: Phases 5 to 7 (M5 to M8). **Increment 4**: Phases 8 to 10, then T038 and T040 once the 20 green CI runs exist.
- Each gate (G1, G2, G3) is a measurement written into `research.md`; a failed gate stops that story and amends the plan before anything depends on it.
- Ask before every push or PR; PR texts carry no agent reference.
