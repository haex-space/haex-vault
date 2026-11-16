import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import {
  clickHeaderControl,
  markHeader,
  waitForNarrowHeader,
} from './header.ts'
import { pollingWaitFor } from './group.testlib.ts'

type Exec = (script: string, args?: unknown[]) => Promise<unknown>

/** An instance whose page answers by what the script asks for. */
function page(exec: Exec) {
  const clicks: string[] = []
  return {
    clicks,
    exec: exec as <T>(script: string, args?: unknown[]) => Promise<T>,
    click: async (hook: string) => {
      clicks.push(hook)
    },
    waitForDisplayed: async () => {},
  }
}

describe('marking a header', () => {
  it('names the window and its width', async () => {
    const instance = page(async () => ({ windowId: 'w1', width: 700 }))
    assert.deepEqual(await markHeader(instance, 'contacts-search'), {
      windowId: 'w1',
      width: 700,
    })
  })

  it('says which control it did not find in a header', async () => {
    const instance = page(async () => null)
    await assert.rejects(
      markHeader(instance, 'contacts-search'),
      /"contacts-search" is not in a window header's row/,
    )
  })
})

describe('waiting for a narrow header', () => {
  const mark = { windowId: 'w1', width: 700 }
  const ctx = { waitFor: pollingWaitFor }

  it('waits for the header of that window to be narrower, then for the layout', async () => {
    const widths = [700, 700, 360]
    let frames = 0
    const instance = page(async (script) => {
      if (script.includes('requestAnimationFrame')) {
        frames += 1
        return true
      }
      return widths.length > 1 ? widths.shift() : widths[0]
    })
    await waitForNarrowHeader(ctx, instance, mark)
    assert.equal(frames, 1)
  })

  it('names the widths when the header never narrows', async () => {
    const instance = page(async () => 700)
    await assert.rejects(
      waitForNarrowHeader(ctx, instance, mark),
      /the header to narrow/,
    )
  })
})

describe('clicking a header control', () => {
  it('waits for the header to be laid out instead of taking a control that is not there yet for collapsed', async () => {
    let asked = 0
    const instance = page(async (script, args) => {
      const hook = String(args?.[0] ?? '')
      if (script.includes('aria-expanded')) return false
      if (script.includes('requestAnimationFrame')) return true
      if (hook === 'wm-header-overflow-menu') return false
      // The control is drawn from the third look on; the overflow toggle never is.
      if (hook === 'wm-header-q') return ++asked >= 3
      return false
    })
    await clickHeaderControl(instance, 'wm-header-q')
    assert.deepEqual(instance.clicks, ['wm-header-q'])
  })

  it('says what the header held when nothing shows up', async () => {
    const instance = page(async (script) =>
      script.includes('querySelectorAll') ? ['wm-header-x'] : false,
    )
    await assert.rejects(
      clickHeaderControl(instance, 'wm-header-q'),
      /neither the header control "wm-header-q" nor the overflow toggle .*\(the header holds: wm-header-x/,
    )
  })
})
