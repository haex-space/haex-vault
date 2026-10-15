import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openLauncher } from '../lib/flows.ts'
import type { FlowInstance } from '../lib/flows.ts'
import {
  WM,
  isShown,
  runAction,
  settingsLocation,
  waitForLocation,
} from '../lib/settings.ts'

const LAUNCHER_ENTRY = '[data-app-id="system.settings"]'

/** What `wm.system.back` decides on: the overlays, and per window its place in the stack and the history of its active tab. */
function backState(instance: FlowInstance): Promise<string> {
  return instance.exec<string>(
    `const wm = ${WM}
     return JSON.stringify({
       overlays: { ...wm.overlays },
       windows: wm.windows.map((w) => ({
         app: w.tabs.find((t) => t.id === w.activeTabId)?.appId,
         workspace: w.workspaceId,
         stack: w.stack,
         minimized: w.minimized,
         history: wm.historyOf(w.activeTabId),
       })),
     })`,
  )
}

// Spec 043 FR-013 with spec 020 FR-019: the system's back gesture closes an open overlay, else goes
// back in the active tab, else (on a phone) opens the window overview, and it never leaves the app.
scenario('android-back-gesture', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance()
  const phone = instance.phone!
  await createAndUnlock(instance, { name: 'phone-vault' })

  const opened = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/general/basic',
  })
  assert.ok(opened.ok, JSON.stringify(opened))

  // The settings singleton may already be open at a nested location when the workspace is
  // restored. Normalize the starting page through the same tab-navigation API before testing
  // the phone's back semantics.
  const positioned = await runAction(instance, 'wm.tab.navigate', {
    to: '/general/basic',
    replace: true,
  })
  assert.ok(positioned.ok, JSON.stringify(positioned))
  await waitForLocation(instance, 'general.basic')
  await instance.click('settings-row-general.basic.password')
  await waitForLocation(instance, 'general.basic.password')
  const before = await backState(instance)
  phone.back()
  try {
    await waitForLocation(instance, 'general.basic')
  } catch (error) {
    // On CI the key got no effect since the address book landed (three runs in a row), where a
    // local emulator with the same APK passes. A second key tells a lost key from a state that
    // has nothing to go back to; the failure carries both states.
    const after = await backState(instance)
    phone.back()
    await new Promise((resolve) => setTimeout(resolve, 2_000))
    const second = await settingsLocation(instance)
    throw new Error(
      `${(error as Error).message}\nbefore the back key: ${before}\nafter it: ${after}\nlocation after a second back key: ${JSON.stringify(second)}`,
      { cause: error },
    )
  }
  ctx.step('back goes back in the active tab')

  await openLauncher(instance)
  await ctx.waitFor('the launcher', () => isShown(instance, LAUNCHER_ENTRY))
  phone.back()
  await ctx.waitFor(
    'the launcher to close',
    async () => !(await isShown(instance, LAUNCHER_ENTRY)),
  )
  ctx.step('back closes an open overlay')

  // Further back gestures with nothing left to go back to: the app stays, on its workspace.
  phone.back()
  phone.back()
  phone.back()
  await new Promise((resolve) => setTimeout(resolve, 1_000))
  assert.ok(instance.alive(), 'the back gesture ended the app')
  assert.match(
    await instance.exec<string>('return location.pathname'),
    /^\/workspace\//,
  )
  ctx.step('back never leaves the app')
})
