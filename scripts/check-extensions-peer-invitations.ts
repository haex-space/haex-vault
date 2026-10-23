// Part of `pnpm check:extensions` (spec 057, T043): the queue of invitation prompts
// (src/lib/extensions/peerInvitations.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  current,
  enqueue,
  remove,
} from '../src/lib/extensions/peerInvitations.ts'
import type { ExtensionPeerInvitation } from '../src/types/bindings/ExtensionPeerInvitation.ts'

function invitation(inviteId: string, raise = false): ExtensionPeerInvitation {
  return {
    inviteId,
    extensionId: 'ext-1',
    extensionName: 'Doom',
    contactName: 'Ana',
    raise,
  }
}

test('several open prompts queue and the oldest is asked first', () => {
  let queue = enqueue([], invitation('i1'))
  queue = enqueue(queue, invitation('i2'))
  assert.equal(current(queue)?.inviteId, 'i1')
  assert.deepEqual(
    queue.map((i) => i.inviteId),
    ['i1', 'i2'],
  )
})

test('an invitation shown again keeps its place and takes the new content', () => {
  let queue = enqueue([], invitation('i1'))
  queue = enqueue(queue, invitation('i2'))
  queue = enqueue(queue, invitation('i1', true))
  assert.deepEqual(
    queue.map((i) => [i.inviteId, i.raise]),
    [
      ['i1', true],
      ['i2', false],
    ],
  )
})

test('an answered or gone invitation leaves the queue', () => {
  let queue = [invitation('i1'), invitation('i2')]
  queue = remove(queue, 'i1')
  assert.equal(current(queue)?.inviteId, 'i2')
  queue = remove(queue, 'unknown')
  assert.equal(queue.length, 1)
  assert.equal(current(remove(queue, 'i2')), undefined)
})
