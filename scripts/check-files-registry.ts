// Part of `pnpm check:files` (spec 044, T019, T034): where a file browser tab stands survives as
// its tab location (session restore, spec 022).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  filesLocation,
  parseFilesPlace,
  type FilesPlace,
} from '../src/lib/files/registry.ts'

test('a device place round-trips through its tab location', () => {
  const place: FilesPlace = {
    source: { kind: 'device' },
    path: '/home/anna/Bilder',
    open: 'urlaub.jpg',
  }
  const location = filesLocation(place)
  assert.deepEqual(location, {
    path: '/device',
    query: { p: '/home/anna/Bilder', open: 'urlaub.jpg' },
  })
  assert.deepEqual(parseFilesPlace(location), place)
})

test('a storage place round-trips', () => {
  const place: FilesPlace = {
    source: { kind: 'storage', storageId: 'abc-123' },
    path: 'fotos/2026/',
  }
  assert.deepEqual(parseFilesPlace(filesLocation(place)), place)
})

test('another own device has its own route and round-trips (spec 056)', () => {
  const place: FilesPlace = {
    source: {
      kind: 'storage',
      storageId: 'peer:0b5c1f9e-2d41-4a77-9f0e-3c1a2b4d5e6f',
    },
    path: '/home/anna',
  }
  const location = filesLocation(place)
  assert.equal(location.path, '/peer/0b5c1f9e-2d41-4a77-9f0e-3c1a2b4d5e6f')
  assert.deepEqual(parseFilesPlace(location), place)
  assert.equal(parseFilesPlace({ path: '/peer/a:b', query: {} }), undefined)
  assert.equal(
    parseFilesPlace({ path: '/storage/peer:abc', query: {} }),
    undefined,
    'a stored storage never carries the prefix of a device',
  )
})

test('a share of a contact has its own route and round-trips (spec 056)', () => {
  const place: FilesPlace = {
    source: {
      kind: 'storage',
      storageId: 'share:7d1e4c2a-5b3f-4e8d-9a1c-2f6b8e0d4a7c',
    },
    path: '/Urlaub',
  }
  const location = filesLocation(place)
  assert.equal(location.path, '/share/7d1e4c2a-5b3f-4e8d-9a1c-2f6b8e0d4a7c')
  assert.deepEqual(parseFilesPlace(location), place)
})

test('the start is the device without a path', () => {
  assert.deepEqual(parseFilesPlace({ path: '/', query: {} }), {
    source: { kind: 'device' },
    path: null,
  })
})

test('unknown query keys and an open name with a slash are dropped', () => {
  assert.deepEqual(
    parseFilesPlace({
      path: '/device',
      query: { p: '/tmp', open: '../etc/passwd', extra: 'x' },
    }),
    { source: { kind: 'device' }, path: '/tmp' },
  )
})

test('an unknown location or a strange storage id is not a place', () => {
  assert.equal(parseFilesPlace({ path: '/elsewhere', query: {} }), undefined)
  assert.equal(parseFilesPlace({ path: '/storage/a b', query: {} }), undefined)
})

test('a search and its filter round-trip with the place', () => {
  const place = {
    source: { kind: 'device' as const },
    path: '/home/anna',
    q: 'urlaub',
    t: 'image,video',
    s: 'large',
    d: 'year',
  }
  assert.deepEqual(parseFilesPlace(filesLocation(place)), place)
})

test('the split keys of spec 062 are not part of a place', () => {
  // `split`, `side` and `o.*` belong to the frame (`lib/files/split.ts`): a place never carries them
  // and building a location from a place never emits them.
  const location = {
    path: '/device',
    query: {
      p: '/home/anna',
      split: '1',
      side: 'r',
      'o.src': 'storage:abc-123',
      'o.p': '/x',
      'o.q': 'urlaub',
    },
  }
  const place = parseFilesPlace(location)
  assert.deepEqual(place, { source: { kind: 'device' }, path: '/home/anna' })
  const built = filesLocation({ ...place!, q: 'x' })
  assert.deepEqual(built.query, { p: '/home/anna', q: 'x' })
})

test('a panel that shows the sync rules keeps that in its tab location (spec 061)', () => {
  const place: FilesPlace = {
    source: { kind: 'device' },
    path: '/home/anna',
    sr: '1',
  }
  const location = filesLocation(place)
  assert.deepEqual(location.query, { p: '/home/anna', sr: '1' })
  assert.deepEqual(parseFilesPlace(location), place)
})

test('the drive of a device place round-trips, only both keys together (spec 064 FR-025)', () => {
  const place: FilesPlace = {
    source: { kind: 'device' },
    path: '/run/media/anna/BACKUP/ordner',
    drive: 'a1b2-c3d4',
    droot: '/run/media/anna/BACKUP',
  }
  const location = filesLocation(place)
  assert.deepEqual(location.query, {
    p: '/run/media/anna/BACKUP/ordner',
    drive: 'a1b2-c3d4',
    droot: '/run/media/anna/BACKUP',
  })
  assert.deepEqual(parseFilesPlace(location), place)

  const only = (query: Record<string, string>) =>
    parseFilesPlace({ path: '/device', query })
  const bare = { source: { kind: 'device' }, path: '/run/media/anna/BACKUP/x' }
  assert.deepEqual(
    only({ p: '/run/media/anna/BACKUP/x', drive: 'a1b2' }),
    bare,
    'drive without droot',
  )
  assert.deepEqual(
    only({ p: '/run/media/anna/BACKUP/x', droot: '/run/media/anna/BACKUP' }),
    bare,
    'droot without drive',
  )
  assert.deepEqual(
    only({
      p: '/run/media/anna/BACKUP/x',
      drive: 'a1b2',
      droot: '/run/media/anna/OTHER',
    }),
    bare,
    'p not below droot',
  )
  assert.deepEqual(
    filesLocation({ ...place, droot: '/run/media/anna/OTHER' }).query,
    { p: '/run/media/anna/BACKUP/ordner' },
  )
})

test('a storage place never carries a drive', () => {
  const storage: FilesPlace = {
    source: { kind: 'storage', storageId: 'abc-123' },
    path: '/fotos',
    drive: 'a1b2',
    droot: '/',
  }
  assert.deepEqual(filesLocation(storage).query, { p: '/fotos' })
  assert.deepEqual(
    parseFilesPlace({
      path: '/storage/abc-123',
      query: { p: '/fotos', drive: 'a1b2', droot: '/' },
    }),
    { source: { kind: 'storage', storageId: 'abc-123' }, path: '/fotos' },
  )
})
