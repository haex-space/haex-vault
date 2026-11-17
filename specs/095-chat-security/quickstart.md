# Quickstart: Validate secure chat design

This guide defines the end-to-end behaviors implementation must demonstrate. It does not prescribe source code or exact test command names.

## Scenario 1: Archive plus later device-key exposure

1. Create two vaults with an authorized direct conversation and at least two devices on one side.
2. Exchange several messages, ensuring processed message secrets are erased.
3. Capture all relay events and a snapshot of the recipient's current long-term device credential.
4. Expose that current credential after later ratchet progress.
5. Verify captured earlier ciphertext remains undecryptable with the exposed credential, while authorized current devices can continue the conversation.

**Expected**: The test fails if a long-term device key or current ratchet state can derive erased earlier message keys.

## Scenario 2: Group membership removal

1. Create a group with devices belonging to three vaults.
2. Capture one removed device's complete state before removal.
3. Remove that device and complete an authenticated epoch transition.
4. Send messages in the new epoch and deliver them to remaining members.
5. Attempt to decrypt them with the captured old state.

**Expected**: Remaining members can read the messages; the removed device cannot. A dropped membership commit leaves the sender in a visibly pending/degraded state until recovery.

## Scenario 3: New device and history boundary

1. Send messages in an existing conversation.
2. Add a newly generated device through an authenticated key package and membership transition.
3. Deliver new messages to that device and replay older relay events to it.
4. Verify older messages remain unreadable unless the user separately authorizes history transfer.

**Expected**: Membership addition grants prospective access only; old events do not decrypt under the new epoch.

## Scenario 4: Relay drop/reorder/replay

1. Configure two independent in-process relays.
2. Make one relay accept but withhold messages; make the other deliver copies out of order and replay a duplicate.
3. Restart sender and recipient between delivery attempts.
4. Verify local history persists, outbox retries continue, duplicate messages appear once, and status distinguishes relay acceptance from recipient acknowledgement.

**Expected**: Relay behavior cannot silently delete local history, forge delivery, or determine trusted group membership/order.

## Scenario 5: Crash-safe state and key erasure

1. Interrupt processing before, during, and after a message receive/commit transaction.
2. Reopen the vault and inspect persisted state through supported test interfaces.
3. Verify the app either resumes one consistent protocol state or explicitly enters recovery; it never reuses an already-erased message key.
4. Exercise the configured database backup, WAL/checkpoint, and restore lifecycle.

**Expected**: Ratchet state and message state cannot diverge after a crash. The security claim is withheld if old secret material can be recovered from app-managed copies.
