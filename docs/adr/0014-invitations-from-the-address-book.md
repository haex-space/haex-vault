# Invitations from the address book

Status: proposed
Date: 2026-10-14

## Context

Decision D33 of the sync design (`docs/plans/2026-09-28-sync-architecture.md`) says invitations to
spaces (spec 027) and data shares (spec 028) are links the admin hands over, "never a message to a
bare vault identity". Spec 027 FR-008 repeats this, and both specs ruled out an address book. The
reason was unsolicited invitations: without a link there is nothing that proves the inviter knows the
invitee, and nothing to block.

Spec 051 adds an address book. Its operator decision (2026-10-13, option C) is that contacts carry
confirmed holzi addresses from a signed business card ("Visitenkarte") of the other vault, and that the
invite dialogs of 027 and 028 can pick a recipient from the address book.

## Decision

**An invitation may be delivered to a contact whose business card holzi has verified, in addition to
the link.** D33 is superseded in this point; everything else in D33 stays.

- Only a vault identity that came from a valid, signed business card is a valid recipient. Inviting a
  typed-in or manually entered vault identity stays forbidden (027 FR-008).
- The delivered invitation is the same signed invitation as the link (fields, one-time secret,
  expiry, withdrawal, confirmation by the admin or owner); only the way it travels is new.
- It travels as a stored NIP-59 gift wrap (kind 1059, NIP-40 expiry) to every device on the
  recipient's card, over the recipient's Nostr relays. The relay sees a one-time key and the
  recipient device key, never the content or the sender's vault identity
  (`specs/051-phone-contacts/contracts/nostr-invite.md`).
- The recipient shows invitations from vaults that are not in its address book with a confirmed card
  in a separate list without notification, and can block their sender. Invitations from blocked vaults
  are dropped silently.

## Consequences

- Having someone's business card is enough to invite them. Separate display and blocking are the
  protection against unwanted invitations; there is no further spam protection.
- holzi starts using stored Nostr events (until now only ephemeral ones). The plans of 027 and 028
  take over the wire format from spec 051.
- D33 in the sync design gets a note pointing to this ADR.
