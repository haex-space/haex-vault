import assert from 'node:assert/strict'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import type { FlowInstance } from '../lib/flows.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  isShown,
  KEY,
  openSettings,
  resizeSettingsWindow,
  wmSnapshot,
} from '../lib/settings.ts'

// Spec 090-app-header-bar, US1, US2 and the quickstart: the header of a window is one row that
// belongs to the app (Settings is the pilot), it collapses as the app ordered when the window gets
// narrow, and it stays reachable by keyboard. The rig has no pointer actions, so dragging the window
// by an empty part of the bar is a hand check (tasks T043); here the bar is shown to have an empty
// part and the double click maximizes.

/** The header's own elements that take part in the row: not the hidden row that measures. */
const HEADER_ITEMS = `
  const header = document.querySelector('[data-testid="wm-header"]')
  const items = [...header.querySelectorAll('button, input')].filter(
    (el) => !el.closest('[inert]'),
  )`

interface RowState {
  header: { left: number; right: number; top: number; bottom: number }
  boxes: Array<{ id: string; left: number; right: number }>
  rowIds: string[]
  hasMenu: boolean
}

/** What the header looks like right now: its box, the box of every control in it, which controls
 * are in the row and whether the overflow button shows. */
const readRow = (instance: FlowInstance) =>
  instance.exec<RowState>(`
    ${HEADER_ITEMS}
    const rect = header.getBoundingClientRect()
    return {
      header: { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom },
      boxes: items.map((el) => {
        const box = el.getBoundingClientRect()
        return { id: el.dataset.testid ?? el.getAttribute('aria-label') ?? el.tagName, left: box.left, right: box.right }
      }),
      rowIds: [...header.querySelectorAll('[data-header-control]')]
        .filter((el) => !el.closest('[inert]'))
        .map((el) => el.dataset.headerControl),
      hasMenu: !!header.querySelector('[data-testid="wm-header-overflow"]'),
    }`)

/** Every control of the description is either in the row or in the menu; the menu is opened to see. */
async function assertReachable(
  instance: FlowInstance & { typeToFocused(text: string): Promise<void> },
  ctx: ScenarioContext,
  state: RowState,
  ids: string[],
) {
  const missing = ids.filter((id) => !state.rowIds.includes(id))
  if (missing.length === 0) return
  assert.ok(
    state.hasMenu,
    `${missing.join(', ')} left the row but there is no menu`,
  )
  await instance.click('wm-header-overflow')
  const inMenu = await ctx.waitFor('the overflow menu', async () => {
    const found = await instance.exec<boolean>(
      `return !!document.querySelector('[data-testid="wm-header-overflow-menu"]')`,
    )
    return found ? true : null
  })
  assert.ok(inMenu)
  for (const id of missing) {
    const shown = await isShown(
      instance,
      `[data-testid="wm-header-overflow-menu"] [data-testid="${id === 'sidebar' ? 'settings-sidebar-toggle' : `wm-header-${id}`}"]`,
    )
    assert.ok(shown, `control "${id}" is neither in the row nor in the menu`)
  }
  await instance.typeToFocused(KEY.escape)
}

