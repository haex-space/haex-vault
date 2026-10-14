import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { clearButtonOf } from '../lib/fields.ts'
import { contactHeaders, openContacts } from '../lib/contacts.ts'

// Spec 051, US1 (quickstart `contacts-basic`): the address book on one device. A contact made in
// the editor with a phone number, an e-mail address and a custom value shows in the list; the
// search finds it by name and by the digits of its number and misses a word that is nowhere; a
// change of its number keeps the other values; deleting it asks first and removes it.
scenario('contacts-basic', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-contacts' })
  await openContacts(instance)
  await instance.waitForDisplayed('contacts-empty')
  ctx.step('opened')

  // The editor starts with a phone and an e-mail row; a custom value is added from the menu.
  await instance.click('contacts-new')
  await instance.waitForDisplayed('contacts-editor')
  await instance.type('contacts-field-givenName', 'Anna')
  await instance.type('contacts-field-familyName', 'Beck')
  await instance.type(
    '[data-testid="contacts-editor-value-phone"] [data-testid="contacts-editor-value"]',
    '0170 123-45 67',
  )
  await instance.type(
    '[data-testid="contacts-editor-value-email"] [data-testid="contacts-editor-value"]',
    'anna@example.org',
  )
  await instance.click('contacts-editor-add-value')
  await instance.click('contacts-editor-add-custom')
  await instance.type('contacts-editor-custom-name', 'Kundennummer')
  await instance.type(
    '[data-testid="contacts-editor-value-custom"] [data-testid="contacts-editor-value"]',
    '4711',
  )
  await instance.click('contacts-editor-save')
  await instance.waitForDisplayed('contacts-entry-name')
  const [anna] = await contactHeaders(instance)
  assert.ok(anna, 'the contact exists')
  assert.equal(anna.name, 'Anna Beck')
  assert.deepEqual(anna.phones, ['0170 123-45 67'])
  assert.deepEqual(anna.emails, ['anna@example.org'])
  const stored = unwrap<{
    values: Array<{ id: string; kind: string; label: string; value: string }>
    origin: { kind: string; caller: string }
  }>(
    'contacts_get',
    await instance.invoke('contacts_get', { args: { id: anna.id } }),
  )
  assert.equal(stored.values.length, 3)
  assert.ok(
    stored.values.some(
      (v) => v.kind === 'custom' && v.label === 'custom:Kundennummer',
    ),
  )
  assert.deepEqual(stored.origin, { ...stored.origin, kind: 'holzi', caller: 'user' })
  ctx.step('created in the editor')

  // The search: by name, by bare digits of the number, and a word that is nowhere.
  await instance.click('contacts-back')
  await instance.waitForDisplayed(`contacts-row-${anna.id}`)
  await instance.type('contacts-search', 'beck')
  await instance.waitForDisplayed(`contacts-row-${anna.id}`)
  await instance.click(clearButtonOf('contacts-search'))
  await instance.type('contacts-search', '01701234567')
  await instance.waitForDisplayed(`contacts-row-${anna.id}`)
  await instance.click(clearButtonOf('contacts-search'))
  await instance.type('contacts-search', 'zzz-nothing')
  await instance.waitForDisplayed('contacts-empty')
  await instance.click(clearButtonOf('contacts-search'))
  ctx.step('searched')

  // A changed number keeps the other values.
  const phone = stored.values.find((v) => v.kind === 'phone')!
  unwrap(
    'contacts_update',
    await instance.invoke('contacts_update', {
      args: {
        id: anna.id,
        patch: {
          values: {
            upsert: [{ id: phone.id, kind: 'phone', label: 'mobile', value: '0171 7654321' }],
            remove: [],
          },
        },
      },
    }),
  )
  const changed = unwrap<{ values: Array<{ id: string; value: string }> }>(
    'contacts_get',
    await instance.invoke('contacts_get', { args: { id: anna.id } }),
  )
  assert.equal(changed.values.length, 3)
  assert.equal(changed.values.find((v) => v.id === phone.id)?.value, '0171 7654321')
  ctx.step('changed one value')

  // Deleting asks first.
  await instance.click(`contacts-row-${anna.id}`)
  await instance.click('contacts-delete')
  await instance.click('contacts-delete-confirm')
  await instance.waitForDisplayed('contacts-empty')
  assert.deepEqual(await contactHeaders(instance), [])
  ctx.step('deleted')
})
