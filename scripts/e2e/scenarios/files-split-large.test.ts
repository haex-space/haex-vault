import assert from 'node:assert/strict'

import { createAndUnlock } from '../lib/flows.ts'
import { entryIn, inPanel, visiblePanels } from '../lib/files-split.ts'
import { doubleClick } from '../lib/passwords.ts'
import { deviceFiles } from '../lib/platform/device-files.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 062 SC-006 (T065): while one panel shows a folder of 50 000 entries, the other stays
// responsive: a selection and a navigation in it show within 200 ms, measured in the page from the
// click to the change. The CI runners draw in software and share their cores, so, as in
// `files-large-folder`, a single measurement may be up to twice the limit. Desktop only.

const LIMIT_MS = 200
const HARD_FACTOR = 2

/** The time from the next `event` on the page to `done()` holding, measured in the page. */
const measure = (
  instance: { exec: <T>(script: string, args?: unknown[]) => Promise<T> },
  event: 'click' | 'dblclick',
  done: string,
) =>
  instance.exec<boolean>(
    `window.__holziMeasured = new Promise((resolve) => {
       let start = 0
       const ready = () => start > 0 && (${done})
       document.addEventListener(arguments[0], () => (start = performance.now()), {
         capture: true,
         once: true,
       })
       const observer = new MutationObserver(() => {
         if (!ready()) return
         observer.disconnect()
         resolve(performance.now() - start)
       })
       observer.observe(document.body, { childList: true, subtree: true, attributes: true })
     })
     return true`,
    [event],
  )

scenario('files-split-large', { timeoutMs: 300_000 }, async (ctx) => {
  const files = deviceFiles('holzi-split-large-', { E2E_PLATFORM: 'linux' })
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-split-large' })
    files.tree('riesig', 1, 50_000)
    files.write('klein/a.txt', 'a')
    files.write('klein/sub/x.txt', 'x')
    ctx.step('a folder of 50 000 files and a small one written')

    const at = new URLSearchParams({
      p: `${files.folder}/riesig/d0`,
      split: '1',
      'o.src': 'device',
      'o.p': files.path('klein'),
    })
    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?${at}`,
    })
    await instance.waitForDisplayed('files-sidebar-toggle')
    await instance.click('files-sidebar-toggle')
    await ctx.waitFor(
      'two panels',
      async () => (await visiblePanels(instance)) === 2,
      { fixed: true, timeoutMs: 15_000 },
    )
    await ctx.waitFor(
      'the left list of 50 000 entries',
      () =>
        instance.exec<boolean>(
          `const list = document.querySelector('[data-files-panel="left"] [data-testid="files-entries"]')
           const row = list?.querySelector('[data-testid^="files-entry-"]')
           const height = row?.getBoundingClientRect().height ?? 0
           return height > 0 && list.scrollHeight / height >= 50000`,
        ),
      { fixed: true, timeoutMs: 60_000 },
    )
    await instance.waitForDisplayed(entryIn('right', 'a.txt'))

    // A selection in the small panel.
    await measure(
      instance,
      'click',
      `document.querySelector('[data-files-panel="right"] [data-testid="files-entry-a.txt"]')?.getAttribute('aria-selected') === 'true'`,
    )
    await instance.click(entryIn('right', 'a.txt'))
    const selectedMs = Math.round(
      await instance.exec<number>('return window.__holziMeasured'),
    )
    assert.ok(
      selectedMs <= LIMIT_MS * HARD_FACTOR,
      `a selection took ${selectedMs} ms beside 50 000 entries`,
    )
    ctx.step('a selection beside 50 000 entries', `${selectedMs} ms`)

    // A navigation in the small panel.
    await measure(
      instance,
      'dblclick',
      `document.querySelector('[data-files-panel="right"] [data-testid="files-entry-x.txt"]')`,
    )
    await doubleClick(instance, entryIn('right', 'sub'))
    const navigatedMs = Math.round(
      await instance.exec<number>('return window.__holziMeasured'),
    )
    assert.ok(
      navigatedMs <= LIMIT_MS * HARD_FACTOR,
      `a navigation took ${navigatedMs} ms beside 50 000 entries`,
    )
    assert.ok(
      await instance.exec<boolean>(
        `return Boolean(document.querySelector(arguments[0]))`,
        [inPanel('left', 'files-entries')],
      ),
      'the large panel stays',
    )
    ctx.step('a navigation beside 50 000 entries', `${navigatedMs} ms`)
  } finally {
    files.remove()
  }
})
