// Part of `pnpm check:files` (spec 044, T049, T050): selection, holzi's clipboard, paste and drop
// rules, and the context menus.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  isLocked,
  bandBetween,
  bandSelection,
  clickEntry,
  dragPayload,
  dropOp,
  EMPTY_SELECTION,
  indexesInBand,
  isWithin,
  menuSelection,
  parseDragPayload,
  pasteRefusal,
  pruneSelection,
  sameSource,
  type Selection,
} from '../src/lib/files/clipboard.ts'
import { entriesMenu, folderMenu, pinMenu } from '../src/lib/files/menus.ts'

const ORDER = ['/x/a', '/x/b', '/x/c', '/x/d']
const CLICK = { toggle: false, range: false, touch: false }
const TAP = { toggle: false, range: false, touch: true }

test('a plain click selects the entry alone and opens nothing', () => {
  const first = clickEntry(EMPTY_SELECTION, '/x/a', ORDER, CLICK)
  assert.equal(first.activate, false)
  assert.deepEqual(first.selection, { paths: ['/x/a'], anchor: '/x/a' })
  const many: Selection = { paths: ['/x/a', '/x/b'], anchor: '/x/a' }
  const other = clickEntry(many, '/x/c', ORDER, CLICK)
  assert.equal(other.activate, false)
  assert.deepEqual(other.selection, { paths: ['/x/c'], anchor: '/x/c' })
})

test('a tap without a selection opens the entry', () => {
  const result = clickEntry(EMPTY_SELECTION, '/x/a', ORDER, TAP)
  assert.equal(result.activate, true)
  assert.deepEqual(result.selection, EMPTY_SELECTION)
})

test('toggle adds and removes; with a selection a tap toggles too', () => {
  let selection: Selection = EMPTY_SELECTION
  ;({ selection } = clickEntry(selection, '/x/a', ORDER, {
    ...CLICK,
    toggle: true,
  }))
  assert.deepEqual(selection.paths, ['/x/a'])
  const plain = clickEntry(selection, '/x/c', ORDER, TAP)
  assert.equal(plain.activate, false)
  assert.deepEqual(plain.selection.paths, ['/x/a', '/x/c'])
  const off = clickEntry(plain.selection, '/x/a', ORDER, TAP)
  assert.deepEqual(off.selection.paths, ['/x/c'])
  const none = clickEntry(off.selection, '/x/c', ORDER, TAP)
  assert.deepEqual(none.selection, EMPTY_SELECTION)
})

test('a range runs from the anchor in folder order, both ways', () => {
  const start: Selection = { paths: ['/x/b'], anchor: '/x/b' }
  const down = clickEntry(start, '/x/d', ORDER, { ...CLICK, range: true })
  assert.deepEqual(down.selection.paths, ['/x/b', '/x/c', '/x/d'])
  const up = clickEntry(start, '/x/a', ORDER, { ...CLICK, range: true })
  assert.deepEqual(up.selection.paths, ['/x/a', '/x/b'])
})

test('a menu acts on the selection when it holds the entry, else on the entry', () => {
  const selection: Selection = { paths: ['/x/a', '/x/b'], anchor: '/x/a' }
  assert.equal(menuSelection(selection, '/x/b'), selection)
  assert.deepEqual(menuSelection(selection, '/x/c').paths, ['/x/c'])
})

test('entries that left the folder leave the selection', () => {
  const selection: Selection = { paths: ['/x/a', '/x/b'], anchor: '/x/b' }
  assert.deepEqual(pruneSelection(selection, ['/x/a']), {
    paths: ['/x/a'],
    anchor: null,
  })
  assert.equal(pruneSelection(selection, ORDER), selection)
})

test('within means the folder itself or below, not a sibling with the same start', () => {
  assert.ok(isWithin('/x/a', '/x/a'))
  assert.ok(isWithin('/x/a/b', '/x/a'))
  assert.ok(!isWithin('/x/ab', '/x/a'))
  assert.ok(isWithin('/anything', '/'))
  assert.ok(isWithin('C:\\Daten\\Fotos', 'C:\\Daten'))
  assert.ok(!isWithin('C:\\DatenAlt', 'C:\\Daten'))
})

