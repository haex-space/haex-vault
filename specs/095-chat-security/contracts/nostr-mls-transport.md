# Nostr transport contract for MLS chat

This contract keeps Nostr as an untrusted store-and-forward layer. The final encoding is chosen during implementation, but the semantic requirements below are binding.

## Transport envelope

Each relay event carries an opaque, versioned Holzi envelope with:

- protocol family and version;
- stable opaque conversation routing identifier;
- message class: key-package advertisement, MLS proposal/commit, application message, or authenticated delivery acknowledgement;
- recipient-device routing tag where needed;
- opaque encrypted MLS payload;
- transport idempotency identifier.

The envelope MUST NOT contain plaintext message content, raw MLS secrets, private key packages, local database identifiers, or vault names. Any metadata used for MLS processing must itself be authenticated by the MLS protocol or by a device credential bound to the conversation.

## Key package discovery

- Devices MAY advertise public, one-time key packages to the Nostr relays associated with a contact card.
- A package is consumed at most once and is bound to a verified device credential and supported protocol version.
- A package advertisement is not proof of authorization. The adding device checks the contact's signed identity/device information before creating a membership commit.
- NIP-42 is optional relay access control and is not part of the security protocol's trust assumptions.

## Delivery and ordering

- Publishers attempt configured recipient relays independently; one relay failure MUST NOT block attempts to others or local chat use.
- Relay `OK`/acceptance means only that the relay accepted the transport event.
- Recipient delivery is confirmed only by an authenticated acknowledgement from a current conversation member/device.
- Receivers deduplicate relay copies, validate MLS epoch and membership, and reject invalid, unauthorized, replayed, or conflicting commits without advancing trusted state.
- Relays may drop, delay, reorder, replay, or withhold events. Clients establish conversation order and detect/recover from conflicting epoch histories without trusting one relay as the source of truth.
- Unacknowledged messages remain in the local outbox and retry indefinitely until acknowledged or deleted by the user. Protocol recovery may require re-encrypting the retained local message under current group state.

## Migration and compatibility

- Legacy NIP-59 kind-1059 chat wraps remain readable by existing code.
- New secure messages use a distinct versioned envelope and MUST NOT silently downgrade to legacy long-lived-key encryption.
- When a peer or relay path cannot carry the new protocol, Holzi surfaces the incompatibility and preserves the local message as pending; it does not claim the message is forward-secret.
- Legacy messages and previously archived ciphertext retain their original security properties; migration cannot make old ciphertext forward-secret.
