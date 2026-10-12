import { after, describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { startProvider } from './provider.ts'
import type { Provider } from './provider.ts'

const providers: Provider[] = []

async function withProvider(behavior?: Parameters<typeof startProvider>[0]) {
  const provider = await startProvider(behavior)
  providers.push(provider)
  return provider
}

after(async () => {
  await Promise.all(providers.map((p) => p.close()))
})

/** Reads whole SSE events (`event: …\ndata: …\n\n`) off a fetch response body. */
async function readEvents(
  body: ReadableStream<Uint8Array>,
  count: number,
): Promise<Array<{ event: string; data: unknown }>> {
  const reader = body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  const events: Array<{ event: string; data: unknown }> = []
  while (events.length < count) {
    const { value, done } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })
    let boundary: number
    while ((boundary = buffer.indexOf('\n\n')) !== -1) {
      const chunk = buffer.slice(0, boundary)
      buffer = buffer.slice(boundary + 2)
      const eventLine = chunk.split('\n').find((l) => l.startsWith('event: '))
      const dataLine = chunk.split('\n').find((l) => l.startsWith('data: '))
      if (eventLine && dataLine) {
        events.push({
          event: eventLine.slice('event: '.length),
          data: JSON.parse(dataLine.slice('data: '.length)),
        })
      }
      if (events.length >= count) break
    }
  }
  await reader.cancel().catch(() => {})
  return events
}

