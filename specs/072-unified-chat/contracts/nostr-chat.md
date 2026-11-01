# Contract: chat events over Nostr

Layers, keys and checks: [research R1, R2, R4, R5](../research.md). Every event below is the JSON
content of a **rumor**; the rumor is sealed (kind 13, signed by the sender's device key) and gift-wrapped
(kind 1059, one-time key, NIP-44 v2) to **one device key** of a recipient. One wrap per recipient
device, including the sender's own other devices.

## Gift wrap

| Field        | Value                                                                  |
| ------------ | ---------------------------------------------------------------------- |
| `kind`       | 1059                                                                   |
| `pubkey`     | one-time key                                                           |
| `created_at` | now − random(0 … 48 h)                                                 |
| tags         | `["p", <recipient device key hex>]`, `["expiration", <now + 30 days>]` |
| size         | ≤ 60 KiB serialized; rumor content ≤ 32 KiB                            |

## Rumor kinds

| kind  | Name           | Purpose                                 |
| ----- | -------------- | --------------------------------------- |
| 24120 | `chat`         | messages, notes, group ops, invitations |
| 24121 | `chat.receipt` | delivered / read / files stored         |

(24110/24111 stay with spec 051 US7.)

## Common envelope (`kind 24120`)

```json
{
  "v": 1,
  "id": "<128-bit hex, random>",
  "type": "message" | "group.op" | "group.invite",
  "at": 1761900000000,
  "from": "<sender vault identity hex>",
  "card": "holzi:card:…",
  "to": { "direct": "<recipient vault hex>" } | { "group": "<group uuid>" },
  "body": { … }
}
```

Receiver checks, in order (R5): wrap not seen → card valid and not older than the stored one → seal key
is a non-removed device of `card` → `from` equals the card's vault → `from` not blocked → `to` names the
own vault (direct) or a group where `from` is a `member` at `at` (except `group.invite`, `group.op` with
`accept`/`decline` from an invited vault). For an own-device copy of a direct message, also accept when
`from` is the own vault and `to.direct` is the peer vault; store it in the conversation with that peer.
Then check `v` is known. Any failure: drop silently, no receipt.

### `type: "message"`

```json
{
  "text": "Kommst du heute Abend?",
  "attachments": [
    {
      "kind": "file",
      "name": "Plan.pdf",
      "mime": "application/pdf",
      "size": 482113,
      "sha256": "<plaintext hex>",
      "blossom": {
        "url": "https://…/<blob sha256>",
        "sha256": "<ciphertext hex>",
        "key": "<32-byte hex>"
      }
    },
    {
      "kind": "contact",
      "share": "haex-vault://contact/…",
      "photo": "<attachment ref or omitted>"
    }
  ]
}
```

- `text` ≤ 4,000 characters; at least one of `text` or `attachments`; ≤ 10 attachments.
- Every file carries `blossom` (R7: no other path). The file is uploaded before the message is queued.
- A contact photo larger than fits the wrap goes as a file attachment with `mime: image/*` and the contact
  entry references it by index (`"photo": {"attachment": 1}`).

### `type: "group.op"`

```json
{ "op": "add" | "remove" | "leave" | "accept" | "decline" | "rename" | "picture",
  "opId": "<128-bit hex>",
  "member": "<vault hex>",            // add, remove
  "memberCard": "holzi:card:…",       // add: the added vault's card (from the adder's address book)
  "name": "Familie",                  // rename
  "picture": "<base64 ≤ 12 KiB>" }    // JPEG/PNG thumbnail (empty = remove)
```

Sent to every `member` and every `invited` vault of the group plus the affected vault. Applied with the
per-member / per-meta LWW rule of R9; each applied op also inserts a `note` message.

### `type: "group.invite"`

```json
{
  "group": "<uuid>",
  "name": "Familie",
  "picture": "<base64 ≤ 12 KiB or omitted>",
  "part": 1,
  "of": 3,
  "members": [
    {
      "vault": "<hex>",
      "card": "holzi:card:…",
      "state": "member",
      "at": 1761900000000,
      "op": "<hex>",
      "by": "<hex>"
    }
  ]
}
```

≤ 12 members per part; the invitee shows the invitation once all `of` parts arrived (R9). The complete
rumor, including an optional picture, MUST remain ≤ 32 KiB; omit the picture if needed to fit.

## Receipt (`kind 24121`)

```json
{
  "v": 1,
  "from": "<receiver vault hex>",
  "card": "holzi:card:…",
  "thread": { "direct": "<original sender vault hex>" } | { "group": "<uuid>" },
  "delivered": ["<event id>", …],
  "read": ["<event id>", …],
  "files": [{ "event": "<event id>", "attachment": 0, "device": "<device key hex>" }, …]
}
```

Sent only to the original sender's devices (they share the result through own-device sync, and the
uploading device needs `files` for R13). `delivered` and `read` come from one device per vault (the one
that inserted the message, or where it was read); each `files` item identifies the event, attachment
index and device key, and comes from **every** device that stored that file. Batched per conversation,
at most every 2 s.
Never to a blocked vault; `read` never for an unaccepted request or with read receipts off.

## Publishing

- To every Nostr-Relay on the recipient's card (union over stored cards, `received::addresses`); if none,
  to the sender's own Nostr-Relays (US2 scenario 8). Own-device copies go to the own Nostr-Relays.
- A recipient vault counts as _sent_ when at least one Nostr-Relay accepted at least one of its wraps.

## Receiving

- Subscribe `{"kinds":[1059], "#p":[<own device key>], "since": <cursor − 48 h>}` on the own Nostr-Relays.
- Store the wrap id in `chat_wraps_seen_no_sync`; advance the relay cursor to the newest `created_at` seen.
