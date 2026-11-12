// Helper of the scenarios that use a window header control (spec 090-app-header-bar). The rig's
// screen is 800 px wide, so a control with an early collapse order can sit in the overflow menu
// instead of the row; its `data-testid` is the same in both places.
import type { FlowInstance } from './flows.ts'

type Instance = Pick<FlowInstance, 'exec' | 'click'>

/** Clicks the header control `testId`, opening the overflow menu first when it is not in the row. */
export async function clickHeaderControl(instance: Instance, testId: string) {
  const shown = await instance.exec<boolean>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return Boolean(el && el.getClientRects().length > 0)`,
    [testId],
  )
  if (!shown) await instance.click('wm-header-overflow')
  await instance.click(testId)
}
