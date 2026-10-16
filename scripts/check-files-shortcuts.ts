// Part of `pnpm check:files` (spec 044 FR-017b): the keyboard shortcuts of the file browser
// (src/lib/files/shortcuts.ts). They never act in a text field, a dialog or on selected text, never
// take the chords of the window manager, and the keys that act on entries need the focus in the list.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  keyboardSelection,
  moveIndex,
  resolveFilesShortcut,
  typeAhead,
  type FilesKeyEvent,
} from '../src/lib/files/shortcuts.ts'

const ROW = { tagName: 'BUTTON' }
const INPUT = { tagName: 'INPUT', type: 'search' }

const resolve = (
  key: string,
  extra: Partial<FilesKeyEvent> = {},
  platform: 'mac' | 'default' = 'default',
) =>
  resolveFilesShortcut({
    key,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    target: ROW,
    platform,
    inList: true,
    inDialog: false,
    textSelected: false,
    grid: false,
    typing: false,
    ...extra,
  })

const command = (name: string) => ({ kind: 'command', command: name })

test('Ctrl chords on Linux and Windows, Cmd chords on macOS', () => {
  for (const [letter, name] of [
    ['a', 'selectAll'],
    ['c', 'copy'],
    ['x', 'cut'],
    ['v', 'paste'],
    ['f', 'focusSearch'],
    ['r', 'reload'],
    ['h', 'toggleHidden'],
  ] as const) {
    assert.deepEqual(resolve(letter, { ctrlKey: true }), command(name))
    assert.deepEqual(resolve(letter, { metaKey: true }, 'mac'), command(name))
    assert.equal(resolve(letter, { metaKey: true }), null)
    assert.equal(resolve(letter, { ctrlKey: true }, 'mac'), null)
  }
  assert.deepEqual(
    resolve('N', { ctrlKey: true, shiftKey: true }),
    command('newFolder'),
  )
  assert.deepEqual(
    resolve('N', { metaKey: true, shiftKey: true }, 'mac'),
    command('newFolder'),
  )
  assert.equal(resolve('C', { ctrlKey: true, shiftKey: true }), null)
})

test('the plain keys', () => {
  assert.deepEqual(resolve('F2'), command('rename'))
  assert.deepEqual(resolve('F5'), command('reload'))
  assert.deepEqual(resolve('Delete'), command('delete'))
  assert.equal(resolve('Delete', { shiftKey: true }), null)
  assert.deepEqual(resolve('Escape'), command('clearSelection'))
  assert.equal(resolve('Backspace'), null)
  assert.deepEqual(resolve('Backspace', {}, 'mac'), command('delete'))
  assert.deepEqual(
    resolve('Backspace', { metaKey: true }, 'mac'),
    command('delete'),
  )
})

test('up one folder with Alt+Up, on macOS also with Cmd+Up', () => {
  assert.deepEqual(resolve('ArrowUp', { altKey: true }), command('up'))
  assert.deepEqual(resolve('ArrowUp', { altKey: true }, 'mac'), command('up'))
  assert.deepEqual(resolve('ArrowUp', { metaKey: true }, 'mac'), command('up'))
  assert.equal(resolve('ArrowUp', { ctrlKey: true }), null)
})

test("the window manager's chords are never taken", () => {
  assert.equal(resolve('ArrowLeft', { altKey: true }), null)
  assert.equal(resolve('ArrowRight', { altKey: true }), null)
  assert.equal(resolve('ArrowLeft', { altKey: true }, 'mac'), null)
  assert.equal(resolve('[', { metaKey: true }, 'mac'), null)
  assert.equal(resolve(']', { metaKey: true }, 'mac'), null)
})

test('the arrows move along the list, and in the grid along rows and between them', () => {
  const move = (to: string, extend = false) => ({ kind: 'move', to, extend })
  assert.deepEqual(resolve('ArrowDown'), move('next'))
  assert.deepEqual(resolve('ArrowUp'), move('previous'))
  assert.equal(resolve('ArrowRight'), null)
  assert.equal(resolve('ArrowLeft'), null)
  assert.deepEqual(resolve('ArrowDown', { grid: true }), move('below'))
  assert.deepEqual(resolve('ArrowUp', { grid: true }), move('above'))
  assert.deepEqual(resolve('ArrowRight', { grid: true }), move('next'))
  assert.deepEqual(resolve('ArrowLeft', { grid: true }), move('previous'))
  assert.deepEqual(resolve('Home'), move('first'))
  assert.deepEqual(resolve('End'), move('last'))
  assert.deepEqual(resolve('ArrowDown', { shiftKey: true }), move('next', true))
  assert.deepEqual(resolve('End', { shiftKey: true }), move('last', true))
})

test('letters jump to a name; Space opens unless a name is being typed', () => {
  assert.deepEqual(resolve('b'), { kind: 'type', text: 'b' })
  assert.deepEqual(resolve('B', { shiftKey: true }), {
    kind: 'type',
    text: 'B',
  })
  assert.deepEqual(resolve('ä'), { kind: 'type', text: 'ä' })
  assert.equal(resolve(' '), null)
  assert.deepEqual(resolve(' ', { typing: true }), { kind: 'type', text: ' ' })
  assert.equal(resolve('Enter'), null)
  assert.equal(resolve('Tab'), null)
})

