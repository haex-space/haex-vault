# Feature Specification: More secure contact chat

**Feature Branch**: `feat/095-chat-forward-secrecy`

**Created**: 2026-11-17

**Status**: Draft

**Input**: User description: "Brainstorm and specify stronger chat security for Holzi; forward secrecy is important. Consider that relay operators or other readers could copy stored encrypted messages and decrypt them if a key is compromised later, and that relays can withhold or remove messages."

## User Scenarios & Testing

### User Story 1 - Old copied messages stay protected after a device key is exposed (Priority: P1)

Anna and Ben exchange messages while either vault may be offline. A relay operator copies the encrypted events. Later, one device's current long-term key is exposed. Messages whose keys were erased before that exposure remain unreadable from the copied archive. What a device still keeps in its own history is not covered: whoever can open the vault reads the history there.

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
4. **Given** a new device is added to a vault, **When** contacts learn of it from the vault's newer card, **Then** they send it the messages created from then on; it gets older history only through its own vault's sync between own devices, and relay copies of older messages stay unreadable to it.
5. **Given** a conversation in which nobody writes for a long time, **When** both vaults are open now and then, **Then** their devices still renew the keys between them without the user sending anything.

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
- A new device is added to a vault after earlier messages were sent; it gets the earlier history from its own vault's sync, and relay copies of earlier messages stay unreadable to it.
- A vault is added to a group; it gets no messages from before it joined, as in spec 072.
- A device is revoked while queued messages, read receipts, or key updates are in flight.
- A participant loses its local key state or opens an older copy of the vault file.
- The vault file is copied to another device; the copy starts with keys of its own and never uses the copied ones.
- Two devices start exchanging keys with each other at the same time.
- A relay accepts a message but drops it before the recipient reconnects; all other configured relays are unavailable.
- A group member is added or removed while some members are offline; removed members must not decrypt messages after the membership change has taken effect. A sender that has not yet seen the removal still addresses the removed member, as in spec 072.
- A user has only one configured relay, or different contacts use disjoint relay sets.
- Messages stored before this feature stay readable in the local history; relay copies made before keep their earlier protection.
- A device still sends events in the earlier format (a development vault that was not updated); they are dropped.
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
- **FR-015**: System MUST define the fate of messages queued before a device removal, group membership change or key-state loss, and of events in a format the receiver does not accept.
- **FR-016**: There is no migration (holzi has no users yet): once this feature ships, devices MUST neither send nor accept contact-chat events in the earlier long-lived-key format. Messages already stored MUST stay readable in the local history.
- **FR-017**: System MUST preserve local access to conversation history when relays are unavailable, consistent with the project's relay-unavailability principle.
- **FR-018**: A device's secret key state MUST be kept only on that device: it MUST NOT be synchronized to other devices, and a copy of the vault file opened on another device MUST start with keys of its own instead of using the copied ones.
- **FR-019**: System MUST include direct and group conversations in the first release, including membership changes and protection against removed members reading messages created after the change takes effect.
- **FR-020**: A device newly added to a vault MUST receive contact messages over relays only from the time contacts know it; older history reaches it only through its own vault's sync between own devices. A vault newly added to a group MUST NOT receive messages from before it joined.
- **FR-021**: System MUST retain unacknowledged messages in the sender's local outbox and retry them without a fixed expiration until delivery succeeds or the user deletes the message.
- **FR-022**: Every device MUST proactively send a keep-alive on each active session at least once every 24 hours while its vault is open, without the user sending anything. The system MUST report recovery from a copied key state only after both devices have contributed fresh DH ratchet keys generated after the copy; it MUST NOT infer recovery from an event count alone.
- **FR-023**: A device that cannot decrypt an event from a known device, because either side lost its key state or runs on an older one, MUST recover without user action by agreeing on fresh keys with that device; the sender MUST then encrypt its still unconfirmed messages again. No message may be lost or shown twice by this.
- **FR-024**: System MUST state that messages kept in a device's local history are outside this protection: whoever can open the vault reads them.

### Key Entities

- **Conversation security state**: What the user sees of the protection of one direct or group conversation, derived from the key state with the participants' devices.
- **Message key**: Secret material that protects one message or a narrowly bounded message interval and can be erased independently.
- **Device participant**: One authorized installation in a vault, with its own key state.
- **Device session**: The key state between one device and one peer device (of a contact or of the own vault); it advances with every message and is renewed with every answer.
- **Chat key record**: The public keys a device publishes so that others can start a device session with it while it is offline.
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
- **SC-007**: A copy of a vault file opened on a second device never sends or decrypts with the first device's keys.
- **SC-008**: A device newly added to a vault cannot decrypt relay copies of messages sent before contacts knew it; a vault newly added to a group receives no earlier messages.
- **SC-009**: A message without recipient acknowledgement remains available for retry after app restarts and relay outages of any duration, unless the user deletes it.
- **SC-010**: After a device's key state was copied, it is reported recovered only after both endpoints have generated and received fresh DH ratchet contributions that advance both sides of the Double Ratchet beyond the copied state. With both vaults open, proactive keep-alives and prompt answers complete this progression within 48 hours without user messages; subsequent messages cannot be decrypted with the copy.
- **SC-011**: After either side lost its key state or went back to an older one, the conversation continues without user action and every unconfirmed message arrives exactly once.

## Assumptions

- The threat model includes a relay operator or relay-reading client that archives public relay events and later obtains a device key.
- Relay operators are not trusted to retain, deliver, or delete events correctly.
- Message contents need confidentiality and authenticity; metadata privacy is a separate, limited goal because routing requires recipient discovery.
- Chat remains asynchronous and continues to use relays as store-and-forward transport; always-online peer connections are not assumed.
- The Nostr event format changes without migration: holzi has no users yet, and development vaults are updated together.
- The current chat's NIP-44/NIP-59 wrapping is not forward-secret; an outer one-time gift-wrap key does not remove the long-term device key's ability to decrypt archived inner ciphertext.
- Direct and group conversations receive the new security guarantees in the first release.
- Every pair of devices keeps its own key state; a message is protected once per receiving device, as spec 072 already sends one copy per device. Group membership stays as spec 072 defines it, without a coordinator.
- A device newly added to a vault gets the vault's chat history through the sync between own devices, as every other vault content. Linking the device is the user's consent to that.
- Unacknowledged messages remain in the sender's local outbox and retry indefinitely unless the user deletes them.
- The protection covers copies outside the devices (relays and whoever reads them). A device's kept history and its current key state are in the same vault file; forward secrecy therefore matters for messages no longer kept, and recovery matters against someone who copied a key state once.
- Recovery after compromise holds against someone who copied a device's state and only listens. Someone who holds a device's keys and also publishes as that device is stopped only by removing the device from its vault.
- A device key exposed later still reveals, for archived relay events addressed to that device, which device sent them, when and how large they were, but not their content.
