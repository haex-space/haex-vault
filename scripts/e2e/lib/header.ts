// Helper of the scenarios that use a window header control (spec 090-app-header-bar). The rig's
// screen is 800 px wide, so a control with an early collapse order can sit in the overflow sidebar
// instead of the row; its `data-testid` is the same in both places. The overflow sidebar is always
// in the page (it is the app sidebar from the other side, hidden by `visibility` while closed), so
// "open" means drawn and visible.
//
// A search is a button of the header; its field (`<testId>-input`) is a popover under the header
// that the button opens, so a scenario types through `typeInHeaderSearch` and friends.
import { clearButtonOf } from './fields.ts'
import type { FlowInstance } from './flows.ts'
import type { WaitContext } from './sync-flows.ts'

type Instance = Pick<FlowInstance, 'exec' | 'click' | 'waitForDisplayed'>
type TypingInstance = Instance & Pick<FlowInstance, 'type'>

const MENU = 'wm-header-overflow-menu'

/** Whether the element `testId` is drawn and visible (a closing sidebar counts until it is gone). */
export const drawn = (instance: Instance, testId: string) =>
  instance.exec<boolean>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return Boolean(el && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden')`,
    [testId],
  )

/** Whether the overflow sidebar of the window header is open. */
export const overflowOpen = (instance: Instance) => drawn(instance, MENU)

/** Whether the overflow toggle says the sidebar is open (its logical state, not what is drawn). */
const overflowExpanded = (instance: Instance) =>
  instance.exec<boolean>(
    `return document.querySelector('[data-testid="wm-header-overflow"]')?.getAttribute('aria-expanded') === 'true'`,
    [],
  )

/** Waits (at most 3 s) until the overflow sidebar is no longer visible, closing animation included. */
const menuGone = (instance: Instance) =>
  instance.exec<boolean>(
    `const menu = '[data-testid="' + arguments[0] + '"]'
     const open = () => {
       const el = document.querySelector(menu)
       return Boolean(el && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden')
     }
     return new Promise((resolve) => {
       const until = Date.now() + 3000
       const step = () =>
         !open() || Date.now() > until ? resolve(true) : requestAnimationFrame(step)
       step()
     })`,
    [MENU],
  )

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

/**
 * Waits (at most `deadlineMs`) until the header has laid itself out far enough to click `testId`:
 * the control is drawn, or the overflow toggle is (the control is behind it), or the sidebar is
 * already open. Right after a window opens or an app's header changes the controls are not there
 * for a moment, and a check made at that instant would take them for collapsed.
 */
async function headerSettled(
  instance: Instance,
  testId: string,
  deadlineMs = 5000,
) {
  const end = Date.now() + deadlineMs
  for (;;) {
    if (
      (await overflowOpen(instance)) ||
      (await drawn(instance, testId)) ||
      (await drawn(instance, 'wm-header-overflow'))
    )
      return
    if (Date.now() >= end) {
      const ids = await instance.exec<string[]>(
        `return [...document.querySelectorAll('[data-testid="wm-header"] [data-testid]')].map((el) => el.dataset.testid)`,
        [],
      )
      throw new Error(
        `neither the header control "${testId}" nor the overflow toggle was drawn within ${deadlineMs} ms (the header holds: ${ids.join(', ') || 'nothing'})`,
      )
    }
    await sleep(50)
  }
}

/**
 * Clicks the header control `testId`, opening the overflow sidebar first when it is not in the row.
 * Over the content the sidebar closes after a click, as a column (a wide window) it stays; this
 * closes it then too, and returns once it is gone, so the next call finds the row.
 */
export async function clickHeaderControl(instance: Instance, testId: string) {
  await headerSettled(instance, testId)
  const inMenu = await overflowOpen(instance)
  if (!inMenu && !(await drawn(instance, testId)))
    await instance.click('wm-header-overflow')
  await instance.click(testId)
  // The toggle's `aria-expanded` is the logical state: a sidebar that closed after the click is
  // still drawn for its slide-out, and clicking the toggle then would open it again.
  if (await overflowExpanded(instance))
    await instance.click('wm-header-overflow')
  await menuGone(instance)
}

/** The field of the header search `testId`: a popover that its button opens. */
const searchField = (testId: string) => `${testId}-input`

/** Escape in the WebDriver's key codes (`KEY.escape` of settings.ts, which this file does not import). */
const ESCAPE = '\uE00C'

/**
 * Whether the search `testId` is open as the app sees it: its button says `aria-expanded`. The
 * field itself is no sign: a field that is closing is still drawn for its fade-out, and one that is
 * about to open is not drawn yet.
 */
const searchExpanded = (instance: Instance, testId: string) =>
  instance.exec<boolean>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]')?.getAttribute('aria-expanded') === 'true'`,
    [testId],
  )

/** Waits (at most `deadlineMs`) until the field `testId` has left the page, its fade-out included. */
async function fieldGone(
  instance: Instance,
  testId: string,
  deadlineMs = 3000,
) {
  const end = Date.now() + deadlineMs
  while (await drawn(instance, searchField(testId))) {
    if (Date.now() >= end) {
      throw new Error(
        `the field of the header search "${testId}" was still drawn ${deadlineMs} ms after it closed`,
      )
    }
    await sleep(50)
  }
}

