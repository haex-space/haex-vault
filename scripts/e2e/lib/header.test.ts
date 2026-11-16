import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import {
  clickHeaderControl,
  closeHeaderSearch,
  markHeader,
  openHeaderSearch,
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

describe('the field of a header search', () => {
  /**
   * A search whose state the test sets: `expanded` is the button's `aria-expanded`, `drawn` what is
   * on the page (the button, the field and the fade-out of a field that is closing).
   */
  function search(initial: { expanded: boolean; fieldDrawn: number }) {
    const state = { ...initial }
    const typed: string[] = []
    const base = page(async (script, args) => {
      const hook = String(args?.[0] ?? '')
      // The overflow toggle is not the search's button; only the latter is read by its argument.
      if (script.includes('aria-expanded'))
        return script.includes('wm-header-overflow') ? false : state.expanded
      if (hook === 'wm-header-q-input') {
        if (state.fieldDrawn > 0) return state.fieldDrawn-- > 0
        return false
      }
      if (hook === 'wm-header-q') return true
      return false
    })
    const instance = {
      ...base,
      // The button toggles its field, as in the app.
      click: async (hook: string) => {
        base.clicks.push(hook)
        if (hook === 'wm-header-q') state.expanded = !state.expanded
      },
      type: async (hook: string, text: string) => {
        typed.push(`${hook}:${text}`)
        // Escape empties, a second one closes (the first leaves it open).
        if (state.expanded) state.expanded = typed.length < 2
      },
    }
    return { state, typed, instance }
  }

  it('does not press the button of a field that is already open', async () => {
    const { instance } = search({ expanded: true, fieldDrawn: 0 })
    await openHeaderSearch(instance, 'wm-header-q')
    assert.deepEqual(instance.clicks, [])
  })

  it('waits for a field that is still fading out before it opens it again', async () => {
    const { instance } = search({ expanded: false, fieldDrawn: 3 })
    await openHeaderSearch(instance, 'wm-header-q')
    assert.deepEqual(instance.clicks, ['wm-header-q'])
  })

  it('closes with a second Escape and says so when it stays open', async () => {
    const { instance, typed } = search({ expanded: true, fieldDrawn: 0 })
    await closeHeaderSearch(instance, 'wm-header-q')
    assert.deepEqual(typed, [
      'wm-header-q-input:\uE00C',
      'wm-header-q-input:\uE00C',
    ])

    const stuck = search({ expanded: true, fieldDrawn: 0 })
    stuck.instance.type = async () => {}
    await assert.rejects(
      closeHeaderSearch(stuck.instance, 'wm-header-q'),
      /still open after two presses of Escape/,
    )
  })
})
