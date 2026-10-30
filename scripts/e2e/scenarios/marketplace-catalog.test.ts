import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  openMarketplace,
  pointPresetAt,
  repointPreset,
  startMarketplace,
  textOf,
  tileCount,
  tileSlugs,
} from '../lib/marketplace.ts'

interface Row {
  id: string
  name: string
  baseUrl: string
  enabled: boolean
}

interface Page {
  entries: Array<{ slug: string; source: { id: string } }>
  totalPages: number
  sourceErrors: Array<{ sourceId: string; kind: string }>
}

const PRESET = 'preset-public'

// Spec 069, US1 (quickstart M1, M2, M3): the catalog app lists what the marketplace offers, one
// page at a time, every tile naming its marketplace; the search narrows it; the details open and
// go back; a marketplace that stops answering shows an error line and the "Keine Verbindung"
// state, and "Erneut versuchen" brings the entries back. A vault gets the preset marketplace when
// it is created; the test marketplace takes its place through `HOLZI_E2E_MARKETPLACE_URL`. The
// second marketplace next to an intact one is the scenario of the management part (spec 069 US4).
scenario('marketplace-catalog', { timeoutMs: 300_000 }, async (ctx) => {
  const market = await startMarketplace()
  ctx.onTeardown(() => market.close())
  const restore = pointPresetAt(market.url)
  ctx.onTeardown(restore)

  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-marketplace' })
  await repointPreset(instance, market.url)

  const rows = unwrap<Row[]>(
    'marketplace_list',
    await instance.invoke('marketplace_list'),
  )
  assert.deepEqual(
    rows.map((row) => [row.id, row.baseUrl, row.enabled]),
    [[PRESET, market.url, true]],
    'the new vault holds the preset, pointed at the test marketplace',
  )

  await openMarketplace(instance)
  await instance.waitForDisplayed(`marketplace-tile-${PRESET}-probe`)
  await instance.waitForDisplayed(`marketplace-tile-${PRESET}-agent-tools`)
  assert.equal(await tileCount(instance), 20)
  assert.match(
    await textOf(instance, `marketplace-tile-source-${PRESET}-probe`),
    /Haex Marketplace/,
  )
  ctx.step('both tiles show with their source')

  // Paging: 26 entries make two pages; the second holds the rest.
  await instance.click('marketplace-page-next')
  await ctx.waitFor(
    'the second page',
    async () => (await tileCount(instance)) === 6,
  )
  assert.ok(
    (await tileSlugs(instance)).every((slug) =>
      slug.startsWith(`${PRESET}-filler-`),
    ),
  )
  await instance.click('marketplace-page-previous')
  await ctx.waitFor(
    'the first page again',
    async () => (await tileCount(instance)) === 20,
  )
  ctx.step('paging')

  // The search waits for a rest of the typing, then asks once and narrows the list.
  await instance.type('marketplace-search', 'agent')
  await ctx.waitFor('the search result', async () => {
    const slugs = await tileSlugs(instance)
    return slugs.length === 1 && slugs[0] === `${PRESET}-agent-tools`
  })
  assert.ok(
    market.requests.some((request) => request.query.includes('search=agent')),
    'the marketplace was asked with the search text',
  )
  ctx.step('search narrows the catalog')

  // The category and the marketplace filter go through the same command the toolbar calls.
  const games = unwrap<Page>(
    'marketplace_catalog',
    await instance.invoke('marketplace_catalog', {
      query: { page: 1, category: 'games' },
    }),
  )
  assert.ok(games.entries.length > 0)
  assert.ok(games.entries.every((entry) => entry.slug.startsWith('filler-')))
  const other = unwrap<Page>(
    'marketplace_catalog',
    await instance.invoke('marketplace_catalog', {
      query: { page: 1, sourceId: 'no-such-marketplace' },
    }),
  )
  assert.equal(other.entries.length, 0)

  // A search still inside its debounce is committed before details navigation; "Zurück" returns
  // to that filtered catalog state.
  await instance.exec(`
    const input = document.querySelector('input[data-testid="marketplace-search"]');
    if (!input) throw new Error('marketplace search input not found');
    input.value = 'agent-tools';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    document.querySelector('li[data-testid="marketplace-tile-${PRESET}-agent-tools"] button')?.click();
  `)
  await instance.waitForDisplayed('marketplace-details')
  assert.match(
    await textOf(instance, 'marketplace-details-source'),
    /Haex Marketplace/,
  )
  await instance.waitForDisplayed('marketplace-version-1.0.0')
  await instance.click('marketplace-back')
  await instance.waitForDisplayed(`marketplace-tile-${PRESET}-agent-tools`)
  assert.equal(await tileCount(instance), 1, 'the pending search is applied')
  ctx.step('pending search, details and back')

  // Clear the search, then let the marketplace stop answering.
  // Escape empties the search (CatalogToolbar); WebDriver refuses to send empty text.
  await instance.type('marketplace-search', '\uE00C')
  await ctx.waitFor(
    'the whole catalog again',
    async () => (await tileCount(instance)) === 20,
  )
  market.down = true
  await instance.click('marketplace-refresh')
  await instance.waitForDisplayed(`marketplace-source-error-${PRESET}`, 15_000)
  await instance.waitForDisplayed('marketplace-offline')
  assert.equal(await tileCount(instance), 0)
  ctx.step(
    'a marketplace that is down shows its error line and the offline state',
  )

  market.down = false
  await instance.click(`marketplace-source-retry-${PRESET}`)
  await instance.waitForDisplayed(`marketplace-tile-${PRESET}-probe`, 15_000)
  assert.equal(await tileCount(instance), 20)
  ctx.step('retry brings the entries back')
})
