# Contract: accepted and blocked notices (`kind 24121`)

Amends spec 072 [contracts/nostr-chat.md](../../072-unified-chat/contracts/nostr-chat.md) "Receipt" and the
sending rules of spec 095 [contracts/nostr-chat-ratchet.md](../../095-chat-security/contracts/nostr-chat-ratchet.md).

## Receipt with an answer

```json
{
  "v": 1,
  "from": "<recipient vault hex>",
  "card": "haex:card:…",
  "thread": { "direct": "<sender vault hex>" },
  "answer": "accepted" | "blocked"
}
```

- `answer` is optional; a receipt without it is a receipt as before. A notice has no `delivered`, `read` or
  `files`.
- Encrypted, sealed and wrapped like every receipt (spec 095: on the device-pair session, one wrap per
  device of the card).
- Sent once per answer through the outbox (research R5), to every device on the sender's card: the stored
  card for an accepted notice; for a blocked notice the card of the hidden contact or the card the block
  entry kept. A blocked notice is the only event that goes to a blocked vault.
- Only in direct conversations. Group conversations (072 US4) define their own invitation answer later.

## Receiving a notice (sender side)

Checked as every receipt (`verify::check_peer`): sender device on the card, card not older than the stored
one, `thread.direct` is the own vault, the sender is a stored card. Otherwise dropped, no answer.

- `accepted`: clear `peer_blocked_at` of the direct thread with `from`; every outbox row awaiting `from`
  gets `next_at = now`; deliveries failed with `blocked` go back to waiting.
- `blocked`: set `peer_blocked_at`; every unconfirmed delivery to `from` fails with `blocked`; their outbox
  rows are parked.

## Receiving side changes (no wire change)

- A `chat.message` (`kind 24120`) of a vault without a stored, unblocked card stores no message and no
  attachment; it leaves a request (research R1, R2) or, with `chat.requests.allowStrangers = false`,
  nothing. No receipt of any kind goes back (FR-005).
- 072's rule "`read` never for an unaccepted request" becomes "no receipt for a request".
