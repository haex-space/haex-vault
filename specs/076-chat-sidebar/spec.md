# Feature Specification: Chat sidebar as a rail, search, and one composer for contacts

**Feature Branch**: `076-chat-sidebar`

**Created**: 2026-11-02

**Status**: Draft

**Input**: User description (operator, 2026-11-02): the chat "soll noch etwas optimiert werden": the button
that hides the sidebar leaves the top row and becomes part of the sidebar; the sidebar never disappears
completely and shows only the avatars of users and bots when collapsed; the Holzi logo goes; the sidebar
gets a search to search chats; its title is just "Chats" with an icon button to start a new conversation
at its right; contact chats get voice messages, with the same input field as the agent, without model
selection and approval button.

This spec revises [spec 072](../072-unified-chat/spec.md) (unified chat): it changes how the list of
conversations looks and behaves (072 FR-001, FR-002, FR-002b, FR-004 stay as they are in content) and
takes message search out of 072's list of things that are not part of it.

## Clarifications

### Session 2026-11-02

- Q: What are "Sprachnachrichten" in contact chats? → A: Both. Dictation (speech to text into the message)
  is part of this spec, as 072 FR-006 already requires it. Real audio messages that the recipient plays
  are a later slice: they travel as a file attachment and need the file transfer of 072 US3, which is not
  finished. They are noted as a follow-up here and are not built in this spec.
- Q: What does the search look at? → A: Names and the content of messages, in contact, group and notes
  conversations and in the agent's sessions.
- Q: Order of work? → A: Sidebar first, then search and composer, as separate pull requests of this spec.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - A sidebar that is always there (Priority: P1)

The chat's list of conversations is always visible. Expanded it is a titled list; collapsed it shrinks to a
narrow strip of avatars, so the user can still jump between conversations without giving up room.
Showing and hiding it, and starting a conversation, are controls of the sidebar itself, not of a bar above
the chat.

**Why this priority**: It is the structural change the other two stories sit on, and the most visible one.

**Independent Test**: Open the chat, collapse and expand the sidebar, pick conversations from the strip,
and start a new conversation from both states; delivers a chat that never loses its list.

**Acceptance Scenarios**:

1. **Given** the sidebar is expanded, **When** the user looks at its top, **Then** they see the sidebar
   button, the title "Chats" and, at the right of it, an icon button to start a new conversation; there is
   no Holzi logo, no instance name and no wide "Neue Unterhaltung" button.
2. **Given** the sidebar is expanded, **When** the user presses the sidebar button, **Then** the sidebar
   collapses to a strip that shows the sidebar button, the button to start a new conversation and one
   avatar for each conversation (the agent, "Ich", contacts and groups), in the order of the list.
3. **Given** the sidebar is collapsed, **When** the user hovers or focuses an avatar, **Then** its name is
   shown; **When** they pick it, **Then** that conversation opens and is marked as the open one in the strip.
4. **Given** the sidebar is collapsed, **When** a conversation has unread messages or requests wait,
   **Then** its avatar (or the strip) shows that, as the expanded list does.
5. **Given** the window is narrow, **When** the user looks at the chat, **Then** the strip is still there
   beside the conversation; **When** they press the sidebar button, **Then** the full sidebar opens over
   the conversation and closes again after a pick or Escape.
6. **Given** the top row above the sidebar and conversation, **When** the chat is shown, **Then** it no
   longer exists; the conversation uses the room.
7. **Given** the user collapsed the sidebar, **When** they reopen the chat later in the same window,
   **Then** the sidebar is as they left it.

---

### User Story 2 - Search chats (Priority: P2)

The user finds a conversation or a message by typing into the sidebar's search field. Names filter the
list at once; messages containing the text are listed below with the conversation they belong to.

**Why this priority**: Useful as soon as the list is long; it needs the sidebar of story 1 but not the
composer of story 3.

