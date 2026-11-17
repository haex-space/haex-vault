# Data Model: More secure contact chat

This document describes logical entities. Secret fields are stored only in local, device-scoped storage and are never synchronized as vault CRDT data.

## Device Credential

- **Purpose**: Authenticates one Holzi installation as an MLS participant.
- **Fields**: vault identity, device public identity, MLS signing credential, credential validity/revocation state.
- **Relationship**: One device credential maps to one MLS leaf in each conversation where that device participates.
- **Rules**: Contact cards and the vault's signed device list authorize the mapping. A relay-provided key package is not trusted until its credential chain is verified.

## Conversation Group

- **Purpose**: MLS security state for one direct or group chat.
- **Fields**: stable conversation identifier, MLS group identifier, current epoch, membership snapshot, last accepted commit/message position, protocol version, security status.
- **Relationships**: Contains one leaf per participating device; belongs to exactly one Holzi conversation.
- **Rules**: Every direct conversation is a two-vault membership set with one or more device leaves per vault. A group conversation uses the same device-leaf semantics. Membership changes become effective at an authenticated epoch transition.

## Ratchet State

- **Purpose**: Local secrets and protocol state required to create or process MLS messages.
- **Fields**: local device/group state, epoch secrets, message secrets awaiting processing, pending proposals/commits, storage generation.
- **Relationships**: Owned by exactly one local device and one conversation group.
- **Rules**: Never synchronized between devices as raw secret state. Persist changes atomically. Delete superseded message secrets irreversibly, including application-managed copies. If restored state rolls back to a generation older than the durable generation marker, refuse to resume it as current and require a fresh recovery/join procedure.

## Key Package

- **Purpose**: One-time, authenticated invitation material that permits an offline device to be added asynchronously.
- **Fields**: credential, supported protocol/ciphersuite identifiers, expiration/consumption state, opaque published representation.
- **Relationships**: Published by a device and consumed once by the group that adds it.
- **Rules**: Publish no private material. Reject expired, already-consumed, unsupported, or identity-mismatched packages. Replenish packages without blocking local chat.

## Encrypted Chat Message

- **Purpose**: User-visible message protected by MLS application-message encryption.
- **Fields**: stable Holzi message id, conversation id, MLS epoch/generation reference, encrypted payload, sender device, local creation time, delivery state.
- **Relationships**: Stored in the local conversation history and referenced by one or more relay-delivery attempts.
- **Rules**: Deduplicate by stable message id, authenticate the sender through current conversation membership, reject replay/unauthorized epochs, and retain the local outbox item until an authorized recipient device acknowledges it or the user deletes it.

## Relay Delivery Attempt

- **Purpose**: Tracks transport copies and retry progress without treating a relay as authoritative.
- **Fields**: message id, relay URL, last attempt, relay acceptance result, recipient acknowledgement, retry state.
- **Relationships**: One message may have attempts across several relays and recipient devices.
- **Rules**: Relay acceptance is not recipient delivery. Copies received from multiple relays converge to one message. Missing relay copies do not alter local conversation history.

## Membership Epoch

- **Purpose**: The authenticated participant/device set after a membership commit.
- **Fields**: epoch number, authorized device leaves, commit identity, effective sequence boundary.
- **Relationships**: Ordered within one conversation group.
- **Rules**: Removed devices cannot decrypt application messages from later epochs. Newly added devices cannot decrypt earlier epochs unless the user explicitly authorizes separate historical transfer.

## Security State

- **Purpose**: User-facing description of the protection currently active in a conversation.
- **Fields**: protocol compatibility, last successful epoch transition, pending recovery, stale-device count, legacy-message boundary.
- **Relationships**: Derived from conversation group and device progress.
- **Rules**: Never report recovered post-compromise security until fresh authenticated entropy has advanced the epoch and the transition is durably stored.
