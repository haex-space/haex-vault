import assert from 'node:assert/strict'

import { createAndUnlock } from '../lib/flows.ts'
import { doubleClick } from '../lib/passwords.ts'
import { deviceFiles } from '../lib/platform/device-files.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { filesGo } from '../lib/storage-setup.ts'

// Spec 044 SC-001 and FR-008 (T035): a folder of 1 000 entries on the device is shown in under one
// second, measured in the page from the double click that opens it to its first row; one of
// 50 000 entries scrolls to its end with no frame longer than 100 ms, measured with
// `requestAnimationFrame` deltas while the list jumps ten screens per frame (on a phone, where CI
// runs a software-drawn emulator, 250 ms).

const SHOWN_LIMIT_MS = 1_000
const FRAME_LIMIT_MS = 100
/**
 * The frame limit on a phone. The CI's emulator draws in software (swiftshader) on four shared
 * cores: the longest frame was 117 ms there and 50–67 ms on a local emulator with the same setup.
 * A list that stopped drawing only its visible rows would take seconds per frame, so this still
 * catches it; the step names the longest frame either way.
 */
const PHONE_FRAME_LIMIT_MS = 250

type Scroll = { worst: number; slow: number; frames: number; end: boolean }

scenario('files-large-folder', { timeoutMs: 300_000 }, async (ctx) => {
  const files = deviceFiles('holzi-files-large-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-large' })
    const frameLimitMs =
      instance.phone === undefined ? FRAME_LIMIT_MS : PHONE_FRAME_LIMIT_MS

    // After the start: on Android it clears the app's data, where the folder lives.
    files.tree('tausend', 1, 1_000)
    files.tree('riesig', 1, 50_000)
    ctx.step('folders of 1 000 and 50 000 files written')

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(`${files.folder}/tausend`)}`,
    })
    await instance.waitForDisplayed('files-entry-d0', 20_000)
    // The clock starts with the double click and stops when the first row of the folder is there.
    await instance.exec(
      `window.__holziShown = new Promise((resolve) => {
         let start = 0
         const shown = () =>
           start > 0 && document.querySelector('[data-testid="files-entry-f0-0.txt"]')
         document.addEventListener('dblclick', () => (start = performance.now()), {
           capture: true,
           once: true,
         })
         const observer = new MutationObserver(() => {
           if (!shown()) return
           observer.disconnect()
           resolve(performance.now() - start)
         })
         observer.observe(document.body, { childList: true, subtree: true })
       })
       return true`,
    )
    await doubleClick(instance, 'files-entry-d0')
    const shownMs = Math.round(
      await instance.exec<number>('return window.__holziShown'),
    )
    assert.ok(
      shownMs < SHOWN_LIMIT_MS,
      `1 000 entries took ${shownMs} ms to show`,
    )
    ctx.step('1 000 entries shown', `${shownMs} ms`)

    await filesGo(
      instance,
      `/device?p=${encodeURIComponent(`${files.folder}/riesig/d0`)}`,
    )
    // The rows of the 1 000 folder share their names: wait until the list is as tall as 50 000 rows.
    const rows = await ctx.waitFor(
      'the list of 50 000 entries',
      () =>
        instance.exec<number | false>(
          `const list = document.querySelector('[data-testid="files-entries"]')
           const row = list?.querySelector('[data-testid^="files-entry-"]')
           const height = row?.getBoundingClientRect().height ?? 0
           const rows = height > 0 ? Math.round(list.scrollHeight / height) : 0
           return rows >= 50000 ? rows : false`,
        ),
      { fixed: true, timeoutMs: 60_000 },
    )
    const scroll = await instance.exec<Scroll>(
      `const list = document.querySelector('[data-testid="files-entries"]')
       return new Promise((resolve) => {
         let last = 0
         let worst = 0
         let slow = 0
         let frames = 0
         const step = (now) => {
           if (last > 0) {
             worst = Math.max(worst, now - last)
             if (now - last > ${frameLimitMs}) slow += 1
           }
           last = now
           frames += 1
           if (list.scrollTop + list.clientHeight >= list.scrollHeight - 1) {
             resolve({ worst, slow, frames, end: true })
             return
           }
           if (frames > 5000) {
             resolve({ worst, slow, frames, end: false })
             return
           }
           list.scrollTop += list.clientHeight * 10
           requestAnimationFrame(step)
         }
         list.scrollTop = 0
         requestAnimationFrame(step)
       })`,
    )
    const worst = Math.round(scroll.worst)
    assert.ok(
      scroll.end,
      `the list did not reach its end: ${JSON.stringify(scroll)}`,
    )
    assert.ok(
      worst <= frameLimitMs,
      `${scroll.slow} of ${scroll.frames} frames took over ${frameLimitMs} ms while scrolling 50 000 entries, the longest ${worst} ms`,
    )
    ctx.step(
      '50 000 entries scrolled to the end',
      `${rows} rows, ${scroll.frames} frames, longest ${worst} ms`,
    )
  } finally {
    files.remove()
  }
})
