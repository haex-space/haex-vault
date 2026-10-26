// Part of `pnpm check:contacts`: event dates as a person reads and types them
// (src/lib/contacts/eventDate.ts, src/lib/contacts/kinds.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  datePattern,
  formatEventDate,
  parseTypedDate,
} from '../src/lib/contacts/eventDate.ts'
import { displayValue } from '../src/lib/contacts/kinds.ts'

test('a stored date reads in the order of the language', () => {
  assert.equal(formatEventDate('1990-05-17', 'de'), '17.05.1990')
  assert.equal(formatEventDate('1990-05-17', 'en-GB'), '17/05/1990')
  assert.equal(formatEventDate('1990-05-17', 'en-US'), '05/17/1990')
})

test('a date without a year has no year in any language', () => {
  assert.equal(formatEventDate('--05-17', 'de'), '17.05.')
  assert.equal(formatEventDate('--05-17', 'en-US'), '05/17')
})

test('a date still being typed stays as it is', () => {
  assert.equal(formatEventDate('17.0', 'de'), '17.0')
})

test('a typed date becomes the stored one', () => {
  assert.equal(parseTypedDate('17.05.1990', 'de'), '1990-05-17')
  assert.equal(parseTypedDate('7.5.1990', 'de'), '1990-05-07')
  assert.equal(parseTypedDate('17.05.', 'de'), '--05-17')
  assert.equal(parseTypedDate('17.05', 'de'), '--05-17')
  assert.equal(parseTypedDate('05/17/1990', 'en-US'), '1990-05-17')
  assert.equal(parseTypedDate('17/05/1990', 'en-GB'), '1990-05-17')
})

test('the stored form is understood in any language', () => {
  assert.equal(parseTypedDate('1990-05-17', 'de'), '1990-05-17')
  assert.equal(parseTypedDate('1990-05-17', 'en-US'), '1990-05-17')
})

test('what is no date yet is not one', () => {
  for (const text of [
    '',
    '17',
    '17.0.19',
    '17.05.19',
    '32.05.1990',
    '17.13.1990',
    'abc',
  ]) {
    assert.equal(parseTypedDate(text, 'de'), null, text)
  }
})

test('typing and showing agree', () => {
  for (const locale of ['de', 'en-GB', 'en-US']) {
    for (const stored of ['1990-05-17', '--05-17']) {
      const shown = formatEventDate(stored, locale)
      assert.equal(parseTypedDate(shown, locale), stored, `${locale} ${shown}`)
    }
  }
})

test('the hint carries the letters of the language', () => {
  const letters = { day: 'TT', month: 'MM', year: 'JJJJ' }
  assert.deepEqual(datePattern('de', letters), {
    full: 'TT.MM.JJJJ',
    short: 'TT.MM.',
  })
})

test('the list shows an event date in the language, the clipboard text too', () => {
  const value = JSON.stringify({ date: '1990-05-17' })
  assert.equal(displayValue('event', value, 'de'), '17.05.1990')
  assert.equal(displayValue('event', value), '1990-05-17')
})
