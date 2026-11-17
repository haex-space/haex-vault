# Feature Specification: More secure contact chat

**Feature Branch**: `095-chat-forward-secrecy`

**Created**: 2026-11-17

**Status**: Draft

**Input**: User description: "Brainstorm and specify stronger chat security for Holzi; forward secrecy is important. Consider that relay operators or other readers could copy stored encrypted messages and decrypt them if a key is compromised later, and that relays can withhold or remove messages."

## User Scenarios & Testing

### User Story 1 - Old copied messages stay protected after a device key is exposed (Priority: P1)

Anna and Ben exchange messages while either vault may be offline. A relay operator copies the encrypted events. Later, one device's current long-term key is exposed. Messages from conversations that were securely erased before that exposure remain unreadable from the copied archive.

**Why this priority**: The current design encrypts message contents to long-lived device keys, so a future key compromise can expose archived traffic. Forward secrecy directly addresses this risk.

**Independent Test**: Capture encrypted traffic from a test conversation, advance and erase session key state, expose the current device secret, and verify that earlier captured message contents cannot be recovered while current messages can still be delivered and read by intended devices.

**Acceptance Scenarios**:

1. **Given** two devices have exchanged messages and securely erased superseded message keys, **When** an attacker later obtains the recipient's current long-term device key and the complete captured relay archive, **Then** the attacker cannot decrypt messages protected by erased keys.
2. **Given** a conversation has advanced through multiple messages, **When** one message key is exposed, **Then** exposure of that key does not reveal messages protected by other erased message keys.
3. **Given** the recipient is offline while messages are queued on relays, **When** the recipient returns, **Then** it can recover the keys needed for its pending messages without requiring the sender to be online.

### User Story 2 - Compromised devices can recover future message security (Priority: P1)

Anna notices that one of her devices may have been compromised. She removes that device from her vault. Other devices establish fresh conversation security so that an attacker retaining the removed device's old state cannot read later messages.

**Why this priority**: Forward secrecy protects the past, while recovery after compromise protects the future. Both are needed for a chat system whose devices can be lost or compromised.

**Independent Test**: Copy a device's full pre-removal state, revoke it, establish fresh keys with remaining devices, and show that the copied state cannot decrypt messages sent after recovery.

**Acceptance Scenarios**:

1. **Given** a device is removed from a vault or contact membership, **When** the remaining participants complete key refresh, **Then** messages created after the refresh are not decryptable with the removed device's prior state.
2. **Given** a participant's current secret is exposed, **When** the participant completes an authenticated recovery exchange with a trusted peer, **Then** new message keys become unknown to an attacker holding only the exposed prior state.
3. **Given** recovery cannot be completed because a required participant is offline, **When** a user sends a message, **Then** the application clearly indicates whether that message is protected by recovered keys or remains exposed to the known compromise.
4. **Given** a new device is added to a vault, **When** it joins an existing conversation, **Then** it receives messages created after joining; older history is available only after an explicit user-approved transfer with a warning that it cannot gain retroactive forward secrecy.

### User Story 3 - Relay interference does not silently erase the only copy (Priority: P1)

Anna sends a message to Ben. One relay accepts it and later disappears or withholds it. Anna can tell which relays accepted the message, and Holzi retries delivery through other configured relays. A relay's deletion or refusal never silently changes the conversation history on either device.

**Why this priority**: Relays control availability. The current delivery indicator only establishes that at least one relay accepted an event; it cannot prove the relay retained or delivered it.

**Independent Test**: Use multiple controlled relays; accept an event on one and withhold it, while another accepts and delivers it. Verify the message remains in the sender's outbox and the recipient gets it once from any available copy.

**Acceptance Scenarios**:

