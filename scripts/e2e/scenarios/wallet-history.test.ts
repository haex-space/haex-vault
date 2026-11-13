import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  activeTab,
  createPass,
  selectTab,
  slideShows,
} from '../lib/passwords.ts'
import { KEY } from '../lib/settings.ts'
import { openWallet } from '../lib/wallet.ts'

const CODE = 'SECRET-MARKER-E2E-PASS-HISTORY'

// Spec 092, US5 (FR-032): the Verlauf tab of a pass shows what changed when the organiser moves the
// date. The state lists kind, start and the code with its format; the code value is not on the
// screen until "Show" is pressed, and it hides again when the tab is left.
scenario('wallet-history', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-wallet-history' })
  await openWallet(instance)

  const id = await createPass(instance, {
    title: 'ICE Berlin',
    start: { date: '2030-11-14' },
    codes: [{ format: 'qr', value: CODE, label: 'Outbound' }],
  })
  const created = unwrap<{ updatedAt: string }>(
    'passwords_get_item',
    await instance.invoke('passwords_get_item', { args: { itemId: id } }),
  )
  unwrap(
    'passwords_pass_update',
    await instance.invoke('passwords_pass_update', {
      args: {
        id,
        updatedAt: created.updatedAt,
        patch: { start: { date: '2030-11-15' } },
      },
    }),
  )
  ctx.step('start moved')

  await instance.waitForDisplayed(`wallet-pass-${id}`)
  await instance.click(`wallet-pass-${id}`)
  await instance.waitForDisplayed('entry-tabs')
  await selectTab(instance, 'history')
  await ctx.waitFor(
    'the Verlauf tab',
    async () => (await activeTab(instance)) === 'history',
  )
  await ctx.waitFor('the Verlauf slide', async () =>
    slideShows(instance, 'history'),
  )
  await instance.waitForDisplayed('passwords-history-pass-start')
  await instance.waitForDisplayed('passwords-history-pass-kind')
  await instance.waitForDisplayed('passwords-history-pass-code-0')
  const start = await instance.exec<string>(
    `return document.querySelector('[data-testid="passwords-history-pass-start"]').innerText`,
  )
  assert.ok(
    start.includes('15'),
    `the newest state has the moved start: ${start}`,
  )
  const before = await instance.exec<string>(`return document.body.innerText`)
  assert.ok(!before.includes(CODE), 'no code value before "Show"')
  ctx.step('state shown')

  // "Show" reads the value from the stored state (the keyboard toggles it; a mouse press only
  // holds it); leaving the tab hides it again.
  const text = (testid: string) =>
    `return document.querySelector('[data-testid="${testid}"]').textContent`
  await instance.type('passwords-reveal-history-code', KEY.enter)
  await ctx.waitFor('the value to show', async () =>
    (
      await instance.exec<string>(text('passwords-value-history-code'))
    ).includes(CODE),
  )
  await selectTab(instance, 'details')
  await ctx.waitFor(
    'Details',
    async () => (await activeTab(instance)) === 'details',
  )
  await ctx.waitFor(
    'the value to hide again',
    async () =>
      !(
        await instance.exec<string>(text('passwords-value-history-code'))
      ).includes(CODE),
  )
  ctx.step('code revealed and hidden')
})
