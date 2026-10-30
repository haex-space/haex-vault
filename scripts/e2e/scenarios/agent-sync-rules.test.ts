import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import {
  connectProvider,
  createAndUnlock,
  startReply,
  unwrap,
  type FlowInstance,
} from '../lib/flows.ts'
import type { Provider } from '../lib/provider.ts'
import { scenario } from '../lib/scenario.ts'
import { openSettings, waitForLocation } from '../lib/settings.ts'

// Spec 061 US5 (T067, quickstart §5): a scripted model manages sync rules. Without the permission
// "Sync-Regeln verwalten" holzi refuses to list them. Turned on in Einstellungen → Agents → Dateien,
// creating a rule shows the confirmation with its places, direction and delete mode, also in the
// approval mode "auto": declined, no rule comes into being; allowed, the rule is stored and runs on
// its own.

type Message = { role: string; content: unknown }
type Body = { tools?: { name: string }[]; messages?: Message[] }
type Rule = { rule: { name: string } }

/** The requests of the chat turns: those that offer tools. */
function turnRequests(provider: Provider): Body[] {
  return provider
    .requests()
    .filter((request) => request.method === 'POST')
    .map((request) => request.body as Body)
    .filter((body) => (body?.tools?.length ?? 0) > 0)
}

/** The text of the tool result a request hands back to the model. */
function toolResult(body: Body): string {
  const last = body.messages?.at(-1)
  const blocks = Array.isArray(last?.content) ? last.content : []
  const result = blocks.find(
    (block: { type?: string }) => block.type === 'tool_result',
  ) as { content?: unknown } | undefined
  return JSON.stringify(result?.content ?? null)
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

const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? el.textContent.trim() : ''`,
    [hook],
  )

const rules = async (instance: FlowInstance) =>
  unwrap<Rule[]>('sync_rules_list', await instance.invoke('sync_rules_list'))

scenario('agent-sync-rules', { timeoutMs: 240_000 }, async (ctx) => {
  const source = deviceFiles('holzi-agent-sync-a-')
  const target = deviceFiles('holzi-agent-sync-b-')
  try {
    const instance = await ctx.startInstance()
    const name = 'e2e-agent-sync-rules'
    await createAndUnlock(instance, { name })
    // After the start: on Android it clears the app's data, where the folders live.
    source.write('foto.txt', 'Foto')
    target.write('.keep', '')
    // The page takes a deep link only after it handed the actions to Rust.
    await instance.navigate(
      `tauri://localhost/workspace/${encodeURIComponent(name)}?open=system.files`,
    )
    await instance.waitForDisplayed('files-sidebar')
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
    const draft = (ruleName: string) => ({
      name: ruleName,
      placeA: { source: { kind: 'device' }, path: source.folder },
      placeB: { source: { kind: 'device' }, path: target.folder },
      direction: 'aToB',
      deleteMode: 'keep',
      intervalSecs: 900,
      enabled: true,
      allowDecrypted: false,
    })

    const provider = await ctx.provider({
      kind: 'script',
      steps: [
        { tool: 'find_actions', input: { query: 'Sync-Regeln' } },
        { tool: 'syncRules_list', input: {} },
        { text: 'fertig' },
      ],
    })
    await connectProvider(instance, provider)
    await startReply(instance, provider, 'Welche Sync-Regeln gibt es?')
    const refused = await waitForRequests(provider, 3)
    assert.match(toolResult(refused[2]!), /sync_rules_not_granted/)
    ctx.step('without the permission holzi refuses')

    await openSettings(instance)
    await instance.click('settings-category-ai')
    await waitForLocation(instance, 'ai')
    await instance.click('settings-row-ai.files')
    await waitForLocation(instance, 'ai.files')
    await instance.click('agent-files-sync-rules-builtin')
    await ctx.waitFor('the permission to be stored', async () =>
      unwrap<{ kind: string; status: string }[]>(
        'files_agent_permissions',
        await instance.invoke('files_agent_permissions'),
      ).some((row) => row.kind === 'syncRules' && row.status === 'granted'),
    )
    ctx.step('the permission is turned on in the settings')

    provider.behave({
      kind: 'script',
      steps: [
        { tool: 'find_actions', input: { query: 'Sync-Regel anlegen' } },
        { tool: 'syncRules_create', input: draft('Abgelehnt') },
        { tool: 'syncRules_create', input: draft('Fotos') },
        { text: 'fertig' },
      ],
    })
    await startReply(instance, provider, 'Sichere meine Fotos laufend')

    await instance.waitForDisplayed('files-sync-rule-confirm', 30_000)
    assert.equal(
      await textOf(instance, 'files-sync-rule-confirm-placeA'),
      source.folder,
    )
    assert.equal(
      await textOf(instance, 'files-sync-rule-confirm-placeB'),
      target.folder,
    )
    assert.match(
      await textOf(instance, 'files-sync-rule-confirm-direction'),
      /Quelle → Ziel|Source → target/,
    )
    assert.match(
      await textOf(instance, 'files-sync-rule-confirm-deleteMode'),
      /Behalten|Keep/,
    )
    ctx.step('the confirmation shows places, direction and delete mode')

    await instance.click('files-sync-rule-confirm-deny')
    const declined = await waitForRequests(provider, 6)
    assert.match(toolResult(declined[5]!), /denied_by_user/)
    assert.deepEqual(await rules(instance), [])
    ctx.step('declined, no rule comes into being')

    // The second call asks anew; the first dialog is gone by now.
    await ctx.waitFor('the confirmation of the second rule', async () =>
      (await textOf(instance, 'files-sync-rule-confirm')).includes('Fotos'),
    )
    await instance.click('files-sync-rule-confirm-allow')
    const allowed = await waitForRequests(provider, 7)
    assert.doesNotMatch(toolResult(allowed[6]!), /"error"/)
    assert.deepEqual(
      (await rules(instance)).map((view) => view.rule.name),
      ['Fotos'],
    )
    await ctx.waitFor(
      'the rule to copy the file on its own',
      async () => target.files().includes('foto.txt'),
      { timeoutMs: 30_000 },
    )
    ctx.step('allowed, the rule is stored and runs without the agent')
  } finally {
    source.remove()
    target.remove()
  }
})
