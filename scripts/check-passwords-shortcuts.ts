// Part of `pnpm check:passwords` (spec 036, FR-016): the keyboard shortcuts of the list
// (src/lib/passwords/shortcuts.ts). They never act in a text field, a dialog or on selected text,
// and they never take the chords of the window manager.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  resolveListShortcut,
  type ShortcutEvent,
} from '../src/lib/passwords/shortcuts.ts'

const ROW = { tagName: 'BUTTON' }
const INPUT = { tagName: 'INPUT', type: 'search' }

function press(
  key: string,
  extra: Partial<ShortcutEvent> = {},
): Omit<ShortcutEvent, 'platform'> & { platform?: 'mac' | 'default' } {
  return {
    key,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    target: ROW,
    inList: true,
    inDialog: false,
    textSelected: false,
    ...extra,
  }
}

const resolve = (
  key: string,
  extra: Partial<ShortcutEvent> = {},
  platform: 'mac' | 'default' = 'default',
) => resolveListShortcut({ ...press(key, extra), platform })

test('Ctrl chords on Linux and Windows, Cmd chords on macOS', () => {
  assert.equal(resolve('a', { ctrlKey: true }), 'selectAll')
  assert.equal(resolve('x', { ctrlKey: true }), 'cut')
  assert.equal(resolve('c', { ctrlKey: true }), 'copy')
  assert.equal(resolve('v', { ctrlKey: true }), 'paste')
  assert.equal(resolve('b', { ctrlKey: true }), 'copyUsername')
  assert.equal(resolve('C', { ctrlKey: true, shiftKey: true }), 'copyPassword')
  assert.equal(resolve('f', { ctrlKey: true }), 'focusSearch')
  assert.equal(resolve('x', { metaKey: true }, 'mac'), 'cut')
  assert.equal(
    resolve('C', { metaKey: true, shiftKey: true }, 'mac'),
    'copyPassword',
  )
  // The other platform's modifier does nothing.
  assert.equal(resolve('x', { metaKey: true }), null)
  assert.equal(resolve('x', { ctrlKey: true }, 'mac'), null)
  // A chord with Alt, or with both modifiers, is not ours.
  assert.equal(resolve('x', { ctrlKey: true, altKey: true }), null)
  assert.equal(resolve('x', { ctrlKey: true, metaKey: true }), null)
})

test('plain keys: Delete, Enter, Escape and the arrows', () => {
  assert.equal(resolve('Delete'), 'delete')
  assert.equal(resolve('Enter'), 'open')
  assert.equal(resolve('Escape'), 'clearSelection')
  assert.equal(resolve('ArrowDown'), 'focusNext')
  assert.equal(resolve('ArrowUp'), 'focusPrevious')
  assert.equal(resolve('ArrowDown', { shiftKey: true }), 'extendNext')
  assert.equal(resolve('ArrowUp', { shiftKey: true }), 'extendPrevious')
  // A letter without a modifier is typing, not a command.
  assert.equal(resolve('x'), null)
  // macOS keyboards name their delete key Backspace.
  assert.equal(resolve('Backspace', {}, 'mac'), 'delete')
  assert.equal(resolve('Backspace'), null)
})

test('the keys that act on a row only work with the focus in the list', () => {
  for (const key of ['Delete', 'Enter', 'ArrowDown', 'ArrowUp']) {
    assert.equal(resolve(key, { inList: false }), null, key)
  }
  // The others work anywhere in the window.
  assert.equal(resolve('Escape', { inList: false }), 'clearSelection')
  assert.equal(resolve('v', { ctrlKey: true, inList: false }), 'paste')
})

test('nothing in a text field, a dialog or on selected text', () => {
  assert.equal(resolve('a', { ctrlKey: true, target: INPUT }), null)
  assert.equal(
    resolve('c', { ctrlKey: true, target: { tagName: 'TEXTAREA' } }),
    null,
  )
  assert.equal(
    resolve('c', { ctrlKey: true, target: { isContentEditable: true } }),
    null,
  )
  assert.equal(resolve('Delete', { target: INPUT }), null)
  assert.equal(resolve('c', { ctrlKey: true, inDialog: true }), null)
  assert.equal(resolve('Escape', { inDialog: true }), null)
  assert.equal(resolve('c', { ctrlKey: true, textSelected: true }), null)
  // A checkbox is no text field.
  assert.equal(
    resolve('a', {
      ctrlKey: true,
      target: { tagName: 'INPUT', type: 'checkbox' },
    }),
    'selectAll',
  )
})

test('the chords of the window manager are never taken', () => {
  assert.equal(resolve('ArrowLeft', { altKey: true }), null)
  assert.equal(resolve('ArrowRight', { altKey: true }), null)
  assert.equal(resolve('ArrowDown', { altKey: true }), null)
  assert.equal(resolve('[', { metaKey: true }, 'mac'), null)
  assert.equal(resolve(']', { metaKey: true }, 'mac'), null)
})
