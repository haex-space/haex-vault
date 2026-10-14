import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  contactHeaders,
  expectContacts,
  phoneCommand,
  startPhoneSync,
} from '../lib/contacts.ts'

// Spec 051 US2-3/4/5/7: once the sync runs, a number changed on the phone reaches the address
// book, an address added in holzi reaches the phone's contact in its account, a contact deleted on
// the phone goes from the address book, and switching the sync off keeps everything on both sides.
scenario(
  'android-contacts-two-way',
  { needs: { phone: true } },
  async (ctx) => {
    const instance = await ctx.startInstance()
    const phone = instance.phone
    assert.ok(phone !== undefined, 'a phone has its controls')
    phone.removeTestContacts()
    try {
      const anna = phone.insertContact({
        given: 'Anna',
        family: 'Beck',
        phones: ['0170 1234567'],
      })
      await createAndUnlock(instance, { name: 'phone-vault' })
      phone.allowContacts()
      await startPhoneSync(instance)
      await expectContacts(ctx, instance, ['Anna Beck'], 'Anna taken over')

      phone.updatePhone(anna, '0170 7654321')
      await phoneCommand(instance, 'contacts_phone_run')
      const changed = (await contactHeaders(instance))[0]
      assert.deepEqual(changed?.phones, ['0170 7654321'])
      ctx.step('a number changed on the phone reaches the address book')

      unwrap(
        'contacts_update',
        await instance.invoke('contacts_update', {
          args: {
            id: changed?.id,
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
      await phoneCommand(instance, 'contacts_phone_run')
      const onPhone = phone
        .queryContacts()
        .find((contact) => contact.id === anna)
      assert.deepEqual(onPhone?.emails, ['anna@example.org'])
      ctx.step(
        'an address added in holzi reaches the same contact on the phone',
      )

      const bernd = phone.insertContact({
        given: 'Bernd',
        family: 'Clausen',
        phones: ['0171 7654321'],
      })
      await phoneCommand(instance, 'contacts_phone_run')
      await expectContacts(
        ctx,
        instance,
        ['Anna Beck', 'Bernd Clausen'],
        'Bernd new',
      )
      phone.deleteContact(bernd)
      await phoneCommand(instance, 'contacts_phone_run')
      await expectContacts(ctx, instance, ['Anna Beck'], 'Bernd gone')
      ctx.step('contacts created and deleted on the phone follow')

      await phoneCommand(instance, 'contacts_phone_disable')
      phone.updatePhone(anna, '0170 0000000')
      const refused = await instance.invoke('contacts_phone_run')
      assert.ok('ok' in refused && !refused.ok, 'no run while off')
      assert.deepEqual((await contactHeaders(instance))[0]?.phones, [
        '0170 7654321',
      ])
      assert.equal(
        phone.queryContacts().length,
        1,
        'nothing deleted on the phone',
      )
      ctx.step('switched off, both sides keep their contacts')
    } finally {
      phone.removeTestContacts()
    }
  },
)
