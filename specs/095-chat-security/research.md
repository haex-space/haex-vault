# Research: More secure contact chat

Terms: a **device session** is the key state between two devices (spec.md, Key Entities). A **ratchet** is a key state that moves forward with every message and deletes what it no longer needs. **Wrap**, **seal**, **rumor**, **card** and **device key** are used as in [spec 072](../072-unified-chat/contracts/nostr-chat.md).

## R1: One Double Ratchet session per pair of devices, inside the existing wraps

**Decision**: Every pair of devices that exchange chat events keeps a Double Ratchet session. The content of every chat event is encrypted on that session and then sealed and gift-wrapped exactly as today, one wrap per receiving device. Group membership, addressing and deduplication stay as spec 072 defines them.

**Rationale**:

- haex-vault already sends one wrap to each device on the recipients' cards and to the sender's own other devices ([072 research R4](../072-unified-chat/research.md), [ADR 0024](../../docs/adr/0024-chat-over-nostr-and-blossom.md) decision 1). A session per device pair replaces only the long-lived key inside that wrap.
- The Double Ratchet gives a fresh key per message (forward secrecy) and new key material with every answer (recovery after a copied key state). Both work with stored events and an offline recipient.
- Groups need no shared key state. A removed member is no longer addressed, as today, so no agreement on an order of membership changes is needed and ADR 0024 decision 4 (ops with last-writer-wins, no coordinator) stays.
- Nostr-Relays see what they see today: kind 1059 events to device keys.

**Alternatives considered**:

- **MLS (RFC 9420)**, the first draft of this plan. Rejected: MLS needs every member to apply membership commits in one order. Without a server, clients must pick a winner among concurrent commits; NIP-EE does it by lowest `created_at`, then lowest event id, and keeps old state for a while to repair forks. haex-vault blurs wrap times by up to 48 hours and decided against a coordinator (ADR 0024 decision 4). MLS also needs one-time key packages that an untrusted relay cannot hand out exactly once, a rule for who may add or remove a device, and a routing identifier per conversation that relays would see. Its gains, one ciphertext for a large group and a member list all members agree on cryptographically, are not needed for groups of at most 32 vaults that already fan out per device. [Spec 072 research R9](../072-unified-chat/research.md) had ruled MLS / NIP-EE out for the same size reason.
- **A receiving key per device that changes weekly, without a ratchet**. Rejected: forward secrecy only by the week, and a protocol of our own instead of a reviewed one.
- **A shared sender key per group (Megolm, Signal sender keys)**. Rejected: saves fan-out haex-vault already pays and weakens recovery after compromise, because a sender key is replaced only on membership change.
- **Keep the long-lived device keys**. Rejected by the operator: a device key copied once would read all later and all archived traffic to that device.

## R2: vodozemac's Olm as the ratchet

**Decision**: Use the Olm implementation of the `vodozemac` crate (Apache-2.0), pinned to an exact version, 0.11.2 or later. Use only its Olm part: `Account` (identity key, start key), `Session` (encrypt, decrypt) and their serializable pickles. Megolm, SAS and the Matrix key formats are not used.

**Rationale** (checked against the sources below on 2026-11-17):

- Olm is a Double Ratchet with a three-way key exchange for sessions between two devices, which is exactly R1.
- The crate is pure Rust and had one external audit (Least Authority, 2022).
- An account keeps two start keys, the current and the previous one (`generate_fallback_key`, `forget_fallback_key`), which is what R3 needs.
- A review published in February 2026 criticised that Olm accepted a Diffie-Hellman result of all zeroes. Since 0.10.0 `Curve25519SecretKey::diffie_hellman()` returns an `Option`. Release 0.11.2 (2026-11-05) fixes three further advisories (a panic on malformed base64 keys, zeroization in dependencies, zeroization of the root-key ratchet output).
- A session accepts messages that arrive out of order: up to 40 skipped message keys per receiving chain, five receiving chains, and a message at most 2,000 ahead of the last one. R8 keeps haex-vault inside these limits.

