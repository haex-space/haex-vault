import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, type FlowInstance } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 044, US4, quickstart §4 (T059): a search with a typo over a tree of 10 000 files shows its
// first hit from the open folder within 1 s and ends within 10 s (SC-004, both measured in the
// page from the input); a type filter narrows the hits and the open folder. That a link loop ends
// is a test of `files/search_tests.rs` (a link needs a platform's own tools here).

/** Types `query` into the search field and measures, in the page, when `hit` shows and when the
 * search ended: the hit through a `MutationObserver` (animation frames are throttled in a window
 * nobody sees, as on the CI's virtual screen), the end by polling. */
const measuredSearch = (instance: FlowInstance, query: string, hit: string) =>
  instance.exec<{ first: number; done: number }>(
    `const input = document.querySelector('[data-testid="files-search"]')
     const hitHook = arguments[1]
     const find = (hook) => document.querySelector('[data-testid="' + hook + '"]')
     const start = performance.now()
     return new Promise((resolve) => {
       let first = -1
       const seen = () => {
         if (first < 0 && find(hitHook)) first = performance.now() - start
       }
       const observer = new MutationObserver(seen)
       observer.observe(document.body, { childList: true, subtree: true, attributes: true })
       const poll = () => {
         seen()
         if (first >= 0 && !find('files-search-running')) {
           observer.disconnect()
           resolve({ first, done: performance.now() - start })
         } else if (performance.now() - start > 20000) {
           observer.disconnect()
           resolve({ first, done: -1 })
         } else {
           setTimeout(poll, 20)
         }
       }
       input.value = arguments[0]
       input.dispatchEvent(new Event('input', { bubbles: true }))
       setTimeout(poll, 20)
     })`,
    [query, hit],
  )

const hitNames = (instance: FlowInstance) =>
  instance.exec<string[]>(
    `return [...document.querySelectorAll('[data-testid^="files-hit-"]:not([data-testid^="files-hit-reveal-"])')]
       .map((el) => el.getAttribute('data-testid').slice('files-hit-'.length)).sort()`,
  )

const entryNames = (instance: FlowInstance) =>
  instance.exec<string[]>(
    `return [...document.querySelectorAll('[data-testid^="files-entry-"]')]
       .map((el) => el.getAttribute('data-testid').slice('files-entry-'.length)).sort()`,
  )

const closeMenu = (instance: FlowInstance) =>
  instance.exec(
    `document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
     return true`,
  )

scenario('files-search', { timeoutMs: 300_000 }, async (ctx) => {
  const files = deviceFiles('haex-search-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-search' })

    // After the start: on Android it clears the app's data, where the folder lives.
    files.write('urlaub-foto.jpg', 'x')
    files.write('urlaub-notizen.txt', 'x')
    files.tree('baum', 100, 100)
    ctx.step('a tree of 10 000 files')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.folder)}`,
    })
    await instance.waitForDisplayed('files-entry-baum')

    // A typo still finds both; the first hit from the open folder within 1 s, all within 10 s.
    const timing = await measuredSearch(
      instance,
      'urlab',
      'files-hit-urlaub-foto.jpg',
    )
    assert.ok(timing.first >= 0, 'no hit')
    assert.ok(timing.first < 1000, `first hit after ${timing.first} ms`)
    assert.ok(
      timing.done >= 0 && timing.done < 10_000,
      `done after ${timing.done} ms`,
    )
    assert.deepEqual(await hitNames(instance), [
      'urlaub-foto.jpg',
      'urlaub-notizen.txt',
    ])
    ctx.step(`typo search: first hit ${Math.round(timing.first)} ms`)

    // A type filter narrows the hits…
    await instance.click('files-filter')
    await instance.click('files-filter-type-image')
    await closeMenu(instance)
    await ctx.waitFor('only the image to remain', async () => {
      const names = await hitNames(instance)
      return names.length === 1 && names[0] === 'urlaub-foto.jpg'
    })
    ctx.step('type filter on hits')

    // …and the open folder, where folders drop out under a type filter.
    await instance.click('files-search-clear')
    await instance.waitForDisplayed('files-entry-urlaub-foto.jpg')
    await ctx.waitFor('the folder filtered', async () => {
      const names = await entryNames(instance)
      return names.length === 1 && names[0] === 'urlaub-foto.jpg'
    })
    ctx.step('type filter on the folder')
  } finally {
    files.remove()
  }
})
