// Part of `pnpm check:passwords` (spec 036, FR-018, FR-019): the context menus of the password
// manager (src/lib/passwords/menus.ts). The right-click menu, the row's menu button and the
// selection bar show the same actions.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  buildMenu,
  type MenuEntry,
  type MenuInput,
} from '../src/lib/passwords/menus.ts'

const ids = (entries: readonly MenuEntry[]) =>
  entries.map((entry) => ('separator' in entry ? '|' : entry.id))

const menu = (input: MenuInput) => ids(buildMenu(input))

test('an entry offers open, copy username and password, cut and delete', () => {
  assert.deepEqual(
    menu({ kind: 'entry', hasUsername: true, hasPassword: true }),
    ['open', '|', 'copyUsername', 'copyPassword', '|', 'cut', '|', 'delete'],
  )
})

test('copy password is greyed out for an entry without one', () => {
  const entries = buildMenu({ kind: 'entry', hasUsername: true })
  const password = entries.find(
    (entry) => !('separator' in entry) && entry.id === 'copyPassword',
  )
  const username = entries.find(
    (entry) => !('separator' in entry) && entry.id === 'copyUsername',
  )
  assert.ok(password && !('separator' in password))
  assert.equal(password.disabled, true)
  assert.ok(username && !('separator' in username))
  assert.equal(username.disabled, false)
})

test('Kopieren shows once the copy dialog is there (stage 3)', () => {
  assert.deepEqual(menu({ kind: 'entry', copyAvailable: true }), [
    'open',
    '|',
    'copyUsername',
    'copyPassword',
    '|',
    'cut',
    'copy',
    '|',
    'delete',
  ])
})

test('a menu for a selection of several rows applies to all of them', () => {
  assert.deepEqual(menu({ kind: 'entry', selectionSize: 3 }), [
    'cut',
    '|',
    'delete',
  ])
  assert.deepEqual(menu({ kind: 'folder', selectionSize: 2 }), [
    'cut',
    '|',
    'delete',
  ])
})

test('in the trash an entry and a folder offer restore and delete for good only', () => {
  const expected = ['restore', 'deleteForGood']
  assert.deepEqual(
    menu({ kind: 'entry', inTrash: true, ablageFilled: true }),
    expected,
  )
  assert.deepEqual(
    menu({ kind: 'folder', inTrash: true, ablageFilled: true }),
    expected,
  )
})

test('a folder offers paste into it only with a filled Ablage', () => {
  assert.deepEqual(menu({ kind: 'folder' }), [
    'open',
    'edit',
    'newSubfolder',
    '|',
    'cut',
    '|',
    'delete',
  ])
  assert.deepEqual(menu({ kind: 'folder', ablageFilled: true }), [
    'open',
    'edit',
    'newSubfolder',
    '|',
    'cut',
    'paste',
    '|',
    'delete',
  ])
})

test('a folder of the sidebar also moves up and down', () => {
  const entries = buildMenu({ kind: 'treeFolder', canMoveUp: false })
  assert.deepEqual(ids(entries), [
    'open',
    'edit',
    'newSubfolder',
    'moveUp',
    'moveDown',
    '|',
    'cut',
    '|',
    'delete',
  ])
  const up = entries.find(
    (entry) => !('separator' in entry) && entry.id === 'moveUp',
  )
  assert.ok(up && !('separator' in up) && up.disabled === true)
})

test('the empty area offers new entry, new folder and paste where a paste goes', () => {
  assert.deepEqual(menu({ kind: 'empty' }), ['newEntry', 'newFolder'])
  assert.deepEqual(menu({ kind: 'empty', ablageFilled: true }), [
    'newEntry',
    'newFolder',
  ])
  assert.deepEqual(
    menu({ kind: 'empty', ablageFilled: true, pasteHere: true }),
    ['newEntry', 'newFolder', '|', 'paste'],
  )
})

test('the trash of the sidebar offers to empty it, greyed out when it is empty', () => {
  const entries = buildMenu({ kind: 'trashNode', trashEmpty: true })
  assert.deepEqual(ids(entries), ['emptyTrash'])
  const only = entries[0]!
  assert.ok(!('separator' in only) && only.disabled === true)
})

test('items carry a label key and the shortcuts they share with the list', () => {
  for (const entry of buildMenu({
    kind: 'entry',
    hasUsername: true,
    hasPassword: true,
    copyAvailable: true,
  })) {
    if ('separator' in entry) continue
    assert.equal(entry.labelKey, `passwords.menu.${entry.id}`)
  }
  const cut = buildMenu({ kind: 'entry' }).find(
    (entry) => !('separator' in entry) && entry.id === 'cut',
  )
  assert.ok(cut && !('separator' in cut))
  assert.deepEqual(cut.shortcut, { mod: true, key: 'X' })
})

test('no menu starts, ends or doubles a separator', () => {
  const inputs: MenuInput[] = [
    { kind: 'entry' },
    { kind: 'entry', selectionSize: 2 },
    { kind: 'folder', inTrash: true },
    { kind: 'empty' },
    { kind: 'trashNode' },
  ]
  for (const input of inputs) {
    const list = menu(input)
    assert.notEqual(list[0], '|')
    assert.notEqual(list.at(-1), '|')
    assert.ok(!list.join(',').includes('|,|'), JSON.stringify(input))
  }
})
