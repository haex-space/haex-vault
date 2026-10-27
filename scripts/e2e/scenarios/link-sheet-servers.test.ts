import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import {
  addServer,
  serverRows,
  switchOffDefaultServers,
} from '../lib/sync-ui.ts'

// Spec 024, FR-008, FR-023: the link form on the start page lists the Nostr-Relays and the
// iroh-Relays: the built-in ones, which can be switched off but not removed, and added ones, which
// can be switched off or removed. A main device with its own servers cannot tell a new installation
// about them, so this is the only place to name them there.
scenario('link-sheet-servers', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  const rows = (kind: string, enabled?: boolean) =>
    instance.exec<number>(
      `return document.querySelectorAll('[data-testid="link-servers-nostr-${kind}"]${
        enabled === undefined ? '' : `[data-enabled="${enabled}"]`
      }').length`,
    )

  await instance.click('landing-link')
  await instance.waitForDisplayed('link-servers-nostr-input')
  await ctx.waitFor('the built-in Nostr servers to load', async () => {
    return (await rows('default', true)) === 3
  })
  assert.equal(await rows('added'), 0)
  ctx.step('the three built-in Nostr servers are listed, all in use')

  await instance.type('link-servers-nostr-input', 'wss://relay.example.org')
  await instance.click('link-servers-nostr-add')
  await ctx.waitFor('the added server to be listed', async () => {
    return (await rows('added', true)) === 1
  })
  assert.equal(await rows('default'), 3)
  ctx.step('an added server is listed after them')

  await instance.click('link-servers-nostr-toggle')
  await ctx.waitFor('the first server to be switched off', async () => {
    return (await rows('default', false)) === 1
  })
  assert.equal(await rows('default'), 3)
  ctx.step('switching one off keeps it listed')

  await instance.click('link-servers-nostr-remove')
  await ctx.waitFor('the added server to be deleted', async () => {
    return (await rows('added')) === 0
  })
  assert.equal(await rows('default'), 3)
  assert.equal(await rows('default', false), 1)
  ctx.step('deleting the added server leaves the built-in ones')

  // The iroh-Relays the same way: a main device with its own ones has no other way to name them.
  const builtIn = unwrap<{ irohRelays: string[] }>(
    'sync_servers_defaults',
    await instance.invoke('sync_servers_defaults'),
  ).irohRelays.length
  assert.ok(builtIn > 0, 'holzi ships iroh-Relays')
  const irohRows = (kind: 'default' | 'added', enabled?: boolean) =>
    serverRows(instance, 'link-servers-iroh', kind, enabled)
  assert.equal(await irohRows('default', true), builtIn)
  await addServer(instance, 'link-servers-iroh', 'https://iroh.example.org')
  await ctx.waitFor('the added iroh-Relay to be listed', async () => {
    return (await irohRows('added', true)) === 1
  })
  assert.equal(
    await switchOffDefaultServers(instance, 'link-servers-iroh'),
    builtIn,
  )
  await ctx.waitFor('the built-in iroh-Relays to be switched off', async () => {
    return (await irohRows('default', false)) === builtIn
  })
  assert.equal(await rows('default', false), 1, 'the Nostr list is unchanged')
  ctx.step('the iroh-Relays are listed, added and switched off the same way')
})
