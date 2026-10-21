import assert from 'node:assert/strict'
import { createServer, type IncomingMessage, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import type { Page } from '../lib/page.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  privateNetworkAddress,
  reachableFromDevice,
  type InstalledExtension,
} from '../lib/extensions.ts'

// Spec 055, US7 (T067, quickstart steps 2–6): the test extension `model-jobs`, the released
// vault-sdk bundled in, runs its page recognition on holzi's models. A stand-in server with the
// OpenAI-compatible interface answers for every model; holzi knows it twice, at `127.0.0.1` (this
// device) and at this machine's address in its private network (a server, which needs consent).
// The first job asks for a model and runs on this device; a job on the server asks for consent,
// held for the session only; after a restart the denied consent sends nothing; revoked models are
// asked for again. Broken output and the steps are covered by `src-tauri/src/extensions/bridge/`.

const PAGE = {
  regions: [
    {
      kind: 'text',
      box: [85, 228, 623, 323],
      text: '5. Raektion von Säurelösungen',
    },
    { kind: 'image', box: [102, 340, 412, 453] },
    {
      kind: 'table',
      box: [58, 470, 978, 986],
      rows: [
        ['Tag', 'Fach'],
        ['Di', ''],
      ],
    },
  ],
  rotation: 0,
}
const FIXED = { texts: ['5. Reaktion von Säurelösungen'] }

interface JobEnd {
  status?: string
  result?: { final: unknown }
  provenance?: Array<{ attempts: Array<{ model: string; place: string }> }>
  rejected?: { code?: number }
}

/** The answer of the stand-in to one request: what the step's instruction asks for. */
function answerFor(body: string): string {
  if (body.includes('[holzi-test:loop]')) return 'ja '.repeat(40)
  if (body.includes('[holzi-test:empty]')) return ''
  if (body.includes('[holzi-test:cjk]')) return '这是一页测试。'.repeat(5)
  return JSON.stringify(body.includes('Correct only spelling') ? FIXED : PAGE)
}

async function text(request: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = []
  for await (const chunk of request) chunks.push(chunk as Buffer)
  return Buffer.concat(chunks).toString()
}

/** The stand-in model server; `hosts` records the address of every chat request. */
async function standIn(): Promise<{
  server: Server
  port: number
  hosts: string[]
}> {
  const hosts: string[] = []
  const server = createServer((request, response) => {
    if (request.url === '/v1/models') {
      response.writeHead(200, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ data: [{ id: 'vision' }] }))
      return
    }
    void text(request).then((body) => {
      hosts.push(request.headers.host ?? '')
      const content = answerFor(body)
      response.writeHead(200, { 'content-type': 'text/event-stream' })
      response.write(
        `data: ${JSON.stringify({ choices: [{ delta: { content } }] })}\n\n`,
      )
      response.end('data: [DONE]\n\n')
    })
  })
  await new Promise<void>((resolve) => server.listen(0, '0.0.0.0', resolve))
  const { port } = server.address() as AddressInfo
  reachableFromDevice(port)
  return { server, port, hosts }
}

/** Adds the stand-in at `address` as an OpenAI-compatible server and states that it takes images. */
async function addServer(
  page: Page,
  name: string,
  address: string,
): Promise<string> {
  const added = unwrap<{ provider: { id: string }; modelCount?: number }>(
    'add_provider',
    await page.invoke('add_provider', {
      args: {
        kind: 'api_key',
        name,
        adapter: 'openai_compatible',
        baseUrl: address,
      },
    }),
  )
  assert.equal(added.modelCount, 1, `${name} lists its model`)
  const [model] = unwrap<Array<{ id: string }>>(
    'list_provider_models',
    await page.invoke('list_provider_models', {
      providerId: added.provider.id,
    }),
  )
  unwrap(
    'set_model_user_capabilities',
    await page.invoke('set_model_user_capabilities', {
      args: { modelId: model!.id, acceptsImages: true },
    }),
  )
  return model!.id
}

