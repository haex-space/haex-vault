// Part of `pnpm check:files` (spec 071): the pinned folders of the sidebar, as far as they are pure.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  addPins,
  addPlaces,
  type FilesPin,
  formatPins,
  isPinned,
  offerPin,
  parseHidden,
  parsePins,
  pinLabel,
  pinnableSource,
  removePin,
  removePlace,
} from '../src/lib/files/pins.ts'

const docs: FilesPin = { source: { kind: 'device' }, path: '/home/anna/docs' }
const cloud: FilesPin = {
  source: { kind: 'storage', storageId: 'abc-123' },
  path: '/fotos',
}

const peer: FilesPin = {
  source: { kind: 'storage', storageId: 'peer:abc' },
  path: '/home/ben/x',
}

test('every source of the sidebar may be pinned, a device: storage not (FR-002)', () => {
  assert.equal(pinnableSource({ kind: 'device' }), true)
  assert.equal(pinnableSource({ kind: 'storage', storageId: 'abc-123' }), true)
  assert.equal(
    pinnableSource({ kind: 'storage', storageId: 'peer:abc-123' }),
    true,
  )
  assert.equal(
    pinnableSource({ kind: 'storage', storageId: 'share:abc-123' }),
    true,
  )
  assert.equal(pinnableSource({ kind: 'storage', storageId: '' }), false)
  assert.equal(
    pinnableSource({ kind: 'storage', storageId: 'device:abc' }),
    false,
  )
})

test('pins round-trip through their stored form, in order', () => {
  const pins = [docs, cloud, peer]
  assert.deepEqual(parsePins(formatPins(pins)), pins)
})

test('what is stored is not trusted: junk, unknown sources and empty paths are dropped', () => {
  assert.deepEqual(parsePins(null), [])
  assert.deepEqual(parsePins('not json'), [])
  assert.deepEqual(parsePins('{"s":"device","p":"/x"}'), [])
  assert.deepEqual(
    parsePins(
      JSON.stringify([
        { s: 'device', p: '/ok' },
        { s: 'device:C:', p: '/x' },
        { s: 'bogus', p: '/x' },
        { s: 'device', p: '' },
        { s: 'device', p: 7 },
        null,
        'text',
      ]),
    ),
    [{ source: { kind: 'device' }, path: '/ok' }],
  )
})

test('adding keeps the order, skips duplicates and what cannot be pinned', () => {
  const encrypted: FilesPin = {
    source: { kind: 'storage', storageId: 'device:C:' },
    path: '/x',
  }
  assert.deepEqual(addPins([docs], [cloud, docs, encrypted, peer]), [
    docs,
    cloud,
    peer,
  ])
  assert.deepEqual(addPins([], [docs, docs]), [docs])
})

test('the same path on another source is another pin', () => {
  const onCloud: FilesPin = { ...cloud, path: docs.path }
  assert.equal(isPinned([docs], onCloud), false)
  assert.deepEqual(removePin([docs, onCloud], docs), [onCloud])
})

test('a pin is named after its folder, a root after its storage', () => {
  assert.equal(pinLabel(docs, 'Cloud'), 'docs')
  assert.equal(pinLabel({ ...cloud, path: '/' }, 'Cloud'), 'Cloud')
  assert.equal(
    pinLabel({ source: { kind: 'device' }, path: 'D:\\Daten\\Fotos\\' }, 'x'),
    'Fotos',
  )
  assert.equal(
    pinLabel({ source: { kind: 'device' }, path: 'D:\\' }, 'x'),
    'D:',
  )
})

const none = { pins: [], hidden: [] }
const home = '/home/anna'

test('the menu offers to add, to remove, or nothing (FR-003)', () => {
  assert.equal(offerPin(none, docs, [home]), 'pin')
  assert.equal(offerPin({ pins: [docs], hidden: [] }, docs, [home]), 'unpin')
  assert.equal(offerPin(none, peer, []), 'pin')
  assert.equal(
    offerPin(
      none,
      { source: { kind: 'storage', storageId: 'device:C:' }, path: '/x' },
      [],
    ),
    undefined,
  )
  // A known place is removed while shown and added back while removed.
  const homePin: FilesPin = { ...docs, path: home }
  assert.equal(offerPin(none, homePin, [home]), 'unpin')
  assert.equal(offerPin({ pins: [], hidden: [home] }, homePin, [home]), 'pin')
  // A known place of this device says nothing about the same path in a storage.
  assert.equal(offerPin(none, { ...cloud, path: home }, [home]), 'pin')
})

test('a known place is hidden when removed and comes back when added (FR-004)', () => {
  const homePin: FilesPin = { ...docs, path: home }
  const removed = removePlace(none, homePin, [home])
  assert.deepEqual(removed, { pins: [], hidden: [home] })
  assert.equal(removePlace(removed, homePin, [home]), removed)
  assert.deepEqual(addPlaces(removed, [homePin, docs], [home]), {
    pins: [docs],
    hidden: [],
  })
  assert.deepEqual(
    removePlace({ pins: [docs, cloud], hidden: [] }, docs, [home]),
    { pins: [cloud], hidden: [] },
  )
})

test('the removed known places stored are not trusted', () => {
  assert.deepEqual(parseHidden(null), [])
  assert.deepEqual(parseHidden('not json'), [])
  assert.deepEqual(parseHidden(JSON.stringify([home, '', 7, null])), [home])
})
