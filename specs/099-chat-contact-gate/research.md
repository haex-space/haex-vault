# Research: Only contacts may write in the chat

Decisions for [plan.md](plan.md). Code references are to the state of `main` at `451ccf87d`.

## R1. Where the gate sits on the receiving side

- **Decision**: `receive::message` (`src-tauri/src/chat/peer/receive.rs`) decides, after `verify::check` has
  passed. It already calls `sort_request` for every incoming direct message, which asks
  `received::stored(tx, from)`. The new rule there: a stored, unblocked card → store the message as today;
  no stored card → leave or refresh the request (R2) and return without a message row; the setting of
  US3 off → return nothing at all. Blocked vaults never get here: `verify::check_peer` drops them
  (`Drop::Blocked`, `verify.rs:212`).
- **Rationale**: one place for direct messages, inside the transaction that would have stored the row; the
  attachment rows are written by the same function, so nothing of a stranger's files is stored and no
  download is queued (downloads start from stored attachment rows).
- **Alternatives considered**: deciding in `verify::check_peer` — it does not write the request and would
  need to return the card; deciding before decryption — impossible, the sender's vault is only known
  from the decrypted rumor (spec 095).

## R2. What a request is now

- **Decision**: a request stays a direct `chat_threads` row with `state = 'request'` and the carried card on
  the peer's `chat_members` row (`store::mark_request`, unchanged), but it has **no** `chat_peer_messages`
  rows. Two new columns on `chat_threads` hold `request_first_at` and `request_last_at` (FR-002); a
  further knock updates `request_last_at` and, if newer, the card (FR-003; `mark_request` already keeps
  the newer card).
- **Rationale**: the requests view, its count, "Annehmen" through `chat_request_card` and the thread id the
  window opens all keep working; only the messages disappear.
- **Alternatives considered**: a table of its own for requests — duplicates the thread/member/card
  handling that `ThreadSidebar.vue` and `store::list` already do for `state = 'request'`.

## R3. Session state for knocks

- **Decision**: a knock confirms the decrypted session like a stored message (`decrypted.confirm`), so the
  accepted notice and the resent messages run on the same Olm session. With requests switched off the
  event takes the `dropped` path: a new session is not saved, an existing one stays advanced.
- **Rationale**: the notice is the recipient's first message to the sender; without the session the
  knock opened it would open a second session in the other direction. With requests off nothing of a
  stranger is kept, also no session per spam vault.
- **Alternatives considered**: never saving knock sessions — works through Olm pre-key messages but
  leaves two sessions per pair after an accept.

## R4. The accepted and blocked notices on the wire

- **Decision**: a receipt (`kind 24121`, spec 072 contracts/nostr-chat.md) gets an optional field
  `answer: "accepted" | "blocked"`; a notice is a receipt with empty `delivered` and `read`. It goes
  through the outbox and the ratchet like every receipt (spec 095). On the sender's side
  `receive::receipt` applies it to the direct thread with `receipt.from` ([contracts/nostr-chat-gate.md](contracts/nostr-chat-gate.md)).
- **Rationale**: a receipt is already "a signed word of the recipient vault about my messages", is already
  authenticated per device and card (FR-011), and the sender's devices already accept receipts only from
  stored cards — the sender has the recipient's card, because it wrote to it.
- **Alternatives considered**: a new event kind — a second path through verify, outbox and delivery for
  two words; a body type on `kind 24120` — messages are stored as rows, a notice is not.

## R5. Which device sends a notice, and to a blocked vault

- **Decision**: the device whose transaction ends a request or changes a block sends the notice. A
  device-local table `chat_gate_notices_no_sync` (peer vault, answer, created_at) is written in that
  transaction by `store::accept_requests_of` (accepted, FR-007, every route), by declining and blocking
  (blocked, FR-009, FR-014) and by unblocking (accepted, FR-016). The chat session turns its rows into
  receipts on the outbox (it has the device keys), like it turns pending receipts into receipts today.
  `targets.rs` refuses every target on the block list (`targets.rs:72`); an outbox row marked as a
  blocked notice is the one exception, addressed to the card the block entry or the hidden contact keeps.
- **Rationale**: the contacts code that stores a card (`received::save`) runs without the chat session's
  device keys; a synced column would make every own device send the same notice. The outbox is
  device-local by design (spec 095).
- **Alternatives considered**: enqueuing the receipt directly in `accept_requests_of` — needs the device
  keys inside the contacts transaction.

## R6. The sender's side of a notice

- **Decision**: `chat_threads` gets `peer_blocked_at` (synced). A blocked notice sets it, marks every
  unconfirmed delivery to that vault failed with reason `blocked` and parks their outbox rows (`next_at =
PARKED`, as `noDevices` does). While it is set, a new message to that vault is stored with the same
  state and parked instead of published (FR-010). An accepted notice clears it and calls the outbox's
  resend for every row awaiting that vault with `next_at = now` (FR-008).
- **Rationale**: the outbox already has parked rows and an explicit resend ("Erneut senden"); the state
  must survive a restart and show on all own devices.
- **Alternatives considered**: keeping the state only in the outbox — device-local, the other own devices
  would not show it.

## R7. The hint while nothing came back (FR-005a)

- **Decision**: the conversation listing gains `peerAnswered` (any incoming message, or any delivery to
  that vault with `delivered_at`) and the window shows the hint when `!peerAnswered` and an own message is
  older than 60 s without `delivered_at`. The minute is a window-side timer.
