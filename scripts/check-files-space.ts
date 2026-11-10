// Part of `pnpm check:files`: the space of a drive or storage as the sidebar shows it.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formatSpace, usedShare } from '../src/lib/files/space.ts'

const GB = 1_000_000_000

test('a value shows in the largest unit in which it is at least 1', () => {
  assert.equal(formatSpace(12.34 * GB, 'en'), '12.3 GB')
  assert.equal(formatSpace(12.34 * GB, 'de'), '12,3 GB')
  assert.equal(formatSpace(0.5e12, 'en'), '500 GB')
  assert.equal(formatSpace(2e12, 'en'), '2 TB')
  assert.equal(formatSpace(512, 'en'), '512 B')
})

test('a small value moves to the smaller unit, nothing reads as 0 of a bigger one', () => {
  assert.equal(formatSpace(0.04 * GB, 'en'), '40 MB')
  assert.equal(formatSpace(0, 'en'), '0 B')
})

test('the used share is the used part of the total', () => {
  assert.equal(usedShare(25 * GB, 100 * GB), 0.25)
  assert.equal(usedShare(0, 100 * GB), 0)
})

test('the used share stays within 0 and 1', () => {
  assert.equal(usedShare(150 * GB, 100 * GB), 1)
  assert.equal(usedShare(-1, 100 * GB), 0)
  assert.equal(usedShare(5, 0), 0)
})
