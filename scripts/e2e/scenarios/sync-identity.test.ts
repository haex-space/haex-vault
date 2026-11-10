import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { successToasts } from '../lib/toasts.ts'
import {
  exists,
  npubIsEditable,
  openFederation,
  shownNpub,
} from '../lib/sync-ui.ts'

// Spec 024, user story 4 (M3 of its quickstart, FR-046, D26): both devices of a vault show the same
// public vault identity, it can be copied and not changed, and a linked device has no way to link or
// remove devices, while a main device has.
scenario('sync-identity', { timeoutMs: 240_000 }, async (ctx) => {
  const g = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
  const laptop = g.device('anna/laptop')
  const phone = g.device('anna/phone')

  await openFederation(laptop.page)
  await openFederation(phone.page)
  const [onLaptop, onPhone] = [
    await shownNpub(laptop.page),
    await shownNpub(phone.page),
  ]
  assert.match(onLaptop, /^npub1[0-9a-z]+$/)
  assert.equal(onPhone, onLaptop, 'both devices show the same vault identity')
  assert.equal((await laptop.identity()).npub, onLaptop)
  ctx.step('both show the same npub', onLaptop)

  for (const device of [laptop, phone]) {
    assert.equal(
      await npubIsEditable(device.page),
      false,
      `${device.address}: the identity cannot be changed`,
    )
  }
  await laptop.page.click('settings-identity-copy')
  await ctx.waitFor(
    'a toast to say it copied',
    async () => (await successToasts(laptop.page)).length > 0,
  )
  ctx.step('copied')

  assert.equal(await exists(laptop.page, 'settings-link-device'), true)
  assert.equal(await exists(phone.page, 'settings-link-device'), false)
  assert.equal(await exists(phone.page, 'settings-device-remove'), false)
  assert.equal(await exists(phone.page, 'settings-main-only'), true)
  assert.equal(await exists(laptop.page, 'settings-device-remove'), true)
  ctx.step('only the main device can link and remove')
})
