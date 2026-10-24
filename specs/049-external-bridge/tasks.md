---
description: 'Task list for spec 049-external-bridge'
---

# Tasks: External Bridge für haex-pass-browser

**Input**: Design documents from `/specs/049-external-bridge/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/wire-protocol.md](./contracts/wire-protocol.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires an executable check for non-trivial logic:

- Rust unit tests go in sibling `*_tests.rs` files, registered with `#[cfg(test)] #[path = "…_tests.rs"] mod tests;`.
- Rust integration tests go in `src-tauri/tests/`.
- The WebCrypto interop check is `pnpm check:browser-bridge`.
- There is one e2e scenario.
- Tests come first inside each story and fail before the code exists.
- Real browsers are manual (quickstart M1–M11, research R17).

**Organization**: Grouped by user story and shipped in the plan's deliveries. Each delivery is one PR from its own worktree off `main`.

| Delivery | Phases     | Stories                                                                                      |
| -------- | ---------- | -------------------------------------------------------------------------------------------- |
| PR A     | Phase 1    | this plan                                                                                    |
| PR B     | Phases 2–4 | Foundational, US1, US6                                                                       |
| PR C     | Phases 5–6 | US2, US3                                                                                     |
| PR D     | Phase 7    | US5                                                                                          |
| PR E     | Phase 8    | US4                                                                                          |
| PR H     | Phase 9    | companion PR in haextension, parallel to B–E                                                 |
| PR F     | Phase 11   | recognise the official extension (FR-045, FR-046), added 2026-10-23                          |
| PR I     | Phase 13   | second companion PR in haextension: protocol 3, released as 1.4.0 before PR G                |
| PR G     | Phase 12   | one approval per instance, proof of the key, protocol 3 (FR-047 to FR-049), added 2026-10-23 |

Priorities: P1 US1, US2, US6; P2 US3, US4, US5.

**Shipping note**:

- PR B lands the migration once and with it the whole table, so C–E add no migration.
- The bridge is off by default (FR-001). PR B ships a bridge that connects and approves but answers every method `NOT_SUPPORTED` until PR C.
- Quickstart M1 (Origin header of real browsers) must pass before PR B merges (research R2).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US6)

**Conventions for every task**:

- **File size**: Files stay ≤ 500 lines. `passkeys_ops.rs` (426) and `settingsActions.ts` (482) get no new code; new code goes into new files (plan, Constraints).
- **Test code**: lives in separate files.
- **Rust**:
  - No `unwrap`/`expect` on input.
  - No localized text.
  - No secret, request URL or response content in a log line, error string, event or `Debug` output (FR-039, research R16).
- **Commits**: Conventional Commits without agent attribution.
- **Running Rust**: only through `nix develop --command scripts/with-nix-host-bridge.sh cargo …` with `CARGO_BUILD_JOBS=4` and `-j 4`, one test target at a time.
- **Before each commit**:
  - Run `pnpm generate:ts-types` after every Rust type change and commit `src/types/bindings/`.
  - Run `pnpm exec prettier --write` on touched Markdown/JSON.
- **New named artifacts**: Before the first new function, module, component or store of a delivery, run `graphify query` for it. Record reused names in the PR description.

---

## Phase 1: Setup (PR A)

- [x] T001 Worktree `.worktrees/049-external-bridge-plan` on branch `049-external-bridge-plan` from `main` (`35e1c34e`); `.specify/feature.json` points at `specs/049-external-bridge`
- [x] T002 Re-pin the client to haextension `c8133b94b37ca90f7b17011f4f58f8c9ab91d1f0` and record the 2026-10-21 clarifications in `specs/049-external-bridge/spec.md` (update-item unsupported, create-item four fields, companion PR, settings under Extensions)
- [x] T003 Plan, research, data model, contracts, quickstart in `specs/049-external-bridge/`; roadmap row in `plans/README.md`; K1 decided as (b) `prompt_park.rs`
- [x] T004 This task list; open PR A against `main` (rebase-merge, no squash)

---

## Phase 2: Foundational (PR B, blocks every story)

**Purpose**: The pieces every story uses: dependencies, envelope crypto, wire types, the new caller, the parked-prompt module, the table, the platform flag and the bridge lifecycle.

- [x] T005 New worktree `.worktrees/049-b` on branch `049-external-bridge-b` from `main` after PR A. Inside `nix develop`, run `pnpm install --frozen-lockfile`; do not symlink `node_modules`.
- [x] T006 Add the direct dependencies in `src-tauri/Cargo.toml` at the versions already in `Cargo.lock` (research R1, R3):
  - `tokio-tungstenite = "0.28"` with default features off plus `handshake`;
  - `aes-gcm = "0.11"`;
  - `curve25519-dalek = "5"`.

  Then check that `cargo tree -d` shows no new duplicate. Run `cargo check -j 4` in both configurations (default and `--no-default-features`).

- [x] T007 [P] Write the tests first, in `src-tauri/src/browser_bridge/envelope_tests.rs`:
  - the X25519 vectors of RFC 7748 §5.2 and §6.1 through `x25519(secret, public)`;
  - an all-zero shared secret (low-order public key) is refused;
  - an AES-256-GCM round trip with a 12-byte IV, tag appended, standard padded base64;
  - `client_id_of(public_key)` equals `hex(SHA-256(pk)[0..16])` in lowercase and refuses keys that are not 32 bytes;
  - decrypting with a wrong key or a wrong IV length is an error, never a panic.
- [x] T008 [P] Implement `src-tauri/src/browser_bridge/envelope.rs` (≤ 200 lines) and make T007 pass:
  - `ServerKeys` holds a random static secret, zeroized on drop, and exposes `public_b64()`.
  - `open_request(&ServerKeys, ephemeral_pub_b64, iv_b64, message_b64) -> Result<Vec<u8>>`.
  - `seal_response(client_pub: &[u8;32], plaintext) -> (ephemeral_pub_b64, iv_b64, message_b64)` uses a fresh ephemeral key per call.
  - Use `curve25519-dalek` `MontgomeryPoint::mul_clamped`, `aes-gcm` 0.11 and `base64` STANDARD.
  - Add a `ponytail:` comment on using the raw shared secret as the key (ceiling: no KDF, no AAD; upgrade path: protocol v3 with HKDF, research R3).
- [x] T009 [P] Tests in `src-tauri/src/browser_bridge/protocol_tests.rs`:
  - The literal JSON of every message in [contracts/wire-protocol.md](./contracts/wire-protocol.md) parses into its type, including a handshake without `requestedExtensions`.
  - Unknown `type` → `PARSE_ERROR`.
  - `response` serializes `extensionPublicKey: null` and `extensionName: null` and `clientId: ""` exactly as the contract shows.
  - The error frame shape `{type:"error",code,message}`.
- [x] T010 [P] Implement `src-tauri/src/browser_bridge/protocol.rs`: `serde` types with `#[serde(tag = "type", rename_all = "camelCase")]` for handshake, handshakeResponse, request, response, authorizationUpdate, ping, pong and error, plus the `ErrorCode` enum of the contract (frame-level and request-level codes, serialized `SCREAMING_SNAKE_CASE`). Make T009 pass.
- [x] T011 [P] Tests in `src-tauri/src/browser_bridge/declaration_tests.rs`:
  - `canonical(declaration)` is independent of key order and list order.
  - `max_grant(declaration)` reads only `permissions.core.passwords`: `readWrite` + `*` → (ReadWrite, All); `read` + tags → (Read, Tags).
  - A missing or empty `passwords` → `DeclarationRequired`.
  - `database`, `bookmarks` and similar entries are kept in the canonical form but never granted (research R5).
  - `fits(access, scope_tags, declaration)` refuses anything wider than declared.
- [x] T012 [P] Implement `src-tauri/src/browser_bridge/declaration.rs` and make T011 pass.
- [x] T013 Add the variant `BrowserClient { id: String }` to `Caller` in `src-tauri/src/caller.rs`, then fix every exhaustive match:
  - `passwords/access.rs:211` `reach()` treats it like `Extension` (union of grant scopes, never trash);
  - `contacts/access.rs:140`, `contacts/service/mod.rs:52-53` and `files/access.rs:133,136,205,271,282` → `Forbidden`.

  Extend `src-tauri/src/passwords/access_tests.rs` (or its existing test file) with a case: a `BrowserClient` with a tag grant sees only that tag and never trash.

