import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  WM,
  openSettings,
  runAction,
  setWindowGeometry,
  waitForLocation,
} from '../lib/settings.ts'

const LISTBOX_OPEN =
  'return document.querySelector(\'[role="listbox"]\') !== null'

// A click on an open select's trigger closes it and leaves its window in front. While the select is
// open, reka disables pointer events on the body; the folder list of a window behind it (a context
// menu trigger) kept them, received the click and raised its window.
scenario('window-select-stays-front', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-select-front' })

  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.files"]')
  await openSettings(instance)
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')

  const windows = await instance.exec<Record<string, string>>(
    `return Object.fromEntries(${WM}.windows.map((w) => [w.tabs[0].appId, w.id]))`,
  )
  const files = windows['system.files']
  const settings = windows['system.settings']
  assert.ok(
    files && settings,
    `expected both windows: ${JSON.stringify(windows)}`,
  )
  // The files window lies right behind the settings, under the select's trigger.
  for (const windowId of [files, settings]) {
    await setWindowGeometry(instance, windowId, {
      x: 40,
      y: 40,
      width: 900,
      height: 600,
    })
  }
  await runAction(instance, 'wm.window.focus', { windowId: settings })
  ctx.step('the settings in front of the files')

  await instance.click('settings-language')
  await ctx.waitFor('the select to open', () =>
    instance.exec<boolean>(LISTBOX_OPEN),
  )
  const trigger = await instance.exec<{ x: number; y: number }>(
    `const r = [...document.querySelectorAll('[data-testid="settings-language"]')]
       .find((el) => el.offsetParent !== null).getBoundingClientRect()
     return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) }`,
  )
  await instance.clickAt(trigger.x, trigger.y)
  await ctx.waitFor(
    'the select to close',
    async () => !(await instance.exec<boolean>(LISTBOX_OPEN)),
  )

  const active = await instance.exec<string | null>(
    `return ${WM}.activeWindowId`,
  )
  assert.equal(
    active,
    settings,
    'closing the select by its trigger brought the files window to the front',
  )
  ctx.step('the settings stay in front')
})
