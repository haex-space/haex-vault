# Quickstart: checking the contact gate

## Automated (Rust, in-process Nostr-Relay as `two_vaults_tests.rs`)

- **Q1** (US1): B has A's card, A not B's. B sends three messages, one with a file. A has one request
  with B's card, `request_first_at` < `request_last_at`, no row in `chat_peer_messages` or
  `chat_attachments` from B, one notification event; B has no `delivered_at`.
- **Q2** (US2): after Q1, A accepts (stores B's card). A's device writes an accepted notice; B's outbox rows
  for A become due at once; A stores B's three messages in `sent_at` order; B sees `delivered_at`.
- **Q3** (US2): after Q1, A declines. A's block list holds B with its card, the request is gone, B's
  thread has `peer_blocked_at`, B's deliveries fail with `blocked`, B's outbox rows are parked, a new
  message of B is parked, and a further knock leaves nothing on A.
- **Q4** (US3): with `chat.requests.allowStrangers = false` a stranger's message leaves no thread, no
  session, no notice and no notification; a contact's message arrives; switching off deletes every open
  request on both own devices.
- **Q5** (US4): A blocks contact C; C's header is `blocked`, `contacts_list`, `contacts.search` (agent),
  the extension contacts API and spec 060's hint candidates leave C out, `contacts_get(C)` is `NotFound`;
  C gets a blocked notice; C's thread leaves `chat_conversations_list`. A unblocks: C is back with all
  fields, C gets an accepted notice and its parked messages arrive.
- **Q6** (US4): A unblocks a declined stranger B: B is a contact with the kept card, B gets an accepted
  notice.
- **Q7**: a notice from a device not on the sender's card, or for a thread that is not the own vault, is
  dropped.
- **Q8**: two own devices of A receive the same knock: one request, one notification.

## End-to-end scenarios (`scripts/e2e/scenarios/`)

- **E1** `chat-contact-gate.test.ts` (two vaults): knock → request without text → accept → messages
  arrive; second pair: decline → sender shows the block.
- **E2** extend `chat-requests` scenario of spec 072 (if present) or E1: switch "Anfragen von Fremden
  zulassen" off → the requests area disappears, a new knock shows nothing.

## By hand

- **H1**: block a contact from the conversation menu and from the contact page; check the address book,
  the agent's contact search and the phone address book (the phone keeps the contact, research R11).
- **H2**: unblock from the settings list; "Verlauf ansehen" opens the history with "Entsperren".
