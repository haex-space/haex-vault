import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  createAndUnlock,
  openChat,
  connectProvider,
  unwrap,
} from '../lib/flows.ts'
import { KEY, isShown } from '../lib/settings.ts'

const MODELS = [
  { id: 'gpt-4o', displayName: 'GPT-4o' },
  { id: 'gpt-4o-mini', displayName: 'GPT-4o mini' },
  { id: 'claude', displayName: 'Claude' },
  { id: 'qwen', displayName: 'Qwen2.5' },
]

const optionShown = (
  instance: Parameters<typeof isShown>[0],
  modelId: string,
) => isShown(instance, `[role="option"][data-value$=":${modelId}"]`)

const triggerTitle = (instance: { exec<T>(script: string): Promise<T> }) =>
  instance.exec<string | null>(
    "return document.querySelector('[data-testid=\"chat-settings-trigger\"]')?.getAttribute('title') ?? null",
  )

// Spec 031-chat-model-search: a search field in the chat composer's model picker filters the list in
// real time, fuzzily, grouped by provider, with a no-results hint and full keyboard control.
scenario('chat-model-search', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  const provider = await ctx.provider(undefined, { models: MODELS })

  await createAndUnlock(instance, { name: 'e2e-model-search' })
  // Before opening the chat: the chat reads the provider list and their cached models once, when it
  // mounts (`modelStore.initialize`), and a provider added behind its back afterwards never reaches
  // the picker. `add_provider` caches the models inline, so they are all there by then.
  await connectProvider(instance, provider)
  await openChat(instance)

  await instance.click('chat-settings-trigger')
  await instance.click('#chat-model-popover')
  await instance.waitForDisplayed('chat-model-search')
  // The chat's own initial load still lands asynchronously after the page shows.
  for (const model of MODELS) {
    await ctx.waitFor(`${model.displayName} to be listed`, () =>
      optionShown(instance, model.id),
    )
  }
  // On a touch screen the field does not take focus by itself (it would open the on-screen
  // keyboard and shrink the window); there the person taps it on purpose.
  if (
    await instance.exec<boolean>(
      'return window.matchMedia("(pointer: coarse)").matches',
    )
  ) {
    await instance.click('chat-model-search')
  }
  await ctx.waitFor('the search field to hold focus', async () =>
    instance.exec<boolean>(
      'return document.activeElement?.getAttribute("data-testid") === "chat-model-search"',
    ),
  )
  ctx.step('all models shown before searching')

  // The CLI delegates are offered only where the device runs them (spec 043 FR-026): on a desktop
  // as entries to connect, on a phone not at all.
  const { cliDelegates } = unwrap<{ cliDelegates: boolean }>(
    'platform_capabilities',
    await instance.invoke('platform_capabilities'),
  )
  assert.equal(
    await isShown(instance, '[role="option"][data-value^="delegate-"]'),
    cliDelegates,
    `delegates offered: ${String(!cliDelegates)}`,
  )

  // Typed to wherever focus is, as a user does, not sent to the search field by hook.
  await instance.typeToFocused('gpt4o')
  await ctx.waitFor('the query to reach the search field', async () => {
    const value = await instance.exec<string>(
      'return document.querySelector(\'[data-testid="chat-model-search"]\')?.value ?? ""',
    )
    return value === 'gpt4o'
  })
  assert.ok(await optionShown(instance, 'gpt-4o'), 'GPT-4o was filtered out')
  assert.ok(
    await optionShown(instance, 'gpt-4o-mini'),
    'GPT-4o mini was filtered out',
  )
  assert.ok(
    !(await optionShown(instance, 'claude')),
    'Claude stayed visible for a non-matching query',
  )
  assert.ok(
    !(await optionShown(instance, 'qwen')),
    'Qwen2.5 stayed visible for a non-matching query',
  )
  ctx.step('non-contiguous query filters to matching models')

  // Backspace from an option (focus moved there by Arrow Down) still edits the search.
  await instance.type('chat-model-search', KEY.arrowDown)
  await ctx.waitFor('an option to hold focus', async () =>
    instance.exec<boolean>(
      'return document.activeElement?.getAttribute("role") === "option"',
    ),
  )
  for (const key of Array(5).fill(KEY.backspace))
    await instance.typeToFocused(key)
  for (const model of MODELS) {
    assert.ok(
      await optionShown(instance, model.id),
      `${model.displayName} did not come back after clearing the search`,
    )
  }
  ctx.step('clearing the search restores the full list')

  await instance.type('chat-model-search', 'zzzzzzzz')
  await instance.waitForDisplayed('chat-model-search-empty')
  for (const model of MODELS) {
    assert.ok(
      !(await optionShown(instance, model.id)),
      `${model.displayName} stayed visible with no matching query`,
    )
  }
  ctx.step('a query with no match shows the empty hint')

  for (const key of Array(8).fill(KEY.backspace))
    await instance.type('chat-model-search', key)
  await instance.type('chat-model-search', 'claude')
  await instance.type('chat-model-search', KEY.arrowDown)
  await instance.typeToFocused(KEY.enter)
  await ctx.waitFor(
    'Claude to become the active model',
    async () => (await triggerTitle(instance))?.includes('Claude') ?? false,
  )
  ctx.step('keyboard select-and-close picks the highlighted match')

  // Picking a model closes only the model select, not the whole settings popover — it is still
  // open, so reopening the model select alone is enough.
  await instance.click('#chat-model-popover')
  await ctx.waitFor('the search field to be empty on reopen', async () => {
    const value = await instance.exec<string>(
      'return document.querySelector(\'[data-testid="chat-model-search"]\')?.value ?? ""',
    )
    return value === ''
  })
  for (const model of MODELS) {
    assert.ok(
      await optionShown(instance, model.id),
      `${model.displayName} was not shown again after reopening`,
    )
  }
  ctx.step('reopening resets the search and shows every model again')

  await instance.type('chat-model-search', 'zzzzzzzz')
  await instance.waitForDisplayed('chat-model-search-empty')
  await instance.typeToFocused(KEY.escape)
  await ctx.waitFor(
    'the model select to close',
    async () => !(await isShown(instance, '[data-testid="chat-model-search"]')),
  )
  // Escape may close the whole settings popover along with the model select.
  if (!(await isShown(instance, '#chat-model-popover')))
    await instance.click('chat-settings-trigger')
  await ctx.waitFor(
    'the model control to still name the active model',
    async () =>
      (
        await instance.exec<string>(
          "return document.querySelector('#chat-model-popover')?.textContent ?? ''",
        )
      ).includes('Claude'),
  )
  ctx.step('closing with a non-matching query keeps the active model shown')
})