- **Rationale**: one query in `store::list`; no new state; never a claim the device cannot know.

## R8. Switching requests off (FR-012)

- **Decision**: vault preference `chat.requests.allowStrangers`, default `true`, checked in
  `validate_value` like the other chat settings. The window does not write it through the generic
  preferences command but through `chat_requests_set_allowed`, which writes the preference and deletes
  every request (`store::delete` per thread, as declining does today, without notice and without block)
  in one transaction.
- **Rationale**: the switch and the removal must not be split, and a request must not survive on another
  own device: the thread deletion syncs. Receiving reads the preference in the same transaction as R1.
- **Alternatives considered**: deleting requests when the preference row changes — the preference is
  written by a generic command that knows nothing of the chat.

## R9. Notification once per request (FR-004)

- **Decision**: the `arrived` event of `delivery_receive` carries a knock only when `mark_request` turned
  the thread into a request (it was not one before); `notify.rs` keeps its title-only, soundless path for
  requests.
- **Rationale**: `notify.rs` already treats `state == "request"` specially; only the trigger changes.

## R10. No receipt for a knock (FR-005)

- **Decision**: a knock returns `acknowledge = false` from `receive_one`; `send::receipt` and `targets.rs`
  no longer address a request's card (the branch "A request: the sender is no contact, so its receipts go
  to the card it carried", `targets.rs:76`) except for the notices of R5.
- **Rationale**: 072 US5 scenario 6 (_delivered_ for requests) is replaced by spec 099 US1 scenario 4.

## R11. Hiding a blocked contact (FR-014, clarification 2)

- **Decision**: a contact is hidden when one of its cards names a vault on the block list. `store::headers`
  (`src-tauri/src/contacts/store.rs:215`) already loads the cards of each contact
  (`received::with_cards`); it sets a new `ContactHeader.blocked` flag from the block list in the same
  read. `ContactsService::list`, `search`, `get` and `photo` (`service/contacts.rs:91-333`) leave blocked
  contacts out (`get` and `photo` answer `NotFound`), so the address book, the agent and MCP
  (`contacts/agent.rs`, `agent_actions.rs`) and extensions (`extensions/contacts.rs`, peer channels) lose
  them through the one service. The direct store readers of spec 060 (`share/preview.rs:183` hint
  candidates) filter the flag too. The files of spec 056 already skip blocked vaults
  (`files/peer/contacts.rs:84`).
- **Exception**: the phone address-book sync of spec 051 (`phone/run.rs`, `phone/sides.rs`,
  `phone/apply.rs`) keeps reading unfiltered. If it saw a hidden contact as missing it would delete the
  phone's copy, and unblocking would not restore what was on the phone. A blocked contact therefore
  stays in the phone's own address book (confirmed by the operator 2026-11-25).
- **Rationale**: one flag computed where the cards are loaded anyway; one filter in the service most
  readers already go through. No new column, so nothing to keep in sync with the block list.
- **Alternatives considered**: a `hidden` column on `haex_contacts` — a second truth next to the block list
  that sync conflicts could split; a SQL view — every reader builds its own SQL from `SELECT_CONTACT`.

## R12. The Visitenkarte a block entry keeps (FR-009, FR-016)

- **Decision**: `haex_contacts_blocked_vaults` gets a nullable `card` column (the signed card bytes, as
  `chat_members.card`). Declining a request stores the carried card there; blocking a stored contact
  leaves it empty (the contact keeps its cards). Entries made before this spec have none.
- **Rationale**: the block list is synced (CRDT) already, so the card reaches every own device with the
  block; the blocked notice of R5 and unblocking both need it for a vault that is no contact.

## R13. Unblocking (FR-016)

- **Decision**: one backend operation in `ContactsService` (`service/cards.rs:62` `set_blocked` today
  only removes the row): remove every block entry of the vault (and, for a hidden contact, of all vaults
  of that contact); if the entry kept a card and no contact holds the vault, create the contact from it
  through spec 060's `share/merge.rs` `apply` (after the entry is gone, since `apply` refuses blocked
  vaults); write an accepted notice (R5). A hidden contact needs nothing else: removing the entries makes
  it visible with all its data.
- **Rationale**: one transaction for list, contact and notice; reuse of the existing import of a card.

## R14. Blocking a contact (FR-014)

- **Decision**: `ContactsService` gains blocking of a contact: every vault of its cards goes on the block
  list (without card, R12), a blocked notice is written per vault (R5). The chat's "Sperren" in the
  conversation menu and the contact page both call it. `chat_request_answer` keeps `decline` only
  (`block` goes): it removes the request, blocks with the carried card and writes a blocked notice.
- **Rationale**: the contact page and the conversation are two entries to the same act.

## R15. Conversations of blocked vaults (FR-015, clarification 4)

- **Decision**: `store::list` (`chat/peer/store.rs:150`) and the chat search leave out direct threads whose
  `peer_vault` is on the block list. The blocked list in the settings opens a conversation by its thread
  id (`ids::direct_thread_id(own, vault)`), so "Verlauf ansehen" needs no list entry; the conversation
  view shows "Entsperren" in place of the composer when its peer is blocked (a `peerBlocked` field on the
  conversation).
- **Rationale**: the filter follows the block list, so unblocking brings the conversation back without a
  write.
