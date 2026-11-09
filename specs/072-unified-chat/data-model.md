# Data Model: One chat for the agent and for contacts

Migration `0049_chat_contacts` in `src-tauri/src/identity/migrations_chat_contacts.rs` (tests in
`migrations_chat_contacts_tests.rs`), `HOLZI_TRIGGER_VERSION` 30 → 31. Tables without `_no_sync` are
CRDT-synced to all own devices (spec 024); the CRDT columns are added by the transformer and are not
written in the SQL. No UNIQUE constraints on synced tables; uniqueness comes from derived ids
(`chat::ids`, UUIDv5 under a fixed `CHAT_NS`). All times are Unix milliseconds.

## Changed: `chat_threads` (conversation)

| Column         | Type                             | Notes                                                                         |
| -------------- | -------------------------------- | ----------------------------------------------------------------------------- |
| `kind`         | TEXT NOT NULL DEFAULT `'agent'`  | `agent`, `direct`, `group`, `note`; existing rows become `agent` (FR-003)     |
| `state`        | TEXT NOT NULL DEFAULT `'active'` | `active`, `request` (R10), `closed` (left/removed group)                      |
| `peer_vault`   | TEXT NULL                        | direct: the other vault identity (hex); NULL otherwise                        |
| `muted`        | INTEGER NOT NULL DEFAULT 0       | FR-008                                                                        |
| `last_read_at` | INTEGER NULL                     | everything with `sort_at` ≤ this is read (FR-022)                             |
| `picture`      | BLOB NULL                        | group picture, ≤ 9 KiB JPEG/PNG thumbnail; fits the 12 KiB base64 event limit |
| `meta_at`      | INTEGER NULL                     | group: sender time of the last rename/picture op (R9)                         |
| `meta_op`      | TEXT NULL                        | group: its op id (tie-break)                                                  |

`title` holds the group name; for direct conversations it is empty and the UI shows the contact's name
(or the carried card's name for a request). Ids: direct = `UUIDv5(CHAT_NS, "direct" ‖ sorted(own vault,
peer vault))`, so both sides and all devices derive the same id; group = random UUID from the creator;
note = `UUIDv5(CHAT_NS, "note:" ‖ lowercase hex of own vault)`, so all own devices derive the same
one (FR-002b). A note
conversation has the own vault as its only member; its messages are `chat_peer_messages` rows with the
own vault as sender and no `chat_deliveries` or outbox rows, so they reach the own devices only through
the CRDT sync.
Agent conversations keep their existing columns and behaviour.

Validation: `peer_vault` set iff `kind = direct`; `state = request` only for `direct` and `group`;
`kind = note` only with the id derived from the own vault.

## New: `chat_members` (direct and group members)

| Column      | Type             | Notes                                                                      |
| ----------- | ---------------- | -------------------------------------------------------------------------- |
| `id`        | TEXT PK          | `UUIDv5(CHAT_NS, thread_id ‖ vault)`                                       |
| `thread_id` | TEXT NOT NULL    | index                                                                      |
| `vault`     | TEXT NOT NULL    | vault identity hex; own vault included                                     |
| `card`      | BLOB NULL        | latest verified signed card of this vault (R5, R9); NULL for the own vault |
| `state`     | TEXT NOT NULL    | `invited`, `member`, `declined`, `left`, `removed`                         |
| `state_at`  | INTEGER NOT NULL | sender time of the winning op (R9)                                         |
| `state_op`  | TEXT NOT NULL    | op id of the winning op (tie-break)                                        |
| `state_by`  | TEXT NOT NULL    | vault that made the change (shown in notes, FR-053)                        |
| `joined_at` | INTEGER NULL     | first time `member`; messages before are never sent to it                  |

State transitions (per member, last `(state_at, state_op)` wins):

```text
(none) --add--> invited --accept--> member --leave--> left
                   |                   \--remove--> removed
                   \--decline--> declined     (add again → invited)