**Independent Test**: With several conversations and messages, type a name and a word from a message;
delivers the list narrowed by name and a list of message hits that open their conversation.

**Acceptance Scenarios**:

1. **Given** the expanded sidebar, **When** the user looks under its header, **Then** they see a search
   field.
2. **Given** text in the search field, **When** it matches the names of conversations, **Then** the list
   shows only those; when nothing matches, it says so.
3. **Given** at least two characters, **When** messages contain that text, **Then** a section "Nachrichten"
   lists them, each with the conversation's name, a short extract around the text and its time, newest
   first; the conversations include contacts, groups, "Ich" and the agent's sessions.
4. **Given** a message hit, **When** the user picks it, **Then** its conversation opens; for the agent, the
   session that contains the message opens.
5. **Given** the search field is cleared, **When** the user looks at the sidebar, **Then** the normal list
   is back.
6. **Given** the sidebar is collapsed, **When** the user looks at it, **Then** there is no search field;
   expanding it brings the field (and a typed search) back.
7. **Given** a request that has not been accepted, **When** the user searches, **Then** its messages are
   not searched; requests stay behind "Anfragen" (072 FR-040).

---

### User Story 3 - Contact chats with the agent's input field (Priority: P1)

In a contact or group conversation the user writes with the same input field as in the agent
conversation, including dictation, but without model, effort, permission and choice controls.

**Why this priority**: 072 FR-006 already requires dictation in contact chats; the shared field also keeps
both chats looking and behaving alike.

**Independent Test**: Open a contact conversation, dictate a sentence and send it; the same field in the
agent conversation behaves as before.

**Acceptance Scenarios**:

1. **Given** a contact or group conversation, **When** the user looks at the input field, **Then** it looks
   and grows like the agent's and offers a microphone for dictation and the send button; there is no model
   or effort choice, no permission mode and no agent activity.
2. **Given** dictation with "send directly" on, **When** the user finishes speaking, **Then** the text is
   sent as one message; with it off, the text is placed in the field to edit.
3. **Given** a draft longer than the message limit (072 FR-010: at least 4,000 characters), **When** the user sends, **Then** it is
   refused with the counter as before.
4. **Given** the agent conversation, **When** the user uses its field, **Then** everything behaves as
   before this change.
5. **Given** the conversation is a request that has not been accepted, **When** the user looks at the
   bottom, **Then** there is no input field, only the answers (072 US5).

---

### Edge Cases

- A very long list of conversations: the strip and the list scroll on their own; the header with the sidebar
  button stays in place.
- Two conversations with the same name: the strip tells them apart through the avatar and the name shown on
  hover; the search lists both.
- A search for one character: names are filtered, message search waits for the second character.
- A search while messages arrive: the hits of the last typed text are shown; a result that arrives after the
  text changed is not shown.
- A search text with characters that mean something to a text match (`%`, `_`, quotes): it is matched
  literally.
- A search in a vault with many messages: typing stays responsive and the number of hits is limited, with a
  hint that there are more.
- Dictation fails (no microphone, no speech): the field shows the same messages as in the agent
  conversation and nothing is sent.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The chat's sidebar MUST be able to be hidden completely, in any window width, like the
  sidebar of every other app (`WmSidebar`). _(Revised 2026-11-14: it was "at all times"; the rail is gone.)_
- **FR-002**: The sidebar MUST show, in its header, the title "Chats" and an icon button that starts a new
  conversation (072 FR-004) at the right of the title. The sidebar button is in the window's header, as in
  every other app.
- **FR-003**: The chat MUST NOT show a row above the sidebar and the conversation that holds the sidebar
  button.
- **FR-004**: The sidebar MUST NOT show the Holzi logo, the vault's instance name or a wide button
  "Neue Unterhaltung".
- **FR-005**: _(Withdrawn 2026-11-14 with the rail: there is no collapsed sidebar with one avatar per
  conversation any more.)_
