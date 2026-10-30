import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { choose, openSettings, waitForLocation } from '../lib/settings.ts'
import { openVault } from '../lib/sync-flows.ts'
import { fixture } from '../lib/extensions.ts'
import {
  PRESET_ID,
  marketplaces,
  openMarketplace,
  pointPresetAt,
  repointPreset,
  startMarketplace,
  tileSlugs,
} from '../lib/marketplace.ts'

interface Page {
  entries: Array<{ slug: string; source: { id: string } }>
  sourceErrors: Array<{ sourceId: string }>
}

const catalog = async (instance: Parameters<typeof marketplaces>[0]) =>
  unwrap<Page>(
    'marketplace_catalog',
    await instance.invoke('marketplace_catalog', { query: { page: 1 } }),
  )

// Spec 069, US4 (quickstart M10, M11): every marketplace is an equal row. A second one behind a
// path prefix is added through the form and both serve the catalog at once, each tile naming its
// marketplace, and the marketplace filter narrows it to one; switching one off removes its entries
// without an error line; the order of the list is the order of the catalog; a marketplace is
// deleted through its form; the public one is deleted like any other, stays away after a restart,
// and only the button of the empty state brings it back, once.
scenario('marketplace-manage', { timeoutMs: 480_000 }, async (ctx) => {
  const preset = await startMarketplace()
  ctx.onTeardown(() => preset.close())
  const team = await startMarketplace({
    prefix: '/team',
    extensions: ['team-one', 'team-two'].map((slug, index) => ({
      slug,
      name: slug,
      description: `The ${slug} of the team marketplace`,
      category: 'tools',
      downloads: 50 - index,
      versions: [{ version: '1.0.0', file: fixture('e2e', 'probe') }],
    })),
  })
  ctx.onTeardown(() => team.close())
  ctx.onTeardown(pointPresetAt(preset.url))

  const name = 'e2e-marketplace-manage'
  const passphrase = 'e2e-marketplace-passphrase'
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name, passphrase })
  await repointPreset(instance, preset.url)

  // The form: name and address with a path prefix; the test passes, so it saves at once.
  await openSettings(instance)
  await instance.click('settings-category-extensions')
  await instance.click('extensions-marketplaces')
  await instance.waitForDisplayed('settings-marketplaces')
  await instance.waitForDisplayed(`settings-marketplace-row-${PRESET_ID}`)
  await instance.click('settings-marketplace-add')
  await instance.waitForDisplayed('settings-marketplace-form')
  await instance.type('settings-marketplace-name', 'Team')
  await instance.type('settings-marketplace-url', team.url)
  await instance.click('settings-marketplace-save')
  await instance.waitForDisplayed('settings-marketplaces', 20_000)
  const added = (await marketplaces(instance)).find(
    (market) => market.name === 'Team',
  )
  assert.ok(added, 'the marketplace was saved')
  assert.equal(
    added.baseUrl,
    team.url,
    'saved the way it was entered, normalized',
  )
  assert.ok(team.hits('/health') >= 1, 'the form tried the address first')
  await instance.waitForDisplayed(`settings-marketplace-row-${added.id}`)
  ctx.step('a second marketplace with a path prefix was added through the form')

  // Both serve the catalog at once, each tile with its source; the filter narrows it.
  await openMarketplace(instance)
  await instance.waitForDisplayed(`marketplace-tile-${PRESET_ID}-probe`)
  await instance.waitForDisplayed(`marketplace-tile-${added.id}-team-one`)
  await instance.waitForDisplayed('marketplace-source-filter')
  await choose(instance, 'marketplace-source-filter', added.id)
  await ctx.waitFor('only the team marketplace', async () => {
    const slugs = await tileSlugs(instance)
    return (
      slugs.length === 2 && slugs.every((slug) => slug.startsWith(added.id))
    )
  })
  ctx.step('both marketplaces serve one catalog and the filter narrows it')

  // Switched off: its entries are gone and nobody reports an error. The settings window is still
  // open on the marketplaces; the launcher brings it back where it was, not to "general".
  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.settings"]')
  await waitForLocation(instance, 'extensions.marketplaces')
  await instance.click(`settings-marketplace-toggle-${added.id}`)
  await ctx.waitFor('the marketplace to be off', async () => {
    const row = (await marketplaces(instance)).find((m) => m.id === added.id)
    return row?.enabled === false
  })
  const withoutTeam = await catalog(instance)
  assert.ok(withoutTeam.entries.every((entry) => entry.source.id === PRESET_ID))
  assert.deepEqual(withoutTeam.sourceErrors, [], 'off is not an error')
  const asked = team.requests.length
  await catalog(instance)
  assert.equal(
    team.requests.length,
    asked,
    'a switched-off marketplace is not asked',
  )
  await instance.click(`settings-marketplace-toggle-${added.id}`)
  await ctx.waitFor('the marketplace to be on again', async () => {
    const row = (await marketplaces(instance)).find((m) => m.id === added.id)
    return row?.enabled === true
  })
  ctx.step('switching off removes the entries without an error')

  // The order of the list is the order of the catalog.
  assert.equal((await catalog(instance)).entries[0]!.source.id, PRESET_ID)
  await instance.click(`settings-marketplace-up-${added.id}`)
  await ctx.waitFor('the team marketplace first', async () => {
    const [first] = await marketplaces(instance)
    return first?.id === added.id
  })
  assert.equal((await catalog(instance)).entries[0]!.source.id, added.id)
  ctx.step('the order of the list is the order of the catalog')

  // Deleted through its form, after the question.
  await instance.click(`settings-marketplace-edit-${added.id}`)
  await instance.waitForDisplayed('settings-marketplace-form')
  await instance.click('settings-marketplace-delete')
  await instance.click('settings-marketplace-delete-confirm')
  await instance.waitForDisplayed('settings-marketplaces', 15_000)
  await ctx.waitFor('the team marketplace to be gone', async () =>
    (await marketplaces(instance)).every((market) => market.id !== added.id),
  )
  ctx.step('deleted through its form')

  // The public one is deleted like any other and does not come back by itself.
  await instance.click(`settings-marketplace-edit-${PRESET_ID}`)
  await instance.waitForDisplayed('settings-marketplace-form')
  await instance.click('settings-marketplace-delete')
  await instance.click('settings-marketplace-delete-confirm')
  await instance.waitForDisplayed('settings-marketplaces-empty', 15_000)
  assert.deepEqual(await marketplaces(instance), [])
  await instance.stop()

  const next = await ctx.startInstance({ reusesRoot: instance.root })
  await openVault(next, name, passphrase)
  assert.deepEqual(
    await marketplaces(next),
    [],
    'opening the vault again does not recreate the public marketplace',
  )
  ctx.step('deleted for good, also after a restart')

  // The empty state offers it; one tap restores exactly one row, a second call changes nothing.
  await openMarketplace(next)
  await next.waitForDisplayed('marketplace-none')
  await next.waitForDisplayed('marketplace-empty-add-own')
  await next.click('marketplace-empty-add-preset')
  await ctx.waitFor(
    'the preset row',
    async () => (await marketplaces(next)).length === 1,
  )
  await repointPreset(next, preset.url)
  await next.waitForDisplayed(`marketplace-tile-${PRESET_ID}-probe`, 20_000)
  unwrap('marketplace_add_preset', await next.invoke('marketplace_add_preset'))
  assert.deepEqual(
    (await marketplaces(next)).map((market) => market.id),
    [PRESET_ID],
  )
  ctx.step('the empty state brings back exactly one public marketplace')
})
