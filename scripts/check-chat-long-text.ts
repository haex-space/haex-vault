// Part of `pnpm check:chat-state` (spec 072 FR-012a): what a long message offers at the end of its
// preview, by the state of its text file on this device.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  textControl,
  textFile,
  type TextFile,
} from '../src/lib/chat/longText.ts'

const file = (patch: Partial<TextFile>): TextFile => ({
  id: 'a1',
  inline: true,
  state: 'stored',
  bytesDone: 0,
  size: 1000,
  error: null,
  ...patch,
})

test('only an inline file holds the full text of a message', () => {
  const plain = { id: 'p', inline: false }
  const text = { id: 't', inline: true }
  assert.equal(textFile([plain]), null)
  assert.equal(textFile([]), null)
  assert.equal(textFile([plain, text]), text)
})

test('a stored copy is read and folded with the toggle', () => {
  assert.deepEqual(textControl(file({ state: 'stored' })), { kind: 'toggle' })
})

test('a copy that is coming shows its progress, and none shows nothing yet', () => {
  assert.deepEqual(
    textControl(file({ state: 'downloading', bytesDone: 250, size: 1000 })),
    { kind: 'loading', percent: 25 },
  )
  assert.deepEqual(
    textControl(file({ state: 'downloading', bytesDone: 5000, size: 1000 })),
    { kind: 'loading', percent: 100 },
    'never over 100 %',
  )
  assert.deepEqual(
    textControl(file({ state: 'downloading', bytesDone: 5, size: 0 })),
    { kind: 'loading', percent: null },
  )
  assert.deepEqual(textControl(file({ state: null })), {
    kind: 'loading',
    percent: null,
  })
})

test('a copy above the automatic limit waits for the user', () => {
  assert.deepEqual(textControl(file({ state: 'pending' })), {
    kind: 'download',
  })
})

test('a failed download names its reason and can be retried', () => {
  assert.deepEqual(
    textControl(file({ state: 'failed', error: 'blossom:timeout' })),
    { kind: 'retry', error: 'blossom:timeout' },
  )
})

test('a file that is gone from the server leaves the preview', () => {
  assert.deepEqual(textControl(file({ state: 'gone' })), { kind: 'gone' })
})
