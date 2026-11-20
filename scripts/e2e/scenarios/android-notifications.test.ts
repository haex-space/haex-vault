import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
} from '../lib/extensions.ts'

// Spec 043 FR-023 (quickstart §6 step 4): a notification of an extension appears as an Android
// notification once the person allowed it; when they refused the system's permission, it stays
// visible inside haex-vault as a message. The system's own question cannot be driven, so the answer is
// given beforehand through the phone's controls.
scenario(
  'android-notifications',
  { needs: { phone: true }, timeoutMs: 180_000 },
  async (ctx) => {
    const instance = await ctx.startInstance()
    const phone = instance.phone
    assert.ok(phone !== undefined, 'a phone has its controls')
    await createAndUnlock(instance, { name: 'phone-vault' })

    const probe = await install(instance, fixture('e2e', 'probe'))
    await openFromLauncher(instance, probe)
    await ctx.waitFor(
      'the probe to be ready',
      () =>
        inFrame<boolean>(
          instance,
          probe,
          `return document.documentElement.dataset.probeReady === '1'`,
        ).catch(() => false),
      { timeoutMs: 20_000 },
    )
    const notify = (title: string) =>
      probeRequest(instance, probe, 'extension_notifications_show', {
        options: { title, body: 'in 5 Minuten' },
      })
    const messages = () =>
      instance.exec<string[]>(
        `return [...document.querySelectorAll('[data-sonner-toast]')].map((t) => t.textContent)`,
      )

    // haex-vault's own permission comes first, as on a desktop.
    phone.allowNotifications(false)
    assert.equal((await notify('Standup')).error?.code, 1004)
    await instance.waitForDisplayed('extension-permission-request', 10_000)
    await instance.click('extension-permission-allow')
    const refused = await notify('Standup')
    assert.equal(refused.error, undefined, JSON.stringify(refused.error))
    await ctx.waitFor('the message to show inside HaexVault', async () =>
      (await messages()).some((text) => text?.includes('Standup')),
    )
    assert.deepEqual(phone.notificationTitles(), [], 'the system shows nothing')
    ctx.step('refused by the system: the message shows inside HaexVault')

    phone.allowNotifications(true)
    const allowed = await notify('Mittag')
    assert.equal(allowed.error, undefined, JSON.stringify(allowed.error))
    await ctx.waitFor('the system to show the notification', () =>
      phone.notificationTitles().includes('Mittag'),
    )
    assert.ok(
      !(await messages()).some((text) => text?.includes('Mittag')),
      'no message inside HaexVault as well',
    )
    ctx.step('allowed: an Android notification')
  },
)
