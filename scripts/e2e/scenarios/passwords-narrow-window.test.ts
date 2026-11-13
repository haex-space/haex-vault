import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  activeTab,
  createEntry,
  horizontalOverflow,
  openPasswords,
  overview,
  selectTab,
} from '../lib/passwords.ts'
import { KEY, isShown, resizeAppWindow } from '../lib/settings.ts'

const APP = 'system.passwords'
const sidebarShown = (instance: Parameters<typeof isShown>[0]) =>
  isShown(instance, '#passwords-sidebar')

// Spec 034, quickstart §11 (FR-041, SC-012): below the same width as the settings the sidebar is gone
// and the header keeps its button and the search; the button opens the sidebar over the content, a
// choice or Escape closes it; at 360 px no place scrolls sideways and every action stays reachable.
scenario('passwords-narrow-window', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-narrow' })
  const id = await createEntry(instance, {
    title: 'A rather long title of an entry that has to fit a narrow window',
    username: 'someone-with-a-long-address@example.invalid',
    tags: ['narrow'],
  })
  await openPasswords(instance)

  await resizeAppWindow(instance, APP, 1000)
  await ctx.waitFor('the sidebar in the wide window', () =>
    sidebarShown(instance),
  )
  await instance.click('passwords-sidebar-toggle')
  await ctx.waitFor(
    'the sidebar to hide',
    async () => !(await sidebarShown(instance)),
  )
  await instance.click('passwords-sidebar-toggle')
  await ctx.waitFor('the sidebar to come back', () => sidebarShown(instance))
  ctx.step('wide: hide and show')

  // Spec 093 (US1, SC-003, FR-008): the controls are in the window's header; in the wide window all
  // of them are in the row, a picked tag puts a dot on the filter, and the dot follows the filter
  // into the overflow panel (on the ⋯ button) when the window is narrow.
  const inRow = (testId: string) =>
    instance.exec<boolean>(
      `return !!document.querySelector('[data-testid="wm-header"] [data-testid="' + arguments[0] + '"]')`,
      [testId],
    )
  const dotOn = (testId: string) =>
    instance.exec<boolean>(
      `return !!document.querySelector('[data-testid="' + arguments[0] + '"] [data-testid="wm-header-dot"]')`,
      [testId],
    )
  for (const hook of [
    'passwords-sidebar-toggle',
    'passwords-search',
    'passwords-tag-filter',
    'passwords-new',
    'passwords-menu',
  ])
    assert.ok(await inRow(hook), `${hook} is not in the header at 1000 px`)
  assert.equal(
    await inRow('wm-header-overflow'),
    false,
    'something moved into the overflow panel at 1000 px',
  )
  assert.equal(await dotOn('passwords-tag-filter'), false)
  const tag = (await overview(instance)).tags.find((t) => t.name === 'narrow')
  assert.ok(tag, 'the tag of the entry is in the overview')
  await instance.click('passwords-tag-filter')
  await instance.click(`passwords-tag-filter-${tag.id}`)
  await ctx.waitFor('the dot on the tag filter', () =>
    dotOn('passwords-tag-filter'),
  )
  await resizeAppWindow(instance, APP, 360)
  await ctx.waitFor('the filter to move into the panel', async () =>
    dotOn('wm-header-overflow'),
  )
  assert.ok(
    await inRow('passwords-sidebar-toggle'),
    'the sidebar button left the row at 360 px',
  )
  await resizeAppWindow(instance, APP, 1000)
  await ctx.waitFor('the filter back in the row', async () =>
    inRow('passwords-tag-filter'),
  )
  await instance.click('passwords-tag-filter')
  await instance.click('passwords-tag-filter-all')
  await ctx.waitFor(
    'the dot to go',
    async () => !(await dotOn('passwords-tag-filter')),
  )
  ctx.step('header: all controls in the row, the dot follows the filter')

  await resizeAppWindow(instance, APP, 600)
  await ctx.waitFor(
    'the sidebar to leave the narrow window',
    async () => !(await sidebarShown(instance)),
  )
  await instance.waitForDisplayed('passwords-sidebar-toggle')
  await instance.waitForDisplayed('passwords-search')
  await instance.waitForDisplayed('passwords-new')
  await instance.click('passwords-sidebar-toggle')
  await ctx.waitFor('the sidebar over the content', () =>
    sidebarShown(instance),
  )
  await instance.click('passwords-trash')
  await ctx.waitFor(
    'a choice to close the sidebar',
    async () => !(await sidebarShown(instance)),
  )
  await instance.click('passwords-sidebar-toggle')
  await ctx.waitFor('the sidebar to open again', () => sidebarShown(instance))
  await instance.type('#passwords-sidebar button', KEY.escape)
  await ctx.waitFor(
    'Escape to close the sidebar',
    async () => !(await sidebarShown(instance)),
  )
  ctx.step('narrow: open over the content, a choice and Escape close')

  await resizeAppWindow(instance, APP, 360)
  const overflow = () => horizontalOverflow(instance)
  // The list, the entry and the editor at 360 px; the sidebar's own choice leads back to the list.
  await instance.click('passwords-sidebar-toggle')
  await ctx.waitFor('the sidebar to open at 360 px', () =>
    sidebarShown(instance),
  )
  await instance.click('passwords-all')
  await ctx.waitFor(
    'the sidebar to close at 360 px',
    async () => !(await sidebarShown(instance)),
  )
  await instance.waitForDisplayed(`passwords-entry-${id}`)
  assert.ok((await overflow()) <= 1, 'the list scrolls sideways at 360 px')
  await instance.click(`passwords-entry-${id}`)
  await instance.waitForDisplayed('passwords-edit')
  assert.ok((await overflow()) <= 1, 'the entry scrolls sideways at 360 px')
  for (const hook of [
    'passwords-edit',
    'entry-tab-details',
    'entry-tab-extra',
    'entry-tab-history',
    'passwords-delete',
  ]) {
    await instance.waitForDisplayed(hook)
  }
  // Spec 036 (FR-042, SC-003): every tab of the entry fits 360 px.
  for (const tab of ['extra', 'history', 'details'] as const) {
    await selectTab(instance, tab)
    await ctx.waitFor(
      `the ${tab} tab`,
      async () => (await activeTab(instance)) === tab,
    )
    assert.ok(
      (await overflow()) <= 1,
      `the ${tab} tab scrolls sideways at 360 px`,
    )
  }
  await instance.click('passwords-edit')
  await instance.waitForDisplayed('passwords-editor')
  assert.ok((await overflow()) <= 1, 'the editor scrolls sideways at 360 px')
  await selectTab(instance, 'extra')
  await ctx.waitFor(
    'the editor on Extra',
    async () => (await activeTab(instance)) === 'extra',
  )
  assert.ok(
    (await overflow()) <= 1,
    'the editor Extra scrolls sideways at 360 px',
  )
  await instance.waitForDisplayed('passwords-editor-save')
  ctx.step('360 px without sideways scrolling, actions reachable')
})
