import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openLauncher } from '../lib/flows.ts'
import { isShown, runAction, waitForLocation } from '../lib/settings.ts'

const LAUNCHER_ENTRY = '[data-app-id="system.settings"]'

// Spec 043 FR-013 with spec 020 FR-019: the system's back gesture closes an open overlay, else goes
// back in the active tab, else (on a phone) opens the window overview, and it never leaves the app.
scenario('android-back-gesture', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance()
  const phone = instance.phone!
  await createAndUnlock(instance, { name: 'phone-vault' })
  // Until the page has registered the back handler, the system's own back runs and the app goes to
  // the background instead of going back in its tab (`plugins/actions.client.ts` marks the document).
  await ctx.waitFor('the back handler to be registered', () =>
    instance.exec<boolean>(
      `return document.documentElement.dataset.backGesture === 'ready'`,
    ),
  )

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
  phone.back()
  try {
    await waitForLocation(instance, 'general.basic')
  } catch (error) {
    // The key did not take the tab back: say what the page saw (a hidden page means the app went to
    // the background) and what the window manager decides when it is asked directly.
    const page = await instance.exec<string>(
      `return JSON.stringify({ visibility: document.visibilityState, focus: document.hasFocus(), handler: document.documentElement.dataset.backGesture ?? null })`,
    )
    const direct = await runAction(instance, 'wm.system.back')
    throw new Error(
      `${(error as Error).message}; page ${page}, direct wm.system.back ${JSON.stringify(direct)}`,
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
