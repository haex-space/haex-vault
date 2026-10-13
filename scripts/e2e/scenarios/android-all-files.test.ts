import assert from 'node:assert/strict'
import { createAndUnlock } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { translations } from '../lib/translations.ts'

// Spec 044 US7 (FR-039, quickstart §7 steps 1–2): without "Zugriff auf alle Dateien" the file browser
// explains why it needs it instead of showing an empty list, and its button leads into the system's
// settings. The person allows it there and comes back with the back key: the device's files show
// without a restart. The permission is given through the phone's controls, as the system's switch
// cannot be driven.

const TITLES = translations('files.allAccess.title')
/** How long a back key gets to bring holzi to the front before it is pressed again. */
const BACK_AGAIN_MS = 3_000

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
  // back in front.
  let pressedAt = 0
  await ctx.waitFor(
    'holzi in front again',
    () => {
      if (phone.frontPackage() === 'com.haex.holzi')
        return Promise.resolve(true)
      if (performance.now() - pressedAt > BACK_AGAIN_MS) {
        phone.back()
        pressedAt = performance.now()
      }
      return Promise.resolve(false)
    },
    { timeoutMs: 20_000 },
  )
  await ctx.waitFor(
    "the device's files",
    () =>
      instance.exec<boolean>(
        `return !document.querySelector('[data-testid="files-all-access"]')
           && Boolean(document.querySelector('[data-testid="files-entries"], [data-testid="files-empty"]'))`,
      ),
    { timeoutMs: 30_000 },
  )
  ctx.step('allowed in the settings, the files show without a restart')
})
