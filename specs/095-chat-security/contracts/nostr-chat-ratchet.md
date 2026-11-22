# Contract: chat events on device sessions

This contract changes one layer of [spec 072's chat contract](../../072-unified-chat/contracts/nostr-chat.md): what a rumor contains. The gift wrap (kind 1059, one-time key, backdated, 30-day expiration, one wrap per receiving device) and the seal (kind 13, signed by the sender's device key) stay as they are. Reasons: [research R3–R8](../research.md). The kind numbers were checked against the NIP kind list on 2026-11-17 ([research, checks done for tasks](../research.md#checks-done-for-tasks)).

## Rumor kinds

| kind  | Name           | Content                                                 |
| ----- | -------------- | ------------------------------------------------------- |
| 24122 | `chat.ratchet` | a ratchet message on the session with the wrap's device |
| 24123 | `chat.reset`   | asks the receiving device to open a new session         |

A rumor of kind 24120 or 24121 that arrives directly in a seal is dropped. Those two kinds exist only inside a ratchet message.

## `chat.ratchet`

```json
{
  "v": 1,
  "type": 0 | 1,
  "body": "<base64 of the Olm message>"
}
```

- `type: 0` is an opening message (Olm pre-key message): it creates the session on the receiving side and carries the sender's ratchet identity key. `type: 1` is a message on an existing session.
- A device sends `type: 0` until it has decrypted an event on that session.

The plaintext of `body`:

```json
{
  "kind": 24120 | 24121 | 0 | 2,
  "content": "<the JSON of the spec 072 event, or empty for kind 0 or 2>"
}
```

- `kind: 24120` and `kind: 24121` carry the chat event and the receipt of spec 072 unchanged, with `from`, `to` or `thread`, and the sender's card.
- `kind: 0` is a keep-alive: it only moves the session forward and is never shown.
- `kind: 2` is its bounded reply: it only moves the session forward, is never shown, and never triggers another reply.

## `chat.reset`

```json
{ "v": 1 }
```

Sealed by the sending device's key, no other content. Sent when a ratchet message from a known device could not be decrypted. Both sides then drop every session with the other device: the sender when it sends the reset, the receiver when it accepts it. Without that, the rule for the sending session (Limits) could keep choosing a session that one side can no longer use.

## Chat key record

A replaceable Nostr event that a device publishes to the Nostr-Relays on its vault's card.

| Field        | Value                                                                       |
| ------------ | --------------------------------------------------------------------------- |
| `kind`       | 10120                                                                       |
| `pubkey`     | the device key; the event's signature is the record's signature             |
| `created_at` | the time the record was published (not backdated)                           |
| tags         | `["expiration", <until>]`                                                   |
| `content`    | `{"v": 1, "identity": "<base64>", "start": "<base64>", "until": <seconds>}` |

- `identity`: the device's ratchet identity key (Curve25519). `start`: its current start key. `until`: the time the start key was made + 30 days. Publishing the same start key again (after the vault's Nostr-Relays changed) keeps `until`, so a start key is never offered longer than the device keeps it.
- A device publishes its record to the Nostr-Relays on its vault's card; if the card names none, to the Nostr-Relays it listens on (its contacts' ones), where senders publish in that case (spec 072 US2 scenario 8).
- A reader checks the event's signature, that `pubkey` is a non-removed device on the vault's current card, that `until` has not passed (by its own clock, whatever the relay does with the expiration tag), and that `created_at` is not older than the record it already holds for that device.
- A different `identity` than the stored one means the device started anew: sessions with it are dropped and a new one is opened.

## Sending

