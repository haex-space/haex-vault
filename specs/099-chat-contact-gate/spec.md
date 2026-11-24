# Feature Specification: Only contacts may write in the chat

**Feature Branch**: `feat/099-chat-contact-gate`

**Created**: 2026-11-25

**Status**: Draft

**Input**: User description: "For the chat we need an approval gate: before a user can write to another the first time, the recipient must accept. That prevents spam. By default every message of another vault is refused unless the user allowed that vault; every vault in the address book is allowed. Blocking is handled the same way: a stored contact can be blocked. A request has only two answers: accepting adds the contact to the address book, declining puts the vault on the block list and drops every further attempt. The sender gets a message so they know what is going on. Blocked vaults can be allowed again later. Users can switch off requests from strangers in the vault settings (on by default), so under a spam attack they need not answer each request while contacts keep writing. A blocked user gets a message. An unblocked contact is part of the address book again."

## Clarifications

### Session 2026-11-24/25 (operator)

- Q: How does a vault that may not write reach the user at all? → A: It knocks. Only a request with its
  Visitenkarte reaches the user; no text, no file.
- Q: Does the gate apply to everything that comes from other vaults? → A: Only to the chat. The block list
  keeps applying where it already does (contact sharing, extension peer channels).
- Q: What are the answers to a request? → A: Only "Annehmen" and "Ablehnen". "Annehmen" always adds the
  vault to the address book; there is no separate list of vaults that may write without being contacts.
  "Ablehnen" puts the vault on the block list, and the sender is told.
- Q: Should blocking be a column of the contact? → A: No, it stays the per-vault block list of spec 051: a
  declined vault is no contact, a contact may hold more than one vault, and the list is already checked in
  four places.
- Q: Can requests from strangers be switched off? → A: Yes, a vault setting "Anfragen von Fremden
  zulassen", on by default. Off, messages of strangers are dropped without telling the sender, so an
  attack gets no answer; contacts write as before.
- Q: What happens to open requests when the setting is switched off? → A: They are gone at once, silently:
  the sender gets no answer and the user sees nothing of them. Under a wave of 1,000 requests the user
  notices none of it. They are not put on the block list.
- Q: Is a blocked vault told? → A: Yes, a blocked contact is told like a declined stranger.
- Q: What does blocking do to a stored contact? → A: It leaves the address book and every list of
  contacts, but keeps all its data; unblocking brings it back as it was.
- Q: Where does an unblocked vault go? → A: Into the address book, also a declined stranger (from the
  Visitenkarte its block entry kept); it is told it may write again. A vault is always exactly one of
  contact, blocked or unknown.

### Session 2026-11-25 (clarify)

- Q: What hint does the sender see while its messages are not delivered? → A: Only when nothing ever came
  back from that vault (no message, no receipt) and a message has not been delivered for one minute: "Noch
  nicht zugestellt. <Name> ist offline oder hat dich noch nicht angenommen." The recipient sends nothing
  for a request.
- Q: Who still sees a blocked (hidden) contact? → A: Nobody but the blocked list and its conversation: the
  address book, every search, the agent, share dialogs and extensions with contact access all leave it
  out.
- Q: What happens to messages the sender writes after a blocked notice? → A: They are kept and shown _not
  delivered_ with the reason "blocked" but not sent; an accepted notice later sends them with the older
  ones.
- Q: Does a blocked contact leave the phone's address book (spec 051 sync)? → A: No, it stays on the
  phone; it is hidden only in HaexVault.
- Q: Where does the conversation with a blocked contact show? → A: Only through the blocked list: it leaves
  the chat list and the chat search; each entry of the blocked list offers "Verlauf ansehen", which opens it
  with "Entsperren" in place of the input field.

## Relationship to existing specs

- **Spec 072** (unified chat): this spec replaces US5 and FR-040 to FR-043 there. A request no longer
  holds the sender's messages, "Blockieren" is no answer of its own any more, and declining is no longer
  silent. 072's edge case "a contact is deleted: new messages from that vault show as a request" stays.
  072 US4 (groups, not built yet) takes FR-013 of this spec.
- **Spec 051** (contacts): its block list of vaults (FR-039) is the block list here, with the same
  settings page. This spec adds blocking a stored contact, hides blocked contacts from the address book,
  keeps the Visitenkarte of a blocked stranger, and makes unblocking restore a contact.
- **Spec 060** (contact sharing): "Annehmen" reads the carried Visitenkarte in through 060's preview, as
  072 does today.
- **Spec 095** (chat security): the outbox already sends a message again until the recipient confirms it;
  this spec only makes it send at once when it learns it may write, and stop when it learns it may not.

## Terms

- **Contact**: a vault whose Visitenkarte is stored in the address book and that is not blocked.
- **Stranger**: a vault that is neither a contact nor blocked.
- **Request**: what a message of a stranger leaves on the receiving vault: the sender's name and
  Visitenkarte and when it knocked, nothing of the message.
