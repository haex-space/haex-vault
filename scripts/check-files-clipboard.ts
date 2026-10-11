// Part of `pnpm check:files` (spec 044, T049, T050): selection, holzi's clipboard, paste and drop
// rules, and the context menus.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  clickEntry,
  dragPayload,
  dropOp,
  EMPTY_SELECTION,
  isWithin,
  menuSelection,
  parseDragPayload,
  pasteRefusal,
  pruneSelection,
  type Selection,
} from '../src/lib/files/clipboard.ts'
import { entriesMenu, folderMenu } from '../src/lib/files/menus.ts'

const ORDER = ['/x/a', '/x/b', '/x/c', '/x/d']
const NO_KEYS = { toggle: false, range: false }

test('a plain click without a selection opens the entry', () => {
  const result = clickEntry(EMPTY_SELECTION, '/x/a', ORDER, NO_KEYS)
  assert.equal(result.activate, true)
  assert.deepEqual(result.selection, EMPTY_SELECTION)
})

test('toggle adds and removes; with a selection a plain click toggles too', () => {
  let selection: Selection = EMPTY_SELECTION
  ;({ selection } = clickEntry(selection, '/x/a', ORDER, {
    toggle: true,
    range: false,
  }))
  assert.deepEqual(selection.paths, ['/x/a'])
  const plain = clickEntry(selection, '/x/c', ORDER, NO_KEYS)
  assert.equal(plain.activate, false)
  assert.deepEqual(plain.selection.paths, ['/x/a', '/x/c'])
  const off = clickEntry(plain.selection, '/x/a', ORDER, NO_KEYS)
  assert.deepEqual(off.selection.paths, ['/x/c'])
  const none = clickEntry(off.selection, '/x/c', ORDER, NO_KEYS)
  assert.deepEqual(none.selection, EMPTY_SELECTION)
})

test('a range runs from the anchor in folder order, both ways', () => {
  const start: Selection = { paths: ['/x/b'], anchor: '/x/b' }
  const down = clickEntry(start, '/x/d', ORDER, { toggle: false, range: true })
  assert.deepEqual(down.selection.paths, ['/x/b', '/x/c', '/x/d'])
  const up = clickEntry(start, '/x/a', ORDER, { toggle: false, range: true })
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
