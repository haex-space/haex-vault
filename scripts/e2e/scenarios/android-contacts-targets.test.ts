import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  contactHeaders,
  createContact,
  openContacts,
  phoneCommand,
  phoneDevice,
  startPhoneSync,
} from '../lib/contacts.ts'

// Spec 051 US3-4/5 (quickstart `android-contacts-targets`): contacts made only in holzi reach the
// phone once they get it as a target, selected in the list or as a whole group. A target removed
// with "keep" leaves the contact on the phone without syncing it any more; one removed with
// "delete" takes it off the phone and leaves it in holzi. Incoming changes start the runs.
scenario(
  'android-contacts-targets',
  { needs: { phone: true }, timeoutMs: 300_000 },
  async (ctx) => {
    const instance = await ctx.startInstance()
    const phone = instance.phone
    assert.ok(phone !== undefined, 'a phone has its controls')
    phone.removeTestContacts()
    const onPhone = (name: string) =>
      phone.queryContacts().find((contact) => contact.name === name)
    try {
      await createAndUnlock(instance, { name: 'targets-vault' })
      phone.allowContacts()
      await startPhoneSync(instance)
      const device = await phoneDevice(instance)
      const person = (given: string, family: string, groups: string[] = []) =>
        createContact(instance, {
          head: { givenName: given, familyName: family },
          values: [{ kind: 'phone', label: 'mobile', value: '0170 5550000' }],
          groups,
        })
      const team = unwrap<{ id: string }>(
        'contacts_group_save',
        await instance.invoke('contacts_group_save', {
          args: { name: 'Team' },
        }),
      ).id
      const gerd = await person('Gerd', 'Hahn')
      const ida = await person('Ida', 'Jung')
      await person('Karl', 'Lang', [team])
      await person('Lena', 'Maier', [team])
      assert.equal(phone.queryContacts().length, 0, 'only in holzi so far')
      ctx.step('four contacts only in holzi')

      await openContacts(instance)
      await instance.click('contacts-select')
      await instance.click(`contacts-row-${gerd}`)
      await instance.click(`contacts-row-${ida}`)
      await instance.click('contacts-selection-send')
      await instance.click(`contacts-selection-send-${device}`)
      await ctx.waitFor(
        'Gerd and Ida on the phone',
        () =>
          onPhone('Gerd Hahn') !== undefined &&
          onPhone('Ida Jung') !== undefined,
        { timeoutMs: 60_000, fixed: true },
      )
      ctx.step('two selected contacts sent to the phone')

      await phoneCommand(instance, 'contacts_targets_set', {
        groupId: team,
        device,
      })
      await ctx.waitFor(
        'the group on the phone',
        () =>
          onPhone('Karl Lang') !== undefined &&
          onPhone('Lena Maier') !== undefined,
        { timeoutMs: 60_000, fixed: true },
      )
      ctx.step('a whole group sent to the phone')

      await phoneCommand(instance, 'contacts_targets_remove', {
        contactIds: [ida],
        device,
        onPhone: 'keep',
      })
      await phoneCommand(instance, 'contacts_phone_run')
      const kept = onPhone('Ida Jung')
      assert.ok(kept !== undefined, 'kept on the phone')
      phone.updatePhone(kept.id, '0170 9999999')
      await phoneCommand(instance, 'contacts_phone_run')
      const idaInHolzi = (await contactHeaders(instance)).find(
        (header) => header.id === ida,
      )
      assert.deepEqual(idaInHolzi?.phones, ['0170 5550000'], 'no longer synced')
      assert.deepEqual(idaInHolzi?.targets, [])
      ctx.step('a target removed with "keep" leaves the contact unsynced')

      await phoneCommand(instance, 'contacts_targets_remove', {
        contactIds: [gerd],
        device,
        onPhone: 'delete',
      })
      await ctx.waitFor(
        'Gerd to leave the phone',
        () => onPhone('Gerd Hahn') === undefined,
        { timeoutMs: 60_000, fixed: true },
      )
      assert.ok(
        (await contactHeaders(instance)).some((header) => header.id === gerd),
        'Gerd stays in holzi',
      )
      ctx.step('a target removed with "delete" takes it off the phone only')
    } finally {
      phone.removeTestContacts()
    }
  },
)
