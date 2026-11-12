import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat } from '../lib/flows.ts'
import { runAction, wmSnapshot } from '../lib/settings.ts'

// Spec 030-app-multi-instance: Chat is now a multi-instance app (amending spec 015 FR-017) —
// opening it again always creates a new, independent instance instead of activating an existing
// one. Settings stays single-instance (no regression).
scenario('chat-multi-instance', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-multi-instance' })
  await openChat(instance)

  const afterFirst = await wmSnapshot(instance)
  assert.equal(afterFirst.windows.length, 1)
  const [firstWindow] = afterFirst.windows
  assert.ok(firstWindow)
  assert.equal(firstWindow.tabs.length, 1)
  ctx.step('one window, one chat tab')

  // The window has no tab strip and no "+" since spec 090; a tab is added by its action.
  const added = await runAction(instance, 'wm.tab.new', {
    windowId: firstWindow.id,
    appId: 'system.chat',
  })
  assert.ok(added.ok, `wm.tab.new failed: ${JSON.stringify(added)}`)
  const afterSecondTab = await wmSnapshot(instance)
  assert.equal(
    afterSecondTab.windows.length,
    1,
    'wm.tab.new opened a second window instead of a second tab',
  )
  const sameWindow = afterSecondTab.windows.find((w) => w.id === firstWindow.id)
  assert.ok(sameWindow)
  assert.equal(sameWindow.tabs.length, 2)
  assert.notEqual(sameWindow.tabs[0]?.id, sameWindow.tabs[1]?.id)
  assert.ok(sameWindow.tabs.every((t) => t.appId === 'system.chat'))
  ctx.step('wm.tab.new adds a second, independent chat tab in the same window')

  await instance.click('open-launcher')
  await instance.click('open-chat')
  const afterLauncher = await wmSnapshot(instance)
  assert.equal(
    afterLauncher.windows.length,
    2,
    'the Launcher activated an existing chat tab instead of opening a new window',
  )
  ctx.step(
    'the Launcher opens a third, independent chat instance in a new window',
  )

  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.settings"]')
  const afterSettingsFirst = await wmSnapshot(instance)
  const settingsWindows = afterSettingsFirst.windows.filter((w) =>
    w.tabs.some((t) => t.appId === 'system.settings'),
  )
  assert.equal(settingsWindows.length, 1)
  const settingsTabCountBefore = settingsWindows[0]!.tabs.length

  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.settings"]')
  const afterSettingsSecond = await wmSnapshot(instance)
  const settingsWindowsAfter = afterSettingsSecond.windows.filter((w) =>
    w.tabs.some((t) => t.appId === 'system.settings'),
  )
  assert.equal(
    settingsWindowsAfter.length,
    1,
    'a second settings window was opened',
  )
  assert.equal(settingsWindowsAfter[0]!.tabs.length, settingsTabCountBefore)
  ctx.step(
    'Settings stays single-instance: reopening activates the existing tab',
  )
})
