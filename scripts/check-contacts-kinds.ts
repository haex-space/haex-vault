// Part of `pnpm check:contacts` (spec 051, FR-003, FR-006, FR-007): how the window shapes and shows
// contact values (src/lib/contacts/kinds.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  customLabel,
  displayValue,
  kindOf,
  makeCustomLabel,
  openTargets,
  parseAddress,
  serializeAddress,
  serializeEventDate,
  serializeIm,
} from '../src/lib/contacts/kinds.ts'

test('only the phone kinds leave holzi', () => {
  for (const kind of [
    'phone',
    'email',
    'address',
    'website',
    'im',
    'sip',
    'event',
    'relation',
  ]) {
    assert.equal(kindOf(kind)?.holziOnly, false, kind)
  }
  for (const kind of [
    'custom',
    'holzi_vault',
    'holzi_nostr_relay',
    'holzi_iroh',
  ]) {
    assert.equal(kindOf(kind)?.holziOnly, true, kind)
  }
  assert.equal(kindOf('future_kind'), undefined)
})

test('labels of the user carry a prefix', () => {
  assert.equal(makeCustomLabel(' Ferienhaus '), 'custom:Ferienhaus')
  assert.equal(customLabel('custom:Ferienhaus'), 'Ferienhaus')
  assert.equal(customLabel('home'), null)
})

test('addresses keep only filled parts and read as one line', () => {
  const stored = serializeAddress({
    street: ' Hauptstr. 1 ',
    city: 'Berlin',
    postcode: '10115',
    region: '',
  })
  assert.deepEqual(JSON.parse(stored), {
    street: 'Hauptstr. 1',
    postcode: '10115',
    city: 'Berlin',
  })
  assert.deepEqual(parseAddress(stored), {
    street: 'Hauptstr. 1',
    postcode: '10115',
    city: 'Berlin',
  })
  assert.equal(displayValue('address', stored), 'Hauptstr. 1, 10115 Berlin')
  assert.deepEqual(parseAddress('not json'), {})
})

test('messengers and dates read as text, unknown kinds as they are', () => {
  assert.equal(
    displayValue('im', serializeIm('Signal', 'anna.01')),
    'anna.01 (Signal)',
  )
  assert.equal(displayValue('event', serializeEventDate('--05-17')), '05-17')
  assert.equal(
    displayValue('event', serializeEventDate('1990-05-17')),
    '1990-05-17',
  )
  assert.equal(displayValue('future_kind', 'raw'), 'raw')
})

test('values open with the right app', () => {
  assert.deepEqual(
    openTargets('phone', '0170 123-45 67').map((target) => target.url),
    ['tel:01701234567', 'sms:01701234567'],
  )
  assert.deepEqual(openTargets('email', 'a@b.de'), [
    { action: 'mail', url: 'mailto:a@b.de' },
  ])
  assert.equal(
    openTargets('website', 'example.org')[0]?.url,
    'https://example.org',
  )
  assert.equal(
    openTargets('address', serializeAddress({ city: 'Berlin' }))[0]?.url,
    'geo:0,0?q=Berlin',
  )
  assert.deepEqual(openTargets('custom', 'x'), [])
})
