import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  runAction,
  openSettings,
  settingsTabs,
  waitForLocation,
  wmSnapshot,
} from '../lib/settings.ts'

const REPO = '/ai/download/repo/Qwen/Qwen2.5-0.5B-Instruct-GGUF'

// Spec 023-settings-app, quickstart S4–S7 and S22 (FR-008–FR-010): the arrow in the title row goes to
// the previous place when that is in the same category, otherwise to the parent; the tab's own back and
// forward walk the same places; choosing a category in the sidebar starts at its overview; reopened
// settings start at the first category. The repo is reached the way a search result does it (a push
// onto the tab), so no network is needed.
scenario('settings-header-back', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-settings-back' })
  await openSettings(instance)
  const [tab] = settingsTabs(await wmSnapshot(instance))
  if (!tab) throw new Error('no settings tab')

  // S22: Modelle → Installierte Modelle → (none installed) Modelle herunterladen, then the arrow twice.
  await instance.click('settings-category-ai')
  await instance.click('settings-row-ai.installed')
  await waitForLocation(instance, 'ai.installed')
  await instance.click('settings-installed-empty')
  await waitForLocation(instance, 'ai.download')
  await instance.click('settings-back')
  await waitForLocation(instance, 'ai.installed')
  await instance.click('settings-back')
  await waitForLocation(instance, 'ai')
  ctx.step('S22 arrow walks the places in the category')

  // S22: from another category through the search, the arrow goes to the parent.
  await instance.click('settings-category-storage')
  await waitForLocation(instance, 'storage')
  await instance.click('settings-search-open')
  await instance.type('settings-search', 'installierte')
  await instance.click(
    '[data-testid="settings-search-hit"][data-location="ai.installed"]',
  )
  await waitForLocation(instance, 'ai.installed')
  await instance.click('settings-back')
  await waitForLocation(instance, 'ai')
  ctx.step('S22 arrow after a jump goes to the parent')

  // S4: download → search → repo, the arrow twice, then forward twice.
  await instance.click('settings-row-ai.download')
  await instance.click('settings-row-ai.download.search')
  await waitForLocation(instance, 'ai.download.search')
  const pushed = await runAction(instance, 'wm.tab.navigate', {
    tabId: tab.tabId,
    to: REPO,
  })
  if (!pushed.ok) throw new Error(`wm.tab.navigate: ${JSON.stringify(pushed)}`)
  await waitForLocation(instance, 'ai.download.repo')
  await instance.click('settings-back')
  await waitForLocation(instance, 'ai.download.search')
  await instance.click('settings-back')
  await waitForLocation(instance, 'ai.download')
  await instance.click('nav-forward')
  await waitForLocation(instance, 'ai.download.search')
  await instance.click('nav-forward')
  await waitForLocation(instance, 'ai.download.repo')
  ctx.step('S4 arrow and forward')

  // S5: the tab's back walks the same places.
  await instance.click('nav-back')
  await waitForLocation(instance, 'ai.download.search')
  await instance.click('nav-back')
  await waitForLocation(instance, 'ai.download')
  ctx.step('S5 tab back')

  // S6: another category, then Modelle again starts at its overview.
  await instance.click('settings-category-storage')
  await waitForLocation(instance, 'storage')
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  ctx.step('S6 category starts at its overview')

  // S7: closed and opened again (session restore off), the settings start at "Allgemein".
  const closed = await runAction(instance, 'wm.tab.close', { tabId: tab.tabId })
  if (!closed.ok) throw new Error(`wm.tab.close: ${JSON.stringify(closed)}`)
  await ctx.waitFor(
    'the settings to close',
    async () => settingsTabs(await wmSnapshot(instance)).length === 0,
  )
  await openSettings(instance)
  ctx.step('S7 reopened at the first category')
})
