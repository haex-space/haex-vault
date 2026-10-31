# ADR 0024: Chat between vaults over stored Nostr gift wraps and Blossom

- **Status**: proposed
- **Date**: 2026-11-01
- **Spec**: [072-unified-chat](../../specs/072-unified-chat/spec.md), research R1–R13

## Context

Spec 072 adds chatting and file exchange with contacts and groups. Contacts are other vaults, known by
their signed Visitenkarte (spec 051/060) with device keys, iroh addresses and Nostr-Relays. Until now
holzi talked to contacts' devices only directly over iroh (specs 056, 057), which needs both vaults open
at the same time. Spec 051 US7 / ADR 0014 proposed stored Nostr gift wraps for invitations, without code.

## Decision

1. Every chat event between vaults is a rumor, sealed (kind 13) with the sender's **device key** and
   gift-wrapped (kind 1059, stored, NIP-44 v2, one-time key, time blurred up to 48 h, NIP-40 expiration
   30 days) to **each device key** on the recipients' current cards and to the sender's own other
   devices. These are ADR 0014's layers; this ADR accepts them for the chat.
2. Messages and files use **only** Nostr-Relays and Blossom-Servers; there is no direct device-to-device
   path for the chat (operator, 2026-11-01).
3. Files are encrypted on the device (AES-256-GCM, 1 MiB segments) with a per-file key carried in the
   rumor, uploaded to a user-chosen Blossom-Server with a per-blob key, downloaded by **every** receiving
   device, and deleted by the uploader once all those devices confirmed, or after 30 days.
4. Group membership is a set of ops with last-writer-wins per member, ordered by (sender time, op id);
   any member may add or remove; no admin, no server authority.
5. Interop with other Nostr messengers is not a goal: rumor kinds 24120/24121 and the JSON envelope are
   HaexVault's own, addressed to device keys, not to a person's npub.

## Consequences

- Delivery works while the recipient's vault is closed; both sides need only Nostr-Relays and one
  Blossom-Server.
- Nostr-Relays see recipient device keys, sizes and blurred times; Blossom-Servers see encrypted blobs,
  their sizes and the uploader's one-time key. Neither sees text, names, groups or vault identities.
- File size is bounded by the chosen Blossom-Server; public servers may refuse encrypted blobs or limit
  free uploads (research R12). Without a Blossom-Server files cannot be sent.
- A device added to a vault after a blob was deleted cannot get that file.
- Linked devices chat like main devices because the vault identity's secret is not needed.
