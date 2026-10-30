import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { PRESET_ID, marketplaces } from '../lib/marketplace.ts'

type Device = Parameters<typeof marketplaces>[0]

const names = async (device: Device) =>
  (await marketplaces(device)).map((market) => market.name).sort()

// Spec 069, SC-007 (quickstart M11): the marketplaces are rows of the vault, so two devices of one
// user keep them in step. A marketplace added on one shows on the other; switching the public one
// off on one leaves it off on the other (a device does not turn it on again); deleting it on one
// removes it on both and no device brings it back. The devices come from the group API of spec 033.
scenario('marketplace-two-devices', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
  const laptop = group.device('anna/laptop').page
  const phone = group.device('anna/phone').page

  const preset = (device: Device) =>
    marketplaces(device).then((all) =>
      all.find((market) => market.id === PRESET_ID),
    )
  await ctx.waitFor(
    'both devices to hold the public marketplace once',
    async () =>
      (await marketplaces(laptop)).length === 1 &&
      (await marketplaces(phone)).length === 1 &&
      (await preset(phone)) !== undefined,
    { timeoutMs: 40_000, fixed: true },
  )
  ctx.step('both devices hold the one public marketplace')

  unwrap(
    'marketplace_save',
    await laptop.invoke('marketplace_save', {
      input: {
        name: 'Team',
        baseUrl: 'https://market.example/team',
        authType: 'none',
      },
    }),
  )
  await ctx.waitFor(
    'the new marketplace to reach the phone',
    async () => (await names(phone)).includes('Team'),
    { timeoutMs: 40_000, fixed: true },
  )
  ctx.step('a marketplace added on one device shows on the other')

  unwrap(
    'marketplace_set_enabled',
    await laptop.invoke('marketplace_set_enabled', {
      id: PRESET_ID,
      enabled: false,
    }),
  )
  await ctx.waitFor(
    'the switch to reach the phone',
    async () => (await preset(phone))?.enabled === false,
    { timeoutMs: 40_000, fixed: true },
  )
  // Another change travels afterwards; it must not bring the switch back.
  unwrap(
    'marketplace_save',
    await phone.invoke('marketplace_save', {
      input: {
        name: 'Second',
        baseUrl: 'https://second.example',
        authType: 'none',
      },
    }),
  )
  await ctx.waitFor(
    'the second marketplace to reach the laptop',
    async () => (await names(laptop)).includes('Second'),
    { timeoutMs: 40_000, fixed: true },
  )
  assert.equal((await preset(laptop))?.enabled, false)
  assert.equal((await preset(phone))?.enabled, false)
  ctx.step('the public marketplace switched off stays off on both')

  unwrap(
    'marketplace_delete',
    await laptop.invoke('marketplace_delete', { id: PRESET_ID }),
  )
  await ctx.waitFor(
    'the deletion to reach the phone',
    async () => (await preset(phone)) === undefined,
    { timeoutMs: 40_000, fixed: true },
  )
  unwrap(
    'marketplace_save',
    await laptop.invoke('marketplace_save', {
      input: {
        name: 'Third',
        baseUrl: 'https://third.example',
        authType: 'none',
      },
    }),
  )
  await ctx.waitFor(
    'a later change to reach the phone',
    async () => (await names(phone)).includes('Third'),
    { timeoutMs: 40_000, fixed: true },
  )
  assert.equal(
    await preset(phone),
    undefined,
    'the phone did not bring it back',
  )
  assert.equal(await preset(laptop), undefined, 'nor did the laptop')
  ctx.step('the public marketplace deleted on one device is gone on both')
})
