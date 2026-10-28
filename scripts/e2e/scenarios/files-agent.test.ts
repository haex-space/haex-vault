import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { fileFixture } from '../lib/file-fixtures.ts'
import { visiblePanels } from '../lib/files-split.ts'
import {
  connectProvider,
  createAndUnlock,
  startReply,
  unwrap,
} from '../lib/flows.ts'
import type { Provider } from '../lib/provider.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction, wmSnapshot } from '../lib/settings.ts'

// Spec 044 US6 (T084, quickstart §6 step 1): a scripted model reaches files through the catalog. It
// finds the file actions with `find_actions` (they are not in the core offer), searches a folder of
// the device, reads a text file and the text of a PDF. The actions run in Rust (ADR 0011); what the model gets back
// is the tool result of the next request.

type Message = { role: string; content: unknown }
type Body = { tools?: { name: string }[]; messages?: Message[] }

/** The requests of the chat turn: those that offer tools. */
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

scenario('files-agent', { timeoutMs: 180_000 }, async (ctx) => {
  const files = deviceFiles('holzi-agent-')
  try {
    const instance = await ctx.startInstance()
    const name = 'e2e-files-agent'
    await createAndUnlock(instance, { name })
    // After the start: on Android it clears the app's data, where the folder lives.
    files.write('notiz-agent.txt', 'Hallo vom Agent')
    files.write('steuer.pdf', fileFixture('steuer.pdf'))
    // The page takes a deep link only after it handed the actions to Rust.
    await instance.navigate(
      `tauri://localhost/workspace/${encodeURIComponent(name)}?open=system.files`,
    )
    await instance.waitForDisplayed('files-sidebar')
    ctx.step('the actions are handed over')

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
    const provider = await ctx.provider({
      kind: 'script',
      steps: [
        { tool: 'find_actions', input: { query: 'Dateien suchen' } },
        {
          tool: 'files_search',
          input: { source: 'device', path: files.folder, query: 'notiz-agent' },
        },
        { tool: 'find_actions', input: { query: 'Datei lesen' } },
        {
          tool: 'files_read',
          input: {
            source: 'device',
            path: `${files.folder}/notiz-agent.txt`,
          },
        },
        {
          tool: 'files_read',
          input: { source: 'device', path: `${files.folder}/steuer.pdf` },
        },
        { text: 'fertig' },
      ],
    })
    await connectProvider(instance, provider)
    await startReply(instance, provider, 'Lies meine Notiz vom Agent')
    const requests = await waitForRequests(provider, 6)

    const offered = (requests[0]?.tools ?? []).map((tool) => tool.name)
    assert.ok(offered.includes('find_actions'), offered.join(', '))
    assert.ok(!offered.includes('files_search'), 'not in the core offer')
    assert.match(toolResult(requests[1]!), /files_search/)
    ctx.step('find_actions offers the file actions')

    const found = toolResult(requests[2]!)
    assert.match(found, /notiz-agent\.txt/)
    assert.doesNotMatch(found, /"error"/)
    ctx.step('files_search finds the note on the device')

    const read = toolResult(requests[4]!)
    assert.match(read, /Hallo vom Agent/)
    ctx.step('files_read gives its text')

    const pdf = toolResult(requests[5]!)
    assert.match(pdf, /Steuerbescheid 2025/)
    assert.match(pdf, /312 Euro/)
    ctx.step('files_read gives the text of a PDF')

    // Spec 062 T063: files.show opens a fresh file-browser tab and leaves an existing split alone.
    if (process.env.E2E_PLATFORM !== 'android') {
      await instance.click('files-sidebar-toggle')
      await instance.click('files-split-toggle')
      await ctx.waitFor(
        'the existing file-browser tab to show a split',
        async () => (await visiblePanels(instance)) === 2,
        { fixed: true, timeoutMs: 15_000 },
      )
      const before = await wmSnapshot(instance)
      const existing = before.windows
        .flatMap((window) => window.tabs)
        .find((tab) => tab.appId === 'system.files')
      assert.ok(existing, 'the initial file-browser tab exists')

      const shown = await runAction(instance, 'files.show', {
        source: 'device',
        path: `${files.folder}/notiz-agent.txt`,
      })
      assert.ok(shown.ok, JSON.stringify(shown))
      await instance.waitForDisplayed('files-viewer-text')
      const after = await wmSnapshot(instance)
      const fileTabs = after.windows
        .flatMap((window) => window.tabs)
        .filter((tab) => tab.appId === 'system.files')
      assert.equal(fileTabs.length, 2, 'files.show opens a second tab')
      assert.ok(
        fileTabs.some((tab) => tab.id === existing.id),
        'the original split tab remains open',
      )
      const activated = await runAction(instance, 'wm.tab.activate', {
        tabId: existing.id,
      })
      assert.ok(activated.ok, JSON.stringify(activated))
      await ctx.waitFor(
        'the original split to remain intact',
        async () => (await visiblePanels(instance)) === 2,
        { fixed: true, timeoutMs: 15_000 },
      )
      ctx.step('files.show opens a new tab and keeps the split')
    }
  } finally {
    files.remove()
  }
})
