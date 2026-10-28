// Part of `pnpm check:files`: the space of a drive or storage as the sidebar shows it.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formatSpace } from '../src/lib/files/space.ts'

const GB = 1_000_000_000

test('used and total share the unit of the total', () => {
  assert.equal(formatSpace(12.34 * GB, 100 * GB, 'en'), '12.3/100 GB')
  assert.equal(formatSpace(12.34 * GB, 100 * GB, 'de'), '12,3/100 GB')
})

test('a total in terabytes gives the used bytes in terabytes too', () => {
  assert.equal(formatSpace(0.5e12, 2e12, 'en'), '0.5/2 TB')
})

test('a small used value keeps a second digit, nothing used reads as 0', () => {
  assert.equal(formatSpace(0.04 * GB, 100 * GB, 'en'), '0.04/100 GB')
  assert.equal(formatSpace(0, 100 * GB, 'en'), '0/100 GB')
})

test('without a total only the used bytes show, in their own unit', () => {
  assert.equal(formatSpace(12.34 * GB, null, 'en'), '12.3 GB')
  assert.equal(formatSpace(512, null, 'en'), '512 B')
})
