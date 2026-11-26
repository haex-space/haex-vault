---
description: 'Task list for the chat contact gate (spec 099)'
---

# Tasks: Only contacts may write in the chat

**Input**: Design documents from `/specs/099-chat-contact-gate/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/nostr-chat-gate.md](contracts/nostr-chat-gate.md), [contracts/ipc.md](contracts/ipc.md), [quickstart.md](quickstart.md)

**Tests**: Included. The gate is a boundary (what a stranger can put into a vault), and the repository rules require tests for boundaries and failure behaviour. Rust tests live in sibling `*_tests.rs` files, never inline.

**Organization**: Grouped by user story. US1 and US2 are P1 and ship together as the MVP (a gate without answers locks strangers out for good); US3 and US4 are P2 and independent of each other.

**State of the code this builds on** (checked 2026-11-25, `main` at `451ccf87d`): requests of spec 072 US5 store the stranger's messages; `chat_request_answer` takes `decline | block`; group conversations (072 US4) are not built.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 to US4 as in spec.md; Setup, Foundational and Polish tasks carry none
- Paths are relative to the repository root

## Commands used by the verification tasks

- Rust on this machine: `env -u CC -u CXX -u AR CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0 cargo <args> -j 1` in `src-tauri`, inside a memory-limited scope.
- Frontend and scripts: `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm check:chat-state`, `pnpm check:templates`, `pnpm check:settings`, `pnpm check:e2e-exclusions`, `npx prettier --check .`.
- After `cargo test` regenerates ts-rs bindings: `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.

## Names used below

| Name                  | Value                                           |
| --------------------- | ----------------------------------------------- |
| `KEY_ALLOW_STRANGERS` | vault preference `chat.requests.allowStrangers` |
| `NOTICE_ACCEPTED`     | receipt `answer: "accepted"`                    |
| `NOTICE_BLOCKED`      | receipt `answer: "blocked"`                     |
| `FAIL_BLOCKED`        | delivery `failed_reason = 'blocked'`            |
| `HINT_AFTER_MS`       | 60,000: when the sender's hint shows (FR-005a)  |

---

## Phase 1: Setup

- [x] T001 Add the migration of [data-model.md](data-model.md): `chat_threads.request_first_at`, `request_last_at`, `peer_blocked_at` (INTEGER NULL), `haex_contacts_blocked_vaults.card` (BLOB NULL), and the device-local table `chat_gate_notices_no_sync (peer_vault TEXT PRIMARY KEY, answer TEXT NOT NULL, created_at INTEGER NOT NULL)`; as a new module `src-tauri/src/identity/migrations_chat_gate.rs` registered after the latest migration (CRDT triggers for the synchronized columns as `migrations_chat_contacts.rs` does)
- [x] T002 [P] Migration test in `src-tauri/src/identity/migrations_chat_gate_tests.rs`: the columns exist, `card` and the three thread columns are NULL on old rows, the notices table has no CRDT trigger

---

## Phase 2: Foundational

**Purpose**: the setting, the notice on the wire and its application; every story needs at least one of them.

