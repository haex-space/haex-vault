import assert from 'node:assert/strict'
import {
  connectProvider,
  createAndUnlock,
  startReply,
  unwrap,
  type FlowInstance,
} from '../lib/flows.ts'
import type { Provider } from '../lib/provider.ts'
import { createContact } from '../lib/contacts.ts'
import { readShare } from '../lib/share.ts'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'

// Spec 051 US6-1 to US6-3 (quickstart `agent-contacts`): a scripted model reaches the contacts
// through the catalog (`find_actions`, then `contacts_search` and `contacts_update`). Without a
// grant holzi asks; "Ablehnen" is kept and the agent learns nothing. With "Lesen" set in the
// settings it finds Ben but may not change him: the write question, denied, keeps the read grant.
// "Immer erlauben" lets the change through, and it is stored like one made in holzi. That it
// reaches the phones is the phone sync's rule, tested in `contacts/agent_tests.rs`.
// Spec 060 US5: `contacts_share` asks in the share dialog on every call, in "Auto" too and without a
// second question of the chat. "Ablehnen" gives the agent nothing of Ben; after "Freigeben" with the
// phone number deselected the text the agent gets lacks it.

type Message = { role: string; content: unknown }
type Body = { tools?: { name: string }[]; messages?: Message[] }

/** The requests of the chat turns: those that offer tools. */
function turnRequests(provider: Provider): Body[] {
  return provider
    .requests()
    .filter((request) => request.method === 'POST')
    .map((request) => request.body as Body)
    .filter((body) => (body?.tools?.length ?? 0) > 0)
}

/** The text of the tool result a request hands back to the model. */
function toolResult(body: Body | undefined): string {
  const last = body?.messages?.at(-1)
  const blocks = Array.isArray(last?.content) ? last.content : []
  const result = blocks.find(
    (block: { type?: string }) => block.type === 'tool_result',
  ) as { content?: unknown } | undefined
  const content = result?.content
  if (Array.isArray(content)) {
    return content.map((block: { text?: string }) => block.text ?? '').join('')
  }
  return JSON.stringify(content ?? null)
}

