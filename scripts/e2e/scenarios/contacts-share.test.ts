import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { SELF_CONTACT_ID, readShare, shareLink } from '../lib/share.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import {
  contactHeaders,
  createContact,
  expectContacts,
  openContacts,
} from '../lib/contacts.ts'

// Spec 060 US2 to US4 and spec 051 US4 (quickstart `contacts-share`): two vaults. Ben fills his own
// contact "Ich", opens the share dialog, deselects his phone number and gets a link and a QR code
// that carry exactly the rest. Mara pastes the link, sees the preview (nothing is stored yet) and
// takes it as a new contact with confirmed holzi addresses. A changed link is refused; Ben's second
// share goes to the same contact (one vault, one contact) and only adds. A holzi address typed in by
// hand shows as unconfirmed. US4-6 (the phone keeps holzi addresses) is the phone sync's rule,
// covered by its tests (FR-014).

/** The error kind of a command that must fail. */
async function refusal(
  instance: Pick<FlowInstance, 'invoke'>,
  command: string,
  args: Record<string, unknown>,
): Promise<string> {
  const result = await instance.invoke(command, { args })
  assert.ok('ok' in result && !result.ok, `${command} must fail`)
  return (result.error as { kind?: string }).kind ?? JSON.stringify(result)
}

/** The text of the share the dialog shows now. */
async function shareLinkText(
  instance: Pick<FlowInstance, 'exec'>,
): Promise<string> {
  return instance.exec<string>(
    `return document.querySelector('[data-testid="contacts-share-link"]')?.textContent?.trim() ?? ''`,
  )
}

