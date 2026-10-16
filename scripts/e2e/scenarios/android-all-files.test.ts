import assert from 'node:assert/strict'
import { createAndUnlock } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { translations } from '../lib/translations.ts'

// Spec 044 US7 (FR-039, quickstart §7 steps 1–2): without "Zugriff auf alle Dateien" the file browser
// explains why it needs it instead of showing an empty list, and its button leads into the system's
// settings. The person allows it there and comes back: the device's files show without a restart.
// The permission is given through the phone's controls, as the system's switch cannot be driven.
//
// How the person comes back is the system's business, not holzi's: the back key on the settings page.
// On the CI emulator that page sometimes stayed in front for the whole wait although the key was
// pressed every few seconds (five runs, logcat showed no change of activity at all). The scenario
// presses the key twice, then comes back through the app's task as a person does among the recent
// apps; the step says which way it was, so the report shows when the key did not work.

const TITLES = translations('files.allAccess.title')
/** How long a back key gets to bring holzi to the front before the next try. */
const BACK_AGAIN_MS = 3_000
/** How often the back key is pressed before the scenario comes back through the app's task. */
const BACK_PRESSES = 2
const HOLZI = 'com.haex.holzi'

scenario('android-all-files', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance({ withoutAllFiles: true })
  const phone = instance.phone
  assert.ok(phone !== undefined, 'a phone has its controls')
  await createAndUnlock(instance, { name: 'phone-files' })
  await runAction(instance, 'wm.app.open', { appId: 'system.files' })

  await instance.waitForDisplayed('files-all-access', 15_000)
  const shown = await instance.exec<string>(
    `return document.querySelector('[data-testid="files-all-access"] h2')?.textContent?.trim() ?? ''`,
  )
  assert.ok(TITLES.includes(shown), `the browser says "${shown}"`)
  ctx.step('without the permission the browser explains it')

  await instance.click('files-all-access-request')
  await ctx.waitFor(
    "the system's settings in front",
    () => Promise.resolve(phone.frontPackage() === 'com.android.settings'),
    { timeoutMs: 15_000 },
  )
  phone.allowAllFiles()
  // The settings open in two steps, a forwarding screen and then the page; a back key pressed
  // between the two is lost and the page stays. Like a person, press it again while holzi is not
  // back in front, and after the last press come back through the app's task.
  let pressedAt = 0
  let presses = 0
  let throughTask = false
  try {
    await ctx.waitFor(
      'holzi in front again',
      () => {
        if (phone.frontPackage() === HOLZI) return Promise.resolve(true)
        if (performance.now() - pressedAt > BACK_AGAIN_MS) {
          if (presses < BACK_PRESSES) {
            phone.back()
            presses += 1
          } else {
            phone.bringToFront()
            throughTask = true
          }
          pressedAt = performance.now()
        }
        return Promise.resolve(false)
      },
      { timeoutMs: 20_000 },
    )
  } catch (error) {
    throw new Error(
      `${(error as Error).message}; in front: ${phone.frontPackage()}, ${phone.frontDetail()}`,
      { cause: error },
    )
  }
  await ctx.waitFor(
    "the device's files",
    () =>
      instance.exec<boolean>(
        `return !document.querySelector('[data-testid="files-all-access"]')
           && Boolean(document.querySelector('[data-testid="files-entries"], [data-testid="files-empty"]'))`,
      ),
    { timeoutMs: 30_000 },
  )
  ctx.step(
    'allowed in the settings, the files show without a restart',
    throughTask
      ? `back through the app's task: ${presses} back keys did not return`
      : `back with ${presses} back key${presses === 1 ? '' : 's'}`,
  )
})
