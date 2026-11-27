import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  choose,
  isShown,
  openSettings,
  waitForLocation,
} from '../lib/settings.ts'
import { vaultFiles } from '../lib/vault-files.ts'

// Spec 098, quickstart steps 1, 2 and 6 (US1 FR-002, US3): the "Protokoll" setting of "Grundeinstellung"
// switches to "Ausführlich" at once and shows when it ends, and back to "Standard"; a computer offers
// to open the log folder, a phone offers to share the log and no other. On a phone, sharing makes a
// copy of the log in the app's cache and opens the system's share sheet (the copy is in the folder the
// provider exposes), and the back key returns to the app.
scenario('diagnostic-log-setting', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-diagnostic-log' })
  await openSettings(instance)
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')
  await instance.waitForDisplayed('diagnostic-log-level')
  assert.equal(
    await isShown(instance, '[data-testid="diagnostic-log-ends"]'),
    false,
    'Standard names no end',
  )
  ctx.step('the setting shows "Standard"')

  await choose(instance, 'diagnostic-log-level', 'verbose')
  await instance.waitForDisplayed('diagnostic-log-ends')
  ctx.step('"Ausführlich" shows when it ends')

  await choose(instance, 'diagnostic-log-level', 'standard')
  await ctx.waitFor(
    'the end to go with "Standard"',
    async () =>
      !(await isShown(instance, '[data-testid="diagnostic-log-ends"]')),
  )
  ctx.step('"Standard" again')

  const phone = instance.phone
  const reveal = await isShown(
    instance,
    '[data-testid="diagnostic-log-reveal"]',
  )
  const share = await isShown(instance, '[data-testid="diagnostic-log-share"]')
  assert.equal(reveal, phone === undefined, 'only a computer opens the folder')
  assert.equal(share, phone !== undefined, 'only a phone shares the log')
  ctx.step('the buttons of this platform')

  if (phone === undefined) return

  await instance.click('diagnostic-log-share')
  await ctx.waitFor(
    "the system's share sheet",
    async () => {
      const front = phone.frontPackage()
      return front !== undefined && !phone.appInFront()
    },
    { timeoutMs: 20_000 },
  )
  const copy = vaultFiles(instance.root).sharedLogCopy()
  assert.ok(
    copy !== null,
    `the copy of the log is in the cache folder the provider exposes: ${copy}`,
  )
  ctx.step(`the share sheet is open (${phone.frontPackage()}) over the copy`)

  phone.back()
  await ctx.waitFor(
    'the app to be in front again',
    async () => phone.appInFront(),
    { timeoutMs: 20_000 },
  )
  assert.equal(
    await isShown(instance, '[data-sonner-toast]'),
    false,
    'sharing showed no error',
  )
  ctx.step('back in the app without an error')
})
