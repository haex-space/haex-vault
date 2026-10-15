import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { reveal } from '../lib/appearance.ts'
import {
  contactHeaders,
  createContact,
  expectContacts,
  openContacts,
  phoneDevice,
  phoneOf,
  startPhoneSync,
} from '../lib/contacts.ts'

// Spec 051 US3-1/2/4/6 (quickstart `android-contacts-desktop`): a phone on the emulator and a
// laptop on Linux, one vault. The laptop offers the phone as a target; a contact made in its
// editor with the phone as target reaches the phone's address book, one left "only in holzi" does
// not, and a change of a contact from the phone goes back to it. Removing the target with "delete"
// takes the contact off the phone, not off holzi. Changes made while the phone's app is closed
// arrive once it opens again. No run is started by hand: incoming changes start them.
scenario(
  'android-contacts-desktop',
  { needs: { phone: true }, timeoutMs: 420_000 },
  async (ctx) => {
    const group = await ctx.group({ users: { anna: ['phone', 'laptop'] } })
    const onPhone = group.device('anna/phone')
    const laptop = group.device('anna/laptop').page
    const phone = phoneOf(onPhone.page)
    phone.removeTestContacts()
    try {
      const annaRaw = phone.insertContact({
        given: 'Anna',
        family: 'Beck',
        phones: ['0170 1234567'],
      })
      phone.allowContacts()
      await startPhoneSync(onPhone.page)
      await expectContacts(
        ctx,
        laptop,
        ['Anna Beck'],
        'Anna to reach the laptop',
      )
      const device = await phoneDevice(laptop)
      ctx.step('the laptop offers the phone as a target')

      await openContacts(laptop)
      await laptop.click('contacts-new')
      await laptop.waitForDisplayed('contacts-editor')
      await laptop.type('contacts-field-givenName', 'Carla')
      await laptop.type('contacts-field-familyName', 'Diehl')
      await laptop.type(
        '[data-testid="contacts-editor-value-phone"] [data-testid="contacts-editor-value"]',
        '0171 2223334',
      )
      await reveal(laptop, 'contacts-editor-only-holzi')
      await laptop.click(`contacts-editor-target-${device}`)
      await laptop.click('contacts-editor-save')
      await laptop.waitForDisplayed('contacts-edit', 15_000)
      await createContact(laptop, {
        head: { givenName: 'Dora', familyName: 'Ernst' },
        values: [{ kind: 'phone', label: 'mobile', value: '0172 5556667' }],
        groups: [],
      })
      const anna = (await contactHeaders(laptop)).find(
        (header) => header.name === 'Anna Beck',
      )
      assert.ok(anna !== undefined)
      unwrap(
        'contacts_update',
        await laptop.invoke('contacts_update', {
          args: {
            id: anna.id,
            patch: {
              values: {
                upsert: [
                  { kind: 'email', label: 'home', value: 'anna@example.org' },
                ],
                remove: [],
              },
            },
          },
        }),
      )
      await ctx.waitFor(
        'Carla and Anna’s address on the phone',
        () => {
          const held = phone.queryContacts()
          return (
            held.some((contact) => contact.name === 'Carla Diehl') &&
            held.some(
              (contact) =>
                contact.id === annaRaw &&
                contact.emails.includes('anna@example.org'),
            )
          )
        },
        { timeoutMs: 90_000, fixed: true },
      )
      assert.ok(
        !phone.queryContacts().some((contact) => contact.name === 'Dora Ernst'),
        'a contact without a target stays only in holzi',
      )
      ctx.step('made and changed on the laptop, written on the phone')

      // Carla's page is open: the target goes, with "delete on the phone".
      await reveal(laptop, `contacts-target-remove-${device}`)
      await laptop.click(`contacts-target-remove-${device}`)
      await laptop.waitForDisplayed('contacts-target-remove-dialog')
      await laptop.click('contacts-target-remove-delete')
      await ctx.waitFor(
        'Carla to leave the phone',
        () =>
          !phone
            .queryContacts()
            .some((contact) => contact.name === 'Carla Diehl'),
        { timeoutMs: 90_000, fixed: true },
      )
      assert.ok(
        (await contactHeaders(laptop)).some(
          (header) => header.name === 'Carla Diehl',
        ),
        'Carla stays in holzi',
      )
      ctx.step('a target removed with "delete" leaves the phone only')

      await onPhone.stop()
      unwrap(
        'contacts_update',
        await laptop.invoke('contacts_update', {
          args: { id: anna.id, patch: { nickname: 'Anni' } },
        }),
      )
      const ella = await createContact(laptop, {
        head: { givenName: 'Ella', familyName: 'Fuchs' },
        values: [],
        groups: [],
        targets: [{ device }],
      })
      assert.ok(ella)
      await onPhone.start()
      await ctx.waitFor(
        'Ella on the phone after it opened again',
        () =>
          phone
            .queryContacts()
            .some((contact) => contact.name === 'Ella Fuchs'),
        { timeoutMs: 120_000, fixed: true },
      )
      ctx.step('changes made while the phone was closed arrive when it opens')
    } finally {
      phone.removeTestContacts()
    }
  },
)
