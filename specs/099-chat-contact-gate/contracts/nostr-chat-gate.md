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
  "answer": "accepted" | "blocked",
  "answerSequence": 42
}
```

- `answer` and `answerSequence` are optional; a receipt without them is a receipt as before. A notice has
  no `delivered`, `read` or `files`. The answering device increases `answerSequence` for each answer to
  that peer. The receiver ignores a notice whose counter is no greater than the last one applied from
  that device; legacy notices without a counter are sequence zero.
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

When a new answer is queued for the same peer, the outbox drops any older unsent gate notice for that
peer before enqueueing it. Thus a pending `blocked` notice is replaced by a later `accepted` notice (and
vice versa), so an older unsent answer cannot be published after the newer one. The receiver uses the
counter to ignore late answers from the same answering device. Because the answer queue is device-local
but the block list is shared, a device also discards a queued answer when it no longer matches the
current synced block decision before publishing it. Wrap `created_at` is not used to order answers.

## Receiving side changes

- A `chat.message` (`kind 24120`) of a vault without a stored, unblocked card stores no message and no
  attachment; it leaves a request (research R1, R2) or, with `chat.requests.allowStrangers = false`,
  nothing. No receipt of any kind goes back (FR-005).
- 072's rule "`read` never for an unaccepted request" becomes "no receipt for a request".
