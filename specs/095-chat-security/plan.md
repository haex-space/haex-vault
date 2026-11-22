# Implementation Plan: More secure contact chat

**Branch**: `feat/095-chat-forward-secrecy` | **Date**: 2026-11-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/095-chat-security/spec.md`

## Summary

Contact-chat events stop being encrypted to long-lived device keys. Every pair of devices keeps a Double Ratchet session (Olm, from the `vodozemac` crate); each chat event is encrypted on the session with the receiving device and then sealed and gift-wrapped as today, one wrap per device. Each device publishes a signed chat key record to its Nostr-Relays so that others can open a session while it is offline. Group membership, addressing, deduplication and what Nostr-Relays see stay as spec 072 and ADR 0024 define them. Secret key state is stored per installation in tables that are never synchronized. A session that broke on either side heals by a reset, and unconfirmed messages are encrypted and sent again without a time limit. There is no migration.

Decisions and their reasons: [research.md](research.md). Wire format: [contracts/nostr-chat-ratchet.md](contracts/nostr-chat-ratchet.md). Tables: [data-model.md](data-model.md).

## Technical Context

**Language/Version**: Rust 1.95, edition 2021 (`src-tauri`); TypeScript / Vue 3 / Nuxt (frontend).

**Primary Dependencies**: new: `vodozemac` (Olm only, exact version, 0.11.2 or later; research R2). Reused: `nostr` 0.45 (`nip44`, `nip59`), `nostr-sdk` 0.45, `haex-crdt`, `zeroize`.

**Storage**: five new `_no_sync` tables in the vault database, keyed by `installation_uuid` where they hold secrets (research R9), and three new columns on the outbox ([data-model.md](data-model.md)). No new synchronized table.

**Testing**: Rust unit tests in `*_tests.rs` files beside each module; two-vault and own-device tests against the in-process Nostr-Relay of `nostr-sdk` (`local-relay`, as `two_vaults_tests.rs` does today); the scenarios of [quickstart.md](quickstart.md). Time is injected; no test sleeps.

**Target Platform**: the existing desktop and mobile targets, including Android.

**Project Type**: Tauri desktop/mobile application with Rust backend and Vue frontend.

**Performance Goals**: encrypting and decrypting stay off the async executor threads where they are batched (fetching a relay backlog); sending a message to a group of 32 vaults stays one transaction and one publish round, as today.

**Constraints**: a Nostr-Relay may drop, delay, reorder, replay or withhold events and records. Session changes are written in the same transaction as the rows they belong to. Secret key state never enters a synchronized table, a log or an error message. No event is sent under the long-lived device key instead of a session (FR-016).

**Scale/Scope**: direct and group conversations (at most 32 vaults, spec 072 FR-052), about three devices per vault: up to about 95 sessions per device for one full group. Sessions belong to pairs of devices, not to conversations; group conversations (spec 072 US4, not built when this plan was written) use them through the same outbox once they exist. The change touches the chat's send, receive, outbox and delivery code and adds one module; the conversation view gains a protection state (US4).

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

- **I. No secrets in Git**: Pass. Documents and tests contain no key material; test keys are generated at run time.
- **II. No local absolute paths**: Pass. All paths are repository-relative.
- **III. Device-independent project identity**: Not affected.
- **IV. Cross-repo references pin immutable revisions**: Pass. The reference to `haex-vault-legacy` names a full commit SHA (research R11).
- **V. External sources opt-in**: Not affected.
- **VI. Self-modifying instructions**: Not affected.
- **VII. Relay unavailability never blocks local work**: Pass. History, composing and the outbox stay local; a missing chat key record keeps a message pending for that device only (research R3).
- **VIII. No concealment instructions**: Not affected.
- **ADR for decisions affecting earlier decisions**: [ADR 0026](../../docs/adr/0026-chat-ratchet-per-device-pair.md) amends ADR 0024 decision 1 and records the exception to ADR 0001.
- **New dependency**: one, justified below.
- **Tests in separate files**: Pass by plan (`*_tests.rs` beside each module).

Re-check after design: no change.

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
│   └── nostr-chat-ratchet.md
└── checklists/
    └── requirements.md
```

### Source Code (repository root)

```text
src-tauri/src/chat/peer/ratchet/       # new: account and start keys, sessions, chat key record (publish, fetch, check),
                                       #      reset, keep-alive; reads and writes only this installation's rows
src-tauri/src/chat/peer/send.rs        # encrypt per receiving device, store the result with the outbox entry
src-tauri/src/chat/peer/receive.rs     # open seals of a batch, decrypt per session in counter order, then the 072 checks
src-tauri/src/chat/peer/outbox.rs      # publish stored results; no give-up after 24 hours
src-tauri/src/chat/peer/delivery.rs    # send again without a delivery receipt; react to a reset
src-tauri/src/chat/peer/envelope.rs    # the two new rumor kinds; inner event limit
src-tauri/src/chat/peer/service.rs     # start record renewal and keep-alive with the vault session
src-tauri/src/identity/                # migration with the new tables
src/                                   # conversation view: protection state and delivery state (US4)
```

**Structure Decision**: one new module `chat::peer::ratchet` behind a small interface (encrypt for a device, decrypt from a device, renew, reset). The wrap and seal code (`wrap.rs`, `sync::gift_wrap`) is not changed. The services start where the chat session starts today: `chat::peer::service::start_for_active_instance`, called from `instances/session_services.rs`.

### Slices

Each slice is one pull request with its tests.

1. **Storage and ratchet module**: migration, account, sessions, pickles, the per-installation rule; no wire change.
2. **Chat key record**: publish, renew, fetch, check against the card.
3. **Send and receive on sessions**: new rumor kinds, stored results in the outbox, batch order on receive, old kinds dropped; the two-vault and own-device tests move to sessions.
4. **Reset and sending again**: undecryptable events, older vault copy, simultaneous openings, retry without limit.
5. **Keep-alive and cleanup**: proactive keep-alives at least every 24 hours, one immediate bounded response to each request, recovery after both sides contribute fresh ratchet keys (up to 48 hours), sessions of removed devices deleted.
6. **Conversation view**: protection state and the texts of US4, FR-014 and FR-024.

## Complexity Tracking

| Addition                 | Why needed                                                                 | Simpler alternative rejected because                                                           |
| ------------------------ | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Dependency `vodozemac`   | A reviewed Double Ratchet; forward secrecy cannot be had from NIP-44 alone | Writing the ratchet ourselves is a cryptographic protocol of our own (research R2)             |
| Public chat key record   | The first message to an offline device needs keys that lie on its relays   | Keys only inside chat events delay the first message until both sides were online twice (R3)   |
| `_no_sync` secret tables | A device's secrets must not reach other devices or outlive their deletion  | The synchronized per-device tables of ADR 0001 would copy every secret to all own devices (R9) |
