# ADR 0026: Chat events are encrypted on a ratchet per pair of devices

- **Status**: proposed
- **Date**: 2026-11-17
- **Spec**: [095-chat-security](../../specs/095-chat-security/spec.md), research R1–R9
- **Amends**: [ADR 0024](0024-chat-over-nostr-and-blossom.md) decision 1; exception to [ADR 0001](0001-device-scoped-data-convention.md)

## Context

ADR 0024 encrypts every chat event to the long-lived device keys on the recipients' cards. Nostr-Relays store these events, and anyone can copy them. A device key obtained later decrypts every archived event that was addressed to that device, and a device key copied once reads all later events too. Spec 095 asks for forward secrecy and for recovery after a copied key state, for direct and group conversations, with stored delivery to offline devices.

A first draft of spec 095 proposed MLS (RFC 9420). MLS needs all members to apply membership changes in one order, which without a server means choosing a winner among concurrent commits; it also adds a per-conversation identifier that relays see. ADR 0024 decision 4 deliberately has no coordinator.

## Decision

1. Every pair of devices that exchange chat events keeps a Double Ratchet session (Olm from the `vodozemac` crate). The content of a chat event is encrypted on the session with the receiving device and only then sealed and gift-wrapped as in ADR 0024. Nothing is encrypted to a device key alone any more.
2. Every device publishes a chat key record, signed by its device key, to the Nostr-Relays on its vault's card. Others open a session with it from that record. The device key on the card stays the only anchor of trust.
3. ADR 0024 decisions 2 to 5 stay: Nostr-Relays and Blossom only, files as before, group membership as ops with last-writer-wins, no interop goal.
4. A device's secret ratchet state is stored in `_no_sync` tables keyed by `installation_uuid` and is never synchronized. This is an exception to ADR 0001, like `device_keys_no_sync`: a secret that reached other devices would make every device hold the keys of all others and would keep deleted keys alive on devices that are offline.
5. There is no migration; events in the earlier format are neither sent nor accepted.

## Consequences

- A relay archive plus a device key obtained later yields no message content whose keys were erased. The wraps and seals still open with that key: sender device, time and size of archived events become visible.
- A key state copied once stops decrypting after both devices have contributed fresh DH ratchet keys generated after the copy; proactive keep-alives and their bounded immediate responses complete the full two-sided ratchet progression within 48 hours while both vaults are open.
- Nostr-Relays additionally see each device's chat key record and when it was renewed. They still see no vault, contact, group or text.
- A copy of a vault file carries the source installation's ratchet rows. The copy never uses them; they are as protected as the history in the same file.
- The kept history on a device is outside this protection. Whoever can open the vault reads it.
- One new dependency, `vodozemac`. The largest chat event becomes smaller, because the ratchet adds a layer inside the 60 KiB wrap.
- Someone who holds a device's keys and also publishes as that device is not stopped by the ratchet; removing the device from its vault is the remedy, as before.
