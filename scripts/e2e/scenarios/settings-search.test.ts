import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  KEY,
  isShown,
  openSettings,
  resizeSettingsWindow,
  waitForLocation,
} from '../lib/settings.ts'

const hitLocations = (instance: { exec<T>(s: string): Promise<T> }) =>
  instance.exec<string[]>(
    'return [...document.querySelectorAll(\'[data-testid="settings-search-hit"]\')].map((el) => el.dataset.location)',
  )

// Spec 023-settings-app, quickstart S19 (FR-023), in the window's header since spec 090: the search
// finds settings by title, synonym and single setting, shows the hits with their path in the sidebar
// (over the content in a narrow window), Enter opens the first hit, a miss says so, the first Escape
// empties the field and the second leaves it. The search is an always open field, so there is no
// button to open it. The queries are German, the interface's default.
scenario('settings-search', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-settings-search' })
  await openSettings(instance)

  const cases: Array<[string, string]> = [
    ['parakeet', 'ai.speech'],
    ['gerätename', 'general.basic'],
    ['huggingface suchen', 'ai.local.search'],
  ]
  for (const [query, location] of cases) {
    await instance.type('wm-header-q', query)
    const hits = await ctx.waitFor(`hits for "${query}"`, async () => {
      const found = await hitLocations(instance)
      return found.length > 0 ? found : null
    })
    assert.equal(
      hits?.[0],
      location,
      `first hit for "${query}": ${JSON.stringify(hits)}`,
    )
    await instance.type('wm-header-q', KEY.enter)
    await waitForLocation(instance, location)
    // The field keeps the text while it has focus (spec 090 FR-013); the next query starts empty.
    await instance.type('wm-header-q', KEY.escape)
  }
  ctx.step('hits and Enter')

  const missAndEscape = async () => {
    await instance.type('wm-header-q', 'xyz')
    await instance.waitForDisplayed('#settings-sidebar [role="status"]')
    assert.deepEqual(await hitLocations(instance), [])
    await instance.type('wm-header-q', KEY.escape)
    const value = await instance.exec<string>(
      'return document.querySelector(\'[data-testid="wm-header-q"]\').value',
    )
    assert.equal(value, '', 'the first Escape did not empty the field')
    await instance.type('wm-header-q', KEY.escape)
    await ctx.waitFor(
      'the second Escape to leave the field',
      async () =>
        !(await instance.exec<boolean>(
          'return document.activeElement === document.querySelector(\'[data-testid="wm-header-q"]\')',
        )),
    )
  }
  await missAndEscape()
  ctx.step('miss and Escape (wide)')

  await resizeSettingsWindow(instance, 480)
  await ctx.waitFor(
    'the sidebar to leave the narrow window',
    async () => !(await isShown(instance, '#settings-sidebar')),
  )
  await instance.type('wm-header-q', 'parakeet')
  await ctx.waitFor('the hits over the content', () =>
    isShown(instance, '#settings-sidebar'),
  )
  await instance.click(
    '[data-testid="settings-search-hit"][data-location="ai.speech"]',
  )
  await waitForLocation(instance, 'ai.speech')
  await ctx.waitFor(
    'the sidebar to close after the hit',
    async () => !(await isShown(instance, '#settings-sidebar')),
  )
  await missAndEscape()
  ctx.step('narrow window')
})
