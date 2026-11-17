# Quickstart: Validate more secure contact chat

The behaviors the implementation must show end to end, as tests against in-process Nostr-Relays with injected time. Each scenario names the success criteria it proves. Test and command names are fixed in tasks.

## Scenario 1: A relay archive and a device key exposed later (SC-001, SC-002)

1. Two vaults exchange several messages in both directions; a test relay keeps every event.
2. The recipient device renews its start key twice, processing all wraps fetched from reachable relays before each renewal, and its vault is closed and opened again.
3. Take the recipient's device key and its key state of that moment, plus the complete relay archive.
4. Try to decrypt the archived messages from before step 2.

**Expected**: none decrypts. The wraps and seals open (sender device and time are visible), their content does not. Taking one message key decrypts that message and no other.

## Scenario 2: A copied key state stops working (SC-003, SC-010)

1. Copy the complete key state of a device that stays in use.
2. The device and a peer device exchange keep-alives until each has generated and received fresh DH ratchet contributions after the copy.
3. Send further messages between them and try to decrypt them with the copy.
4. Repeat without any user message: only time passes with both vaults open.

**Expected**: the copy cannot decrypt messages sent after the full two-sided ratchet progression. In step 4 proactive keep-alives cause that progression within 48 hours while both vaults remain open.

## Scenario 3: A device is removed (SC-003)

1. A vault with two devices chats with a contact. Copy the second device's complete state.
2. Remove the second device from the vault; the contact receives the newer card.
3. The contact sends messages.

**Expected**: no wrap is addressed to the removed device, and the copied state decrypts none of the messages. The sessions with the removed device are deleted.

## Scenario 4: A new device and a new group member (SC-008)

1. Two vaults exchange messages; a relay keeps every event.
2. Link a new device to one vault and give it the relay archive.
3. Add a third vault to a group that already has messages.

**Expected**: the new device decrypts no archived event from before it was on the card; it shows the earlier history only after the sync with an own device. Messages sent after the contact received the newer card reach it directly. The new group member receives no earlier message.

## Scenario 5: A copied vault file (SC-007)

1. Copy a vault file with existing sessions to a second installation and open it there.
2. Send and receive on both installations.

**Expected**: the second installation creates its own account and record and never reads the first one's rows. The first installation's sessions keep working.

## Scenario 6: Relays drop, reorder and replay (SC-004, SC-005, SC-009)

1. Two in-process relays: one accepts wraps and withholds them, the other delivers them in reverse order and repeats some.
2. Send 60 messages in a row; restart sender and recipient in between.
3. Then let every relay withhold a message for longer than 24 hours of injected time.

**Expected**: every message appears once and in order. _Delivered_ is shown only after the recipient's receipt, never because a relay accepted. The withheld message stays pending, is encrypted and sent again, and arrives once a relay delivers.

## Scenario 7: Lost and older key state (SC-011)

1. Delete a device's sessions, then let its peer send to it.
2. Separately: restore an older copy of a vault file on the same installation, then send and receive.
3. Separately: let two devices open sessions with each other at the same moment, deliver both opening messages in either order, then send on both endpoints.
4. Separately: a relay hands a sender an older chat key record than the one the sender already holds.

**Expected**: in 1 and 2 a reset opens a new session and every unconfirmed message arrives exactly once, without user action. In 3 both devices converge on the same lexicographically smallest retained session ID for sending. In 4 the older record is refused.

## Scenario 8: First message to an offline device (FR-003, FR-016)

1. A vault receives a contact's card and sends a message while every device of the contact is offline.
2. Separately: the contact's device has published no chat key record.

**Expected**: in 1 the message is delivered when the contact's device comes online, without the sender being online again. In 2 the message stays pending for that device and nothing is sent to it under the device key alone.

## Scenario 9: Crash safety and erasure (SC-001)

1. Interrupt receiving and sending before, inside and after the transaction that saves the session.
2. Open the vault again.
3. Before deletion, use the vault key to open the database and prove the test can recover the message key from its session state (positive control). Then delete that key, close the vault, and use the same key to inspect the database and write-ahead log, including freed pages, for recoverable session state.

**Expected**: session and message rows never disagree; the conversation continues or heals by reset. The positive control succeeds, but after close the deleted session key cannot be recovered from the database or write-ahead log.

## Scenario 10: What the user sees (SC-006)

Bring a conversation into each protection state of [data-model.md](data-model.md#protection-state-derived-not-stored).

**Expected**: the conversation view names the state in plain words and says that relays can see routing data and that the kept history is readable to whoever opens the vault.