- [x] T014 [P] Tests in `src-tauri/src/prompt_park_tests.rs` (K1, research R10):
  - `park(kind) -> (PromptId, Receiver)` and `answer(id, value)` deliver once; a second answer is ignored.
  - Timeout → `Expired`.
  - Cancelling the parent `CancellationToken` → `Cancelled`.
  - `cancel_all()` resolves every waiter with `Cancelled`.
  - An unknown id → no effect.

  Use `tokio::time::pause` and no sleeps.

- [x] T015 [P] Implement `src-tauri/src/prompt_park.rs` (≤ 150 lines) and make T014 pass:
  - generic `PromptPark<A>` holding `Mutex<HashMap<PromptId, oneshot::Sender<A>>>`;
  - `ask(&self, timeout, token) -> Result<A, PromptEnd>`;
  - `answer`, `cancel_all`;
  - the pattern of `files/agent/prompt.rs:157-240` without its domain types.

  Leave `files/agent/prompt.rs` and `chat/tools/action_bridge.rs` untouched; follow-up T080 migrates them.

- [x] T016 Migration `src-tauri/src/identity/migrations_browser_clients.rs`, constant `BROWSER_CLIENTS_0038` (use the next free number on `main` at the time; rename if `0038` is taken). Columns exactly as in [data-model.md](./data-model.md):

  | Column              | Definition                                                                  |
  | ------------------- | --------------------------------------------------------------------------- |
  | `id`                | TEXT PRIMARY KEY NOT NULL                                                   |
  | `vault_device_uuid` | TEXT NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE |
  | `client_id`         | TEXT NOT NULL                                                               |
  | `public_key`        | TEXT NOT NULL                                                               |
  | `client_name`       | TEXT NOT NULL                                                               |
  | `origin`            | TEXT NOT NULL                                                               |
  | `declaration`       | TEXT NOT NULL                                                               |
  | `status`            | TEXT NOT NULL, values `allowed` or `blocked`                                |
  | `access`            | TEXT, `read`/`readWrite`, NULL exactly when `blocked`                       |
  | `scope_tags`        | TEXT, JSON list of tag names, NULL = all entries                            |
  | `default_tag`       | TEXT                                                                        |
  | `write_denied`      | INTEGER NOT NULL DEFAULT 0                                                  |
  | `decided_at`        | INTEGER NOT NULL                                                            |
  | `last_seen_at`      | INTEGER                                                                     |

  No UNIQUE constraint (it would halt the sync): `id` is `UUIDv5(NS, device ":" client_id)` (`approvals::row_id`), so one client has one row per device. Register the migration in `src-tauri/src/identity/migrations.rs`. Raise `HOLZI_TRIGGER_VERSION` 25 → 26 with a changelog line ("26: `0038_browser_clients` (spec 049) introduced the CRDT-tracked `haex_browser_clients`"). The migration tests in `identity/migrations_browser_clients_tests.rs` cover a fresh vault and an upgrade from the schema before.

- [x] T017 [P] Tests in `src-tauri/src/browser_bridge/approvals_tests.rs`:
  - Store and load by `(device, client_id)`.
  - A row matches a handshake only if `client_id`, `public_key`, `origin` and canonical `declaration` are equal (data-model, "Abgleich beim Handshake").
  - `blocked` rows have `access`, `scope_tags` and `default_tag` NULL and `write_denied` 0.
  - `readWrite` with `scope_tags` requires `default_tag` ∈ `scope_tags`.
  - Session decisions shadow nothing that is remembered as blocked ("Verweigerung schlägt Erteilung").
  - `grants_of(approval, session)` returns exactly one `Grant` (data-model, "Aufrufer und Freigabe").
- [x] T018 Implement `src-tauri/src/browser_bridge/approvals.rs` and make T017 pass:
  - remembered rows through `VaultDb` on `gate.spawn_blocking`, only rows of `current_device_uuid` (`storage/wm_session_commands.rs:80`);
  - session decisions in memory (`SessionDecisions`);
  - `decide(handshake) -> Decision { Authorized(Approval) | Blocked | Unknown | Changed }`.
- [x] T019 [P] Add `browser_bridge: bool` to `PlatformCapabilities` in `src-tauri/src/platform/mod.rs` (`desktop()` true, `ANDROID` false) and extend its existing test. Expose it as `browserBridge` in `src/composables/useDeviceCapabilities.ts`.
- [x] T020 Device preferences `browserBridge.enabled` (bool, default false) and `browserBridge.port` (u16, default 19455, valid 1024–65535) in `src-tauri/src/browser_bridge/settings.rs`, read and written with `storage/preferences.rs` `PrefScope::Device`. Write a unit test for the defaults and port validation in `settings_tests.rs`.
- [x] T021 `src-tauri/src/browser_bridge/mod.rs`: `BridgeRuntime` with state `Off | Running{port} | PortInUse{port} | NoVault | NotAvailable`, server keys, child token of `gate.token()` and the connection registry. Add:
  - `start_for_active_instance(app)`: reads the settings and starts when enabled;
  - `stop()`: cancel token, `prompt_park.cancel_all()`, clear session decisions, emit `browser-bridge-changed`.

  Call `start_for_active_instance` from all three open paths: `src-tauri/src/instances/open.rs:133-163`, `instances/create.rs:147-152` and `instances/import.rs:204-216`, next to `sync::start_for_active_instance`. Register the module in `src-tauri/src/lib.rs`.

- [x] T022 `src-tauri/src/browser_bridge/methods/mod.rs`:
  - `pub struct Method { name, need: GrantAction, handler, module: &'static str }`;
  - `METHODS: &[Method]` (empty in PR B);
  - `NOT_SUPPORTED: &[&str]` with `update-item`, `bookmarks-collections-list`, `bookmarks-collection-create`, `bookmarks-list`, `bookmarks-upsert`, `bookmarks-delete` and `bookmarks-device-upsert`;
  - `dispatch(ctx, action, target, payload) -> Reply`: target other than `__core__`/`core` → `NOT_SUPPORTED`; unknown method → `NOT_SUPPORTED`; listed unsupported → `NOT_SUPPORTED`; no prompt and no effect (FR-028, FR-029);
  - `Reply` → `{requestId, success, data|error, errorCode}` with the fixed strings of the contract (research R16).
- [x] T023 Contract test `src-tauri/tests/browser_bridge_contract.rs`, after `tests/extension_bridge_contract.rs`. It parses the method and "Nicht unterstützt" lists out of `specs/049-external-bridge/contracts/wire-protocol.md` and checks:
  - `METHODS` names equal the contract's method headings that are implemented so far (PR B: none; each later PR adds its names);
  - `NOT_SUPPORTED` equals the contract list;
  - every `module` starts with `holzi_lib::browser_bridge::` and contains none of `::chat`, `::llm`, `::adapters`, `::providers`;
  - each name appears once.

**Checkpoint**: `cargo test --lib browser_bridge`, `cargo test --lib prompt_park` and `cargo test --test browser_bridge_contract` are green. The bridge can be started and stopped but has no server yet.

---

## Phase 3: User Story 1 - Browser-Erweiterung verbinden und in holzi zulassen (P1) 🎯 MVP — PR B

**Goal**: The extension connects, holzi shows the approval dialog, the user approves with a grant and a default tag, and the client is told at once. Remembered approvals survive restarts; session ones end with the vault (FR-001–FR-004, FR-008–FR-012).

**Independent Test**: Integration test with the Rust test client (below), then quickstart M1–M2 with the real extension.

- [x] T024 [US1] Integration test first, `src-tauri/tests/browser_bridge.rs` part 1. The test client lives in this file: a `tokio-tungstenite` client that sends `Origin: chrome-extension://test`, builds X25519 keys and seals requests per the contract. The test starts a vault and the bridge on port 0. Scenarios:
  - unknown client → `handshakeResponse(false, true)` and exactly one `browser-bridge-prompt` with `kind: 'approval'` (capture the emitted events through the test app handle);
  - answer allow + remember → `authorizationUpdate(true)` on the open socket without reconnect;
  - reconnect → `handshakeResponse(true, false)` without prompt;
  - session approval → after `stop()` and restart, asks again;
  - deny + remember → reconnect gets `(false,false)` and is closed, with no prompt;
  - changed declaration (extra `bookmarks` entry) → asks again, and requests before the answer get `UNAUTHORIZED`;
  - bridge off or no vault → connection refused.