scenario('window-header', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-window-header' })
  await openSettings(instance)
  const [settingsWindow] = (await wmSnapshot(instance)).windows
  assert.ok(settingsWindow, 'no window after opening settings')

  // --- One bar, no row under it (FR-005, SC-001) -----------------------------------------------
  const geometry = await instance.exec<{
    height: number
    rem: number
    gap: number
    label: string | null
    role: string | null
  }>(`
    const header = document.querySelector('[data-testid="wm-header"]')
    const panel = document.querySelector('[role="tabpanel"]:not([aria-hidden="true"])')
    const rect = header.getBoundingClientRect()
    return {
      height: rect.height,
      rem: parseFloat(getComputedStyle(document.documentElement).fontSize),
      gap: panel.getBoundingClientRect().top - rect.bottom,
      label: header.getAttribute('aria-label'),
      role: header.getAttribute('role'),
    }`)
  // The old title row: `pt-1` + `h-8` at the theme's spacing of 0.275 rem, plus its 1 px border
  // (40.6 px at a 16 px root); the header (`h-9`, border included) must not be higher.
  const oldRow = 9 * 0.275 * geometry.rem + 1
  assert.ok(
    geometry.height <= oldRow + 0.5,
    `the header is ${geometry.height} px high, the old title row was ${oldRow} px (root ${geometry.rem} px)`,
  )
  assert.ok(
    Math.abs(geometry.gap) <= 1,
    `the content starts ${geometry.gap} px below the header`,
  )
  const noRowBelow = await instance.exec<boolean>(
    `return !document.querySelector('[data-testid="settings-search-open"], [data-testid="settings-toolbar"]')`,
  )
  assert.ok(noRowBelow, 'Settings still has a row of its own below the header')
  assert.equal(geometry.role, 'toolbar')
  assert.ok(geometry.label, 'the toolbar has no label')
  ctx.step(
    'one bar no higher than the old title row, content directly below, a labelled toolbar',
  )

  // --- The empty part of the bar and the double click ------------------------------------------
  await instance.waitForDisplayed('wm-header-q')
  const emptyPart = await instance.exec<boolean>(`
    ${HEADER_ITEMS}
    const rect = header.getBoundingClientRect()
    const boxes = items.map((el) => el.getBoundingClientRect())
    const free = boxes.length === 0 || boxes.some((_, i) => {
      const edge = i === 0 ? rect.left : boxes[i - 1].right
      return boxes[i].left - edge > 8
    }) || rect.right - Math.max(...boxes.map((b) => b.right)) > 8
    return free`)
  assert.ok(emptyPart, 'the bar has no empty part to drag it by')
  await instance.exec(
    `document.querySelector('[data-testid="wm-header"]').dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, detail: 2 })); return true`,
  )
  await ctx.waitFor('the window to change its maximized state', async () => {
    const now = (await wmSnapshot(instance)).windows.find(
      (w) => w.id === settingsWindow.id,
    )
    return now && now.maximized !== settingsWindow.maximized ? true : null
  })
  await instance.exec(
    `document.querySelector('[data-testid="wm-header"]').dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, detail: 2 })); return true`,
  )
  await ctx.waitFor('the window to be restored', async () => {
    const now = (await wmSnapshot(instance)).windows.find(
      (w) => w.id === settingsWindow.id,
    )
    return now && now.maximized === settingsWindow.maximized ? true : null
  })
  ctx.step('an empty part exists; a double click maximizes and restores')

  // --- The width sweep (SC-002, FR-014, FR-016, FR-017) -----------------------------------------
  const WIDTHS = [460, 500, 540, 580, 620, 680, 760, 860, 980, 1100]
  const ids = ['sidebar', 'q']
  const seenGoingNarrower = new Map<number, string>()
  for (const width of [...WIDTHS].reverse()) {
    await resizeSettingsWindow(instance, width)
    // Settled when the header is as wide as the window the store holds (less its two border
    // pixels), so the row read next is the one for this width, not the previous one.
    await ctx.waitFor(`the header to settle at ${width}`, async () => {
      const stored = (await wmSnapshot(instance)).windows.find(
        (w) => w.id === settingsWindow.id,
      )
      const state = await readRow(instance)
      const drawn = state.header.right - state.header.left
      return stored && Math.abs(drawn - stored.width) <= 4 ? true : null
    })
    const state = await readRow(instance)
    for (const box of state.boxes) {
      assert.ok(
        box.left >= state.header.left - 1 &&
          box.right <= state.header.right + 1,
        `at ${width}: ${box.id} leaves the header (${box.left}-${box.right} of ${state.header.left}-${state.header.right})`,
      )
    }
    const sorted = [...state.boxes].sort((a, b) => a.left - b.left)
    for (let i = 1; i < sorted.length; i += 1) {
      assert.ok(
        sorted[i]!.left >= sorted[i - 1]!.right - 1,
        `at ${width}: ${sorted[i - 1]!.id} and ${sorted[i]!.id} overlap`,
      )
    }
    assert.ok(
      state.boxes.some((b) => /Fenster|window/i.test(b.id)),
      `at ${width}: the window buttons are not visible`,
    )
    await assertReachable(instance, ctx, state, ids)
    seenGoingNarrower.set(width, JSON.stringify(state.rowIds))
  }
  ctx.step('narrowing: nothing clipped or overlapping, every control reachable')

  // Widening gives the same controls back at the same width.
  for (const width of WIDTHS) {
    await resizeSettingsWindow(instance, width)
    await ctx.waitFor(`the row at ${width} to match`, async () => {
      const state = await readRow(instance)
      return JSON.stringify(state.rowIds) === seenGoingNarrower.get(width)
        ? true
        : null
    })
  }
  ctx.step('widening restores the same row at every width')

  // A control that stays in the row keeps its focus while others collapse (FR-016).
  await resizeSettingsWindow(instance, 1100)
  await instance.exec(
    `document.querySelector('[data-testid="settings-sidebar-toggle"]').focus(); return true`,
  )
  await resizeSettingsWindow(instance, 540)
  const kept = await instance.exec<boolean>(
    `return document.activeElement === document.querySelector('[data-testid="settings-sidebar-toggle"]')`,
  )
  assert.ok(
    kept,
    'the sidebar button lost its focus while the search collapsed',
  )
  ctx.step('a control that stays keeps its focus')

  // --- Right to left (spec Edge Cases, layout direction) ----------------------------------------
  await instance.exec(`document.documentElement.dir = 'rtl'; return true`)
  for (const width of [620, 860, 1100]) {
    await resizeSettingsWindow(instance, width)
    const mirrored = await instance.exec<boolean | null>(`
      const start = document.querySelector('[data-testid="wm-header"] [data-area="start"]')
      const end = document.querySelector('[data-testid="wm-header"] [data-area="end"]')
      if (!start) return null
      const startBox = start.getBoundingClientRect()
      const middle = document.querySelector('[data-testid="wm-header"] [data-area="center"]').getBoundingClientRect()
      return startBox.left >= middle.left - 1 && (!end || end.getBoundingClientRect().left <= startBox.left)`)
    assert.ok(
      mirrored !== false,
      `at ${width}: the areas did not mirror under dir="rtl"`,
    )
  }
  await instance.exec(`document.documentElement.dir = 'ltr'; return true`)
  ctx.step('the areas mirror in a right-to-left layout')

  // --- Keyboard (FR-006, SC-005) -------------------------------------------------------------
  await resizeSettingsWindow(instance, 1100)
  await instance.exec(
    `document.querySelector('[data-testid="settings-sidebar-toggle"]').focus(); return true`,
  )
  const areas = ['start']
  for (let i = 0; i < 6; i += 1) {
    await instance.typeToFocused(KEY.tab)
    const area = await instance.exec<string | null>(`
      const header = document.querySelector('[data-testid="wm-header"]')
      const active = document.activeElement
      if (!header.contains(active)) return null
      return active.closest('[data-area]')?.dataset.area ?? 'window'`)
    if (area === null) break
    areas.push(area)
  }
  const order = ['start', 'center', 'end', 'window']
  const indexes = areas.map((a) => order.indexOf(a))
  assert.ok(
    indexes.every((v, i) => i === 0 || v >= indexes[i - 1]!),
    `Tab walks the header out of order: ${areas.join(' → ')}`,
  )
  assert.ok(
    areas.includes('window'),
    `Tab did not reach the window buttons: ${areas.join(' → ')}`,
  )
  ctx.step('Tab walks start → centre → end → window buttons')

  await resizeSettingsWindow(instance, 460)
  await ctx.waitFor('the overflow button', async () => {
    const state = await readRow(instance)
    return state.hasMenu ? true : null
  })
  await instance.exec(
    `document.querySelector('[data-testid="wm-header-overflow"]').focus(); return true`,
  )
  await instance.typeToFocused(KEY.enter)
  await instance.waitForDisplayed('wm-header-overflow-menu')
  await instance.typeToFocused(KEY.escape)
  await ctx.waitFor('the menu to close and the focus to return', async () => {
    const back = await instance.exec<boolean>(
      `return !document.querySelector('[data-testid="wm-header-overflow-menu"]') && document.activeElement === document.querySelector('[data-testid="wm-header-overflow"]')`,
    )
    return back ? true : null
  })
  ctx.step('the overflow menu opens and closes by keyboard and returns focus')

  // --- The search, as in spec 023 FR-023 --------------------------------------------------------
  await resizeSettingsWindow(instance, 1100)
  await instance.type('wm-header-q', 'parakeet')
  await instance.waitForDisplayed(
    '#settings-sidebar [data-testid="settings-search-hit"]',
  )
  await instance.type('wm-header-q', KEY.escape)
  const emptied = await instance.exec<string>(
    `return document.querySelector('[data-testid="wm-header-q"]').value`,
  )
  assert.equal(emptied, '', 'Escape did not empty the search')
  ctx.step('the search finds hits and Escape empties it')
})
