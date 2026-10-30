// Part of `pnpm check:files` (spec 062): the split view of the file browser, as far as it is pure:
// the keys in the tab location (`contracts/split-location.md`), the changes of state and the
// minimum width.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  createHistory,
  currentLocation,
  MAX_HISTORY_ENTRIES,
  push,
} from '../src/lib/wm/navigation.ts'
import {
  type FilesPlace,
  filesLocation,
  parseFilesPlace,
} from '../src/lib/files/registry.ts'
import {
  disableSplit,
  enableSplit,
  fitsSplit,
  incomingHistory,
  readSplit,
  sourceFromToken,
  sourceToken,
  SPLIT_PANEL_MIN_WIDTH,
  splitAvailable,
  type SplitState,
  splitQuery,
  stripSplit,
  swapActive,
  withSplit,
} from '../src/lib/files/split.ts'
import { locationsEqual, type TabLocation } from '../src/lib/wm/navigation.ts'

const device = (path: string | null, rest: Partial<FilesPlace> = {}) =>
  ({ source: { kind: 'device' as const }, path, ...rest }) satisfies FilesPlace

const storage = (storageId: string, path: string | null) =>
  ({
    source: { kind: 'storage' as const, storageId },
    path,
  }) satisfies FilesPlace

const plain = (): TabLocation => ({
  path: '/device',
  query: { p: '/home/anna', q: 'urlaub', open: 'a.png' },
})

test('a location without split keys is a tab without split', () => {
  const state = readSplit(plain())
  assert.equal(state.on, false)
  assert.equal(state.side, 'left')
  assert.equal(state.other, null)
  assert.deepEqual(state.active, {
    source: { kind: 'device' },
    path: '/home/anna',
    q: 'urlaub',
    open: 'a.png',
  })
  assert.deepEqual(splitQuery(state), {})
})

test('a location that is not a file browser place reads as the device start', () => {
  const state = readSplit({ path: '/elsewhere', query: {} })
  assert.deepEqual(state.active, { source: { kind: 'device' }, path: null })
})

test('an on tab with the active panel on the right round-trips', () => {
  const state: SplitState = {
    on: true,
    side: 'right',
    active: storage('abc-123', '/fotos'),
    other: device('/home/anna', { q: 'x', t: 'image', s: 'large', d: 'year' }),
  }
  const location = withSplit(filesLocation(state.active), state)
  assert.deepEqual(location, {
    path: '/storage/abc-123',
    query: {
      p: '/fotos',
      split: '1',
      side: 'r',
      'o.src': 'device',
      'o.p': '/home/anna',
      'o.q': 'x',
      'o.t': 'image',
      'o.s': 'large',
      'o.d': 'year',
    },
  })
  assert.deepEqual(readSplit(location), state)
})

test('an on tab without o.* is valid, and the other place is none', () => {
  const state = readSplit({
    path: '/device',
    query: { p: '/home/anna', split: '1' },
  })
  assert.equal(state.on, true)
  assert.equal(state.other, null)
})

test('a place of the other panel without a path is the start of its source', () => {
  const state = readSplit({
    path: '/device',
    query: { split: '1', 'o.src': 'peer:phone-1' },
  })
  assert.deepEqual(state.other, {
    source: { kind: 'storage', storageId: 'peer:phone-1' },
    path: null,
  })
})

test('every kind of source has a token and the token reads back', () => {
  const sources = [
    { kind: 'device' as const },
    { kind: 'storage' as const, storageId: 'abc-123' },
    { kind: 'storage' as const, storageId: 'peer:phone-1' },
    { kind: 'storage' as const, storageId: 'share:s-9' },
  ]
  assert.deepEqual(sources.map(sourceToken), [
    'device',
    'storage:abc-123',
    'peer:phone-1',
    'share:s-9',
  ])
  for (const source of sources)
    assert.deepEqual(sourceFromToken(sourceToken(source)), source)
})