1. **Given** one relay accepts a message and then withholds or removes it, **When** another configured relay remains available, **Then** Holzi delivers the same authenticated message from that relay without showing duplicates.
2. **Given** every configured relay refuses or fails to retain a message, **When** the sender views delivery state, **Then** it is not shown as delivered and remains retryable with a clear failure state.
3. **Given** a relay returns an event that was modified or forged, **When** Holzi processes it, **Then** it rejects the event and preserves the conversation state.
4. **Given** relays disagree about which events they hold, **When** Holzi reconciles delivery, **Then** valid events are deduplicated by stable message identity and missing messages remain discoverable for retry.

### User Story 4 - Users understand the remaining privacy and delivery limits (Priority: P2)

Anna reviews a contact conversation and can understand whether it uses refreshed message security, whether a participant is offline or removed, and what message metadata relays can still observe.

**Why this priority**: Security state is useful only when the user can distinguish protected, degraded, and unrecoverable conversations.

**Independent Test**: Exercise healthy, offline, key-recovery-pending, and relay-failure states and verify the interface gives accurate, actionable descriptions without claiming that relay acceptance proves delivery.

**Acceptance Scenarios**:

1. **Given** a conversation has not completed secure key recovery after a device compromise, **When** the user views its security state, **Then** Holzi explains that future messages may remain exposed until recovery completes.
2. **Given** a relay can observe routing metadata, **When** Holzi explains the chat's privacy, **Then** it does not claim that encryption hides recipient routing, timing, event size, or network address.
3. **Given** a message was accepted by a relay but not acknowledged by a recipient device, **When** the sender views its status, **Then** Holzi distinguishes relay acceptance from recipient delivery.

### Edge Cases

- A sender or recipient has several devices, some offline for a long time, and each device has independent message-key progress.
- A new device is added after earlier messages were sent; it must not gain access to old messages unless the user explicitly chooses a history transfer.
- A user explicitly transfers history to a new device; the application explains that older messages do not gain retroactive forward secrecy.
- A device is revoked while queued messages, read receipts, or key updates are in flight.
- A participant loses local ratchet state or restores an old vault backup.
- A relay accepts a message but drops it before the recipient reconnects; all other configured relays are unavailable.
- A group member is added or removed while some members are offline; removed members must not decrypt messages after the membership change has taken effect.
- A user has only one configured relay, or different contacts use disjoint relay sets.
- Existing stored messages use the current long-lived device-key encryption and cannot be retroactively given forward secrecy.
- Sender and recipient run different protocol versions during rollout.
- An attacker copies encrypted events but never obtains a device secret; message contents remain confidential.
- An attacker obtains a device secret and an old database or backup containing un-erased session state.

## Requirements

### Functional Requirements

- **FR-001**: System MUST protect newly sent contact messages so that compromise of a participant's current long-term device key alone does not reveal messages whose message keys have already been erased.
- **FR-002**: System MUST use distinct cryptographic message-key state so exposure of one message key does not expose unrelated past or future messages after their keys have been erased or replaced.
- **FR-003**: System MUST support asynchronous delivery: a sender and recipient do not need to be online at the same time for a recipient to recover the keys for queued messages.
- **FR-004**: System MUST define how message-key state advances, is acknowledged, and is safely erased across multiple devices belonging to one vault.
- **FR-005**: System MUST define how group messages receive forward secrecy and how membership changes prevent removed members from reading subsequent messages.
- **FR-006**: System MUST provide a recovery path that can establish fresh message security after compromise, and MUST state what authenticated action or peer contribution is required for recovery.
- **FR-007**: System MUST NOT claim that forward secrecy is restored solely by deleting a device locally; remaining participants must establish fresh secret material unknown to the removed device.
- **FR-008**: System MUST keep pending messages deliverable to authorized offline devices without retaining superseded message keys longer than required by the agreed delivery policy.
- **FR-009**: System MUST retain the sender's authoritative message record independently of relay retention, so relay deletion cannot silently delete the sender's conversation history.
- **FR-010**: System MUST distinguish relay acceptance, recipient-device delivery, and read acknowledgement in message status.
- **FR-011**: System MUST retry or replicate messages over the configured recipient relays according to a defined policy, deduplicating valid copies at the recipient.
- **FR-012**: System MUST reject altered, forged, replayed, or unauthorized messages without changing conversation state.
- **FR-013**: System MUST make security-state degradation visible when required key updates, acknowledgements, or recovery exchanges have not completed.
- **FR-014**: System MUST explain that relays can observe routing and traffic metadata and can refuse, withhold, or delete their own copies of events.
- **FR-015**: System MUST define the fate of messages queued before a device removal, group membership change, key-state loss, or protocol-version mismatch.
- **FR-016**: System MUST provide a migration strategy for conversations and queued events created before this feature; existing ciphertext MUST NOT be described as retroactively forward-secret.
- **FR-017**: System MUST preserve local access to conversation history when relays are unavailable, consistent with the project's relay-unavailability principle.
- **FR-018**: System MUST support interoperability during rollout through an explicit, user-visible security status and a documented minimum compatible version policy.
- **FR-019**: System MUST include direct and group conversations in the first release, including membership changes and protection against removed members reading messages created after the change takes effect.
- **FR-020**: System MUST give a newly added device access only to messages created after it joins by default; older history MAY be transferred only after an explicit user action and a warning about the security implications.
- **FR-021**: System MUST retain unacknowledged messages in the sender's local outbox and retry them without a fixed expiration until delivery succeeds or the user deletes the message.