test('paste rules across sources: the paths of two sources are not compared', () => {
  const dir = [{ path: '/backup', kind: 'dir' as const }]
  const into = (path: string) => ({ path, holziOwned: false })
  // Storage A `/backup` into storage B `/backup/2026` is not "into itself".
  assert.equal(pasteRefusal('cut', '/', dir, into('/backup/2026'), false), null)
  assert.equal(
    pasteRefusal('copy', '/', dir, into('/backup/2026'), false),
    null,
  )
  // A cut from the root of one storage to the root of another is not "nothing".
  assert.equal(pasteRefusal('cut', '/', dir, into('/'), false), null)
  // The same source keeps both rules.
  assert.equal(
    pasteRefusal('cut', '/', dir, into('/backup/2026'), true),
    'intoItself',
  )
  assert.equal(
    pasteRefusal(
      'cut',
      '/x',
      [{ path: '/x/f', kind: 'file' }],
      into('/x'),
      true,
    ),
    'nothing',
  )
  // Holzi's own places refuse whatever the source.
  assert.equal(
    pasteRefusal('copy', '/', dir, { path: '/own', holziOwned: true }, false),
    'holziOwned',
  )
  // Leaving the argument out is the same source, as before.
  assert.equal(
    pasteRefusal('cut', '/x', [{ path: '/x/f', kind: 'file' }], into('/x')),
    'nothing',
  )
})

test("a read only target (a share) refuses with its own reason, holzi's own place first", () => {
  const file = [{ path: '/x/f', kind: 'file' as const }]
  const into = { path: '/share', holziOwned: false, readOnly: true }
  assert.equal(pasteRefusal('copy', '/x', file, into, false), 'readOnly')
  assert.equal(
    pasteRefusal('copy', '/x', file, { ...into, holziOwned: true }, false),
    'holziOwned',
  )
  assert.ok(isLocked({ holziOwned: false, readOnly: true }))
  assert.ok(isLocked({ holziOwned: true, readOnly: false }))
  assert.ok(!isLocked({ holziOwned: false, readOnly: false }))
  assert.ok(!isLocked(null))
})

test('paste rules: own places, into itself, cut into the same folder', () => {
  const dir = [{ path: '/x/a', kind: 'dir' as const }]
  const file = [{ path: '/x/f', kind: 'file' as const }]
  assert.equal(
    pasteRefusal('copy', '/x', file, { path: '/own', holziOwned: true }),
    'holziOwned',
  )
  assert.equal(
    pasteRefusal('copy', '/x', dir, { path: '/x/a/b', holziOwned: false }),
    'intoItself',
  )
  assert.equal(
    pasteRefusal('cut', '/x', dir, { path: '/x/a', holziOwned: false }),
    'intoItself',
  )
  assert.equal(
    pasteRefusal('cut', '/x', file, { path: '/x', holziOwned: false }),
    'nothing',
  )
  assert.equal(
    pasteRefusal('copy', '/x', file, { path: '/x', holziOwned: false }),
    null,
    'a copy into the same folder keeps both',
  )
  assert.equal(
    pasteRefusal('cut', '/x', dir, { path: '/y', holziOwned: false }),
    null,
  )
})

test('a drop moves within a source and copies with the copy key or across sources', () => {
  const keys = { ctrl: false, alt: false, mac: false }
  assert.equal(dropOp(true, keys), 'cut')
  assert.equal(dropOp(true, { ...keys, ctrl: true }), 'copy')
  assert.equal(dropOp(true, { ...keys, mac: true, ctrl: true }), 'cut')
  assert.equal(dropOp(true, { ...keys, mac: true, alt: true }), 'copy')
  assert.equal(dropOp(false, keys), 'copy')
})

test('a drag payload survives the trip and foreign text is refused', () => {
  const payload = {
    source: { kind: 'device' },
    folder: '/x',
    items: [{ path: '/x/a', kind: 'dir' as const }],
  }
  assert.deepEqual(parseDragPayload(dragPayload(payload)), payload)
  for (const text of [
    '',
    null,
    'nope',
    '{}',
    '{"source":{"kind":"device"},"folder":"/x","items":[]}',
    '{"source":{"kind":"device"},"folder":"/x","items":[{"path":"/x/a","kind":"link"}]}',
  ])
    assert.equal(parseDragPayload(text), null, String(text))
})

const ids = (entries: ReturnType<typeof entriesMenu>) =>
  entries.flatMap((entry) =>
    'separator' in entry ? [] : [`${entry.id}${entry.disabled ? '-' : ''}`],
  )

