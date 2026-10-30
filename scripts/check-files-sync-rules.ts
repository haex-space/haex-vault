// Part of `pnpm check:files` (spec 061, T033): defaults of a new sync rule, the interval, the shown
// state, the hints of the dialog, the paths of the folder picker, the name of a place, the groups
// of a preview and the log depth.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  canSave,
  filterRules,
  childPath,
  defaultDeleteMode,
  intervalSecs,
  lastName,
  needsDecryptConsent,
  parseLogDepth,
  placeLabel,
  planGroups,
  progressShare,
  sortRules,
  type RuleRow,
  parentPath,
  shownState,
  showsCloudHint,
  targetSides,
} from '../src/lib/files/syncRules.ts'

test('a new rule mirrors both ways and keeps one way (FR-005)', () => {
  assert.equal(defaultDeleteMode('both'), 'mirror')
  assert.equal(defaultDeleteMode('aToB'), 'keep')
})

test('the interval stays between one minute and a day', () => {
  assert.equal(intervalSecs(15), 900)
  assert.equal(intervalSecs(0), 60)
  assert.equal(intervalSecs(5000), 86_400)
  assert.equal(intervalSecs(Number.NaN), 900)
  assert.equal(intervalSecs(2.4), 120)
})

test('a rule shows "inactive" while the service does not run, except when paused', () => {
  assert.equal(shownState('active', true), 'active')
  assert.equal(shownState('active', false), 'inactive')
  assert.equal(shownState('paused', false), 'paused')
})

test('the dialog asks for consent and points to encrypted folders', () => {
  assert.equal(needsDecryptConsent(true, false), true)
  assert.equal(needsDecryptConsent(true, true), false)
  assert.equal(needsDecryptConsent(false, false), false)
  assert.equal(showsCloudHint(true, false), true)
  assert.equal(showsCloudHint(true, true), false)
  assert.equal(showsCloudHint(false, false), false)
})

test('a two-way rule copies into both places (FR-011)', () => {
  assert.deepEqual(targetSides('aToB'), ['b'])
  assert.deepEqual(targetSides('both'), ['a', 'b'])
})

test('paths of the picker go up and down on every platform', () => {
  assert.equal(parentPath('/home/anna/Fotos'), '/home/anna')
  assert.equal(parentPath('/home'), '/')
  assert.equal(parentPath('/'), null)
  assert.equal(parentPath('/Fotos/2026/'), '/Fotos')
  assert.equal(parentPath('C:\\Users\\anna'), 'C:\\Users')
  assert.equal(parentPath('C:\\Users'), 'C:\\')
  assert.equal(parentPath('C:\\'), null)
  assert.equal(childPath('/', 'Fotos'), '/Fotos')
  assert.equal(childPath('/home', 'anna'), '/home/anna')
  assert.equal(childPath('C:\\Users', 'anna'), 'C:\\Users\\anna')
  assert.equal(lastName('/home/anna'), 'anna')
  assert.equal(lastName('/'), '/')
})

test('the dialog saves only a complete rule', () => {
  const full = {
    name: 'Fotos',
    hasA: true,
    hasB: true,
    needsConsent: false,
    consent: false,
  }
  assert.equal(canSave(full), true)
  assert.equal(canSave({ ...full, name: '  ' }), false)
  assert.equal(canSave({ ...full, name: 'x'.repeat(81) }), false)
  assert.equal(canSave({ ...full, hasB: false }), false)
  assert.equal(canSave({ ...full, needsConsent: true }), false)
  assert.equal(canSave({ ...full, needsConsent: true, consent: true }), true)
})

test('the progress of a run is the share of bytes, else of files, and unknown while scanning', () => {
  const copying = { phase: 'copying' as const, filesDone: 1, filesTotal: 4 }
  assert.equal(
    progressShare({ ...copying, bytesDone: 50, bytesTotal: 200 }),
    0.25,
  )
  assert.equal(progressShare({ ...copying, bytesDone: 0, bytesTotal: 0 }), 0.25)
  assert.equal(
    progressShare({ ...copying, filesTotal: 0, bytesDone: 0, bytesTotal: 0 }),
    null,
  )
  assert.equal(
    progressShare({
      ...copying,
      phase: 'scanning',
      bytesDone: 0,
      bytesTotal: 0,
    }),
    null,
  )
  assert.equal(
    progressShare({ ...copying, bytesDone: 300, bytesTotal: 200 }),
    0.99,
  )
  assert.equal(
    progressShare({
      ...copying,
      filesDone: 4,
      bytesDone: 300,
      bytesTotal: 200,
    }),
    1,
  )
})

