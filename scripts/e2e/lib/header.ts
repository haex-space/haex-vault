// Helper of the scenarios that use a window header control (spec 090-app-header-bar). The rig's
// screen is 800 px wide, so a control with an early collapse order can sit in the overflow menu
// instead of the row; its `data-testid` is the same in both places.
import type { FlowInstance } from './flows.ts'

type Instance = Pick<FlowInstance, 'exec' | 'click'>

const MENU = 'wm-header-overflow-menu'

/** Whether the element `testId` is drawn (the closing menu still counts until it is gone). */
const drawn = (instance: Instance, testId: string) =>
  instance.exec<boolean>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return Boolean(el && el.getClientRects().length > 0)`,
    [testId],
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
  await instance.exec<boolean>(
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
}
