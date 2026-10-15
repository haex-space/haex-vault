import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  runAction,
  settingsTabs,
  waitForLocation,
  wmSnapshot,
} from '../lib/settings.ts'

// Spec 023-settings-app, quickstart S8–S10 and the address half of S13 (FR-009, FR-011, FR-012,
// FR-017): a deep link opens the settings at a location, both when the page loads with it and when it
// arrives while the settings are open elsewhere (then the existing tab moves and keeps its history); a
// repo reached directly still has a way up; an unknown location falls back with a hint; the federation
// app's old address and id lead to the federation category.
scenario('settings-deep-links', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  const name = 'e2e-settings-links'
  await createAndUnlock(instance, { name })
  const workspace = `tauri://localhost/workspace/${encodeURIComponent(name)}`

  await instance.navigate(`${workspace}?open=system.settings&at=%2Fai%2Fspeech`)
  await waitForLocation(instance, 'ai.speech', 10000)
  const query = await instance.exec<string>('return location.search')
  assert.equal(query, '', 'the deep link stayed in the address')
  ctx.step('S8 page load')

  await instance.click('settings-category-storage')
  await waitForLocation(instance, 'storage')
  await instance.exec(
    `return document.querySelector('#__nuxt').__vue_app__.config.globalProperties.$router
       .replace({ query: { open: 'system.settings', at: '/ai/speech' } }).then(() => true)`,
  )
  await waitForLocation(instance, 'ai.speech')
  const tabs = settingsTabs(await wmSnapshot(instance))
  assert.equal(
    tabs.length,
    1,
    `expected one settings tab: ${JSON.stringify(tabs)}`,
  )
  const back = await runAction(instance, 'wm.tab.back', {
    tabId: tabs[0]!.tabId,
  })
  assert.ok(back.ok, `wm.tab.back failed: ${JSON.stringify(back)}`)
  await waitForLocation(instance, 'storage')
  ctx.step('S8 open settings, same tab, back to the previous place')

  await instance.navigate(
    `${workspace}?open=system.settings&at=%2Fai%2Fdownload%2Frepo%2FQwen%2FQwen2.5-0.5B-Instruct-GGUF`,
  )
  await waitForLocation(instance, 'ai.download.repo', 10000)
  await instance.click('settings-back')
  await waitForLocation(instance, 'ai.download.search')
  ctx.step('S9 repo, arrow up to the search')

  await instance.navigate(
    `${workspace}?open=system.settings&at=%2Fgibt-es-nicht`,
  )
  await waitForLocation(instance, 'general', 10000)
  await instance.waitForDisplayed('[data-sonner-toast]')
  ctx.step('S10 unknown location')

  await instance.navigate(
    `tauri://localhost/federation/${encodeURIComponent(name)}`,
  )
  await waitForLocation(instance, 'federation', 10000)
  ctx.step('S13 old federation address')

  await instance.navigate(`${workspace}?open=system.federation`)
  await waitForLocation(instance, 'federation', 10000)
  const apps = (await wmSnapshot(instance)).windows.flatMap((w) =>
    w.tabs.map((t) => t.appId),
  )
  assert.ok(
    !apps.includes('system.federation'),
    `a federation tab opened: ${JSON.stringify(apps)}`,
  )
  ctx.step('S13 old federation app id')
})
