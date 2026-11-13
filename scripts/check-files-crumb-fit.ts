// Part of `pnpm check:files`: which crumbs of a path stay drawn when the room is short
// (`src/lib/wm/crumbFit.ts`, drawn by `CrumbTrail`).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  firstShownCrumb,
  MIN_LAST_CRUMB,
  type CrumbMeasure,
  type CrumbRect,
} from '../src/lib/wm/crumbFit.ts'

const GAP = 6
const SEP = 10
const MORE = 20

/** A trail laid out as the probe does: crumbs of the given widths, a separator between each pair. */
function trail(widths: number[]): CrumbMeasure {
  const items: CrumbRect[] = []
  const separators: CrumbRect[] = []
  let x = 0
  widths.forEach((width, index) => {
    if (index > 0) {
      separators.push({ left: x, right: x + SEP, width: SEP })
      x += SEP + GAP
    }
    items.push({ left: x, right: x + width, width })
    x += width + GAP
  })
  return { items, separators, more: MORE }
}

// Four crumbs of 40: the whole trail is 4 * 40 + 3 * 10 + 6 * 6 = 226 wide.
const FOUR = trail([40, 40, 40, 40])

test('a trail that fits is drawn whole', () => {
  assert.equal(firstShownCrumb(FOUR, 226), 0)
  assert.equal(firstShownCrumb(FOUR, 1000), 0)
})

test('the crumbs at the front give way first, the ones at the back stay', () => {
  // One crumb too wide: the menu and the last three remain.
  assert.equal(firstShownCrumb(FOUR, 225), 1)
  // Room for the menu, the last two crumbs and their separators only.
  assert.equal(firstShownCrumb(FOUR, 150), 2)
  assert.equal(firstShownCrumb(FOUR, 100), 3)
})

test('the open folder alone stays while it keeps its minimum width, then the menu holds all', () => {
  const alone = MORE + GAP + SEP + GAP + MIN_LAST_CRUMB
  assert.equal(firstShownCrumb(FOUR, alone), 3)
  assert.equal(firstShownCrumb(FOUR, alone - 1), 4)
})

test('a trail of one crumb is never put in a menu', () => {
  const one = trail([300])
  assert.equal(firstShownCrumb(one, 100), 0)
})

test('an empty trail has nothing to hide', () => {
  assert.equal(
    firstShownCrumb({ items: [], separators: [], more: MORE }, 10),
    0,
  )
})

test('a wider box never hides more than a narrower one', () => {
  let previous = Infinity
  for (let room = 60; room <= 240; room += 1) {
    const from = firstShownCrumb(FOUR, room)
    assert.ok(from <= previous, `room ${room}: ${from} > ${previous}`)
    previous = from
  }
})