- [x] T025 [US1] `src-tauri/src/browser_bridge/server.rs` (≤ 250 lines):
  - Bind `127.0.0.1:<port>`; `AddrInUse` → `PortInUse`.
  - Bind `[::1]:<port>` too: `AddrNotAvailable` → IPv4 only; `AddrInUse` → `PortInUse` and drop the IPv4 listener (research R1).
  - Accept loop under `gate.spawn`, `select!` on the bridge token, at most 16 connections.
  - `accept_hdr_async` with `WebSocketConfig { max_message_size: 64 KiB, max_frame_size: 64 KiB }`.
  - The header callback is implemented in T037 (US6). Until then it accepts only `chrome-extension://` and `moz-extension://`.
- [x] T026 [US1] `src-tauri/src/browser_bridge/connection.rs` (≤ 400 lines), the state machine of data-model "Verbindung":
  - writer task fed by a bounded `mpsc` (capacity 32);
  - 10 s handshake deadline;
  - repeated handshakes on the same socket answer the current state without a second prompt; a different `clientId` closes;
  - `ping` → `pong` in any state;
  - requests only when `Authorized`, each in its own task under a per-connection `Semaphore(4)`, decrypted with `envelope::open_request`;
  - `requestId` required, otherwise the request is dropped (research R4);
  - replies sealed to the handshake key;
  - on close: cancel the connection's child token so its prompts and requests end.
- [x] T027 [US1] Pending approvals in `src-tauri/src/browser_bridge/pending.rs`: at most one approval prompt per fingerprint across all connections (`PendingApprovals`, FR-007). On answer, notify every waiting connection of that fingerprint (`authorizationUpdate`, or close on deny). Prompt timeout 120 s → treated as deny without remember.
- [x] T028 [US1] `src-tauri/src/browser_bridge/prompts.rs`: the bridge's prompt kinds on top of `prompt_park`.
  - `BridgePrompt` and `BridgeAnswer` with `ts-rs` exactly as in [contracts/tauri-commands.md](./contracts/tauri-commands.md); the `presence` variants are added in Phase 8.
  - Events `browser-bridge-prompt` and `browser-bridge-prompt-cancelled`.
  - Command `browser_bridge_answer(prompt_id, answer)`. It validates an approval answer like `browser_bridge_client_update` (T066 reuses the same validator in `approvals.rs`); an invalid answer counts as deny.
- [x] T029 [US1] `src-tauri/src/browser_bridge/commands.rs`:
  - `browser_bridge_overview` (read-only part: `enabled`, `port`, `state`, empty lists until Phase 7);
  - `browser_bridge_configure { enabled, port }` (validates the port, saves both, starts, stops or restarts, emits `browser-bridge-changed`);
  - `browser_bridge_answer`.

  On Android every command returns `NotAvailable`. Register them in `src-tauri/src/lib.rs` and `capabilities` as the other commands are.

- [x] T030 [P] [US1] `src/lib/actions/browserBridgeActions.ts` (new): `settings.browserBridge.set` with scope `guardrails`, so not agent-callable (FR-013), input `{ enabled, port }`, calling `browser_bridge_configure`. Spread it into the settings catalog next to `SETTINGS_ACTIONS` without adding lines to `settingsActions.ts`. Extend `scripts/check-agent-actions.ts` (or its existing guardrails case) so the new id is asserted non-agent-callable.
- [x] T031 [P] [US1] `src/stores/browserBridge.ts` (new):
  - listens to `browser-bridge-prompt`, `-cancelled` and `-changed`;
  - keeps a FIFO of prompts and shows one at a time;
  - before showing, calls `raiseWindowAsync()` (`src/composables/useExtensionHost.ts:53-66`); if the window is still not focused, sends one system notification through `tauri-plugin-notification` that opens the window;
  - `answerAsync(promptId, answer)`.
- [x] T032 [US1] `src/components/browserBridge/ApprovalDialog.vue` (new, ≤ 300 lines), using haex-ui `ShadcnDialog`, `OptionRow`, tag picker of the password manager. Content:
  - the client name marked as the client's own claim ("Angabe des Clients", "ohne Namen" if empty);
  - the fingerprint in groups of four;
  - origin;
  - the declaration in words;
  - Art (Lesen / Lesen und Schreiben, at most declared, default as declared);
  - Bereich (alle Einträge / Einträge mit Tags);
  - Standard-Tag (required for readWrite with tags, must be in scope);
  - "Erlaubnis merken" (default off);
  - "Zulassen" and "Ablehnen" buttons with `type="button"`.

  Usable at 360 px without horizontal scroll (FR-044). Test ids `bridge-approval-*`. Mount it in `src/components/wm/Desktop.vue` next to `PermissionRequestDialog`.

- [x] T033 [US1] Settings page skeleton `src/components/settings/BrowserBridgeSettings.vue` (new), registered as section "Browser-Erweiterungen" with `subView('extensions.browserBridge', 'extensions/browser-bridge', 'extensions')` in `src/lib/settings/registry.ts` and the views map in `src/components/wm/appRoutes.ts`. It shows the on/off switch (saves on change, no save button), the port field and the state (`off`, `running`, `portInUse` with a hint about another holzi or haex-vault 2.x, `noVault`). On Android it shows `NotOnThisDevice.vue`. Update `scripts/check-settings.ts` for the new location.
- [x] T034 [P] [US1] i18n: `browserBridge.*` keys in `src/i18n/locales/de.json` and `en.json` for the dialog, settings, states and errors. No `<…>` in messages (vue-i18n).
- [x] T035 [US1] `src-tauri/src/bin/e2e_bridge_client.rs` (feature `e2e`, after `bin/e2e_nostr_relay.rs`): a CLI that connects with `Origin: chrome-extension://e2e`, performs the handshake with a fixed test key, waits for `authorized`, then sends one request given on the command line and prints the decrypted reply as JSON. Reuse the client code by moving the T024 client into `src-tauri/tests/support/bridge_client.rs` only if the e2e binary needs the same code. Otherwise keep both small and local (constitution: test helpers local).
- [x] T036 [US1] e2e scenario `scripts/e2e/scenarios/browser-bridge.test.ts` part 1:
  1. Enable the bridge in Settings → Erweiterungen → Browser-Erweiterungen (port from the e2e port helper, not 19455).
  2. Run `e2e_bridge_client`.
  3. The approval dialog shows name, fingerprint and origin.
  4. Approve with tag scope and default tag, remember.
  5. The client prints authorized.
  6. Run the client again: no dialog.

**As built (PR B)**: the shared state of a running bridge (keys, session decisions, connections per client, the one approval question per client of T027) is `browser_bridge/runtime.rs` (`BridgeCore`); `browser_bridge/mod.rs` holds the Tauri state `BrowserBridge` and `start_for_active_instance`; `server.rs` binds and accepts. The prompt event carries the question as `question` (`BridgePromptEvent`, contracts updated). The window raises itself and falls back to the command `browser_bridge_notify` with its own text (Rust holds no text).

**Checkpoint**: US1 works with the test client. Run quickstart M1 and M2 with real Chromium and Firefox before merging PR B.

---

## Phase 4: User Story 6 - Webseiten und fremde Programme kommen nicht durch (P1) — PR B

**Goal**: Web pages, wrong ids, v1 handshakes, oversized messages and handshake floods never reach a dialog or data (FR-005–FR-007, SC-006).

**Independent Test**: `cargo test --test browser_bridge` part 2.

- [x] T037 [US6] Tests first in `src-tauri/src/browser_bridge/origin_tests.rs`. `accepts_origin(header)` is:
  - true for `chrome-extension://abc` and `moz-extension://8c2…`;
  - false for `https://evil.example`, `http://localhost`, `null`, an empty string, a missing header, `chrome-extension:` without host, mixed case `Chrome-Extension://` and `file://`.

  Implement `src-tauri/src/browser_bridge/origin.rs` and use it in the `accept_hdr_async` callback of T025, which answers HTTP 403 with an empty body.