- [x] T003 [P] `KEY_ALLOW_STRANGERS` in `src-tauri/src/chat/peer/settings.rs` (constant, reader `allow_strangers(q) -> bool` defaulting to `true`) and its check (`"true" | "false"`) in `src-tauri/src/storage/preferences_commands.rs` `validate_value` (research R8); tests in `src-tauri/src/chat/peer/settings_tests.rs`
- [x] T004 [P] `Receipt.answer: Option<Answer>` (`accepted | blocked`, serde lowercase, skipped when `None`) in `src-tauri/src/chat/peer/envelope.rs` per [contracts/nostr-chat-gate.md](contracts/nostr-chat-gate.md); round-trip and "old receipt without `answer`" tests in `src-tauri/src/chat/peer/envelope_tests.rs`
- [x] T005 New module `src-tauri/src/chat/peer/gate.rs`: `write_notice(tx, peer_vault, answer, now)` (insert or replace in `chat_gate_notices_no_sync`), `take_notices(tx)`, and `send_due_notices(tx, me, now)` that turns each row into a receipt with `answer` and empty lists, enqueues it through `outbox::enqueue` (as `send::receipt` does) and deletes the row; call `send_due_notices` in its own write on every receipt tick, even when the receipt batch is empty; when queueing a notice, drop any older unsent gate notice for that peer (contract `nostr-chat-gate.md`); tests in `src-tauri/src/chat/peer/gate_tests.rs`
- [x] T006 Split `src-tauri/src/chat/peer/outbox.rs` as its header documents (move `Attempt`, `record`, its delivery helpers and the explicit resend operations into `src-tauri/src/chat/peer/outbox/state.rs`), then add `park_vault(tx, vault)` (every row awaiting `vault`: `next_at = PARKED`, deliveries failed with `FAIL_BLOCKED`) and `resume_vault(tx, vault, now)` (every row awaiting `vault`: `next_at = now`, `FAIL_BLOCKED` cleared); `FailReason::Blocked` with code `"blocked"`; tests in `src-tauri/src/chat/peer/outbox_tests.rs`
- [x] T007 Apply received notices in `src-tauri/src/chat/peer/receive.rs` `receipt`: `NOTICE_BLOCKED` sets `chat_threads.peer_blocked_at` of the direct thread with `receipt.from` and calls `park_vault`; `NOTICE_ACCEPTED` clears it and calls `resume_vault`; delivered/read handling unchanged; notices are sent in outbox order after older unsent notices for the same peer are replaced (contract `nostr-chat-gate.md`); tests in `src-tauri/src/chat/peer/receive_tests.rs` (quickstart Q7: a notice whose `thread.direct` is not the own vault changes nothing)
- [x] T008 `src-tauri/src/chat/peer/targets.rs`: an outbox row whose rumor is a receipt with `NOTICE_BLOCKED` is addressed to a blocked vault, using the hidden contact's cards (`received::addresses`) or the card of its block entry, read by a new `received::blocked_card(q, vault)` in `src-tauri/src/contacts/card/received.rs` (column of T001; test in `received_tests.rs`); every other rumor to a blocked vault stays refused; tests in `src-tauri/src/chat/peer/targets_tests.rs`

**Checkpoint**: notices can be written, sent and applied; no behaviour of receiving has changed yet.

---

## Phase 3: User Story 1 - A stranger cannot put messages into my chat (Priority: P1) 🎯 MVP

**Goal**: a stranger's message leaves one request with its card and nothing else; the sender sees _sent_ and the hint.

**Independent Test**: quickstart Q1 and Q8.

### Tests for User Story 1

- [x] T009 [P] [US1] Rewrite `src-tauri/src/chat/peer/requests_tests.rs` for the new rules (quickstart Q1): three messages of B (one with a file) leave one request thread with B's card, `request_first_at < request_last_at`, no `chat_peer_messages` or `chat_attachments` row of B, one `arrived` notification event, no receipt on B's side; replace the tests of 072 US5 scenario 6 (_delivered_ for requests) and of message rows in requests
- [x] T010 [P] [US1] Own-device test in `src-tauri/src/chat/peer/own_devices_tests.rs` (quickstart Q8): two own devices of A receive the same knock; one request, one notification

### Implementation for User Story 1

- [x] T011 [US1] Gate in `src-tauri/src/chat/peer/receive.rs` `message` (research R1, R2): for an incoming direct message, a sender without a stored card returns a new `Knock { thread_id, first: bool }` outcome instead of storing: `store::mark_request` with the card, `request_first_at` set when the thread becomes a request, `request_last_at` on every knock; no message row, no attachment rows. Stored contacts and own messages unchanged
- [x] T012 [US1] `src-tauri/src/chat/peer/store.rs`: `mark_request` writes the two timestamps; `list` returns `requestLastAt` and `peerAnswered` (any incoming message or any delivery with `delivered_at` in the thread) on `ChatConversation`; regenerate the binding `src/types/bindings/ChatConversation.ts`; tests in `src-tauri/src/chat/peer/store_tests.rs`
- [x] T013 [US1] `src-tauri/src/chat/peer/delivery_receive.rs` `receive_one`: a `Knock` confirms the session (research R3), acknowledges nothing (R10), and sends the `arrived` event only when `first` (R9); `src-tauri/src/chat/peer/notify.rs` keeps the title-only, soundless request notification for it
- [x] T014 [US1] Remove the request branch of `src-tauri/src/chat/peer/targets.rs` (receipts to the card a request carried, `targets.rs:76`) and the request case of `src-tauri/src/chat/peer/send.rs` `receipt`; no receipt goes to a request (FR-005)
- [x] T015 [P] [US1] Requests view without messages: `src/components/chat/RequestBanner.vue` and `src/components/chat/PeerConversation.vue` show the card's name, the time of the latest knock and the answers, no message list, for `state === 'request'`; `src/components/chat/ThreadSidebar.vue` shows the latest knock time in the requests area
- [x] T016 [P] [US1] Sender hint (FR-005a): in `src/components/chat/PeerConversation.vue`, when `!conversation.peerAnswered`, `!conversation.blockedByPeer` and an own message's `sentAt` is older than `HINT_AFTER_MS` and its `delivery` is neither `delivered` nor `read`, show `chat.peer.notYetAccepted` ("Noch nicht zugestellt. {name} ist offline oder hat dich noch nicht angenommen."); a one-minute timer re-evaluates; i18n de/en in `src/i18n/locales/{de,en}.json`; case in `scripts/check-chat-state.ts`
- [ ] T017 [US1] Run `cargo test --lib chat::peer` and the frontend checks; fix findings

