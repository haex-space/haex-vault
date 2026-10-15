import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import {
  connectProvider,
  createAndUnlock,
  startReply,
  unwrap,
  type FlowInstance,
} from '../lib/flows.ts'
import type { Provider, ScriptStep } from '../lib/provider.ts'
import { startRustfs } from '../lib/rustfs.ts'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import { openSettings, waitForLocation } from '../lib/settings.ts'
import {
  type Source,
  startImport,
  transfersDone,
} from '../lib/storage-files.ts'
import { connectStorage } from '../lib/storage-setup.ts'

// Spec 048 US6, quickstart §7 (T056): the agent of the chat, with "Lesen" for a storage, meets an
// encrypted folder in it. It lists the storage and sees only "verschlüsselter Ordner 1" with an
// opaque name; its access asks the user, naming the folder, the model and that it runs in the
// cloud (the rig's scripted model is an API model). "Lesen" without the checkbox holds for that
// turn: the agent reads the letter, and the next turn asks again. Remembered without "auch für
// Cloud-Modelle" it covers local models only, so the cloud model is asked once more. The settings
// list the remembered answer, and a revoke removes it. Needs Docker for RustFS; skipped without it.

const BUCKET = 'holzi-encrypted-agent-e2e'
const LETTER = 'Steuerbescheid für Anna, 312 Euro'

type Message = { role: string; content: unknown }
type Body = { tools?: { name: string }[]; messages?: Message[] }
type Permission = {
  kind: string
  target: string
  status: string
  reach?: string
}

/** The requests of the chat turns: those that offer tools. */
function turnRequests(provider: Provider): Body[] {
  return provider
    .requests()
    .filter((request) => request.method === 'POST')
    .map((request) => request.body as Body)
    .filter((body) => (body?.tools?.length ?? 0) > 0)
}

/** The text of the tool result a request hands back to the model. */
function toolResult(body: unknown): string {
  const last = (body as Body).messages?.at(-1)
  const blocks = Array.isArray(last?.content) ? last.content : []
  const result = blocks.find(
    (block: { type?: string }) => block.type === 'tool_result',
  ) as { content?: unknown } | undefined
  const content = result?.content
  if (typeof content === 'string') return content
  if (Array.isArray(content)) {
    return content.map((block: { text?: string }) => block.text ?? '').join('')
  }
  return JSON.stringify(content ?? null)
}