- **Accepted notice / blocked notice**: what the receiving vault tells the sender's devices: that it may
  write now, or that it may not (declined or blocked).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - A stranger cannot put messages into my chat (Priority: P1)

Ben has Anna's Visitenkarte; Anna has never saved Ben's. Ben writes to Anna twice and attaches a photo.
Anna sees one request from Ben under "Anfragen", with Ben's name and Visitenkarte, but neither of his
texts nor the photo. Her vault stored nothing of them.

**Why this priority**: having someone's Visitenkarte is enough to send to them. Today a request still holds
everything the sender sent, so anyone holding the card can fill the requests with text and files.

**Independent Test**: Two vaults; B has A's card, A not B's. B sends several messages with and without
files. A shows exactly one request with B's card and no message content; A's data holds no text and no
attachment of B; B sees no _delivered_.

**Acceptance Scenarios**:

1. **Given** a stranger, **When** a message of it arrives, **Then** Anna's vault keeps a request with the
   sender's name and Visitenkarte and keeps neither the text nor any file of the message.
2. **Given** a request is open, **When** further messages of the same vault arrive, **Then** there is
   still one request for that vault, and Anna gets no further notification for it.
3. **Given** a first request of a vault, **When** it arrives, **Then** Anna gets one notification "Neue
   Anfrage" without sound and without any text of the message.
4. **Given** a request is open, **When** Ben looks at his messages to Anna, **Then** they show _sent_
   (a Nostr-Relay took them) but never _delivered_ or _read_; once one of them has not been delivered for
   one minute and nothing ever came back from Anna's vault, the conversation shows "Noch nicht zugestellt.
   Anna ist offline oder hat dich noch nicht angenommen."
5. **Given** a vault on Anna's block list, **When** a message of it arrives, **Then** it is dropped
   without a trace, as today.

---

### User Story 2 - Accept or decline a request (Priority: P1)

Anna opens Ben's request. She sees his Visitenkarte and two choices. "Annehmen" adds Ben to her address
book; Ben's devices learn it and send his messages again at once, and they arrive in the normal
conversation. Had she chosen "Ablehnen", Ben would be blocked, and Ben's conversation would say that Anna
does not take messages from him.

**Why this priority**: without answers the gate would lock everyone out; the sender must not wait in the
dark.

**Independent Test**: As in US1, then accept on A: B's queued messages reach A within the delivery time of
a normal message, show in A's conversation, and B sees _delivered_. Repeat with decline: B's vault lands on
A's block list, B's conversation shows it, B's outbox stops sending, later attempts leave nothing on A.

**Acceptance Scenarios**:

1. **Given** a request, **When** Anna chooses "Annehmen", **Then** the vault becomes a contact with the
   carried Visitenkarte (preview as spec 060) and the request becomes a normal conversation.
2. **Given** Anna accepted, **When** Ben's devices learn it, **Then** they send every message to Anna not
   yet confirmed at once, and those arrive in Anna's conversation in the order Ben wrote them.
3. **Given** Ben's devices miss the accepted notice, **When** their outbox sends again on its usual
   schedule, **Then** the messages arrive, because Ben is a contact now.
4. **Given** a request, **When** Anna chooses "Ablehnen" and confirms, **Then** the vault is on her block
   list with the carried Visitenkarte, the request is gone, and Ben's devices get a blocked notice.
5. **Given** Ben's devices learn of the block, **When** Ben looks at the conversation, **Then** it says
   that Anna does not take messages from him, his messages to her show _not delivered_ with that reason,
   and they are no longer sent again. What he writes afterwards is kept with the same state and not
   sent.
6. **Given** a vault was declined, **When** it writes again, **Then** nothing of it reaches Anna and no
   new request appears.
7. **Given** a request is open, **When** Anna adds the vault as a contact another way (a shared card, a
   card read in by hand, another own device), **Then** the request ends as if accepted, and Ben's devices
   get the accepted notice.

---

### User Story 3 - Switch off requests during a spam attack (Priority: P2)

Anna gets dozens of requests from vaults she does not know. In the chat settings she switches off
"Anfragen von Fremden zulassen". From then on no new request appears and none notifies her; Clara and her
other contacts write to her as before, and the requests that were already there are gone. When the wave
is over she switches it on again.

**Why this priority**: answering each request one by one does not scale under an attack; the gate of US1
already keeps the content out, this keeps the requests out too.

**Independent Test**: With the setting off, messages of several strangers leave nothing on A (no request,
no notification, no notice back), and a contact's message arrives normally. Switched on again, the next
stranger's message is a request.

**Acceptance Scenarios**:

1. **Given** a new vault, **When** Anna opens the chat settings, **Then** "Anfragen von Fremden zulassen"
   is on.
