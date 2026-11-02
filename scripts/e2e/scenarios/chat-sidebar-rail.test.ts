import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat } from '../lib/flows.ts'
import { resizeAppWindow } from '../lib/settings.ts'

// The collapsed chat sidebar (spec 076 FR-005, FR-006): one avatar per entry of the list, in its
// order; picking one opens that conversation and marks it; the new-conversation button works in
// both states.
scenario('chat-sidebar-rail', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-chat-rail' })
  await openChat(instance)
  await resizeAppWindow(instance, 'system.chat', 1000)

  const count = (selector: string) =>
    instance.exec<number>(
      `return document.querySelectorAll(arguments[0]).length`,
      [selector],
    )
  const listed = await ctx.waitFor('the expanded list', async () => {
    const n = await count(
      '[data-testid="chat-agent"],[data-testid="chat-notes"],[data-testid="chat-peer"]',
    )
    return n >= 2 ? n : false
  })

  await instance.click('chat-sidebar-toggle')
  await ctx.waitFor(
    'one rail item for each entry',
    async () => (await count('[data-testid="chat-rail-item"]')) === listed,
  )
  ctx.step('the rail lists the same entries as the list')

  // The second entry is the notes ("Ich"): the agent comes first while both have no activity.
  await instance.exec(
    `document.querySelectorAll('[data-testid="chat-rail-item"]')[1].click()`,
  )
  await instance.waitForDisplayed('chat-peer-conversation')
  await ctx.waitFor(
    'the picked item to be marked',
    async () =>
      (await count('[data-testid="chat-rail-item"][data-active]')) === 1,
  )
  ctx.step('picking an avatar opens its conversation and marks it')

  await instance.click('chat-new-conversation')
  await instance.waitForDisplayed('chat-new-contact')
  ctx.step('the new-conversation button works in the rail')
})
