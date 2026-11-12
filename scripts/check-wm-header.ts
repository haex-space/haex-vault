// Part of `pnpm check:wm-state` (spec 090-app-header-bar, T003): how the header collapses
// (`src/lib/wm/headerLayout.ts`, contracts/collapse.md) and how a search field keeps what the user
// typed (`src/lib/wm/headerSearch.ts`, FR-012, FR-013). Both are pure; the clock is injected.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  layoutHeader,
  minWidthOf,
  type ControlState,
  type LayoutControl,
  type LayoutFixed,
} from '../src/lib/wm/headerLayout.ts'
import { newTabRuntime, type TabRuntime } from '../src/lib/wm/types.ts'
import { createWmTabHeader } from '../src/stores/wmTabHeader.ts'
import type { WmHeaderEvent } from '../src/types/bindings/WmHeaderEvent.ts'
import {
  createHeaderSearch,
  INPUT_MERGE_MS,
} from '../src/lib/wm/headerSearch.ts'

// ---------------------------------------------------------------------------
// layoutHeader. The row's width is
//   padding + windowButtons + the widths of the items + gap * (number of items),
// an item being a control in the row or the menu button.
// ---------------------------------------------------------------------------

const FIXED: LayoutFixed = {
  windowButtons: 100,
  menuButton: 32,
  gap: 8,
  padding: 16,
}

function control(
  id: string,
  extra: Partial<LayoutControl> = {},
): LayoutControl {
  return {
    id,
    area: 'start',
    index: 0,
    order: null,
    form: 'overflow',
    fullWidth: 100,
    iconWidth: null,
    minWidth: null,
    ...extra,
  }
}

/** Three controls of one area, in array order a, b, c, each 100 wide. */
function three(
  orders: [number | null, number | null, number | null],
  forms: LayoutControl['form'][] = ['overflow', 'overflow', 'overflow'],
): LayoutControl[] {
  return ['a', 'b', 'c'].map((id, index) =>
    control(id, {
      index,
      order: orders[index] ?? null,
      form: forms[index] ?? 'overflow',
    }),
  )
}

function states(
  controls: LayoutControl[],
  available: number,
  fixed: LayoutFixed = FIXED,
): Record<string, ControlState> {
  return Object.fromEntries(layoutHeader(controls, fixed, available).states)
}

test('everything fits: every control is in the row and there is no menu', () => {
  // 16 + 100 + 3 * 100 + 3 * 8 = 440
  const result = layoutHeader(three([1, 2, 3]), FIXED, 440)
  assert.equal(result.menuVisible, false)
  assert.deepEqual(Object.fromEntries(result.states), {
    a: 'row',
    b: 'row',
    c: 'row',
  })
})

test('a header with no controls has no menu, however narrow', () => {
  const result = layoutHeader([], FIXED, 10)
  assert.equal(result.menuVisible, false)
  assert.equal(result.states.size, 0)
})

test('the control with the lowest order collapses first, then the next; widening restores them in reverse', () => {
  const controls = three([1, 2, 3])
  // 372: one control and the menu button; 264: two; 156: all three.
  assert.deepEqual(states(controls, 372), {
    a: 'overflow',
    b: 'row',
    c: 'row',
  })
  assert.deepEqual(states(controls, 264), {
    a: 'overflow',
    b: 'overflow',
    c: 'row',
  })
  assert.deepEqual(states(controls, 156), {
    a: 'overflow',
    b: 'overflow',
    c: 'overflow',
  })
  // Widening goes back through the same states, c first.
  assert.deepEqual(states(controls, 264), {
    a: 'overflow',
    b: 'overflow',
    c: 'row',
  })
  assert.deepEqual(states(controls, 440), { a: 'row', b: 'row', c: 'row' })
})

test('the menu button is counted once, not once per control, and only while the menu has content', () => {
  const controls = three([1, 2, 3])
  // Three overflowed controls cost one menu button: 16 + 100 + 32 + 8 = 156.
  assert.equal(layoutHeader(controls, FIXED, 156).menuVisible, true)
  assert.equal(layoutHeader(controls, FIXED, 440).menuVisible, false)
  assert.equal(layoutHeader(controls, FIXED, 371).menuVisible, true)
})