- **FR-006**: _(Withdrawn 2026-11-14 with the rail. The full list keeps marking the open conversation,
  unread messages and waiting requests.)_
- **FR-007**: In a narrow window the sidebar MUST be gone, and the sidebar button MUST open it over the
  conversation, which closes after a pick, the button or Escape.
- **FR-008**: The sidebar's state (shown or hidden in a wide window) MUST be kept when the chat is left and
  opened again in the same window.
- **FR-009**: The expanded sidebar MUST offer a search field under its header.
- **FR-010**: Typing in the search field MUST narrow the list of conversations to those whose name contains
  the text, ignoring case, at once.
- **FR-011**: With at least two characters the search MUST also find messages whose text contains the text,
  ignoring case, in contact, group and notes conversations and in the agent's sessions, and list them in a
  section "Nachrichten" with conversation name, an extract and the time, newest first, up to a limit; when
  there are more, it MUST say so.
- **FR-012**: Picking a message hit MUST open its conversation, and for the agent the session that holds
  the message. Scrolling to the message is not required.
- **FR-013**: The search MUST NOT include messages of requests that are not accepted and MUST match the
  typed text literally.
- **FR-014**: The search MUST read only vault data of the user and MUST NOT send the text anywhere.
- **FR-015**: Contact and group conversations MUST use the same input field as the agent conversation, with
  text, dictation (072 FR-006) and send, and without model selection, effort, permission mode, choice
  prompts and agent activity.
- **FR-016**: Dictation in a contact or group conversation MUST follow the user's "send directly" choice
  like the agent's field: on sends the text as one message, off places it in the field.
- **FR-017**: Sending in a contact or group conversation MUST keep its rules: text limit, one send per
  draft on retry, delivery state, errors shown at the field (072 FR-007, FR-010).
- **FR-018**: The agent conversation's input field MUST behave as before this change.

### Key Entities _(include if feature involves data)_

- **Conversation entry**: what the sidebar lists for one counterpart: name, picture, newest activity, unread
  count, whether it is the open one. Shown as a row when expanded and as an avatar when collapsed.
- **Message hit**: one message found by the search: the conversation (and for the agent the session) it
  belongs to, an extract around the match, its time.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In every window width the user can switch to any conversation with at most two actions, and
  the sidebar is never completely gone.
- **SC-002**: With 200 conversations, typing in the search field narrows the list without noticeable delay
  (under 100 ms after the last key).
- **SC-003**: With 50,000 stored messages, message hits appear within one second after the user stops typing.
- **SC-004**: A user finds a message they remember one word of, and opens its conversation, in under 15
  seconds.
- **SC-005**: A user dictates and sends a message to a contact in under 15 seconds, using the same steps as
  with the agent.
- **SC-006**: No behavior of the agent conversation's input field changes.

## Assumptions

- The sidebar's expanded width stays what it is today; the collapsed strip is just wide enough for an
  avatar and its badge.
- The sidebar state is remembered per window on the device, not synced between devices.
- "Sidebar button" and "new conversation" keep their existing automated-test handles, so existing end-to-end
  tests keep finding them.
- The search is a plain text match on stored messages; ranking, word stemming, fuzzy match and searching
  attachments or file names are not part of it. Large vaults may later need an index; the limit on hits
  keeps the plain match fast enough until then.
- Messages stay readable to the search because the vault keeps them readable to the user on the device; the
  search does not weaken 072's rules for what leaves the device (072 FR-013).
- Real audio messages (recording, sending as an attachment, playing in the conversation) are a later spec.
  They depend on 072's file transfer (US3), whose first parts are in review, and replace 072's
  "voice messages are not part of this spec" only when that spec is written.
- Jumping to the found message inside a long conversation, search inside one open conversation, and
  filters (by date, by sender) are not part of this spec.
- The instance name leaves the sidebar without replacement; where it is still useful (settings, window
  title) it stays as it is.