2. **Given** the setting is off, **When** a message of a stranger arrives, **Then** it is dropped without a
   trace: no request, no notification, and the sender gets neither a receipt nor a notice.
3. **Given** the setting is off, **When** a contact writes, **Then** the message arrives normally.
4. **Given** requests are open, **When** Anna switches the setting off, **Then** they are all removed at
   once, without a notice to their senders and without being put on the block list; "Anfragen" no longer
   shows.
5. **Given** the setting was switched off on one of Anna's devices, **When** a stranger's message reaches
   her other device, **Then** it is dropped there too once the setting synced.

---

### User Story 4 - Block a contact and allow a vault again (Priority: P2)

Anna no longer wants messages from her contact Clara. In the conversation with Clara, or on Clara's
contact, she chooses "Sperren". Clara leaves the address book, Clara's devices learn that Anna does not
take messages from her, and Clara's messages are dropped from then on. Months later Anna unblocks Clara in
the settings: Clara is in the address book again with everything Anna had stored about her, and Clara can
write at once. Ben, whom Anna declined, she also unblocks: he becomes a contact too.

**Why this priority**: the gate is only complete when a contact can be shut out too and a mistake can be
undone; it is not needed for the gate to stop strangers.

**Independent Test**: Block a contact from the conversation; it disappears from the address book, its
messages leave nothing and get no receipt, its devices show the block. Unblock in the settings; the
contact is back with all fields, and the next message arrives normally. Unblock a declined stranger; it is
a contact with the card of its request.

**Acceptance Scenarios**:

1. **Given** a conversation with a contact, **When** Anna chooses "Sperren" and confirms, **Then** every
   vault of that contact is on the block list, the contact no longer shows in the address book or in any
   list or search of contacts, its data is kept, the contact's devices get a blocked notice, and the
   conversation keeps its messages but leaves the chat list and the chat search; the blocked list opens it
   ("Verlauf ansehen"), with "Entsperren" in place of the input field.
2. **Given** a contact in the address book, **When** Anna chooses "Sperren" there and confirms, **Then** the
   same happens as from the conversation.
3. **Given** a blocked vault, **When** a message of it arrives, **Then** it is dropped without a trace and
   without a receipt.
4. **Given** the blocked list in the settings, **When** Anna unblocks a former contact, **Then** the contact
   shows in the address book again with all its data, its devices get an accepted notice, and its next
   message arrives normally.
5. **Given** the blocked list, **When** Anna unblocks a declined stranger, **Then** it becomes a contact
   with the Visitenkarte its block entry kept, and its devices get an accepted notice.

---

### Edge Cases

- A contact is deleted from the address book: its next message is a request (072 edge case); its earlier
  messages stay.
- Anna's vault is open on two own devices: both receive the same knock; there is one request, one
  notification, and an answer on one device holds on the other.
- Anna answers on one device while the other has not synced yet and gets a further knock: the later answer
  wins; a declined vault is not shown as a request again once the block reached that device.
