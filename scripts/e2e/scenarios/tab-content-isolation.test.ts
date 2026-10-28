import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat } from '../lib/flows.ts'
import { WM } from '../lib/settings.ts'

const settle = () => new Promise((resolve) => setTimeout(resolve, 500))

// Spec 020-tab-navigation, quickstart M13/M21 (FR-020, FR-034, FR-035, SC-009): neither the webview's own
// history nor an embedded document's history changes holzi's navigation. The top document stays on the
// workspace page, the chat stays open, and the chat tab's history keeps no back entry.
scenario('tab-content-isolation', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-isolation' })
  await openChat(instance)
  ctx.step('chat open')

  await instance.exec('history.back(); history.go(-5); return true')
  await settle()
  const afterTopBack = await instance.exec<string>('return location.pathname')
  assert.ok(
    afterTopBack.startsWith('/workspace/'),
    `webview back left the workspace page: ${afterTopBack}`,
  )
  await instance.waitForDisplayed('chat-sidebar-toggle')
  ctx.step('webview back ignored')

  await instance.exec(`
    const frame = document.createElement('iframe')
    frame.id = 'e2e-embedded'
    frame.srcdoc = '<p>embedded</p>'
    document.querySelector('[role="tabpanel"]').appendChild(frame)
    return true
  `)
  await settle()
  await instance.exec(`
    const w = document.getElementById('e2e-embedded').contentWindow
    for (let i = 0; i < 20; i++) w.history.pushState({ i }, '')
    w.history.go(-30)
    w.history.back()
    return true
  `)
  await settle()

  const afterFrame = await instance.exec<string>('return location.pathname')
  assert.ok(
    afterFrame.startsWith('/workspace/'),
    `embedded history left the workspace page: ${afterFrame}`,
  )
  await instance.waitForDisplayed('chat-sidebar-toggle')
  const backDisabled = await instance.exec<boolean>(
    `const wm = ${WM}; return wm.windows.every((w) => wm.historyOf(w.activeTabId).index === 0)`,
  )
  assert.ok(
    backDisabled,
    'the chat tab gained back entries from the embedded document',
  )
  ctx.step('embedded history ignored')
})
