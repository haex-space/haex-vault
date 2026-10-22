---
description: 'Task list for spec 057-extension-peer-channels'
---

# Tasks: Kanäle zwischen Erweiterungen auf verschiedenen Vaults

**Input**: Design documents from `/specs/057-extension-peer-channels/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/bridge.md](./contracts/bridge.md),
[contracts/wire-protocol.md](./contracts/wire-protocol.md), [contracts/sdk.md](./contracts/sdk.md),
[quickstart.md](./quickstart.md),
[ADR 0016](../../docs/adr/0016-non-contact-devices-for-public-lobbies.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic, and SC-006
(no channel without accepted invitation or valid code), SC-007 (uniform answer for bad codes, also in
time) and SC-008 (no message reaches another extension) need dedicated test catalogs. Rust tests live
in `*_tests.rs` next to the file (`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline.
Frontend checks of the frame queue live in `scripts/check-extensions-bridge.ts` (`pnpm
check:extensions`). No network services and no `sleep` in `cargo test`: vaults in one process on
loopback endpoints with `RelayMode::Disabled` (`src-tauri/src/sync/endpoint_tests.rs`), clocks
injected. The relay path is checked in the E2E scene, which has an iroh relay
(`scripts/e2e/lib/iroh-relay.ts`). Build only the affected targets with `-j 4`; ask the operator
before a full workspace build.

**Commands**: Rust checks run as `nix develop --command scripts/with-nix-host-bridge.sh cargo …
--manifest-path src-tauri/Cargo.toml -j 4`; frontend and formatting as `nix develop --command pnpm …`.

**Graphify**: Before naming any new function, type, module, component or command, run a bounded
`graphify query "<question>" --budget 1200` in the worktree (refresh with `graphify update .` first
when `graphify-out/.meta.json` does not match the fork point) and prefer extending a match; record the
candidates checked in the PR description. research.md R18 lists the starting set.

**Organization**: Grouped by user story in priority order (P1: US1, US2, US3; P2: US4; P3: US5).
Phase 8 wires every story to the bridge and carries the story label of the method it adds. Phase 9 is
the separate change in `haex-space/vault-sdk`.

**Shipping note** (plan.md, Lieferungen): each PR is reviewed and merged before the next one starts on
top of `main`.

| PR  | Phases and tasks                                                     | Content                                                                 |
| --- | -------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| 1   | Phase 1, Phase 2, Phase 3 (US2), Phase 4 (US3, host side): T001–T032 | permission kind, ALPN, verification, channel, lifecycle, revoke watcher |
| 2   | Phase 6 (US4), Phase 7 (US5): T046–T057                              | lobbies, join codes, admission of non-contacts, rate limiting, ADR 0016 |
| 3   | Phase 5 (US1): T033–T045                                             | invitations, holzi's invitation prompt, opening the extension           |
| 4   | Phase 8, Phase 10: T058–T070, T080–T087                              | bridge methods and events, frame queue, texts, contract, fixture, E2E   |
| SDK | Phase 9: T071–T079 (in `haex-space/vault-sdk`)                       | `client.peers`; may start after PR 1 against contracts/sdk.md           |

Task ids follow the document (stories in priority order); the PRs are merged in the order 1, 2, 3, 4,
so Phase 6 and 7 (PR 2) are done before Phase 5 (PR 3).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US5)

---

## Phase 1: Setup (PR 1)

- [x] T001 Spec, clarifications, plan, research, data model, contracts, quickstart and ADR 0016 in `specs/057-extension-peer-channels/` and `docs/adr/0016-non-contact-devices-for-public-lobbies.md`
- [X] T002 [P] Row for 057 in `plans/README.md` (status "Spezifiziert und geplant 2026-10-22 (PR 1–4 + SDK)", needs 017, 024 and 051 in use; first user haex-doom)
- [X] T003 Prepare the worktree for code: `nix develop --command pnpm install --frozen-lockfile` (done for the docs; repeat after a rebase), check `df -h`, then `nix develop --command scripts/with-nix-host-bridge.sh cargo check -j 4 --manifest-path src-tauri/Cargo.toml`
- [ ] T004 [P] Graphify consultation before the first new name: bounded queries for "protocol handler on the sync router", "verify a device of another vault", "per-extension state cleaned up when the last frame closes", "notification with buttons answered in rust", "rate limit per remote endpoint"; note the candidates in the PR description (plan.md Constitution Check lists the starting set)
- [X] T005 [P] Glossary in `CONTEXT.md` under "Erweiterungen (spec 017)": **Kanal / Lobby / Beitrittscode / Einladungsabfrage / Peer-Berechtigung** (spec 057), with _Avoid_: "Peer" for a contact, "Space" or "Raum" for a lobby, "Session" for a channel

---

## Phase 2: Foundational (PR 1, no visible change)

**Purpose**: The permission kind, the ALPN on the session endpoint, the verification of foreign
devices, the peers module skeleton and the multi-vault test support. Every story builds on these.

### Permission kind

- [ ] T006 `PermissionKind::Peers` (stored `"peers"`, not device-scoped) and `Action::Connect` (stored `"connect"`, covers only itself) in `src-tauri/src/extensions/permissions/model.rs`; extend `PermissionKind::ALL`, `parse`, `as_str`, `Action::parse` (only `(Peers, "connect")`), `as_string`
- [ ] T007 [P] Manifest category `"peers"` → `PermissionKind::Peers` with default action `connect` in `src-tauri/src/extensions/permissions/manifest_map.rs`; target `"*"` only for `Peers` in `src-tauri/src/extensions/permissions/target.rs`
- [ ] T008 [P] Tests in `src-tauri/src/extensions/permissions/model_tests.rs`, `manifest_map_tests.rs` and `target_tests.rs`: `peers`/`connect`/`*` round trip; any other action or target of `peers` is absent (FR-022 of 017); vault-wide scope (FR-001)

