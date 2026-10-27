import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { expectOnline } from '../lib/group-expect.ts'
import { addThread, expectThreads, NO_IROH_RELAY } from '../lib/sync-flows.ts'
import {
  deviceRows,
  linkProgress,
  openFederation,
  openLinkedVault,
  showLinkCode,
  submitLinkForm,
} from '../lib/sync-ui.ts'

// Spec 024, user story 5 (M1 of its quickstart, SC-009, SC-011): linking a new device the way a person
// does it. The main device shows a code in its window; the new installation is a second application
// process whose start page takes the code, a device name, a vault name, a passphrase and the test relay.
// The main device shows the new device's name and asks for the role, left at "no"; after "Verknüpfen"
// the new device opens the vault with all of its data, and both list it as a linked device, online. A
// device that is refused gets nothing and keeps no vault.
scenario('sync-link', { timeoutMs: 360_000 }, async (ctx) => {
  const g = await ctx.group({ users: { anna: ['laptop'] } })
  const laptop = g.device('anna/laptop')
  await addThread(laptop, 'vor dem Verknüpfen')
  await openFederation(laptop.page)

  const join = async (name: string) => {
    const device = g.addDevice('anna', name)
    await device.startUnopened()
    await submitLinkForm(device.page, {
      code: await showLinkCode(laptop.page),
      deviceName: name,
      vaultName: device.vaultName,
      passphrase: device.passphrase,
      relayUrl: g.relay.url,
      irohRelays: g.irohRelay ? [g.irohRelay.url] : NO_IROH_RELAY,
    })
    return device
  }

  // A new device that is agreed to.
  const phone = await join('phone')
  await laptop.page.waitForDisplayed('link-confirm', 40_000)
  assert.ok(
    await laptop.page.exec<boolean>(
      `return document.body.textContent.includes('phone')`,
    ),
    'the main device shows the name of the new device',
  )
  assert.equal(
    await laptop.page.exec<boolean>(
      `return document.querySelector('[data-testid="link-role-linked"]').getAttribute('aria-checked') === 'true' && document.querySelector('[data-testid="link-role-main"]').getAttribute('aria-checked') === 'false'`,
    ),
    true,
    'the role question is answered "no" (a linked device) by default',
  )
  await laptop.page.click('link-confirm')
  await ctx.waitFor(
    'the new device to get the vault',
    async () => (await linkProgress(phone.page)) === 'done',
    { timeoutMs: 40_000, fixed: true },
  )
  await openLinkedVault(phone.page, phone.passphrase)
  await expectThreads(
    ctx,
    phone,
    ['vor dem Verknüpfen'],
    'the new device to show the data of the vault',
  )
  ctx.step('agreed to, the vault arrived with its data')

  await openFederation(phone.page)
  await openFederation(laptop.page)
  await expectOnline(ctx, laptop, phone, true)
  await expectOnline(ctx, phone, laptop, true)
  await ctx.waitFor(
    'both device lists to name both devices as such',
    async () => {
      const [mine, theirs] = [
        await deviceRows(laptop.page),
        await deviceRows(phone.page),
      ]
      return (
        mine.some(
          (row) => !row.current && row.role === 'linked' && row.online,
        ) &&
        theirs.some((row) => row.current && row.role === 'linked') &&
        theirs.some((row) => !row.current && row.role === 'main' && row.online)
      )
    },
    { timeoutMs: 40_000, fixed: true },
  )
  ctx.step('both list the new device as a linked device, online')

  // A new device that is refused.
  const stranger = await join('stranger')
  await laptop.page.waitForDisplayed('link-reject', 40_000)
  await laptop.page.click('link-reject')
  await ctx.waitFor(
    'the refused device to hear it was refused',
    async () => (await linkProgress(stranger.page)) === 'failed',
    { timeoutMs: 40_000, fixed: true },
  )
  assert.deepEqual(
    unwrap<{ state: string; reason?: string }>(
      'link_join_status',
      await stranger.page.invoke('link_join_status'),
    ),
    { state: 'failed', reason: 'rejected' },
    'the stranger hears that the link was rejected',
  )
  assert.deepEqual(
    unwrap<unknown[]>(
      'list_instances',
      await stranger.page.invoke('list_instances'),
    ),
    [],
    'a refused device keeps no vault',
  )
  ctx.step('refused, nothing left on the new device')
})
