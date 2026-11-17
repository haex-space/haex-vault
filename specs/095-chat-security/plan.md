# Implementation Plan: More secure contact chat

**Branch**: `feat/095-chat-forward-secrecy` | **Date**: 2026-11-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/095-chat-security/spec.md`

## Summary

Replace long-lived device-key encryption for new contact-chat messages with MLS (RFC 9420), using each Holzi device as an independently authenticated group member. MLS provides asynchronous group key establishment with forward secrecy and post-compromise security. Keep Nostr as an untrusted store-and-forward delivery service, carrying opaque MLS messages and key-package advertisements; retain authoritative history and retry state locally. Make persisted cryptographic state transactional and ensure superseded secrets are actually erased. Existing messages remain readable as legacy NIP-44/NIP-59 messages and do not gain retroactive forward secrecy.

## Technical Context

**Language/Version**: Rust (workspace toolchain)

**Primary Dependencies**: Existing Rust/Tauri, Nostr SDK, SQLite-backed vault storage; evaluate OpenMLS RFC 9420 implementation and provider versions during implementation review.

**Storage**: Per-device, per-conversation MLS state and outbox in the local vault database; state is device-local secret data and must not enter synchronized CRDT tables or Git.

**Testing**: Rust unit and integration tests with in-process Nostr relays and deterministic multi-device fixtures; security-property tests must cover key compromise, deletion, membership changes, replay, reordering, and crash consistency.

**Target Platform**: Existing Holzi desktop and mobile targets, including Android.

**Project Type**: Tauri desktop/mobile application with Rust backend and Vue frontend.

**Performance Goals**: Direct and group message encryption/decryption should complete as part of ordinary send/receive without blocking the async runtime; relay outages must not block local chat history or composition.

**Constraints**: The relay may observe, drop, reorder, replay, or inject traffic. It cannot be trusted for group ordering, delivery, or deletion. Key state changes and message handling must be crash-safe and atomic with durable outbox/inbox state. Secret key deletion must be durable and must not leave recoverable copies in application-managed storage. Offline devices must be able to resume from queued MLS messages or explicitly enter a recovery/resync flow. No NIP-42 relay identity is required for the protocol.

**Scale/Scope**: Direct and group conversations; each installation is a separate cryptographic participant. Group membership updates apply to devices, not merely vault identities. Existing chat history, contacts, message rendering, receipts, and Nostr/Blossom routing remain compatible through staged migration.

**Predecessor guidance**: `haex-vault-legacy`'s MLS implementation is a reference for the manager/provider/storage boundary, identity-bound KeyPackages, proactive KeyPackage replenishment, per-recipient commit acknowledgements, crash-safe Welcome consumption, explicit missed-epoch rejoin, and authorization checks independent from MLS validity. Its leader/server delivery assumptions and exported MLS epoch keys do not transfer to Holzi chat. See [research.md](research.md#decision-6-reuse-lessons-from-haex-vault-legacys-mls-implementation) for the pinned source revision and reuse limits.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

- **No secrets in Git**: Pass. Specs and contracts describe key handling but contain no key material.
- **No local absolute paths in versioned configuration**: Pass. Artifacts use repository-relative paths.
- **Project identity is device-independent**: Pass. Participants are bound to vault/device identities, not machine paths.
- **Relay unavailability never blocks local work**: Pass. Message history and pending outbox remain local and usable offline.
- **Phase discipline and PR workflow**: Pass. This creates design artifacts only on a topic branch; no source implementation is included.
- **Security storage invariant**: Design gate. Ratchet state must be persisted in the same durable transaction as corresponding message state, and erased secrets must not be retained in app-managed copies, backups, logs, or synced rows.
- **Multi-device and offline correctness**: Design gate. Devices advance state independently; an offline device must not cause a sender to reuse or retain old message keys indefinitely. The recovery behavior must be explicit.
- **MLS authorization and recovery**: Design gate. A cryptographically valid commit is not automatically an authorized membership change. The receiver must verify device authorization using facts available and verifiable on every device; missing epochs, stale state, rejected commits, and retries need explicit transitions. The predecessor's documented authorization and recovery gaps are inputs to these requirements.

No constitution violations identified. The storage and offline-device design must be validated before implementation begins.

## Project Structure

### Documentation (this feature)

```text
specs/095-chat-security/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── nostr-mls-transport.md
└── checklists/
    └── requirements.md
```

### Source Code (repository root)

```text
src-tauri/src/chat/peer/       # Existing chat message and delivery boundary
src-tauri/src/chat/peer/files/ # Existing file key delivery, to be reviewed for group epoch integration
src-tauri/src/sync/            # Existing device identities, contact trust, and vault membership
src-tauri/tests/               # Existing Rust integration-test convention
```

**Structure Decision**: Extend the existing contact-chat and device-identity boundaries. A future implementation should keep MLS protocol state behind a focused chat security module and its storage adapter; final module names and crate layout are deferred until the OpenMLS provider and persistence design is prototyped. Do not put cryptographic implementation in the Nostr transport layer.

## Complexity Tracking

No constitution violations require an exception.
