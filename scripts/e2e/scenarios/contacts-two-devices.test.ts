import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import {
  contactHeaders,
  createContact,
  expectContacts,
} from '../lib/contacts.ts'

// Spec 051, US1-4 (quickstart `contacts-two-devices`): two devices of one user keep the address
// book in step. A contact made on one shows on the other with its values; a change on the other
// comes back; a deletion on one removes it on both. The devices come from the group API of spec 033
// and find each other through the scenario's Nostr relay.
scenario('contacts-two-devices', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
  const laptop = group.device('anna/laptop').page
  const phone = group.device('anna/phone').page

  const ben = await createContact(laptop, {
    head: { givenName: 'Ben', familyName: 'Krüger' },
    values: [{ kind: 'phone', label: 'mobile', value: '0170 1111' }],
    groups: [],
  })
  await expectContacts(ctx, phone, ['Ben Krüger'], 'the contact to show on the phone')
  const [onPhone] = await contactHeaders(phone)
  assert.deepEqual(onPhone?.phones, ['0170 1111'])
  ctx.step('created on one device, shown on the other')

  unwrap(
    'contacts_update',
    await phone.invoke('contacts_update', {
      args: { id: ben, patch: { nickname: 'Benni' } },
    }),
  )
  await ctx.waitFor(
    'the change to reach the laptop',
    async () => {
      const contact = unwrap<{ head: { nickname?: string } }>(
        'contacts_get',
        await laptop.invoke('contacts_get', { args: { id: ben } }),
      )
      return contact.head.nickname === 'Benni'
    },
    { timeoutMs: 40_000, fixed: true },
  )
  ctx.step('changed on the other device')

  unwrap(
    'contacts_delete',
    await laptop.invoke('contacts_delete', { args: { ids: [ben] } }),
  )
  await expectContacts(ctx, phone, [], 'the deletion to reach the phone')
  ctx.step('deleted on both')
})
