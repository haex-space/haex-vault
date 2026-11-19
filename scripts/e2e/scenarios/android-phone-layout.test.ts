import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { runAction } from '../lib/settings.ts'

/** What the page reports about its width and the space the system takes, in CSS pixels. */
interface Layout {
  width: number
  height: number
  scrollWidth: number
  insetTop: number
  insetBottom: number
  desktopTop: number
  desktopBottom: number
}

const LAYOUT = `
  const root = document.documentElement
  const inset = (side) => parseFloat(getComputedStyle(root).getPropertyValue('--haex-inset-' + side)) || 0
  const desktop = document.querySelector('[data-testid="wm-desktop"]').getBoundingClientRect()
  return {
    width: window.innerWidth,
    height: window.innerHeight,
    scrollWidth: root.scrollWidth,
    insetTop: inset('top'),
    insetBottom: inset('bottom'),
    desktopTop: desktop.top,
    desktopBottom: desktop.bottom,
  }`

// Spec 043 FR-010, FR-011 at the narrowest phone width (360 CSS pixels): no page scrolls sideways, and
// the window manager keeps clear of the status bar at the top and the navigation bar at the bottom.
scenario('android-phone-layout', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance({ phoneScreen: true })
  await createAndUnlock(instance, { name: 'phone-vault' })
  await ctx.waitFor('the phone width', async () => {
    const now = await instance.exec<Layout>(LAYOUT)
    return now.width <= 360 && now.insetTop > 0
  })
  const layout = await instance.exec<Layout>(LAYOUT)
  assert.ok(layout.scrollWidth <= layout.width + 1, JSON.stringify(layout))
  assert.ok(layout.desktopTop >= layout.insetTop - 0.5, JSON.stringify(layout))
  assert.ok(
    layout.desktopBottom <= layout.height - layout.insetBottom + 0.5,
    JSON.stringify(layout),
  )
  ctx.step('the workspace fits between the system bars')

  for (const appId of ['system.passwords', 'system.settings', 'system.chat']) {
    const opened = await runAction(instance, 'wm.app.open', { appId })
    assert.ok(opened.ok, JSON.stringify(opened))
    await ctx.waitFor(`${appId} without sideways scrolling`, async () => {
      const now = await instance.exec<Layout>(LAYOUT)
      return now.scrollWidth <= now.width + 1
    })
  }
  ctx.step('the apps fit the phone width')
})
