# Feature Specification: One chat for the agent and for contacts, delivered over Nostr

**Feature Branch**: `072-unified-chat`

**Created**: 2026-10-31

**Status**: Draft

**Input**: User description (operator, 2026-10-31): "ich möchte gerne als nächstes den chat überarbeiten
und generischer machen. der chat soll sowohl dazu dienen, um mit kontakten chatten und dateien
austauschen zu können, sowie mit dem agent chatten zu können" and "für das austauschen von nachrichten
sowie dateien hatte ich gehofft, wir können nostr dafür nutzen".

## Clarifications

### Session 2026-10-31

- Q: How does a message reach a contact whose vault is not open? → A: Over Nostr. Every message goes as
  an encrypted gift wrap to every device of the contact's Visitenkarte and is stored on the contact's
  Nostr-Relays until the contact's vault opens and picks it up. The same path is used when the contact
  is online.
- Q: How are files exchanged? → A: As a copy: afterwards the file is with the recipient. The sender puts
  it, encrypted, on a Blossom-Server and the message carries where it is and the key to open it. The
  user chooses the Blossom-Server in the settings (public servers preset); the file is removed from it
  once it was picked up. _Changed 2026-11-01 (operator):_ files go only this way, also when both sides
  are online; there is no direct transfer between the devices ("können wir nicht den nostr weg gehen,
  um dateien und nachrichten austauschen zu können?"). Messages and files use only Nostr-Relays and
  Blossom-Servers.
- Q: Groups now or later? → A: Now. Group conversations with a member list, adding, leaving and
  delivery to every member belong to this spec.
- Q: How do the agent and contact conversations relate? → A: One chat app and one conversation list for
  both. The agent is never a member of a contact conversation. It may read contact conversations only
  with a permission of its own (off by default) and may send a message only after the user confirmed
  it.
- Q: Who may remove another member from a group? → A: Every member; there are no admin roles.
- Q: Read receipts on or off by default? → A: On, reciprocal, switchable in the settings.
- Q: Chat with people using other Nostr messengers (by npub)? → A: No; both sides use HaexVault. A
  later spec may add it.

### Session 2026-11-01

- Q: How should the chat's list look? → A: Like a common messenger (Signal): the list shows only the
  people and groups the user wrote with, newest activity first, one entry each; a click shows the whole
  history with them. The agent is one of these entries ("Agent"), always listed, and moves up with
  activity like any contact. There are no kind filters. Only the agent has sessions: its conversation
  view offers "Neue Sitzung" and "Verlauf" (all sessions, rename, delete); a click on the agent opens
  its latest session. Model or delegate are still chosen inside the agent's conversation, per session
  (operator: "ich möchte das chat ui/ux mehr wie ein ganz normalen messenger haben, wie etwa signal. der
  agent soll dabei einfach ein eigener kontakt sein").
- Q: Can the user write notes to themselves? → A: Yes. "Ich", the user's own contact as the address
  book shows it, is a further entry of the list, always listed like "Agent". What the user writes there
  is a note: it shows on all own devices. _Changed 2026-11-25 (operator):_ a note is sent like a
  message, as a gift wrap to the own other devices and with its files over a Blossom-Server, so an
  own device that was offline gets it although the sending device is offline now (operator: "eine
  notiz an mich selbst soll wie eine normale nachricht verschickt werden und nicht nur via CRDT [...]
  ich kann von einem gerät etwas verschicken (nachricht und dateien), das gerät kann offline gehen und
  ich bekomme die nachricht mit einem anderen gerät"). It was "stays in the vault and never goes to a
  Nostr-Relay" before (operator 2026-09, 2026-11-20).

### Session 2026-11-28

- Q: What happens to a message the user sends while the agent is still answering? → A: Both ways exist.
  Enter appends it to a queue: it goes out as the next turn once the running turn has ended.
  Ctrl+Enter sends it immediately, meaning it is injected into the running turn before the agent's
  next model call (after the current tool round); nothing is cancelled (operator 2026-11-28: "mit
  Enter wird die Nachricht gequeued und mit Shift+Enter sofort gesendet, so wie das alle anderen
  Agents auch machen"). Shift+Enter is the line break of the input field and stays that, so the key
  for "immediately" is Ctrl+Enter.
- Q: How do waiting messages look? → A: As chips above the input field, each with an X to withdraw
  it. A waiting message is not in the history; it enters the history only when it is actually sent.
- Q: How many messages may wait, and what ends the queue? → A: Several, in the order they were typed.
  Closing the app (and locking the vault) discards the queue; nothing goes out silently later. _Refined
  later the same day, see the entries below; a stop does not end the queue._
- Q: What does the stop button do to waiting messages? → A: Nothing. A stop only interrupts the running
  turn of its conversation (the running step and its tool calls); the queue is not discarded. Afterwards
  the entries go out as usual, in the order typed: the first starts the next turn, the rest follow as
  their turns end (operator 2026-11-28; this reverses "stopping the turn discards the queue").
- Q: What happens to the queue when the turn ends with an error? → A: The queued texts go back into the
  input field, so nothing typed is lost (operator 2026-11-28).
  Several texts come back in the order typed, separated by a blank line, with their attachments
  attached again; a draft that is already in the field stays, after them.
- Q: One queue for the chat or one per conversation? → A: One per conversation (per Verlauf). A chip
  belongs to the conversation it was typed in and shows only there, and each conversation's queue
  drains when that conversation's own turn ends. Several conversations are meant to run turns at the
  same time (spec 068 R1, T009); this spec defines no order of entries across conversations (operator
  2026-11-28).
- Q: Can a message queued with Enter be sent immediately afterwards? → A: Yes. Every chip has a button
  "Jetzt senden" that injects it the way Ctrl+Enter does, and every chip has an X to delete it. Where
  injection is not possible (a delegate, an adapter that cannot take it, a conversation whose turn is
  not the running one) the button is not offered (operator 2026-11-28).
- Q: Attachments on injected messages, and what does the agent see? → A: Attachments are read with the
  same function as a typed send, for the first version; the agent gets an injected message as a plain
  user message with no added note (operator 2026-11-28).

## Terms

- **Conversation**: the messages between the user and one counterpart. It is one of three kinds:
  - **Agent conversation**: the user talks with a model or a delegate, as the chat does today (specs
    003, 007, 032; spec 068 reworks how the agent answers). The user may hold many of them; each is a
    **session** of the agent.
  - **Direct conversation**: the user and one contact.
  - **Group conversation**: the user and two or more contacts, with a name.
  - **Notes**: the user's conversation with themselves, shown with the own contact's picture as "Notiz an mich selber". There is at
    most one per vault.
- **Member**: a vault taking part in a direct or group conversation, identified by its vault identity
  (spec 024) and reached through its Visitenkarte (spec 051). The user's own vault is always a member.
- **Message**: what one member writes: text, attachments or both. It has a sender, a time and, for the
  sender, a delivery state.
- **Attachment**: a file sent with a message (a copy, see FR-030), or a contact passed on with spec
  060's share.
- **Delivery state**: what the sender sees about a message per recipient: _waiting_ (not yet handed to
  any Nostr-Relay), _sent_ (at least one Nostr-Relay of the recipient took it), _delivered_ (a device of
  the recipient confirmed it), _read_ (the recipient opened it, only with read receipts, FR-024), or
  _failed_.
- **Nostr-Relay**: a Nostr server that stores the encrypted messages until they are picked up. Which
  ones a contact uses is on the contact's Visitenkarte. Never just "Relay" in the interface.
- **Blossom-Server**: a server that stores encrypted files for Nostr messages until they are picked up.
- **Request**: a direct or group conversation started by a vault the user has no confirmed contact
  for, or an invitation into a group. It waits in its own area until the user accepts, declines or
  blocks it.

## Relationship to existing specs

- **Specs 003, 004, 006, 011, 046** (agent chat, window, history, composer, choice prompt): their
  behaviour is kept for agent conversations. The list, the window and the composer become shared by
  all three kinds; model selection, effort, permission mode, choice prompts and agent activity show
  only in agent conversations.
- **Spec 068** (orchestrator): owns how the agent answers in an agent conversation. This spec does not
  change the agent's turn; it gives agent conversations their place next to contact conversations.
  Both specs keep "a conversation is a chat thread".
- **Spec 024** (own-device sync): conversations and messages are vault data and appear on all own
  devices. Contact messages additionally reach every own device as Nostr wraps (FR-016), so own devices that
  are never online at the same time still show the same conversation.
- **Spec 051** (address book): contacts, groups of contacts and blocking. Its Nostr invitation contract
  (US7, ADR 0014) fixes the layers this spec uses for messages: gift wrap to a device key, seal signed
  by the sender's device key, unsigned inner message. Its area "Von Unbekannten" is this spec's
  requests (FR-040). 051 US7 itself stays with spec 027/028.
- **Spec 056** (file shares): stays the way to give live access to folders and big files. A file too
  large for the chat (FR-034) points the user there.
- **Spec 057** (extension peer channels): unrelated channels between extension frames; no change.
- **Spec 060** (contact sharing): this is the "later messenger spec" 060 names. The share dialog gets
  the target "Im Chat senden" (US6), and a contact passed on in a chat carries its photo.
- **Spec 048** (encrypted folders): received chat files are kept in an encrypted folder (FR-031).
- **Spec 021** (MCP server) and **spec 032** (model operates HaexVault): the agent permissions of US7 apply
  to the built-in agent and to agents connected over MCP alike.
- **Spec 023** (settings app): the settings of US8 are a category of the settings app.
- **Spec 086** (context for local models): the fitting and compaction of 086 keep applying to what the
  model is sent after a message was injected into a running turn (FR-100).
- **Spec 066** (voice assistant): spoken rounds take the same chat operation as a typed turn and are not
  queued by US9; a queued message waits for the voice round the way a typed message does (FR-094).
- **Spec 068** (orchestrator): US9 adds one injection point to the turn loop (`chat/turn/tool_round.rs`,
  research R22, which amends R20); how the agent answers is otherwise unchanged. 068's changes to the
  turn engine must keep that point.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - One chat app for the agent and for contacts (Priority: P1)

Anna opens the chat. It looks like the messenger on her phone: on the left the people and groups she
last wrote with, newest first, each with name, picture and the last message; unread ones are marked. The
agent is one of them, "Agent". A click on an entry shows the whole history with that person or group. A
click on "Agent" shows the agent's latest session; "Neue Sitzung" starts a fresh one, and "Verlauf"
brings back any earlier one, all her earlier talks with the agent are there. With "Neue Unterhaltung"
she picks one or more contacts. From a contact in the address book she can start a direct conversation
with "Nachricht senden".

**Why this priority**: the operator's core request: one chat that serves both purposes. Without the
shared list no other story is reachable.

**Independent Test**: Update a vault with agent conversations, open the chat, see every agent
conversation unchanged, start a direct conversation from the address book and see it in the same list.

**Acceptance Scenarios**:

1. **Given** a vault with agent conversations from before this spec, **When** Anna opens the agent's
   "Verlauf" after the update, **Then** every agent conversation is there as a session with all its
   messages, unchanged, and continues as before.
2. **Given** the list, **When** Anna looks at it, **Then** it holds one entry per contact she wrote with,
   one per group and exactly one "Agent" entry (also before her first session), newest activity first;
   each entry shows picture or icon, name, the time and the start of the last message, and agent
   sessions never show as entries of their own.
3. **Given** the list, **When** Anna clicks "Agent", **Then** the agent's latest session opens; **When**
   she chooses "Neue Sitzung", **Then** an empty session opens and "Agent" moves to the top once she
   writes; **When** she opens "Verlauf", **Then** she sees every session with its title and time, newest
   first, can open, rename or delete it.
4. **Given** a contact with a confirmed Visitenkarte in the address book, **When** Anna chooses
   "Nachricht senden", **Then** the chat opens the direct conversation with this contact, creating it if
   there is none yet.
5. **Given** a direct or group conversation is open, **When** Anna looks at the composer, **Then** it
   offers text, attachments and dictation, and no model selection, effort, permission mode or agent
   controls.
6. **Given** a contact without a confirmed Visitenkarte (no vault identity), **When** Anna looks at it,
   **Then** "Nachricht senden" is not offered, with a hint that the contact first needs to share its
   Visitenkarte.
7. **Given** the list, **When** Anna looks at it, **Then** it holds exactly one entry "Notiz an mich selber"
   with the picture of her own contact, also before her first note; **When** she clicks it and writes
   "Reifen wechseln", **Then** the note shows at once with its time and no delivery state, the entry moves
   to the top, and the note appears on her phone as a message of her own other device (gift wrap over
   her Nostr-Relays), also when her laptop went offline right after sending; **When** she attaches a
   file to a note, **Then** it goes over her Blossom-Server like a file of a direct conversation and
   her phone downloads it from there.

---

### User Story 2 - Write with a contact, whether the contact is online or not (Priority: P1)

Anna writes to Ben "Kommst du heute Abend?". Ben's vault is closed. The message shows _sent_. In the
evening Ben opens HaexVault on his phone: the message is there, Anna's side switches to _delivered_. Ben
answers; Anna sees the answer on her laptop and, later, on her phone too, even though her two devices
were never on at the same time.

**Why this priority**: messaging with contacts is the new capability; delivery while the other side is
offline is what makes it a messenger rather than a live channel.

**Independent Test**: Two vaults that are contacts of each other; close vault B, send from A, open B,
see the message on B and the state change on A; answer from B and see it on A.

**Acceptance Scenarios**:

1. **Given** Ben's vault is closed, **When** Anna sends a message, **Then** it shows _sent_ as soon as a
   Nostr-Relay of Ben took it, and Ben sees it after he next opens his vault.
2. **Given** both vaults are open, **When** Anna sends a message, **Then** Ben sees it within seconds,
   without any action.
3. **Given** Anna has no internet, **When** she sends a message, **Then** it shows _waiting_, stays in
   the conversation, and is sent by itself once a Nostr-Relay is reachable, also after a restart of the
   app.
4. **Given** a sent message, **When** a device of Ben has stored it, **Then** Anna's side shows
   _delivered_.
5. **Given** Ben has several devices on his Visitenkarte, **When** Anna sends a message, **Then** each
   of Ben's devices receives it, and Ben sees it once per conversation, never twice.
6. **Given** Anna has a laptop and a phone, **When** she writes from the laptop or receives on the
   laptop, **Then** the phone shows the same messages in the same order, also if the two devices are
   never online at the same time.
7. **Given** no Nostr-Relay of Ben took the message after all attempts, **When** Anna looks at it,
   **Then** it shows _failed_ with "Erneut senden" and the reason (for example "Keiner der Nostr-Relays
   des Kontakts ist erreichbar").
8. **Given** Ben's Visitenkarte lists no Nostr-Relay, **When** Anna writes to him, **Then** the message
   goes to Anna's own Nostr-Relays and Anna sees a hint that Ben may only receive it if he uses one of
   them.
9. **Given** Ben shared a newer Visitenkarte with a new device, **When** Anna sends the next message,
   **Then** the new device receives it too.

---

### User Story 3 - Send files to a contact (Priority: P1)

Anna attaches three photos and a PDF to a message to Ben. HaexVault puts them encrypted on Anna's
Blossom-Server and sends the message. Ben is online: the files arrive within seconds. A week later Ben
is offline; Anna sends a document anyway. When Ben opens his vault, the document is downloaded and,
once all his devices have it, the copy on the server is removed. Ben opens the files in the chat or
saves them wherever he wants.

**Why this priority**: file exchange is part of the request and the main gap today: spec 056 only gives
live access while both are online.

**Independent Test**: Send a file while both are online and see it arrive; send one while the recipient
is closed, open the recipient later and see it arrive; check the file is gone from the Blossom-Server
afterwards.

**Acceptance Scenarios**:

1. **Given** both vaults are open, **When** Anna sends a file, **Then** it is put encrypted on Anna's
   chosen Blossom-Server, and Ben's devices download it as soon as they receive the message.
2. **Given** Ben's vault is closed, **When** Anna sends a file, **Then** the same happens once Ben opens
   his vault, also if Anna's vault is closed by then.
3. **Given** a file was put on a Blossom-Server, **When** every device of every recipient and every
   other own device of Anna confirmed it has the file, **Then** Anna's vault removes it from the
   server.
4. **Given** a received file, **When** Ben taps it, **Then** it opens in HaexVault where it can show the
   type (images, text, PDF, with the file viewer of the file browser) or in the system app; "Speichern unter" copies it to a place
   of his choice.
5. **Given** a file is still being sent or received, **When** Anna or Ben looks at the message, **Then**
   it shows progress; a failed transfer shows the reason and can be retried.
6. **Given** a file larger than the chat accepts (FR-034), **When** Anna attaches it, **Then** HaexVault
   refuses it before sending and offers to share it as a file share (spec 056) instead.
7. **Given** no Blossom-Server is chosen ("Keiner"), **When** Anna wants to attach a file, **Then**
   attaching is not offered and a hint leads to the chat settings; text and contacts can still be sent.
8. **Given** Ben received a file on his laptop, **When** he opens the conversation on his phone,
   **Then** the phone has downloaded it too (or downloads it on tap above the automatic limit) while it
   is on the Blossom-Server.

---

### User Story 4 - Group conversations (Priority: P2)

Anna creates the group "Familie" with Ben and Clara, both her contacts. Ben and Clara each get an
invitation and accept. Messages and files go to every member. Clara adds her partner Dan, whom Anna
does not know; Anna sees Dan's name and can add him to her contacts. Ben leaves the group; the others
see that he left and he gets no further messages.

**Why this priority**: the operator wants groups in this spec; they build on direct conversations and
file sending.

**Independent Test**: Three vaults; create a group on A with B and C, accept on B and C, send from each
and see it everywhere; let B leave and check B gets no further messages.

**Acceptance Scenarios**:

1. **Given** at least two contacts with confirmed Visitenkarten, **When** Anna creates a group with a
   name and those contacts, **Then** the group shows in her list and each contact receives an
   invitation.
2. **Given** an invitation into a group, **When** Ben accepts it, **Then** he sees the group with its
   name and members and receives every message from then on; he does not see messages from before he
   joined.
3. **Given** an invitation, **When** Ben declines it, **Then** the group does not appear for him and the
   other members see that he declined.
4. **Given** a group, **When** any member sends a message or file, **Then** every member that has
   accepted receives it, with the same delivery behaviour as in a direct conversation (US2, US3).
5. **Given** a group, **When** a member adds one of their own contacts, **Then** that contact receives
   an invitation and every member sees the change in the conversation ("Clara hat Dan hinzugefügt").
6. **Given** a member who is not in Anna's address book, **When** Anna looks at a message of that
   member, **Then** she sees the name from that member's Visitenkarte, marked as not in her contacts,
   and can add the member to her contacts from there.
7. **Given** a group, **When** Ben leaves it, **Then** every member sees "Ben hat die Gruppe verlassen",
   new messages no longer go to Ben, and Ben's conversation stays readable for him but closed.
8. **Given** a group, **When** Anna renames it, **Then** every member sees the new name and a note in
   the conversation.
9. **Given** a group, **When** any member (here Anna) removes Clara, **Then** Clara no longer receives
   messages, sees "Anna hat dich entfernt" with her copy of the conversation readable and closed, and
   every member sees the change.
10. **Given** a group has reached the member limit (FR-052), **When** a member tries to add another,
    **Then** HaexVault refuses with the limit named.

---

### User Story 5 - Messages from unknown senders, and blocking (Priority: P2)

_Replaced 2026-11-25 (operator) by spec 099:_ a request keeps no text and no file of the sender, its answers
are "Annehmen" and "Ablehnen" (which blocks and tells the sender), a contact can be blocked, and requests
from strangers can be switched off. The text below is the state before.

Someone Anna once gave her Visitenkarte to writes to her, but Anna has never saved that person's card.
The conversation shows in "Anfragen", not in the main list. Anna sees the sender's name from the card
and the first message and chooses "Annehmen", "Ablehnen" or "Blockieren".

**Why this priority**: having one's Visitenkarte is enough to send a message; without a gate anyone
holding it could push messages straight into the list.

**Independent Test**: Vault B has A's card, A has not B's; send from B; A sees a request; accept adds B
as a contact and moves the conversation to the list; block on a second try drops further messages.

**Acceptance Scenarios**:

1. **Given** a message from a vault without a confirmed contact in Anna's address book, **When** it
   arrives, **Then** it shows under "Anfragen" with the sender's name from the Visitenkarte the message
   carries, and Anna gets no notification sound for it.
2. **Given** a request, **When** Anna accepts it, **Then** the sender becomes a contact with the
   Visitenkarte from the message (like accepting a contact share, spec 060) and the conversation moves
   to the list.
3. **Given** a request, **When** Anna declines it, **Then** the conversation is removed, the sender
   learns nothing about it, and a later message from the same sender shows as a new request.
4. **Given** a request or a contact, **When** Anna blocks the sender, **Then** further messages and
   invitations of that vault are dropped without a trace and without a delivery confirmation (as spec
   051 FR-039), and the block shows in the blocked list of the contact settings.
5. **Given** an invitation into a group from a non-contact, **When** it arrives, **Then** it is a
   request too.
6. **Given** a request, **When** Anna has not decided yet, **Then** the sender sees _delivered_ but
   never _read_.

---

### User Story 6 - Pass on a contact in the chat (Priority: P2)

In Anna's address book, "Teilen" on Clara's contact now offers "Im Chat senden". Anna picks Ben's
conversation. Ben sees Clara as a contact card in the chat, with her photo, and adds her with one tap,
after the same preview as spec 060.

**Why this priority**: spec 060 left this target to this spec; it is small once US2 exists.

**Independent Test**: Share a contact into a conversation, receive it, add it from the preview, check it
in the address book with its photo.

**Acceptance Scenarios**:

1. **Given** a contact, **When** Anna chooses "Teilen" → "Im Chat senden" and a conversation, **Then**
   the contact goes as an attachment, with the fields Anna chose in spec 060's share dialog and with its
   photo in full size.
2. **Given** a contact card in a conversation, **When** Ben taps it, **Then** he sees spec 060's
   preview and can create a new contact or merge it into an existing one.
3. **Given** the composer of a direct or group conversation, **When** Anna chooses to attach a contact,
   **Then** she can pick it from her address book and gets the same share dialog.

---

### User Story 7 - The agent and contact conversations (Priority: P3)

Anna asks the agent "Was hat Ben mir heute geschrieben?". The agent asks for permission to read the
conversation with Ben; Anna allows it. Later she says "Schreib Ben, dass ich später komme": the agent
drafts the message and Anna sees exactly the text and the recipient, and confirms it before it is sent.

**Why this priority**: useful, but only after contacts can chat; and it must not weaken privacy, so it
stays off until the user allows it.

**Independent Test**: Ask the agent to read a contact conversation with the permission off (it asks),
allow it, check the answer; ask it to send, check the confirmation dialog, decline once (nothing sent),
confirm once (sent).

**Acceptance Scenarios**:

1. **Given** the agent has no permission for contact conversations, **When** it tries to read one,
   **Then** HaexVault asks the user, naming the agent and the conversation, with "Einmal erlauben",
   "Immer erlauben für diese Unterhaltung" and "Ablehnen"; a denied conversation looks to the agent as
   if it does not exist.
2. **Given** an agent with read permission, **When** it reads a conversation, **Then** it sees text,
   senders and times, and file names; it opens a received file only if it also has the agent file
   permission for the chat file folder (the existing agent file rules).
3. **Given** any agent, **When** it wants to send a message to a contact or group, **Then** the user
   sees the exact text, any attachment and every recipient, and the message is sent only after the user
   confirmed it; this is asked every time and cannot be switched off.
4. **Given** any agent, **When** it tries to create groups, add or remove members, accept requests or
   block, **Then** it is refused; these stay with the user.
5. **Given** an agent conversation, **When** the agent answers, **Then** its answer never goes to a
   contact; the agent is never a member of a direct or group conversation.
6. **Given** an agent connected over MCP (spec 021), **When** it uses the chat, **Then** the same
   permissions and confirmations apply as for the built-in agent.

---

### User Story 8 - Chat settings (Priority: P3)

In the settings, category "Chat", Anna chooses the Blossom-Server for her files from a preset list or
enters her own, decides whether to send read receipts, sets up to which size files are downloaded
without asking, and mutes the group "Verein".

**Why this priority**: sensible defaults let US1–US7 work without it; the settings let the user decide
where her files go and what others see.

**Independent Test**: Change each setting and see its effect on the next message or file.

**Acceptance Scenarios**:

1. **Given** the category "Chat", **When** Anna picks a Blossom-Server from the list or enters an
   address, **Then** it is used for the next file and saved at once (no save button).
2. **Given** a Blossom-Server that only accepts registered keys (Blossom has no passwords; a paid plan
   registers a Nostr key), **When** Anna creates a Blossom upload key, **Then** HaexVault stores it as an
   entry of the password manager (spec 034/052), never elsewhere, and shows its public key to register
   at the server.
3. **Given** read receipts are off, **When** Anna reads a message, **Then** the sender never sees _read_
   for it, and Anna does not see _read_ for her own messages either.
4. **Given** an automatic download limit, **When** a larger file arrives, **Then** it shows with its
   size and "Herunterladen" and is fetched only on tap (it stays on the Blossom-Server until then).
5. **Given** a muted conversation, **When** a message arrives there, **Then** it is marked unread but
   there is no system notification.

---

### User Story 9 - Write while the agent is answering (Priority: P2)

Anna asks the agent to read ten pages. While it works she remembers one more thing. She types it and
presses Enter: it shows as a chip above the input field and goes to the agent as her next message once
the answer is done. A moment later she thinks of something that changes the task: she types it and
presses Ctrl+Enter, and the agent gets it before its next step, inside the same answer, without
starting over. The first thought, still waiting as a chip, matters now too: she presses "Jetzt senden"
on it and it is injected the same way. A chip she no longer wants gets its X. What waits belongs to the
conversation she typed it in.

**Why this priority**: today the send button is blocked while the agent answers; a thought typed
meanwhile either waits in the field or is lost, and a correction reaches the agent only after it has
spent its whole answer on the wrong path. It is not needed for contacts (US1–US8), which have no
running answer.

**Independent Test**: Start a turn with several tool rounds (a model asked to read three pages). Press
Enter with a second message and again with a third: two chips, no history rows. Let the turn end: the
second message is sent, then the third, in order. Start another such turn, press Ctrl+Enter with a
correction after the first round: the history shows it between the tool rows and the answer, and the
answer takes it into account. Queue a chip with Enter and press "Jetzt senden": same result. Press the
stop button with two chips: the turn stops, the first chip starts the next turn and the second follows
it. Let a turn fail with two chips: both texts
are in the input field.

**Acceptance Scenarios**:

1. **Given** the agent is not answering, **When** Anna presses Enter, **Then** the message is sent as
   today; Ctrl+Enter does the same, and Shift+Enter is a line break.
2. **Given** a turn runs in the open conversation (also while it is being set up), **When** Anna
   presses Enter on a non-empty input, **Then** the input is cleared, a chip with the start of the text
   (and the number of attachments, if any) appears above the input field, and the message is not in the
   history. The chip belongs to that conversation and shows only there. Shift+Enter still inserts a
   line break.
3. **Given** several chips in a conversation, **When** that conversation's turn ends normally, **Then**
   its first chip is sent as an ordinary new turn (its own user row and answer), the next one only after
   that turn ended, and so on; no chip is sent while a turn of its conversation runs.
4. **Given** the agent is answering in the open conversation and has tool rounds left, **When** Anna
   presses Ctrl+Enter on a non-empty input or the button "Jetzt senden" of a chip, **Then** the chip is
   marked "Wird eingefügt"; once the current tool round is done and before the agent's next model call
   the message is stored in the history after the tool results, the chip disappears, and the agent's
   next step sees the message as a normal user message at that place. Nothing of the running turn is
   cancelled or repeated.
5. **Given** a chip marked "Wird eingefügt", **When** the turn ends without another model call (the
   agent was already writing its final answer, the round cap ended it, or Anna stopped it), **Then** the message was not
   injected; it is a plain queue chip again (with its button) and is sent as the next turn in its place
   in the queue.
6. **Given** any chip, queued or marked "Wird eingefügt", **When** Anna presses its X, **Then** the chip
   is removed and nothing is sent; for a "Wird eingefügt" chip this holds only until the message was
   taken, after that it is in the history and the chip and its X are gone.
7. **Given** chips, **When** Anna stops the turn with the stop button, **Then** only the running turn of
   that conversation is interrupted (its step and tool calls); the chips stay, and afterwards they go
   out as usual: the first starts the next turn, the rest follow as their turns end, in the order
   typed. A message that was already injected stays in the history; one that was not yet taken is a
   plain chip again and goes out in its place. Chips of other conversations are not touched.
8. **Given** chips, **When** the app is closed or the vault is locked, **Then** they are gone at the next
   opening; no queue is stored in the vault or sent to other devices.
9. **Given** a model that runs its own agent loop (a delegate, spec 032), **When** Anna presses
   Ctrl+Enter while it answers, **Then** it behaves as Enter (the host has no step to inject into), and
   its chips have no "Jetzt senden" button.
10. **Given** a tool approval is waiting for Anna's answer, **When** she presses Enter or Ctrl+Enter,
    **Then** the message is queued or injected after the round as in scenarios 2 and 4, and the
    approval is neither answered nor changed by it.
11. **Given** chips in a conversation, **When** its turn ends with an error, **Then** nothing is sent
    and the texts of its chips go back into the input field: in the order typed, separated by a blank
    line, with their attachments attached again. A draft that was in the field stays, after them and
    separated by a blank line, with its own attachments. If another conversation is open meanwhile, the
    texts wait with their conversation and are put into the field when it is opened (the same rule).
    The error shows as it does today.
12. **Given** a turn runs in conversation A and Anna opens conversation B, **When** she looks at B's
    input field, **Then** A's chips are hidden and B shows only its own; Enter in B does what it does
    without a queue (see the Assumptions for today's single lock and for concurrent turns). When she
    returns to A, its chips are there and go out when A's turn ends, whichever conversation is open.
13. **Given** a queue chip in the conversation whose turn runs, **When** Anna presses "Jetzt senden" and
    the host cannot take it (the turn just ended, or the model cannot take injections), **Then** the
    chip stays queued, unmarked, and is sent as the next turn; nothing is lost and nothing is shown
    as an error.

---

### Edge Cases

- A message arrives twice (from two Nostr-Relays, or as wrap to two own devices): it shows once.
- Messages arrive out of order: the conversation orders them by the time the sender wrote them; a late
  message slots in at its place and counts as unread.
- The device clock of a sender is wrong by hours: the order follows the sender's time, but no message is
  shown later than when it arrived plus a small tolerance, so a wrong clock cannot pin a message to the
  top.
- A contact removes a device from the Visitenkarte: messages from that device signed after the newer card
  are dropped; messages already received stay.
- A contact's Visitenkarte expires or is replaced by a newer generation: the newer one is used from the
  next message on (spec 051/060 rules for newer cards).
- The recipient's devices all were removed according to a newer card: the message shows _failed_, "Der
  Kontakt hat keine Geräte mehr auf seiner Visitenkarte".
- A text is larger than one wrap holds (FR-012): it goes as a text file with a preview and shows in full
  in the conversation; without a Blossom-Server it stays in the input field with the hint to the
  settings; above 1,000,000 characters it is refused before sending with the limit named.
- A Nostr-Relay drops a stored message before the recipient picked it up: the message stays _sent_;
  another Nostr-Relay of the recipient that still has it delivers it; if none does, the sender sees no
  _delivered_ and can resend.
- The Blossom-Server removed a file before pickup (expiry, quota): the recipient sees "Datei nicht mehr
  verfügbar" and can ask the sender to send it again; the sender's side offers "Erneut senden".
- The vault is open on two own devices: both receive, store the message once in the shared vault data,
  and only one confirms _delivered_ per message.
- A contact is deleted from the address book: the conversation stays; new messages from that vault show
  as a request.
- A group member is blocked by Anna: Anna no longer sees that member's messages in the group (shown as
  "Nachricht von blockiertem Mitglied ausgeblendet"); the member still gets Anna's messages unless Anna
  leaves.
- The last other member leaves a group: the group stays readable and closed for Anna.
- The vault is locked or the app closed while a file is being sent: the transfer resumes or restarts
  after the next opening; the message keeps its state meanwhile.
- A received file has the same name as an earlier one: both are kept, distinguishable by message.
- Android: the app is in the background when a message arrives while the vault is open: a system
  notification shows; with the vault closed nothing arrives until it opens (no push service in this
  spec).
- Ctrl+Enter twice (or two "Jetzt senden" presses) in one round: both messages are injected in the order
  typed, in the same gap, and the request is fitted once afterwards.
- Ctrl+Enter while the chat is held by a model load or a spoken round (no running typed turn): there is
  nothing to inject into; it behaves as Enter and the message goes out when the chat is free (the wait
  of the existing typed send applies).
- The turn is at the round cap and takes its last step to wrap up: nothing is injected there; waiting
  messages go out as next turns.
- Ctrl+Enter or "Jetzt senden" reaches the host just as the turn ends: the host answers that no turn
  runs, the message stays a plain chip and is sent as the next turn; it is never stored twice and never
  dropped.
- The response to an injection was lost and the window sends it again: the same idempotency key returns
  the original row; no second row, no second turn. A replay of the key of an injected message never
  starts a turn and never shows an empty answer. A promoted chip keeps the key it was queued with.
- A queued message has an attachment that was deleted from the disk before it is sent: it is left out
  and its name is reported, the rest of the message goes (FR-018 of the typed send, spec 011). Attachments
  of an injected message are read the same way.
- A turn ends with an error while the field holds a draft: the returned texts come first, the draft
  after them (FR-107). An attachment already attached is not attached twice.
- A turn ends with an error in a conversation that is not open: its texts are held with it and put into
  the field when it is opened, not into the field of the conversation that is open (FR-107).
- When a conversation's turn ends, something else holds the chat (a spoken round, a model load, or today
  another conversation's turn): the send of its next entry waits as a typed send does, and when the
  wait ends it follows the failed-send path (FR-094); no entry is lost.
- The stop button is pressed while Anna looks at another conversation than the running turn's (the
  button shows for any running turn): that turn is interrupted; chips of every conversation, hers
  included, stay and go out when their own conversation's turn is over.
- A stop lands while a tool round is open (calls running or an approval waiting): as today, the calls
  are cancelled, pending approvals are dropped and the rows of the interrupted round are not stored;
  the turn ends as cancelled. An injection waiting in the inbox is not taken (the round never ends), so
  it becomes a plain chip again (FR-097) and goes out as the next turn.
- A stop lands after a message was injected: the injected row is in the history (FR-098) and stays,
  and it gets no answer from the stopped turn; the next turn, started by the first chip or by the
  person's next message, reads it as part of the history. No chip is shown for it and nothing is sent
  again.
- A stop lands while an injection is _taken in_ but not yet processed: it is either stored and reported
  (then the stop leaves it in the history) or given back unstored (then it is a plain chip); never
  both, never neither (FR-103).
- An entry is sent into a conversation that is not open, and the model of that conversation's session
  differs from the one loaded: the send loads it as opening that conversation and sending would, which
  can take a while for a large local model; the person is not asked first (see Assumptions).
- The model is changed while chips wait (possible only between turns): a chip is sent with the session
  model of the moment it is sent, as a typed message is.
- A message is injected into a turn of a model with a small context (spec 086): it is fitted like the
  rest; an injection never makes the engine refuse the prompt because it did not fit.
- The injected message is about another topic than the first one: the tools offered to the turn are
  not chosen again; the tool search of the turn finds what the first offer did not contain.
- An adapter whose wire format does not accept a user message directly after tool results: the adapter
  must handle it, tested per adapter (FR-102); until it does, Ctrl+Enter acts as Enter there and the
  chips have no "Jetzt senden" button.

## Requirements _(mandatory)_

### Functional Requirements

**One chat for every conversation**

- **FR-001**: The chat's list MUST show one entry per direct conversation, one per group conversation
  and exactly one entry "Agent" for all agent conversations, newest activity first, with name, picture
  or icon, time and the start of the last message, and an unread mark with count for direct and group
  conversations. The "Agent" entry MUST always be listed, also without any session; its activity and
  time are those of its most recently active session, its preview is that session's title. An entry
  MUST be laid out as in Signal (revision 2026-11-26): a large picture, the name with the time small
  at the top right, and under it the start of the last message over up to two lines. The time MUST be
  human friendly: "Jetzt" under a minute, "N Min." under an hour, the clock time for the rest of
  today, "Gestern", the weekday within the last week, else the date (with the year when it is not this
  year); it MUST keep up with the clock without a reload. When the last message of a direct or group
  conversation is an own one, the entry MUST show its delivery state (FR-024) at the right of the
  preview, the least advanced recipient's for a group; the unread count takes that place while there
  is something unread. The notes and the agent show no delivery state. A click on an entry MUST show that
  conversation; a click on "Agent" MUST open its most recently active session (an empty new one when
  there is none).
- **FR-002**: The list MUST NOT offer kind filters. When there are requests (FR-040) it MUST show an
  entry "Anfragen" with their count above the conversations.
- **FR-002a**: Only the agent's conversation view MUST offer "Neue Sitzung" (opens an empty agent
  conversation) and "Verlauf" (every agent conversation with title and time of last activity, newest
  first; open, rename, delete). Direct and group conversations have no sessions: their whole history is
  one conversation.
- **FR-003**: Existing agent conversations MUST be kept with all their messages and keep working as
  before; the update MUST NOT need any action of the user.
- **FR-002b**: The list MUST always show exactly one entry "Notiz an mich selber" (_changed 2026-11-20,
  operator; it read "Ich") with the picture of the user's own contact (spec 051), sorted by its last
  note like any entry. The address book still calls the own contact "Ich". A click opens the notes; the
  first note creates them. The notes view offers the same composer as a direct conversation (FR-006),
  shows each note with its time and no delivery state, and can be deleted (FR-008). _Changed
  2026-11-25 (operator):_ a note MUST be sent like a message of a direct conversation: as a gift wrap
  (FR-010) to every other device on the user's own Visitenkarte, over the Nostr-Relays on it (FR-011),
  and with attachments over the Blossom-Server (FR-030 to FR-038), so that an own device which is
  online later receives the note and its files although the sending device is offline by then. The
  note is also vault data (FR-009), so the sync of the own devices MAY deliver it as well; both ways
  land on the same row. A note MUST NOT cause receipts or notifications (the sender is the user), and
  with no other own device or no Nostr-Relay it stays in the vault only. A note that is sent follows
  FR-012 and FR-034 to FR-038 like a direct message (the limit of 1,000,000 characters, a long text
  as a file, files only with a Blossom-Server); a note that stays in the vault has no wrap to fit,
  so its text only has the limit of 1,000,000 characters.
- **FR-004**: "Neue Unterhaltung" MUST let the user choose one contact (direct) or several contacts plus
  a name (group); the agent and the notes are reached through their list entries (FR-001, FR-002b). Only contacts with a confirmed Visitenkarte of another vault are
  offered; the user's own contact ("Ich") is never offered.
- **FR-005**: The address book MUST offer "Nachricht senden" on a contact with a confirmed Visitenkarte;
  it opens the existing direct conversation or creates it.
- **FR-006**: In direct and group conversations the composer MUST offer text, attachments (files and
  contacts) and dictation (spec 008/066 speech-to-text into the text field), and MUST NOT show model
  selection, effort, permission mode, choice prompts or agent activity. In agent conversations the
  composer stays as specified by specs 011, 046 and 068.
- **FR-007**: Every message in a direct or group conversation MUST show its sender (name and picture from
  the address book, or from the sender's Visitenkarte if not a contact), its time, and for own messages
  the delivery state.
- **FR-008**: The user MUST be able to rename a group, mute or unmute any conversation, and delete a
  conversation. Deleting removes it and its messages and received files from all own devices; deleting a
  group the user is still a member of first leaves it (FR-055).
- **FR-009**: Conversations, messages, delivery states, read state and mute MUST be vault data that
  appear on all own devices (spec 024).

**Delivery over Nostr**

- **FR-010**: Every message to a direct or group conversation MUST be sent as an encrypted gift wrap
  (layers as in spec 051's Nostr invitation contract: gift wrap from a one-time key to a device key,
  seal signed by the sender's device key, unsigned inner message) to every device on the current
  Visitenkarte of every other member.
- **FR-011**: The wraps MUST go to every Nostr-Relay on the recipient's Visitenkarte; if the card lists
  none, to the sender's own Nostr-Relays, with the hint of US2 scenario 8.
- **FR-012**: A message MUST fit into one wrap within the size Nostr-Relays commonly accept. _Changed
  2026-11-20 (operator: 4,000 characters are far too few, at least 100,000):_ a text that does not fit
  MUST NOT be refused up to 1,000,000 characters. It MUST be sent as a text file (`message.txt`,
  `text/plain`, marked `inline`) with the first ~1,000 characters as the message's `text`, and every
  device of the sender and the recipient MUST show the full text in the conversation like any text,
  as soon as the file is on that device (FR-033, FR-036 never holds it back: it is below the
  automatic limit). A text over 1,000,000 characters MUST be refused before sending with the limit
  named. Without a Blossom-Server (FR-038) such a text MUST stay in the input field with the hint to
  the settings. _Changed 2026-11-25:_ a note (FR-002b) follows this rule like any message; it
  had no wrap and no file, and only the 1,000,000-character limit applied, before.
- **FR-012a**: A text longer than about twelve lines MUST be shown collapsed with a "Weiterlesen"
  button and fully shown after a click, with "Weniger anzeigen" to collapse it again; the state is
  not stored. For a message with an `inline` file the button always shows, with the size, and reads
  the text from the file on this device; while the file is coming it shows the progress, on failure
  the reason and a retry, and when the file is gone from the server the preview stays with "Text
  nicht mehr verfügbar". Search, notifications and the conversation list use the preview.
- **FR-013**: Nostr-Relays and anyone watching them MUST NOT learn the text, attachments, sender, group,
  group name or membership; they see only the recipient device key, the size and a time that is
  deliberately blurred (NIP-59).
- **FR-014**: Every device MUST, while its vault is open, receive wraps addressed to its device key from
  its own Nostr-Relays, including those stored while the vault was closed, and MUST check that the seal's
  key is a device on the sender's Visitenkarte; others are dropped silently.
- **FR-015**: Each message MUST carry a unique id; a message received more than once (several
  Nostr-Relays, several own devices) MUST be stored and shown once.
- **FR-016**: The sender MUST also send each message to its own other devices, and every received
  message reaches every own device (one wrap per own device), so every own device shows the complete
  conversation even if own devices are never online at the same time.
- **FR-017**: A message that cannot be handed to any Nostr-Relay MUST stay _waiting_ and be sent
  automatically when one is reachable, also after a restart; after the attempts end without success it
  is _failed_ with the reason and "Erneut senden".
- **FR-018**: Messages and files MUST travel only through Nostr-Relays and Blossom-Servers; there is no
  direct connection between the devices of different vaults for the chat (Clarifications 2026-11-01).
- **FR-019**: A device that stored a message MUST confirm it to the sender (a delivery receipt on the
  same path); the sender then shows _delivered_ for that recipient. No receipt of any kind is ever sent
  to a blocked vault, and no read receipt for a request not yet accepted (FR-043).
- **FR-020**: The inner message MUST carry the sender's current Visitenkarte, or a reference the
  recipient can check against a stored one, so a recipient always knows the sender's current devices
  and Nostr-Relays. A newer card received this way updates the stored card of an existing contact by the
  rules of spec 060 for newer cards.
- **FR-021**: Conversation and order: a direct conversation is identified by its two vaults; a group
  conversation by a group id. Messages MUST be ordered by the sender's time, bounded by the arrival time
  plus a tolerance (edge case "wrong clock").
- **FR-022**: Opening a conversation MUST mark its messages read on all own devices.
- **FR-023**: The app MUST show a system notification for a new message in a conversation that is not
  muted and not open, on every platform, while the vault is open; it MUST NOT show the text for a
  request.
- **FR-024**: Read receipts MUST be a setting, on by default and reciprocal: when off, the user sends none
  and sees _read_ for nobody.
- **FR-025**: Delivery to a contact MUST work on Linux, Windows, macOS and Android with the same
  behaviour; a gap on one platform is a defect.

**Files**

- **FR-030**: A file attached to a message MUST reach every recipient as a copy that stays with the
  recipient, independent of whether the sender is still online.
- **FR-031**: Received files MUST be kept in the vault's chat file folder, an encrypted folder of spec
  048, on each device that downloaded them; "Öffnen" uses the file viewer or the system app;
  "Speichern unter" writes a plain copy to a place the user picks.
- **FR-032**: The sender MUST encrypt every file with a key of its own, put it on the chosen
  Blossom-Server and send the address, the file's hash, size, type, name and the key inside the message;
  the recipient MUST check the hash before keeping the file.
- **FR-033**: Every device that receives the message (each device of each recipient and each other own
  device of the sender) MUST download the file itself from the Blossom-Server, subject to FR-036. A
  device added to a vault after the file was removed from the server does not get it.
- **FR-034**: The chat MUST accept files up to 200 MiB (209,715,200 bytes) each, and the Blossom-Server's
  own limit when it is lower; a larger file is refused before sending and the user is offered a file share
  (spec 056) instead. Sending and receiving MUST NOT hold a file in memory beyond a few segments, so the
  limit is the same on a phone as on a desktop.
- **FR-035**: Once every device that received the message confirmed it has the file, the sender's vault
  MUST remove it from the Blossom-Server. A file not confirmed within 30 days MUST be removed then; recipients that
  have not downloaded it see "Datei nicht mehr verfügbar".
- **FR-036**: Files above the automatic download limit (setting, default 25 MB; Android on mobile data:
  default only files up to 5 MB) MUST be fetched only on tap.
- **FR-037**: Sending and receiving a file MUST show progress and, on failure, the reason and a retry; a
  transfer interrupted by closing the app MUST resume or restart after the next opening.
- **FR-038**: Without a chosen Blossom-Server, attaching files MUST NOT be offered (US3 scenario 7); a
  preset public Blossom-Server (nostr.download) is chosen by default; clearing the setting on purpose means none.

**Requests and blocking**

- _FR-040 to FR-043 replaced 2026-11-25 (operator) by spec 099 FR-001 to FR-016._
- **FR-040**: A direct conversation started by, or a group invitation from, a vault without a confirmed
  contact MUST show in "Anfragen" with the name from the carried Visitenkarte, without a notification
  sound and without the message text in system notifications.
- **FR-041**: Accepting a request MUST add the sender as a contact with that Visitenkarte (preview and
  merge as spec 060) and move the conversation to the list; declining MUST remove it without telling the
  sender.
- **FR-042**: Messages and invitations from a blocked vault MUST be dropped without a trace and without
  any receipt, in direct and in group conversations; the user's block list is the one of spec 051.
- **FR-043**: Until a request is accepted, the sender MUST NOT see _read_.

**Groups**

- **FR-050**: A group MUST have a name, a picture (optional) and a member list; every member's devices
  receive every message (FR-010) and every member sees the member list with each member's state
  (invited, member, declined, left).
- **FR-051**: Adding a member MUST send an invitation that the invited vault accepts or declines in a
  HaexVault dialog; only accepted members receive messages, and from the time they joined only.
- **FR-052**: A group MUST hold up to 32 members including the user; adding beyond that is refused with
  the limit named.
- **FR-053**: Any member MAY add their own confirmed contacts. Membership changes, renames and picture
  changes MUST appear to every member as a note in the conversation, naming who did it.
- **FR-054**: Every member MUST be able to remove any other member; the removed member receives nothing
  further, keeps the conversation readable and closed, and every member sees who removed whom. There
  are no admin roles.
- **FR-055**: Every member MUST be able to leave a group; the others see it, the leaving member receives
  nothing further, and their copy of the conversation stays readable and closed.
- **FR-056**: All members MUST agree on the member list: when two members change it at the same time,
  every member ends with the same list.
- **FR-057**: A member who is not a contact of the user MUST be shown with the name from their
  Visitenkarte, marked "nicht in deinen Kontakten", with "Zu Kontakten hinzufügen".

**Contacts in the chat (spec 060)**

- **FR-060**: Spec 060's share dialog MUST offer the target "Im Chat senden", which sends the contact as
  an attachment to a chosen direct or group conversation, including its photo in full size.
- **FR-061**: A received contact attachment MUST open spec 060's preview with "Neu anlegen" and
  "Zusammenführen".

**Agents**

- **FR-070**: Agents MUST NOT read direct or group conversations without the permission "Unterhaltungen
  mit Kontakten lesen", off by default, granted per conversation or for all, asked when needed with
  "Einmal erlauben", "Immer erlauben" and "Ablehnen"; a denial MUST look to the agent as if the
  conversation did not exist.
- **FR-071**: Every message an agent wants to send to a contact or group MUST be shown to the user with
  the exact text, attachments and recipients and sent only after the user confirmed it; this
  confirmation MUST NOT be switchable off.
- **FR-072**: Agents MUST NOT create groups, change members, accept or decline requests, block, or change
  chat settings.
- **FR-073**: Agents MUST NOT be members of direct or group conversations, and nothing an agent writes in
  an agent conversation goes to a contact except through FR-071.
- **FR-074**: FR-070 to FR-073 MUST apply equally to the built-in agent, delegates and agents connected
  over MCP (spec 021).

**Settings**

- **FR-080**: The settings app MUST have a category "Chat" with: Blossom-Server (preset list of public
  servers plus own address, or none), read receipts (FR-024), automatic download limit (FR-036), and
  notifications on/off. Every change is saved on selection.
- **FR-081**: A Blossom upload key (the credential of a Blossom-Server that only accepts registered keys)
  MUST come from or go into the password manager.
- **FR-082**: The Nostr-Relays used for the chat MUST be the vault's Nostr-Relays as configured today and
  listed on the own Visitenkarte; changing them in the settings MUST update the own Visitenkarte the next
  time it is shared (spec 060).

**Writing while the agent answers**

- **FR-090**: In an agent conversation the input field MUST stay editable and sendable while a turn
  runs in that conversation, including while it is being set up. The stop button stays as it is. The
  conversations of contacts, groups and notes are not changed (FR-105).
- **FR-091**: While a turn runs in the open conversation, Enter on a non-empty input MUST append a
  queue entry; Ctrl+Enter MUST send the message as an injection (FR-095); Shift+Enter MUST stay a line
  break. While no turn runs there, Enter and Ctrl+Enter send as today. An Enter that confirms an input
  method's composition is neither. Ctrl+Enter MUST act as Enter where injection is not offered
  (FR-102, FR-106).
- **FR-092**: A queue entry MUST hold what a typed send holds: conversation, text, the paths of its
  attachments (read when the message is sent or injected, never earlier), the selected effort, and an
  idempotency key made when the entry is created and kept when the entry is promoted. The text limits
  of a typed message apply when the entry is made.
- **FR-093**: Entries MUST show as chips above the input field, in the order typed, only in the
  conversation they belong to. Each chip MUST show the start of its text and the number of its
  attachments, MUST have an X to delete it (queued and "Wird eingefügt" chips alike, FR-095 for the
  latter) and, where injection is offered (FR-102, FR-106), a button "Jetzt senden" (FR-095). A chip
  waiting for injection MUST be marked "Wird eingefügt". A waiting message MUST NOT be in the history; it
  enters it only when it is sent. The chips show no success message; a refused entry is an error toast
  (at most 10 entries per conversation, FR-104).
- **FR-094**: The entries of a conversation MUST be sent one at a time, in the order typed, and only
  after that conversation's own turn has ended: when it ends with a normal finish or a stop, the first
  entry MUST be sent as an ordinary new turn in that conversation, and the next one only after that turn
  ended. No order is defined between the entries of different conversations. A turn that ends with an
  error does not send them (FR-107). If the send fails (the chat stayed busy beyond the wait of a typed
  send, or an error), the entry follows the existing failed-send path (toast, retry) and that
  conversation's queue does not go on until the person decides.
- **FR-095**: A new command `chat_inject_message` (conversation, text, attachments, idempotency key) MUST
  hand the message to the running turn of that conversation. Ctrl+Enter and the button "Jetzt
  senden" of a chip MUST both use it, the button with the key the entry already has. The host MUST
  keep the inbox per running turn, found by the conversation, never one inbox for the whole chat, so
  turns of different conversations each have their own. The command answers _taken in_ (the turn will
  inject it or give it back) or _no running turn_ (no turn of that conversation runs, or the model
  cannot take injections); on _no running turn_ the entry MUST stay or become a plain queue entry,
  silently. A command `chat_withdraw_injection` MUST remove an entry that was not yet taken (the X of
  a "Wird eingefügt" chip) and refuse one that was.
- **FR-096**: The turn loop MUST take the entries in order after a tool round has been stored and added
  to the request, and before the next model call. For each it MUST read the attachments with the same
  function a typed send uses, store the message as a user row after the tool results of the round, make
  that row the parent of the rows that follow, add the message to the request as a plain user message,
  and emit an event naming the conversation, the row and the key. The request MUST then be fitted as in
  FR-100. Nothing of the running turn is cancelled, repeated or restarted. The step that wraps up at the
  round cap takes no injection.
- **FR-097**: When a turn ends, its inbox MUST be closed. An entry that was not taken MUST NOT be stored
  and MUST become a plain queue entry again in its place by the time it was typed, so it goes out as a
  next turn (FR-094). A _taken in_ entry that was not consumed is therefore never lost and never stored
  twice.
- **FR-098**: An injected message MUST be an ordinary user row of the conversation, synced like every
  other row of the conversation (spec 024), found by its idempotency key. A replay of the key MUST
  return the original row, MUST NOT create another row and MUST NOT start a turn. No answer row belongs
  to an injected message; the rows after it chain to it. Later requests built from the history MUST
  read a user row between tool rows and the answer correctly.
- **FR-099**: The agent MUST see an injected message as a plain user message at the place where it was
  stored, without added text.
- **FR-100**: The fitting of spec 086 MUST run again after an injection, in the order of its FR-012. The
  tool results of the round before an injected message MUST still count as the latest round for
  fitting, not as earlier results to leave out. Compaction (spec 086 FR-015) keeps the latest user
  message and the open tool round verbatim, an injected message included.
- **FR-101**: A message that is queued or injected while a tool approval waits MUST NOT answer, change or
  time out the approval; it is taken after the round as in FR-096. A stop drops the approval as today and leaves the message as FR-103 says.
- **FR-102**: A model that runs its own agent loop (a delegate, spec 032 R14) has no step to inject
  into: Ctrl+Enter MUST act as Enter there and the chips MUST have no "Jetzt senden" button. The same
  holds for an adapter whose request builder cannot take a user message directly after tool results
  (tested per adapter, task T165), where the message is queued as the next turn instead. The window learns this from the loaded model, so the button is not shown where
  it could do nothing. Local and remote adapters share the one injection point.
- **FR-103**: A stop MUST interrupt only the running turn of its conversation (its running step and tool
  calls, as today); it MUST NOT discard, return or reorder the queue entries of any conversation. When
  the turn has ended, the entries go out by FR-094. At the end of a turn (stop included) its inbox MUST
  be closed under the same lock that guards taking an entry, so that every entry that was _taken in_
  is either stored and reported before the close or given back unstored after it: never lost, never
  sent twice. An entry already injected stays in the history. A turn that ends with an error MUST NOT
  send the entries but return them as FR-107 says. Locking the vault and closing the app MUST discard
  every entry and every inbox, and nothing else does. A queue MUST NOT be written to the vault, to disk
  or to another device.
- **FR-104**: At most 10 entries MAY wait per conversation; the eleventh is refused with an error
  toast. An entry that was withdrawn, or discarded by closing the app, sends nothing, ever.
- **FR-105**: FR-090 to FR-107 apply to agent conversations of the chat app only. Contact, group and note
  conversations keep their own send and outbox (US2, FR-002b); spoken rounds of the voice assistant
  (spec 066) are not queued and still take the chat as before.
- **FR-106**: The queue is per conversation: a chip belongs to the conversation it was typed in, shows
  only there, and its conversation's queue drains when that conversation's own turn ends (FR-094).
  Injection goes only into the running turn of the entry's own conversation; where that conversation
  has no running turn, Ctrl+Enter acts as Enter and "Jetzt senden" is not shown.
- **FR-107**: When a turn ends with an error, the texts of its conversation's entries MUST be put back
  into the input field: in the order typed, each separated from the next by a blank line, their
  attachments attached again (in the same order, once each). A draft already in the field MUST stay,
  after them and separated by a blank line, with its attachments after theirs. Nothing is sent. If the
  conversation is not the open one, the texts and attachments MUST be held with it and put into the
  field by the same rule when it is opened. A stop does not return the texts; they are sent (FR-103).

### Key Entities

- **Conversation**: kind (`agent`, `direct`, `group`, `note`), name (group, or the contact's name), picture, members,
  created and last activity, mute, unread count, archived state of a left group. Agent conversations
  keep their existing data (title, model used).
- **Member**: vault identity, last known Visitenkarte, state in the conversation (invited, member,
  declined, left, removed), joined at.
- **Message**: id, conversation, sender vault and device, sender's time, arrival time, text, attachments,
  kind (text, membership note, contact card), and for own messages the delivery state per recipient.
  Agent conversation messages keep their existing roles and fields.
- **Attachment**: kind (file or contact); for files name, type, size, hash, where it is (on a
  Blossom-Server with address and key, kept on this device), transfer state.
- **Outbox entry**: a message or file still to be sent, with attempts and last error, kept across
  restarts.
- **Chat settings**: Blossom-Server, read receipts, automatic download limits, notifications.
- **Queue entry**: a message typed while the agent answers, held only in the window (never stored): its
  conversation, text, attachment paths, effort, idempotency key (kept when promoted), whether it asked
  for injection (reset when the turn ends first), and its place by the time it was typed. Shown as a
  chip in its conversation only.
- **Inbox**: the in-memory list of injection requests of one running turn (one per turn, never one for the chat); closed when the turn ends.
- **Returned texts**: the texts and attachments of a conversation's entries after a turn failed, held
  with that conversation until they are in the input field (FR-107).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: After the update, 100% of existing agent conversations and their messages are present and
  usable.
- **SC-002**: With both vaults open, 95% of messages appear at the recipient within 5 seconds of sending.
- **SC-003**: A message sent while the recipient's vault was closed appears within 30 seconds after the
  recipient opens the vault, in 95% of cases.
- **SC-004**: A message sent while the sender had no internet is sent within 30 seconds after the
  connection is back, without user action.
- **SC-005**: Inspecting every event stored on the Nostr-Relays and every blob on the Blossom-Server of a
  test run shows no message text, file content, name, group name or vault identity in readable form.
- **SC-006**: No message appears twice and none is lost across 1,000 messages exchanged between two
  vaults with two devices each, with devices going on and offline at random.
- **SC-007**: A 10 MB file sent while the recipient was offline is available at the recipient within one
  minute after the recipient opens the vault on a 50 Mbit/s connection, and is gone from the
  Blossom-Server within one minute after the confirmation reached the sender.
- **SC-008**: In a group of 10 members, every member receives every message and every file of a test run
  of 100 messages.
- **SC-009**: A user starts a conversation with a contact from the address book and sends the first
  message in under 20 seconds.
- **SC-010**: In an automated test, no agent reads a contact conversation without the permission and no
  agent message reaches a contact without the user's confirmation.
- **SC-011**: SC-002 to SC-008 hold on Linux, Windows, macOS and Android.
- **SC-012**: In an automated test with a scripted model, three messages queued with Enter during a
  turn go out as three turns in the order typed, none lost and none twice, in 20 of 20 runs.
- **SC-013**: In an automated test, a message injected with Ctrl+Enter, and one queued with Enter and
  promoted with the button "Jetzt senden", during the first of two tool rounds are each in the request
  of the third model call after the results of round one, and the stored rows read: user, tool rows,
  injected user row, answer.
- **SC-014**: In an automated test with three entries in the stopped conversation and two in another, a
  stop interrupts only the running turn, then the first three go out as three turns in the order typed
  and the other two after their own conversation's turn, none lost and none twice, in 20 of 20 runs;
  after the app is closed and opened again no entry exists.
- **SC-015**: With a model context of 8,192 tokens, an injection after a large tool result never ends in
  the engine refusing the prompt (spec 086 SC-006 holds).
- **SC-016**: In an automated test with entries queued in two conversations, each conversation's entries
  go out in typed order after that conversation's own turn ended, none lost and none twice, in 20 of 20
  runs; no order across the two conversations is asserted.
- **SC-017**: In an automated test, a turn that fails with two entries (one with an attachment) and a
  draft in the field leaves the field with the two texts in typed order, a blank line between them
  and the draft last, the attachment attached once, and nothing sent.

## Assumptions

- Both sides use HaexVault. Chatting with users of other Nostr messengers is not part of this spec:
  messages are addressed to HaexVault device keys, not to a person's general Nostr key, and the inner
  message format is HaexVault's own.
- The vault already has Nostr-Relays configured (own-device presence, spec 024) and they are on the
  own Visitenkarte; contacts who received an older card without Nostr-Relays get the hint of US2
  scenario 8.
- Sending needs the sender's device key; every own device has one (spec 024). The vault identity's
  private key is not needed to chat, so linked devices chat like main devices.
- Delivery is only while the vault is open; there is no push service that wakes a closed vault or a
  stopped Android app in this spec.
- Public Blossom-Servers may limit size, keep files only for a while or refuse uploads; HaexVault shows
  their errors and the user may choose another server. The 30-day removal (FR-035) is HaexVault's own,
  servers may remove earlier.
- Default limits (1,000,000 characters per text, 200 MiB per file, 25 MB / 5 MB automatic download, 32 group members,
  30 days on the Blossom-Server) are first values the plan may adjust with a reason.
- Message edit, "delete for everyone", reactions, replies to a single message, voice messages, calls
  and stickers are not part of this spec. _Changed 2026-11-02 (operator):_ message search is now part of
  [spec 076](../076-chat-sidebar/spec.md); real voice messages stay out until the file transfer (US3) is in.
- Received files are kept in an encrypted folder (spec 048) per the rule that data kept encrypted on disk
  goes there; their plain copies exist only where the user saves them.
- The queue is state of the window and the inbox is state of the running turn; neither is stored, so a
  restart loses them on purpose (FR-103). The limit of 10 entries is a first value the plan may adjust
  with a reason.
- Task T165 exists because some chat templates and provider APIs may reject or mishandle a user message
  placed directly after tool results, so it is tested per adapter, and an adapter that fails falls back
  to queuing the message as the next turn instead of injecting it.
- The tools of a turn are chosen from the first message (spec 018, 082, 087); an injected message does
  not choose them again, since the tool search of the turn is the way to more tools.
- A delegate (spec 032) cannot be steered from the host: its answer is whole until it ends, so only
  the queue applies there.
- The input field and the attachments of the composer are one state of the window today, not a draft
  per conversation (`input` and `attachments` in `ChatApp.vue`). FR-107 therefore holds the returned
  texts of a conversation that is not open until it is opened; this spec does not introduce drafts
  per conversation.
- Concurrent turns in different conversations are provided by spec 068 R1 and T009 (`TurnSlots`: the
  chat's `operation` Mutex in `chat/session.rs` becomes an RwLock, with `turn_cancellation` and the
  generation moving into a slot) and T015 (streams keyed by turn id in the window). Neither is built:
  checked 2026-11-28, `src-tauri` has no `TurnSlot`. This spec therefore defines nothing on the order of
  entries across conversations, and it keeps the inbox per running turn so it works unchanged once
  slots exist. Interim with today's single lock: a send in a conversation while a turn runs elsewhere
  waits up to the wait of a typed send or is refused, as today (the window's busy state is global and
  still blocks it), and a conversation's entries wait for the lock if something else holds it after
  its own turn ended.
- The window's send today reads the open conversation and shares `busy`, `turnSetupPending` and the
  streaming refs across conversations. Sending an entry into a conversation that is not open (its turn
  ended while another is open) needs that send to take the conversation explicitly (task T168); it
  then loads that conversation's session model as opening it would, without asking, because the person
  queued the message on purpose.
- The stop button shows for any running turn, whichever conversation is open (existing behaviour);
  spec 068 R1 makes `abort_current_generation` abort the active conversation's turn. Either way a stop
  only interrupts that turn and leaves every queue as it is (FR-103).
