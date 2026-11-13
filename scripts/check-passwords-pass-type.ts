// Part of `pnpm check:passwords` (spec 092-wallet-passes, FR-001, FR-042): the type of an entry
// (src/lib/passwords/passType.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  PASS_KIND_ICONS,
  PASS_KINDS,
  entryTypeOf,
  isEditable,
  passFallbackIcon,
  passKindOf,
} from '../src/lib/passwords/passType.ts'

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

test('an unknown or missing kind of pass reads as other and the kinds keep their own icon', () => {
  assert.equal(passKindOf({ passKind: 'season_ticket' }), 'other')
  assert.equal(passKindOf({ passKind: null }), 'other')
  assert.equal(passKindOf(undefined), 'other')
  for (const kind of PASS_KINDS) {
    assert.equal(passKindOf({ passKind: kind }), kind)
    assert.match(PASS_KIND_ICONS[kind], /^lucide:[a-z-]+$/)
  }
  assert.equal(
    new Set(Object.values(PASS_KIND_ICONS)).size,
    PASS_KINDS.length,
    'every kind has its own icon',
  )
})

test('the fallback icon is the kind of a pass and nothing for any other entry', () => {
  assert.equal(
    passFallbackIcon({ kind: 'pass', passKind: 'boarding_pass' }),
    'lucide:plane',
  )
  assert.equal(passFallbackIcon({ kind: 'pass' }), 'lucide:wallet')
  assert.equal(passFallbackIcon({ kind: 'login' }), null)
  assert.equal(passFallbackIcon({ kind: 'payment_card' }), null)
})
