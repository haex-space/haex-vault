# Data Model: More secure contact chat

All new tables are `_no_sync`: they are never part of the sync between own devices. Tables that hold secrets are keyed by `installation_uuid`, and holzi reads and writes only the rows of its own installation ([research R9](research.md#r9-secret-key-state-is-stored-per-installation-and-never-synchronized)). Exact column names and types are fixed in tasks; the fields and rules below are binding. No existing synchronized table changes.

## Ratchet account — `chat_ratchet_account_no_sync`

One row per installation: the device's long-term part of the ratchet.

| Field                 | Meaning                                                                      |
| --------------------- | ---------------------------------------------------------------------------- |
| `installation_uuid`   | key; the installation the row belongs to                                     |
| `account`             | **secret**: the pickled Olm account (ratchet identity key, start keys)       |
| `start_key_made_at`   | when the current start key was made                                          |
| `record_published_at` | when the current chat key record was accepted by a Nostr-Relay; empty before |
| `record_relays`       | the Nostr-Relays that accepted it                                            |

Rules:

- Created on first use for an installation that has none. A copied vault file therefore gets a new account on the other device; rows of other installations are never read, changed or deleted.
- A new start key is made 30 days after `start_key_made_at`, only after every wrap in the fetched batches from reachable Nostr-Relays has been opened, processed and committed. A catch-up without a connected Nostr-Relay does not count. Making it drops the start key before the previous one. A start key remains available for at least 30 days after it stops being current, so an opening message remains decryptable through the full 30-day wrap lifetime even if a relay delays it.
- The account is written in the same transaction as the session it creates (an opening message uses the start key).

## Device session — `chat_ratchet_sessions_no_sync`

The key state between this installation and one peer device. Peer devices are devices of contacts and the own vault's other devices.

| Field               | Meaning                                                                                                                 |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `installation_uuid` | key part                                                                                                                |
| `peer_device`       | key part; the device key the session was opened with                                                                    |
| `session_id`        | key part; Olm's session id                                                                                              |
| `session`           | **secret**: the pickled Olm session                                                                                     |
| `created_at`        | when it was opened                                                                                                      |
| `last_received_at`  | when an event was last decrypted on it                                                                                  |
| `last_sent_at`      | when an event was last encrypted on it                                                                                  |
| `peer_identity_key` | the peer's ratchet identity key the session was opened with                                                             |
| `renewed_at`        | when this device made the newest ratchet key that the peer has answered with a new ratchet key of its own; empty before |

Rules:

- Every event on a session must be sealed by `peer_device`; otherwise it is dropped.
- Written in the same transaction as the message, receipt or outbox row the change belongs to.
- Sending uses the retained session with the lexicographically smallest Olm `session_id`; both endpoints therefore choose the same session once they have seen the same set. Incoming messages are accepted on any retained session. At most four sessions per peer device; the least recently used is deleted, with `session_id` as a tie-breaker.
- While the vault is open, a keep-alive is sent on the session this device sends on as soon as nothing was sent on it for 24 hours. A received keep-alive or receipt is never answered by a keep-alive at once ([contract](contracts/nostr-chat-ratchet.md#sending)).
- `renewed_at` is what recovery is reported from (FR-022): a key state copied at a time T is overcome once `renewed_at` is later than T. It moves only when a message with a new ratchet key of the peer arrives, and then to the time this device made the ratchet key that the peer answered. It never moves because events were counted.
- Operations on one session are serialized by a per-session lock keyed by `(installation_uuid, peer_device, session_id)`. The lock covers loading the latest pickle, decrypting or encrypting, persisting the advanced pickle with its message/outbox row and transaction commit, and updating or invalidating any in-memory copy. A caller reloads the pickle after acquiring the lock; it never commits an advance derived from a stale state.
- Deleted when the peer device left its vault's card or no conversation with its vault is left, when the peer's record or an accepted opening shows another ratchet identity key, and all sessions with a device when a reset with it is sent or accepted.

## Peer device state — `chat_ratchet_peers_no_sync`

What this installation remembers about a peer device beside its sessions.

| Field               | Meaning                                                                                    |
| ------------------- | ------------------------------------------------------------------------------------------ |
| `installation_uuid` | key part                                                                                   |
| `peer_device`       | key part                                                                                   |
| `reset_sent_at`     | when a reset was last sent to it                                                           |
| `reset_received_at` | when a reset from it was last accepted                                                     |
| `recovering_since`  | set by a reset in either direction; emptied when an event was decrypted on a newer session |
| `refetch_record`    | the stored record must be fetched again before a session is opened (after a reset)         |

Rules: a reset is sent, and one is accepted, at most once per hour and device. No secret is stored here.

## Chat key record

**Own record**: built from the account, not stored: a Nostr event signed by the device key with the ratchet identity key, the current start key and a validity that ends 30 days after the start key was made. Published to the Nostr-Relays on the own card (to the ones this device listens on if the card names none) after every new start key and when those relays change.

**Peer records — `chat_key_records_no_sync`**: what this device last accepted for a peer device. Public data, kept per device so that a Nostr-Relay cannot hand back an older record.

| Field          | Meaning                                |
| -------------- | -------------------------------------- |
| `peer_device`  | key; the device key that signed it     |
| `identity_key` | the peer's ratchet identity key        |
| `start_key`    | the peer's start key                   |
| `made_at`      | the record's time                      |
| `valid_until`  | after this, no session is opened on it |

Rules: accept a record only if its signer is a non-removed device on the vault's current card, it is not past `valid_until`, and its `made_at` is not older than the stored one. A record is needed only to open a session; existing sessions do not depend on it.

## Outbox — `chat_outbox_no_sync` (changed) and stored results

The outbox entry of an event now lives until every recipient vault confirmed delivery or the user deleted the message.

| Field (new)   | Meaning                                                     |
| ------------- | ----------------------------------------------------------- |
| `awaiting`    | recipient vaults whose delivery receipt has not arrived     |
| `accepted_at` | when a Nostr-Relay first accepted the wraps; empty before   |
| `resends`     | how often the event was encrypted again after `accepted_at` |

**Stored results — `chat_outbox_sealed_no_sync`**: per outbox entry and receiving device, the finished wrap that was made for it.

| Field              | Meaning                                             |
| ------------------ | --------------------------------------------------- |
| `outbox_id`        | key part                                            |
| `recipient_device` | key part                                            |
| `wrap`             | the kind 1059 event for that device (no secret)     |
| `accepted_at`      | when a Nostr-Relay accepted this wrap; empty before |

Rules:

- Encrypting for a device, saving the advanced session and inserting the stored result happen in one transaction.
- Until a wrap's `accepted_at`, retries publish that same wrap again (back-off as in spec 072, without the 24-hour end). A recipient vault leaves `pending` when a wrap was accepted for at least one of its devices and for every device that has a session or a usable record; the entry's `accepted_at` is the first accepted wrap.
- After 24 hours without any Nostr-Relay accepting, the delivery is shown as failed with its reason and is still retried; the failure is cleared when a Nostr-Relay accepts.
- After `accepted_at`, while `awaiting` is not empty, only user-created chat events are encrypted again for the devices of the awaiting vaults and published again, 1 day, 3 days and then every 7 days after the last attempt. Receipts and keep-alives are sent once and are not entries in this retry schedule.
- A reset from a device replaces that device's stored results of all entries whose vault is in `awaiting`.
- A device without a usable record has no stored result yet; it is tried again with every later attempt at the entry, also those for vaults in `awaiting`. A vault none of whose devices has a record stays in `pending`.

## Protection state (derived, not stored)

What the conversation view shows (US4, FR-013). Per conversation, the weakest state over the participants' devices on their current cards:

| State                 | Condition                                                                                                    |
| --------------------- | ------------------------------------------------------------------------------------------------------------ |
| `protected`           | a session exists with every device, and none is in `renewal outstanding`                                     |
| `renewal outstanding` | the last renewal with a device (`renewed_at`, else `created_at`) is more than 7 days ago                     |
| `waiting for keys`    | no session could be opened with a device yet; messages for it are pending                                    |
| `recovering`          | a reset with a device was sent or received and the new session has not carried an event (`recovering_since`) |

## Unchanged

`chat_threads`, `chat_members`, `chat_peer_messages`, `chat_attachments` and `chat_deliveries` keep their columns and stay synchronized between own devices. Message ids, deduplication and delivery states are those of spec 072.