**Limits to plan around**:

- The default session config (version 1) shortens the message authentication tag to 8 bytes; version 2 with the full tag is behind the crate's `experimental-session-config` feature. Every haex-vault chat event is additionally signed by the sender's device key in the seal and authenticated by NIP-44 twice, so the short tag is not the only check. Decided: version 1, the default that was audited and is in use; the experimental feature stays off.
- Pickles hold secrets. They are stored in the vault database, which SQLCipher encrypts; vodozemac's own pickle encryption is not needed on top.

**Alternatives considered**:

- **libsignal**: licensed under AGPL-3.0. Rejected for the licence.
- **A Double Ratchet written for haex-vault on the secp256k1 keys it already has**. Rejected: a cryptographic protocol of our own.

## R3: A device publishes a signed chat key record so others can start a session while it is offline

**Decision**: Every device publishes one **chat key record** to the Nostr-Relays on its vault's card: a replaceable Nostr event signed by its **device key** that carries the device's ratchet identity key (Curve25519), its current start key (Olm's fallback key) and a time until which it is valid (30 days after it was made). A sender that has no session with a device fetches that device's record from the device's Nostr-Relays, checks it and opens the session with it. The first message travels in the same wrap as the opening.

- The record is trusted because its event is signed by a device key that is on the vault's current card, and the card is signed by the vault identity. No change to the card or to the device list is needed, and linked devices need no main device to publish it.
- A device makes a new start key every 30 days while its vault is open, after every wrap in the fetched batches from reachable Nostr-Relays has been opened, processed and committed, and publishes a new record. It forgets the start key before the previous one at the same moment, so each start key remains available for at least 30 days after it stops being current. This covers the full 30-day wrap lifetime even when a relay delays an opening message.
- A sender refuses a record that is past its validity, that is older than one it already holds for that device, or whose signer is not a non-removed device on the card.
- Without a usable record the message stays in the outbox for that device and the record is fetched again with each retry. It is never sent under the long-lived device key instead (FR-016).

**Rationale**: the first message to a device must not need the device to be online (FR-003); so the keys to open a session have to lie where stored messages lie. A relay that withholds or replays a record can only delay delivery to that device, which it can do anyway by withholding the message. Changing the start key is what makes the first messages of a session forward-secret: once the start key is forgotten, the opening cannot be recomputed from the device's remaining keys. Since wraps remain valid for 30 days, the old start key is retained for at least that long after it is no longer current.

**What relays learn in addition**: that a device key takes part in the chat and when its record was renewed. Device keys are already visible as recipients of wraps. The record names no vault, no contact and no conversation.

**Alternatives considered**:

- **Keys on the card**: only a main device signs cards ([own_card.rs](../../src-tauri/src/contacts/card/own_card.rs)), a card is at most 2,048 characters, and a weekly change would mean a new card every week. Rejected.
- **Hand the keys over inside chat events only**: the first message to a new contact or a new device would wait for an answer from that device and for the sender to be online again. Rejected against FR-003.
- **One-time keys per session (as in Signal or NIP-EE key packages)**: a Nostr-Relay cannot hand a key out exactly once, and two senders taking the same key is the normal case. Rejected; the 30-day start key takes their place. The cost: the messages of a session's first flight, until the receiving device answers, stay decryptable with that device's keys until it forgets the start key (at most one additional 30-day retention interval after it ceases to be current).

## R4: The device key stays the anchor of trust

**Decision**:

- The seal (kind 13) stays signed by the sender's device key. An opening message contains the sender's ratchet identity key, so the seal's signature binds that key to the sender device. A session is stored with the device key it was opened with, and every later event on it must be sealed by that device key.
- The encrypted content is the spec 072 envelope with `from`, `to` and the sender's card. The receiver runs the checks of the 072 contract unchanged after decrypting, so both vault identities are bound into the first message of every session.
- If a device's record shows a new ratchet identity key (the device lost its state), senders accept it because the device key signed it, open a new session and drop the old ones with that device.

**Rationale**: haex-vault's existing chain of trust (vault identity → card → device key) stays the only one. The ratchet identity key is never shown to the user and needs no verification of its own.

## R5: Transport and sizes

**Decision**: A new rumor kind carries the ratchet message (see [contracts/nostr-chat-ratchet.md](contracts/nostr-chat-ratchet.md)); its plaintext is the kind and content of a spec 072 chat event or receipt. A second rumor kind without secret content tells a sender that a session must be opened again (R7). Rumors of kinds 24120 and 24121 that are not inside a ratchet message are dropped.

**Measured (2026-11-17)**: the ratchet adds one base64 layer below the two NIP-44 layers. A plaintext of 24 KiB gives a wrap of 76,993 bytes, above the 60 KiB wrap limit; NIP-44's padding steps make the wrap jump from 55,149 to 66,073 bytes. The largest plaintext whose wrap stays within 60 KiB is 21,215 bytes for an opening message. The limit for the plaintext of a ratchet message is therefore 20 KiB (20,480 bytes), instead of 24 KiB for a rumor in spec 072. Group invitation parts (spec 072 US4, not built yet) have to be cut by size, about 9 members a part instead of 12.

## R6: Keys are renewed without the user

**Decision**:

- A session's keys change in one direction with every message and are replaced by new key material when the other device answers. Each device proactively sends an empty ratchet event (**keep-alive**) on every active session as soon as it sent nothing on it for 24 hours while its vault is open, even if no recent event arrived. Receipts and messages count as answers. A keep-alive request gets one immediate response with a distinct kind that never triggers another response; receipts do not trigger an immediate keep-alive. Recovery is claimed only after each endpoint has generated and received fresh DH ratchet contributions after a copied state, completing both sides of the ratchet progression; message counts alone do not prove recovery. This may take up to 48 hours with both vaults open.
- Start keys are renewed every 30 days (R3), and the previous key remains available for the full 30-day wrap lifetime after it ceases to be current.
- When a device is removed from a card or a vault leaves a group, the sessions with those devices are deleted once no conversation needs them.

**Rationale**: only one device per vault sends the delivery receipt ([072 research R6](../072-unified-chat/research.md)); without the keep-alive the other devices' sessions would never recover from a copied key state (FR-022, SC-010).

**Known ceiling**: in a busy group of 32 vaults with three devices each, a device sends up to about 95 keep-alives a day. Upgrade path if that is too much: a longer interval for group-only peers.

## R7: Recovery when an event cannot be decrypted

**Decision**:

- A device that cannot decrypt a ratchet event from a device on a known vault's card (no such session, a start key it has forgotten, a message key it no longer has) answers with a **reset** event to that device, sealed by its device key, at most once per peer device per hour. Events from unknown devices are dropped silently, as today.
- A device that receives a reset fetches the peer's record, opens a new session and encrypts again every event to that device's vault that has no delivery receipt yet.
- Both sides drop every session with the other device, the sender of the reset when it sends it and the receiver when it accepts it. The sending session is chosen by the smallest session id (below); a session that one side can no longer use could otherwise stay the chosen one.
- A wrap that was seen before or is past its expiration never reaches a session, so a Nostr-Relay cannot cause resets by replaying old wraps.
- Opening a device with an older copy of its own vault file is the same case: its sessions are behind, the peers cannot decrypt what it sends or it cannot decrypt what arrives, and the reset heals both directions. No counter outside the vault file is kept to detect this.
- If two devices open sessions with each other at the same time, both sessions are kept. A device sends on the retained session with the lexicographically smallest shared Olm session ID; both endpoints make the same choice after seeing the same set. It keeps at most four sessions per peer device and deletes the least recently used, breaking ties by session ID.

**Rationale**: the sender still holds the plaintext of its messages, so a broken session costs a new key exchange and never a message (FR-023, SC-011). Events are deduplicated by their id as in spec 072.

**Known ceiling**: events the vault already confirmed are not sent again to a device that reset; it gets those messages through the sync between own devices. Upgrade path: a per-device request for the events of the last days.

## R8: Delivery and retry

**Decision**:

- An event is encrypted once per receiving device, and the finished wrap is stored with the outbox entry in the same transaction that saves the advanced session. Retries until a Nostr-Relay accepts publish that same wrap again, so failed attempts use up no message keys and the receiver never gets one ratchet message in two wraps.
- A message that arrives again more than 12 hours after it was stored is a resend; the receiver acknowledges it again, so a lost receipt ends the sender's resending.
- _Sent_ and _delivered_ keep their meaning from spec 072: a Nostr-Relay accepted the wrap; a receipt from the recipient vault arrived. The receipt is now authenticated by the session as well.
- A message without a delivery receipt is no longer given up after 24 hours. It is encrypted again on the current session and published again at growing intervals (1 day, 3 days, then weekly) until a receipt arrives or the user deletes it (FR-021). The 30-day expiration of each single wrap stays.
- A device opens the seals of a fetched batch first and decrypts ratchet events per session in the order of their message counters, so a batch that arrives in any order stays within the 40 skipped keys.

**Rationale**: a relay's acceptance proves nothing about delivery (spec.md US3). Encrypting again, instead of sending the old ciphertext, also covers a receiver that meanwhile reset its session.

## R9: Secret key state is stored per installation and never synchronized

**Decision**: The ratchet account and the sessions live in `_no_sync` tables keyed by `installation_uuid`, like `device_keys_no_sync` ([keys.rs](../../src-tauri/src/sync/keys.rs)). haex-vault reads and writes only the rows of its own installation.

- A vault file copied to another device finds no rows for that installation and creates an account of its own; the copied rows are never used (SC-007). They stay in the copy. The operator accepted that on 2026-11-17: whoever can open the copy reads the kept history anyway, and deleting other installations' rows would break using one vault file on two computers in turn.
- This departs from the naming rule of [ADR 0001](../../docs/adr/0001-device-scoped-data-convention.md), which puts per-device data into synchronized tables. Synchronizing would give every own device the secrets of all others and would keep old keys alive on devices that are offline. [ADR 0026](../../docs/adr/0026-chat-ratchet-per-device-pair.md) records the exception.
- Every change of a session is written in the same transaction as the message, receipt or outbox row it belongs to. All operations on one session are serialized by a per-session lock from loading the latest pickle through crypto, transaction commit and cache update/invalidation; a caller reloads after taking the lock, so concurrent sends, receives and keep-alives cannot commit advances from the same predecessor state.
- Erasure uses what spec 022 set up: `secure_delete = ON` overwrites freed pages, and the write-ahead log is truncated when the vault closes ([maintenance.rs](../../src-tauri/src/storage/maintenance.rs)). A key counts as erased once the vault was closed after its deletion. The same holds for a deleted message, so the keys are never easier to recover than the text they protected.
- haex-vault has no backup of the vault database of its own. A copy the user makes holds the key state of that moment, protected by the vault password.

## R10: New devices, own devices and groups

**Decision**: unchanged from spec 072, with sessions added.

- A new device of a contact appears on the contact's newer card; from then on senders fetch its record and address it. Earlier relay copies were never encrypted to it.
- A new own device gets the history through the sync between own devices, which already carries the chat tables. There is no separate history transfer.
- Own other devices are peer devices like any other: one session each.
- A vault added to a group gets no earlier messages; a removed vault is no longer addressed by every sender that applied the removal.

## R11: What carries over from haex-vault-legacy's MLS code

**Decision**: the protocol does not carry over; three lessons do.

- Keys for joining were made ahead of time and refilled without the user. Here: the chat key record and its 30-day renewal (R3).
- A cryptographically valid message was not treated as an authorized one; authorization was checked against the application's own facts. Here: the 072 receiver checks against the card run after decryption (R4).
- Consuming a Welcome was separated from marking it consumed, so a crash lost nothing. Here: session and message are written in one transaction (R9), and a lost session heals by reset (R7).

Source: [`haex-vault-legacy` at `8dce379d94e18fcd42c3b73686a06f984ca3f574`](https://github.com/haex-space/haex-vault-legacy/tree/8dce379d94e18fcd42c3b73686a06f984ca3f574), `src-tauri/src/mls/` and `src-tauri/tests/mls_lifecycle.rs`.

## Checks done for tasks

Done on 2026-11-17 with a throwaway crate against `vodozemac` 0.11.2 and `nostr` 0.45, building the same three layers as `sync::gift_wrap`.

- **vodozemac**: its minimum Rust version is 1.89 (haex-vault: 1.95). It compiles for `aarch64-linux-android` and `armv7-linux-androideabi`, also with `default-features = false`. It uses `curve25519-dalek` 5, `ed25519-dalek` 3 and `sha2` 0.11, the major versions haex-vault already has. Session config: version 1 (R2).
- **Behaviour the plan relies on**, each observed: an opening on the previous start key still decrypts after one renewal and fails after two; a sender keeps sending opening messages until it decrypted an answer; both ends get the same session id; a message's ratchet key and counter can be read without decrypting it; of 60 messages delivered in reverse order only 41 decrypt (40 skipped keys); a message delivered twice fails the second time.
- **Kind numbers**: 24122, 24123 and 10120 (and spec 072's 24120, 24121) are not in the [NIP kind list](https://github.com/nostr-protocol/nips/blob/master/README.md).
- **Sizes**: plaintext limit 20 KiB (R5). A keep-alive wrap is 1,729 bytes, a reset wrap 1,561 bytes, a pickled session about 1.8 KiB.
- **Keep-alive volume**: one wrap per peer device with a session and day. A device in a full group of 32 vaults with three devices each sends about 95 a day, about 165 KiB. Accepted; the upgrade path of R6 stays.
- **Write-ahead log**: truncating on close stays the rule (R9). While a vault is open, whoever reads its file also reads the kept history; a truncating checkpoint in between would not change what SC-001 states.
- **Erasure test**: this build of SQLite has no page-level access (`sqlite_dbpage`) to an encrypted file. Quickstart scenario 9 therefore proves the erasure on an unencrypted database with the same settings and checks on the vault that those settings are on.

## Sources

- [Signal: The Double Ratchet Algorithm](https://signal.org/docs/specifications/doubleratchet/)
- [Signal: The Sesame Algorithm](https://signal.org/docs/specifications/sesame/), for sessions between several devices
- [vodozemac](https://github.com/matrix-org/vodozemac): README, `CHANGELOG.md`, `src/olm/account/mod.rs`, `src/olm/session/receiver_chain.rs` (read 2026-11-17)
- [Least Authority: vodozemac final audit report, 2022](https://leastauthority.com/static/publications/LeastAuthority-Matrix_vodozemac_Final_Audit_Report.pdf)
- [Soatok: Cryptographic Issues in Matrix's Rust Library Vodozemac, 2026-02-17](https://soatok.blog/2026/02/17/cryptographic-issues-in-matrixs-rust-library-vodozemac/)
- [NIP-EE](https://github.com/nostr-protocol/nips/blob/master/EE.md), for how MLS over Nostr orders commits and handles key packages
- [RFC 9420: The Messaging Layer Security Protocol](https://www.rfc-editor.org/rfc/rfc9420.html)
- [NIP-44](https://github.com/nostr-protocol/nips/blob/master/44.md) and [NIP-59](https://github.com/nostr-protocol/nips/blob/master/59.md)
- [ADR 0024: Chat over Nostr and Blossom](../../docs/adr/0024-chat-over-nostr-and-blossom.md)