### Endpoint

- [ ] T009 `EXT_PEER_ALPN = b"holzi-ext-peer/1"` next to `SYNC_ALPN` in `src-tauri/src/sync/wire.rs`; `Domain::ExtPeerAuth` with tag `holzi-ext-peer-auth/v1` in `src-tauri/src/sync/signing.rs`; test in `src-tauri/src/sync/signing_tests.rs` that a `DeviceAuth` signature does not verify as `ExtPeerAuth`
- [ ] T010 Extra protocol handlers in a new `src-tauri/src/sync/endpoint_protocols.rs`: a list of `(alpn, handler)` that `SyncNode::bind` (`src-tauri/src/sync/endpoint.rs:167`) registers on its `Router` beside `SYNC_ALPN`, and `SyncNode::dial_alpn(addr, alpn) -> Result<Connection, NodeError>` after the pattern of `dial_link` (adds the address to the `MemoryLookup`); `endpoint.rs` grows only by the hand-over (documented 500-LoC exception)
- [ ] T011 [P] Tests in `src-tauri/src/sync/endpoint_protocols_tests.rs`: a dummy handler on a second ALPN accepts a dial; `holzi-sync/1` still works beside it; an ALPN nobody registered is refused
- [ ] T012 Build the `holzi-ext-peer/1` handler in `src-tauri/src/sync/service.rs` (`run`, before `SyncNode::bind`) from the app's `ExtensionHost`, `VaultDb`, window emitter and the session tracker; clear `ExtensionHost.peers` when the session's `CancellationToken` fires

### Verifying a foreign device

