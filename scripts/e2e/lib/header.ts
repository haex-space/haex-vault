// Helper of the scenarios that use a window header control (spec 090-app-header-bar). The rig's
// screen is 800 px wide, so a control with an early collapse order can sit in the overflow menu
// instead of the row; its `data-testid` is the same in both places.
import type { FlowInstance } from './flows.ts'
import type { WaitContext } from './sync-flows.ts'

type Instance = Pick<FlowInstance, 'exec' | 'click' | 'waitForDisplayed'>

const MENU = 'wm-header-overflow-menu'

/** Whether the element `testId` is drawn (the closing menu still counts until it is gone). */
const drawn = (instance: Instance, testId: string) =>
  instance.exec<boolean>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return Boolean(el && el.getClientRects().length > 0)`,
    [testId],
  )

/** Waits (at most 3 s) until the overflow menu is no longer drawn, closing animation included. */
const menuGone = (instance: Instance) =>
  instance.exec<boolean>(
    `const menu = '[data-testid="' + arguments[0] + '"]'
     return new Promise((resolve) => {
       const until = Date.now() + 3000
       const step = () =>
         !document.querySelector(menu) || Date.now() > until
           ? resolve(true)
           : requestAnimationFrame(step)
       step()
     })`,
    [MENU],
  )

/**
 * Clicks the header control `testId`, opening the overflow menu first when it is not in the row.
 * The menu closes after a click; this returns once it is gone, so the next call finds the row.
 */
export async function clickHeaderControl(instance: Instance, testId: string) {
  const inMenu = await drawn(instance, MENU)
  if (!inMenu && !(await drawn(instance, testId)))
    await instance.click('wm-header-overflow')
  await instance.click(testId)
  await menuGone(instance)
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

/**
 * Waits until the header has laid itself out for a narrower window: the search field `testId`,
 * which gives way first, is narrower than `wideWidth` (its width in the wide window, read with
 * `headerRowWidth` before the resize) or has left the row. A resize returns before the header
 * lays out again, so a check made right after it may still read the wide row.
 */
export async function waitForNarrowHeader(
  ctx: Pick<WaitContext, 'waitFor'>,
  instance: Instance,
  testId: string,
  wideWidth: number,
): Promise<void> {
  await ctx.waitFor('the header to narrow', async () => {
    const width = await headerRowWidth(instance, testId)
    return width === null || width < wideWidth - 1
  })
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
  await instance.click('wm-header-overflow-close')
  await menuGone(instance)
  return found
}