```

## Group name and picture

Rename and picture use the same LWW rule on the thread row: an op whose `(at, op_id)` is older than
`(meta_at, meta_op)` is ignored.

## New: `chat_peer_messages` (direct and group messages, notes)

| Column            | Type             | Notes                                                                      |
| ----------------- | ---------------- | -------------------------------------------------------------------------- |
| `id`              | TEXT PK          | `UUIDv5(CHAT_NS, thread_id ‖ sender_vault ‖ event id)` (R4)                |
| `thread_id`       | TEXT NOT NULL    | index `(thread_id, sort_at, id)`                                           |
| `event_id`        | TEXT NOT NULL    | the rumor's `id` (hex, 128 bit)                                            |
| `idempotency_key` | TEXT NULL        | sender's request UUID; reused on retries to return the original message    |
| `sender_vault`    | TEXT NOT NULL    | own vault for own messages                                                 |
| `sender_device`   | TEXT NOT NULL    | device key hex                                                             |
| `kind`            | TEXT NOT NULL    | `text`, `note` (membership/rename), `contact` (060 share)                  |
| `text`            | TEXT NULL        | ≤ 4,000 characters (R2)                                                    |
| `note`            | TEXT NULL        | JSON for `kind = note`: `{op, by, member?, name?}`                         |
| `sent_at`         | INTEGER NOT NULL | sender's time                                                              |
| `received_at`     | INTEGER NOT NULL | arrival on the inserting device (own: = `sent_at`)                         |
| `sort_at`         | INTEGER NOT NULL | `min(sent_at, received_at + 300000)` (R4)                                  |
| `inserted_by`     | TEXT NOT NULL    | device uuid that inserted the row (R6: only it sends the delivery receipt) |

## New: `chat_attachments`

| Column        | Type          | Notes                                                              |
| ------------- | ------------- | ------------------------------------------------------------------ |
| `id`          | TEXT PK       | `UUIDv5(CHAT_NS, message id ‖ index)`                              |
| `message_id`  | TEXT NOT NULL | index                                                              |
| `kind`        | TEXT NOT NULL | `file` or `contact`                                                |
| `name`        | TEXT NULL     | file name (sanitised, ≤ 255 bytes)                                 |
| `mime`        | TEXT NULL     |                                                                    |
| `size`        | INTEGER NULL  | plaintext bytes, ≤ 100 MB (FR-034)                                 |
| `sha256`      | TEXT NULL     | plaintext hash                                                     |
| `blossom_url` | TEXT NULL     | full blob URL                                                      |
| `blob_sha256` | TEXT NULL     | ciphertext hash (Blossom address)                                  |
| `blob_key`    | BLOB NULL     | 32-byte file key (R11); vault data, encrypted at rest by the vault |
| `contact`     | TEXT NULL     | `kind = contact`: the 060 share text (`haex-vault://contact/…`)    |
| `photo`       | BLOB NULL     | `kind = contact`: full-size photo (FR-060)                         |

Whether a device holds a file is device-local (`chat_attachment_files_no_sync` below); transfer
progress is runtime state (events, R18).

## New: `chat_deliveries` (own messages, per recipient vault)

| Column            | Type          | Notes                                                                                   |
| ----------------- | ------------- | --------------------------------------------------------------------------------------- |
| `id`              | TEXT PK       | `UUIDv5(CHAT_NS, message id ‖ recipient vault)`                                         |
| `message_id`      | TEXT NOT NULL | index                                                                                   |
| `recipient_vault` | TEXT NOT NULL |                                                                                         |
| `sent_at`         | INTEGER NULL  | a Nostr-Relay of the recipient accepted                                                 |
| `delivered_at`    | INTEGER NULL  | delivery receipt                                                                        |
| `read_at`         | INTEGER NULL  | read receipt (FR-024)                                                                   |
| `files_at`        | INTEGER NULL  | a device of the recipient stored every file of the message (shown as "Datei empfangen") |
| `failed_at`       | INTEGER NULL  |                                                                                         |
| `failed_reason`   | TEXT NULL     | `noRelay`, `noDevices`, `tooLarge`, `blossom:<code>`                                    |

Derived state: `failed` if `failed_at` and no later `sent_at`; else `read` > `delivered` > `sent` > `waiting`.
"Erneut senden" clears `failed_*` and re-queues (outbox).