**Checkpoint**: strangers leave only requests. Requests can still only be answered with the old answers until US2.

---

## Phase 4: User Story 2 - Accept or decline a request (Priority: P1) 🎯 MVP

**Goal**: "Annehmen" lets the waiting messages in; "Ablehnen" blocks with the card and tells the sender.

**Independent Test**: quickstart Q2 and Q3; e2e E1.

### Tests for User Story 2

- [x] T018 [P] [US2] Two-vault tests in `src-tauri/src/chat/peer/requests_tests.rs`: Q2 (accept → `NOTICE_ACCEPTED` → B's rows due at once → A stores B's messages in `sent_at` order → B sees _delivered_), Q3 (decline → A's block entry holds B's card → B's thread `peer_blocked_at`, deliveries `FAIL_BLOCKED`, rows parked → B's next message parked → a further knock leaves nothing on A), and US2 scenario 7 (a card stored by another route also sends `NOTICE_ACCEPTED`)

### Implementation for User Story 2

- [x] T019 [US2] `src-tauri/src/contacts/card/received.rs`: `block_with_card(tx, vault, card: Option<&SignedCard>)` (keeps an existing card when `None`), `blocked` returns the kept card's name; tests in `src-tauri/src/contacts/card/received_tests.rs`
- [x] T020 [US2] `src-tauri/src/chat/peer/store.rs` `accept_requests_of`: for each thread it turns from `request` to `active`, `gate::write_notice(.., NOTICE_ACCEPTED)` (FR-007, every route through `received::save`)
- [x] T021 [US2] `src-tauri/src/chat/peer/requests.rs`: `Answer` keeps only `Decline`; decline = `block_with_card` with the request's card, `store::delete`, `gate::write_notice(.., NOTICE_BLOCKED)` in one transaction; `src-tauri/src/chat/peer/commands/requests.rs` `chat_request_answer` accepts only `"decline"`; update `requests_tests.rs` cases of `block`
- [x] T022 [US2] Sender side of a block in `src-tauri/src/chat/peer/send.rs`: a message to a vault whose thread has `peer_blocked_at` is stored, its delivery set `FAIL_BLOCKED`, and its outbox row parked instead of due (FR-010); `ChatConversation.blockedByPeer` from `peer_blocked_at` in `store.rs`
- [x] T023 [P] [US2] `src/components/chat/RequestBanner.vue`: exactly "Annehmen" (060 preview as today) and "Ablehnen" with a confirmation that names the consequence (blocked, sender is told); remove "Blockieren"; the action catalog drops `chat.request.block` (`src/lib/actions/chatActions.ts`) and keeps `chat.request.decline`
- [x] T024 [P] [US2] `src/components/chat/PeerConversation.vue`: with `blockedByPeer`, a notice "{name} nimmt keine Nachrichten von dir an."; messages with `FAIL_BLOCKED` show _not delivered_ with that reason (`src/components/chat/PeerMessageList.vue` reason text); input stays usable; i18n de/en
- [x] T025 [P] [US2] E2E scenario `scripts/e2e/scenarios/chat-contact-gate.test.ts` (model `chat-two-vaults.test.ts`): knock → request without text → accept → messages arrive; second pair: decline → the sender shows the block (quickstart E1)
- [ ] T026 [US2] Run `cargo test --lib chat::peer` and `cargo test --lib contacts::card` separately, the frontend checks and `check:e2e-exclusions`; fix findings