const rows: RuleRow[] = [
  {
    name: 'Fotos 10',
    state: 'active',
    lastRunAt: '2026-10-27T10:00:00Z',
    places: '/home/anna/Bilder → Cloud /Fotos',
  },
  {
    name: 'Fotos 2',
    state: 'running',
    lastRunAt: null,
    places: '/home/anna/Kamera → Cloud /Kamera',
  },
  {
    name: 'Dokumente',
    state: 'error',
    lastRunAt: '2026-10-28T08:00:00Z',
    places: '/home/anna/Dokumente → NAS /Dokumente',
  },
]

test('rules sort by name with numbers in order, by state with the busy first, and by last run', () => {
  const names = (sorted: RuleRow[]) => sorted.map((row) => row.name)
  assert.deepEqual(names(sortRules(rows, { key: 'name', ascending: true })), [
    'Dokumente',
    'Fotos 2',
    'Fotos 10',
  ])
  assert.deepEqual(names(sortRules(rows, { key: 'status', ascending: true })), [
    'Fotos 2',
    'Dokumente',
    'Fotos 10',
  ])
  assert.deepEqual(
    names(sortRules(rows, { key: 'lastRun', ascending: false })),
    ['Dokumente', 'Fotos 10', 'Fotos 2'],
  )
})

test('the filter needs every word in the name or the places', () => {
  assert.equal(filterRules(rows, '').length, 3)
  assert.deepEqual(
    filterRules(rows, 'fotos cloud').map((row) => row.name),
    ['Fotos 10', 'Fotos 2'],
  )
  assert.deepEqual(
    filterRules(rows, 'NAS').map((row) => row.name),
    ['Dokumente'],
  )
  assert.equal(filterRules(rows, 'nichts').length, 0)
})

test('a preview lists its steps by action, in a fixed order, without empty groups', () => {
  const groups = planGroups([
    { action: 'copy', path: 'b' },
    { action: 'delete', path: 'c' },
    { action: 'copy', path: 'a' },
    { action: 'move', path: 'd' },
  ] as const)
  assert.deepEqual(
    groups.map((group) => [group.action, group.steps.map((s) => s.path)]),
    [
      ['move', ['d']],
      ['copy', ['b', 'a']],
      ['delete', ['c']],
    ],
  )
})

test('the log depth is a whole number from 0 to 100 (FR-044)', () => {
  assert.equal(parseLogDepth('0'), 0)
  assert.equal(parseLogDepth(' 25 '), 25)
  assert.equal(parseLogDepth('100'), 100)
  for (const bad of ['', '101', '-1', '2.5', 'zehn', '1000']) {
    assert.equal(parseLogDepth(bad), null, bad)
  }
})

test('a place shows the path on the device and the name of a storage or device', () => {
  const sources = {
    storages: [{ id: 's1', name: 'Cloud' }],
    devices: [
      { id: 'peer:1', name: 'Laptop' },
      { id: 'peer:2', name: null },
    ],
  }
  const storage = (storageId: string, path: string) => ({
    source: { kind: 'storage' as const, storageId },
    path,
  })
  assert.equal(
    placeLabel(
      { source: { kind: 'device' }, path: '/home/anna' },
      sources,
      '?',
    ),
    '/home/anna',
  )
  assert.equal(placeLabel(storage('s1', '/'), sources, '?'), 'Cloud')
  assert.equal(
    placeLabel(storage('s1', '/Fotos'), sources, '?'),
    'Cloud /Fotos',
  )
  assert.equal(placeLabel(storage('peer:1', '/a'), sources, '?'), 'Laptop /a')
  assert.equal(placeLabel(storage('peer:2', '/'), sources, '?'), '?')
  assert.equal(placeLabel(storage('gone', '/'), null, '?'), '?')
})