## New device-local tables (`_no_sync`)

**`chat_outbox_no_sync`** — events still to publish from this device.

| Column       | Type             | Notes                                                           |
| ------------ | ---------------- | --------------------------------------------------------------- |
| `id`         | TEXT PK          | event id                                                        |
| `thread_id`  | TEXT NOT NULL    |                                                                 |
| `message_id` | TEXT NULL        | NULL for receipts and group ops without a message row           |
| `rumor`      | TEXT NOT NULL    | the rumor JSON (wrapped fresh per attempt)                      |
| `pending`    | TEXT NOT NULL    | JSON list of recipient vaults not yet accepted by a Nostr-Relay |
| `attempts`   | INTEGER NOT NULL |                                                                 |
| `next_at`    | INTEGER NOT NULL | back-off (R6)                                                   |
| `last_error` | TEXT NULL        |                                                                 |
| `created_at` | INTEGER NOT NULL | 24 h without success → `failed`                                 |

**`chat_wraps_seen_no_sync`** — `(wrap_id TEXT PK, seen_at INTEGER)`, pruned after 30 days.

**`chat_relay_cursors_no_sync`** — `(relay TEXT PK, newest_created_at INTEGER)` for `since` (R3).

**`chat_blobs_no_sync`** — uploads of this device: `(blob_sha256 TEXT PK, server TEXT, signer TEXT,
upload_secret BLOB NULL, key_entry_id TEXT NULL, attachment_id TEXT, pending_devices TEXT,
uploaded_at INTEGER)`. `signer` is `per_blob` or `registered`; per-blob uploads keep their signing key in
`upload_secret`, while registered-key uploads store only the password-manager entry id in
`key_entry_id` (never the secret itself). `pending_devices` lists every recipient device at send time
and the sender's other devices whose `files` receipt is still missing; row removed after `DELETE` (R13).

**`chat_attachment_files_no_sync`** — files on this device: `(attachment_id TEXT PK, state TEXT
(`pending`, `downloading`, `stored`, `failed`, `gone`), bytes_done INTEGER, error TEXT, updated_at
INTEGER)`. `pending` above the automatic download limit waits for a tap (FR-036).

**`chat_group_parts_no_sync`** — incomplete invitation parts: `(group_id, part INTEGER, part_count INTEGER,
payload TEXT, received_at)`, PK `(group_id, part)`; dropped after 7 days or when complete (R9).

## Preferences (`preferences`, typed accessors in `chat::settings`)

| Key                              | Scope  | Default   | Notes                                                              |
| -------------------------------- | ------ | --------- | ------------------------------------------------------------------ |
| `chat.blossom.server`            | vault  | `none`    | URL or `none`; a preset is enabled only after T104 passes (FR-080) |
| `chat.blossom.keyEntry`          | vault  | unset     | id of the password entry holding the upload key (R12)              |
| `chat.receipts.read`             | vault  | `true`    | FR-024                                                             |
| `chat.download.autoLimit`        | device | `25` (MB) | FR-036                                                             |
| `chat.download.autoLimitMetered` | device | `5` (MB)  | Android on mobile data                                             |
| `chat.notifications`             | device | `true`    | FR-023                                                             |

Device-scoped keys are added to `DEVICE_PREFERENCE_KEYS` (storage/maintenance.rs:70). An invalid stored
value reads as the default (pattern passwords/settings.rs:12-34).

## Agent permissions

`agent_file_permissions` rows with the new kind `contact_chats` and target `*` or `thread:<id>`
(files/permissions.rs:28-36); no schema change.

## Frontend types (ts-rs, `src/types/bindings/`)

`ChatConversation` (the list entry: id, kind, state, title, peerVault, peerWithoutRelays, picture, muted, unread,
lastMessage, updatedAt), `ChatPeerMessage`, `ChatAttachment`, `ChatDeliveryState`, `ChatMember`,
`ChatSettings`, `ChatTransferEvent`. The hand-written `Thread`/`Message` in `src/composables/useChat.ts`
stay for agent conversations; `Thread` gains `kind` and `state`.
