// Part of `pnpm check:extensions` (spec 017, T064): the queue of permission questions of
// extensions (src/lib/extensions/queue.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  current,
  enqueue,
  remove,
  type PermissionQuestion,
} from '../src/lib/extensions/queue.ts'

function question(
  requestId: string,
  target: string,
  extensionId = 'ext-1',
): PermissionQuestion {
  return {
    requestId,
    extensionId,
    displayName: 'Notizen',
    kind: 'database',
    action: 'read',
    target,
    declared: false,
    deviceScoped: false,
    targetMissing: false,
    byAgent: false,
  }
}

test('identical questions are one and the oldest is shown first', () => {
  let queue = enqueue([], question('r1', 'a'))
  queue = enqueue(queue, question('r2', 'a'))
  queue = enqueue(queue, question('r3', 'b'))
  queue = enqueue(queue, question('r4', 'a', 'ext-2'))
  assert.deepEqual(
    queue.map((q) => q.requestId),
    ['r1', 'r3', 'r4'],
  )
  assert.equal(current(queue)?.requestId, 'r1')
})

test('an answered, cancelled or dropped question leaves the queue', () => {
  let queue = [question('r1', 'a'), question('r2', 'b')]
  queue = remove(queue, 'r1')
  assert.equal(current(queue)?.requestId, 'r2')
  queue = remove(queue, 'unknown')
  assert.equal(queue.length, 1)
  assert.equal(current(remove(queue, 'r2')), undefined)
})