async function waitForRequests(provider: Provider, count: number) {
  const end = Date.now() + 60_000
  for (;;) {
    const requests = turnRequests(provider)
    if (requests.length >= count) return requests
    if (Date.now() >= end) {
      throw new Error(
        `the model was asked ${requests.length} times, expected ${count}`,
      )
    }
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

/** Waits for a tool result whose content proves that the requested action finished. */
async function waitForToolResult(
  provider: Provider,
  matches: RegExp,
): Promise<string> {
  const end = Date.now() + 60_000
  for (;;) {
    const result = turnRequests(provider)
      .map((request) => toolResult(request))
      .find((text) => matches.test(text))
    if (result !== undefined) return result
    if (Date.now() >= end)
      throw new Error(`no tool result matched ${matches} within 60000 ms`)
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

const questionText = (instance: FlowInstance) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="files-agent-permission"]')
     return el ? el.textContent : ''`,
  )

/** Waits for the question for the contacts, checks what it asks for and answers it; `closes`: no
 * other question follows at once. */
async function answer(
  ctx: ScenarioContext,
  instance: FlowInstance,
  wants: RegExp,
  button: 'deny' | 'once' | 'always',
  closes = true,
) {
  await instance.waitForDisplayed('files-agent-permission', 30_000)
  const text = await questionText(instance)
  assert.match(text, /Kontakte/)
  assert.match(text, wants)
  await instance.click(`files-agent-permission-${button}`)
  if (!closes) return
  await ctx.waitFor(
    'the question to close',
    async () => (await questionText(instance)) === '',
  )
}

const contactsRows = async (instance: FlowInstance) =>
  unwrap<Array<{ kind: string; target: string; status: string }>>(
    'files_agent_permissions',
    await instance.invoke('files_agent_permissions'),
  )
    .filter((row) => row.kind === 'contacts')
    .map((row) => [row.target, row.status])

scenario('agent-contacts', { timeoutMs: 240_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  const name = 'e2e-agent-contacts'
  await createAndUnlock(instance, { name })
  const ben = await createContact(instance, {
    head: { givenName: 'Ben', familyName: 'Krüger' },
    values: [{ kind: 'phone', label: 'mobile', value: '0170 1111' }],
    groups: [],
  })
  const valueId = unwrap<{ values: Array<{ id: string }> }>(
    'contacts_get',
    await instance.invoke('contacts_get', { args: { id: ben } }),
  ).values[0]!.id
  // The page takes a deep link only after it handed the actions to Rust.
  await instance.navigate(
    `tauri://localhost/workspace/${encodeURIComponent(name)}?open=system.contacts`,
  )
  await instance.waitForDisplayed('contacts-search')
  unwrap(
    'set_pref',
    await instance.invoke('set_pref', {
      args: {
        scope: { kind: 'vault' },
        key: 'chat.permission_mode',
        value: 'auto',
      },
    }),
  )

  const search = { tool: 'contacts_search', input: { query: 'Ben' } }
  const update = {
    tool: 'contacts_update',
    input: {
      id: ben,
      values: [
        { id: valueId, kind: 'phone', label: 'mobile', value: '0170 2222' },
      ],
    },
  }
  const provider = await ctx.provider({
    kind: 'script',
    steps: [
      { tool: 'find_actions', input: { query: 'Kontakte suchen' } },
      search,
      { text: 'fertig' },
    ],
  })
  await connectProvider(instance, provider)

  // Turn 1: no grant; holzi asks, "Ablehnen" is kept.
  await startReply(instance, provider, 'Wie ist Bens Nummer?')
  await answer(ctx, instance, /Kontakte lesen\./, 'deny')
  let requests = await waitForRequests(provider, 3)
  const refused = toolResult(requests[2])
  assert.match(refused, /error/i, refused)
  assert.doesNotMatch(refused, /Krüger|0170/)
  assert.deepEqual(await contactsRows(instance), [['*', 'denied']])
  ctx.step('without a grant: asked, denied, nothing about the contacts')

  // The user allows reading in the settings.
  unwrap(
    'files_agent_permission_set',
    await instance.invoke('files_agent_permission_set', {
      agentId: 'builtin',
      kind: 'contacts',
      target: '*',
      status: 'read',
    }),
  )

  // Turn 2: Ben is found without a question; changing him asks for writing.
  provider.behave({
    kind: 'script',
    steps: [
      { tool: 'find_actions', input: { query: 'Kontakte suchen' } },
      search,
      { tool: 'find_actions', input: { query: 'Kontakt ändern' } },
      update,
      update,
      { text: 'fertig' },
    ],
  })
  await startReply(instance, provider, 'Ändere Bens Nummer')
  await answer(ctx, instance, /lesen und ändern/, 'deny', false)
  const found = toolResult((await waitForRequests(provider, 6))[5])
  assert.match(found, /Ben Krüger/)
  assert.match(found, /0170 1111/)
  ctx.step('with "Lesen" the agent finds Ben')

  requests = await waitForRequests(provider, 8)
  const notChanged = toolResult(requests[7])
  assert.match(notChanged, /error/i, notChanged)
  assert.deepEqual(await contactsRows(instance), [['*', 'read']])
  ctx.step('a read grant does not change Ben; the denied write keeps it')

  await answer(ctx, instance, /lesen und ändern/, 'always')
  requests = await waitForRequests(provider, 9)
  assert.match(toolResult(requests[8]), /0170 2222/)
  const stored = unwrap<{
    values: Array<{ value: string }>
    origin: { kind: string }
  }>(
    'contacts_get',
    await instance.invoke('contacts_get', { args: { id: ben } }),
  )
  assert.deepEqual(
    stored.values.map((value) => value.value),
    ['0170 2222'],
  )
  assert.equal(stored.origin.kind, 'holzi')
  assert.deepEqual(await contactsRows(instance), [['*', 'readWrite']])
  ctx.step('"Immer erlauben" lets the change through and is kept')

  // Turn 4 (spec 060 US5): the agent shares Ben. One dialog per call: "Ablehnen" first.
  const share = { tool: 'contacts_share', input: { contactId: ben } }
  provider.behave({
    kind: 'script',
    steps: [
      { tool: 'find_actions', input: { query: 'Kontakt teilen' } },
      share,
      share,
      { text: 'fertig' },
    ],
  })
  await startReply(instance, provider, 'Gib mir Bens Kontakt als Link')
  await instance.waitForDisplayed('contacts-share-confirm-intro', 30_000)
  const asked = await instance.exec<string>(
    `return document.querySelector('[data-testid="contacts-share-confirm-intro"]')?.textContent ?? ''`,
  )
  assert.match(asked, /Ben Krüger/)
  // The chat's own approval did not come first, although Auto asks for risky actions.
  assert.equal(await questionText(instance), '')
  await instance.click('contacts-share-confirm-deny')
  const declined = await waitForToolResult(provider, /denied_by_user/)
  assert.match(declined, /denied_by_user/, declined)
  assert.doesNotMatch(declined, /Krüger|0170|haex-vault:/)
  ctx.step('"Ablehnen" gives the agent nothing of Ben')

  // The same call asks again; this time the phone number is taken out before "Freigeben".
  await instance.waitForDisplayed('contacts-share-confirm-intro', 30_000)
  await instance.click(`contacts-share-check-${valueId}`)
  await instance.click('contacts-share-confirm-allow')
  const shared = JSON.parse(
    await waitForToolResult(provider, /haex-vault:/),
  ) as { link: string }
  const text = readShare(shared.link)
  assert.equal(text.name, 'Ben Krüger')
  assert.deepEqual(text.values, [])
  ctx.step('"Freigeben" with the number removed hands over a text without it')
})