test('nothing acts in a text field, a dialog or on selected text', () => {
  for (const extra of [
    { target: INPUT },
    { inDialog: true },
    { textSelected: true },
  ]) {
    assert.equal(resolve('c', { ctrlKey: true, ...extra }), null)
    assert.equal(resolve('Delete', extra), null)
    assert.equal(resolve('ArrowDown', extra), null)
    assert.equal(resolve('b', extra), null)
  }
})

test('the keys that act on entries need the focus in the list', () => {
  const outside = { inList: false }
  assert.equal(resolve('Delete', outside), null)
  assert.equal(resolve('F2', outside), null)
  assert.equal(resolve('ArrowDown', outside), null)
  assert.equal(resolve('b', outside), null)
  assert.equal(resolve('a', { ctrlKey: true, ...outside }), null)
  assert.equal(resolve('c', { ctrlKey: true, ...outside }), null)
  assert.equal(resolve('x', { ctrlKey: true, ...outside }), null)
  assert.deepEqual(
    resolve('v', { ctrlKey: true, ...outside }),
    command('paste'),
  )
  assert.deepEqual(
    resolve('N', { ctrlKey: true, shiftKey: true, ...outside }),
    command('newFolder'),
  )
  assert.deepEqual(
    resolve('ArrowUp', { altKey: true, ...outside }),
    command('up'),
  )
  assert.deepEqual(resolve('F5', outside), command('reload'))
  assert.deepEqual(resolve('Escape', outside), command('clearSelection'))
  assert.deepEqual(
    resolve('f', { ctrlKey: true, ...outside }),
    command('focusSearch'),
  )
  assert.deepEqual(
    resolve('h', { ctrlKey: true, ...outside }),
    command('toggleHidden'),
  )
})

test('a move stays inside the folder and the grid', () => {
  assert.equal(moveIndex(-1, 'next', 5, 1), 0)
  assert.equal(moveIndex(-1, 'previous', 5, 1), 0)
  assert.equal(moveIndex(-1, 'last', 5, 1), 4)
  assert.equal(moveIndex(2, 'next', 5, 1), 3)
  assert.equal(moveIndex(4, 'next', 5, 1), 4)
  assert.equal(moveIndex(0, 'previous', 5, 1), 0)
  assert.equal(moveIndex(3, 'first', 5, 1), 0)
  assert.equal(moveIndex(1, 'next', 0, 1), -1)
  // A grid of 7 entries in rows of 3: 0 1 2 / 3 4 5 / 6
  assert.equal(moveIndex(1, 'below', 7, 3), 4)
  assert.equal(moveIndex(4, 'below', 7, 3), 6, 'the last row is shorter')
  assert.equal(moveIndex(6, 'below', 7, 3), 6)
  assert.equal(moveIndex(4, 'above', 7, 3), 1)
  assert.equal(moveIndex(1, 'above', 7, 3), 1)
  assert.equal(moveIndex(2, 'next', 7, 3), 3, 'next goes on to the next row')
})

test('a move selects the entry; Shift selects from the anchor', () => {
  const order = ['/x/a', '/x/b', '/x/c', '/x/d']
  const one = { paths: ['/x/b'], anchor: '/x/b' }
  assert.deepEqual(keyboardSelection(one, '/x/c', order, false), {
    paths: ['/x/c'],
    anchor: '/x/c',
  })
  assert.deepEqual(keyboardSelection(one, '/x/d', order, true), {
    paths: ['/x/b', '/x/c', '/x/d'],
    anchor: '/x/b',
  })
  assert.deepEqual(keyboardSelection(one, '/x/a', order, true), {
    paths: ['/x/a', '/x/b'],
    anchor: '/x/b',
  })
  const none = { paths: [], anchor: null }
  assert.deepEqual(keyboardSelection(none, '/x/c', order, true), {
    paths: ['/x/c'],
    anchor: '/x/c',
  })
})

test('type-to-jump finds the next name that starts with the letters', () => {
  const names = ['Bilder', 'brief.txt', 'Musik', 'notiz.md', 'Bericht.pdf']
  assert.equal(typeAhead(names, 'm', -1), 2)
  assert.equal(typeAhead(names, 'M', -1), 2, 'case is ignored')
  assert.equal(typeAhead(names, 'b', 0), 1, 'one letter goes on to the next')
  assert.equal(typeAhead(names, 'bb', 1), 4, 'a repeated letter steps on')
  assert.equal(typeAhead(names, 'bbb', 4), 0, 'and wraps at the end')
  assert.equal(typeAhead(names, 'bi', 0), 0, 'a longer prefix may stay')
  assert.equal(typeAhead(names, 'bri', 0), 1)
  assert.equal(typeAhead(names, 'x', 2), -1)
  assert.equal(typeAhead([], 'a', -1), -1)
})
