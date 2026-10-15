import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import {
  contactHeaders,
  createContact,
  expectContacts,
  openContacts,
} from '../lib/contacts.ts'

// Spec 051 US4-1 to US4-5 (quickstart `contacts-card`): two vaults. Ben chooses his own contact
// and an e-mail address for his Visitenkarte and opens it; Mara pastes the link, sees the preview
// (nothing is stored yet) and takes it as a new contact with confirmed holzi addresses. A changed
// link is refused; Ben's newer card replaces the addresses, his older one changes nothing. A holzi
// address typed in by hand shows as unconfirmed. US4-6 (the phone keeps holzi addresses) is the
// phone sync's rule, covered by its tests (FR-014).

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

scenario('contacts-card', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({
    users: { ben: ['laptop'], mara: ['desktop'] },
  })
  const ben = group.device('ben/laptop').page
  const mara = group.device('mara/desktop').page

  const own = await createContact(ben, {
    head: { givenName: 'Ben', familyName: 'Krüger' },
    values: [
      { kind: 'email', label: 'work', value: 'ben@example.org' },
      { kind: 'phone', label: 'mobile', value: '0170 1111' },
    ],
    groups: [],
  })
  const ownContact = unwrap<{ values: Array<{ id: string; kind: string }> }>(
    'contacts_get',
    await ben.invoke('contacts_get', { args: { id: own } }),
  )
  const email = ownContact.values.find((value) => value.kind === 'email')!.id
  unwrap(
    'contacts_own_card_set',
    await ben.invoke('contacts_own_card_set', {
      args: { ownContact: own, valueIds: [email] },
    }),
  )
  await openContacts(ben)
  await ben.click('contacts-card-menu')
  await ben.click('contacts-card-menu-own')
  await ben.waitForDisplayed('contacts-own-card-qr')
  const older = await ben.exec<string>(
    `return document.querySelector('[data-testid="contacts-own-card-text"]')?.textContent?.trim() ?? ''`,
  )
  assert.ok(older.startsWith('holzi:card:'), older)
  ctx.step('Ben shows his card as QR code and link')

  await openContacts(mara)
  await mara.click('contacts-card-menu')
  await mara.click('contacts-card-menu-import')
  await mara.waitForDisplayed('contacts-card-text')
  await mara.type('contacts-card-text', older)
  await mara.click('contacts-card-check')
  await mara.waitForDisplayed('contacts-card-preview')
  const shown = await mara.exec<string>(
    `return document.querySelector('[data-testid="contacts-card-preview-name"]')?.textContent ?? ''`,
  )
  assert.ok(shown.includes('Ben Krüger'), shown)
  assert.equal((await contactHeaders(mara)).length, 0, 'nothing stored yet')
  ctx.step('Mara sees the preview before anything is stored')

  await mara.click('contacts-card-new')
  await mara.waitForDisplayed('contacts-card-addresses', 15_000)
  await expectContacts(ctx, mara, ['Ben Krüger'], 'Ben in the address book')
  const [header] = await contactHeaders(mara)
  const accepted = unwrap<{
    origin: { kind: string }
    values: Array<{ value: string }>
    card: { devices: unknown[]; issuedAt: string } | null
  }>(
    'contacts_get',
    await mara.invoke('contacts_get', { args: { id: header!.id } }),
  )
  assert.equal(accepted.origin.kind, 'card')
  assert.deepEqual(
    accepted.values.map((value) => value.value),
    ['ben@example.org'],
    'only what Ben put on the card',
  )
  assert.equal(accepted.card?.devices.length, 1)
  ctx.step('the card became a contact with confirmed holzi addresses')

  const chars = [...older]
  const at = chars.length - 12
  chars[at] = chars[at] === 'A' ? 'B' : 'A'
  assert.equal(
    await refusal(mara, 'contacts_card_preview', { text: chars.join('') }),
    'CardInvalid',
  )
  ctx.step('a changed card is refused')

  unwrap(
    'contacts_own_card_set',
    await ben.invoke('contacts_own_card_set', {
      args: { ownContact: own, valueIds: [] },
    }),
  )
  const newer = unwrap<{ text: string }>(
    'contacts_own_card',
    await ben.invoke('contacts_own_card'),
  ).text
  assert.notEqual(newer, older)
  const preview = unwrap<{ existingContact: { id: string } | null }>(
    'contacts_card_preview',
    await mara.invoke('contacts_card_preview', { args: { text: newer } }),
  )
  assert.equal(preview.existingContact?.id, header!.id, 'the same contact')
  const updated = unwrap<{ card: { issuedAt: string } }>(
    'contacts_card_accept',
    await mara.invoke('contacts_card_accept', {
      args: { text: newer, into: { kind: 'contact', contactId: header!.id } },
    }),
  )
  assert.ok(updated.card.issuedAt > accepted.card!.issuedAt)
  assert.equal(
    await refusal(mara, 'contacts_card_accept', {
      text: older,
      into: { kind: 'contact', contactId: header!.id },
    }),
    'CardOlder',
  )
  ctx.step('a newer card replaces the addresses, an older one is refused')

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