- [x] T038 [US6] `src-tauri/tests/browser_bridge.rs` part 2 (SC-006):
  - origins `https://…`, `http://…`, `null` and none → upgrade refused, no prompt event;
  - handshake `version: 1` → `error PERMISSIONS_DECLARATION_REQUIRED`, then `handshakeResponse(false,false)`, then close;
  - handshake without `permissions.core.passwords` → same;
  - `clientId` not matching the key → `(false,false)`, close, no prompt;
  - a 65 KiB message → connection closed, process alive;
  - 100 handshakes in a row from the same unknown client on fresh sockets → exactly one prompt event;
  - 20 parallel sockets → the 17th is closed;
  - no handshake within 10 s → closed (use paused time or an injected deadline, no sleep);
  - a binary frame → closed.
- [x] T039 [US6] Handshake checks in `connection.rs`:
  - `version < 2` or `declaration::max_grant` fails → `PERMISSIONS_DECLARATION_REQUIRED` then close;
  - `envelope::client_id_of(publicKey) != clientId` → `(false,false)` then close;
  - a remembered or session block → `(false,false)` then close.

  Make T038 pass.

**Checkpoint**: PR B complete. Run `cargo test --lib browser_bridge`, `--test browser_bridge`, `--test browser_bridge_contract`, `--test vault_upgrade`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, the e2e scenario and quickstart M1, M2 and M8 (web page part). Then open PR B.

---

## Phase 5: User Story 2 - Anmeldedaten im Browser ausfüllen, auch TOTP (P1) — PR C

**Goal**: `get-items` returns matching entries in scope with resolved fields; `get-totp` returns the current code (FR-019–FR-021, FR-024, FR-042).

**Independent Test**: The access matrix in `src-tauri/tests/browser_bridge.rs` part 3, then quickstart M3.

- [x] T040 [US2] New worktree `.worktrees/049-c` on branch `049-external-bridge-c` from `main` after PR B, with a real `pnpm install`.
- [x] T041 [P] [US2] Tests first in `src-tauri/src/passwords/site_tests.rs`. `site_of(host)`:
  - `www.example.de` and `login.example.de` → `example.de`;
  - `bad-example.de` → `bad-example.de`;
  - `example.co.uk` and `a.example.co.uk` → `example.co.uk`;
  - `co.uk` → None;
  - `127.0.0.1`, `[::1]` and `localhost` → the host itself;
  - `foo.localhost` → `foo.localhost`.

  `is_public_suffix("co.uk")` is true.

- [x] T042 [US2] K2: create `src-tauri/src/passwords/site.rs` with `site_of` (`psl::domain_str`, IP/localhost rule) and `is_public_suffix`. Move the suffix logic of `webauthn.rs:45-63` (`relying_party`) there and rewrite `checked_origin` to call it. `webauthn_tests` and `tests/passwords_passkeys.rs` stay green unchanged. Make T041 pass.
- [x] T043 [US2] K4: change `PasswordsService::totp_code` in `src-tauri/src/passwords/service/items.rs:196` to `totp_code(&caller, &grants, item_id)`, using `authorize_read` instead of `require_user`. An item without TOTP returns the same `PasswordsNotFound` as one out of scope (FR-024, FR-042). Update every call site; the user commands pass `&[]`. Add a test to the existing service test file: a `BrowserClient` with a tag grant gets a code for an entry in scope and `PasswordsNotFound` for one outside and for one without TOTP.
- [x] T044 [US2] Integration test first, `src-tauri/tests/browser_bridge.rs` part 3 (SC-002, SC-003). Fixture vault:
  - `https://example.de` [web];
  - `https://www.example.de/login` [web, TOTP];
  - `https://bad-example.de` [web];
  - `https://example.de` without tag;
  - `https://example.de` [web] in trash;
  - an entry of a holzi feature (034 Z14);
  - `https://example.de` [web] whose username references an entry without tag.

  Client granted `Read` on tag `web`. Assertions:
  - `get-items` for `https://login.example.de/x` returns exactly the two `web` entries plus the referencing one, without its username field. Each has id, title, url, `fields` with string values only, `otp: "TOTP"` on the TOTP entry, `hasTotp`, `autofillAliases`, and no key `otpSecret`, `notes`, `attachments` or `passkeys` anywhere in the JSON.
  - `fields: ["otp"]` → only the TOTP entry.
  - Missing `url` → `INVALID_REQUEST`.
  - `get-totp` → `{code, validFor}` with a 6-digit code.
  - `get-totp` for the untagged entry or a non-TOTP entry → `NOT_FOUND` with the same error string.
  - A vault with 5 000 entries and 20 matches answers `get-items` in under 500 ms in release-free debug as a rough bound. Mark it `#[ignore]` if CI is slower and note it in the test.

- [x] T045 [US2] `src-tauri/src/browser_bridge/methods/items.rs`: `get-items` per research R7.
  1. Parse the URL with `url`, adding `https://` when there is no scheme.
  2. `list_headers(&caller, &grants)`.
  3. Filter on `site_of(host)` and `fields`.
  4. `read_secret_item` per hit.
  5. Build each entry from selected fields only; never serialize `SecretItem`.

  Add a `ponytail:` comment on filtering in the bridge (upgrade path: a service method when a second caller needs it). Register it in `METHODS` with `need: Read` and `module: MODULE`.

- [x] T046 [US2] `get-totp` in the same file, `need: Read`, `{ entryId }` → `{ code, validFor }`. Map service errors to `NOT_FOUND`, `PERMISSION_DENIED` or `SERVER_ERROR` in `methods/mod.rs` (research R16). Make T044 pass. Add `get-items` and `get-totp` to the expected list in `tests/browser_bridge_contract.rs`.

**Checkpoint**: Run quickstart M3 with the real extension.

---

## Phase 6: User Story 3 - Neue Zugangsdaten aus dem Browser speichern (P2) — PR C

**Goal**: Generator presets for every grant; `create-item` with the default tag; a write prompt when the grant is read-only (FR-015–FR-017, FR-022, FR-025, FR-027, FR-028).

**Independent Test**: `src-tauri/tests/browser_bridge.rs` part 4, then quickstart M4 and M5.

- [x] T047 [US3] K4: change `preset_list` in `src-tauri/src/passwords/service/presets.rs:10` to `preset_list(&caller, &grants)`, allowing any caller with at least one password grant (FR-027). Update the user call sites to pass `&[]`. Add a service test: `BrowserClient` with a `Read` grant lists presets; with no grant → `PasswordsForbidden`.
- [x] T048 [US3] Integration test first, `src-tauri/tests/browser_bridge.rs` part 4:
  - `get-password-config` → `{config, presetName}` with the keys `length, uppercase, lowercase, numbers, symbols, excludeChars, usePattern, pattern`; `{config:null, presetName:null}` without presets.
  - `get-password-presets` → list with `isDefault`.
  - `ReadWrite` + tag `web` + default `web`: `create-item {title:"", url:"https://shop.de/x", username, password, groupId:"g", otpSecret:"x", iconBase64:"…", tags:["other"]}` → `{entryId, title:"shop.de"}`. The entry has exactly tag `web`, top level, one history state, no TOTP and no icon.
  - `update-item` → `NOT_SUPPORTED`, nothing changed.
  - Every `bookmarks-*` → `NOT_SUPPORTED`, connection still open.
  - Target `{extensionPublicKey:"x", extensionName:"y"}` → `NOT_SUPPORTED` (SC-009).
  - Read-only client: `create-item` → one `kind:'write'` prompt; deny → `PERMISSION_DENIED`, no entry; allow+remember → entry created and `access` stored as `readWrite`; a declaration without write → `PERMISSION_DENIED` without prompt; remembered write deny → `PERMISSION_DENIED` without prompt; no answer in 120 s → `PERMISSION_PROMPT_TIMEOUT` (paused time).
- [x] T049 [US3] `create-item`, `get-password-config` and `get-password-presets` in `src-tauri/src/browser_bridge/methods/items.rs` (or `methods/presets.rs` if `items.rs` passes 400 lines):
  - The `create-item` input struct has exactly `title`, `url`, `username`, `password`; serde ignores everything else.
  - Title fallback is the URL host, then `Untitled`.
  - `ItemInput.tags = [default_tag]` (empty without one), `group_id: None`.
  - Map `Preset` to `config` 1:1 (`model.rs:569`).