test('unknown values are dropped when reading, not repaired', () => {
  const read = (query: Record<string, string>) =>
    readSplit({ path: '/device', query })
  assert.equal(read({ split: '2' }).on, false)
  assert.equal(read({ split: 'true' }).on, false)
  assert.equal(read({ side: 'x' }).side, 'left')
  assert.equal(read({ side: 'l' }).side, 'left')
  for (const bad of [
    'storage:a/b',
    'nope',
    'storage:',
    'peer:',
    'device:x',
    '',
  ])
    assert.equal(read({ 'o.src': bad, 'o.p': '/x' }).other, null, bad)
  assert.equal(sourceFromToken(undefined), undefined)
  assert.equal(sourceFromToken(`storage:${'a'.repeat(65)}`), undefined)
})

test('the other panel never has a file open', () => {
  const state = readSplit({
    path: '/device',
    query: { split: '1', 'o.src': 'device', 'o.open': 'x.png', open: 'a.png' },
  })
  assert.equal(state.active.open, 'a.png')
  assert.equal(state.other && 'open' in state.other, false)
  assert.equal(
    'o.open' in splitQuery(swapActive({ ...state, on: true })),
    false,
  )
  const swapped = swapActive(state)
  assert.equal('open' in (swapped.other ?? {}), false)
})

test('withSplit leaves the plain keys alone and stripSplit takes the split keys away', () => {
  const state: SplitState = {
    on: true,
    side: 'right',
    active: device('/home/anna'),
    other: device('/tmp'),
  }
  const location = withSplit(plain(), state)
  for (const key of ['p', 'q', 'open'])
    assert.equal(location.query[key], plain().query[key])
  assert.deepEqual(stripSplit(location), plain())
  assert.ok(locationsEqual(stripSplit(withSplit(plain(), state)), plain()))
  // Writing the same state twice does not pile up keys.
  assert.deepEqual(withSplit(location, state), location)
  // Writing an off state removes `split` but keeps the remembered other place and the side.
  const off = withSplit(location, disableSplit(state))
  assert.equal(off.query.split, undefined)
  assert.equal(off.query.side, 'r')
  assert.equal(off.query['o.src'], 'device')
  assert.equal(off.query['o.p'], '/tmp')
})

test('enableSplit shows the remembered place, or the active one', () => {
  const fresh: SplitState = {
    on: false,
    side: 'left',
    active: storage('abc-123', '/fotos'),
    other: null,
  }
  assert.deepEqual(enableSplit(fresh).other, storage('abc-123', '/fotos'))
  assert.equal(enableSplit(fresh).on, true)
  // The copy of the active place carries neither the file open nor the search.
  const searching: SplitState = {
    ...fresh,
    active: { ...storage('abc-123', '/fotos'), open: 'a.png', q: 'x' },
  }
  assert.deepEqual(enableSplit(searching).other, storage('abc-123', '/fotos'))
  const remembered: SplitState = { ...fresh, other: device('/tmp') }
  assert.deepEqual(enableSplit(remembered).other, device('/tmp'))
  assert.equal(enableSplit(remembered).side, 'left')
})

test('disableSplit keeps the other place and the side', () => {
  const on: SplitState = {
    on: true,
    side: 'right',
    active: device('/a'),
    other: device('/b'),
  }
  const off = disableSplit(on)
  assert.equal(off.on, false)
  assert.equal(off.side, 'right')
  assert.deepEqual(off.active, on.active)
  assert.deepEqual(off.other, on.other)
})

test('swapActive exchanges the places and flips the side; twice is the identity', () => {
  const state: SplitState = {
    on: true,
    side: 'left',
    active: device('/a', { q: 'one' }),
    other: device('/b', { t: 'image' }),
  }
  const swapped = swapActive(state)
  assert.equal(swapped.side, 'right')
  assert.deepEqual(swapped.active, device('/b', { t: 'image' }))
  assert.deepEqual(swapped.other, device('/a', { q: 'one' }))
  assert.deepEqual(swapActive(swapped), state)
})

test('swapActive carries the search and the filter of the inactive panel along', () => {
  const state: SplitState = {
    on: true,
    side: 'left',
    active: device('/a'),
    other: device('/b', { q: 'x', t: 'image', s: 'large', d: 'year' }),
  }
  const { active } = swapActive(state)
  assert.deepEqual(
    [active.q, active.t, active.s, active.d],
    ['x', 'image', 'large', 'year'],
  )
})