test('the order counts across the areas: an end control with order 1 goes before a start control with order 5', () => {
  const controls = [
    control('s', { area: 'start', order: 5 }),
    control('e', { area: 'end', order: 1 }),
  ]
  // 16 + 100 + 200 + 16 = 332 does not fit 300; one overflow and the menu button do (264).
  assert.deepEqual(states(controls, 300), { s: 'row', e: 'overflow' })
})

test('icon first, overflow only when even the icon row would not fit', () => {
  const controls = [
    control('a', { order: 1, form: 'icon', iconWidth: 32 }),
    control('b', { index: 1, order: 2 }),
  ]
  // 16 + 100 + 32 + 100 + 16 = 264: a as icon is enough.
  assert.deepEqual(states(controls, 300), { a: 'icon', b: 'row' })
  // 16 + 100 + 32 + 32 + 16 = 196: a as icon, b in the menu.
  assert.deepEqual(states(controls, 200), { a: 'icon', b: 'overflow' })
  // 190 is too narrow for 196: a gives up its icon form as well (second stage).
  assert.deepEqual(states(controls, 190), { a: 'overflow', b: 'overflow' })
})

test('icon on a control without an icon behaves as overflow', () => {
  const controls = [control('a', { order: 1, form: 'icon', iconWidth: null })]
  // 16 + 100 + 100 + 8 = 224 does not fit 200.
  assert.deepEqual(states(controls, 200), { a: 'overflow' })
})

test('a shrink control gets only as narrow as needed and never below its minimum, then the rest gives way', () => {
  const controls = [
    control('s', {
      area: 'center',
      order: 10,
      form: 'shrink',
      fullWidth: 300,
      minWidth: 120,
    }),
    control('b', { area: 'end', order: 20 }),
  ]
  // 16 + 100 + 300 + 100 + 16 = 532; 52 too wide: s loses 52.
  assert.deepEqual(states(controls, 480), { s: { shrunk: 248 }, b: 'row' })
  // Narrower: s is at its minimum, b goes into the menu (16 + 100 + 120 + 32 + 16 = 284).
  assert.deepEqual(states(controls, 300), { s: { shrunk: 120 }, b: 'overflow' })
  // Narrower still: s goes into the menu too (second stage).
  assert.deepEqual(states(controls, 250), { s: 'overflow', b: 'overflow' })
})

test('only search, breadcrumb and title can shrink, to the sizes of research R7', () => {
  assert.equal(minWidthOf('search'), 120)
  assert.equal(minWidthOf('breadcrumb'), 96)
  assert.equal(minWidthOf('title'), 64)
  assert.equal(minWidthOf('button'), null)
  assert.equal(minWidthOf('choice'), null)
})

test('shrink on a type with no smallest width behaves as overflow', () => {
  const controls = [control('a', { order: 1, form: 'shrink', minWidth: null })]
  assert.deepEqual(states(controls, 200), { a: 'overflow' })
})

test('keep controls collapse after everything else', () => {
  const controls = [
    control('k', { order: 1, form: 'keep' }),
    control('o', { index: 1, order: 2 }),
  ]
  // o has the higher order but k keeps its place: o goes (16 + 100 + 100 + 32 + 16 = 264).
  assert.deepEqual(states(controls, 300), { k: 'row', o: 'overflow' })
})

test('when everything is keep, the keep controls collapse in the order of the places', () => {
  // Place order: the end control first, then the start area from its last control.
  const controls = [
    control('k1', { area: 'start', index: 0, form: 'keep' }),
    control('k2', { area: 'start', index: 1, form: 'keep' }),
    control('e1', { area: 'end', index: 0, form: 'keep' }),
  ]
  assert.deepEqual(states(controls, 400), {
    k1: 'row',
    k2: 'row',
    e1: 'overflow',
  })
  assert.deepEqual(states(controls, 371), {
    k1: 'row',
    k2: 'overflow',
    e1: 'overflow',
  })
})

test('equal orders are broken by place, whatever the order of the array', () => {
  const a = control('a', { index: 0, order: 5 })
  const b = control('b', { index: 1, order: 5 })
  // b is later in its area, so it collapses first.
  assert.deepEqual(states([a, b], 300), { a: 'row', b: 'overflow' })
  assert.deepEqual(states([b, a], 300), { a: 'row', b: 'overflow' })
})

test('a control without an order of its own collapses after every control that has one', () => {
  const controls = [
    control('plain', { index: 0, order: null }),
    control('own', { index: 1, order: 900 }),
  ]
  assert.deepEqual(states(controls, 300), { plain: 'row', own: 'overflow' })
})