- [x] T050 [US3] The write prompt in `src-tauri/src/browser_bridge/methods/mod.rs`. When `need == ReadWrite` and the grant is `Read`:
  - if the declaration has no write or `write_denied`/session deny → `PERMISSION_DENIED`;
  - else ask `kind:'write'` through `prompts.rs` (120 s);
  - allow + remember → update the row (`access = readWrite`, `default_tag` from the answer, required when the scope is tags);
  - allow without remember → session extension;
  - deny + remember → `write_denied = 1`;
  - timeout → `PERMISSION_PROMPT_TIMEOUT`.

  No prompt for entries out of scope (FR-016): the check happens before any item lookup.

- [x] T051 [P] [US3] `src/components/browserBridge/WriteAccessDialog.vue` (new): client, method in words, "Erlauben", "Ablehnen", "Merken"; a default-tag picker when the scope is tags and the method creates entries (FR-015). Buttons have `type="button"`. Mount it in `Desktop.vue`; the store from T031 routes `kind:'write'`. Add i18n keys.
- [x] T052 [US3] Make T048 pass. Add `create-item`, `get-password-config` and `get-password-presets` to the contract test's expected list. Extend the e2e scenario: a read-only approval, `e2e_bridge_client create-item` → write dialog → allow → the entry appears in the password manager without reload (FR-025).

**As built (PR C)**: the write question lives in `browser_bridge/write_access.rs`; this session's write answers are kept in `SessionDecisions` (`allow_write`, `deny_write`, `effective`). The fixture and the test client moved to `tests/common/bridge_fixture.rs`; the password cases are in `tests/browser_bridge_passwords.rs`, and the 5 000-entry test runs every time (about 10 s, mostly seeding). `PasswordsService::totp_code` and `preset_list` take the grants; the service shares `authorize_item_read` between the single read and the TOTP code.

**Checkpoint**: PR C complete. Run quickstart M4 and M5, then open PR C.

---

## Phase 7: User Story 5 - Zugelassene Clients verwalten, ändern und widerrufen (P2) — PR D

**Goal**: The settings page lists approved and blocked clients and lets the user change, revoke and unblock them, with immediate effect on open connections (FR-037, FR-038, SC-007).

**Independent Test**: `src-tauri/tests/browser_bridge.rs` part 5, the e2e scenario part 2, quickstart M8 and M9.

- [x] T053 [US5] New worktree `.worktrees/049-d` on branch `049-external-bridge-d` from `main` after PR C.
- [x] T054 [US5] Integration test first, `src-tauri/tests/browser_bridge.rs` part 5:
  - The overview lists remembered and session clients and blocks with all `BridgeClientView` fields and `connected`.
  - Revoke on an open connection → `authorizationUpdate(false)` arrives in under 1 s, the socket closes, a pending write prompt is cancelled, and the next connect asks again.
  - Update `access` to `read` → the next `create-item` asks; update `scope_tags` → the next `get-items` follows it.
  - An update wider than the declaration → `InvalidInput`.
  - Unblock → the next connect asks.
  - Port change while running → the old port is free (bind succeeds) and the new one accepts.
  - `stop()` → all sockets closed and the port free in under 2 s (SC-008).
  - `last_seen_at` is set on an authorized handshake.
- [x] T055 [US5] `src-tauri/src/browser_bridge/admin.rs`:
  - `overview()`;
  - `update(client_id, access, scope_tags, default_tag)`, validated by the validator from T028 against the stored declaration;
  - `revoke(client_id)`: delete the row or the session approval, push `authorizationUpdate(false)` to every connection of the fingerprint, then close them, which cancels their prompts and requests;
  - `unblock(client_id)`.

  Commands `browser_bridge_client_update`, `browser_bridge_client_revoke` and `browser_bridge_client_unblock` in `commands.rs`. Emit `browser-bridge-changed` on every change and on connect/disconnect. Make T054 pass.

- [x] T056 [P] [US5] Add the actions `settings.browserBridge.client.update`, `.client.revoke` and `.client.unblock` to `src/lib/actions/browserBridgeActions.ts`, scope `guardrails`. Extend the agent-actions check.
- [x] T057 [US5] `src/components/browserBridge/ClientRow.vue` and `ClientEditor.vue` (new). Fill `BrowserBridgeSettings.vue` with the lists:
  - Each allowed client shows name (marked as claim), fingerprint, origin, Art, Bereich, Standard-Tag, gemerkt/vorläufig, decided and last seen (relative time) and a connected dot.
  - Blocked clients are in their own group.
  - The editor has the same fields as the approval dialog. It saves on selection with no save button (memory: settings save on selection). Revoke and unblock ask for confirmation.
  - "Port belegt" offers changing the port.
  - Usable at 360 px.
- [x] T058 [US5] e2e scenario part 2: the approved test client is listed; narrow it to "Lesen"; revoke while `e2e_bridge_client --hold` is connected → it prints `authorized:false` and exits; unblock flow.

**As built (PR D)**: the management lives in `browser_bridge/admin.rs` (`ClientAdmin` with `overview`, `update`, `revoke`, `unblock`); it works with and without a running bridge, which `BrowserBridge::core()` hands over. The cases of T054 are in their own file, `tests/browser_bridge_admin.rs`, to keep `browser_bridge.rs` small; stopping and freeing the port were already covered in part 1. `BridgeClientView` also carries `declared` and `declaresWrite`, so the editor offers nothing beyond the declaration. An update forgets this session's answers about writing (`SessionDecisions::reset_write`). The grant fields are shared by the approval question and the editor (`components/browserBridge/GrantFields.vue`, `lib/browserBridgeGrant.ts`). A client that is neither approved nor blocked gives the new error `BrowserBridgeClientNotFound`. The action schema has no `null`, so `scopeTags` and `defaultTag` are left out instead.

**Checkpoint**: PR D complete. Run quickstart M8, M9 and M11, then open PR D.

---

## Phase 8: User Story 4 - Mit Passkey anmelden und Passkey anlegen (P2) — PR E

**Goal**: `passkey-create`, `passkey-get` and `passkey-list` through the passkey service, with a presence confirmation per ceremony; UP after confirming, UV only with the vault password (FR-030–FR-036, SC-005).

**Independent Test**: `src-tauri/tests/passwords_passkeys.rs` (extended), `browser_bridge.rs` part 6, then quickstart M6 and M7 (needs PR H).

- [x] T059 [US4] New worktree `.worktrees/049-e` on branch `049-external-bridge-e` from `main` after PR D (PR E only needs PR B and C; branch from whatever is merged).
- [x] T060 [US4] Write `docs/adr/0015-passkey-presence-after-ceremony-bound-confirmation.md` in the format of `0009-…md`:
  - The passkey service sets UP only after a confirmation bound to exactly one request (client, RP, origin, challenge hash), and UV only when the RP requires it and the vault password was re-entered in that confirmation.
  - `UserPresence` cannot be deserialized from outside.
  - It supersedes ADR-0009 on this point.

  Add a "Superseded in part by ADR-0015" line to ADR-0009. Add a note to `specs/036-password-redesign/contracts/passkey-service.md` (UP/UV section) pointing to 049 and ADR-0015.

- [x] T061 [US4] Tests first, extend `src-tauri/tests/passwords_passkeys.rs`:
  - `presence: None` keeps today's flags (`BE|BS|AT`, no UP).
  - `Present` → UP set, UV not.
  - `Verified` → UP and UV.
  - For create and confirm, check the flags in `authData` and the signature over `authData ‖ SHA-256(clientDataJSON)` with `p256`.
  - `PasskeyTarget::New { title, url, username, tags }` creates entry and passkey in one transaction; a forced passkey failure (excluded credential) leaves no entry.
  - `Existing(id)` behaves like the old `item_id`.
  - Also extend `webauthn_tests.rs` for the flag bytes.
- [x] T062 [US4] Implement the service changes:
  - `UserPresence` (`None` default, `Present`, `Verified`; derives `Debug, Clone, Copy, PartialEq` but **not** `Deserialize`) and `PasskeyTarget` in `src-tauri/src/passwords/model_passkeys.rs`.
  - `presence` on `PasskeyCreateRequest` and `PasskeyConfirmRequest`; `item_id` → `target` on create.
  - `create_auth_data`/`get_auth_data` in `webauthn.rs:287,304` take the presence and set `FLAG_UP`/`FLAG_UV`.
  - `PasskeyTarget::New` in the new `src-tauri/src/passwords/passkeys_new_entry.rs` (create item through the items layer with history, then the passkey, same transaction).
  - `passkeys_ops.rs` only dispatches on the target.
  - Update all existing callers and tests.

  Make T061 pass.

