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
  "answerSequence": 42,
  "answerDecisionHlc": "<haex-crdt HLC>"
}
```

- `answer`, `answerSequence` and `answerDecisionHlc` are optional; a receipt without them is a receipt
  as before. A notice has no `delivered`, `read` or `files`. The answering device increases
  `answerSequence` for each answer to that peer. `answerDecisionHlc` is the synced vault-wide decision
  revision. The receiver ignores a notice whose HLC is no newer than the latest revision received from
  any device of that vault, so an answer delayed across devices cannot overwrite a later decision. The
  per-device counter also rejects repeats from that device; legacy notices without a revision retain
  per-device ordering and are ignored after a revisioned notice has arrived.
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
vault-wide HLC revision to ignore late answers across devices, with the counter as a per-device replay
check. Because the answer queue is device-local but the decision is shared, a device discards a queued
answer when its answer or revision no longer matches the current synced decision before publishing it.
Wrap `created_at` is not used to order answers.

## Receiving side changes

- A `chat.message` (`kind 24120`) of a vault without a stored, unblocked card stores no message and no
  attachment; it leaves a request (research R1, R2) or, with `chat.requests.allowStrangers = false`,
  nothing. No receipt of any kind goes back (FR-005).
- 072's rule "`read` never for an unaccepted request" becomes "no receipt for a request".