test('a window narrower than its own buttons leaves only the buttons and the menu', () => {
  const result = layoutHeader(three([1, 2, 3]), FIXED, 50)
  assert.equal(result.menuVisible, true)
  assert.deepEqual(Object.fromEntries(result.states), {
    a: 'overflow',
    b: 'overflow',
    c: 'overflow',
  })
})

test('a compact window has one window button, so the controls get that much more room', () => {
  const controls = [control('a', { order: 1 }), control('b', { index: 1 })]
  // Normal: 16 + 100 + 200 + 16 = 332 does not fit 300; compact: 16 + 50 + 200 + 16 = 282 does.
  assert.deepEqual(states(controls, 300), { a: 'overflow', b: 'row' })
  assert.deepEqual(states(controls, 300, { ...FIXED, windowButtons: 50 }), {
    a: 'row',
    b: 'row',
  })
})

// ---------------------------------------------------------------------------
// createHeaderSearch: merging input events and keeping what was typed (FR-012, FR-013)
// ---------------------------------------------------------------------------

/** A clock the test moves by hand. */
function fakeClock() {
  let now = 0
  const timers = new Map<number, { at: number; fn: () => void }>()
  let next = 0
  return {
    schedule(fn: () => void, ms: number) {
      const id = next++
      timers.set(id, { at: now + ms, fn })
      return () => {
        timers.delete(id)
      }
    },
    advance(ms: number) {
      now += ms
      for (const [id, timer] of [...timers]) {
        if (timer.at <= now) {
          timers.delete(id)
          timer.fn()
        }
      }
    },
  }
}

function field(initial = '') {
  const clock = fakeClock()
  const inputs: string[] = []
  const submits: string[] = []
  const search = createHeaderSearch(initial, {
    emitInput: (text) => inputs.push(text),
    emitSubmit: (text) => submits.push(text),
    schedule: clock.schedule,
  })
  return { search, clock, inputs, submits }
}

test('the field shows the description’s value at first', () => {
  assert.equal(field('abc').search.text(), 'abc')
})

test('several changes within 100 ms give one event with the latest text', () => {
  const { search, clock, inputs } = field()
  search.type('a')
  clock.advance(30)
  search.type('ab')
  clock.advance(30)
  search.type('abc')
  assert.deepEqual(inputs, [])
  clock.advance(INPUT_MERGE_MS)
  assert.deepEqual(inputs, ['abc'])
  clock.advance(500)
  assert.deepEqual(inputs, ['abc'])
})

test('a change after the window opens a new one', () => {
  const { search, clock, inputs } = field()
  search.type('a')
  clock.advance(INPUT_MERGE_MS)
  search.type('ab')
  clock.advance(INPUT_MERGE_MS)
  assert.deepEqual(inputs, ['a', 'ab'])
})

test('Enter sends the waiting text at once, then submits it', () => {
  const { search, inputs, submits } = field()
  search.type('abc')
  search.enter()
  assert.deepEqual(inputs, ['abc'])
  assert.deepEqual(submits, ['abc'])
})

test('blur sends the waiting text at once', () => {
  const { search, inputs } = field()
  search.focus()
  search.type('abc')
  search.blur()
  assert.deepEqual(inputs, ['abc'])
})

test('a new description sends the waiting text at once, before it is looked at', () => {
  const { search, inputs } = field()
  search.type('abc')
  search.describe('x')
  assert.deepEqual(inputs, ['abc'])
})

test('nothing waits: Enter and blur send no input event, a flush is not sent twice', () => {
  const { search, clock, inputs, submits } = field()
  search.enter()
  assert.deepEqual(inputs, [])
  assert.deepEqual(submits, [''])
  search.type('a')
  search.blur()
  clock.advance(INPUT_MERGE_MS)
  assert.deepEqual(inputs, ['a'])
})

test('an echo of the typed text changes nothing', () => {
  const { search, clock } = field()
  search.focus()
  search.type('abc')
  clock.advance(INPUT_MERGE_MS)
  search.describe('abc')
  assert.equal(search.text(), 'abc')
})

test('an older echo that arrives after the user typed on does not replace the newer text', () => {
  const { search, clock } = field()
  search.focus()
  search.type('a')
  clock.advance(INPUT_MERGE_MS)
  search.type('ab')
  search.describe('a')
  assert.equal(search.text(), 'ab')
})