test('the menu of one file opens, selects, copies, renames and deletes', () => {
  assert.deepEqual(
    ids(
      entriesMenu({
        count: 1,
        singleFile: true,
        readOnly: false,
        selecting: false,
      }),
    ),
    ['open', 'openSystem', 'select', 'copy', 'cut', 'rename', 'delete'],
  )
})

test('without system apps (Android) a file cannot open with one', () => {
  assert.deepEqual(
    ids(
      entriesMenu({
        count: 1,
        singleFile: true,
        readOnly: false,
        selecting: false,
        systemApps: false,
      }),
    ),
    ['open', 'select', 'copy', 'cut', 'rename', 'delete'],
  )
})

test('several entries cannot be renamed; in own places only copying is left', () => {
  assert.deepEqual(
    ids(
      entriesMenu({
        count: 3,
        singleFile: false,
        readOnly: false,
        selecting: true,
      }),
    ),
    ['copy', 'cut', 'rename-', 'delete'],
  )
  assert.deepEqual(
    ids(
      entriesMenu({
        count: 1,
        singleFile: false,
        readOnly: true,
        selecting: false,
      }),
    ),
    ['open', 'select', 'copy', 'cut-', 'rename-', 'delete-'],
  )
})

test('a pin command shows for one folder only, and for the open folder (spec 071 FR-003)', () => {
  const base = { readOnly: false, selecting: false }
  assert.deepEqual(
    ids(entriesMenu({ ...base, count: 1, singleFile: false, pin: 'pin' })),
    ['open', 'pin', 'select', 'copy', 'cut', 'rename', 'delete'],
  )
  assert.deepEqual(
    ids(entriesMenu({ ...base, count: 1, singleFile: false, pin: 'unpin' })),
    ['open', 'unpin', 'select', 'copy', 'cut', 'rename', 'delete'],
  )
  // Several entries: no pin, even if the caller offered one.
  assert.deepEqual(
    ids(entriesMenu({ ...base, count: 2, singleFile: false, pin: 'pin' })),
    ['select', 'copy', 'cut', 'rename-', 'delete'],
  )
  assert.deepEqual(
    ids(folderMenu({ readOnly: false, canPaste: true, pin: 'pin' })),
    ['newFolder', 'paste', 'pin'],
  )
  assert.deepEqual(ids(pinMenu()), ['unpin'])
})

test('the folder menu creates and pastes where it may', () => {
  assert.deepEqual(ids(folderMenu({ readOnly: false, canPaste: true })), [
    'newFolder',
    'paste',
  ])
  assert.deepEqual(ids(folderMenu({ readOnly: false, canPaste: false })), [
    'newFolder',
    'paste-',
  ])
  assert.deepEqual(ids(folderMenu({ readOnly: true, canPaste: true })), [
    'newFolder-',
    'paste-',
  ])
})

test('one entry of this device can be shared with contacts (spec 056)', () => {
  assert.deepEqual(
    ids(
      entriesMenu({
        count: 1,
        singleFile: false,
        readOnly: false,
        selecting: false,
        shareable: true,
      }),
    ),
    ['open', 'share', 'select', 'copy', 'cut', 'rename', 'delete'],
  )
  assert.ok(
    !ids(
      entriesMenu({
        count: 2,
        singleFile: false,
        readOnly: false,
        selecting: true,
        shareable: true,
      }),
    ).includes('share'),
    'one entry at a time',
  )
})

const LIST = { columns: 1, cellWidth: Infinity, cellHeight: 40 }
const GRID = { columns: 3, cellWidth: 100, cellHeight: 100 }

test('a band is the same from every corner', () => {
  const box = { left: 10, top: 20, right: 50, bottom: 80 }
  assert.deepEqual(bandBetween({ x: 10, y: 20 }, { x: 50, y: 80 }), box)
  assert.deepEqual(bandBetween({ x: 50, y: 80 }, { x: 10, y: 20 }), box)
  assert.deepEqual(bandBetween({ x: 50, y: 20 }, { x: 10, y: 80 }), box)
})

test('a band over the list touches the rows it crosses', () => {
  // Rows are 40 high: a band from y 30 to y 90 crosses rows 0, 1 and 2.
  const band = { left: 5, top: 30, right: 20, bottom: 90 }
  assert.deepEqual(indexesInBand(band, LIST, 10), [0, 1, 2])
  const inside = { left: 5, top: 45, right: 20, bottom: 50 }
  assert.deepEqual(indexesInBand(inside, LIST, 10), [1])
})

