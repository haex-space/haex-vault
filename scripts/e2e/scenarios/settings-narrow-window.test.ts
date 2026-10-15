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

const sidebarShown = (instance: Parameters<typeof isShown>[0]) =>
  isShown(instance, '#settings-sidebar')

// Spec 023-settings-app, quickstart S15 (FR-004, SC-004): below 672 px of window width the sidebar is
// gone and the toolbar keeps its button and the search; the button opens the sidebar over the content,
// choosing a category or Escape closes it. In a wide window the button hides and shows it. At 360 px
// nothing scrolls sideways.
scenario('settings-narrow-window', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-settings-narrow' })
  await openSettings(instance)

  await resizeSettingsWindow(instance, 1000)
  await ctx.waitFor('the sidebar in the wide window', () =>
    sidebarShown(instance),
  )
  await instance.click('settings-sidebar-toggle')
  await ctx.waitFor(
    'the sidebar to hide',
    async () => !(await sidebarShown(instance)),
  )
  await instance.click('settings-sidebar-toggle')
  await ctx.waitFor('the sidebar to come back', () => sidebarShown(instance))
  ctx.step('wide: hide and show')

  await resizeSettingsWindow(instance, 600)
  await ctx.waitFor(
    'the sidebar to leave the narrow window',
    async () => !(await sidebarShown(instance)),
  )
  await instance.waitForDisplayed('settings-sidebar-toggle')
  await instance.waitForDisplayed('settings-search-open')
  await instance.click('settings-sidebar-toggle')
  await ctx.waitFor('the sidebar over the content', () =>
    sidebarShown(instance),
  )
  await ctx.waitFor('the sidebar to cover the content', () =>
    instance.exec<boolean>(
      `const sidebar = document.getElementById('settings-sidebar').getBoundingClientRect()
       const main = document.querySelector('#settings-sidebar + main').getBoundingClientRect()
       return sidebar.left <= main.left + 1 && sidebar.right >= main.right - 1`,
    ),
  )
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  await ctx.waitFor(
    'a category to close the sidebar',
    async () => !(await sidebarShown(instance)),
  )
  await instance.click('settings-sidebar-toggle')
  await ctx.waitFor('the sidebar to open again', () => sidebarShown(instance))
  await instance.type('[aria-current="page"]', KEY.escape)
  await ctx.waitFor(
    'Escape to close the sidebar',
    async () => !(await sidebarShown(instance)),
  )
  ctx.step('narrow: open over the content, category and Escape close')

  await resizeSettingsWindow(instance, 360)
  for (const id of ['general', 'ai', 'federation']) {
    await instance.click('settings-sidebar-toggle')
    await ctx.waitFor('the sidebar to finish opening', () =>
      sidebarShown(instance),
    )
    await instance.click(`settings-category-${id}`)
    await waitForLocation(instance, id)
    await ctx.waitFor(
      'the category selection to finish closing the sidebar',
      async () => !(await sidebarShown(instance)),
    )
    const overflow = await instance.exec<number>(
      `const frame = document.querySelector('[data-testid="settings-title"]').closest('[data-wm-window-id]')
       const right = frame.getBoundingClientRect().right
       const clipped = (el) => {
         for (let node = el.parentElement; node && node !== frame; node = node.parentElement) {
           if (getComputedStyle(node).overflowX !== 'visible') return true
         }
         return false
       }
       let worst = 0
       for (const el of frame.querySelectorAll('*')) {
         const style = getComputedStyle(el)
         if (style.visibility === 'hidden') continue
         if (style.overflowX === 'auto' || style.overflowX === 'scroll') {
           worst = Math.max(worst, el.scrollWidth - el.clientWidth)
         }
         if (!clipped(el)) worst = Math.max(worst, el.getBoundingClientRect().right - right)
       }
       return worst`,
    )
    assert.ok(
      overflow <= 1,
      `${id} scrolls sideways by ${overflow} px at 360 px`,
    )
  }
  ctx.step('360 px without sideways scrolling')
})