describe('startProvider', () => {
  it('listens on 127.0.0.1 only', async () => {
    const provider = await withProvider()
    assert.match(provider.baseUrl, /^http:\/\/127\.0\.0\.1:\d+$/)
  })

  it('answers GET /v1/models with one model in the shape the adapter parses', async () => {
    const provider = await withProvider()
    const response = await fetch(`${provider.baseUrl}/v1/models`)
    assert.equal(response.status, 200)
    const body = (await response.json()) as {
      data: Array<{ id: string }>
      has_more: boolean
      first_id: string
      last_id: string
    }
    assert.equal(body.data.length, 1)
    assert.equal(body.data[0]?.id, 'stand-in-model')
    assert.equal(body.has_more, false)
    assert.equal(body.first_id, 'stand-in-model')
    assert.equal(body.last_id, 'stand-in-model')
  })

  it('answers anything else with 404', async () => {
    const provider = await withProvider()
    const response = await fetch(`${provider.baseUrl}/v1/nonsense`)
    assert.equal(response.status, 404)
  })

  it('stream-forever sends the start events then keeps sending deltas until the client drops it', async () => {
    const provider = await withProvider({
      kind: 'stream-forever',
      intervalMs: 20,
    })
    const controller = new AbortController()
    const response = await fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      body: '{}',
      signal: controller.signal,
    })
    const events = await readEvents(response.body!, 4)
    assert.deepEqual(
      events.slice(0, 2).map((e) => e.event),
      ['message_start', 'content_block_start'],
    )
    assert.ok(
      events.slice(2).every((e) => e.event === 'content_block_delta'),
      JSON.stringify(events),
    )

    const [connection] = provider.connections()
    assert.equal(connection?.closedAt, undefined)
    controller.abort()
    await new Promise((resolve) => setTimeout(resolve, 200))
    assert.ok(
      connection && connection.closedAt !== undefined,
      'closedAt must be set once the client drops the connection',
    )
  })

  it('stream-then-finish sends exactly that many deltas, then the stop events, and ends', async () => {
    const provider = await withProvider({
      kind: 'stream-then-finish',
      chunks: 3,
      intervalMs: 5,
    })
    const response = await fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      body: '{}',
    })
    const text = await response.text()
    const eventNames = text
      .split('\n\n')
      .filter((chunk) => chunk.length > 0)
      .map((chunk) => chunk.split('\n')[0]?.slice('event: '.length))
    assert.deepEqual(eventNames, [
      'message_start',
      'content_block_start',
      'content_block_delta',
      'content_block_delta',
      'content_block_delta',
      'content_block_stop',
      'message_delta',
      'message_stop',
    ])
  })

  it('error answers at once with that status and a JSON error body', async () => {
    const provider = await withProvider({
      kind: 'error',
      status: 529,
      body: {
        type: 'error',
        error: { type: 'overloaded_error', message: 'busy' },
      },
    })
    const response = await fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      body: '{}',
    })
    assert.equal(response.status, 529)
    assert.deepEqual(await response.json(), {
      type: 'error',
      error: { type: 'overloaded_error', message: 'busy' },
    })
  })

  it('a later behave() only applies to requests that arrive after it', async () => {
    const provider = await withProvider({ kind: 'error', status: 500 })
    const first = await fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      body: '{}',
    })
    assert.equal(first.status, 500)
    provider.behave({ kind: 'error', status: 402 })
    const second = await fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      body: '{}',
    })
    assert.equal(second.status, 402)
  })

  it('a script answers each request that offers tools with its next step', async () => {
    const provider = await withProvider({
      kind: 'script',
      steps: [
        { tool: 'files_list', input: { path: '/a' } },
        { text: 'fertig' },
      ],
    })
    const send = async (tools: unknown[]) =>
      (
        await fetch(`${provider.baseUrl}/v1/messages`, {
          method: 'POST',
          body: JSON.stringify({ tools }),
        })
      ).text()
    const call = await send([{ name: 'files_list' }])
    assert.match(call, /"type":"tool_use".*"name":"files_list"/)
    assert.match(call, /"partial_json":"\{\\"path\\":\\"\/a\\"\}"/)
    assert.match(call, /"stop_reason":"tool_use"/)
    assert.match(await send([]), /"text":"title"/)
    assert.match(await send([{ name: 'files_list' }]), /"text":"fertig"/)
    assert.match(await send([{ name: 'files_list' }]), /"text":"done"/)
  })

  it('a step builds its input from the request it answers', async () => {
    const provider = await withProvider({
      kind: 'script',
      steps: [
        {
          tool: 'files_list',
          input: (request) => ({
            path: (request as { messages: { content: string }[] }).messages[0]!
              .content,
          }),
        },
      ],
    })
    const call = await (
      await fetch(`${provider.baseUrl}/v1/messages`, {
        method: 'POST',
        body: JSON.stringify({
          tools: [{ name: 'files_list' }],
          messages: [{ content: '/b' }],
        }),
      })
    ).text()
    assert.match(call, /"partial_json":"\{\\"path\\":\\"\/b\\"\}"/)
  })

  it('records method, path and body, and never the credential headers', async () => {
    const provider = await withProvider({ kind: 'error', status: 500 })
    const sentinel = 'sk-test-sentinel-do-not-record'
    await fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      headers: { 'x-api-key': sentinel, 'content-type': 'application/json' },
      body: JSON.stringify({ model: 'stand-in-model' }),
    })
    const recorded = provider.requests()
    const last = recorded[recorded.length - 1]
    assert.equal(last?.method, 'POST')
    assert.equal(last?.path, '/v1/messages')
    assert.deepEqual(last?.body, { model: 'stand-in-model' })
    assert.ok(!JSON.stringify(recorded).includes(sentinel))
  })

  it('waitForOpen resolves with the connection once one is open', async () => {
    const provider = await withProvider({
      kind: 'stream-forever',
      intervalMs: 20,
    })
    const controller = new AbortController()
    const opened = provider.waitForOpen()
    fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      body: '{}',
      signal: controller.signal,
    }).catch(() => {})
    const connection = await opened
    assert.equal(connection.path, '/v1/messages')
    controller.abort()
  })

  it('close ends open streams and frees the port', async () => {
    const provider = await startProvider({
      kind: 'stream-forever',
      intervalMs: 20,
    })
    const response = await fetch(`${provider.baseUrl}/v1/messages`, {
      method: 'POST',
      body: '{}',
    })
    await readEvents(response.body!, 1)
    await provider.close()
    await assert.rejects(fetch(provider.baseUrl))
  })
})
