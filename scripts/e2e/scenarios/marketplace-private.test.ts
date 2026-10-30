import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { typeChecked } from '../lib/fields.ts'
import { createEntry, overview, trashEntry } from '../lib/passwords.ts'
import { choose, settingsLocation } from '../lib/settings.ts'
import { fixture } from '../lib/extensions.ts'
import {
  PRESET_ID,
  marketplaces,
  openMarketplace,
  pointPresetAt,
  repointPreset,
  startMarketplace,
} from '../lib/marketplace.ts'
import type { FlowInstance } from '../lib/flows.ts'

interface Page {
  entries: Array<{ slug: string; source: { id: string } }>
  sourceErrors: Array<{ sourceId: string; kind: string }>
}

const catalog = async (instance: FlowInstance) =>
  unwrap<Page>(
    'marketplace_catalog',
    await instance.invoke('marketplace_catalog', { query: { page: 1 } }),
  )

const MARKET_TAG = 'haex-marketplace'

// Spec 069, US5 (quickstart M12–M14): a private marketplace asks for a token. Without a login its
// source shows "login refused" and the public one is not disturbed; the token is an entry of the
// password manager that the user chooses in the form (it gets the tag, the table holds only its id)
// and the catalog loads with it; deleting that entry warns first and names the marketplace, then
// the source says the login is missing while the other source stays intact; a login typed in the
// form becomes an entry owned by the marketplace and goes with it; the form refuses a login with
// http://. The test marketplace listens on the loopback address, which a debug build takes for a
// login (research R15); the public one is the preset row.
scenario('marketplace-private', { timeoutMs: 480_000 }, async (ctx) => {
  const preset = await startMarketplace()
  ctx.onTeardown(() => preset.close())
  const priv = await startMarketplace({
    prefix: '/private',
    token: true,
    extensions: ['secret-one', 'secret-two'].map((slug, index) => ({
      slug,
      name: slug,
      description: `The ${slug} of the private marketplace`,
      category: 'tools',
      downloads: 50 - index,
      versions: [{ version: '1.0.0', file: fixture('e2e', 'probe') }],
    })),
  })
  ctx.onTeardown(() => priv.close())
  ctx.onTeardown(pointPresetAt(preset.url))
  const token = priv.token!

  const instance = await ctx.startInstance()
  await createAndUnlock(instance, {
    name: 'e2e-marketplace-private',
    passphrase: 'e2e-marketplace-passphrase',
  })
  await repointPreset(instance, preset.url)

  const privateRow = async () => {
    const row = (await marketplaces(instance)).find(
      (market) => market.name === 'Private',
    )
    assert.ok(row, 'the private marketplace is listed')
    return row
  }
  // The settings are a singleton: opened again from the launcher they keep their last place, which
  // is the list itself after a saved form, and start at "general" otherwise.
  const openList = async () => {
    await instance.click('open-launcher')
    await instance.click('[data-app-id="system.settings"]')
    await ctx.waitFor(
      'the settings to show a place',
      async () =>
        ['general', 'extensions.marketplaces'].includes(
          await settingsLocation(instance),
        ),
      { timeoutMs: 10_000, fixed: true },
    )
    if ((await settingsLocation(instance)) === 'general') {
      await instance.click('settings-category-extensions')
      await instance.click('extensions-marketplaces')
    }
    await instance.waitForDisplayed('settings-marketplaces')
  }

  // Added without a login (the form's switch stays on "Keine"): the server answers 401.
  await openList()
  await instance.click('settings-marketplace-add')
  await instance.waitForDisplayed('settings-marketplace-form')
  await instance.type('settings-marketplace-name', 'Private')
  await instance.type('settings-marketplace-url', priv.url)
  await instance.click('settings-marketplace-save')
  // The test fails (401), so the form shows why and offers to save anyway.
  await instance.waitForDisplayed('settings-marketplace-save-anyway', 20_000)
  await instance.click('settings-marketplace-save-anyway')
  await instance.waitForDisplayed('settings-marketplaces', 20_000)
  const added = await privateRow()
  await openMarketplace(instance)
  await instance.waitForDisplayed(
    `marketplace-source-error-${added.id}`,
    20_000,
  )
  const refused = await catalog(instance)
  assert.deepEqual(
    refused.sourceErrors.map((error) => [error.sourceId, error.kind]),
    [[added.id, 'auth_rejected']],
  )
  assert.ok(
    refused.entries.some((entry) => entry.source.id === PRESET_ID),
    'the public marketplace is not disturbed',
  )
  ctx.step(
    'without a login the private source says so and the other stays intact',
  )

  // The token is an entry of the password manager; the form chooses it.
  const entry = await createEntry(instance, {
    title: 'Private marketplace token',
    password: token,
  })
  await openList()
  await instance.click(`settings-marketplace-edit-${added.id}`)
  await instance.waitForDisplayed('settings-marketplace-form')
  await choose(instance, 'settings-marketplace-auth', 'bearer')
  await choose(instance, 'settings-marketplace-login-source', 'entry')
  await instance.waitForDisplayed(`storage-entry-option-${entry}`, 20_000)
  await instance.click(`storage-entry-option-${entry}`)
  const beforeLinkedRequests = priv.requests.length
  await instance.click('settings-marketplace-save')
  await instance.waitForDisplayed('settings-marketplaces', 20_000)
  const linked = await privateRow()
  assert.equal(linked.credentialsItemId, entry, 'the row points at the entry')
  assert.equal(linked.credentials, 'present')
  assert.equal(
    linked.credentialsEntry?.owned,
    false,
    'it stays the user’s entry',
  )
  assert.ok(
    priv.requests.some(
      (request) => request.authorization === `Bearer ${token}`,
    ),
    'the form tried the address with the token',
  )
  const loaded = await catalog(instance)
  assert.deepEqual(loaded.sourceErrors, [])
  assert.ok(loaded.entries.some((item) => item.source.id === added.id))
  const chosen = (await overview(instance)).headers.find((h) => h.id === entry)
  assert.ok(
    chosen?.tags.some((tag) => tag.name === MARKET_TAG),
    'the chosen entry carries the tag',
  )
  ctx.step(
    'the chosen entry is the login; the catalog of the private source loads',
  )

  // Only the signed address may go without the login: nothing but the catalog calls carried it.
  assert.ok(
    priv.requests
      .slice(beforeLinkedRequests)
      .filter((request) => !request.path.startsWith('/files/'))
      .every((request) => request.authorization === `Bearer ${token}`),
    'every call of the catalog carries the login',
  )

  // Deleting the entry warns first and names the marketplace, then the source says so.
  const usage = unwrap<{ features: string[] }>(
    'passwords_item_usage',
    await instance.invoke('passwords_item_usage', { args: { itemId: entry } }),
  )
  assert.deepEqual(usage.features, ['marketplace'])
  await trashEntry(instance, entry)
  await ctx.waitFor('the login to be missing', async () => {
    const row = await privateRow()
    return row.credentials === 'missing'
  })
  const missing = await catalog(instance)
  assert.deepEqual(
    missing.sourceErrors.map((error) => [error.sourceId, error.kind]),
    [[added.id, 'credentials_missing']],
  )
  assert.ok(
    missing.entries.some((item) => item.source.id === PRESET_ID),
    'the second source stays intact',
  )
  await openList()
  await instance.waitForDisplayed(
    `settings-marketplace-credentials-${added.id}`,
  )
  ctx.step(
    'deleting the entry warns, then the source says the login is missing',
  )

  // A login typed in the form is an entry owned by the marketplace.
  await instance.click(`settings-marketplace-edit-${added.id}`)
  await instance.waitForDisplayed('settings-marketplace-form')
  await instance.waitForDisplayed('settings-marketplace-secret', 20_000)
  await typeChecked(instance, 'settings-marketplace-secret', token)
  await instance.click('settings-marketplace-save')
  await instance.waitForDisplayed('settings-marketplaces', 20_000)
  const typed = await privateRow()
  assert.equal(typed.credentials, 'present')
  assert.notEqual(typed.credentialsItemId, entry)
  assert.equal(typed.credentialsEntry?.owned, true)
  assert.equal(typed.credentialsEntry?.title, 'Marketplace: Private')
  assert.deepEqual((await catalog(instance)).sourceErrors, [])
  ctx.step('a typed login becomes an entry owned by the marketplace')

  // The form refuses a login with http:// (US5-6).
  await instance.click(`settings-marketplace-edit-${added.id}`)
  await instance.waitForDisplayed('settings-marketplace-form')
  await typeChecked(
    instance,
    'settings-marketplace-url',
    'http://market.example',
  )
  await instance.waitForDisplayed('settings-marketplace-insecure')
  assert.equal(
    await instance.exec<boolean>(
      `return document.querySelector('[data-testid="settings-marketplace-save"]').disabled`,
    ),
    true,
    'the form does not save a login with http://',
  )
  await typeChecked(instance, 'settings-marketplace-url', priv.url)
  ctx.step('http:// with a login is refused in the form')

  // The entry the form created goes with the marketplace.
  await instance.click('settings-marketplace-delete')
  await instance.click('settings-marketplace-delete-confirm')
  await instance.waitForDisplayed('settings-marketplaces', 15_000)
  await ctx.waitFor('the login entry to be gone', async () =>
    (await overview(instance)).headers.every(
      (header) => header.id !== typed.credentialsItemId,
    ),
  )
  ctx.step('deleting the marketplace deletes the entry it created')
})