/** The path of the hidden folder in the listing the last tool returned. */
function hiddenPath(body: unknown): string {
  const listing = JSON.parse(toolResult(body)) as {
    entries: { path: string; encrypted?: boolean }[]
  }
  const hidden = listing.entries.find((entry) => entry.encrypted)
  if (!hidden) throw new Error(`no hidden folder in ${toolResult(body)}`)
  return hidden.path
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

const permissions = async (instance: FlowInstance) =>
  unwrap<Permission[]>(
    'files_agent_permissions',
    await instance.invoke('files_agent_permissions'),
  )

const questionText = (instance: FlowInstance) =>
  instance.exec<string>(
    `const el = document.querySelector('[data-testid="files-agent-permission"]')
     return el ? el.textContent : ''`,
  )

/** A turn that lists the storage and then the hidden folder by its opaque name. */
function listingTurn(source: string, then: ScriptStep[]): ScriptStep[] {
  return [
    { tool: 'find_actions', input: { query: 'Ordner auflisten' } },
    { tool: 'files_list', input: { source, path: '/' } },
    {
      tool: 'files_list',
      input: (request) => ({ source, path: hiddenPath(request) }),
    },
    ...then,
  ]
}

/** Waits for the question for the folder, checks what it names and answers it. */
async function answerQuestion(
  ctx: ScenarioContext,
  instance: FlowInstance,
  model: string,
  answer: 'read' | 'deny',
  remember: boolean,
) {
  await instance.waitForDisplayed('files-agent-permission', 30_000)
  const text = await questionText(instance)
  assert.match(text, /Unterlagen/)
  assert.match(text, /Fotos/)
  assert.ok(text.includes(model), `the model is named: ${text}`)
  assert.match(text, /in der Cloud|in the cloud/)
  if (remember) {
    await instance.click('files-agent-permission-remember')
    await instance.waitForDisplayed('files-agent-permission-also-cloud')
  }
  await instance.click(
    answer === 'read'
      ? 'files-agent-permission-read'
      : 'files-agent-permission-deny',
  )
  await ctx.waitFor(
    'the question to close',
    async () => (await questionText(instance)) === '',
  )
}

scenario(
  'files-encrypted-agent',
  { timeoutMs: 360_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    const device = deviceFiles('holzi-encrypted-agent-')
    ctx.onTeardown(() => device.remove())

    const instance = await ctx.startInstance()
    const name = 'e2e-files-encrypted-agent'
    await createAndUnlock(instance, { name })
    device.write('brief.txt', LETTER)
    const storageId = await connectStorage(instance, rustfs, BUCKET, 'Fotos')
    const source: Source = { kind: 'storage', storageId }
    // The page takes a deep link only after it handed the actions to Rust.
    await instance.navigate(
      `tauri://localhost/workspace/${encodeURIComponent(name)}?open=system.files`,
    )
    await instance.waitForDisplayed('files-sidebar')
    unwrap(
      'files_create_encrypted_folder',
      await instance.invoke('files_create_encrypted_folder', {
        source,
        path: '/',
        name: 'Unterlagen',
      }),
    )
    await startImport(
      instance,
      [device.path('brief.txt')],
      source,
      '/Unterlagen',
    )
    await transfersDone(ctx, instance, 'the letter to go in')
    unwrap(
      'files_agent_permission_set',
      await instance.invoke('files_agent_permission_set', {
        agentId: 'builtin',
        kind: 'storage',
        target: storageId,
        status: 'read',
      }),
    )
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
    ctx.step(
      'an encrypted folder with a letter, the storage granted for reading',
    )

    const storage = `storage:${storageId}`
    const provider = await ctx.provider({
      kind: 'script',
      steps: listingTurn(storage, [
        { tool: 'find_actions', input: { query: 'Datei lesen' } },
        {
          tool: 'files_read',
          input: { source: storage, path: '/Unterlagen/brief.txt' },
        },
        { text: 'fertig' },
      ]),
    })
    await connectProvider(instance, provider)

    // Turn 1: hidden, asked, read for this turn only.
    await startReply(instance, provider, 'Lies meinen Steuerbescheid')
    await answerQuestion(ctx, instance, provider.modelId, 'read', false)
    let requests = await waitForRequests(provider, 6)
    const top = toolResult(requests[2])
    assert.match(top, /verschlüsselter Ordner 1/)
    assert.doesNotMatch(top, /Unterlagen|brief|\.hxef/)
    assert.match(toolResult(requests[3]), /brief\.txt/)
    assert.match(toolResult(requests[5]), /312 Euro/)
    assert.deepEqual(
      (await permissions(instance)).filter((p) => p.kind === 'encryptedFolder'),
      [],
      'nothing stored without the checkbox',
    )
    ctx.step('hidden, asked naming folder, model and cloud, read for the turn')

    // Turn 2: the answer is gone; remembered now, for local models only.
    provider.behave({
      kind: 'script',
      steps: listingTurn(storage, [{ text: 'fertig' }]),
    })
    await startReply(instance, provider, 'Was liegt im Speicher?')
    await answerQuestion(ctx, instance, provider.modelId, 'read', true)
    requests = await waitForRequests(provider, 10)
    assert.match(toolResult(requests[8]), /verschlüsselter Ordner 1/)
    assert.match(toolResult(requests[9]), /brief\.txt/)
    const folders = (await permissions(instance)).filter(
      (p) => p.kind === 'encryptedFolder',
    )
    assert.equal(folders.length, 1, JSON.stringify(folders))
    assert.deepEqual([folders[0]?.status, folders[0]?.reach], ['read', 'local'])
    ctx.step('the next turn asks again; remembered for local models only')

    // Turn 3: a cloud model again: hidden, and asked once more.
    provider.behave({
      kind: 'script',
      steps: listingTurn(storage, [{ text: 'fertig' }]),
    })
    await startReply(instance, provider, 'Und jetzt?')
    await answerQuestion(ctx, instance, provider.modelId, 'deny', false)
    requests = await waitForRequests(provider, 14)
    assert.match(toolResult(requests[12]), /verschlüsselter Ordner 1/)
    assert.match(toolResult(requests[13]), /files_not_granted/)
    ctx.step('a grant for local models asks again for the cloud model')

    // The settings list the remembered answer; a revoke removes it.
    await openSettings(instance)
    await instance.click('settings-category-ai')
    await waitForLocation(instance, 'ai')
    await instance.click('settings-row-ai.files')
    await waitForLocation(instance, 'ai.files')
    const revoke = `agent-files-folder-revoke-builtin-${folders[0]!.target}`
    await instance.waitForDisplayed(revoke)
    const row = await instance.exec<string>(
      `return document.querySelector('[data-testid="' + arguments[0] + '"]').closest('li').textContent`,
      [revoke],
    )
    assert.match(row, /Unterlagen/)
    assert.match(row, /nur Modelle auf diesem Gerät|only models on this device/)
    await instance.click(revoke)
    await ctx.waitFor('the answer to be revoked', async () =>
      (await permissions(instance)).every((p) => p.kind !== 'encryptedFolder'),
    )
    ctx.step('the settings list the answer and revoke it')
  },
)