- [ ] T013 Contacts lookup by vault in `src-tauri/src/contacts/service/`: `ContactsService::card_of_vault(caller, vault) -> Result<Option<(contact_id, SignedCard)>>` allowed only for `Caller::Internal { feature: "extensionPeers" }` (and `"fileShares"` for spec 056, same method as 056 T016 — whichever lands first builds it), wrapping `card::received::stored` and returning `None` for a blocked vault (`src-tauri/src/contacts/card/received.rs:102`, `:165`); test in the matching `*_tests.rs` (other callers refused, blocked vault yields `None`)
- [ ] T014 Shared verification in a new `src-tauri/src/sync/foreign_auth.rs`: `Hello { v, vault, device_pubkey, list_payload, list_sig, nonce }`, `Proof { v, sig }`; `dial(…)` and `accept(…)` over the first bi-stream returning `Verified { device_pubkey, vault, trust: OwnVault | Contact { contact_id, card_name } | Unverified, nonces }`; transcript `lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(own endpoint) ‖ lp(other endpoint) ‖ lp(vault) ‖ lp(alpn)` under a caller-given `Domain`; list via `device_list::check_pushed` (`src-tauri/src/sync/device_list.rs:371`), own vault via `valid_lists` + `effective`, contact via T013 with generation ≥ `card.list_generation` and ≥ the in-memory `SeenGenerations` (`ponytail:` comment: memory only, upgrade to 056's `haex_file_peer_lists_no_sync`); device listed, not removed, `endpoint_id == connection.remote_id()`; frames through `sync::wire::write_value`/`read_value` with `HANDSHAKE_FRAME_LIMIT`; whole handshake within 5 s
- [ ] T015 [P] Test catalog in `src-tauri/src/sync/foreign_auth_tests.rs` with `Member::genesis()` (`src-tauri/src/sync/test_support.rs:110`) over in-memory streams: own linked device → `OwnVault`; vault with stored card → `Contact`; blocked vault → refused; vault without card → `Unverified`; device not on its list, removed, endpoint mismatch, bad signature, generation below the card's or below one seen → refused; a proof signed for another ALPN or domain does not verify

### Peers module skeleton

- [ ] T016 Module `src-tauri/src/extensions/peers/mod.rs` registered in `src-tauri/src/extensions/mod.rs`: limits exactly as research.md R14 ("Inhalt einer Einladung, eines Beitritts: 4 KiB", "Anzeigename beim Beitritt: 1–64 Zeichen", "schnelle Nachricht: 1.024 Byte", "gesicherte Nachricht: 64 KiB", "Kanäle je Erweiterung und Gerät: 16", "offene Lobbys je Erweiterung und Gerät: 4", "offene ausgehende Einladungen je Erweiterung: 16", "Ablauf einer Einladung: 5 min", "Zulassung nach dem Handshake: 5 s", "`extension_peers_join` bis Antwort: 15 s"); `ExtensionRef { public_key, name }` and its lookup to `extension_id` (installed, or development version with developer mode on, like `dispatch::is_enabled`); `PeerError` with the reasons of contracts/bridge.md and its mapping to `BridgeError` (codes 1001, 1002, 3001, 7000, 8001 of `src-tauri/src/extensions/error.rs`)
- [ ] T017 `PeerState` in a new `src-tauri/src/extensions/peers/state.rs` with the entities of data-model.md (`Channel`, `Lobby`, `OutgoingInvite`, `IncomingInvite`, `PendingOpen`, `Remote`); ids UUIDv4; every lookup takes the calling `extension_id` and answers a foreign id like an unknown one (FR-006); `ExtensionHost.peers` field in `src-tauri/src/extensions/host.rs`
- [ ] T018 [P] Tests in `src-tauri/src/extensions/peers/state_tests.rs`: id of another extension answers `channel_unknown`/`invite_not_open` exactly like a missing id; limits per extension enforced with `limit`
- [ ] T019 Wire messages in a new `src-tauri/src/extensions/peers/wire.rs` per contracts/wire-protocol.md (`Invite`, `Join`, `JoinReply`, `InviteReply`, `InviteConfirm`, `Withdraw`, `Reliable`, `Close`, all with `v: 1`); frame limit 64 KiB before admission, 64 KiB + 64 bytes after; test in `src-tauri/src/extensions/peers/wire_tests.rs` (round trip, oversize refused before allocation, unknown `v` refused)
- [ ] T020 Admission skeleton in a new `src-tauri/src/extensions/peers/admission.rs`: `ProtocolHandler` for `EXT_PEER_ALPN` that spawns each connection on the session tracker (like `SyncProtocol`, `src-tauri/src/sync/endpoint.rs:141-157`), runs `foreign_auth::accept`, closes `Unverified` (until PR 2), and waits at most 5 s for the first admission message, else closes (FR-032); unknown or not-yet-supported messages close
- [ ] T021 Multi-vault test support in a new `src-tauri/src/extensions/peers/test_support.rs`: two or three vaults (`Member::genesis`, linked devices via `Member::join`), cards exchanged and stored as contacts, an installed test extension (same key and name) per vault, loopback endpoints with the `holzi-ext-peer/1` handler, an injected clock, and `open_test_channel(a, b)` that runs the handshake and registers a channel without invitation or lobby (test-only)

**Checkpoint**: two vaults verify each other on `holzi-ext-peer/1`; nothing is visible yet.

---

## Phase 3: User Story 2 - Nachrichten über einen Kanal austauschen und sein Ende erfahren (Priority: P1) (PR 1)

**Goal**: An open channel carries fast and reliable messages, directly or through a relay, and both
sides learn its end with a reason.

**Independent Test**: Two vaults with `open_test_channel` exchange 35 fast messages per second per
direction plus reliable ones; one side closes, the other gets the end with its reason.

### Tests for User Story 2

- [ ] T022 [P] [US2] Channel tests in `src-tauri/src/extensions/peers/channel_tests.rs`: a fast message of 1.024 bytes arrives; 1.025 bytes → `too_large` and nothing sent; reliable messages arrive complete and in order up to 64 KiB, 64 KiB + 1 → `too_large`; `max_datagram_size() ≥ 1024` on the loopback path; explicit close → both sides `closed` exactly once; endpoint dropped → `lost`; send on a closed channel → `channel_closed`
- [ ] T023 [P] [US2] Close-reason mapping test in `src-tauri/src/extensions/peers/channel_tests.rs`: `ApplicationClosed`/`LocallyClosed` → `closed`, `TimedOut` → `timeout`, anything else → `lost` (unit test on the mapping function; the 30 s idle itself is checked in the E2E scene, T081)
- [ ] T024 [P] [US2] Load test in `src-tauri/src/extensions/peers/load_tests.rs`: one host with three channels, 35 fast messages of 1.024 bytes per second per direction for 10 s of measured (not slept) time, ≥ 99 % delivered on loopback, added delay recorded (SC-003, SC-004 baseline)

### Implementation for User Story 2

- [ ] T025 [US2] Channel in a new `src-tauri/src/extensions/peers/channel.rs`: fast send via `Connection::send_datagram` (never waits, oldest dropped when full; FR-036, FR-038) with the 1.024-byte limit; receive task on `read_datagram` emitting `peers:message { channelId, data: base64, reliable: false }` through `bridge::events::emit_to_frames` (`src-tauri/src/extensions/bridge/events.rs:17`); control stream task for `Reliable`/`Close` emitting `reliable: true`; refuse to open when `max_datagram_size()` is below 1.024; `close(reason)` sends `Close` then `connection.close(0, b"closed")`; one `peers:channel-closed { channelId, reason }` per side (FR-039, FR-040); no message content in logs (FR-042)
- [ ] T026 [US2] Register channels opened by T021 and later by lobbies and invitations in `PeerState` (`src-tauri/src/extensions/peers/state.rs`) with `origin` and `remote`, and remove them on close

**Checkpoint**: messages flow between two vaults in tests; US2 works without the bridge.

---

## Phase 4: User Story 3 - Der Nutzer erlaubt Kanäle, und Erweiterungen bleiben getrennt (Priority: P1) (PR 1, bridge side in PR 4)

**Goal**: Channels exist only for the extension that holds the permission, reach only frames of the
same extension, and end when the permission, the extension or the vault goes away.

**Independent Test**: Two test extensions A and B; only A has `peers`. A's channel never reaches B;
revoking A's permission, disabling A or closing A's last frame closes A's channels on both sides.

### Tests for User Story 3

- [ ] T027 [P] [US3] Isolation tests in `src-tauri/src/extensions/peers/isolation_tests.rs`: a channel bound to extension A delivers only to A's frames (counting emitter); an `Invite`/`Join` naming another extension (key or name differs) is never delivered (FR-004, FR-005, SC-008)
- [ ] T028 [P] [US3] Lifecycle tests in `src-tauri/src/extensions/peers/watch_tests.rs`: revoking `peers`, disabling, removing, the last frame closing and blocking the other vault each close the channels, lobbies and invitations of that extension and the other side gets `closed` (FR-008, FR-034, SC-010)

### Implementation for User Story 3

- [ ] T029 [US3] `PeerState::end_all(extension_id)` in `src-tauri/src/extensions/peers/state.rs`: close channels with `Close`, drop lobbies, send `Withdraw` for open outgoing invitations and drop incoming ones with their prompts
- [ ] T030 [US3] Call `host.peers.end_all(id)` beside `host.shells.end_all` in `src-tauri/src/extensions/commands/frames.rs` (last frame closed), `src-tauri/src/extensions/commands/manage.rs` (removed, disabled), `src-tauri/src/extensions/commands/dev.rs` (development version unloaded) and `src-tauri/src/extensions/registry/lifecycle.rs` (`stopped_here`, other device disabled or removed)
- [ ] T031 [US3] Watcher in a new `src-tauri/src/extensions/peers/watch.rs`: subscribes to `AppState::vault_changes()` (`src-tauri/src/vault_events.rs:42`); on `permissions::store::TABLES` re-evaluates `peers`/`connect` for every extension with open state and calls `end_all` when it is no longer granted; on `haex_contacts_blocked_vaults` and `haex_contacts_cards` closes channels whose remote vault is now blocked or no longer a contact; a lagged receiver re-checks everything; started where `extensions::registry::lifecycle::start_for_active_instance` starts (`src-tauri/src/extensions/registry/lifecycle.rs:282`)
- [ ] T032 [US3] Delivery only to frames of the bound extension: every emit in `src-tauri/src/extensions/peers/` goes through `emit_to_frames(…, extension_id, …)`; no `peers:*` content to holzi's window (FR-005)

**Checkpoint (end of PR 1)**: run `nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib`, `nix develop --command pnpm lint:rust` and `nix develop --command pnpm format:check`; open PR 1.

---

## Phase 5: User Story 1 - Einen Kontakt einladen und verbunden werden (Priority: P1) (PR 3)

**Goal**: An extension invites a contact; holzi asks the user on every reachable device of the
contact; the first acceptance wins and becomes a channel; the inviter gets exactly one status.

**Independent Test**: Vault A and vault B (two devices, cards exchanged). A invites B; both B devices
show holzi's prompt and no B frame gets anything; accepting on one device yields one channel, the
other prompt disappears, A gets `accepted`.

### Tests for User Story 1

- [ ] T033 [P] [US1] Invitation tests in `src-tauri/src/extensions/peers/invite_tests.rs` (prompt answered by a test double): accept on one of two devices → one channel, `Withdraw` to the other, `accepted` with `channelId`; two accepts at once → exactly one `InviteConfirm`, the other prompt reports "not open"; decline on all → one `declined`; mixed decline and not-installed → `declined`; extension missing, disabled or `peers` denied → `not-installed`; inviter not a contact or blocked → nothing shown, inviter `unreachable` with `hint: "share_current_card"`; no device reachable → `unreachable` within 15 s of measured time; expiry after 5 min of injected time → `expired`; `extension_peers_invite_cancel` → `Withdraw` everywhere and no status (FR-009–FR-019, SC-006, SC-009)
- [ ] T034 [P] [US1] Prompt tests in `src-tauri/src/extensions/peers/prompt_tests.rs`: with an open frame the window event `extension-peer-invitation` is sent; without a frame a notification with buttons `accept`/`decline` is shown through a fake `Desktop`; `Body` brings the window prompt; `notifications_allowed() == false` falls back to the window; a withdrawn or expired invitation closes the notification and sends `extension-peer-invitation-gone`; no `peers:*` event reaches any frame before acceptance (SC-006)
- [ ] T035 [P] [US1] Contacts-scope test in `src-tauri/src/extensions/peers/invite_tests.rs`: a contact outside the extension's `contacts` grant, a missing contact and a contact without card all answer `1001 contact_unknown` (051 FR-031); `peers:channel-opened.from.contactId` is present only when the grant covers the inviter

### Implementation for User Story 1

- [ ] T036 [US1] Addresses of a contact in `src-tauri/src/extensions/peers/invite.rs`: `EndpointAddr` per device of the stored card (`endpoint_id`, `iroh_relay`, `src-tauri/src/contacts/card/format.rs:43`), dialled in parallel through `SyncNode::dial_alpn` with 10 s each; a device not reachable counts as `Unreachable` (research R5, no knocking yet)
- [ ] T037 [US1] Inviting side in `src-tauri/src/extensions/peers/invite.rs`: `OutgoingInvite` per data-model.md (`expires_at` = now + 5 min, payload "JSON ≤ 4 KiB"), `Invite` on each connection after `foreign_auth::dial`, first `InviteReply::Accepted` → `InviteConfirm` and the channel, `Withdraw` and close to all others; final status exactly once per FR-018; `cancel(invite)`
- [ ] T038 [US1] Invited side in `src-tauri/src/extensions/peers/admission.rs` and `invite.rs`: accept `Invite` only from `Contact` (else close without reply); `NotInstalled` when the extension is missing, disabled or `peers` is denied; keep the connection open while asking; limits "4 offene je fremder Vault, 16 insgesamt" (data-model.md); answer `Accepted`/`Declined`; open the channel only after `InviteConfirm`; on `Withdraw`, expiry or connection end → prompt gone
- [ ] T039 [US1] Invitation prompt in a new `src-tauri/src/extensions/peers/prompt.rs`: contact name from holzi's address book through `ContactsService` (`Caller::Internal { feature: "extensionPeers" }`), never sent to the extension; with an open frame of the extension emit `extension-peer-invitation { inviteId, extensionId, extensionName, contactName }`; otherwise `Desktop::show_notification` (`src-tauri/src/extensions/host.rs:42`) with buttons `accept`/`decline` independent of the extension's `notifications` permission (FR-014, FR-015); `Body` → window prompt and raise; `ShownNotification::close` and `extension-peer-invitation-gone` when the invitation goes
- [ ] T040 [US1] Command `extension_peer_invitation_answer { inviteId, accept } -> { result: "open" | "gone" }` in a new `src-tauri/src/extensions/commands/peers.rs`, registered in `src-tauri/src/lib.rs`; ts-rs types `ExtensionPeerInvitation`, `ExtensionPeerInvitationGone` exported to `src/types/bindings/` (check `ls src/types/bindings/<Name>.ts` first)
- [ ] T041 [US1] Opening after acceptance in `src-tauri/src/extensions/peers/state.rs` and `src-tauri/src/extensions/commands/frames.rs`: without an open frame emit `extension-notification-click { extensionId }` (opens the extension through `windowManager.showExtension`, `src/stores/windowManager.ts:321`) and keep `PendingOpen`; `extension_frame_open` (`src-tauri/src/extensions/commands/frames.rs:38`) sends the waiting `peers:channel-opened` to the new frame; after 60 s without a frame close the channel with `closed`
- [ ] T042 [US1] `peers:channel-opened { channelId, origin: "invitation", inviteId, from: { contactId?, name }, payload }` and `peers:invite-status { inviteId, status, channelId?, hint? }` built in `src-tauri/src/extensions/peers/invite.rs` exactly as contracts/bridge.md; `from.name` is the card name of the other side
- [ ] T043 [P] [US1] Dialog `src/components/extensions/PeerInvitationDialog.vue` after the pattern of `PermissionRequestDialog.vue`: "[Kontakt] lädt dich zu [Erweiterung] ein" with "Annehmen" / "Ablehnen", calls `extension_peer_invitation_answer`; shows "Diese Einladung ist nicht mehr offen." when the answer is `gone`; several open prompts queue
- [ ] T044 [US1] Listen for `extension-peer-invitation` and `extension-peer-invitation-gone` in `src/composables/useExtensionHost.ts` and mount the dialog where `PermissionRequestDialog.vue` is mounted
- [ ] T045 [P] [US1] i18n keys `extensions.peers.invitation.*` (title, accept, decline, gone) in `src/i18n/locales/de.json` and `src/i18n/locales/en.json`; notification title and button labels from the same keys via the window locale held by `ExtensionHost::context()`

**Checkpoint (end of PR 3)**: `cargo test … --lib extensions::peers`, `nix develop --command pnpm typecheck`, `nix develop --command pnpm lint`, quickstart.md §2 by hand with the test double replaced by a dev build; open PR 3.

---

## Phase 6: User Story 4 - Einer Lobby mit Beitrittscode beitreten (Priority: P2) (PR 2)

**Goal**: A host's extension opens a contacts-only lobby with a join code; contacts join, others are
refused, a removed player cannot return, closing the lobby kills the code but keeps the channels.

**Independent Test**: Three vaults (A host, B contact, D not a contact). B joins, D gets
`not_admitted`, removed B gets `not_admitted`, after closing every join gets `lobby_closed`.

### Tests for User Story 4

- [ ] T046 [P] [US4] Join code tests in `src-tauri/src/extensions/peers/wire_tests.rs`: `holzi:join:` + `BASE32_NOPAD(postcard(JoinCode { v: 1, endpoint_id, relay, lobby, secret }))` round trip; any malformed text → `lobby_closed` without dialling; the code contains no name, vault data or contact data (FR-021)
- [ ] T047 [P] [US4] Lobby tests in `src-tauri/src/extensions/peers/lobby_tests.rs`: contact joins a contacts-only lobby → channel and `peers:joined { verified: true, contactId? }`; own linked device joins → verified; non-contact → `not_admitted`; removed device and removed vault → `not_admitted` until the lobby closes; closed lobby → `lobby_closed`; channels survive `lobby_close`; a second open lobby with the same `lobbyId` → `invalid_input`; display name "1–64 Zeichen" and payload "≤ 4 KiB" enforced (FR-020–FR-027)
- [ ] T048 [P] [US4] Uniform answers in `src-tauri/src/extensions/peers/lobby_uniform_tests.rs`: unknown lobby, closed lobby, wrong proof, other extension and blocked vault return the same `JoinReply::Closed`; over 1.000 runs per case the medians of the host's handling time differ by less than 1 ms (FR-025, SC-007)

### Implementation for User Story 4

- [ ] T049 [US4] `JoinCode` encode/decode and `join_proof(secret, nonces, lobby) = HMAC-SHA256(secret, lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(lobby))` in `src-tauri/src/extensions/peers/wire.rs` (`hmac` as in `src-tauri/src/sync/link/code.rs:11`, `data_encoding::BASE32_NOPAD`)
- [ ] T050 [US4] Lobby host side in a new `src-tauri/src/extensions/peers/lobby.rs`: `open(extension, lobbyId, visibility) -> joinCode` with `lobbyId` "1–64 Zeichen `[A-Za-z0-9._-]`", random 16-byte `key` and `secret` (`Zeroizing`), the code built from the endpoint's current address (`SyncNode::addr`); `close(lobbyId)` idempotent; `remove(channelId)` closes and bans the endpoint and, when verified, the vault (FR-027)
- [ ] T051 [US4] Join admission in `src-tauri/src/extensions/peers/admission.rs` and `lobby.rs`: `Join` → look up `lobby`, verify the proof with `Mac::verify_slice` (a fixed stand-in secret for an unknown `lobby`, same path), same extension, not banned, not blocked, contacts-only requires `OwnVault` or `Contact`; answer `Accepted` / `Closed` / `NotAdmitted` per contracts/wire-protocol.md; on `Accepted` register the channel and emit `peers:joined { lobbyId, channelId, displayName, verified, contactId?, payload }`
- [ ] T052 [US4] Joining side in `src-tauri/src/extensions/peers/lobby.rs`: decode the code, `dial_alpn` to `endpoint_id` with the code's relay, `foreign_auth::dial`, send `Join` within 5 s, wait for `JoinReply` at most 15 s in total; map `Closed` → `1001 lobby_closed`, `NotAdmitted` → `1002 not_admitted`, no connection → `8001 network_unavailable`

**Checkpoint**: contacts-only lobbies work between three vaults in tests.

---

## Phase 7: User Story 5 - Eine öffentliche Lobby für Mitspieler ohne Kontakt (Priority: P3) (PR 2)

**Goal**: While a public lobby is open, devices of non-contact vaults may join with the code, shown as
not verified; outside that window they are cut right after the handshake; admission is rate-limited.

**Independent Test**: A opens a public lobby, D (not a contact) joins as `verified: false`; after
closing, D's connection is cut after the handshake; D connecting without `Join` is cut after 5 s.

### Tests for User Story 5

- [ ] T053 [P] [US5] Admission tests in `src-tauri/src/extensions/peers/admission_tests.rs`: `Unverified` without an open lobby → closed right after the handshake without any message (FR-031); with only a contacts-only lobby → one bounded `Join` with a valid proof reaches `NotAdmitted`; with a public lobby → admitted to send `Join`; no admission message within 5 s of injected time → closed (FR-032); blocked vault with a valid code → `Closed` (FR-034); a contact in a public lobby → `verified: true`
- [ ] T054 [P] [US5] Throttle tests in `src-tauri/src/extensions/peers/throttle_tests.rs` with an injected clock: at most 16 unverified pending connections at once while verified contacts use separate capacity; per endpoint at most 5 attempts in 10 s and a 60 s ban after 3 failed joins (answer stays `Closed`); at most 20 new unverified connections per 10 s; verified contacts are limited only per endpoint (FR-033); the per-endpoint map keeps at most 1.024 entries

### Implementation for User Story 5

- [ ] T055 [US5] Throttle in a new `src-tauri/src/extensions/peers/throttle.rs` with the values of research.md R7 and an injected clock; `ponytail:` comment naming the ceiling (fixed windows per endpoint id) and the upgrade (token bucket per address prefix)
- [ ] T056 [US5] Admission of `Unverified` in `src-tauri/src/extensions/peers/admission.rs`: allowed only while `PeerState` has a lobby with `visibility = Public`, except for one bounded proof-bearing `Join` when only contacts-only lobbies are open; throttle checked before the handshake (pending, unverified window) and after a failed join (per endpoint); public lobbies admit `Unverified` with `verified: false` and the self-chosen display name
- [ ] T057 [US5] ADR status and links: `docs/adr/0016-non-contact-devices-for-public-lobbies.md` referenced from the module doc of `src-tauri/src/extensions/peers/admission.rs`; status stays `proposed` until the operator accepts it with PR 2

**Checkpoint (end of PR 2)**: `cargo test … --lib extensions::peers`, `nix develop --command pnpm lint:rust`; open PR 2.

---

## Phase 8: Bridge, frames and texts (PR 4)

**Purpose**: Expose every story to extensions through the bridge, deliver events to frames safely,
and show the permission text. Each task carries the story whose method or text it adds.

### Tests

- [ ] T058 [P] [US3] Bridge tests in `src-tauri/src/extensions/peers/bridge_tests.rs`: every `extension_peers_*` method answers `1002` with `peers` denied and `1004` with `details { resourceType: "peers", action: "connect", target: "*" }` when asked, before anything leaves the device (FR-002, SC-006); ids of another extension answer like unknown ones (FR-006); oversize parameters → `7000 too_large` with `details.limit`; malformed parameters → `3001 invalid_input`; no endpoint → `8001 network_unavailable` (FR-044)
- [ ] T059 [P] [US2] Frame queue cases in `scripts/check-extensions-bridge.ts`: `peers:message` with `reliable: false` is dropped before other events when 256 are held; a held `peers:message` with `reliable: true` that would be dropped triggers `extension_peers_frame_overflow` once; other events keep their order
- [ ] T060 [P] [US3] Contract test `src-tauri/tests/extension_bridge_contract.rs` lists the new methods and their module `extensions::peers::bridge`

### Implementation

- [ ] T061 [US1] `extension_peers_invite { contactId, payload }` and `extension_peers_invite_cancel { inviteId }` in a new `src-tauri/src/extensions/peers/bridge.rs` (contact scope through the grant resolution of `src-tauri/src/extensions/contacts.rs`)
- [ ] T062 [US4] `extension_peers_lobby_open`, `extension_peers_lobby_close`, `extension_peers_join` (blocking at most 15 s via `extensions::bridge::blocking::block_on`) and `extension_peers_remove` in `src-tauri/src/extensions/peers/bridge.rs`
- [ ] T063 [US2] `extension_peers_send { channelId, data: {"$bytes"}, reliable? }` and `extension_peers_close { channelId }` in `src-tauri/src/extensions/peers/bridge.rs`; `ponytail:` comment on one call per datagram (research R8) with the batching upgrade
- [ ] T064 [US3] Register all eight methods in `src-tauri/src/extensions/bridge/allowlist.rs` with `MODULE`; each checks `peers`/`connect`/`*` through the existing `evaluate` path
- [ ] T065 [US3] Limits per extension in `src-tauri/src/extensions/peers/state.rs`: 16 channels, 4 lobbies, 16 open outgoing invitations → `7000 limit` (FR-041)
- [ ] T066 [US2] Frame queue in `src/lib/extensions/bridge.ts`: `peers:message` with `reliable: false` droppable like `shell:output`; dropping a reliable one calls `extension_peers_frame_overflow { frame, channelId }`
- [ ] T067 [US2] Command `extension_peers_frame_overflow` in `src-tauri/src/extensions/commands/peers.rs`, registered in `src-tauri/src/lib.rs`: closes the channel with `lost` on both sides (FR-038)
- [ ] T068 [P] [US3] Permission texts: kind `peers` in the install dialog and permission list (`src/components/extensions/InstallDialog.vue`, `src/components/extensions/PermissionSummary.vue`) and the runtime question (`src/components/extensions/PermissionRequestDialog.vue`): "Verbindungen zu derselben Erweiterung auf Geräten anderer Vaults öffnen. Mitspieler können bei direkter Verbindung die Netzwerkadresse dieses Geräts sehen." (FR-003); keys `extensions.permissions.peers.*` in `src/i18n/locales/de.json` and `en.json`; check `src-tauri/src/extensions/commands/permission_list.rs` names the kind
- [ ] T069 [US1] Test extension `src-tauri/tests/fixtures/extension_e2e/peers/` (`manifest.json` with `peers` and `contacts`, `index.html`, `main.js`, `build.sh`, `SOURCE.md` like `model-jobs/`): invite, lobby, join, send counters, timestamps for SC-004, shows every `peers:*` event; bundled with the released vault-sdk that contains `client.peers` (needs T079); built and signed by `src-tauri/tests/extension_e2e_fixtures.rs` into `src-tauri/tests/fixtures/extension_e2e/peers.xt`
- [ ] T070 [US3] Bridge contract in `specs/017-extension-host/contracts/bridge.md`: method rows for `extension_peers_*` and rows for the `peers:*` events in "Meldungen holzi → Erweiterung", linking to `specs/057-extension-peer-channels/contracts/bridge.md`

**Checkpoint**: `cargo test … --test extension_bridge_contract`, `nix develop --command pnpm check:extensions`, `nix develop --command pnpm typecheck`.

---

## Phase 9: vault-sdk `client.peers` (separate change in `haex-space/vault-sdk`)

**Purpose**: The SDK surface of contracts/sdk.md. Own topic branch in a repository-local worktree of
`haex-space/vault-sdk` (for example `.worktrees/feat-peers` on `feat/peers`), Conventional Commits
(`feat(peers): …`) for release-please, that repository's spaex constitution and checks
(`pnpm lint`, `pnpm typecheck`, `pnpm test`). Paths below are relative to that repository. Pattern:
the contacts API of `feat(contacts): read the host's address book (#71)`.

- [ ] T071 vault-sdk: graphify consultation in that repository before naming `PeersAPI`, `PEERS_EVENTS`, `PeerError`; note candidates in the PR
- [ ] T072 [P] vault-sdk: bridge method names in a new `src/commands/peers.ts` (`PEER_COMMANDS`, the eight `extension_peers_*` names of contracts/bridge.md) and export in `src/commands/index.ts`
- [ ] T073 [P] vault-sdk: types in a new `src/types/peers.ts` (`PeerVisibility`, `PeerInviteStatus` with `hint?: "share_current_card"`, `PeerChannelOpened` with `from: { contactId?: string; name: string }`, `PeerJoined`, `PeerMessage` with `data: Uint8Array`, `PeerChannelClosed`, `PeerErrorReason`), re-exported from `src/types.ts`; manifest permission `peers?: { target: "*"; action?: "connect" }[]` in `src/types.ts`
- [ ] T074 [P] vault-sdk: events `PEERS_EVENTS` (`peers:invite-status`, `peers:channel-opened`, `peers:joined`, `peers:message`, `peers:channel-closed`) in `src/events.ts`
- [ ] T075 vault-sdk: `PeersAPI` in a new `src/api/peers.ts` with `inviteAsync`, `cancelInviteAsync`, `openLobbyAsync`, `closeLobbyAsync`, `joinAsync`, `removeAsync`, `sendAsync` (bytes as `{"$bytes"}`, `reliable` default false), `closeAsync`, `on(type, handler)` decoding `peers:message.data` from Base64; constants `MAX_FAST_BYTES = 1024`, `MAX_RELIABLE_BYTES = 65536`, `MAX_PAYLOAD_BYTES = 4096`; `PeerError` mapping `reason` to the names of contracts/sdk.md
- [ ] T076 vault-sdk: wire `client.peers` in `src/client.ts` and export the API and types from `src/index.ts`
- [ ] T077 [P] vault-sdk: tests in `src/api/__tests__/peers.test.ts` (request names and parameters, Base64 round trip of `data`, error mapping per reason, events decoded)
- [ ] T078 vault-sdk: README section on `client.peers` (invitation accepted only in holzi's prompt, no accept/decline in the SDK, lobbies, size limits)
- [ ] T079 vault-sdk: PR against `main` of `haex-space/vault-sdk`, released by release-please; holzi T069 and haex-doom pin the released version

---

## Phase 10: Polish & Cross-Cutting Concerns (PR 4)

- [ ] T080 [P] E2E scene `scripts/e2e/scenarios/extension-peers.test.ts` with two users with own vaults (`scripts/e2e/lib/group.ts`), cards exchanged, the `peers` fixture installed: invite → holzi's prompt → accept → channel → messages → close; contacts-only lobby with a non-contact refused; public lobby joined by the non-contact
- [ ] T081 [P] E2E relay and idle: the same scene with `HOLZI_E2E_SYNC_LOOPBACK=1` on one device so the channel runs through the scene's iroh relay (SC-005, `max_datagram_size ≥ 1024` on the relay path); cutting one device's network yields `timeout` within 30 s on the other (FR-039); add the scene to `scripts/e2e/platform-exclusions.ts` only where a platform cannot run it, with the reason
- [ ] T082 [P] Docs: `CONTEXT.md` entries from T005 checked against the final names; `plans/README.md` row updated per merged PR; `specs/017-extension-host/spec.md` "Beziehung"/follow-up list points to 057
- [ ] T083 Size and structure: every new or touched hand-written file ≤ 500 LoC (`extensions/peers/*`, `sync/foreign_auth.rs`); `sync/endpoint.rs` grows only by the handler hand-over
- [ ] T084 Security pass: grep the new code for logging of payloads, message data, secrets or addresses (FR-042, FR-043); secrets in `Zeroizing`; no `unwrap`/`expect` on wire input
- [ ] T085 Clippy also for Android: `nix develop --command scripts/with-nix-host-bridge.sh cargo clippy -j 4 --manifest-path src-tauri/Cargo.toml --target aarch64-linux-android --all-targets -- -D warnings` (notification buttons and network change on Android)
- [ ] T086 Full checks: `nix develop --command pnpm lint:all`, `nix develop --command pnpm typecheck`, `nix develop --command pnpm typecheck:scripts`, `nix develop --command pnpm check:extensions`, `nix develop --command pnpm format:check`, and the affected `cargo test` targets
- [ ] T087 Run quickstart.md §1–§6 by hand with two or three vaults (one Android device for §6); record SC-001–SC-012 measurements and open findings in the PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none; T003 before any code.
- **Foundational (Phase 2)**: after Setup; blocks all stories. T006 → T007 → T008; T009 → T010 → T011, T012; T013 → T014 → T015; T016 → T017 → T018; T019 after T016; T020 after T014 and T019; T021 after T020.
- **US2 (Phase 3, PR 1)**: after Phase 2. T025 → T026; tests T022–T024 against T025.
- **US3 (Phase 4, PR 1)**: after Phase 2 and T025. T029 → T030, T031; T032 alongside T025.
- **US4 (Phase 6, PR 2)**: after PR 1. T049 → T050 → T051 → T052.
- **US5 (Phase 7, PR 2)**: after T051. T055 → T056.
- **US1 (Phase 5, PR 3)**: after PR 2 is merged (shares `admission.rs`); T036 → T037; T038 after T037; T039 → T040 → T043, T044; T041 after T038.
- **Bridge (Phase 8, PR 4)**: after PR 3. T061–T063 → T064; T066 → T067; T069 after T079 (released SDK).
- **vault-sdk (Phase 9)**: after PR 1 (contract fixed); T079 before T069.
- **Polish (Phase 10, PR 4)**: after Phase 8 and the SDK release.

Execution follows the PRs (1 → 2 → 3 → 4), not the story priorities: US1 needs the admission path that
PR 2 completes, and the bridge makes all stories usable at once.

### Parallel Opportunities

- Phase 2: T007, T009, T013, T016 can start together; T011, T015, T018 in parallel once their code exists.
- US2: T022, T023, T024 in parallel.
- US1: T033, T034, T035 in parallel; T043 and T045 parallel to the Rust tasks.
- US4: T046, T047, T048 in parallel.
- Phase 8: T058, T059, T060, T068 in parallel.
- Phase 9 runs in parallel to PR 2–4 in the other repository.

## Parallel Example: User Story 1

```text
Task: "T033 [P] [US1] Invitation tests in src-tauri/src/extensions/peers/invite_tests.rs"
Task: "T034 [P] [US1] Prompt tests in src-tauri/src/extensions/peers/prompt_tests.rs"
Task: "T043 [P] [US1] Dialog src/components/extensions/PeerInvitationDialog.vue"
```

## Implementation Strategy

### MVP First (PR 1)

1. Phase 1 + Phase 2 — permission kind, ALPN, verification of foreign devices.
2. Phase 3 (US2) + Phase 4 (US3 host side) — channels with fast and reliable messages, isolation and
   cleanup, proven between two vaults in tests.
3. **Stop and validate**: the PR 1 checkpoint commands. Nothing is visible to users yet; this is the
   foundation every later PR uses.

### Incremental Delivery

1. PR 2 (US4, US5): lobbies and join codes; ADR 0016 accepted.
2. PR 3 (US1): invitations with holzi's prompt.
3. vault-sdk release with `client.peers`.
4. PR 4 (Phase 8 + Phase 10): bridge, frame queue, texts, fixture, E2E and the hand check; haex-doom
   can build against it.

## Notes

- Never `git stash` bare; never force-push; fetch before push.
- Commits follow Conventional Commits without agent trailers.
- New ts-rs types use the prefix `ExtensionPeer`; check `ls src/types/bindings/<Name>.ts` before adding one.
- Spec 054 renames ALPN, signature domain and the `holzi:join:` prefix; follow whatever has landed on `main` when each PR is opened.
- If spec 056 lands first, reuse its `ContactsService::card_of_vault` and handler list instead of T010/T013, and move its handshake onto `sync/foreign_auth.rs` in a separate change.
