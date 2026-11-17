// Part of `pnpm check:contacts` (spec 051): how the window names an account of the phone
// (src/lib/contacts/accounts.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  accountLine,
  splitOriginAccount,
} from '../src/lib/contacts/accounts.ts'

const PHONE = 'Telefon (ohne Konto)'

test('a messenger account shows its service, not the id behind it', () => {
  assert.equal(
    accountLine('org.telegram.messenger', '1911554025', PHONE),
    'Telegram',
  )
})

test('an account named by an e-mail address keeps the address', () => {
  assert.equal(
    accountLine('com.google', 'mara@example.org', PHONE),
    'Google (mara@example.org)',
  )
})

test('an unknown type shows its name and type, so nothing is hidden', () => {
  assert.equal(
    accountLine('org.example.sync', 'work', PHONE),
    'org.example.sync (work)',
  )
  assert.equal(accountLine('org.example.sync', '', PHONE), 'org.example.sync')
  assert.equal(accountLine('toString', 'work', PHONE), 'toString (work)')
  assert.equal(accountLine('constructor', '', PHONE), 'constructor')
})

test('an account without a type is the phone itself', () => {
  assert.equal(accountLine('', 'Phone', PHONE), PHONE)
})

test('the origin account splits at the first slash only', () => {
  assert.deepEqual(splitOriginAccount('org.telegram.messenger/1911554025'), [
    'org.telegram.messenger',
    '1911554025',
  ])
  assert.deepEqual(splitOriginAccount('com.google/a/b@c.d'), [
    'com.google',
    'a/b@c.d',
  ])
  assert.deepEqual(splitOriginAccount('local'), ['local', ''])
})
