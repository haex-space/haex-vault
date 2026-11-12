// Part of `pnpm check:extensions` (spec 090-app-header-bar, T029): what the frontend takes from the host's
// `extension-header-set` event and what it sends back (`src/lib/extensions/header.ts`).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  acceptHeaderSet,
  applyIcons,
  HEADER_EVENT,
  headerEventMessage,
  iconNames,
} from '../src/lib/extensions/header.ts'
import type { WmHeaderDescription } from '../src/types/bindings/WmHeaderDescription.ts'

const button = (id: string, icon?: string) =>
  ({ type: 'button', id, label: id, ...(icon ? { icon } : {}) }) as const

const header = (icon: string): WmHeaderDescription => ({
  end: [button('a', icon)],
})

// ---------------------------------------------------------------------------
// acceptHeaderSet: only the frame's own, well formed events
// ---------------------------------------------------------------------------

test('a payload for the own frame is accepted with its number and description', () => {
  const given = header('a.svg')
  assert.deepEqual(
    acceptHeaderSet('f1', { frame: 'f1', seq: 3, header: given }),
    {
      seq: 3,
      header: given,
    },
  )
})

test('a payload naming another frame changes nothing', () => {
  assert.equal(acceptHeaderSet('f1', { frame: 'f2', seq: 1, header: {} }), null)
  assert.equal(acceptHeaderSet('f1', { seq: 1, header: {} }), null)
})

test('a frame that is not open yet accepts nothing', () => {
  assert.equal(acceptHeaderSet(null, { frame: 'f1', seq: 1, header: {} }), null)
})

test('a malformed payload is dropped', () => {
  for (const payload of [
    null,
    undefined,
    'x',
    42,
    [],
    { frame: 'f1' },
    { frame: 'f1', seq: '1', header: {} },
    { frame: 'f1', seq: Number.NaN, header: {} },
    { frame: 'f1', seq: 1 },
    { frame: 'f1', seq: 1, header: null },
    { frame: 'f1', seq: 1, header: [] },
    { frame: 'f1', seq: 1, header: 'x' },
  ]) {
    assert.equal(acceptHeaderSet('f1', payload), null, JSON.stringify(payload))
  }
})

// ---------------------------------------------------------------------------
// headerEventMessage
// ---------------------------------------------------------------------------

test('a header event travels as the SDK port message', () => {
  assert.equal(HEADER_EVENT, 'haextension:header:event')
  assert.deepEqual(
    headerEventMessage({ id: 'q', kind: 'input', value: 'abc' }, 1234),
    {
      type: 'haextension:header:event',
      data: { id: 'q', kind: 'input', value: 'abc' },
      timestamp: 1234,
    },
  )
  assert.deepEqual(headerEventMessage({ id: 'a', kind: 'click' }, 1), {
    type: 'haextension:header:event',
    data: { id: 'a', kind: 'click' },
    timestamp: 1,
  })
})

// ---------------------------------------------------------------------------
// iconNames and applyIcons: the table kept for the frame session
// ---------------------------------------------------------------------------

test('every place a picture can be named is found, each name once', () => {
  const given: WmHeaderDescription = {
    start: [button('a', 'a.svg'), button('b', 'a.svg'), button('c')],
    center: [
      {
        type: 'menu',
        id: 'm',
        label: 'M',
        icon: 'm.svg',
        items: [
          { type: 'item', id: 'i', label: 'I', icon: 'i.svg' },
          { type: 'separator' },
          { type: 'item', id: 'j', label: 'J' },
        ],
      },
    ],
    end: [
      {
        type: 'choice',
        id: 'c1',
        label: 'C',
        selected: 'o',
        options: [{ id: 'o', label: 'O', icon: 'o.svg' }],
      },
    ],
  }
  assert.deepEqual([...iconNames(given)].sort(), [
    'a.svg',
    'i.svg',
    'm.svg',
    'o.svg',
  ])
})

test('a description brings its new pictures and the table keeps the old ones it still names', () => {
  const first = applyIcons(
    {},
    { ...header('a.svg'), icons: { 'a.svg': 'data:a' } },
  )
  assert.deepEqual(first.table, { 'a.svg': 'data:a' })
  assert.deepEqual(first.header.icons, { 'a.svg': 'data:a' })

  // Nothing new is sent for a picture already sent; the table still resolves it.
  const second = applyIcons(first.table, header('a.svg'))
  assert.deepEqual(second.header.icons, { 'a.svg': 'data:a' })
})

test('a picture the description no longer names is dropped, as Rust forgets it', () => {
  const withA = applyIcons(
    {},
    { ...header('a.svg'), icons: { 'a.svg': 'data:a' } },
  )
  const withB = applyIcons(withA.table, {
    ...header('b.svg'),
    icons: { 'b.svg': 'data:b' },
  })
  assert.deepEqual(withB.table, { 'b.svg': 'data:b' })
  // Used again later, it comes with the description again.
  const again = applyIcons(withB.table, {
    ...header('a.svg'),
    icons: { 'a.svg': 'data:a' },
  })
  assert.deepEqual(again.table, { 'a.svg': 'data:a' })
})

test('a picture the description names but nobody sent draws nothing, it is not invented', () => {
  const result = applyIcons({}, header('missing.svg'))
  assert.deepEqual(result.table, {})
  assert.deepEqual(result.header.icons, {})
})

test('an old table is not changed in place', () => {
  const table = { 'a.svg': 'data:a' }
  applyIcons(table, { ...header('b.svg'), icons: { 'b.svg': 'data:b' } })
  assert.deepEqual(table, { 'a.svg': 'data:a' })
})