- [x] T063 [US4] K3: make `verify_current` in `src-tauri/src/instances/passphrase_change.rs:75` `pub(crate)` and unchanged. Add a test in its existing test file (or a new `passphrase_change_tests.rs`): right passphrase → Ok, wrong → `WrongPassphrase`.
- [x] T064 [US4] Integration test first, `src-tauri/tests/browser_bridge.rs` part 6 (SC-005 without a browser). Client `ReadWrite`, tag `web`, default `web`.
  - **`passkey-create`**:
    - It contains `origin: "https://login.example.de"`, `relyingPartyId: "example.de"`, std-base64 `challenge` and `userHandle`, and an extra page-injected `itemId`, which must be ignored.
    - It raises one `kind:'presence'` prompt with `choices` = new entry plus `web` entries of site `example.de`.
    - Confirm `newEntry` → the response has `credentialId`, `publicKey`, `attestationObject` (CBOR `fmt:"none"`, `authData` with UP, no UV), `clientDataJson` (decoded: `type: "webauthn.create"`, base64url challenge, the given origin), `transports:["internal"]` and `passkeyId`. A new entry titled with the RP name and tagged `web` exists.
  - **`userVerification:"required"`**: the prompt has `requireUv`; the right passphrase → UV set; a wrong one → `USER_CANCELLED` and nothing created.
  - **`passkey-get`**:
    - With two passkeys, it prompts with both.
    - Choosing one → the signature verifies with the stored public key and the flags are UP|BE|BS. `last_used_at` is set and the counter stays 0.
    - `userHandle` is present for discoverable passkeys.
  - **`passkey-list`**: returns the headers in scope with std-base64 `credentialId` and no key material.
  - **Without a prompt**:
    - Origin missing or mismatching → `ORIGIN_MISMATCH` and no prompt.
    - `excludeCredentials` hitting a passkey in scope → `PASSKEY_EXCLUDED` and no prompt; hitting one out of scope → no effect.
    - An RS256 passkey on get → `UNSUPPORTED_ALGORITHM`.
  - **Deny, timeout or closing the socket** during the prompt → `USER_CANCELLED` or no answer, and no passkey, no signature, no entry.
  - **Read-only grant** on create → write prompt first, then the presence prompt (US4 scenario 6).
- [x] T065 [US4] `src-tauri/src/browser_bridge/methods/passkeys.rs` per research R11, with the input structs whitelisted (FR-035):
  - std-base64 ↔ base64url conversion at the edge;
  - pre-checks in order: grant (write prompt), `checked_origin`, exclude via `passkey_list` ∩ `excludeCredentials`; for get, candidates = `passkey_list(rp)` ∩ `allowCredentials`;
  - then the presence prompt; for UV, `verify_current` on `gate.spawn_blocking`;
  - then `passkey_create` with `pub_key_cred_params: [-7]` (ES256, FR-034) or `passkey_confirm` with `allow_credentials = [chosen]` and the presence;
  - responses exactly as in the contract.

  Bind the confirmation to the request: the prompt carries the SHA-256 of the challenge, and the answer is consumed once by this call only (FR-031). Register `passkey-create` (`ReadWrite`), `passkey-get` and `passkey-list` (`Read`) in `METHODS` and the contract test. Make T064 pass.

- [x] T066 [US4] Add the `presence` variants to `BridgePrompt` and `BridgeAnswer` in `prompts.rs` per [contracts/tauri-commands.md](./contracts/tauri-commands.md). The answer validation checks that `choice` comes from the prompt's `choices` and that `passphrase` is present exactly when `requireUv`; the passphrase is held as `Passphrase` (zeroizing), never logged.
- [x] T067 [P] [US4] `src/components/browserBridge/PresenceDialog.vue` (new, ≤ 300 lines). It shows:
  - client, RP name and id, origin, user;
  - for create, the entry choice (default "Neuer Eintrag"); for get, the account choice when there are several;
  - for `requireUv`, a password field (`autocomplete="current-password"`);
  - "Bestätigen" and "Ablehnen", no "Merken";
  - a countdown to `expiresAt`.

  Usable at 360 px; i18n keys. Mount it in `Desktop.vue`.

- [x] T068 [US4] e2e scenario part 3: `e2e_bridge_client passkey-create` with a fixed challenge → presence dialog → confirm → the client prints a response; the passkey appears in the entry's Extra tab. A second run denied → `USER_CANCELLED`.

**As built (PR E)**: the worktree branches from PR D, not from `main`, since both change the e2e scenario and this file; after D was merged, its commits were rebased onto `main`. The cases of T064 are in their own file, `tests/browser_bridge_passkeys.rs`. The presence confirmation is `browser_bridge/methods/presence.rs`; the methods are `methods/passkeys.rs`. The bridge gets the vault file in `StartArgs` (`vault_path`) and checks the password with `instances::verify_current` on a blocking thread that holds the vault open. `PasskeyCreated` also carries `passkey_id`. `BridgeAnswer` is no longer `Clone` or comparable, since it may carry the vault password. A wrong vault password closes the dialog and is told to the window by the event `browser-bridge-wrong-passphrase` (a toast), because the answer command returns before Rust checks it. The presence dialog is mounted in `Prompts.vue`, next to the other two. The extension decides between holzi and the browser before holzi is asked (`webauthn-bridge.ts`: its own consent prompt, and a sign-in without a matching passkey goes to the browser), so a security key keeps working; declining holzi's confirmation cancels like a browser dialog.

**Checkpoint**: PR E complete. Run quickstart M6 and M7 with the PR H build of the extension, then open PR E.

---

## Phase 9: Companion PR in haextension (PR H, parallel to PR B–E)

**Goal**: The extension reaches holzi for passkey sign-in, names no specific vault app and treats unsupported bookmarks calmly (research R19).

