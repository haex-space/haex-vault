import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { fixture } from '../lib/extensions.ts'
import {
  PRESET_ID,
  enabled,
  installFromCatalog,
  installedVersions,
  openMarketplace,
  pointPresetAt,
  startMarketplace,
  textOf,
  tileSlugs,
} from '../lib/marketplace.ts'
import { openSettings } from '../lib/settings.ts'

interface Known {
  candidates: Array<{ installedName: string; sourceId: string; slug: string }>
  selected: Array<{ installedName: string; latestCompatible: string }>
}

// Spec 069, US3 (quickstart M7, M8, M9): updates and removal from the catalog. With 1.0.0 installed
// and 1.1.0 served, the tile says "Update verfügbar" and so does the extensions list of the
// settings, even while the catalog shows a search that does not contain the extension; a newest
// version that does not fit this HaexVault is locked and offers no update; the update runs through
// the preview ("Update von 1.0.0 auf 1.1.0"); an older version asks for the downgrade
// confirmation; "Entfernen" in the details removes after the data question.
scenario('marketplace-update', { timeoutMs: 420_000 }, async (ctx) => {
  const market = await startMarketplace()
  ctx.onTeardown(() => market.close())
  const restore = pointPresetAt(market.url)
  ctx.onTeardown(restore)
  const probe = market.extensions.find(
    (extension) => extension.slug === 'probe',
  )
  assert.ok(probe)

  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-marketplace-update' })
  await openMarketplace(instance)
  await instance.waitForDisplayed(`marketplace-install-${PRESET_ID}-probe`)
  await installFromCatalog(instance, 'probe')
  assert.deepEqual(await installedVersions(instance), { probe: '1.0.0' })
  ctx.step('1.0.0 is installed')

  // 1.1.0 appears but does not fit this HaexVault: locked, no update offered.
  probe.versions.unshift({
    version: '1.1.0',
    file: fixture('e2e', 'probe-v2'),
    minAppVersion: '99.0.0',
  })
  await instance.click('marketplace-refresh')
  await ctx.waitFor(
    'the tile to know the new version',
    async () => market.hits('/extensions') > 1,
  )
  await instance.click(
    `li[data-testid="marketplace-tile-${PRESET_ID}-probe"] button`,
  )
  await instance.waitForDisplayed('marketplace-version-1.1.0')
  assert.equal(
    await enabled(instance, 'marketplace-install-version-1.1.0'),
    false,
  )
  assert.equal(
    await instance.exec<boolean>(
      `return !!document.querySelector('[data-testid="marketplace-update-${PRESET_ID}-probe"]')`,
    ),
    false,
    'an update that does not fit is not offered',
  )
  ctx.step('a version that does not fit is locked and offers no update')

  // It fits now: the update is offered, also when the catalog shows something else.
  delete probe.versions[0]!.minAppVersion
  await instance.click('marketplace-back')
  await instance.type('marketplace-search', 'filler-01')
  await ctx.waitFor('the search result', async () => {
    const slugs = await tileSlugs(instance)
    return slugs.length === 1 && slugs[0] === `${PRESET_ID}-filler-01`
  })
  await instance.click('marketplace-refresh')
  await ctx.waitFor('the host to know the update', async () => {
    const known = unwrap<Known>(
      'marketplace_known_updates',
      await instance.invoke('marketplace_known_updates'),
    )
    return known.selected.some(
      (update) =>
        update.installedName === 'probe' && update.latestCompatible === '1.1.0',
    )
  })
  ctx.step('the update is known although the catalog shows another entry')

  // The settings list shows the hint and it leads into the catalog, to that entry.
  const extensionId = unwrap<Array<{ id: string; name: string }>>(
    'extension_list',
    await instance.invoke('extension_list'),
  ).find((extension) => extension.name === 'probe')!.id
  await openSettings(instance)
  await instance.click('settings-category-extensions')
  await instance.waitForDisplayed(
    `extension-update-hint-${extensionId}`,
    15_000,
  )
  await instance.click(`extension-update-hint-${extensionId}`)
  await instance.waitForDisplayed('marketplace-details', 15_000)
  assert.match(
    await textOf(instance, 'marketplace-details-source'),
    /Haex Marketplace/,
  )
  ctx.step('the hint in the settings opens the entry')

  // The update runs through the preview and names both versions.
  await instance.click(`marketplace-update-${PRESET_ID}-probe`)
  await instance.waitForDisplayed('extensions-install-dialog', 30_000)
  const preview = await instance.exec<string>(
    `return document.querySelector('[data-testid="extensions-install-dialog"]')?.textContent ?? ''`,
  )
  assert.match(preview, /1\.0\.0/)
  assert.match(preview, /1\.1\.0/)
  await instance.click('extensions-install-confirm')
  await ctx.waitFor(
    'the update to be installed',
    async () => (await installedVersions(instance)).probe === '1.1.0',
  )
  assert.equal(
    await instance.exec<boolean>(
      `return !!document.querySelector('[data-testid="marketplace-update-${PRESET_ID}-probe"]')`,
    ),
    false,
    'the update mark goes away',
  )
  ctx.step('the update installed through the preview')

  // An older version asks for the downgrade confirmation.
  await instance.click(`marketplace-install-version-1.0.0`)
  await instance.waitForDisplayed('extensions-install-dialog', 30_000)
  assert.equal(await enabled(instance, 'extensions-install-confirm'), false)
  await instance.click('extensions-install-cancel')
  assert.deepEqual(await installedVersions(instance), { probe: '1.1.0' })
  ctx.step('an older version needs the downgrade confirmation')

  // Removal from the details asks whether the data stays.
  await instance.click(`marketplace-remove-${PRESET_ID}-probe`)
  await instance.waitForDisplayed('extension-remove-delete')
  await instance.click('extension-remove-delete')
  await ctx.waitFor(
    'the extension to be gone',
    async () => (await installedVersions(instance)).probe === undefined,
  )
  await instance.waitForDisplayed(
    `marketplace-install-${PRESET_ID}-probe`,
    15_000,
  )
  ctx.step('removed from the details')
})