scenario('extension-model-jobs', { timeoutMs: 300_000 }, async (ctx) => {
  const stand = await standIn()
  try {
    const group = await ctx.group({ users: { anna: ['laptop'] } })
    const device = group.device('anna/laptop')
    let page = device.page
    const extension = await install(page, fixture('e2e', 'model-jobs'))
    const here = await addServer(
      page,
      'Dieses Gerät',
      `http://127.0.0.1:${stand.port}/v1`,
    )
    await openFromLauncher(page, extension)

    const run = async (mode: string): Promise<void> => {
      await inFrame(page, extension, 'window.modelJobs.run(arguments[0])', [
        mode,
      ])
    }
    const end = async (): Promise<JobEnd> => {
      // The driver hands an unset `last` back as null.
      let last: JobEnd | null = null
      await ctx.waitFor(
        'the job to end',
        async () => {
          last = await inFrame<JobEnd | null>(
            page,
            extension,
            'return window.modelJobs.last ? JSON.parse(JSON.stringify(window.modelJobs.last)) : null',
          )
          return last !== null
        },
        { timeoutMs: 60_000 },
      )
      return last!
    }
    const asked = (testId: string) =>
      page.waitForDisplayed(
        `[data-testid="extension-permission-request"] [data-testid="${testId}"]`,
        15_000,
      )

    // Quickstart steps 2 and 3: no grant, holzi asks for a model; allowed and remembered.
    await run('page')
    await asked('extension-model-pick-vision')
    await page.click('extension-permission-allow')
    const first = await end()
    assert.equal(first.status, 'completed', JSON.stringify(first))
    assert.deepEqual(first.result?.final, FIXED)
    assert.deepEqual(
      first.provenance?.map((step) => step.attempts.map((a) => a.place)),
      [['this_device'], ['this_device']],
    )
    ctx.step('a job asks for a model, then reads and corrects on this device')

    // Quickstart step 5: a model on a server in the private network, granted and ranked first,
    // asks for consent before anything is sent; allowed for the session only.
    const server = `${privateNetworkAddress()}:${stand.port}`
    const school = await addServer(page, 'Schul-Server', `http://${server}/v1`)
    unwrap(
      'extension_model_grants_set',
      await page.invoke('extension_model_grants_set', {
        args: { extensionId: extension.id, modelId: school, effort: 'medium' },
      }),
    )
    unwrap(
      'extension_model_grants_reorder',
      await page.invoke('extension_model_grants_reorder', {
        args: { extensionId: extension.id, modelIds: [school, here] },
      }),
    )
    const sentToServer = () =>
      stand.hosts.filter((host) => host === server).length
    await run('page')
    await asked('extension-model-destination')
    assert.equal(sentToServer(), 0, 'nothing is sent before the consent')
    await page.click('extension-permission-remember')
    await page.click('extension-permission-allow')
    const second = await end()
    assert.equal(second.status, 'completed', JSON.stringify(second))
    assert.equal(second.provenance?.[0]?.attempts[0]?.place, 'network')
    assert.equal(sentToServer(), 2)
    ctx.step('a server asks for consent first, then reads the page')

    // Quickstart step 6: the consent holds for the session, …
    await run('page')
    const third = await end()
    assert.equal(third.status, 'completed', JSON.stringify(third))
    assert.equal(sentToServer(), 4, 'no second question in the session')
    // … and ends with it: after a restart holzi asks again, and a denial sends nothing.
    await device.restart()
    page = device.page
    await openFromLauncher(page, extension)
    await run('page')
    await asked('extension-model-destination')
    await page.click('extension-permission-remember')
    await page.click('extension-permission-deny')
    const denied = await end()
    assert.equal(denied.rejected?.code, 1002, JSON.stringify(denied))
    assert.equal(sentToServer(), 4, 'a denied consent sends nothing')
    ctx.step('the consent holds for the session only, a denial sends nothing')

    // Revoked models are no longer used: the next job asks for a model again.
    for (const modelId of [school, here]) {
      unwrap(
        'extension_model_grants_revoke',
        await page.invoke('extension_model_grants_revoke', {
          args: { extensionId: extension.id, modelId },
        }),
      )
    }
    await run('page')
    await asked('extension-model-pick-vision')
    await page.click('extension-permission-deny')
    assert.equal((await end()).rejected?.code, 1002)
    assert.equal(sentToServer(), 4)
    ctx.step('revoked models are asked for again')
  } finally {
    stand.server.close()
  }
})
