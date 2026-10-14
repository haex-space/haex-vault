import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  contactHeaders,
  expectContacts,
  openContactsSettings,
  phoneCommand,
} from '../lib/contacts.ts'

// Spec 051 US2-1/2: switching the sync on asks for the permission (given beforehand, the system's
// question cannot be driven), shows the overview, and after the confirmation the address book
// holds every contact of the phone from both accounts, with their values and the phone as origin.
scenario('android-contacts-import', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance()
  const phone = instance.phone
  assert.ok(phone !== undefined, 'a phone has its controls')
  phone.removeTestContacts()
  try {
    phone.insertContact({
      given: 'Anna',
      family: 'Beck',
      phones: ['0170 1234567'],
      emails: ['anna@example.org'],
    })
    phone.insertContact({
      given: 'Bernd',
      family: 'Clausen',
      phones: ['0171 7654321'],
    })
    phone.insertContact({
      account: ['com.holzi.e2e', 'test'],
      given: 'Clara',
      family: 'Dorn',
      emails: ['clara@example.org'],
    })
    await createAndUnlock(instance, { name: 'phone-vault' })
    phone.allowContacts()

    await openContactsSettings(instance)
    await instance.click('contacts-phone-switch')
    await instance.waitForDisplayed('contacts-review-counts', 30_000)
    const counts = await instance.exec<string>(
      `return document.querySelector('[data-testid="contacts-review-to-address-book"]')?.textContent ?? ''`,
    )
    assert.match(counts, /3/, `the overview says "${counts}"`)
    ctx.step('the overview shows what will happen before anything is written')

    assert.equal((await contactHeaders(instance)).length, 0)
    await instance.click('contacts-review-confirm')
    await expectContacts(
      ctx,
      instance,
      ['Anna Beck', 'Bernd Clausen', 'Clara Dorn'],
      'every contact of the phone in the address book',
    )
    const anna = (await contactHeaders(instance)).find(
      (header) => header.name === 'Anna Beck',
    )
    assert.deepEqual(anna?.phones, ['0170 1234567'])
    assert.deepEqual(anna?.emails, ['anna@example.org'])
    const clara = (await contactHeaders(instance)).find(
      (header) => header.name === 'Clara Dorn',
    )
    const contact = unwrap<{ origin: { kind: string; account: string } }>(
      'contacts_get',
      await instance.invoke('contacts_get', { args: { id: clara?.id } }),
    )
    assert.deepEqual(contact.origin.kind, 'phone')
    assert.equal(contact.origin.account, 'com.holzi.e2e/test')
    const status = await phoneCommand<{ state: string }>(
      instance,
      'contacts_phone_status',
    )
    assert.equal(status.state, 'active')
    ctx.step('both accounts are in the address book, with the phone as origin')
  } finally {
    phone.removeTestContacts()
  }
})
