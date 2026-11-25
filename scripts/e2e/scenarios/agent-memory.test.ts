import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import { openStandardProfile } from '../lib/agent-profiles.ts'
import { openSettings, waitForLocation } from '../lib/settings.ts'
import { errorToasts } from '../lib/toasts.ts'

type Pointer = { id: string | null; title: string }
type Page = {
  id: string
  title: string
  body: string
  authorKind: string
  pointsTo: Pointer[]
  pointedFrom: Pointer[]
}
type List = { pages: Array<{ id: string; title: string }> }

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = [...document.querySelectorAll('[data-testid="' + arguments[0] + '"]')].find((el) => el.offsetParent !== null)
     return el ? el.textContent.trim() : ''`,
    [hook],
  )

const pages = async (instance: FlowInstance) =>
  unwrap<List>(
    'agent_memory_list',
    await instance.invoke('agent_memory_list', { args: { query: null } }),
  ).pages

const pageOf = async (instance: FlowInstance, title: string) => {
  const found = (await pages(instance)).find((page) => page.title === title)
  assert.ok(found, `no page "${title}" in the memory`)
  return unwrap<Page>(
    'agent_memory_page',
    await instance.invoke('agent_memory_page', { args: { id: found.id } }),
  )
}

// Spec 097, quickstart steps 1 to 4 (US3, FR-005, FR-024, FR-025): "KI → Gedächtnis" starts empty;
// the user adds a page whose text points to a page that does not exist yet, creates that page from
// the pointer, and follows the pointers both ways. A token in a page's text is refused with the
// reason named and the page stays as it was. The group "Gedächtnis" of "Standard" reads and writes
// from the factory. What an agent writes, and the question after foreign content, are the unit
// tests' and the measurement's (`agent_memory`, `chat::eval::everyday`).
scenario('agent-memory', { timeoutMs: 240_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-agent-memory' })

  await openStandardProfile(instance)
  await instance.waitForDisplayed('agent-memory-level')
  assert.match(
    await textOf(instance, 'agent-memory-level'),
    /Lesen und schreiben|Read and write/,
  )
  ctx.step('Standard reads and writes the memory from the factory')

  await openSettings(instance)
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  await instance.click('settings-row-ai.memory')
  await waitForLocation(instance, 'ai.memory')
  await instance.waitForDisplayed('agent-memory-empty')
  assert.deepEqual(await pages(instance), [])
  ctx.step('the memory starts empty')

  await instance.click('agent-memory-add')
  await waitForLocation(instance, 'ai.memory.page')
  await instance.type('agent-memory-title', 'Hund')
  await instance.type('agent-memory-description', 'Kims Hund Krümel')
  await instance.type(
    'agent-memory-body',
    'Krümel ist fünf Jahre alt. Geimpft wird er in der [[Praxis Wendt]].',
  )
  await instance.click('agent-memory-save')
  await instance.waitForDisplayed('agent-memory-page-title')
  await instance.waitForDisplayed('agent-memory-points-to-missing')
  const dog = await pageOf(instance, 'Hund')
  assert.equal(dog.authorKind, 'user')
  assert.deepEqual(dog.pointsTo, [{ id: null, title: 'Praxis Wendt' }])
  ctx.step('a page with a pointer to no page')

  // The pointer in the text offers to create its page, with the title filled in.
  await instance.click('[data-memory-missing="Praxis Wendt"]')
  await waitForLocation(instance, 'ai.memory.page')
  await ctx.waitFor(
    'the title to be filled in',
    async () =>
      (await instance.exec<string>(
        `return document.querySelector('#agent-memory-title')?.value ?? ''`,
      )) === 'Praxis Wendt',
  )
  await instance.type('agent-memory-description', 'Tierarztpraxis von Krümel')
  await instance.click('agent-memory-save')
  await instance.waitForDisplayed('agent-memory-pointed-from')
  assert.match(await textOf(instance, 'agent-memory-pointed-from'), /Hund/)
  ctx.step('the missing page created from its pointer')

  // Back to "Hund" through the page that points to it, then forward through its pointer.
  await instance.click('agent-memory-pointed-from')
  await ctx.waitFor('the page "Hund" to open', async () =>
    /Hund/.test(await textOf(instance, 'agent-memory-page-title')),
  )
  await instance.waitForDisplayed('agent-memory-points-to')
  await instance.click('[data-testid="agent-memory-text"] [data-memory-page]')
  await ctx.waitFor('the page "Praxis Wendt" to open', async () =>
    /Praxis Wendt/.test(await textOf(instance, 'agent-memory-page-title')),
  )
  assert.equal(
    (await pageOf(instance, 'Hund')).pointsTo[0]?.id,
    (await pageOf(instance, 'Praxis Wendt')).id,
  )
  ctx.step('the pointers lead both ways')

  // A token in the text is refused with its reason; the stored page is unchanged.
  await instance.click('agent-memory-edit')
  const token = ['sk', 'e2eMemoryCheck42'].join('-')
  await instance.type('agent-memory-body', ` Zugang: ${token}`)
  await instance.click('agent-memory-save')
  await ctx.waitFor('the refusal to show', async () =>
    (await errorToasts(instance)).some((text) =>
      /nie ins Gedächtnis|never go into the memory/.test(text),
    ),
  )
  await instance.waitForDisplayed('agent-memory-body')
  assert.ok(!(await pageOf(instance, 'Praxis Wendt')).body.includes(token))
  ctx.step('a secret is refused and not stored')
})