test('each panel keeps its own view and sort across a swap and a round trip', () => {
  const state: SplitState = {
    on: true,
    side: 'left',
    active: device('/a', { vw: 'grid', so: 'size:desc' }),
    other: device('/b', { vw: 'list', so: 'name:asc' }),
  }
  assert.deepEqual(
    readSplit(withSplit(filesLocation(state.active), state)),
    state,
  )
  const swapped = swapActive(state)
  assert.deepEqual(
    [
      swapped.active.vw,
      swapped.active.so,
      swapped.other?.vw,
      swapped.other?.so,
    ],
    ['list', 'name:asc', 'grid', 'size:desc'],
  )
})

test('swapActive without another place changes nothing', () => {
  const state: SplitState = {
    on: true,
    side: 'left',
    active: device('/a'),
    other: null,
  }
  assert.equal(swapActive(state), state)
})

test('the place of the active panel is the plain place of the location after a swap', () => {
  const state: SplitState = {
    on: true,
    side: 'left',
    active: device('/a'),
    other: storage('abc-123', '/b'),
  }
  const next = swapActive(state)
  const location = withSplit(filesLocation(next.active), next)
  assert.equal(location.path, '/storage/abc-123')
  assert.deepEqual(parseFilesPlace(location), storage('abc-123', '/b'))
  assert.deepEqual(readSplit(location), next)
})

test('two panels need 720 px, one panel at least 360', () => {
  assert.equal(SPLIT_PANEL_MIN_WIDTH, 360)
  assert.equal(fitsSplit(719), false)
  assert.equal(fitsSplit(720), true)
  assert.equal(fitsSplit(0), false)
})

test('a panel without a lane comes with a history of one entry at its place', () => {
  const location = filesLocation(device('/b'))
  const history = incomingHistory(null, location)
  assert.equal(history.entries.length, 1)
  assert.equal(history.index, 0)
  assert.deepEqual(currentLocation(history), location)
})

test('a lane keeps its entries and only its current entry takes the new place', () => {
  const first = filesLocation(device('/x'))
  let lane = createHistory(first)
  lane = push(lane, filesLocation(device('/x/y')))
  lane = push(lane, filesLocation(device('/x/y/z')))
  const location = filesLocation(device('/x/y/z', { q: 'neu' }))
  const history = incomingHistory(lane, location)
  assert.equal(history.entries.length, 3)
  assert.equal(history.index, 2)
  assert.deepEqual(history.entries[0]?.location, first)
  assert.deepEqual(currentLocation(history), location)
  assert.ok(history.entries.length <= MAX_HISTORY_ENTRIES)
  assert.equal(
    currentLocation(lane).query.q,
    undefined,
    'the lane itself is not changed',
  )
})

test('phones do not offer two panels, everything else does', () => {
  assert.equal(splitAvailable('android'), false)
  assert.equal(splitAvailable('ios'), false)
  assert.equal(splitAvailable('linux'), true)
  assert.equal(splitAvailable('windows'), true)
  assert.equal(splitAvailable('macos'), true)
  assert.equal(splitAvailable(undefined), true)
})

test('the drive of the other panel survives, and comes back when it is the active one (spec 064)', () => {
  const state: SplitState = {
    on: true,
    side: 'left',
    active: storage('abc-123', '/fotos'),
    other: device('/run/media/anna/BACKUP/x', {
      drive: 'a1b2',
      droot: '/run/media/anna/BACKUP',
    }),
  }
  const location = withSplit(filesLocation(state.active), state)
  assert.equal(location.query['o.drive'], 'a1b2')
  assert.equal(location.query['o.droot'], '/run/media/anna/BACKUP')
  assert.deepEqual(readSplit(location), state)
  const swapped = swapActive(state)
  assert.deepEqual(filesLocation(swapped.active).query, {
    p: '/run/media/anna/BACKUP/x',
    drive: 'a1b2',
    droot: '/run/media/anna/BACKUP',
  })
})
