// `pnpm check:chat-state` (spec 018): an extension's tool call is named by its extension and the
// tool's title, and its input is shown as fields (src/lib/chat/toolOrigin.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { originLabel, inputRows } from '../src/lib/chat/toolOrigin.ts'

const origin = {
  extensionId: 'e1',
  extensionName: 'Haex Mail',
  toolTitleDe: 'Ungelesene zählen',
  toolTitleEn: 'Count unread',
  dev: false,
}

test('the label names the extension and the tool title in the shown locale', () => {
  assert.equal(originLabel(origin, 'de'), 'Haex Mail: Ungelesene zählen')
  assert.equal(originLabel(origin, 'en'), 'Haex Mail: Count unread')
  assert.equal(originLabel(origin, 'fr'), 'Haex Mail: Count unread')
})

test('the input of an object is shown field by field, strings as they are', () => {
  assert.deepEqual(
    inputRows({ role: 'inbox', limit: 3, tags: ['a'], none: null }),
    [
      ['role', 'inbox'],
      ['limit', '3'],
      ['tags', '["a"]'],
      ['none', 'null'],
    ],
  )
})

test('anything but an object has no fields', () => {
  for (const input of [null, 'text', 3, [1, 2]])
    assert.deepEqual(inputRows(input), [])
  assert.deepEqual(inputRows({}), [])
})
