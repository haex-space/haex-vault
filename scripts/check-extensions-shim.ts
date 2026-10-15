// Part of `pnpm check:extensions` (spec 017, T026): the messages between holzi and the frame shim
// of an extension (src/lib/extensions/shim-protocol.ts) and the shortcut fields holzi sends
// (src/lib/wm/keybindings.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import {
  chordFromEvent,
  embeddedShortcuts,
  resolveChord,
} from '../src/lib/wm/keybindings.ts'
import {
  MAX_TITLE_LENGTH,
  initMessage,
  navigateMessage,
  readShimMessage,
  sameLocation,
} from '../src/lib/extensions/shim-protocol.ts'

const SENT = [
  {
    id: 'wm.tab.back',
    code: 'ArrowLeft',
    ctrl: false,
    alt: true,
    shift: false,
    meta: false,
  },
]

test('hello says whether the document is new', () => {
  assert.deepEqual(readShimMessage({ type: 'hello', fresh: true }, SENT), {
    kind: 'hello',
    fresh: true,
  })
  assert.deepEqual(readShimMessage({ type: 'hello', fresh: false }, SENT), {
    kind: 'hello',
    fresh: false,
  })
  assert.equal(readShimMessage({ type: 'hello' }, SENT), null)
  assert.equal(readShimMessage({ type: 'hello', fresh: 'yes' }, SENT), null)
})

test('nav becomes a navigation of the own tab, with replace', () => {
  assert.deepEqual(
    readShimMessage(
      { type: 'nav', path: '/notes/1', query: 'b=2&a=1', replace: true },
      SENT,
    ),
    {
      kind: 'navigate',
      location: { path: '/notes/1', query: { a: '1', b: '2' } },
      replace: true,
    },
  )
  assert.deepEqual(
    readShimMessage({ type: 'nav', path: '/', query: '' }, SENT),
    {
      kind: 'navigate',
      location: { path: '/', query: {} },
      replace: false,
    },
  )
  assert.equal(readShimMessage({ type: 'nav', path: 'relative' }, SENT), null)
  assert.equal(readShimMessage({ type: 'nav', path: 42 }, SENT), null)
})

test('titles, close guards and close are read; long titles are refused', () => {
  assert.deepEqual(readShimMessage({ type: 'title', text: 'Notizen' }, SENT), {
    kind: 'title',
    title: 'Notizen',
  })
  assert.equal(
    readShimMessage(
      { type: 'title', text: 'x'.repeat(MAX_TITLE_LENGTH + 1) },
      SENT,
    ),
    null,
  )
  assert.deepEqual(
    readShimMessage({ type: 'closeGuard', active: true }, SENT),
    {
      kind: 'closeGuard',
      active: true,
    },
  )
  assert.equal(
    readShimMessage({ type: 'closeGuard', active: 'yes' }, SENT),
    null,
  )
  assert.deepEqual(readShimMessage({ type: 'close' }, SENT), { kind: 'close' })
  assert.deepEqual(readShimMessage({ type: 'focus' }, SENT), { kind: 'focus' })
})

test('only shortcuts holzi sent run; unknown types and non-objects are ignored', () => {
  assert.deepEqual(
    readShimMessage({ type: 'shortcut', id: 'wm.tab.back' }, SENT),
    {
      kind: 'shortcut',
      actionId: 'wm.tab.back',
    },
  )
  assert.equal(
    readShimMessage({ type: 'shortcut', id: 'wm.window.close' }, SENT),
    null,
  )
  for (const data of [{ type: 'eval', code: 'x' }, null, 'nav', 5, {}]) {
    assert.equal(readShimMessage(data, SENT), null)
  }
})

test('holzi Back becomes navigate with the path and the sorted query', () => {
  assert.deepEqual(
    navigateMessage({ path: '/notes/1', query: { b: '2', a: '1' } }),
    {
      type: 'navigate',
      path: '/notes/1',
      query: 'a=1&b=2',
    },
  )
  assert.deepEqual(navigateMessage({ path: '/', query: {} }), {
    type: 'navigate',
    path: '/',
    query: '',
  })
  assert.ok(
    sameLocation(
      { path: '/a', query: { x: '1', y: '2' } },
      { path: '/a', query: { y: '2', x: '1' } },
    ),
  )
})

test('the shortcut fields resolve to the same action as the chord of the same key event', () => {
  for (const platform of ['default', 'mac'] as const) {
    const fields = embeddedShortcuts(ALL_ACTIONS, platform)
    assert.ok(fields.length > 0)
    for (const f of fields) {
      const chord = chordFromEvent({
        code: f.code,
        ctrlKey: f.ctrl,
        altKey: f.alt,
        shiftKey: f.shift,
        metaKey: f.meta,
      })
      assert.ok(chord)
      assert.equal(resolveChord(ALL_ACTIONS, chord, platform)?.id, f.id)
    }
  }
  assert.deepEqual(initMessage(SENT), {
    type: 'holzi:frame:init',
    shortcuts: SENT,
  })
})
