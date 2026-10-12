import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { isShown, runAction } from '../lib/settings.ts'

// Spec 044 US7 (FR-039, quickstart §7 steps 1 and 2): without "Zugriff auf alle Dateien" the file
// browser explains why it needs it instead of the device's folders, and its button leads into the
// system settings. Granted there, back in holzi the folders appear without a restart. The system's
// switch cannot be driven, so the grant comes through the phone's controls while its page is open.

const SHARED = '/storage/emulated/0'

scenario('android-all-files', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance({ withoutAllFiles: true })
  const phone = instance.phone
  assert.ok(phone !== undefined, 'a phone has its controls')
  await createAndUnlock(instance, { name: 'phone-vault' })

  const opened = await runAction(instance, 'wm.app.open', {
    appId: 'system.files',
    at: `/device?p=${encodeURIComponent(SHARED)}`,
  })
  assert.ok(opened.ok, JSON.stringify(opened))
  await instance.waitForDisplayed('files-all-access')
  assert.ok(
    !(await isShown(instance, '[data-testid="files-entries"]')),
    'the folder shows despite the missing permission',
  )
  ctx.step('without the permission the file browser explains it')

  await instance.click('files-all-access-request')
  phone.allowAllFiles()
  phone.back()
  await instance.waitForDisplayed('files-entry-Download', 30_000)
  assert.ok(
    !(await isShown(instance, '[data-testid="files-all-access"]')),
    'the explanation stays after the grant',
  )
  ctx.step('after the grant the shared storage shows without a restart')
})