test('a band over the grid touches the cells in its rectangle', () => {
  // Cells are 100 x 100, three per row: x 50-150 spans columns 0-1, y 50-150 rows 0-1.
  const band = { left: 50, top: 50, right: 150, bottom: 150 }
  assert.deepEqual(indexesInBand(band, GRID, 9), [0, 1, 3, 4])
})

test('a band stops at the last entry and past the columns', () => {
  const tall = { left: 0, top: 0, right: 300, bottom: 1000 }
  assert.deepEqual(indexesInBand(tall, GRID, 4), [0, 1, 2, 3])
  const beside = { left: 320, top: 0, right: 400, bottom: 100 }
  assert.deepEqual(indexesInBand(beside, GRID, 9), [])
  const below = { left: 0, top: 500, right: 50, bottom: 600 }
  assert.deepEqual(indexesInBand(below, LIST, 3), [])
  assert.deepEqual(indexesInBand(tall, GRID, 0), [])
})

test('a band replaces the selection, with a key it adds to it', () => {
  const base: Selection = { paths: ['/x/a'], anchor: '/x/a' }
  assert.deepEqual(bandSelection(base, ['/x/c', '/x/d'], false), {
    paths: ['/x/c', '/x/d'],
    anchor: '/x/c',
  })
  assert.deepEqual(bandSelection(base, ['/x/a', '/x/c'], true), {
    paths: ['/x/a', '/x/c'],
    anchor: '/x/a',
  })
  assert.deepEqual(bandSelection(base, [], false), EMPTY_SELECTION)
})

test('the split view adds copying and moving into the other panel, after copy and cut', () => {
  const input = {
    count: 2,
    singleFile: false,
    readOnly: false,
    selecting: true,
  }
  assert.deepEqual(ids(entriesMenu({ ...input, split: true })), [
    'copy',
    'cut',
    'copyToOther',
    'moveToOther',
    'rename-',
    'delete',
  ])
  // Without the split view the menu is the one it was.
  assert.deepEqual(ids(entriesMenu(input)), [
    'copy',
    'cut',
    'rename-',
    'delete',
  ])
  assert.deepEqual(entriesMenu({ ...input, split: false }), entriesMenu(input))
})

test('the two commands name F5 and F6 and follow the write rules', () => {
  const input = {
    count: 1,
    singleFile: true,
    readOnly: false,
    selecting: false,
    split: true,
  }
  const entries = entriesMenu(input)
  const copyTo = entries.find(
    (entry) => 'id' in entry && entry.id === 'copyToOther',
  )
  const moveTo = entries.find(
    (entry) => 'id' in entry && entry.id === 'moveToOther',
  )
  assert.deepEqual(copyTo && 'shortcut' in copyTo && copyTo.shortcut, {
    key: 'F5',
  })
  assert.deepEqual(moveTo && 'shortcut' in moveTo && moveTo.shortcut, {
    key: 'F6',
  })
  assert.equal(ids(entries).filter((id) => id.endsWith('ToOther-')).length, 0)
  // The other panel is read only: nothing goes there.
  assert.deepEqual(
    ids(entriesMenu({ ...input, readOnlyTarget: true })).filter((id) =>
      id.includes('ToOther'),
    ),
    ['copyToOther-', 'moveToOther-'],
  )
  // The open place is read only (holzi's own): it may be copied from, not moved from.
  assert.deepEqual(
    ids(entriesMenu({ ...input, readOnly: true })).filter((id) =>
      id.includes('ToOther'),
    ),
    ['copyToOther', 'moveToOther-'],
  )
  // Nothing selected: neither.
  assert.deepEqual(
    ids(entriesMenu({ ...input, count: 0 })).filter((id) =>
      id.includes('ToOther'),
    ),
    ['copyToOther-', 'moveToOther-'],
  )
})

test('two source references are the same source when they name the same place', () => {
  assert.equal(sameSource({ kind: 'device' }, { kind: 'device' }), true)
  assert.equal(
    sameSource(
      { kind: 'storage', storageId: 'abc' },
      { kind: 'storage', storageId: 'abc' },
    ),
    true,
  )
  assert.equal(
    sameSource(
      { kind: 'storage', storageId: 'abc' },
      { kind: 'storage', storageId: 'abd' },
    ),
    false,
  )
  assert.equal(
    sameSource({ kind: 'device' }, { kind: 'storage', storageId: 'abc' }),
    false,
  )
})
