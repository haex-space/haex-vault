// Part of `pnpm check:files` (spec 044, T023): sorting, hidden entries and the path bar.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  type SortableEntry,
  breadcrumbs,
  childPath,
  DEFAULT_SORT,
  formatSort,
  parentPath,
  parseSort,
  visibleEntries,
} from '../src/lib/files/state.ts'

type Entry = SortableEntry & { path: string }

function entry(name: string, partial: Partial<Entry> = {}): Entry {
  return {
    name,
    path: `/x/${name}`,
    kind: 'file',
    size: 0,
    modifiedMs: 0,
    hidden: name.startsWith('.'),
    ...partial,
  }
}

const names = (entries: Entry[]) => entries.map((e) => e.name)

test('folders come first, names sort naturally', () => {
  const entries = [
    entry('datei10.txt'),
    entry('Ordner', { kind: 'dir', size: null }),
    entry('datei2.txt'),
  ]
  assert.deepEqual(names(visibleEntries(entries, DEFAULT_SORT, false)), [
    'Ordner',
    'datei2.txt',
    'datei10.txt',
  ])
})

test('sorting by size descending keeps folders first', () => {
  const entries = [
    entry('klein', { size: 1 }),
    entry('gross', { size: 100 }),
    entry('dir', { kind: 'dir', size: null }),
  ]
  assert.deepEqual(
    names(visibleEntries(entries, { key: 'size', ascending: false }, false)),
    ['dir', 'gross', 'klein'],
  )
})

test('sorting by date and by type', () => {
  const entries = [
    entry('b.txt', { modifiedMs: 2 }),
    entry('a.pdf', { modifiedMs: 1 }),
  ]
  assert.deepEqual(
    names(visibleEntries(entries, { key: 'modified', ascending: true }, false)),
    ['a.pdf', 'b.txt'],
  )
  assert.deepEqual(
    names(visibleEntries(entries, { key: 'type', ascending: false }, false)),
    ['b.txt', 'a.pdf'],
  )
})

test('hidden entries show only when asked', () => {
  const entries = [entry('.bashrc'), entry('notiz.txt')]
  assert.deepEqual(names(visibleEntries(entries, DEFAULT_SORT, false)), [
    'notiz.txt',
  ])
  assert.equal(visibleEntries(entries, DEFAULT_SORT, true).length, 2)
})

test('the sort preference parses and falls back to name ascending', () => {
  assert.deepEqual(parseSort('size:desc'), { key: 'size', ascending: false })
  assert.equal(formatSort({ key: 'size', ascending: false }), 'size:desc')
  assert.deepEqual(parseSort('nonsense'), DEFAULT_SORT)
  assert.deepEqual(parseSort(null), DEFAULT_SORT)
})

test('the path bar on unix', () => {
  assert.deepEqual(breadcrumbs('/home/anna/Bilder'), [
    { name: '/', path: '/' },
    { name: 'home', path: '/home' },
    { name: 'anna', path: '/home/anna' },
    { name: 'Bilder', path: '/home/anna/Bilder' },
  ])
  assert.deepEqual(breadcrumbs('/'), [{ name: '/', path: '/' }])
})

test('the path bar on windows', () => {
  assert.deepEqual(breadcrumbs('C:\\Users\\Anna'), [
    { name: 'C:', path: 'C:\\' },
    { name: 'Users', path: 'C:\\Users' },
    { name: 'Anna', path: 'C:\\Users\\Anna' },
  ])
})

test('parent and child paths keep their style', () => {
  assert.equal(parentPath('/home/anna'), '/home')
  assert.equal(parentPath('/'), null)
  assert.equal(parentPath('C:\\Users'), 'C:\\')
  assert.equal(parentPath('C:\\'), null)
  assert.equal(childPath('/home', 'anna'), '/home/anna')
  assert.equal(childPath('/', 'tmp'), '/tmp')
  assert.equal(childPath('C:\\', 'Users'), 'C:\\Users')
})
