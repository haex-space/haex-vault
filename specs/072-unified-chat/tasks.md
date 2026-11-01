---
description: 'Task list for spec 072-unified-chat'
---

# Tasks: One chat for the agent and for contacts, delivered over Nostr

**Input**: Design documents from `/specs/072-unified-chat/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/nostr-chat.md](./contracts/nostr-chat.md),
[contracts/blossom.md](./contracts/blossom.md), [contracts/ipc.md](./contracts/ipc.md),
[quickstart.md](./quickstart.md), [ADR 0024](../../docs/adr/0024-chat-over-nostr-and-blossom.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic, and SC-005 (no
plaintext on Nostr-Relays or Blossom-Servers), SC-006 (no duplicate, none lost), SC-010 (agent gate) and
FR-056 (group convergence) need dedicated catalogs. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. No public network in `cargo test`:
Nostr through `MockRelay` (nostr-sdk `local-relay`), Blossom through the in-process server of T056,
clocks injected. Build only the affected targets with `-j 4`; ask the operator before a full workspace
build.

**Commands**: Rust checks run as `nix develop --command scripts/with-nix-host-bridge.sh cargo …
--manifest-path src-tauri/Cargo.toml -j 4`; frontend and formatting as `nix develop --command pnpm …`.

**Graphify**: Before naming any new function, type, module, component or command, run a bounded
`graphify query "<question>" --budget 1200` (refresh with `graphify update .` when
`graphify-out/.meta.json` does not match the fork point) and prefer extending a match; record the
candidates checked in the PR description. research.md R21 lists the starting set.

**Transport rule** (operator 2026-11-01, FR-018): messages and files travel only through Nostr-Relays and
Blossom-Servers. No task adds an iroh protocol or any direct device-to-device path for the chat.

**Organization**: phases follow the delivery slices of plan.md; each slice is one PR, reviewed and merged
before the next starts on top of `main`. Within a slice, tasks carry the label of the story they serve.

| PR  | Phases                                 | Tasks     | Content                                                        |
| --- | -------------------------------------- | --------- | -------------------------------------------------------------- |
| A   | –                                      | –         | docs (#724)                                                    |
| B   | Phase 1, Phase 2, Phase 3 (US1)        | T001–T022 | migration, store, conversation list, ChatApp split             |
| C   | Phase 4 (US2)                          | T023–T047 | wrapping, delivery service, outbox, receipts, notifications    |
| D   | Phase 5 (US5)                          | T048–T055 | requests and blocking                                          |
| E   | Phase 6 (US3, US8 Blossom part)        | T056–T073 | HXCF, Blossom client, transfers, chat file store, sweep        |
| F   | Phase 7 (US4)                          | T074–T084 | groups                                                         |
| G   | Phase 8 (US6)                          | T085–T089 | contact share in the chat                                      |
| H   | Phase 9 (US7)                          | T090–T097 | agent grant, actions, confirmation, MCP                        |
| I   | Phase 10 (US8 rest), Phase 11 (polish) | T098–T111 | settings, upload key, presets test, e2e, platforms, manual run |

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US8)

---

## Phase 1: Setup (PR B)

**Purpose**: module skeleton and shared test rigs.

- [x] T001 Create module `chat::peer` (`src-tauri/src/chat/peer/mod.rs`) and add `pub mod peer;` to `src-tauri/src/chat/mod.rs`; each later slice adds its submodules when it adds their first user (no empty files, no dead code under `-D warnings`)
- [x] T002 [P] Create `src-tauri/src/chat/peer/ids.rs` with a fixed `CHAT_NS: Uuid` and derived ids per data-model.md: direct thread `UUIDv5(CHAT_NS, "direct" ‖ sorted(own vault, peer vault))`, member `UUIDv5(thread_id ‖ vault)`, message `UUIDv5(thread_id ‖ sender_vault ‖ event id)`, attachment `UUIDv5(message id ‖ index)`, delivery `UUIDv5(message id ‖ recipient vault)`; tests in `ids_tests.rs` (stable values, order-independence of the direct id)
- [ ] T003 [P] Create `src-tauri/src/chat/peer/test_support.rs`: two and three vaults in one process with exchanged cards, one vault with two devices, a `MockRelay`, injected clock; modelled on `src-tauri/src/extensions/peers/test_support.rs` — built in PR C together with its first user (T027), not in PR B

---

## Phase 2: Foundational (PR B)

**Purpose**: schema and storage every story needs. ⚠️ No story work before this phase is done.

- [x] T004 Create `src-tauri/src/identity/migrations_chat_contacts.rs` with `NAME_0049 = "0049_chat_contacts"`: `ALTER TABLE chat_threads ADD COLUMN` `kind TEXT NOT NULL DEFAULT 'agent'`, `state TEXT NOT NULL DEFAULT 'active'`, `peer_vault TEXT NULL`, `muted INTEGER NOT NULL DEFAULT 0`, `last_read_at INTEGER NULL`, `picture BLOB NULL`, `meta_at INTEGER NULL`, `meta_op TEXT NULL`; new CRDT tables `chat_members`, `chat_peer_messages` (index `(thread_id, sort_at, id)`), `chat_attachments`, `chat_deliveries` and `_no_sync` tables `chat_outbox_no_sync`, `chat_wraps_seen_no_sync`, `chat_relay_cursors_no_sync`, `chat_blobs_no_sync`, `chat_attachment_files_no_sync`, `chat_group_parts_no_sync` exactly as in data-model.md; no UNIQUE constraint on any synced table; statements separated by `\n--> statement-breakpoint\n`; `#[cfg(test)] SYNCED_TABLES`
- [x] T005 Register 0049 in `holzi_migration_source()` and bump `HOLZI_TRIGGER_VERSION` 30 → 31 with a changelog line in `src-tauri/src/identity/migrations.rs`; declare the module and its test module in `src-tauri/src/identity/mod.rs`
- [x] T006 [P] Tests in `src-tauri/src/identity/migrations_chat_contacts_tests.rs`: fresh vault and upgrade from 0048 (`migration_source_before`), every new table exists, synced tables have the CRDT columns and `_no_sync` ones do not, no unique index on synced tables, existing `chat_threads` rows read `kind = 'agent'`, `state = 'active'` and their `chat_messages` are unchanged (SC-001, quickstart A1)
- [x] T007 Implement `src-tauri/src/chat/peer/store.rs` for PR B: `ConversationKind` (`agent`, `direct`, `group`), `ConversationState` (`active`, `request`, `closed`), the list over all kinds with unread count (`sort_at > last_read_at`, others' messages only), last message preview and newest activity, `open_direct` (derived id, two member rows, refuses the own vault), `set_muted`, `delete` (agent rows through `storage::chat_threads`, peer rows with members, messages, attachments, deliveries); check-then-write, no `INSERT … ON CONFLICT`; tests in `store_tests.rs`. Message, attachment and delivery accessors come with their first users in PR C/E
- [x] T008 [P] ts-rs types `ChatConversation`, `ChatConversationKind`, `ChatConversationState` in `src-tauri/src/chat/peer/store.rs` (camelCase, exported to `src/types/bindings/`); `ChatPeerMessage`, `ChatAttachment`, `ChatDeliveryState`, `ChatMember` come with their commands in PR C/E/F

**Checkpoint**: schema and store ready.

---

## Phase 3: User Story 1 - One chat app for the agent and for contacts (Priority: P1) 🎯 MVP (PR B)

**Goal**: agent, direct and group conversations in one list; agent chats unchanged; a direct conversation
can be opened from the address book.

**Independent Test**: update a vault with agent conversations, open the chat, see them unchanged, start a
direct conversation from the address book and see it in the same list (spec US1).

### Tests for User Story 1

- [x] T009 [P] [US1] Tests in `src-tauri/src/chat/peer/store_tests.rs` for the list (agent rows included, filter by kind, `requests` separate, newest activity first, unread counts from `last_read_at`, preview) and `open_direct` (creates once, the same id on both sides, refuses the own vault), mute, delete of both kinds, and that the agent storage never sees a contact conversation
- [x] T010 [P] [US1] Extend `scripts/check-chat-navigation.ts` (`pnpm check:chat-state`) for the location `contact/:id`: opens on top of the agent conversation without aligning it away, back/forward, initial location on mount, `/` leaves it; harness `scripts/lib/chat-state-harness.ts` provides `useAgentModelState` and an empty `useConversations`

### Implementation for User Story 1

- [x] T011 [US1] Commands `chat_conversations_list`, `chat_conversation_open_direct`, `chat_conversation_mute`, `chat_conversation_delete` (agent rows via `storage::chat_threads::delete_thread_and_messages`, peer rows with members, messages, attachments, deliveries) in `src-tauri/src/chat/peer/commands/conversations.rs` (module `chat::peer::commands` split by area to stay below 500 lines: `conversations`, `messages`, `requests`, `files`, `groups`, `settings`), registered in `src-tauri/src/lib.rs` (contracts/ipc.md)
- [x] T012 [US1] Limit every reader and change of `src-tauri/src/storage/chat_threads.rs` to `kind = 'agent'` (new rows get the column default), so the agent's commands (`list_threads`, `rename_thread`, `delete_thread`, `send_message`) never see or change a contact conversation; covered by T009
- [x] T013 [P] [US1] Composable `src/composables/useConversations.ts`: contact and group conversations with display name (contact, else card name) and photo from the address book, live refresh via `onVaultTablesChanged(['chat_threads','chat_peer_messages','chat_members','chat_deliveries','haex_contacts*'])`, command wrappers
- [x] T014 [US1] Keep the agent conversation in `src/components/apps/ChatApp.vue` (its state is shared with the tab actions) and move the model wiring (model store, `sendDisabled`, `composerInputDisabled`, `isDelegateModel`, error banner, `showModelSelection`, model actions) into `src/composables/useAgentModelState.ts` without behaviour change; ChatApp renders `ChatPeerConversation` when `activePeerId` is set, else the agent section
- [x] T015 [US1] Turn `src/components/chat/ThreadSidebar.vue` into the conversation list: agent and peer rows in one list by newest activity, filters "Alle", "Agent", "Kontakte", "Gruppen", "Mit Kontakt schreiben"; peer rows in `src/components/chat/PeerListItem.vue` with `ContactsAvatar`, name, preview, time, `ShadcnBadge` unread, muted mark (literal `lucide:*` names) (FR-001, FR-002); the "Anfragen" entry comes with PR D
- [x] T016 [US1] Location `contact/:id` in `src/components/wm/appRoutes.ts` and `src/composables/useChatNavigation.ts` (`contactChatPath`, `activePeerId`, tab title of the contact conversation); `requests` comes with PR D
- [x] T017 [P] [US1] New `src/components/chat/PeerConversation.vue` (header with name and avatar, empty message area, composer without model/effort/permission/agent controls but with text, attachment button and dictation via `ChatVoiceInputControl`; FR-006) — sending is wired in US2
- [x] T018 [P] [US1] New `src/components/chat/NewContactDialog.vue` ("Mit Kontakt schreiben"): search, only contacts with a confirmed card of another vault, never "Ich" (FR-004); groups (several contacts plus a name) join it in PR F
- [x] T019 [US1] "Nachricht senden" in `src/components/contacts/EntryPage.vue` next to "Teilen" when `contact.card` exists and not `isSelf`; otherwise the hint that the contact first needs to share its Visitenkarte (US1 scenarios 4, 6)
- [x] T020 [P] [US1] Actions `chat.contact.open`, `chat.contact.openDirect`, `chat.conversation.mute` (not agent-callable) in `src/lib/actions/chatActions.ts`; handlers in `src/composables/useChatTab.ts`; `chat.conversation.delete` deletes a contact conversation only for the user (an agent gets "no conversation", FR-072)
- [x] T021 [P] [US1] i18n `actions.chat.contact.*`, `actions.chat.conversation.mute`, `chat.list.*`, `chat.peer.*`, `chat.newContact.*`, `contacts.message.*` in `src/i18n/locales/de.json` and `en.json`
- [x] T022 [US1] Keep `ChatApp.vue`, `ThreadSidebar.vue` and `useChatTab.ts` below 500 lines; run `pnpm check:chat-state`, `pnpm lint`, `pnpm typecheck`, `cargo test --lib chat::`, `cargo test --lib migrations_chat_contacts`, clippy for both feature sets

**Checkpoint**: PR B — one list, agent chats unchanged, empty direct conversations from the address book.

---

## Phase 4: User Story 2 - Write with a contact, online or not (Priority: P1) (PR C)

**Goal**: text messages to a contact over stored Nostr gift wraps, with delivery states, own-device
completeness and notifications.

**Independent Test**: two vaults that are contacts; close B, send from A, open B, see the message on B
and _delivered_ on A; answer from B (spec US2, quickstart A3, A4).

### Tests for User Story 2

- [ ] T023 [P] [US2] Tests in `src-tauri/src/sync/presence_tests.rs` that presence keeps kind 21059, real `created_at` and the 16 KiB limit after T030
- [ ] T024 [P] [US2] Tests in `src-tauri/src/chat/peer/wrap_tests.rs`: kind 1059, `p` tag, `expiration` = 30 days, `created_at` within 48 h back, wrap ≤ 60 KiB, rumor content > 32 KiB refused
- [ ] T025 [P] [US2] Tests in `src-tauri/src/chat/peer/verify_tests.rs` for every check of contracts/nostr-chat.md "Receiver checks": seen wrap, bad card signature, older card refused, seal key not on card or removed, `from` ≠ card vault, blocked vault (dropped, no receipt), `to` not the own vault, unknown `v` (quickstart A2)
- [ ] T026 [P] [US2] Tests in `src-tauri/src/chat/peer/outbox_tests.rs`: back-off 5 s, 15 s, 1 min, 5 min, then 15 min; _failed_ after 24 h; resend re-queues; survives restart
- [ ] T027 [P] [US2] Integration test `src-tauri/tests/chat_two_vaults.rs`: US2 scenarios 1–5, 7, 8, 9 with `MockRelay` (receiver closed, both open, sender offline, multi-device recipient seen once, no Nostr-Relay → failed with reason, card without Nostr-Relays → own relays + hint, newer card reaches new device)
- [ ] T028 [P] [US2] Integration test `src-tauri/tests/chat_own_devices.rs`: Anna's two devices never online together end with the same rows (FR-016, quickstart A4)
- [ ] T029 [P] [US2] Soak test `src-tauri/tests/chat_soak.rs` (`#[ignore]`): 1,000 messages between two vaults with two devices each, random on/offline, no duplicate, none lost (SC-006, quickstart A5)

### Implementation for User Story 2

- [ ] T030 [US2] Generalise `wrap_payload` / `unwrap_rumor` in `src-tauri/src/sync/presence.rs`: wrap kind, `created_at` tweak and size limit as parameters; presence passes 21059, no tweak, 16 KiB (one wrapping implementation, research R1)
- [ ] T031 [US2] `src-tauri/src/chat/peer/envelope.rs`: serde types of the envelope (`v`, `id`, `type`, `at`, `from`, `card`, `to`, `body`) and receipt (kind 24121) of contracts/nostr-chat.md; `text` ≤ 4,000 characters, ≤ 10 attachments, rumor content ≤ 32 KiB checked before sending (`chat.error.tooLarge`)
- [ ] T032 [US2] `src-tauri/src/chat/peer/wrap.rs`: wrap a rumor (kinds 24120/24121) to one device key with kind 1059, `["p", key]`, `["expiration", now + 30 days]`, `created_at` = now − random(0 … 48 h); unwrap with the device secret (`DeviceKeys`)
- [ ] T033 [US2] `src-tauri/src/chat/peer/verify.rs`: the receiver checks of contracts/nostr-chat.md using `contacts::card` decode/verify, `contacts::card::received::{stored, replace, is_blocked}` and the `(list_generation, issued_at)` rule; a newer card of an existing contact replaces the stored one (FR-020); drops are logged with a reason code and without content
- [ ] T034 [US2] `src-tauri/src/chat/peer/outbox.rs`: `chat_outbox_no_sync` (rumor, pending recipient vaults, attempts, `next_at`, `last_error`, `created_at`), back-off of research R6, _failed_ after 24 h with `failed_reason` (`noRelay`, `noDevices`, `tooLarge`)
- [ ] T035 [US2] `src-tauri/src/chat/peer/delivery.rs`: own `nostr_sdk::Client` per vault session; read relays = `ServerConfig::effective_nostr_relays()`; subscribe `{"kinds":[1059], "#p":[own device key], "since": cursor − 48 h}`; cursors in `chat_relay_cursors_no_sync`, seen wraps in `chat_wraps_seen_no_sync` (pruned after 30 days); publish wraps for every device on every recipient card (`received::addresses`) and every own other device, to the recipient's Nostr-Relays (else own, US2 scenario 8) with `send_event_to`; write-only relays removed after 10 minutes idle (`ponytail:`); `sent_at` when one Nostr-Relay of a recipient accepted; reconnect on `wakeups.relays`/`network`; `ensure_crypto_provider()` first; test in `delivery_tests.rs` that the subscription relays equal the Nostr-Relays on the own card (`own_card::addresses`, FR-082)
- [ ] T036 [US2] Start and stop the service with the vault session: `chat::peer::start_for_active_instance` called from `src-tauri/src/instances/open.rs`, `create.rs`, `import.rs` under the vault gate's tracker and cancel token
- [ ] T037 [US2] `src-tauri/src/chat/peer/receive.rs`: verified `message` → direct thread by derived id (created if missing; `state = request` handled in US5 — until then non-contacts are dropped), message row by derived id (check-then-insert, `inserted_by` = this device), attachments rows, `received_at`, `sort_at`
- [ ] T038 [US2] `src-tauri/src/chat/peer/receipts.rs`: delivery receipt only from the device that inserted the row, read receipts from the device where the conversation was opened and only with `chat.receipts.read` on (FR-024), batched per conversation at most every 2 s, never to a blocked vault; incoming receipts set `delivered_at` / `read_at` once
- [ ] T039 [US2] Command `chat_peer_send` (text only in this PR; `idempotencyKey` dedup), `chat_peer_resend`, `chat_peer_messages_list`, `chat_conversation_read` (sets `last_read_at` on the thread, FR-022) in `src-tauri/src/chat/peer/commands/messages.rs`
- [ ] T040 [P] [US2] `src-tauri/src/chat/peer/settings.rs`: `chat.receipts.read` (vault, default `true`) and `chat.notifications` (device, default `true`) with typed accessors (invalid value reads as default); add device keys to `DEVICE_PREFERENCE_KEYS` in `src-tauri/src/storage/maintenance.rs`
- [ ] T041 [US2] Extract the notification routing of `src-tauri/src/extensions/desktop.rs` (`on_action`, shown-id map, Android permission request) into `src-tauri/src/notifications/` with a typed target; extensions keep their behaviour (tests stay green)
- [ ] T042 [US2] `src-tauri/src/chat/peer/notify.rs`: notification for a new message in a conversation that is not muted and not open in a focused window, while the vault is open; tap opens the chat at `thread/<id>` (FR-023)
- [ ] T043 [P] [US2] `src/composables/usePeerConversation.ts` and `src/components/chat/PeerMessageList.vue`: messages ordered by `sortAt`, sender name/avatar (address book, else card name), time, own delivery state icons for waiting/sent/delivered/read/failed with "Erneut senden" and reason; the _read_ state is hidden when `chat.receipts.read` is off (FR-024, reciprocal)
- [ ] T044 [US2] Wire sending in `src/components/chat/PeerConversation.vue` (text, 4,000-character counter, hints for card without Nostr-Relays); mark read on open
- [ ] T045 [P] [US2] i18n `chat.delivery.*`, `chat.error.*` in de/en; actions `chat.peer.send`, `chat.peer.resend` (not agent-callable) in `src/lib/actions/chatActions.ts`
- [ ] T046 [US2] Own-device addressing in `src-tauri/src/chat/peer/delivery.rs`: the own vault's current device list (the effective list used by `sync::foreign_auth` for `OwnVault`) minus this device, removed devices excluded; test in `delivery_tests.rs` that a removed own device gets no wraps and a newly linked one does
- [ ] T047 [US2] Run quickstart A2–A4 and `pnpm check:chat-state`; record results in the PR

**Checkpoint**: PR C — text chat with contacts works offline and online, own devices complete.

---

## Phase 5: User Story 5 - Messages from unknown senders, and blocking (Priority: P2) (PR D)

**Goal**: unknown senders land in "Anfragen"; accept adds the contact; block silences.

**Independent Test**: B has A's card, A not B's; B writes; A sees a request; accept → contact; block →
further messages dropped (spec US5, quickstart A9).

- [ ] T048 [P] [US5] Integration test `src-tauri/tests/chat_requests.rs`: US5 scenarios 1–6 (no notification sound and no text for requests, accept creates the contact with the carried card, decline removes silently and a later message is a new request, block drops messages and invitations without receipt and appears in the blocked list, sender never sees _read_ before acceptance)
- [ ] T049 [US5] In `src-tauri/src/chat/peer/receive.rs` create direct threads from vaults without a stored, non-blocked card with `state = 'request'` and store the carried card on the member row
- [ ] T050 [US5] Command `chat_request_answer` (`accept` → spec 060 preview/merge with the carried card via `contacts/share/preview.rs` and `merge.rs`, `state = active`; `decline` → delete locally; `block` → `received::block` + delete) in `src-tauri/src/chat/peer/commands/requests.rs`
- [ ] T051 [US5] Read receipts suppressed for `state = request` in `src-tauri/src/chat/peer/receipts.rs` (FR-043); notification for requests title-only "Neue Anfrage" in `notify.rs` (FR-040)
- [ ] T052 [P] [US5] `src/components/chat/RequestBanner.vue` with "Annehmen", "Ablehnen", "Blockieren"; accept opens the existing 060 preview (`src/components/contacts/ShareDialog.vue` confirm mode or its preview component)
- [ ] T053 [US5] Requests view at route `requests` in `src/components/apps/ChatApp.vue` / `ThreadSidebar.vue`
- [ ] T054 [P] [US5] i18n `chat.request.*` de/en; actions `chat.request.accept|decline|block` (not agent-callable)
- [ ] T055 [US5] Deleting a contact keeps its conversation; new messages from that vault show as a request (edge case) — test in `chat_requests.rs`

**Checkpoint**: PR D.

---

## Phase 6: User Story 3 - Send files to a contact (Priority: P1) (PR E)

**Goal**: files as copies over a Blossom-Server; every receiving device downloads itself; blob deleted
after all confirmations.

**Independent Test**: send a file while both are online and while the recipient is closed; both arrive;
the blob is gone afterwards (spec US3, quickstart A6).

### Tests for User Story 3

- [ ] T056 [P] [US3] In-process Blossom test server in `src-tauri/tests/support/blossom.rs` on `hyper` (dev-dependency): in-memory blobs, `HEAD /upload` with configurable max size and `X-Reason`, `PUT /upload` checking a kind 24242 auth event (`t=upload`, `x`, `expiration`), `GET /<sha256>`, `DELETE /<sha256>` only by the uploading key; request log for privacy assertions
- [ ] T057 [P] [US3] Tests in `src-tauri/src/chat/peer/files/crypto_tests.rs`: HXCF round trip for 0 B, 1 B, 1 MiB, 1 MiB + 1, 10 MiB; truncated, reordered, flipped segment and wrong last flag rejected; plaintext hash mismatch rejected
- [ ] T058 [P] [US3] Tests in `src-tauri/src/chat/peer/files/blossom_tests.rs` against the T056 server: `HEAD /upload` limits and `X-Reason`, upload auth event fields, descriptor hash check, `404` → gone, delete only with the uploading key
- [ ] T059 [P] [US3] Integration test `src-tauri/tests/chat_files.rs`: US3 scenarios 1–3, 5, 6, 8 (receiver open, receiver closed and sender closed by then, deletion after every recipient device and every other own device sent `files`, progress events, refused over 100 MB and over the server limit, second own device downloads itself) and the 30-day sweep with an injected clock

### Implementation for User Story 3

- [ ] T060 [US3] `src-tauri/src/chat/peer/files/crypto.rs`: HXCF v1 per contracts/blossom.md (magic, version 1, segment size 1 MiB, 8-byte nonce prefix ‖ u32 BE index, AAD `"HXCF1" ‖ last_flag`), AES-256-GCM (`aes-gcm` 0.11), streaming over segments in `spawn_blocking`
- [ ] T061 [US3] `src-tauri/src/chat/peer/files/blossom.rs`: `HEAD /upload` (BUD-06), streamed `PUT /upload` (`Body::wrap_stream`), `GET`, `DELETE` with kind 24242 auth events (`t`, `x`, `expiration` + 10 min), fresh key per blob; pinned client from `src-tauri/src/remote_storage/http.rs` (HTTPS only, `http://127.0.0.1` in tests, no redirects, no proxy, 30 s connect, 5 min per transfer)
- [ ] T062 [US3] `src-tauri/src/chat/peer/files/store.rs`: per-device encrypted folder `Chat` under `<app_local_data>/chat-files/<vault instance>/` created if missing (pattern `agent_browser::profile_store::ProfileStore`), files at `<conversation id>/<attachment id>`; writes through `files::encrypted::transfer::put_file` behind a semaphore of 1 (`ponytail:` whole file in memory, ≤ 100 MB, upgrade = streaming put); `chat_attachment_files_no_sync` states `pending`, `downloading`, `stored`, `failed`, `gone`
- [ ] T063 [US3] `src-tauri/src/chat/peer/files/transfers.rs`: on send, encrypt and upload every file before queuing the message (refuse > 100 MB or > server limit, offer a 056 file share); on receive, download when size ≤ automatic limit (`chat.download.autoLimit` 25 MB, `chat.download.autoLimitMetered` 5 MB on Android mobile data) else `pending`; verify both hashes; `chat-transfer` events throttled 100 ms; resume/restart after reopening (FR-037)
- [ ] T064 [US3] Metered-network detection for FR-036: `isActiveNetworkMetered` via `ConnectivityManager` in `src-tauri/plugins/holzi-android/android/src/main/java/space/haex/holzi/android/HolziAndroidPlugin.kt` (new command) and a Rust accessor `chat::peer::files::network::is_metered()` in `src-tauri/src/chat/peer/files/network.rs` that asks the plugin on Android and returns `false` on desktop; checked again before each automatic download; test the desktop branch in `network_tests.rs` and the Android branch in the emulator e2e (T108)
- [ ] T065 [US3] `files` receipts from every device that stored a file (field `device`) in `src-tauri/src/chat/peer/receipts.rs`; `chat_blobs_no_sync.pending_devices` = every device on every recipient card at send time plus own other devices
- [ ] T066 [US3] `src-tauri/src/chat/peer/files/sweep.rs`: `DELETE` when `pending_devices` is empty; sweep on vault open and every 6 h deleting blobs older than 30 days; failed deletes retried at the next sweep (`ponytail:` dead devices keep the blob until the sweep)
- [ ] T067 [US3] Local file cleanup for FR-008 in `src-tauri/src/chat/peer/files/store.rs`: on vault open and whenever `chat_attachments` changes (local delete or inbound sync, via the `vault_events` table report), delete local files and `chat_attachment_files_no_sync` rows whose attachment row no longer exists; test in `store_tests.rs` that deleting a conversation on one device removes the files on the other after sync
- [ ] T068 [US3] Commands `chat_attachment_download`, `chat_attachment_open` (via `files_open` with a chat source), `chat_attachment_open_system` (desktop only, via `files_open_system`), `chat_attachment_save` (`PickedFile`, `tauri_plugin_fs`, Android `content://`) in `src-tauri/src/chat/peer/commands/files.rs`; `chat_peer_send` accepts `files: PickedFile[]`
- [ ] T069 [P] [US3] Setting `chat.blossom.server` (vault; default `none` until T104 sets the tested preset) and the automatic download limits in `src-tauri/src/chat/peer/settings.rs`; without a server attaching is not offered (FR-038)
- [ ] T070 [P] [US3] `src/components/chat/AttachmentBubble.vue` and `src/composables/useChatTransfers.ts`: name, size, type icon, progress, "Herunterladen" above the limit, "Öffnen" (files `Viewer.vue`), "Mit System-App öffnen" (desktop), "Speichern unter" (`pickSaveTargetAsync`), "Datei nicht mehr verfügbar", retry
- [ ] T071 [US3] Attachment picking in `PeerConversation.vue` (`pickManyAsync`), size check before sending, hint to the chat settings without a Blossom-Server (US3 scenario 7)
- [ ] T072 [P] [US3] i18n `chat.file.*` de/en
- [ ] T073 [US3] Privacy test `src-tauri/tests/chat_privacy.rs`: across a run with text, files and a group, the `MockRelay` store and the test Blossom request log contain no plaintext, file content, names, group names or vault identities (SC-005, quickstart A11)

**Checkpoint**: PR E.

---

## Phase 7: User Story 4 - Group conversations (Priority: P2) (PR F)

**Goal**: groups with invitations, any member adds/removes, leave, rename, convergent member list.

**Independent Test**: three vaults; group on A with B and C; accept; messages everywhere; B leaves and
gets nothing further (spec US4, quickstart A7, A8).

### Tests for User Story 4

- [ ] T074 [P] [US4] Property-style tests in `src-tauri/src/chat/peer/group_tests.rs`: for random op sequences (add, remove, leave, accept, decline, rename, picture) delivered in every permutation to every member, all members end with the same member list and meta (FR-056); ops by non-members at `at` ignored; tie-break by `op_id`
- [ ] T075 [P] [US4] Integration test `src-tauri/tests/chat_group_three_vaults.rs`: US4 scenarios 1–10 incl. a member who is not a contact of A, the limit of 32 members, messages not delivered to a member before `joined_at`, blocked member hidden for A only
- [ ] T076 [P] [US4] Soak test `src-tauri/tests/chat_group_soak.rs` (`#[ignore]`): a group of 10 vaults, 100 messages and 5 files from random members, every member receives everything (SC-008)

### Implementation for User Story 4

- [ ] T077 [US4] `src-tauri/src/chat/peer/group.rs`: apply `group.op` with LWW per member on `(state_at, state_op)` and per meta on `(meta_at, meta_op)`; any member may add or remove (no admins); each applied op inserts a `note` message `{op, by, member?, name?}`
- [ ] T078 [US4] Invitations: build `group.invite` in parts of ≤ 12 members (each with signed card, state, at, op, by), receive into `chat_group_parts_no_sync`, show once all `of` parts arrived, drop after 7 days
- [ ] T079 [US4] Addressing in `src-tauri/src/chat/peer/delivery.rs`: group messages to every `member` at send time (and ops additionally to `invited` and the affected vault), verifying non-contact members against the member row's card (R5, R9); card updates from messages update the member row
- [ ] T080 [US4] Commands `chat_group_create` (≥ 2 contacts, ≤ 31 others), `chat_group_add`, `chat_group_remove`, `chat_group_leave`, `chat_group_rename`, `chat_group_picture` (≤ 48 KiB base64 in the op, ≤ 64 KiB stored), `chat_invitation_answer` in `src-tauri/src/chat/peer/commands/groups.rs`
- [ ] T081 [US4] Leaving or being removed sets the thread `state = 'closed'` (readable, composer disabled); deleting a group thread leaves first (FR-008, FR-055)
- [ ] T082 [P] [US4] `src/components/chat/GroupMembersPanel.vue`: members with state, "nicht in deinen Kontakten" + "Zu Kontakten hinzufügen" (060 preview with the member card), add from contacts, remove, leave, rename, picture
- [ ] T083 [US4] Group creation in `NewConversationDialog.vue`; invitation prompt ("Annehmen"/"Ablehnen") in `PeerConversation.vue` / `RequestBanner.vue`; note messages rendered in `PeerMessageList.vue`; per-member delivery detail for own messages
- [ ] T084 [P] [US4] i18n `chat.group.*` de/en; actions `chat.group.*` (not agent-callable)

**Checkpoint**: PR F.

---

## Phase 8: User Story 6 - Pass on a contact in the chat (Priority: P2) (PR G)

**Goal**: "Im Chat senden" in the share dialog; contact card bubble with preview and import.

**Independent Test**: share a contact into a conversation, receive it, add it from the preview, see it
with its photo in the address book (spec US6).

- [ ] T085 [P] [US6] Tests in `src-tauri/src/chat/peer/commands/messages_tests.rs`: a contact attachment carries the 060 share text with the chosen fields; a photo too large for the wrap goes as a file attachment referenced by index; receiving opens the preview data
- [ ] T086 [US6] Contact attachments in `chat_peer_send` (`contacts: ContactShareBuildArgs[]` → `contacts_share_build`), photo full size via the file path of US3 when needed (contracts/nostr-chat.md)
- [ ] T087 [P] [US6] `src/components/chat/ConversationPicker.vue` and target "Im Chat senden" in `src/components/contacts/ShareDialog.vue` (FR-060)
- [ ] T088 [P] [US6] `src/components/chat/ContactCardBubble.vue`: opens the 060 preview with "Neu anlegen" / "Zusammenführen" and the photo (FR-061); attach-contact entry in the composer
- [ ] T089 [P] [US6] i18n `chat.contact.*`, `contacts.share.toChat` de/en; extend `scripts/check-contacts-*.ts` for the new target

**Checkpoint**: PR G.

---

## Phase 9: User Story 7 - The agent and contact conversations (Priority: P3) (PR H)

**Goal**: agents read only with a grant and send only after confirmation; same rules over MCP.

**Independent Test**: read with grant off (asks), allow, read; send → dialog; decline (nothing sent);
confirm (sent) (spec US7, quickstart A10).

- [ ] T090 [P] [US7] Tests in `src-tauri/src/chat/peer/agent_actions_tests.rs`: no read without grant; denied conversation looks absent; "Einmal" holds for one call; "Immer" per conversation and `*`; file names visible, file content only with the agent file permission; send always asks, refused on timeout (120 s) or "Nein"; group/request/block/settings actions not reachable (FR-070–FR-073, SC-010)
- [ ] T091 [P] [US7] MCP tests in `src-tauri/src/mcp_server/` test files: scope `chat.contacts.read` required for both read actions, send reachable only with it and still confirming (FR-074)
- [ ] T092 [US7] `AgentFileKind::ContactChats` (targets `*` or `thread:<id>`) in `src-tauri/src/files/permissions.rs`; `ask_contact_chats` in `src-tauri/src/files/agent/prompt.rs`; dialog branch in `src/components/files/AgentPermissionDialog.vue`; settings display in `src/components/settings/AgentFilesSetting.vue` and `agents/AgentDetailView.vue`
- [ ] T093 [US7] Native executor `src-tauri/src/chat/peer/agent_actions.rs` for `chat.contacts.conversations.list` and `chat.contacts.messages.read` (`limit ≤ 100`, serves external agents; refusals mapped to not-found like `contacts::agent::hidden`), registered in `src-tauri/src/lib.rs`
- [ ] T094 [US7] `src-tauri/src/chat/peer/agent_send_confirm.rs` (pattern `contacts/agent_share_confirm.rs`): event `chat-send-confirm-request` with agent, conversation, recipients, exact text and attachments, 120 s timeout, command `chat_send_confirm_answer`; action `chat.contacts.message.send` with `confirming_action_ids`
- [ ] T095 [P] [US7] `src/components/chat/SendConfirmDialog.vue` mounted in `src/pages/workspace/[instance].vue` beside `ShareConfirmDialog`
- [ ] T096 [US7] Action definitions `chat.contacts.conversations.list`, `chat.contacts.messages.read`, `chat.contacts.message.send` (`agentCallable`, `runner: 'native'`, send `confirmsItself: true`) in `src/lib/actions/chatActions.ts`; scope descriptions in `src/lib/actions/scopes.ts`; MCP data scope `chat.contacts.read` in `src-tauri/src/mcp_server/grants.rs`; run `scripts/check-agent-actions.ts`, `check-mcp-actions.ts`
- [ ] T097 [P] [US7] i18n `chat.agent.*`, `files.agentPermission.contactChats.*` de/en

**Checkpoint**: PR H.

---

## Phase 10: User Story 8 - Chat settings (Priority: P3) (PR I)

**Goal**: settings category "Chat"; upload key; tested presets.

**Independent Test**: change each setting and see its effect on the next message or file (spec US8).

- [ ] T098 [P] [US8] Tests in `src-tauri/src/chat/peer/settings_tests.rs`: defaults, invalid values read as default, `none` server, device vs vault scope
- [ ] T099 [US8] Commands `chat_settings_get`/`set`, `chat_blossom_check` (`HEAD /upload` probe with auth, returns `ok`, `maxSize`, `reason`) and `chat_blossom_key_create` (new Nostr key stored as a password-manager entry tagged `haex-blossom`, id in `chat.blossom.keyEntry`, returns the npub) in `src-tauri/src/chat/peer/commands/settings.rs`; uploads use that key when set (R12)
- [ ] T100 [US8] Settings category `chat` in `src/lib/settings/registry.ts` (`SettingsCategoryId`, `SETTINGS_CATEGORIES`, `SETTINGS_LOCATIONS`), view mapping in `src/components/wm/appRoutes.ts`; `scripts/check-settings.ts` expects seven categories
- [ ] T101 [US8] `src/components/settings/ChatView.vue` with `SettingsGroup`/`SettingsRow`/`SettingsOptionRow`: Blossom-Server (presets, own address with check result, "Keiner"), Blossom upload key (create, show npub), read receipts, automatic download limits, notifications; saved on selection, revert on failure (pattern `AutonomyModeSetting.vue`)
- [ ] T102 [P] [US8] i18n `settings.categories.chat.*`, `chat.settings.*` de/en
- [ ] T103 [US8] Mute in the conversation header and list context menu; muted conversations count unread but never notify (US8 scenario 5)

**Checkpoint**: PR I part 1.

---

## Phase 11: Polish & Cross-Cutting Concerns (PR I)

- [ ] T104 Test the public Blossom-Servers for the preset list (quickstart M13): upload an encrypted 20 MB blob anonymously, `HEAD` limits, `DELETE`; put only passing servers into the presets next to the default Nostr-Relays (`src-tauri/src/sync/servers.rs` or `chat/peer/settings.rs`); if none passes, default `none` with the settings hint; record the results in research.md R12; then set the default of `chat.blossom.server` to the first passing preset (replaces the `none` default of T069)
- [ ] T105 [P] `src-tauri/src/bin/e2e_blossom.rs` behind feature `e2e` (hyper optional dependency, in-memory, prints its address), and a helper in `scripts/e2e/lib/blossom.ts` like `nostr-relay.ts`
- [ ] T106 E2E scenario `scripts/e2e/scenarios/chat-two-vaults.test.ts` (model `extension-peers.test.ts`): text both ways, a 5 MB file both ways, _read_ state, request accept (quickstart A13)
- [ ] T107 [P] E2E scenario `scripts/e2e/scenarios/chat-group.test.ts` with three vaults (create, accept, message, leave)
- [ ] T108 [P] Android e2e: notification on a new message while backgrounded (extend `android-notifications.test.ts`), download limit on metered network if the emulator can simulate it
- [ ] T109 Update spec 060's "spätere Messenger-Spec" references to point at spec 072 in `specs/060-contact-sharing/spec.md`; set ADR 0024 to accepted
- [ ] T110 Run the full quickstart automated list A1–A13 and fix findings; in the e2e scenario of T106 measure and record SC-002 (95 % < 5 s with both open), SC-003 (< 30 s after opening), SC-004 (< 30 s after reconnect) and SC-007 (10 MB file < 1 min, blob gone < 1 min after confirmation)
- [ ] T111 Manual run M1–M13 on Linux, Windows, macOS and Android (SC-011); record results in the PR, including SC-009 (first message from the address book in < 20 s)

---

## Dependencies & Execution Order

- **Setup (T001–T003)** → **Foundational (T004–T008)** → US1 (PR B).
- **US2 (PR C)** needs PR B. Everything after needs PR C (transport).
- **US5 (PR D)**, **US3 (PR E)** need PR C; independent of each other.
- **US4 (PR F)** needs PR C (group pictures go inline, ≤ 48 KiB); file messages in groups work once PR E is merged.
- **US6 (PR G)** needs PR C; large photos need PR E.
- **US7 (PR H)** needs PR C (read) and works for files after E.
- **US8 (PR I)** needs PR E (Blossom settings) and finishes T069's default via T104.
- Merge order: B → C → D → E → F → G → H → I.

### Within each story

- Tests first where listed; they must fail before the implementation.
- Store/envelope before services; services before commands; commands before UI.

## Parallel Opportunities

- Phase 1: T002, T003 together; Phase 2: T006, T008 alongside T007.
- PR B: T013, T017, T018, T020, T021 in parallel after T011/T014.
- PR C: all tests T023–T029 in parallel; T040, T043, T045 alongside the service work.
- PR E: T056–T059 in parallel; T069, T070, T072 alongside T060–T068.
- PR F: T074–T076 in parallel; T082, T084 alongside T077–T081.

### Parallel example (PR C)

```text
Task: "Tests for wrap in src-tauri/src/chat/peer/wrap_tests.rs" (T024)
Task: "Tests for verify in src-tauri/src/chat/peer/verify_tests.rs" (T025)
Task: "Tests for outbox in src-tauri/src/chat/peer/outbox_tests.rs" (T026)
Task: "Integration test src-tauri/tests/chat_two_vaults.rs" (T027)
```

## Implementation Strategy

### MVP

PR B (US1) is shippable on its own (one list, agent unchanged) but not useful to contacts. The first
user-visible value is **PR B + PR C**: text chat with contacts, offline delivery, own devices. Stop and
validate (quickstart A3, A4, M2–M4) before files.

### Incremental delivery

B → C (MVP) → D (safety against unknown senders) → E (files) → F (groups) → G (contact share) → H
(agent) → I (settings, presets, e2e, platforms). Each PR is merged before the next one starts.
