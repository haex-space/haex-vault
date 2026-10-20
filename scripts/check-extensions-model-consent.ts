// Part of `pnpm check:extensions` (spec 055, T049): what the consent question lists of a step it
// would send (src/lib/extensions/modelConsent.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { sizeLabel, transferLines } from '../src/lib/extensions/modelConsent.ts'

const key = (name: string) =>
  `extensions.permissionRequest.modelDestination.${name}`

test('a short text is less than a kilobyte, not nothing', () => {
  assert.equal(sizeLabel(0), '< 1 KB')
  assert.equal(sizeLabel(1023), '< 1 KB')
  assert.equal(sizeLabel(1024), '1 KB')
  assert.equal(sizeLabel(3 * 1024 * 1024), '3 MB')
})

test('a text step lists its text only', () => {
  assert.deepEqual(
    transferLines({
      kinds: ['text'],
      textBytes: 49,
      imageCount: 0,
      imageBytes: 0,
      earlierResult: false,
    }),
    [{ key: key('text'), params: { size: '< 1 KB' } }],
  )
})

test('images are counted for the plural, an earlier result has no size', () => {
  assert.deepEqual(
    transferLines({
      kinds: ['text', 'image'],
      textBytes: 2048,
      imageCount: 2,
      imageBytes: 5 * 1024 * 1024,
      earlierResult: true,
    }),
    [
      { key: key('text'), params: { size: '2 KB' } },
      { key: key('images'), params: { count: 2, size: '5 MB' }, plural: 2 },
      { key: key('earlierResult'), params: {} },
    ],
  )
})
