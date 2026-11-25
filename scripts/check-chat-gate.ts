// Part of `pnpm check:chat-state` (spec 099 FR-005a): the sender's hint shows only while nothing
// came back from the other vault and an own message waited a minute, never in a request, and not
// once the other vault blocked (which has its own notice).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { HINT_AFTER_MS, showsNotYetAccepted } from '../src/lib/chat/gate.ts'

const direct = {
  kind: 'direct',
  state: 'active',
  peerAnswered: false,
  blockedByPeer: false,
}
const waiting = { own: true, sentAt: 0, delivery: 'sent' }

test('the hint waits a minute and needs silence from the other vault', () => {
  assert.equal(showsNotYetAccepted(direct, [waiting], HINT_AFTER_MS - 1), false)
  assert.equal(showsNotYetAccepted(direct, [waiting], HINT_AFTER_MS), true)
  assert.equal(
    showsNotYetAccepted(
      { ...direct, peerAnswered: true },
      [waiting],
      HINT_AFTER_MS,
    ),
    false,
    'something came back once: no guessing',
  )
  assert.equal(
    showsNotYetAccepted(
      direct,
      [{ ...waiting, delivery: 'delivered' }],
      HINT_AFTER_MS,
    ),
    false,
  )
  assert.equal(
    showsNotYetAccepted(direct, [{ ...waiting, own: false }], HINT_AFTER_MS),
    false,
  )
})

test('no hint in a request, a note or after a block', () => {
  for (const conversation of [
    { ...direct, state: 'request' },
    { ...direct, kind: 'note' },
    { ...direct, blockedByPeer: true },
  ]) {
    assert.equal(
      showsNotYetAccepted(conversation, [waiting], HINT_AFTER_MS),
      false,
    )
  }
})