/**
 * Opens the field of the header search `testId` (its button is in the row or in the sidebar). A
 * field that is still fading out from a close is waited for first: typing into it would lose the
 * text when it goes, and pressing the button then would put a second field beside it.
 */
export async function openHeaderSearch(instance: Instance, testId: string) {
  if (!(await searchExpanded(instance, testId))) {
    await fieldGone(instance, testId)
    await clickHeaderControl(instance, testId)
  }
  await instance.waitForDisplayed(searchField(testId))
}

/**
 * Closes the field of the header search `testId` the way a person does: Escape empties it, a second
 * Escape closes it. The open field lies over the top of the window's content, so a click on a
 * control there is intercepted until it is closed. Returns once it has left the page.
 */
export async function closeHeaderSearch(
  instance: TypingInstance,
  testId: string,
) {
  for (let presses = 0; presses < 2; presses++) {
    if (!(await searchExpanded(instance, testId))) break
    await instance.type(searchField(testId), ESCAPE)
  }
  if (await searchExpanded(instance, testId)) {
    throw new Error(
      `the header search "${testId}" is still open after two presses of Escape`,
    )
  }
  await fieldGone(instance, testId)
}

/** Types into the header search `testId`, opening its field first; the field stays open. */
export async function typeInHeaderSearch(
  instance: TypingInstance,
  testId: string,
  text: string,
) {
  await openHeaderSearch(instance, testId)
  await instance.type(searchField(testId), text)
}

/** Clears the header search `testId` with the clear button of its field. */
export async function clearHeaderSearch(instance: Instance, testId: string) {
  await openHeaderSearch(instance, testId)
  await instance.click(clearButtonOf(searchField(testId)))
}

/** Whether the control `testId` is in the window header's row (not in the overflow menu). */
export const inHeaderRow = (instance: Instance, testId: string) =>
  instance.exec<boolean>(
    `return !!document.querySelector('[data-testid="wm-header"] [data-testid="' + arguments[0] + '"]')`,
    [testId],
  )

/** The width in px of the control `testId` in the header's row; `null` when it is not in the row. */
export const headerRowWidth = (instance: Instance, testId: string) =>
  instance.exec<number | null>(
    `const el = document.querySelector('[data-testid="wm-header"] [data-testid="' + arguments[0] + '"]')
     return el ? el.getBoundingClientRect().width : null`,
    [testId],
  )

/** A header as it was before a resize: the window it belongs to and its width. */
export interface HeaderMark {
  windowId: string
  width: number
}

/**
 * Notes the header of the window that holds the control `testId` (in the row) and its width, to
 * compare with after the window is resized (`waitForNarrowHeader`).
 */
export async function markHeader(
  instance: Instance,
  testId: string,
): Promise<HeaderMark> {
  const mark = await instance.exec<HeaderMark | null>(
    `const control = document.querySelector('[data-testid="wm-header"] [data-testid="' + arguments[0] + '"]')
     const window = control?.closest('[data-wm-window-id]')
     const header = window?.querySelector('[data-testid="wm-header"]')
     return header ? { windowId: window.dataset.wmWindowId, width: header.getBoundingClientRect().width } : null`,
    [testId],
  )
  if (mark === null) {
    throw new Error(`the control "${testId}" is not in a window header's row`)
  }
  return mark
}

/** The width of the header of the window `windowId`, or `null` when it has none. */
const headerWidthOf = (instance: Instance, windowId: string) =>
  instance.exec<number | null>(
    `const header = document.querySelector('[data-wm-window-id="' + arguments[0] + '"] [data-testid="wm-header"]')
     return header ? header.getBoundingClientRect().width : null`,
    [windowId],
  )

/** Resolves after two animation frames: what a resize changed has been laid out and applied. */
const laidOut = (instance: Instance) =>
  instance.exec<boolean>(
    `return new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve(true))))`,
    [],
  )

/**
 * Waits until the header has laid itself out for a narrower window: the header of the marked
 * window is narrower than it was (`markHeader`, read before the resize), and the controls have
 * been placed for that width. The header's own width is the sign: a control cannot be, since a
 * search that is only a button no longer gives way before the others do. A resize returns before
 * the header lays out again, so a check made right after it may still read the wide row.
 */
export async function waitForNarrowHeader(
  ctx: Pick<WaitContext, 'waitFor'>,
  instance: Instance,
  mark: HeaderMark,
): Promise<void> {
  await ctx.waitFor(
    'the header to narrow',
    async () => {
      const width = await headerWidthOf(instance, mark.windowId)
      return width !== null && width < mark.width - 1
    },
    {
      observe: async () => ({
        windowId: mark.windowId,
        wasPx: mark.width,
        nowPx: await headerWidthOf(instance, mark.windowId),
      }),
    },
  )
  await laidOut(instance)
}

/**
 * Whether the control `testId` can be used: in the row, or in the overflow menu, which this opens
 * to look and closes again (spec 093, SC-002). The control itself is not clicked.
 */
export async function headerControlReachable(
  instance: Instance,
  testId: string,
): Promise<boolean> {
  if (await inHeaderRow(instance, testId)) return true
  if (!(await drawn(instance, 'wm-header-overflow'))) return false
  await instance.click('wm-header-overflow')
  await instance.waitForDisplayed(MENU)
  const found = await drawn(instance, testId)
  await instance.click('wm-header-overflow')
  await menuGone(instance)
  return found
}
