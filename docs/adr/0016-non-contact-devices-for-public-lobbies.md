# Devices of non-contact vaults for public lobbies

Status: proposed
Date: 2026-10-22

## Context

ADR-0015 opened the iroh endpoint of a vault session to devices of other vaults for the first time,
under one condition: the other vault identity must belong to a contact with a verified business card.
Everyone else is refused.

Spec 057 gives extensions channels to the same extension on other vaults (first user: the multiplayer
mode of haex-doom). Invitations stay contacts-only. But the operator decided that a host may open a
**public lobby**: anyone who holds its join code may join, also a person who is not in the address
book (Clarifications 2026-10-22 of specs 057 and haex-doom). That needs connections from devices whose
vault identity holzi cannot vouch for.

## Decision

**While at least one public lobby is open on a device, its endpoint accepts connections on
`haex-vault-ext-peer/1` from devices of vaults that are not contacts.** Outside that window such connections
are closed right after the handshake, without saying whether lobbies exist.

Bounds that hold for every such connection:

- The device still proves its device key on a device list signed by its vault identity, bound to the
  QUIC peer (same check as ADR-0015); only the trust in that identity is missing. It is shown to the
  extension as "not verified" with a self-chosen name.
- It must present a valid join proof for an open public lobby of the same extension within 5 seconds,
  or it is closed. The proof is an HMAC over the handshake nonces with a 128-bit secret that only the
  join code carries; wrong, unknown and closed codes get one uniform answer.
- It reaches nothing but that one channel: no vault data (`holzi-sync/1` still refuses foreign vaults),
  no files (`holzi-files/1` still requires a contact), no other extension, no holzi function.
- Blocked vaults are refused in every case.
- Admission is rate-limited: few unadmitted connections at once, a cap on new unverified connections per
  time window, and a cooldown per endpoint after failed joins.
- Public device address discovery stays off. The join code carries the host's endpoint id and relay; a
  directly connected player learns the host's network address, which the permission prompt states.

## Consequences

- Anyone who gets a public join code can make the device do a handshake and one HMAC check while the
  lobby is open. Rate limits bound the load; the secret makes guessing hopeless.
- A public lobby exposes the device's network address to the players who connect directly. This is
  accepted and shown to the user before the extension gets the permission.
- The endpoint id in a join code is stable per device. An old code cannot be used to connect once no
  public lobby is open, but it does identify the device to whoever kept it.
- ADR-0015 stays valid for `holzi-files/1`; this ADR widens only `haex-vault-ext-peer/1`, and only during a
  public lobby.
