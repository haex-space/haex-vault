# Data model: Only contacts may write in the chat

Changes to the vault database. Synchronized tables are CRDT tables of `haex-crdt`; `_no_sync` tables stay
on the device. No data is migrated (no users of the old request behaviour).

## `chat_threads` (synchronized, changed)

| Column             | Type         | New | Meaning                                                                                          |
| ------------------ | ------------ | --- | ------------------------------------------------------------------------------------------------ |
| `state`            | TEXT         |     | `'request'` now means: a stranger knocked; the thread has no message rows (research R2)          |
| `request_first_at` | INTEGER NULL | yes | First knock of the request (ms); set when the thread becomes a request                           |
| `request_last_at`  | INTEGER NULL | yes | Latest knock (ms); updated by every further knock (FR-003)                                       |
| `peer_blocked_at`  | INTEGER NULL | yes | Sender side: when the peer's blocked notice arrived; cleared by an accepted notice (research R6) |

State transitions of a direct thread on the receiving vault:

```text
(none) --knock, requests allowed--> request
request --"Annehmen" / card stored any way--> active        (accepted notice, R5)
request --"Ablehnen"--> (deleted)                            (block entry with card, blocked notice)
request --requests switched off--> (deleted)                 (nothing else)
active --peer blocked--> active, hidden from list/search     (block entry, blocked notice; R15)
hidden --unblocked--> active, listed again                   (accepted notice)
```

Sender side: `peer_blocked_at` NULL → set by a blocked notice → NULL by an accepted notice.

## `chat_peer_messages` / `chat_attachments` (unchanged)

A knock writes no rows here (FR-002). Resent messages after an accept land on the row their derived id
names (spec 072 research R4), in the order of `sent_at`.

## `chat_deliveries` (unchanged columns, new reason)

`failed_reason` gains `'blocked'`: set on every unconfirmed delivery to a vault that sent a blocked notice,
and on every message written while `peer_blocked_at` is set (FR-010). Cleared like other reasons when the
message is sent again after an accepted notice.

## `chat_outbox_no_sync` (device-local, unchanged columns)

Rows to a vault with `peer_blocked_at` are parked (`next_at = PARKED`). An accepted notice sets `next_at =
now` on every row whose `awaiting` holds that vault (FR-008). A row carrying a blocked notice is the one row
`targets.rs` addresses to a blocked vault (research R5).

## `chat_gate_notices_no_sync` (device-local, new)

| Column       | Type    | Meaning                                                 |
| ------------ | ------- | ------------------------------------------------------- |
| `peer_vault` | TEXT PK | The vault the notice goes to (hex)                      |
| `answer`     | TEXT    | `'accepted'` or `'blocked'`; a later answer replaces it |
| `created_at` | INTEGER | When the answer was given (ms)                          |

Written in the transaction that changes the request or the block (research R5); the chat session turns a
row into a receipt on the outbox and deletes it.

## `haex_contacts_blocked_vaults` (synchronized, changed; spec 051)

| Column         | Type      | New | Meaning                                                                                   |
| -------------- | --------- | --- | ----------------------------------------------------------------------------------------- |
| `vault_pubkey` | TEXT PK   |     | The blocked vault (hex)                                                                   |
| `blocked_at`   | TEXT      |     | When (as today)                                                                           |
| `card`         | BLOB NULL | yes | The signed Visitenkarte of a declined stranger (research R12); NULL for a blocked contact |

## `ContactHeader` (no column; computed)

`blocked: bool` — one of the contact's cards names a vault on the block list (research R11). Set by
`store::headers`; `ContactsService` leaves such contacts out of `list`, `search`, `get`, `photo`.

## Vault preference `chat.requests.allowStrangers` (new)

`"true"` | `"false"`, default `"true"`; checked in `validate_value`; written only through
`chat_requests_set_allowed` (research R8).

## `ChatConversation` binding (changed)

| Field           | New | Meaning                                                                        |
| --------------- | --- | ------------------------------------------------------------------------------ |
| `peerAnswered`  | yes | Direct: any incoming message or any delivery with `delivered_at` (research R7) |
| `peerBlocked`   | yes | Direct: the peer vault is on the own block list (research R15)                 |
| `blockedByPeer` | yes | Direct: `peer_blocked_at` is set (research R6)                                 |
| `requestLastAt` | yes | Request: latest knock (ms)                                                     |