For each receiving device (every device on the recipients' cards and the own other devices, as in spec 072):

1. Use the session to send on with that device (Limits: the retained one with the smallest session ID). Without a session: use the stored record, or fetch it from the device's Nostr-Relays; without a usable record the device stays pending and is tried again with each retry. Nothing is sent to it under the device key alone.
2. Encrypt the plaintext on the session, save the session and store the result with the outbox entry, in one transaction.
3. Seal and wrap the result as in spec 072, store the finished wrap and publish it to the device's Nostr-Relays. Retries until a Nostr-Relay accepts publish that same wrap again (same event id). A second wrap around the same ratchet message would reach the receiver as a message whose key is already used and would look like a broken session.
4. Without a delivery receipt from the recipient vault: encrypt again on the current session and publish again after 1 day, 3 days and then every 7 days, until the receipt arrives or the user deletes the message. Before each of these, fetch the record again if nothing was ever received on the session, and open a new session if the record changed.

Step 4 applies only to user-created chat events. Receipts and keep-alives are sent once and are not retried by this schedule.

While its vault is open, each device sends on every active session at least once every 24 hours, even when it has received no recent event: a keep-alive when it sent nothing else on the session in that time. A session is active when it is the one the device sends on (Limits) to a peer device it still addresses. A received message is answered promptly by the delivery receipt of spec 072. A received keep-alive request is answered promptly by a distinct keep-alive reply; a reply never triggers another reply, so the exchange is bounded. A received receipt is answered only by the next event that falls due. A session is not considered recovered from a copied state until both devices have contributed fresh DH ratchet keys that were generated after the copy; exchanging one event in each direction alone is not sufficient. With both vaults open, this full two-sided ratchet progression completes within 48 hours.

## Receiving

For a fetched batch of wraps:

1. Per wrap: not seen before and not past its `expiration` (a wrap without one is dropped); open the wrap and the seal; this gives the sender device key and the rumor. Seen wraps are remembered for the 30 days a wrap lives, so a replayed wrap never reaches a session.
2. `chat.reset`: accept only from a device on a stored card of a contact, a group member or the own vault, and at most once per device and hour. Then drop every session with that device, fetch its record again, open a new session and encrypt again every outbox entry whose awaiting vault is the device's vault.
3. `chat.ratchet`: group by sender device. `type: 0` whose session already exists is decrypted on it; otherwise it creates the session with the identity key it carries. `type: 1` is tried on the sessions with that device. Within a session, decrypt in the order of the message counters. An accepted opening that carries another ratchet identity key than the sessions already held with that device drops those sessions (the device started anew).
4. Before committing a session created by an opening, authorize the sender before saving any session state. For a decrypted user event, run the spec 072 receiver checks unchanged, including verifying and storing its carried card and checking the seal's device key; commit the card, session and message atomically only if they pass. A decrypted `kind: 0` keep-alive request or `kind: 2` keep-alive reply has no carried card, so accept it only from a non-removed device on an already verified stored card of a contact, group member or the own vault; otherwise drop it without saving its session. Only a request produces a reply. This also prevents an unauthenticated opening from creating a stored keep-alive session.
5. Save the session, the wrap id and the rows of the event in one transaction.
6. A ratchet message that cannot be decrypted: if the sender device is on a stored card, drop every session with it and send it a `chat.reset` (at most once per device and hour); otherwise drop silently.
7. A message that is already stored and was received more than 12 hours ago is a resend (Sending, step 4): it is acknowledged with a delivery receipt again, so a lost receipt does not make the sender send for ever.

## Limits

- At most four sessions per peer device.
- If there is more than one session with a peer device, send on the retained session with the lexicographically smallest Olm session ID. Both endpoints compute the same choice from the shared session IDs; they accept valid incoming messages on any retained session. Once they know the same sessions, simultaneous openings therefore converge on the same sending session.
- A session tolerates 40 skipped message keys per receiving chain and a message at most 2,000 ahead; step 3's ordering and encrypting once per attempt keep ordinary traffic inside that.
- Sizes: the wrap limit of 60 KiB stays. The plaintext of a ratchet message is at most 20 KiB (20,480 bytes; spec 072 allowed a rumor of 24 KiB). Measured: an opening message with 21,215 bytes of plaintext is the largest whose wrap stays within 60 KiB.

## What a Nostr-Relay can and cannot do

- It sees kind 1059 events to device keys, their sizes and blurred times, as before, and the chat key records with the times they were renewed. It sees no vault, contact, group or text.
- It can drop, delay, reorder, replay or withhold wraps and records. That delays delivery and can force a new session; it cannot make a device accept a key that the device key on the card did not sign, and it cannot make a message count as delivered.
- Someone who later obtains a device key can open the wraps and seals archived for that device and sees who sent them and when, but not their content.
