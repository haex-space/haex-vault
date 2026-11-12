import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  openSettings,
  runAction,
  setSessionRestore,
  settingsLocation,
  waitForLocation,
  wmSnapshot,
} from '../lib/settings.ts'

// Spec 090-app-header-bar, US5 (FR-028, SC-007): the window has no tab strip, but its tabs are still
// there. A window with three tabs comes back after a restart with all three, each at its place and
// with its history, and the actions that switch and close tabs act as before. (A close guard that
// still asks is unchanged model code, covered by `check:wm-state`.)
scenario('window-tabs-restore', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const device = group.device('anna/laptop')
  const page = device.page

  await setSessionRestore(page, true)
  await openSettings(page)
  const [window] = (await wmSnapshot(page)).windows
  assert.ok(window, 'no window after opening settings')

  // The settings tab goes one place deeper, so it has a history to restore.
  await page.click('settings-category-ai')
  await waitForLocation(page, 'ai')

  for (const appId of ['system.chat', 'system.chat']) {
    const opened = await runAction(page, 'wm.tab.new', {
      windowId: window.id,
      appId,
    })
    assert.ok(opened.ok, `wm.tab.new failed: ${JSON.stringify(opened)}`)
  }
  const before = (await wmSnapshot(page)).windows.find(
    (w) => w.id === window.id,
  )
  assert.equal(before?.tabs.length, 3, 'the window does not hold three tabs')
  // Both chat tabs share an app id, so the order and the active tab are told apart by tab id (a
  // restored session keeps them).
  const tabIds = before?.tabs.map((t) => t.id)
  const activeTabId = before?.activeTabId
  ctx.step('three tabs in one window, none of them drawn as a strip')

  const noStrip = await page.exec<boolean>(
    `return !document.querySelector('[role="tablist"], [data-testid="tab-close"], [data-testid="wm-new-tab"]')`,
  )
  assert.ok(noStrip, 'a tab strip is still drawn')

  await ctx.waitFor(
    'the saved session to hold three tabs',
    async () => {
      const text = JSON.stringify(await page.invoke('wm_session_load'))
      return text.split('"appId"').length - 1 >= 3 ? true : false
    },
    { fixed: true, timeoutMs: 15_000 },
  )
  ctx.step('session saved')

  await device.restart()
  const restored = device.page
  const after = await ctx.waitFor('the window to come back', async () => {
    const found = (await wmSnapshot(restored)).windows.at(0)
    return found && found.tabs.length === 3 ? found : null
  })
  assert.deepEqual(
    after?.tabs.map((t) => t.id),
    tabIds,
    'the tabs came back in another order or as other tabs',
  )
  assert.equal(
    after?.activeTabId,
    activeTabId,
    'another tab is active after the restart',
  )
  ctx.step('all three tabs and the active one are back')

  // The actions switch and walk the history as before.
  const settingsTab = after?.tabs.find((t) => t.appId === 'system.settings')
  assert.ok(settingsTab)
  const activated = await runAction(restored, 'wm.tab.activate', {
    tabId: settingsTab.id,
  })
  assert.ok(
    activated.ok,
    `wm.tab.activate failed: ${JSON.stringify(activated)}`,
  )
  await waitForLocation(restored, 'ai')
  const back = await runAction(restored, 'wm.tab.back', {
    tabId: settingsTab.id,
  })
  assert.ok(back.ok, `wm.tab.back failed: ${JSON.stringify(back)}`)
  await waitForLocation(restored, 'general')
  const forward = await runAction(restored, 'wm.tab.forward', {
    tabId: settingsTab.id,
  })
  assert.ok(forward.ok, `wm.tab.forward failed: ${JSON.stringify(forward)}`)
  await waitForLocation(restored, 'ai')
  assert.equal(await settingsLocation(restored), 'ai')
  ctx.step('activate, back and forward act as before')

  const chatTab = after?.tabs.find((t) => t.appId === 'system.chat')
  assert.ok(chatTab)
  const closed = await runAction(restored, 'wm.tab.close', {
    tabId: chatTab.id,
  })
  assert.ok(closed.ok, `wm.tab.close failed: ${JSON.stringify(closed)}`)
  const left = (await wmSnapshot(restored)).windows.find(
    (w) => w.id === after?.id,
  )
  assert.equal(left?.tabs.length, 2, 'closing a tab did not leave two')
  ctx.step('closing a tab keeps the window and the other tabs')
})
