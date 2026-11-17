# Research: More secure contact chat

## Decision 1: Use MLS as the candidate protocol for direct and group chat

**Decision**: Use the Messaging Layer Security protocol (MLS, RFC 9420) as the architecture basis for both direct and group conversations, treating each Holzi installation/device as a separate group member. Bind each MLS credential to a device key and the contact-verified vault identity. Keep authorization of devices and group membership in Holzi's signed identity/contact model; MLS authenticates the cryptographic group state after that trust check.

**Rationale**: RFC 9420 explicitly defines asynchronous group key establishment with forward secrecy and post-compromise security for groups from two to thousands. It covers one-to-one conversations as two-member groups, allowing one security state machine across direct and group chat. MLS message keys are derived across epochs and message generations and are expected to be erased after use.

**Alternatives considered**:

- Keep NIP-44 with a custom per-message key schedule: rejected because persistent recipient public keys alone cannot supply fresh shared entropy to offline peers, and bespoke group key management would create a large security-critical protocol.
- Signal Double Ratchet for direct messages plus a separate group protocol: rejected as the initial architecture because it creates two protocol/state families and still requires careful asynchronous multi-device session management. Signal's Sesame specification explicitly addresses asynchronous session management, but does not provide a single standard group state machine matching this scope.
- Pairwise Double Ratchet sessions for every group member: rejected as a bespoke fan-out design with duplicated state and group membership/key-refresh complexity.

## Decision 2: Keep Nostr as an untrusted delivery service

**Decision**: Nostr transports opaque MLS application messages, proposals/commits, and device key-package advertisements. MLS application payloads, including chat text and any attachment key material, remain protected end to end. Nostr relay acceptance is only a transport receipt; recipient delivery requires an authenticated MLS-level acknowledgement.

**Rationale**: MLS assumes a delivery service that can control message delivery, while the cryptographic protocol detects invalid or unauthorized messages. Holzi already uses Nostr for asynchronous store-and-forward and stores local conversation history. Replacing the transport is not required to obtain the requested confidentiality property.

**Alternatives considered**:

- Require relay NIP-42 authentication: rejected as unrelated to message confidentiality, forward secrecy, and relay availability.
- Move chat to direct Iroh connections only: rejected because it breaks offline delivery and the existing product requirement that messages arrive while the recipient vault is closed.

## Decision 3: Persist key erasure as a security requirement

**Decision**: Persist MLS state through a storage provider that supports atomic updates with message/outbox changes. Treat key deletion as an irreversible security operation. Do not copy ratchet state into CRDT-synchronized data, logs, diagnostic reports, or ordinary backups. Define how the encrypted vault database, SQLite journaling/WAL, snapshots, and backup/restore behavior satisfy the erasure claim before implementation.

**Rationale**: OpenMLS's persistence guidance says its storage provider holds sensitive key material and must irrevocably delete values requested for deletion without retaining copies. It also treats stored state as trusted and recommends protecting storage integrity at the application boundary. Holzi's SQLite-backed local storage and backups may retain deleted pages or snapshots, so simply deleting a row does not prove forward secrecy.

**Alternatives considered**:

- Claim forward secrecy based only on calling the library's delete method: rejected because retained WAL, backup, snapshot, or synchronized copies could preserve old keys.
- Keep all prior states to support arbitrarily stale offline devices: rejected because this defeats forward secrecy. Stale devices must recover via fresh authenticated state or explicit history resync.

## Decision 4: New devices join prospectively by default

**Decision**: Add a device with fresh device credentials and a fresh MLS join operation. It receives messages created after its membership becomes active. Historical transfer is an explicit separate user action and must disclose that copied historical plaintext/ciphertext does not gain retroactive forward secrecy.

**Rationale**: User decision: default access is future messages only. MLS epochs provide a natural boundary for prospective membership, while the explicit history path avoids silently broadening access to old data.

## Decision 5: Retain unacknowledged messages locally without a time limit

**Decision**: Keep unacknowledged messages in the local outbox and retry indefinitely until an authenticated recipient acknowledgement or user deletion. Keep old encryption keys only as long as the protocol requires to process a specific pending ciphertext; after required processing, erase them even if delivery is still pending and recover through protocol resync rather than retaining secrets forever.

**Rationale**: User decision: no fixed message expiry. Message retention does not justify retaining old ratchet secrets. Pending ciphertext may need re-encryption under current MLS state after a recovery transition, while message identity and local content stay durable.

## Decision 6: Reuse lessons from haex-vault-legacy's MLS implementation

**Decision**: Treat `haex-vault-legacy`'s MLS implementation as a design reference for lifecycle, persistence integration, and authorization boundaries. Re-evaluate each mechanism for Holzi's Nostr transport and forward-secrecy requirements; do not transplant its delivery or key-export model unchanged.

**Useful existing patterns**:

