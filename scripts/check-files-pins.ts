// Part of `pnpm check:files` (spec 071): the pinned folders of the sidebar, as far as they are pure.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  addPins,
  type FilesPin,
  formatPins,
  isPinned,
  offerPin,
  parsePins,
  pinLabel,
  pinnableSource,
  removePin,
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

test('the menu offers to pin, to unpin, or nothing (FR-003)', () => {
  const home = '/home/anna'
  assert.equal(offerPin([], docs, [home]), 'pin')
  assert.equal(offerPin([docs], docs, [home]), 'unpin')
  assert.equal(offerPin([], { ...docs, path: home }, [home]), undefined)
  assert.equal(offerPin([], peer, []), 'pin')
  assert.equal(
    offerPin(
      [],
      { source: { kind: 'storage', storageId: 'device:C:' }, path: '/x' },
      [],
    ),
    undefined,
  )
  // A known place of this device says nothing about the same path in a storage.
  assert.equal(offerPin([], { ...cloud, path: home }, [home]), 'pin')
})