### Key Entities

- **Conversation security state**: The current cryptographic epoch and participant/device progress for one direct or group conversation.
- **Message key**: Secret material that protects one message or a narrowly bounded message interval and can be erased independently.
- **Device participant**: One authorized installation in a vault, with its own membership and key-state progress.
- **Membership epoch**: The participant set and key generation that govern which devices may read messages in a group.
- **Delivery copy**: A relay-held copy of an encrypted message, independent from the sender's authoritative local record.
- **Recovery event**: An authenticated exchange that replaces exposed key state with secret material unavailable to the compromised state.

## Success Criteria

### Measurable Outcomes

- **SC-001**: In security tests, possession of a current long-term device key and a complete relay archive does not decrypt any earlier messages whose required message keys were erased.
- **SC-002**: Exposure of a single message key reveals no other message whose key is distinct and erased.
- **SC-003**: After a compromised device is removed and recovery completes, that device's captured prior state cannot decrypt any later message in the conversation.
- **SC-004**: With at least one available configured relay retaining a valid copy, the recipient receives a message once, even if other relays withhold or delete their copies.
- **SC-005**: No message is shown as recipient-delivered solely because a relay accepted it.
- **SC-006**: Users can identify when forward secrecy or post-compromise recovery is not active, without interpreting raw protocol logs.
- **SC-007**: Existing conversation history remains readable after migration, and the application clearly distinguishes legacy messages from messages protected by the new security guarantees.
- **SC-008**: A newly added device receives no pre-join message content unless the user explicitly authorizes a history transfer.
- **SC-009**: A message without recipient acknowledgement remains available for retry after app restarts and relay outages of any duration, unless the user deletes it.

## Assumptions

- The threat model includes a relay operator or relay-reading client that archives public relay events and later obtains a device key.
- Relay operators are not trusted to retain, deliver, or delete events correctly.
- Message contents need confidentiality and authenticity; metadata privacy is a separate, limited goal because routing requires recipient discovery.
- Chat remains asynchronous and continues to use relays as store-and-forward transport; always-online peer connections are not assumed.
- A security protocol may change the current Nostr event format and require a staged rollout.
- The current chat's NIP-44/NIP-59 wrapping is not forward-secret; an outer one-time gift-wrap key does not remove the long-term device key's ability to decrypt archived inner ciphertext.
- Direct and group conversations receive the new security guarantees in the first release.
- A newly added device receives only messages created after joining unless the user explicitly authorizes transfer of older history.
- Unacknowledged messages remain in the sender's local outbox and retry indefinitely unless the user deletes them.
