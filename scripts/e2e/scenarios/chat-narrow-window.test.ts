import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat } from '../lib/flows.ts'
import { KEY, resizeAppWindow } from '../lib/settings.ts'

/** The chat sidebar's state (spec 076): `rail`, `expanded` or `overlay`. */
const sidebarState = (instance: Parameters<typeof resizeAppWindow>[0]) =>
  instance.exec<string | null>(
    `return document.querySelector('#chat-sidebar')?.getAttribute('data-state') ?? null`,
  )

// The chat's thread sidebar follows the window's width like the settings sidebar does, but it is
// never gone (spec 076 FR-001): in a wide window the toggle switches it between the full sidebar
// and the rail; below 672 px the rail stays and the toggle opens the full sidebar over the content,
// the toggle and Escape close it again.
scenario('chat-narrow-window', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-chat-narrow' })
  await openChat(instance)

  const waitForState = (state: string, label: string) =>
    ctx.waitFor(label, async () => (await sidebarState(instance)) === state)

  await resizeAppWindow(instance, 'system.chat', 1000)
  await waitForState('expanded', 'the sidebar in the wide window')
  await instance.click('chat-sidebar-toggle')
  await waitForState('rail', 'the sidebar to collapse to the rail')
  await instance.waitForDisplayed('chat-sidebar-toggle')
  await instance.click('chat-sidebar-toggle')
  await waitForState('expanded', 'the sidebar to expand again')
  ctx.step('wide: collapse to the rail and expand')

  await resizeAppWindow(instance, 'system.chat', 600)
  await waitForState('rail', 'the rail in the narrow window')
  await instance.waitForDisplayed('chat-sidebar-toggle')
  await instance.click('chat-sidebar-toggle')
  await waitForState('overlay', 'the sidebar over the content')
  await instance.type('chat-sidebar-toggle', KEY.escape)
  await waitForState('rail', 'Escape to close the sidebar')
  await instance.click('chat-sidebar-toggle')
  await waitForState('overlay', 'the sidebar to reopen')
  await instance.click('chat-sidebar-toggle')
  await waitForState('rail', 'the toggle to close the sidebar')
  ctx.step('narrow: the rail stays, the toggle opens it over the content')
})