- A notice cannot be delivered (all of the other vault's devices offline, no Nostr-Relay takes it): it is
  tried like a message; after an accept the sender's outbox delivers on its own schedule anyway.
- A notice arrives that does not come from the vault the conversation is with: it is dropped.
- A contact holds more than one vault: blocking it blocks every one of them; unblocking one of them
  unblocks them all and restores the contact.
- A request carries a Visitenkarte that does not parse or whose signature fails: no request is kept,
  nothing is shown.
- A stranger's later knock carries a newer Visitenkarte than its open request: the request keeps the
  newer one.
- Requests are switched on again after a wave: a vault of the wave whose outbox sends again later becomes
  a request again then; nothing of the time the setting was off comes back.
- Many new vaults knock at once while requests are allowed: each is one request without sound; the
  setting of US3 stops them.
- A vault blocked before this spec has no kept Visitenkarte: unblocking it only removes it from the list,
  and its next message is a request.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The chat MUST store a message only when its sender's vault is a contact. Own messages from own
  devices are not affected.
- **FR-002**: A message of a stranger MUST leave only a request, and only while "Anfragen von Fremden
  zulassen" is on: the sender's name and Visitenkarte from the message and the time of its first and
  latest attempt. No text, no file and no file reference of the message may be stored or downloaded.
- **FR-003**: There MUST be at most one open request per vault; a further attempt only updates its latest
  time and, if newer, its Visitenkarte.
- **FR-004**: A new request MUST cause one notification "Neue Anfrage" without sound and without message
  content; further attempts of the same vault MUST NOT notify.
- **FR-005**: No delivery or read receipt MUST go back for a message that became or updated a request, or
  that was dropped.
- **FR-005a**: While nothing ever came back from a vault (no message, no receipt) and a message to it has
  not been delivered for one minute, the sender's conversation MUST show that the recipient is offline or
  has not accepted yet; it MUST NOT claim either. The recipient MUST NOT answer a request.
- **FR-006**: A request MUST offer exactly "Annehmen" and "Ablehnen"; "Ablehnen" MUST be confirmed first.
- **FR-007**: "Annehmen" MUST add the vault as a contact with the carried Visitenkarte through spec 060's
  preview. Whenever an open request ends because its vault became a contact (by this or any other way),
  the vault MUST send an accepted notice to the sender's devices.
- **FR-008**: On an accepted notice the sender's devices MUST send every message to that vault not yet
  confirmed at once, instead of waiting for the next scheduled attempt.
- **FR-009**: "Ablehnen" MUST put the vault on the block list together with the carried Visitenkarte,
  remove the request, and send a blocked notice to the sender's devices.
- **FR-010**: On a blocked notice the sender's devices MUST mark their unconfirmed messages to that vault
  _not delivered_ with the reason "blocked", stop sending them again, and show in the conversation that
  the recipient does not take messages from them. The input field MUST stay usable; a message written
  afterwards MUST be kept with the same state and MUST NOT be sent until an accepted notice arrives
  (FR-008 then sends it with the others).
- **FR-011**: Accepted and blocked notices MUST be authenticated as coming from the recipient vault's
  devices (as messages are) and MUST be dropped otherwise.
- **FR-012**: The vault setting "Anfragen von Fremden zulassen" MUST exist in the chat settings, be on for a
  new vault, apply on all own devices, and apply on selection. While it is off, a message of a stranger
  MUST be dropped without request, notification, receipt or notice. Switching it off MUST remove every
  open request at once, without notice to its sender and without putting the vault on the block list.
- **FR-013**: In group conversations (072 US4), an invitation of a stranger MUST become a request under the
  same rules; within a group Anna joined, its members may write in that group.
- **FR-014**: Users MUST be able to block a contact from its conversation and from the contact, after a
  confirmation. Blocking MUST put every vault of the contact on the block list, send a blocked notice to
  the contact's devices, and hide the contact from the address book and from every list, search and
  choice of contacts, keeping all its data. Hidden means for every reader of contacts: the agent, share
  dialogs and extensions with contact access too; only the blocked list and the conversation name it.
- **FR-015**: A conversation with a blocked vault MUST keep its messages, MUST leave the chat list and the
  chat search, and MUST open from its entry of the blocked list ("Verlauf ansehen") with "Entsperren" in
  place of the input field; blocking MUST NOT delete history. Unblocking MUST bring it back to the list.
- **FR-016**: The settings MUST list the blocked vaults (spec 051's list), each with the contact's name or
  the name on the kept Visitenkarte and its address, and allow unblocking. Unblocking MUST make the vault a
  contact again: a hidden contact shows again with all its data, a vault with a kept Visitenkarte becomes a
  contact with it. The vault's devices MUST get an accepted notice. A vault with neither only leaves the
  list.

### Key Entities

- **Request**: one per stranger that knocked and was not answered; holds the sender's vault, name,
  Visitenkarte, first and latest attempt. Shared between the user's own devices.
- **Block list entry** (spec 051): one per vault; holds the vault, when it was blocked, and for a vault
  that is no stored contact the Visitenkarte it carried. Shared between the user's own devices.
- **Contact** (spec 051): unchanged, except that a contact with a blocked vault is hidden, not deleted.
- **Accepted / blocked notice**: a signed message from the recipient vault to the sender's devices, naming
  only the answer.
- **Setting "Anfragen von Fremden zulassen"**: a vault preference, on by default.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: After any number of messages of a stranger, the recipient's vault holds none of their text or
  files, and the user sees exactly one request for that vault.
- **SC-002**: After "Annehmen" with both vaults open, the sender's waiting messages show in the recipient's
  conversation within the time a normal message takes (072 SC-002).
- **SC-003**: After "Ablehnen" or "Sperren" with both vaults open, the sender's conversation shows it within
  the time a normal message takes, and no later attempt leaves anything on the recipient's vault.
- **SC-004**: With "Anfragen von Fremden zulassen" off, messages of any number of strangers leave nothing on
  the recipient's vault and cause no notification, no open request remains visible, and contacts' messages
  arrive as before.
- **SC-005**: Unblocking a former contact restores it with every field it had, and its next message arrives
  without any other step.

## Assumptions

- No limit on the number of requests from many new vaults while requests are allowed; the setting of US3
  is the answer to an attack, a rate limit is left to a later spec.
- The sender's outbox keeps its schedule (spec 095); FR-008 only moves the next attempt forward.
- No user has data of the old request behaviour (072 US5); requests holding message text are not migrated.
- The blocked notice says the same for a declined request and a blocked contact; the sender does not learn
  which of the two it was.
