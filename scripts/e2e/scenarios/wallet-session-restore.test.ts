import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { activeTab, createPass, selectTab } from '../lib/passwords.ts'
import { setSessionRestore } from '../lib/settings.ts'
import { openWallet } from '../lib/wallet.ts'

const MARKER = 'SECRET-MARKER-E2E-WALLET-RESTORE'

// Spec 092, FR-040 (quickstart): with session restore on, a pass open in Wallet comes back after a
// restart at the same place and tab, the stored session names it by its id and holds neither a code
// nor a title, and a code that was shown large is not shown again (it ended when the person left).
scenario('wallet-session-restore', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const device = group.device('anna/laptop')
  const page = device.page

  await setSessionRestore(page, true)
  const id = await createPass(page, {
    title: 'Restore this ticket',
    codes: [{ format: 'qr', value: MARKER }],
  })
  await openWallet(page)
  await page.waitForDisplayed(`wallet-pass-${id}`)
  await page.click(`wallet-pass-${id}`)
  await page.waitForDisplayed('wallet-code-image-0')
  await page.click('wallet-show-large-0')
  await page.waitForDisplayed('wallet-large')
  ctx.step('code shown large')

  // Leaving ends the large view; the tab is part of the place.
  await page.click('wallet-large-close')
  await selectTab(page, 'extra')
  await ctx.waitFor(
    'the Extra tab',
    async () => (await activeTab(page)) === 'extra',
  )
  const saved = (await ctx.waitFor(
    'the saved session to hold the pass place',
    async () => {
      const text = JSON.stringify(await page.invoke('wm_session_load'))
      return text.includes(`/entry/${id}`) && text.includes('extra')
        ? text
        : false
    },
    { fixed: true, timeoutMs: 15_000 },
  )) as string
  assert.ok(!saved.includes(MARKER), 'the saved session holds a code')
  assert.ok(
    !saved.includes('Restore this ticket'),
    'the saved session holds a title',
  )
  ctx.step('session saved without a value')

  await device.restart()
  const restored = device.page
  await restored.waitForDisplayed('wallet-title')
  await ctx.waitFor(
    'the Extra tab to come back',
    async () => (await activeTab(restored)) === 'extra',
    { fixed: true, timeoutMs: 15_000 },
  )
  const text = await restored.exec<string>('return document.body.innerText')
  assert.ok(text.includes('Restore this ticket'), 'the pass is shown again')
  assert.ok(!text.includes(MARKER), 'a code is shown as text after a restart')
  assert.equal(
    await restored.exec<boolean>(
      `return document.querySelector('[data-testid="wallet-large"]') === null`,
    ),
    true,
    'the large code is open again after a restart',
  )
  ctx.step('restored at the same place, no code on screen')
})
