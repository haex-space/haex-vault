import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { isShown, settingsLocation, waitForLocation } from '../lib/settings.ts'

const CATEGORIES = [
  'general',
  'models',
  'agents',
  'storage',
  'contacts',
  'extensions',
  'federation',
]

// Spec 023-settings-app, quickstart S1–S3 and the launcher half of S13 (FR-001, FR-002, FR-005, FR-007,
// FR-016): the launcher has no federation app; the settings open on "Allgemein" with the toolbar, the
// sidebar in registry order and the category icon in the title row; every category shows its own
// content, the title row keeps its place and the tab keeps its name; every setting from before the
// spec is at most three clicks away.
scenario('settings-categories', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-settings-categories' })

  await instance.click('open-launcher')
  await instance.waitForDisplayed('[data-app-id="system.settings"]')
  const apps = await instance.exec<string[]>(
    'return [...document.querySelectorAll("[data-app-id]")].map((el) => el.dataset.appId)',
  )
  assert.ok(
    !apps.includes('system.federation'),
    `the launcher still offers the federation app: ${JSON.stringify(apps)}`,
  )
  await instance.click('[data-app-id="system.settings"]')
  await waitForLocation(instance, 'general')
  ctx.step('opened from the launcher')

  await instance.waitForDisplayed('settings-sidebar-toggle')
  await instance.waitForDisplayed('settings-search-open')
  await instance.waitForDisplayed('settings-category-icon')
  const sidebar = await instance.exec<{ ids: string[]; current: string[] }>(
    `const items = [...document.querySelectorAll('[data-testid^="settings-category-"]:not([data-testid="settings-category-icon"])')]
     return {
       ids: items.map((el) => el.dataset.testid.replace('settings-category-', '')),
       current: items.filter((el) => el.getAttribute('aria-current') === 'page').map((el) => el.dataset.testid),
     }`,
  )
  assert.deepEqual(sidebar.ids, CATEGORIES)
  const withoutIcon = await instance.exec<string[]>(
    `return [...document.querySelectorAll('[data-testid^="settings-category-"]:not([data-testid="settings-category-icon"])')]
       .filter((el) => !el.querySelector('svg'))
       .map((el) => el.dataset.testid)`,
  )
  assert.deepEqual(withoutIcon, [], 'category icons missing without network')
  assert.deepEqual(sidebar.current, ['settings-category-general'])
  // Spec 042: "Allgemein" is an overview of "Grundeinstellung" and "Erscheinungsbild".
  await instance.waitForDisplayed('settings-row-general.basic')
  await instance.waitForDisplayed('settings-row-general.appearance')
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')
  await instance.waitForDisplayed('settings-alias')
  await instance.waitForDisplayed('session-restore-switch')
  ctx.step('S1 first category')

  const titleBox = () =>
    instance.exec<{ left: number; top: number; tab: string }>(
      `const rect = document.querySelector('[data-testid="settings-title"]').getBoundingClientRect()
       const frame = document.querySelector('[data-testid="settings-title"]').closest('[data-wm-window-id]')
       return { left: Math.round(rect.left), top: Math.round(rect.top), tab: frame.getAttribute('aria-label') }`,
    )
  const first = await titleBox()
  for (const id of CATEGORIES) {
    await instance.click(`settings-category-${id}`)
    await waitForLocation(instance, id)
    const box = await titleBox()
    assert.deepEqual(
      { left: box.left, top: box.top },
      { left: first.left, top: first.top },
      `the title moved on ${id}`,
    )
    assert.equal(box.tab, first.tab, `the tab title changed on ${id}`)
    assert.ok(
      await isShown(instance, '[data-testid="settings-category-icon"]'),
      `no category icon in the title row of ${id}`,
    )
  }
  ctx.step('S2 every category')

  // S3: every setting from before the spec, as (clicks from the settings, hook that shows it).
  const reach: Array<[string[], string, string | null]> = [
    [
      ['settings-category-general', 'settings-row-general.basic'],
      'general.basic',
      'settings-alias',
    ],
    [
      ['settings-category-general', 'settings-row-general.basic'],
      'general.basic',
      'session-restore-switch',
    ],
    [
      ['settings-category-general', 'settings-row-general.appearance'],
      'general.appearance',
      'settings-color-scheme',
    ],
    [
      ['settings-category-models', 'settings-row-models.default'],
      'models.default',
      'settings-default-model',
    ],
    [
      ['settings-category-models', 'settings-row-models.speech'],
      'models.speech',
      null,
    ],
    [
      ['settings-category-models', 'settings-row-models.installed'],
      'models.installed',
      'settings-installed-empty',
    ],
    [
      [
        'settings-category-models',
        'settings-row-models.download',
        'settings-row-models.download.search',
      ],
      'models.download.search',
      null,
    ],
    [
      ['settings-category-agents', 'settings-row-agents.providers'],
      'agents.providers',
      null,
    ],
    [
      ['settings-category-agents', 'settings-row-agents.autonomy'],
      'agents.autonomy',
      'settings-autonomy-standard',
    ],
    [
      ['settings-category-agents', 'settings-row-agents.denyRules'],
      'agents.denyRules',
      'settings-deny-network_access',
    ],
  ]
  for (const [clicks, location, hook] of reach) {
    assert.ok(clicks.length <= 3, `${location} needs ${clicks.length} clicks`)
    for (const click of clicks) await instance.click(click)
    await waitForLocation(instance, location)
    if (hook) await instance.waitForDisplayed(hook)
  }
  assert.equal(await settingsLocation(instance), 'agents.denyRules')
  ctx.step('S3 every setting within three clicks')
})