- A focused `MlsManager` sits behind an `OpenMlsProvider` and a custom SQLite/SQLCipher `StorageProvider`. This is a useful module boundary for Holzi's chat security layer.
- MLS credentials and KeyPackages are bound to an application identity. The predecessor checks the expected DID and uses proof-of-possession material to bind its MLS signing key to that identity. Holzi should similarly bind every MLS leaf to a contact-verified vault identity and an authorized device key.
- KeyPackages are replenished ahead of invitations, rather than requiring an online recipient at send time.
- Commit acknowledgements are tracked per message and recipient, with retry on reconnect. Welcome retrieval is separated from marking a Welcome consumed, so a crash during processing does not lose the invitation.
- Missed-epoch recovery uses an explicit rejoin/External Commit path. Holzi needs an equivalent recovery state machine, with user-visible consequences for prior history access.
- Membership authorization is checked separately from MLS cryptographic validity. The predecessor's authorization work documents concrete failure modes when a receiver lacks the same authorization facts as the committer, when state is stale, and when an exception is too broad. Holzi must define a verifiable authorization source for every device before accepting Add/Remove commits.

**Do not copy unchanged**:

- The predecessor's delivery flow assumes a leader and, in some paths, a sync server that can buffer messages, Welcomes, KeyPackages, and acknowledgements. Nostr relays are independent, untrusted mailboxes and provide no authoritative ordering or guaranteed delivery.
- Its MLS epoch secret is exported for a separate sync-encryption use. Holzi chat must keep message protection inside MLS application messages; exported epoch secrets must not become general-purpose chat keys or be copied into synced state.
- A SQLite/SQLCipher-backed provider alone does not prove forward secrecy. The predecessor's adapter stores serialized MLS values in SQLite; Holzi must additionally account for WAL/journal pages, snapshots, backups, rollback, and secure deletion before claiming erased keys cannot be recovered.
- The predecessor's code and research have evolved across OpenMLS versions. Use the pinned source revision below as evidence for patterns, then re-check APIs, platform support, and security properties against the OpenMLS release selected for Holzi.

**Source revision**: [`haex-vault-legacy` at `8dce379d94e18fcd42c3b73686a06f984ca3f574`](https://github.com/haex-space/haex-vault-legacy/tree/8dce379d94e18fcd42c3b73686a06f984ca3f574), especially `src-tauri/src/mls/{manager.rs,provider.rs,storage/}`, `src-tauri/src/mls/authorization.rs`, `src-tauri/src/space_delivery/local/sync_loop/mls.rs`, `src-tauri/src/space_delivery/local/buffer.rs`, and `src-tauri/tests/mls_lifecycle.rs`.

## Open technical investigations for implementation

- Select and pin an OpenMLS release/ciphersuite only after verifying supported Rust MSRV, Android targets, cryptographic provider, API stability, and current security advisories. OpenMLS 0.9 documentation supports RFC 9420 and lists Android as built but not tested; this is not sufficient evidence for production readiness.
- Prototype an atomic OpenMLS StorageProvider adapter over the vault database and determine how SQLite WAL, compaction, encrypted backups, and device restores affect irreversibility of key erasure.
- Define mailbox routing for key packages and MLS messages that works with current recipient-relay routing while minimizing added metadata.
- Define behavior when an offline device misses epochs or loses its state: reject old state, rejoin with a fresh key package, and make the loss of access to old messages explicit.
- Define application-level ordering and fork recovery across independent relays. A relay must not be authoritative for membership or commit ordering.
- Review file attachments: their per-file keys are carried in chat payloads, but Blossom's encrypted blob retention and deletion lifecycle must respect new group membership epochs.
- Compare Holzi's persistence and message lifecycle against the predecessor's MLS manager, provider, storage adapter, commit-ACK/retry, Welcome crash-safety, KeyPackage replenishment, and rejoin paths. Reuse only mechanisms that preserve Holzi's stronger requirements around Nostr ordering and irreversible key erasure.

## Sources

- [RFC 9420: The Messaging Layer Security Protocol](https://www.rfc-editor.org/rfc/rfc9420.html) — normative asynchronous group key establishment, forward secrecy, post-compromise security, epochs, and message key deletion.
- [RFC 9750: The Messaging Layer Security Architecture](https://www.rfc-editor.org/rfc/rfc9750.html) — roles and architecture around clients and delivery services.
- [Signal Double Ratchet specification](https://signal.org/docs/specifications/doubleratchet/) — comparison point for per-message key evolution and forward security.
- [Signal Sesame specification](https://signal.org/docs/specifications/sesame/) — comparison point for asynchronous multi-device session management.
- [OpenMLS Book](https://book.openmls.tech/) — Rust implementation of RFC 9420 and supported platform/provider overview.
- [OpenMLS persistence guidance](https://book.openmls.tech/user_manual/persistence.html) — storage integrity assumptions and irreversible deletion requirement for forward secrecy.
- [OpenMLS traits and storage provider](https://book.openmls.tech/traits/traits.html) — pluggable crypto, randomness, and persistent storage integration.
- [OpenMLS 0.9 release](https://blog.openmls.tech/posts/2026-08-25-0.9.0-release/) — current release notes, storage changes, and migration concerns as of this plan.
- [NIP-44](https://github.com/nostr-protocol/nips/blob/master/44.md) — limitations of current Nostr payload encryption, including lack of forward secrecy.
- [ADR 0024: Chat over Nostr and Blossom](../../../docs/adr/0024-chat-over-nostr-and-blossom.md) — existing transport, identity, and group behavior decisions.
- [haex-vault-legacy MLS implementation at pinned revision](https://github.com/haex-space/haex-vault-legacy/tree/8dce379d94e18fcd42c3b73686a06f984ca3f574) — predecessor implementation and lifecycle/authorization code; see Decision 6 for paths and applicability.
