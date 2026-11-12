// Part of `pnpm check:passwords` (spec 092-wallet-passes, FR-001, FR-042): the type of an entry
// (src/lib/passwords/passType.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { entryTypeOf, isEditable } from '../src/lib/passwords/passType.ts'

test('an entry of a vault from before the types is a login', () => {
  assert.equal(entryTypeOf({}), 'login')
  assert.equal(entryTypeOf({ kind: null }), 'login')
  assert.equal(entryTypeOf({ kind: '' }), 'login')
  assert.equal(entryTypeOf(undefined), 'login')
  assert.equal(entryTypeOf({ kind: 'login' }), 'login')
})

test('a pass is a pass', () => {
  assert.equal(entryTypeOf({ kind: 'pass' }), 'pass')
})

test('a type of a newer holzi is unknown, never an empty login', () => {
  assert.equal(entryTypeOf({ kind: 'payment_card' }), 'unknown')
  assert.equal(entryTypeOf({ kind: 'Login' }), 'unknown')
})

test('an unknown type is not editable, the others are', () => {
  assert.equal(isEditable({ kind: 'payment_card' }), false)
  assert.equal(isEditable({ kind: 'login' }), true)
  assert.equal(isEditable({ kind: 'pass' }), true)
})