- [x] T069 Worktree in `haex-space/haextension` on branch `fix/haex-pass-browser-holzi` from `origin/main` (`c8133b94` or later). Run the app's install there.
- [x] T070 [P] Test first, then the fix for the passkey-list reply. `apps/haex-pass-browser/src/background/main.ts:233-242` answers `{ success, passkeys }` while `contentScripts/webauthn-bridge.ts:141-150` reads `response.data`. Make both use the `BridgeResult` shape `{ success, data: { passkeys } }` of `background/vaultResponse.ts`. Add a vitest case: a vault reply with one passkey for the RP makes `vaultHasPasskeyFor` return true. Done as `passkeyListReply` in `contentScripts/webauthn-routing.ts`, tested in `webauthn-routing.test.ts`.
- [x] T071 [P] Name: no change in the end. #113 renamed "HaexVault" to "haex-vault"; #114 reverted it, because HaexVault is the product name holzi takes over.
- [x] T072 Bookmarks: when a `bookmarks-*` reply has `errorCode === 'NOT_SUPPORTED'`, the bookmark calls keep the marker `BOOKMARKS_NOT_SUPPORTED` (`bookmarks/messages.ts` `bookmarkErrorText`) and onboarding and options show "Deine Vault-App synchronisiert keine Lesezeichen." (`shownBookmarkError`). The 5-minute retry stays: it only runs with an active collection, which holzi never creates. Vitest `bookmarks/__tests__/messages.test.ts`.
- [x] T073 (haex-space/haextension#113 merged 2026-10-21, #114 after it; pinned to `ab4e0b0ac9a05488659836a7526a250beab9dfd9`) Run the app's `lint`, `typecheck` and `test`, then open the PR (push as `haex-space`, no agent attribution). After merge, update the client pin in `specs/049-external-bridge/spec.md` and `contracts/wire-protocol.md` to the merge SHA, in the next holzi PR or a small docs PR.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [x] T074 [P] Leak test (SC-004, research R16), `src-tauri/tests/browser_bridge.rs` part 7:
  - Install a capturing `log` logger for the test binary.
  - Plant markers in a TOTP secret, a passkey private key, an out-of-scope entry title and password, and in the request URL of `get-items`.
  - Run every method, including the failing paths.
  - Assert that no marker appears in any response plaintext, error string or captured log line.
- [x] T075 [P] Interop fixture (research R18):
  - `scripts/fixtures/browser-bridge-envelope.ts` generates `src-tauri/tests/fixtures/browser_bridge/webcrypto.json` with Node 22 WebCrypto `X25519` and `AES-GCM`, mirroring `background/crypto.ts` (test keys only).
  - A Rust test in `envelope_tests.rs` opens the fixture request and seals a response with fixed keys that equals the fixture's expected response.
  - `scripts/check-browser-bridge.ts` (`pnpm check:browser-bridge`, add it to `package.json` and to the check workflow next to `check:passwords`) decrypts that response with WebCrypto.
- [x] T076 [P] `ponytail:` comments in place: raw ECDH secret (`envelope.rs`), `last_seen_at` on every handshake (`approvals.rs`), the `get-items` filter in the bridge (`methods/items.rs`).
- [ ] T077 Run every quickstart row (M1–M11) with Chromium and Firefox, the extension built from PR H, and record the results and dates in `specs/049-external-bridge/quickstart.md`.
- [ ] T078 Update the roadmap row in `plans/README.md` and set the spec status to Implemented.
- [x] T079 Release the extension. In haextension `apps/haex-pass-browser`:
  1. `node scripts/bump-version.js minor` → 1.3.0.
  2. Commit, then push the tag `haex-pass-browser-v1.3.0`.
  3. Watch `browser-extension-release.yml`; confirm the AMO listing shows 1.3.0. Chrome store publishing is commented out in the workflow; say so if it is still off.

  Done 2026-10-23: the bump went through haextension#128, the tag `haex-pass-browser-v1.3.0` was pushed by hand on its merge commit `0055f55`, AMO signed and approved 1.3.0, and the GitHub release carries both packages. Chrome Web Store publishing is still off (no Chrome secrets). Since `dda7181` the bump script only bumps; a release is `pnpm release:haex-pass-browser <patch|minor|major>` in the haextension root, which bumps, commits, tags and pushes to `main` by itself.

- [x] T080 Follow-up refactor PR (K1): move `src-tauri/src/files/agent/prompt.rs` and `src-tauri/src/chat/tools/action_bridge.rs` onto `prompt_park.rs`, with behavior unchanged (their tests stay green). This is a separate PR after PR B. Done: both keep their public API and their own rules (the asking turn of the files question; the run lock, the early `action_unavailable` and the ignored second emitter of the action bridge); `PromptPark` gained `has_emitter` for the last two.

**As built (T074–T076, PR E)**: the leak test is its own binary, `tests/browser_bridge_leaks.rs`, because its capturing logger is process-wide. It keeps what a log could hold: holzi's own lines at every level, and every line at `Info` and above (the level of `lib.rs`); below that, tungstenite traces raw frames, whose envelope repeats the action in the clear and whose payload is sealed. Besides the markers of T074 it plants the client's own saved password, a wrong vault password, an unknown entry id, a foreign origin and an undecryptable message, and also searches the questions to the window and the notices. An unknown action name is only searched in the log, because the answer's envelope repeats it (wire protocol). The fixture generator uses fixed keys and IVs, so `pnpm check:browser-bridge` also checks that the committed file is what the generator writes. The `ponytail:` comments of T076 were already in place from PR B and C.

---

## Phase 11: Recognise the official extension (US1 addendum, FR-045, FR-046) — PR F

**Goal**: The approval dialog and the settings list say whether a client is the official extension, only calls itself so, is unknown or cannot be checked, so a fork with the original's name no longer looks like the original (research R21). Nothing about what holzi allows changes.

**Independent Test**: `browser_bridge/known_tests.rs`, `tests/browser_bridge.rs` part 8, the e2e scenario part 4, quickstart M12.

- [ ] T081 Find the official Chromium origin (needs the operator; blocks only the list entry of T084, not the rest):
  - The manifest of `apps/haex-pass-browser` has no `key`, the Chrome Web Store assigns the id on the first upload, and `CHROME_EXTENSION_ID` is not a secret of haextension today.
  - Write down the store id, or that there is none yet; in the second case `KNOWN` has no Chromium entry and Chromium shows "nicht prüfbar" (FR-046).
  - Decide whether the GitHub-release build can carry the store's public key as `key` so it gets the same id (check that the store accepts it), or whether those installs show "nennt sich so" with the hint of the edge case.
- [ ] T082 New worktree `.worktrees/049-f` on branch `049-external-bridge-f` from `main`, with a real `pnpm install`.
- [ ] T083 Tests first, `src-tauri/src/browser_bridge/known_tests.rs`, all against `recognise_in(list, …)` with a list built in the test:
  - the official origin → `Known` with the list's name, whatever name the client claims;
  - another `chrome-extension://` origin with the list's name, in other case and with spaces around it → `Lookalike`;
  - another `chrome-extension://` origin with another name → `Unknown`;
  - any `moz-extension://` origin, also with the official name → `Unverifiable`;
  - a list without a Chromium entry → every `chrome-extension://` origin is `Unverifiable`;
  - the origin is compared whole: a longer id with the official one as prefix is not `Known`.
- [ ] T084 `src-tauri/src/browser_bridge/known.rs`: `KnownExtension`, the fixed list `KNOWN` (a Chromium entry only with the id from T081), `Recognition` (`TS`-exported, camelCase tag `kind`), `recognise_in` and `recognise`. Register the module in `browser_bridge/mod.rs`.
- [ ] T085 Carry it through Rust:
  - `recognition` on `BridgeClientRef` and on `BridgeClientView` (`admin.rs`). `BridgeClientRef` is built in three places (`runtime.rs` for the approval, `write_access.rs`, `methods/presence.rs`): give it one constructor from the client's identity and use it in all three, so no question goes out without the value;
  - integration test, `tests/browser_bridge.rs` part 8 and `tests/browser_bridge_admin.rs`: the approval question and the overview carry the value for an unverifiable Chromium client; approving a client the list calls `Lookalike` behaves exactly like any other (same choices, same stored approval);
  - regenerate the bindings (`generate:ts-types`) and keep only real changes.
- [ ] T086 [P] Frontend: `src/components/browserBridge/RecognitionNote.vue` (≤ 80 lines), used in the approval dialog and in `ClientRow.vue`.
  - Neutral tone for `known`, `unknown` and `unverifiable`; the warning token only for `lookalike`, and no palette colors (`check:templates`).
  - Texts in German and English (FR-044), in words and without angle brackets (vue-i18n). `lookalike` warns that the name is no proof and adds that this is expected for an install from the GitHub release or a self-built one; `unverifiable` says Firefox gives every install its own id and advises to approve only what the user just installed.
  - Run `pnpm check:templates`, `typecheck`, `lint`, `format:check`.
- [ ] T087 e2e `browser-bridge` part 4: the test client connects with `chrome-extension://e2e` and with `moz-extension://e2e`; the dialog and the settings row show "nicht prüfbar" for both (the list has no Chromium entry in the test build), and approving works as before.
- [ ] T088 Run quickstart M12 in Firefox and a second Chromium build, record the result and date there, update `plans/README.md` if the roadmap names this, and mark FR-045/FR-046 in the spec's status.

**Checkpoint**: PR F complete. Nothing in it changes the protocol or the extension; T081 is the only open question for the operator.

---

## Phase 12: One approval per instance, proof of the key (US1, US6 addendum, FR-047 to FR-049) — PR G

**Goal**: Only the holder of an instance's private key gets through as that instance; protocol 2 is refused; an approval never carries over to another browser, profile or device (research R22, SC-011, SC-012).

**Independent Test**: `tests/browser_bridge_instances.rs`, `protocol_tests.rs`, the e2e scenario with the protocol-3 test client, quickstart M13 and M14.

- [x] T089 New worktree `.worktrees/049-g` on branch `049-external-bridge-g` from `main` after PR I is released as 1.4.0 (T106), with a real `pnpm install`.
- [x] T090 Tests first, `src-tauri/src/browser_bridge/protocol_tests.rs`: parse `proof`; `challenge` and the two new error frames serialise as in [contracts/wire-protocol.md](./contracts/wire-protocol.md); a handshake with `version: 2` is recognised as too old.
- [x] T091 Integration tests first, new file `src-tauri/tests/browser_bridge_instances.rs` (SC-011, SC-012):
  - protocol 2 → `PROTOCOL_UPDATE_REQUIRED`, closed, no question;
  - protocol 3 with the right proof → the approval question, as before;
  - an approved instance's client id, public key and origin, but no private key: no proof within the deadline, a wrong proof, a second proof, and the proof taken from another connection → `PROOF_FAILED` or closed, no question, no request answered, no row written;
  - a `request` before the proof → `UNAUTHORIZED`;
  - two instances: key A approved and remembered, key B with the same origin → asked; key A with another origin → asked;
  - a second device: the vault holds the remembered approval of device A; a bridge started for device B (register a second row in `known_devices` in the fixture) → asked for the same key and origin;
  - after the proof, a repeated handshake of the same client id on the same connection needs no new challenge.
- [x] T092 `protocol.rs`: frames `challenge` and `proof`, `ErrorCode::ProtocolUpdateRequired` and `ErrorCode::ProofFailed` with fixed messages (FR-039); the update message says to update haex-pass.
- [x] T093 `connection.rs`: the state `AwaitingProof` per research R22 and data-model "Verbindung":
  - draw 32 random bytes, seal them with `seal_response` to the handshake key, send `challenge` with `serverPublicKey`;
  - open `proof` with the bridge key and compare in constant time; keep the challenge only until it is used or the handshake deadline passes, and zero it;
  - only then call `decide`, show a question or answer requests.
- [x] T093a The outdated-extension hint (FR-049): the first refused protocol-2 handshake of a bridge run sends the event `browser-bridge-outdated-client` (once per run) that the window shows as a toast, and `BridgeOverview` carries `outdatedClientSeen: boolean` so the page "Browser-Erweiterungen" shows the hint while the bridge runs. Integration test: two protocol-2 handshakes → one notice, overview flag set; German and English text in words.
- [x] T094 `tests/common/bridge_fixture.rs` and `src-tauri/src/bin/e2e_bridge_client.rs`: the test client speaks protocol 3 and answers the challenge; add a helper that sends a handshake and returns the challenge frame, for the negative cases. Update every existing bridge test to the new helper without changing what it checks.
- [x] T095 Leak test (`tests/browser_bridge_leaks.rs`): plant the challenge bytes as a marker; they appear in no log line, no notice and no question.
- [x] T096 Run `cargo fmt`, `pnpm lint:rust` (both feature sets), Android clippy, all bridge test binaries, `check:browser-bridge`, the e2e `browser-bridge` scenario; then the spec's "As built" note, the pin of the client in `spec.md` and `contracts/wire-protocol.md` to the merge of PR I, and open PR G.

**As built (PR G)**: the proof lives in `browser_bridge/connection.rs`, state `AwaitingProof` with the challenge in `Zeroizing`, compared with `subtle::ConstantTimeEq` (`subtle` was already in the lock via aes-gcm). The connection is bound to its client only after the proof, so a failed attempt never counts as a connection of the instance or touches `last_seen_at`. The handshake deadline now covers the proof too; a proof that is late gets `PROOF_FAILED`. A repeated handshake while the challenge is out is ignored, one from another client closes. The outdated-client hint is `BridgeCore::note_outdated_client` (an `AtomicBool`, `Notice::OutdatedClient` once per run) and `BridgeOverview.outdatedClientSeen`; the window shows a toast from `Prompts.vue` and a hint on the settings page. The test fixture's `recv` answers a challenge with the key of the test client it was sealed to, so the existing tests run through the proof unchanged; the new tests in `tests/browser_bridge_instances.rs` read raw frames with `recv_frame`. A mutation that accepts any decryptable proof fails the impersonation test.

PR I (Phase 13) is haex-space/haextension#131, merged 2026-10-23, released as 1.4.0 the same day with `pnpm release:haex-pass-browser minor`; AMO signed it.

**Checkpoint**: PR G complete. Run quickstart M13 and M14 with haex-pass-browser 1.4.0 and with 1.3.0.

---

## Phase 13: Second companion PR in haextension (PR I, before PR G)

**Goal**: haex-pass-browser speaks protocol 3, shows its own fingerprint, and still works with a holzi that does not ask for a proof yet (spec, section "Änderungen an haex-pass-browser", points 7 to 9).

- [x] T097 Worktree in `haex-space/haextension` on branch `feat/haex-pass-browser-protocol-3` from `origin/main`; run the app's install there.
- [x] T098 [P] Tests first (vitest, `background/__tests__/connection.test.ts`):
  - a `challenge` sealed to the extension's key is opened and answered with a `proof` that the server key opens to the same bytes;
  - a `handshakeResponse` without a preceding `challenge` (holzi before PR G) still leads to "paired" or "pending" as before;
  - a `challenge` the extension cannot open sends no `proof` and shows an error.
- [x] T099 `background/protocol.ts` and `connection.ts`: `PROTOCOL_VERSION = 3`, the `challenge` and `proof` messages, using `decryptMessageEnvelope` and the request sealing that exist.
- [x] T100 [P] Options page: show the instance's fingerprint (the `clientId`) in groups of four like holzi's dialog, with the sentence that it must match the one holzi shows. German and English.
- [x] T101 [P] Texts for `PROTOCOL_UPDATE_REQUIRED` and `PROOF_FAILED` in German and English instead of the raw code.
- [x] T102 Run the app's `lint`, `typecheck` and `test`; open the PR (push as `haex-space`, no agent attribution).
- [x] T103 After merge, release with `pnpm release:haex-pass-browser minor` from the haextension root (1.4.0); watch `browser-extension-release.yml` and confirm AMO signed 1.4.0. Only then start PR G (T089).

---

## Dependencies & Execution Order

- **Phase 1** (PR A) → **Phase 2** blocks everything in PR B.
- **US1** (Phase 3) and **US6** (Phase 4) ship together in PR B; US6's origin callback T037 replaces the stub in T025.
- **US2** and **US3** (PR C) need PR B. US3's write prompt T050 needs `prompts.rs` (T028).
- **US5** (PR D) needs PR B; the editor validation reuses T028's validator. It can start in parallel with PR C on a separate branch, but merges after C because both touch `methods/mod.rs`.
- **US4** (PR E) needs PR B and the method dispatch of PR C (T045/T049 patterns, the write prompt T050).
- **PR H** is independent of holzi and can start now. Quickstart M6/M7 and the release T079 need it merged.
- **PR F** (Phase 11) needs PR B (dialog, `BridgeClientRef`) and PR D (settings rows); it can ship any time after both. T081 only decides the list entry.
- **PR I** (Phase 13) is released as 1.4.0 before **PR G** (Phase 12) is merged: 1.4.0 also works with holzi before G, while G refuses everything below protocol 3. holzi's own release should carry G, so no holzi release ever accepts protocol 2.
- **Polish** T074–T076 can land in the PR of the code they test; T077–T079 come last. T080 comes any time after PR B.

## Parallel Opportunities

- **PR B**: T007/T008, T009/T010, T011/T012, T014/T015 and T019 are independent files. T030, T031 and T034 (frontend) run while the Rust connection work (T025–T027) is in progress.
- **PR C**: T041/T042 (site) in parallel with T043 (TOTP) and T047 (presets). T051 (dialog) in parallel with T049/T050.
- **PR E**: T060 (ADR) and T067 (dialog) in parallel with T061–T062.
- **PR H**: T070, T071 and T072 touch different files.

### Parallel Example: PR B start

```text
Task: "T007/T008 envelope.rs + envelope_tests.rs"
Task: "T009/T010 protocol.rs + protocol_tests.rs"
Task: "T011/T012 declaration.rs + declaration_tests.rs"
Task: "T014/T015 prompt_park.rs + prompt_park_tests.rs"
Task: "T019 PlatformCapabilities.browser_bridge"
```

## Implementation Strategy

- **MVP = PR B (US1 + US6)**: the bridge connects, approves safely and refuses everything else. It is off by default, so shipping it alone exposes nothing.
- **First useful release = PR C**: fill, TOTP and save. This is the point where haex-pass-browser users can switch from haex-vault 2.x to holzi.
- **Then PR D** (management) and **PR E** (passkeys, with PR H).
- **Release**: the extension release (T079) comes after PR H and the quickstart, so the store build matches what holzi was tested against.