scenario('contacts-share', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({
    users: { ben: ['laptop'], mara: ['desktop'] },
  })
  const ben = group.device('ben/laptop').page
  const mara = group.device('mara/desktop').page

  unwrap(
    'contacts_update',
    await ben.invoke('contacts_update', {
      args: {
        id: SELF_CONTACT_ID,
        patch: {
          givenName: 'Ben',
          familyName: 'Krüger',
          values: {
            upsert: [
              { kind: 'email', label: 'work', value: 'ben@example.org' },
              { kind: 'phone', label: 'mobile', value: '0170 1111' },
            ],
          },
        },
      },
    }),
  )
  const ownContact = unwrap<{ values: Array<{ id: string; kind: string }> }>(
    'contacts_get',
    await ben.invoke('contacts_get', { args: { id: SELF_CONTACT_ID } }),
  )
  const phone = ownContact.values.find((value) => value.kind === 'phone')!.id
  await openContacts(ben)
  await ben.click(`contacts-row-${SELF_CONTACT_ID}`)
  await ben.click('contacts-share-open')
  await ben.waitForDisplayed('contacts-share-link')
  const everythingLink = await shareLinkText(ben)
  assert.equal(
    readShare(everythingLink).values.length,
    2,
    'everything is chosen at first',
  )
  await ben.click(`contacts-share-check-${phone}`)
  await ctx.waitFor(
    'the link after the change',
    async () => {
      const text = await shareLinkText(ben)
      return text !== '' && text !== everythingLink
    },
    { timeoutMs: 15_000, fixed: true },
  )
  const older = await shareLinkText(ben)
  const chosen = readShare(older)
  assert.deepEqual(
    chosen.values.map((value) => value.value),
    ['ben@example.org'],
    'the deselected phone number is not in the share',
  )
  assert.equal(chosen.name, 'Ben Krüger')
  assert.ok(chosen.card, 'the vault identity goes along')
  await ben.waitForDisplayed('contacts-share-qr')
  ctx.step(
    'Ben shares "Ich" as link and QR code with his phone number left out',
  )

  await openContacts(mara)
  await mara.click('contacts-add')
  await mara.click('contacts-import-menu')
  await mara.waitForDisplayed('contacts-import-text')
  await mara.type('contacts-import-text', older)
  await mara.click('contacts-import-check')
  await mara.waitForDisplayed('contacts-import-preview')
  const shown = await mara.exec<string>(
    `return document.querySelector('[data-testid="contacts-import-name"]')?.textContent ?? ''`,
  )
  assert.ok(shown.includes('Ben Krüger'), shown)
  assert.equal((await contactHeaders(mara)).length, 0, 'nothing stored yet')
  ctx.step('Mara sees the preview before anything is stored')

  await mara.click('contacts-import-accept')
  await mara.waitForDisplayed('contacts-card-addresses', 15_000)
  await expectContacts(ctx, mara, ['Ben Krüger'], 'Ben in the address book')
  const [header] = await contactHeaders(mara)
  const accepted = unwrap<{
    origin: { kind: string }
    values: Array<{ value: string }>
    card: { devices: unknown[]; nostrRelays: string[] } | null
  }>(
    'contacts_get',
    await mara.invoke('contacts_get', { args: { id: header!.id } }),
  )
  assert.equal(accepted.origin.kind, 'card')
  assert.deepEqual(
    accepted.values.map((value) => value.value),
    ['ben@example.org'],
    'only what Ben chose, not the phone number he left out',
  )
  assert.equal(accepted.card?.devices.length, 1)
  ctx.step('the share became a contact with confirmed holzi addresses')

  const chars = [...older]
  const at = chars.length - 12
  chars[at] = chars[at] === 'A' ? 'B' : 'A'
  assert.ok(
    ['CardInvalid', 'ShareInvalid'].includes(
      await refusal(mara, 'contacts_share_preview', { text: chars.join('') }),
    ),
  )
  ctx.step('a changed share is refused')

  // Ben shares again, now with everything chosen: his vault is on the contact already, so the
  // share goes there, and what it adds is added next to what is there.
  const newer = await shareLink(ben, SELF_CONTACT_ID)
  assert.notEqual(newer, older)
  const preview = unwrap<{
    target: { kind: string; id?: string }
    targetRevision?: string
    items: Array<{ item: { type: string }; state: string; choices: string[] }>
  }>(
    'contacts_share_preview',
    await mara.invoke('contacts_share_preview', { args: { text: newer } }),
  )
  assert.equal(preview.target.id, header!.id, 'the same contact')
  const addresses = preview.items.find(
    (item) => item.item.type === 'holziAddresses',
  )
  assert.ok(addresses, 'the share carries addresses')
  assert.ok(
    addresses.state === 'same' || addresses.choices.includes('add'),
    'the addresses are asked about only when they add something',
  )
  const updated = unwrap<{
    card: { nostrRelays: string[]; devices: unknown[] }
    values: Array<{ value: string }>
  }>(
    'contacts_share_accept',
    await mara.invoke('contacts_share_accept', {
      args: {
        text: newer,
        target: { kind: 'contact', id: header!.id },
        decisions: [],
        previewRevision: preview.targetRevision,
      },
    }),
  )
  assert.ok(
    updated.card.nostrRelays.length >= accepted.card!.nostrRelays.length,
    'no address went missing',
  )
  assert.ok(
    updated.values.some((value) => value.value === 'ben@example.org'),
    'nothing the contact had went missing',
  )
  assert.equal(
    await refusal(mara, 'contacts_share_accept', {
      text: newer,
      target: { kind: 'new' },
      decisions: [],
    }),
    'ShareInvalid',
  )
  ctx.step(
    'a second share adds and loses nothing, and one vault stays on one contact',
  )

  const typed = await createContact(mara, {
    head: { givenName: 'Carla' },
    values: [
      {
        kind: 'holzi_nostr_relay',
        label: 'other',
        value: 'wss://relay.example.org',
      },
    ],
    groups: [],
  })
  await mara.click('contacts-back')
  await mara.waitForDisplayed(`contacts-row-${typed}`)
  await mara.click(`contacts-row-${typed}`)
  await mara.waitForDisplayed('contacts-holzi-addresses', 15_000)
  const marked = await mara.exec<string>(
    `return document.querySelector('[data-testid="contacts-holzi-addresses"]')?.textContent ?? ''`,
  )
  assert.ok(marked.includes('unbestätigt'), marked)
  ctx.step('a holzi address typed in by hand shows as unconfirmed')
})