**Checkpoint**: MVP complete — the gate with both answers.

---

## Phase 5: User Story 3 - Switch off requests during a spam attack (Priority: P2)

**Goal**: with the switch off nothing of a stranger reaches the vault, and open requests are gone.

**Independent Test**: quickstart Q4; e2e E2.

- [x] T027 [P] [US3] Tests in `src-tauri/src/chat/peer/requests_tests.rs` (quickstart Q4): with `KEY_ALLOW_STRANGERS = false` a stranger's message leaves no thread, no session row, no notice and no notification; a contact's message arrives; `remove_when_off` deletes every open request without block or notice; a request present only on the second own device is deleted when the synced `false` preference arrives (`follow_switch`)
- [x] T028 [US3] `src-tauri/src/chat/peer/verify.rs` (`Drop::Stranger`): with `allow_strangers` false, a stranger's message takes the `dropped` path of `delivery_receive.rs` (no session saved, research R3), no knock
- [x] T029 [US3] `src-tauri/src/chat/peer/requests.rs` `remove_when_off(tx)` clears requests and hides empty closed request threads while `KEY_ALLOW_STRANGERS` is `false`. `follow_switch` runs at session start and after local or synced changes to preferences or threads (research R8), started in `src-tauri/src/chat/peer/service.rs`. `validate_value` takes the key as a `true`/`false` switch through the generic preferences command (no own command, no action).
- [x] T030 [P] [US3] Switch "Anfragen von Fremden zulassen" as a `SettingsChatFlagSetting` in `src/components/settings/ChatView.vue`, applying on selection; the requests area of `ThreadSidebar.vue` follows the preference; i18n de/en; `check:settings` entry if the settings registry needs one
- [ ] T031 [US3] Extend `scripts/e2e/scenarios/chat-contact-gate.test.ts`: switch off → the requests area disappears, a new knock shows nothing (quickstart E2)

**Checkpoint**: the switch works independently of US4.

---

## Phase 6: User Story 4 - Block a contact and allow a vault again (Priority: P2)

**Goal**: a blocked contact disappears everywhere but the phone and its conversation; unblocking restores it or creates it from the kept card.

**Independent Test**: quickstart Q5 and Q6.

### Tests for User Story 4

- [ ] T032 [P] [US4] Tests in `src-tauri/src/contacts/service/contacts_tests.rs`: a contact with a blocked vault is left out of `list` and `search`, `get` and `photo` answer `NotFound`, the phone readers (`phone/run.rs` headers) still see it; unblocking shows it again with all values (quickstart Q5)
- [ ] T033 [P] [US4] Tests in `src-tauri/src/contacts/service/cards_tests.rs`: `block_contact` blocks every vault of the contact and writes `NOTICE_BLOCKED` per vault; `unblock` removes every block entry of a hidden contact and writes `NOTICE_ACCEPTED` to every vault whose entry it removes; `unblock` of a declined stranger creates a contact from the kept card and writes `NOTICE_ACCEPTED` (quickstart Q6); `unblock` of an entry without card only removes it
- [ ] T034 [P] [US4] Chat tests in `src-tauri/src/chat/peer/store_tests.rs`: a direct thread with a blocked peer is left out of `list` and of the chat search, carries `peerBlocked`, and returns after unblocking

### Implementation for User Story 4

