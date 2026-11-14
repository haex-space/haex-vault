import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { createPass, overview } from '../lib/passwords.ts'
import { openWallet } from '../lib/wallet.ts'

const CODE = 'SECRET-MARKER-E2E-WALLET-SYNC'

/** The title the overview of `page` holds for the pass `id`. */
async function titleOf(
  page: Parameters<typeof overview>[0],
  id: string,
): Promise<string | null | undefined> {
  return (await overview(page)).headers.find((h) => h.id === id)?.title
}

// Spec 092, FR-041: two devices of one user keep the passes in step, and a change from the other
// device never overwrites an edit in progress. A pass made on the laptop shows on the phone; the
// phone opens the editor and types a title; the laptop then changes the pass: the phone has the
// change in its data, and the draft in the field is still what was typed.
scenario('wallet-sync-two-devices', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
  const laptop = group.device('anna/laptop').page
  const phone = group.device('anna/phone').page

  const id = await createPass(laptop, {
    title: 'Concert',
    codes: [{ format: 'qr', value: CODE }],
  })
  await ctx.waitFor(
    'the pass to show on the phone',
    async () => (await titleOf(phone, id)) === 'Concert',
    { timeoutMs: 40_000, fixed: true },
  )
  ctx.step('created on one device, shown on the other')

  await openWallet(phone)
  await phone.waitForDisplayed(`wallet-pass-${id}`)
  await phone.click(`wallet-pass-${id}`)
  await phone.click('wallet-edit')
  await phone.waitForDisplayed('passwords-editor')
  await phone.type('passwords-field-title', ' (my draft)')
  const draft = () =>
    phone.exec<string>(
      `const el = document.querySelector('[data-testid="passwords-field-title"]')
       const input = el?.matches('input') ? el : el?.querySelector('input')
       return input?.value ?? ''`,
    )
  assert.ok((await draft()).includes('(my draft)'))
  ctx.step('draft typed on the phone')

  // The laptop changes the pass; the phone gets it while its editor is open.
  const current = unwrap<{ updatedAt: string }>(
    'passwords_get_item',
    await laptop.invoke('passwords_get_item', { args: { itemId: id } }),
  )
  unwrap(
    'passwords_pass_update',
    await laptop.invoke('passwords_pass_update', {
      args: {
        id,
        updatedAt: current.updatedAt,
        patch: { title: 'Concert (moved)' },
      },
    }),
  )
  await ctx.waitFor(
    'the change to reach the phone',
    async () => (await titleOf(phone, id)) === 'Concert (moved)',
    { timeoutMs: 40_000, fixed: true },
  )
  const kept = await draft()
  assert.ok(kept.includes('(my draft)'), `the draft was overwritten: ${kept}`)
  const text = await phone.exec<string>('return document.body.innerText')
  assert.ok(!text.includes(CODE), 'a code value is on the screen')
  ctx.step('the change arrived and the draft is untouched')
})
