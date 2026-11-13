import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { createFolder, createPass } from '../lib/passwords.ts'
import { openWallet } from '../lib/wallet.ts'
import type { FlowInstance } from '../lib/flows.ts'

/** The ids of the passes the list shows, in order. */
async function rows(instance: FlowInstance): Promise<string[]> {
  return await instance.exec<string[]>(
    `return [...document.querySelectorAll('button[data-testid^="wallet-pass-"]')].map((b) => b.getAttribute('data-testid').slice('wallet-pass-'.length))`,
  )
}

// Spec 092, US4 in Wallet: with tickets of two providers and loyalty cards, the list sorts by date
// (the next event first, over ones after it, undated last) or by title, filters by kind, hides what is
// over, and the search finds a ticket by a word of its folder.
scenario('wallet-organize', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-wallet-organize' })

  const tickets = await createFolder(instance, 'Tickets', 'wallet')
  const bahn = await createFolder(instance, 'Deutsche Bahn', tickets)
  const soon = await createPass(
    instance,
    {
      title: 'ICE Berlin',
      start: { at: '2030-11-14T08:00:00+00:00', zone: 'Europe/Berlin' },
      expiresAt: '2030-11-14',
    },
    bahn,
  )
  const later = await createPass(
    instance,
    {
      title: 'ICE Munich',
      start: { at: '2031-01-20T08:00:00+00:00', zone: 'Europe/Berlin' },
    },
    bahn,
  )
  const over = await createPass(
    instance,
    {
      title: 'Old ride',
      start: { at: '2020-01-01T08:00:00+00:00', zone: 'Europe/Berlin' },
      expiresAt: '2020-01-01',
    },
    bahn,
  )
  const card = await createPass(instance, {
    title: 'Bonus card',
    passKind: 'loyalty_card',
  })
  const voucher = await createPass(instance, {
    title: 'Coffee voucher',
    passKind: 'voucher',
    expiresAt: '2031-06-30',
  })

  await openWallet(instance)
  await instance.waitForDisplayed(`wallet-pass-${soon}`)

  // Date order: upcoming nearest first (the voucher has no start, it is dated by its end), then
  // over, then undated (the card).
  assert.deepEqual(await rows(instance), [soon, later, voucher, over, card])
  ctx.step('sorted by date')

  // By title.
  await instance.click('wallet-filters')
  await instance.click('wallet-sort-title')
  await ctx.waitFor(
    'the list to sort by title',
    async () =>
      (await rows(instance)).join() ===
      [card, voucher, soon, later, over].join(),
  )
  ctx.step('sorted by title')

  // Hide expired: the ride that is over leaves.
  await instance.click('wallet-filters')
  await instance.click('wallet-hide-expired')
  await ctx.waitFor(
    'the expired pass to leave',
    async () => !(await rows(instance)).includes(over),
  )
  ctx.step('expired hidden')

  // Kind: only vouchers.
  await instance.click('wallet-filters')
  await instance.click('wallet-kind-voucher')
  await ctx.waitFor(
    'the list to show only the voucher',
    async () => (await rows(instance)).join() === voucher,
  )
  await instance.click('wallet-filters')
  await instance.click('wallet-kind-all')
  ctx.step('filtered by kind')

  // Search by a word of the folder: the two tickets of the provider, not the cards.
  await instance.type('wallet-search', 'bahn')
  await ctx.waitFor(
    'the search to find the tickets by their folder',
    async () => {
      const found = await rows(instance)
      return (
        found.includes(soon) &&
        found.includes(later) &&
        !found.includes(card) &&
        !found.includes(voucher)
      )
    },
  )
  ctx.step('searched by folder')
})
