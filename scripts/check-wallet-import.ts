// Checks the place of an imported pass as the dialog edits it (spec 092, FR-019, FR-031).
// Run: node --test scripts/check-wallet-import.ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  pathText,
  startFields,
  tagOrNull,
  textPath,
} from '../src/lib/wallet/importPlace.ts'

test('a path is one line and back', () => {
  assert.equal(pathText(['Work', 'Trips']), 'Work / Trips')
  assert.deepEqual(textPath('Work / Trips'), ['Work', 'Trips'])
  assert.deepEqual(textPath(pathText(['A', 'B', 'C'])), ['A', 'B', 'C'])
})

test('empty names and stray separators are left out', () => {
  assert.deepEqual(textPath('Work//Trips/'), ['Work', 'Trips'])
  assert.deepEqual(textPath('  /  / '), [])
  assert.deepEqual(textPath(''), [])
  assert.deepEqual(textPath('  Work  '), ['Work'])
})

test('a tag is trimmed and none when empty', () => {
  assert.equal(tagOrNull('  Ticket '), 'Ticket')
  assert.equal(tagOrNull('   '), null)
})

test('a start becomes the fields the date helpers read', () => {
  assert.deepEqual(startFields(null), {})
  assert.deepEqual(startFields({ date: '2026-11-14' }), {
    startDate: '2026-11-14',
  })
  assert.deepEqual(
    startFields({ at: '2026-11-14T07:30:00.000Z', zone: '+01:00' }),
    {
      startAt: '2026-11-14T07:30:00.000Z',
      startZone: '+01:00',
    },
  )
})
