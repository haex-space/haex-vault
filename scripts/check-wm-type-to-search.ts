// Part of `pnpm check:wm-state`: which key events open a window's header search by typing
// (`src/lib/wm/typeToSearch.ts`). Pure; the elements are stubs that answer `closest`.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { startsTyping, type TypeKeyEvent } from '../src/lib/wm/typeToSearch.ts'

const PAGE = { closest: () => null }
const FIELD = { closest: () => ({}) }

function key(over: Partial<TypeKeyEvent> = {}): TypeKeyEvent {
  return {
    key: 'a',
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    isComposing: false,
    defaultPrevented: false,
    target: PAGE,
    ...over,
  }
}

test('a printable character aimed at the page starts the search', () => {
  assert.equal(startsTyping(key(), true), true)
  assert.equal(startsTyping(key({ key: 'Ä' }), true), true)
  assert.equal(startsTyping(key({ key: '7' }), true), true)
})

test('a field keeps its own keys', () => {
  assert.equal(startsTyping(key({ target: FIELD }), true), false)
})

test('a key aimed elsewhere than at this window or the page is not ours', () => {
  assert.equal(startsTyping(key(), false), false)
})

test('shortcuts, special keys, space and compositions do not start it', () => {
  assert.equal(startsTyping(key({ ctrlKey: true }), true), false)
  assert.equal(startsTyping(key({ metaKey: true }), true), false)
  assert.equal(startsTyping(key({ altKey: true }), true), false)
  assert.equal(startsTyping(key({ key: 'Enter' }), true), false)
  assert.equal(startsTyping(key({ key: 'ArrowDown' }), true), false)
  assert.equal(startsTyping(key({ key: ' ' }), true), false)
  assert.equal(startsTyping(key({ isComposing: true }), true), false)
})

test('a key a handler already took is left alone', () => {
  assert.equal(startsTyping(key({ defaultPrevented: true }), true), false)
})
