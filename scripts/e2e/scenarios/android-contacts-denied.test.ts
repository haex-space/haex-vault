import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { translations } from '../lib/translations.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { PROCESS_END_LIMIT_MS } from '../lib/close-promises.ts'
import {
  contactHeaders,
  openContactsSettings,
  phoneCommand,
  startPhoneSync,
} from '../lib/contacts.ts'

// Spec 051 US2-6, FR-017, FR-025: a refused permission leaves the sync off and explains how to
// allow it later. A permission taken back after the sync was on pauses it with the reason, and
// nothing changes on either side.

/** The explanation in every language holzi speaks. */
const EXPLANATIONS = translations('contacts.error.contactsDenied')

scenario('android-contacts-denied', { needs: { phone: true } }, async (ctx) => {
  const { passphrase } = ctx.credentials()
  const first = await ctx.startInstance()
  const phone = first.phone
  assert.ok(phone !== undefined, 'a phone has its controls')
  phone.removeTestContacts()
  try {
    await createAndUnlock(first, { name: 'phone-vault', passphrase })
    phone.refuseContacts()
    await openContactsSettings(first)
    await first.click('contacts-phone-switch')
    await first.waitForDisplayed('contacts-phone-denied')
    const shown = await first.exec<string>(
      `return document.querySelector('[data-testid="contacts-phone-denied"]')?.textContent?.trim() ?? ''`,
    )
    assert.ok(EXPLANATIONS.includes(shown), `the settings say "${shown}"`)
    const off = await phoneCommand<{ state: string }>(
      first,
      'contacts_phone_status',
    )
    assert.equal(off.state, 'off')
    ctx.step('a refused permission leaves the sync off and is explained')

    phone.insertContact({ given: 'Anna', family: 'Beck', phones: ['0170 1'] })
    phone.allowContacts()
    await startPhoneSync(first)
    phone.revokeContacts()
    await first.waitForEnd(PROCESS_END_LIMIT_MS)
    await first.stop()
    const next = await ctx.startInstance({ reusesRoot: first.root })
    unwrap(
      'open_instance',
      await next.invoke('open_instance', {
        args: { name: 'phone-vault', passphrase },
      }),
    )
    await ctx.waitFor('the sync to pause', async () => {
      const status = await phoneCommand<{ state: string; paused: string }>(
        next,
        'contacts_phone_status',
      )
      return status.state === 'paused' && status.paused === 'permission'
    })
    assert.equal(
      (await contactHeaders(next)).length,
      1,
      'nothing changed in the address book',
    )
    ctx.step('a permission taken back pauses the sync')
  } finally {
    phone.removeTestContacts()
  }
})