- [ ] T035 [US4] `ContactHeader.blocked` in `src-tauri/src/contacts/model.rs`, set in `src-tauri/src/contacts/store.rs` `headers` from the cards it loads and the block list (research R11); regenerate `src/types/bindings/ContactHeader.ts`
- [ ] T036 [US4] Filter in `src-tauri/src/contacts/service/contacts.rs` `list`, `search`, `get`, `photo` (hidden → left out / `NotFound`); `src-tauri/src/contacts/share/preview.rs` hint candidates skip `blocked`; leave the phone readers in `src-tauri/src/contacts/phone/` unfiltered
- [ ] T037 [US4] `src-tauri/src/contacts/service/cards.rs`: `block_contact(caller, contact_id)` and `unblock(caller, vault)` per research R13/R14 (unblock removes the entries of every vault of a hidden contact; a kept card becomes a contact through `share/merge.rs` `apply` after the entry is gone; `NOTICE_ACCEPTED`); `set_blocked(true)` on a vault that a contact holds acts as `block_contact` for that contact (every vault, spec edge case "more than one vault"), otherwise blocks the vault and writes `NOTICE_BLOCKED`; test in `cards_tests.rs`; `blocked` returns `contactId`, name and the direct `threadId` per [contracts/ipc.md](contracts/ipc.md)
- [ ] T038 [US4] Commands in `src-tauri/src/contacts/commands/cards.rs`: new `contacts_block_contact`, changed `contacts_block`, `contacts_unblock`, `contacts_blocked`; register in `src-tauri/src/lib.rs`; regenerate `src/types/bindings/BlockedVault.ts`
- [ ] T039 [US4] `src-tauri/src/chat/peer/store.rs` `list` and `src-tauri/src/chat/search.rs`: leave out direct threads whose `peer_vault` is blocked, in the list of requests too (a request a not-yet-synced own device re-created after a decline never shows); `ChatConversation.peerBlocked`; two-device test of that case in `src-tauri/src/chat/peer/own_devices_tests.rs`; opening a conversation by thread id still works for a blocked peer
- [ ] T040 [P] [US4] "Sperren" in the conversation menu of `src/components/chat/PeerConversation.vue` and on the contact page `src/components/contacts/EntryPage.vue`, both with a confirmation and the action `contacts.blockContact`; for `peerBlocked` the composer is replaced by "Entsperren"
- [ ] T041 [P] [US4] `src/components/settings/ContactsBlockedSetting.vue` and `src/composables/useContactCards.ts`: entries show the contact's or the kept card's name and the address, "Verlauf ansehen" (opens the chat at the entry's `threadId`) and "Entsperren" (action `contacts.unblock`); i18n de/en
- [ ] T042 [US4] Run `cargo test --lib contacts` and `cargo test --lib chat::peer` separately; run `scripts/e2e/scenarios/extension-contacts.test.ts` and `agent-contacts.test.ts` expectations still hold (a hidden contact is no result), plus the frontend checks; fix findings

**Checkpoint**: all stories work on their own.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T043 [P] Amend spec 072 contracts: `specs/072-unified-chat/contracts/nostr-chat.md` (receipt `answer`, no receipt for a request) and `specs/072-unified-chat/contracts/ipc.md` (`chat_request_answer`, new commands), each pointing to spec 099
- [ ] T044 [P] Terms "Anfrage", "gesperrt", "Fremde Vault" in `CONTEXT.md` if the chat terms of spec 095 live there
- [ ] T045 Run the whole quickstart list Q1–Q8 and the e2e scenario through the dispatched E2E workflow; record results here, with the time from accept/decline to the sender's view (SC-002, SC-003, compared to 072 SC-002)
- [ ] T046 Hand checks H1–H2 of [quickstart.md](quickstart.md) (operator)

---

## Dependencies & Execution Order

- **Setup (T001–T002)** → **Foundational (T003–T008)** → user stories.
- **US1 (T009–T017)** before **US2 (T018–T026)**: US2 answers the requests US1 creates. Ship together as the MVP.
- **US3 (T027–T031)** needs T003 and US1 (the knock path it drops).
- **US4 (T032–T042)** needs T005–T008 and T019; independent of US3.
- **Polish** after the stories it documents.
- **FR-013** (groups) has no task here: group conversations (072 US4) are not built; their tasks take the rule when they are.

## Parallel Opportunities

- Foundational: T003, T004 in parallel; T005 after T004; T006 parallel to T005; T007 after T005/T006; T008 after T005.
- US1: T009, T010 (tests) and T015, T016 (frontend) in parallel with T011–T014.
- US2: T023, T024, T025 in parallel once T020–T022 are in.
- US4: T032–T034 (tests) in parallel; T040, T041 in parallel once T037–T039 are in.

## Implementation Strategy

1. **PR A** (MVP): Setup + Foundational + US1 + US2. Strangers are gated, requests answered, senders told.
2. **PR B**: US3 (the switch).
3. **PR C**: US4 (blocking contacts, hiding, unblock restoring).
4. **PR D**: Polish.

Each PR runs the dispatched E2E workflow before merge.
