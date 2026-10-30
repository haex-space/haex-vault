import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import {
  enabled,
  openMarketplace,
  pointPresetAt,
  repointPreset,
  startMarketplace,
  textOf,
} from '../lib/marketplace.ts'

const PRESET = 'preset-public'

const installed = async (instance: FlowInstance): Promise<string[]> =>
  unwrap<Array<{ name: string }>>(
    'extension_list',
    await instance.invoke('extension_list'),
  )
    .map((extension) => extension.name)
    .sort()

// Spec 069, US2 (quickstart M4, M5, M6): a bundle goes from the marketplace through the host into
// the one install preview of spec 017. The signed bundle installs after the preview names its
// marketplace and the tile says "Installiert"; closing the preview installs nothing; a bundle
// whose checksum or size differs from what the marketplace promised is refused with its message;
// a version without a checksum cannot be installed at all, and no download is asked for.
scenario('marketplace-install', { timeoutMs: 300_000 }, async (ctx) => {
  const market = await startMarketplace()
  ctx.onTeardown(() => market.close())
  const restore = pointPresetAt(market.url)
  ctx.onTeardown(restore)

  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-marketplace-install' })
  await repointPreset(instance, market.url)
  await openMarketplace(instance)
  await instance.waitForDisplayed(`marketplace-install-${PRESET}-probe`)
  assert.deepEqual(await installed(instance), [])

  // The signed bundle: preview with the marketplace named, then the install.
  await instance.click(`marketplace-install-${PRESET}-probe`)
  await instance.waitForDisplayed('extensions-install-dialog', 30_000)
  await instance.waitForDisplayed('extensions-install-origin')
  assert.match(
    await textOf(instance, 'extensions-install-origin'),
    /Haex Marketplace/,
  )
  assert.equal(
    await instance.exec<boolean>(
      `return !!document.querySelector('[data-testid="extensions-install-publisher-differs"]')`,
    ),
    false,
    'the bundle is signed with the key the marketplace lists',
  )
  assert.equal(
    await instance.exec<boolean>(
      `return !!document.querySelector('[data-testid="marketplace-install-error"]')`,
    ),
    false,
  )
  ctx.step('the preview names the marketplace')
  await instance.click('extensions-install-confirm')
  await instance.waitForDisplayed(
    `marketplace-installed-${PRESET}-probe`,
    30_000,
  )
  assert.deepEqual(await installed(instance), ['probe'])
  ctx.step('installed, and the tile says so without reopening the catalog')

  // Closing the preview installs nothing and drops the downloaded bytes.
  await instance.click(`marketplace-install-${PRESET}-agent-tools`)
  await instance.waitForDisplayed('extensions-install-dialog', 30_000)
  await instance.click('extensions-install-cancel')
  await ctx.waitFor(
    'the dialog to close',
    async () =>
      await instance.exec<boolean>(
        `return !document.querySelector('[data-testid="extensions-install-dialog"]')`,
      ),
  )
  assert.deepEqual(await installed(instance), ['probe'])
  ctx.step('closing the preview installs nothing')

  // A checksum that differs: refused, nothing installed.
  market.hash = 'wrong'
  await instance.click('marketplace-refresh')
  await instance.click(`marketplace-install-${PRESET}-agent-tools`)
  await instance.waitForDisplayed('marketplace-install-error', 30_000)
  assert.match(
    await textOf(instance, 'marketplace-install-error'),
    /stimmt nicht mit der Angabe des Marketplace/,
  )
  assert.deepEqual(await installed(instance), ['probe'])
  await instance.click('marketplace-install-error-dismiss')
  ctx.step('a wrong checksum is refused')

  // A size that differs.
  market.hash = 'ok'
  market.size = 'wrong'
  await instance.click('marketplace-refresh')
  await instance.click(`marketplace-install-${PRESET}-agent-tools`)
  await instance.waitForDisplayed('marketplace-install-error', 30_000)
  assert.match(
    await textOf(instance, 'marketplace-install-error'),
    /nicht die Größe/,
  )
  assert.deepEqual(await installed(instance), ['probe'])
  await instance.click('marketplace-install-error-dismiss')
  ctx.step('a wrong size is refused')

  // No checksum at all: the version cannot be installed and nothing is downloaded.
  market.size = 'ok'
  market.hash = 'missing'
  const downloads = market.hits('/extensions/agent-tools/download')
  await instance.click('marketplace-refresh')
  await instance.waitForDisplayed(
    `marketplace-blocked-${PRESET}-agent-tools`,
    15_000,
  )
  assert.equal(
    await enabled(instance, `marketplace-install-${PRESET}-agent-tools`),
    false,
  )
  assert.equal(market.hits('/extensions/agent-tools/download'), downloads)
  assert.deepEqual(await installed(instance), ['probe'])
  ctx.step('a version without a checksum is not offered')
})
