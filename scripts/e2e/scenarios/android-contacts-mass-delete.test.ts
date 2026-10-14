import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  expectContacts,
  openContactsSettings,
  phoneCommand,
  startPhoneSync,
} from '../lib/contacts.ts'

// Spec 051 FR-016: when a run would delete more than 10 contacts that are also more than 10 % of
// the contacts synced with this phone, it deletes nothing and asks. "Behalten" keeps them in the
// address book and writes them to the phone again. Twelve contacts written through adb take most of a
// minute on an emulator, hence the longer deadline.
scenario(
  'android-contacts-mass-delete',
  { needs: { phone: true }, timeoutMs: 180_000 },
  async (ctx) => {
    const instance = await ctx.startInstance()
    const phone = instance.phone
    assert.ok(phone !== undefined, 'a phone has its controls')
    phone.removeTestContacts()
    try {
      const names = Array.from(
        { length: 12 },
        (_, n) => `Person ${String(n + 1).padStart(2, '0')}`,
      )
      const raws = names.map((name, n) =>
        phone.insertContact({
          given: name.split(' ')[0]!,
          family: name.split(' ')[1]!,
          phones: [`0170 ${String(n).padStart(7, '0')}`],
        }),
      )
      await createAndUnlock(instance, { name: 'phone-vault' })
      phone.allowContacts()
      await startPhoneSync(instance)
      await expectContacts(ctx, instance, names, 'all twelve taken over')

      for (const raw of raws.slice(0, 11)) phone.deleteContact(raw)
      const held = await phoneCommand<{
        paused: string | null
        massDelete: { fromPhone: number } | null
      }>(instance, 'contacts_phone_run')
      assert.equal(held.paused, 'mass_delete')
      assert.equal(held.massDelete?.fromPhone, 11)
      await expectContacts(ctx, instance, names, 'nothing deleted yet')
      ctx.step('a mass deletion waits for the person')

      await openContactsSettings(instance)
      await instance.waitForDisplayed('contacts-mass-delete')
      await instance.click('contacts-mass-delete-keep')
      await ctx.waitFor(
        'the contacts back on the phone',
        () => phone.queryContacts().length === 12,
      )
      await expectContacts(ctx, instance, names, 'all twelve kept')
      const status = await phoneCommand<{ state: string }>(
        instance,
        'contacts_phone_status',
      )
      assert.equal(status.state, 'active')
      ctx.step('"Behalten" keeps them and writes them to the phone again')
    } finally {
      phone.removeTestContacts()
    }
  },
)