test('a description with the same value as the previous one changes nothing, whatever is typed', () => {
  const { search } = field('x')
  search.type('xyz')
  search.describe('x')
  assert.equal(search.text(), 'xyz')
})

test('a changed value is applied when the field has no focus', () => {
  const { search } = field('x')
  search.describe('y')
  assert.equal(search.text(), 'y')
})

test('while the field has focus a changed value waits and is applied on blur if it differs from the text', () => {
  const { search } = field('x')
  search.focus()
  search.describe('reset')
  assert.equal(search.text(), 'x')
  search.blur()
  assert.equal(search.text(), 'reset')
})

test('on blur nothing is applied when the received value is what the field holds', () => {
  const { search, clock } = field()
  search.focus()
  search.type('abc')
  clock.advance(INPUT_MERGE_MS)
  search.describe('abc')
  search.blur()
  assert.equal(search.text(), 'abc')
})

test('an app that clears the search (value back to the empty text) clears the field', () => {
  const { search, clock } = field()
  search.type('abc')
  clock.advance(INPUT_MERGE_MS)
  search.describe('abc')
  search.describe('')
  assert.equal(search.text(), '')
})

// ---------------------------------------------------------------------------
// createWmTabHeader: a tab's description and the handler of its events (T005)
// ---------------------------------------------------------------------------

function headerStore(...tabIds: string[]) {
  const runtime = new Map<string, TabRuntime>(
    tabIds.map((id) => [id, newTabRuntime()]),
  )
  return { runtime, ...createWmTabHeader(runtime) }
}

test('a new tab has no header', () => {
  const { headerOf } = headerStore('t1')
  assert.equal(headerOf('t1'), null)
})

test('a description is kept per tab and replaced whole', () => {
  const { headerOf, setTabHeader } = headerStore('t1', 't2')
  setTabHeader('t1', { start: [] })
  setTabHeader('t2', { end: [] })
  assert.deepEqual(headerOf('t1'), { start: [] })
  setTabHeader('t1', { center: [] })
  assert.deepEqual(headerOf('t1'), { center: [] })
  assert.deepEqual(headerOf('t2'), { end: [] })
  setTabHeader('t1', null)
  assert.equal(headerOf('t1'), null)
})

test('the description of a tab that is not shown stays', () => {
  // Nothing in the store knows which tab is active: a background tab keeps what it set.
  const { headerOf, setTabHeader } = headerStore('t1', 't2')
  setTabHeader('t1', { start: [] })
  setTabHeader('t2', { end: [] })
  assert.deepEqual(headerOf('t1'), { start: [] })
})

test('a tab that has closed (its runtime pruned) has no header and setting one does nothing', () => {
  const { runtime, headerOf, setTabHeader } = headerStore('t1')
  setTabHeader('t1', { start: [] })
  runtime.delete('t1')
  assert.equal(headerOf('t1'), null)
  setTabHeader('t1', { end: [] })
  assert.equal(headerOf('t1'), null)
  assert.equal(runtime.has('t1'), false)
})

test('events reach the registered handler, a new handler replaces the old one', () => {
  const { registerTabHeaderHandler, sendHeaderEvent } = headerStore('t1')
  const first: WmHeaderEvent[] = []
  const second: WmHeaderEvent[] = []
  registerTabHeaderHandler('t1', (event) => first.push(event))
  sendHeaderEvent('t1', { id: 'a', kind: 'click' })
  registerTabHeaderHandler('t1', (event) => second.push(event))
  sendHeaderEvent('t1', { id: 'b', kind: 'toggle', value: true })
  assert.deepEqual(first, [{ id: 'a', kind: 'click' }])
  assert.deepEqual(second, [{ id: 'b', kind: 'toggle', value: true }])
})

test('unregistering removes the handler, but not a newer one that replaced it', () => {
  const { registerTabHeaderHandler, sendHeaderEvent } = headerStore('t1')
  const seen: string[] = []
  const stale = registerTabHeaderHandler('t1', () => seen.push('old'))
  registerTabHeaderHandler('t1', () => seen.push('new'))
  stale()
  sendHeaderEvent('t1', { id: 'a', kind: 'click' })
  assert.deepEqual(seen, ['new'])
})

test('an event for a tab without a handler or a closed tab is dropped quietly', () => {
  const { sendHeaderEvent } = headerStore('t1')
  sendHeaderEvent('t1', { id: 'a', kind: 'click' })
  sendHeaderEvent('gone', { id: 'a', kind: 'click' })
})
