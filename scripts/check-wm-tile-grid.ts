// Part of `pnpm check:wm-state` (spec 091-tab-switcher, T003): the column count of a tile grid and the
// keyboard's moves through it (`src/lib/wm/tileGrid.ts`). The grid scrolls, so there are no rows and no
// pages to count; the count only follows the width of the grid.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  GAP,
  gridColumns,
  MAX_COLUMNS,
  MIN_TILE_WIDTH,
  moveFocus,
} from '../src/lib/wm/tileGrid.ts'

// ---------------------------------------------------------------------------
// gridColumns
// ---------------------------------------------------------------------------

test('the constants are those of the data model', () => {
  assert.equal(GAP, 12)
  assert.equal(MIN_TILE_WIDTH, 112)
  assert.equal(MAX_COLUMNS, 7)
})

test('a grid has at least one column, however narrow', () => {
  assert.equal(gridColumns(0, { compact: false }), 1)
  assert.equal(gridColumns(111, { compact: false }), 1)
  assert.equal(gridColumns(235, { compact: false }), 1)
})

test('the column count changes exactly where a tile would get narrower than MIN_TILE_WIDTH', () => {
  // n columns need n * MIN_TILE_WIDTH + (n - 1) * GAP: 112, 236, 360, 484, 608, 732, 856.
  assert.equal(gridColumns(235, { compact: false }), 1)
  assert.equal(gridColumns(236, { compact: false }), 2)
  assert.equal(gridColumns(359, { compact: false }), 2)
  assert.equal(gridColumns(360, { compact: false }), 3)
  assert.equal(gridColumns(855, { compact: false }), 6)
  assert.equal(gridColumns(856, { compact: false }), 7)
})

test('a wide grid stops at seven columns', () => {
  assert.equal(gridColumns(1200, { compact: false }), 7)
  assert.equal(gridColumns(5000, { compact: false }), 7)
})

test('a compact window has at most two columns', () => {
  assert.equal(gridColumns(235, { compact: true }), 1)
  assert.equal(gridColumns(236, { compact: true }), 2)
  assert.equal(gridColumns(360, { compact: true }), 2)
  assert.equal(gridColumns(2000, { compact: true }), 2)
})

test('a 360 px phone gets three columns only when it is not compact, never three cramped ones', () => {
  assert.equal(gridColumns(360, { compact: true }), 2)
  // A tile is never narrower than MIN_TILE_WIDTH at the chosen count.
  for (let width = 0; width <= 1000; width += 1) {
    const columns = gridColumns(width, { compact: false })
    if (columns > 1) {
      const tile = (width - (columns - 1) * GAP) / columns
      assert.ok(tile >= MIN_TILE_WIDTH, `width ${width}: ${tile}px per tile`)
    }
  }
})

// ---------------------------------------------------------------------------
// moveFocus: 8 tiles in rows of 3 (0 1 2 / 3 4 5 / 6 7)
// ---------------------------------------------------------------------------

const move = (index: number, key: string, rtl = false) =>
  moveFocus(index, key, 3, 8, rtl)

test('left and right move by one and stop at the ends', () => {
  assert.equal(move(0, 'ArrowRight'), 1)
  assert.equal(move(1, 'ArrowLeft'), 0)
  assert.equal(move(0, 'ArrowLeft'), 0)
  assert.equal(move(7, 'ArrowRight'), 7)
})

test('in a right-to-left layout left and right swap', () => {
  assert.equal(move(1, 'ArrowRight', true), 0)
  assert.equal(move(0, 'ArrowLeft', true), 1)
  assert.equal(move(0, 'ArrowRight', true), 0)
  assert.equal(move(7, 'ArrowLeft', true), 7)
})

test('up and down do not depend on the layout direction', () => {
  assert.equal(move(1, 'ArrowDown', true), 4)
  assert.equal(move(4, 'ArrowUp', true), 1)
})

test('down and up move by a row', () => {
  assert.equal(move(1, 'ArrowDown'), 4)
  assert.equal(move(4, 'ArrowDown'), 7)
  assert.equal(move(4, 'ArrowUp'), 1)
  assert.equal(move(1, 'ArrowUp'), 1)
})

test('down from a full row above a shorter last row goes to the last tile, and stays there', () => {
  // Index 5 has no tile below it: the last row holds 6 and 7.
  assert.equal(move(5, 'ArrowDown'), 7)
  assert.equal(move(7, 'ArrowDown'), 7)
  assert.equal(move(6, 'ArrowDown'), 6)
})

test('Home and End go to the first and the last tile', () => {
  assert.equal(move(4, 'Home'), 0)
  assert.equal(move(4, 'End'), 7)
})

test('a key that does not move the focus gives null, so the caller keeps its default', () => {
  assert.equal(move(2, 'a'), null)
  assert.equal(move(2, 'Enter'), null)
  assert.equal(move(2, 'Tab'), null)
})

test('with no tiles there is nothing to focus', () => {
  assert.equal(moveFocus(0, 'ArrowRight', 3, 0, false), null)
})

test('no tile focused yet: the first arrow lands on the first tile', () => {
  assert.equal(moveFocus(-1, 'ArrowDown', 3, 8, false), 0)
  assert.equal(moveFocus(-1, 'ArrowRight', 3, 8, false), 0)
})

test('a single column moves one tile per arrow in both directions', () => {
  assert.equal(moveFocus(0, 'ArrowDown', 1, 4, false), 1)
  assert.equal(moveFocus(2, 'ArrowUp', 1, 4, false), 1)
})
