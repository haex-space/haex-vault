import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat } from '../lib/flows.ts'
import { KEY, resizeAppWindow } from '../lib/settings.ts'

/** The chat sidebar's state (spec 076): `hidden`, `expanded` or `overlay`. */
const sidebarState = (instance: Parameters<typeof resizeAppWindow>[0]) =>
  instance.exec<string | null>(
    `return document.querySelector('#chat-sidebar')?.getAttribute('data-state') ?? null`,
  )

// The chat's thread sidebar is the same as every app's (`WmSidebar`) and can be hidden completely:
// in a wide window the button in the window's header hides and shows it; below 672 px it is gone
// and the button opens it over the content, the button and Escape close it again.
scenario('chat-narrow-window', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-chat-narrow' })
  await openChat(instance)

  const waitForState = (state: string, label: string) =>
    ctx.waitFor(label, async () => (await sidebarState(instance)) === state)

  await resizeAppWindow(instance, 'system.chat', 1000)
  await waitForState('expanded', 'the sidebar in the wide window')
  await instance.click('chat-sidebar-toggle')
  await waitForState('hidden', 'the sidebar to hide')
  await instance.waitForDisplayed('chat-sidebar-toggle')
  await instance.click('chat-sidebar-toggle')
  await waitForState('expanded', 'the sidebar to show again')
  ctx.step('wide: hide the sidebar completely and show it again')

  await resizeAppWindow(instance, 'system.chat', 600)
  await waitForState('hidden', 'the sidebar gone in the narrow window')
  await instance.waitForDisplayed('chat-sidebar-toggle')
  await instance.click('chat-sidebar-toggle')
  await waitForState('overlay', 'the sidebar over the content')
  await instance.type('chat-sidebar-toggle', KEY.escape)
  await waitForState('hidden', 'Escape to close the sidebar')
  await instance.click('chat-sidebar-toggle')
  await waitForState('overlay', 'the sidebar to reopen')
  await instance.click('chat-sidebar-toggle')
  await waitForState('hidden', 'the button to close the sidebar')
  ctx.step('narrow: the sidebar is gone, the button opens it over the content')
})
