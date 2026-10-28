import assert from 'node:assert/strict'

import {
  activePanel,
  entryIn,
  inPanel,
  panelShows,
  visiblePanels,
} from '../lib/files-split.ts'
import { doubleClick } from '../lib/passwords.ts'
import { deviceFiles } from '../lib/platform/device-files.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction, setSessionRestore, wmSnapshot } from '../lib/settings.ts'

// Spec 062 US4 (T055, quickstart M10, M11): the split with two places comes back after a restart
// (spec 022 session restore) with both places and the active side; Back goes back on the active
// panel; the other panel starts with an empty history; off and on again keeps both places.
// Desktop only, like `files-split`.

scenario('files-split-restore', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const device = group.device('anna/laptop')
  const page = device.page
  const files = deviceFiles('holzi-split-restore-', { E2E_PLATFORM: 'linux' })
  ctx.onTeardown(() => files.remove())
  files.write('a/a.txt', 'a')
  files.write('b/b.txt', 'b')

  const hideSidebar = async (instance: typeof page) => {
    await instance.waitForDisplayed('files-sidebar-toggle')
    await instance.click('files-sidebar-toggle')
    await ctx.waitFor(
      'two panels',
      async () => (await visiblePanels(instance)) === 2,
      { fixed: true, timeoutMs: 15_000 },
    )
  }

  await setSessionRestore(page, true)
  // The active panel is the right one, in the base folder; the left one stands in `a`.
  const at = new URLSearchParams({
    p: files.folder,
    split: '1',
    side: 'r',
    'o.src': 'device',
    'o.p': files.path('a'),
  })
  const opened = await runAction(page, 'wm.app.open', {
    appId: 'system.files',
    at: `/device?${at}`,
  })
  assert.ok(opened.ok, JSON.stringify(opened))
  await hideSidebar(page)
  await page.waitForDisplayed(entryIn('left', 'a.txt'))
  await page.waitForDisplayed(entryIn('right', 'b'))
  assert.equal(await activePanel(page), 'right')
  // One step in the active panel's history.
  await doubleClick(page, entryIn('right', 'b'))
  await page.waitForDisplayed(entryIn('right', 'b.txt'))
  ctx.step('the split with the right panel active, one step taken')

  await ctx.waitFor(
    'the saved session to hold the split',
    async () => {
      const response = await page.invoke('wm_session_load')
      assert.ok(response.ok, JSON.stringify(response))
      const loaded = response.data as {
        session?: {
          windows?: Array<{
            tabs?: Array<{
              appId?: string
              history?: {
                entries?: Array<{
                  location?: { query?: Record<string, string> }
                }>
                index?: number
              }
            }>
          }>
        }
      }
      const tab = loaded.session?.windows
        ?.flatMap((window) => window.tabs ?? [])
        .find((candidate) => candidate.appId === 'system.files')
      const history = tab?.history
      const entries = history?.entries ?? []
      const current = entries[history?.index ?? -1]?.location?.query
      // The debounced save must include the step into `b` as the current history entry.
      return (
        current?.p === files.path('b') &&
        typeof history?.index === 'number' &&
        history.index === entries.length - 1 &&
        entries.length > 1
      )
    },
    { fixed: true, timeoutMs: 15_000 },
  )
  await device.restart()
  const restored = device.page
  await ctx.waitFor(
    'the file browser tab after the restart',
    async () =>
      (await wmSnapshot(restored)).windows.some((window) =>
        window.tabs.some((tab) => tab.appId === 'system.files'),
      ),
    { fixed: true, timeoutMs: 30_000 },
  )
  await hideSidebar(restored)
  await restored.waitForDisplayed(entryIn('left', 'a.txt'))
  await restored.waitForDisplayed(entryIn('right', 'b.txt'))
  assert.equal(await activePanel(restored), 'right')
  ctx.step('both places and the active side are back')

  // The active panel's history came back, the other one starts empty.
  const disabled = (side: 'left' | 'right') =>
    restored.exec<boolean>(
      `return document.querySelector(arguments[0]).disabled`,
      [inPanel(side, 'files-back')],
    )
  assert.equal(await disabled('left'), true, 'the left history starts empty')
  assert.equal(await disabled('right'), false, 'the right history came back')
  await restored.click(inPanel('right', 'files-back'))
  await restored.waitForDisplayed(entryIn('right', 'b'))
  assert.ok(await panelShows(restored, 'left', 'a.txt'), 'the left one stays')
  ctx.step('Back goes back in the active panel only')

  // Off and on again keeps both places.
  await restored.click('files-split-toggle')
  await ctx.waitFor(
    'one panel',
    async () => (await visiblePanels(restored)) === 1,
    { fixed: true, timeoutMs: 15_000 },
  )
  await restored.click('files-split-toggle')
  await ctx.waitFor(
    'two panels again',
    async () => (await visiblePanels(restored)) === 2,
    { fixed: true, timeoutMs: 15_000 },
  )
  await restored.waitForDisplayed(entryIn('left', 'a.txt'))
  await restored.waitForDisplayed(entryIn('right', 'b'))
  ctx.step('off and on again keeps both places')
})
