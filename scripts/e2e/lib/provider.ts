// A stand-in for an external model provider of the Anthropic kind: plays GET /v1/models and
// POST /v1/messages exactly as the application's own adapter expects (src-tauri/src/adapters/anthropic.rs),
// so a reply can run, stall or fail without the internet and without any application change
// (contracts/stand-in-provider.md).
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import http from 'node:http'
import type { IncomingMessage, ServerResponse } from 'node:http'
import https from 'node:https'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { reachFromDevice } from './platform/reach.ts'

const MODEL_ID = 'stand-in-model'

export type Behavior =
  | { kind: 'stream-forever'; intervalMs?: number; text?: string }
  | {
      kind: 'stream-then-finish'
      chunks?: number
      intervalMs?: number
      text?: string
    }
  | { kind: 'error'; status?: number; body?: unknown }
  /** A model that calls tools: each request that offers tools gets the next step, a request
   * without tools (a thread title) a short text. */
  | { kind: 'script'; steps: ScriptStep[] }

/** One answer of a scripted model: a call of `tool` with `input`, or a closing text. An `input`
 * given as a function is built from the request it answers, so a step can use what the last tool
 * returned (a name only the app knows). */
export type ScriptStep =
  | {
      tool: string
      input:
        | Record<string, unknown>
        | ((request: unknown) => Record<string, unknown>)
    }
  | { text: string }

export interface Connection {
  id: number
  openedAt: number
  closedAt?: number
  method: string
  path: string
}

export interface RecordedRequest {
  id: number
  at: number
  method: string
  path: string
  body?: unknown
}

export interface WaitForOpenOptions {
  timeoutMs?: number
}

export interface ProviderOptions {
  models?: StandInModel[]
  /**
   * Serve HTTPS with a certificate nobody trusts, made fresh for the run (spec 043 FR-024): a
   * client that checks certificates refuses it in the handshake, before any request.
   */
  selfSignedTls?: boolean
}

export interface Provider {
  baseUrl: string
  /** TLS handshakes the client broke off (only with `selfSignedTls`). */
  refusedHandshakes(): number
  modelId: string
  behave(behavior: Behavior): void
  connections(): Connection[]
  requests(): RecordedRequest[]
  waitForOpen(options?: WaitForOpenOptions): Promise<Connection>
  close(): Promise<void>
}

function sseEvent(event: string, data: unknown): string {
  return `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`
}

export interface StandInModel {
  id: string
  displayName: string
}

const DEFAULT_MODELS: StandInModel[] = [
  { id: MODEL_ID, displayName: 'Stand-in model' },
]

function modelsBody(models: StandInModel[]) {
  const ids = models.map((m) => m.id)
  return {
    data: models.map((m) => ({
      id: m.id,
      display_name: m.displayName,
      type: 'model',
      created_at: '2026-01-01T00:00:00Z',
      max_input_tokens: 200000,
    })),
    has_more: false,
    first_id: ids[0],
    last_id: ids[ids.length - 1],
  }
}

function messageStartEvents(connectionId: number): string {
  return (
    sseEvent('message_start', {
      type: 'message_start',
      message: { id: `msg_${connectionId}`, usage: { input_tokens: 1 } },
    }) +
    sseEvent('content_block_start', {
      type: 'content_block_start',
      index: 0,
      content_block: { type: 'text', text: '' },
    })
  )
}

function textDelta(text: string): string {
  return sseEvent('content_block_delta', {
    type: 'content_block_delta',
    index: 0,
    delta: { type: 'text_delta', text },
  })
}

function messageEndEvents(outputTokens: number): string {
  return (
    sseEvent('content_block_stop', { type: 'content_block_stop', index: 0 }) +
    sseEvent('message_delta', {
      type: 'message_delta',
      delta: { stop_reason: 'end_turn' },
      usage: { output_tokens: outputTokens },
    }) +
    sseEvent('message_stop', { type: 'message_stop' })
  )
}

function streamForever(
  res: ServerResponse,
  connectionId: number,
  behavior: { intervalMs?: number; text?: string },
): void {
  const intervalMs = behavior.intervalMs ?? 100
  const text = behavior.text ?? 'tick '
  res.writeHead(200, { 'content-type': 'text/event-stream' })
  res.write(messageStartEvents(connectionId))
  const timer = setInterval(() => res.write(textDelta(text)), intervalMs)
  res.on('close', () => clearInterval(timer))
}

function streamThenFinish(
  res: ServerResponse,
  connectionId: number,
  behavior: { chunks?: number; intervalMs?: number; text?: string },
): void {
  const total = behavior.chunks ?? 5
  const intervalMs = behavior.intervalMs ?? 200
  const text = behavior.text ?? 'tick '
  res.writeHead(200, { 'content-type': 'text/event-stream' })
  res.write(messageStartEvents(connectionId))
  let sent = 0
  const timer = setInterval(() => {
    sent += 1
    res.write(textDelta(text))
    if (sent >= total) {
      clearInterval(timer)
      res.write(messageEndEvents(total))
      res.end()
    }
  }, intervalMs)
  res.on('close', () => clearInterval(timer))
}

function scripted(
  res: ServerResponse,
  connectionId: number,
  step: ScriptStep,
  request: unknown,
): void {
  res.writeHead(200, { 'content-type': 'text/event-stream' })
  if ('text' in step) {
    res.write(messageStartEvents(connectionId))
    res.write(textDelta(step.text))
    res.end(messageEndEvents(1))
    return
  }
  res.end(
    sseEvent('message_start', {
      type: 'message_start',
      message: { id: `msg_${connectionId}`, usage: { input_tokens: 1 } },
    }) +
      sseEvent('content_block_start', {
        type: 'content_block_start',
        index: 0,
        content_block: {
          type: 'tool_use',
          id: `toolu_${connectionId}`,
          name: step.tool,
          input: {},
        },
      }) +
      sseEvent('content_block_delta', {
        type: 'content_block_delta',
        index: 0,
        delta: {
          type: 'input_json_delta',
          partial_json: JSON.stringify(
            typeof step.input === 'function' ? step.input(request) : step.input,
          ),
        },
      }) +
      sseEvent('content_block_stop', { type: 'content_block_stop', index: 0 }) +
      sseEvent('message_delta', {
        type: 'message_delta',
        delta: { stop_reason: 'tool_use' },
        usage: { output_tokens: 1 },
      }) +
      sseEvent('message_stop', { type: 'message_stop' }),
  )
}

function errorBody(behavior: { status?: number; body?: unknown }) {
  return (
    behavior.body ?? {
      type: 'error',
      error: { type: 'api_error', message: 'stand-in provider error' },
    }
  )
}

/**
 * Starts the stand-in on `127.0.0.1`, on a port the operating system chooses. Started and closed by the
 * scenario context; it never outlives its scenario.
 */
/** A key and a self-signed certificate for `127.0.0.1`, valid for a day, made with `openssl`. */
export function selfSignedCertificate(): { key: Buffer; cert: Buffer } {
  const folder = mkdtempSync(join(tmpdir(), 'holzi-e2e-tls-'))
  try {
    const [key, cert] = [join(folder, 'key.pem'), join(folder, 'cert.pem')]
    execFileSync(
      'openssl',
      [
        'req',
        '-x509',
        '-newkey',
        'ec',
        '-pkeyopt',
        'ec_paramgen_curve:prime256v1',
        '-nodes',
        '-subj',
        '/CN=127.0.0.1',
        '-addext',
        'subjectAltName=IP:127.0.0.1',
        '-days',
        '1',
        '-keyout',
        key,
        '-out',
        cert,
      ],
      { stdio: 'ignore' },
    )
    return { key: readFileSync(key), cert: readFileSync(cert) }
  } finally {
    rmSync(folder, { recursive: true, force: true })
  }
}

export async function startProvider(
  initial?: Behavior,
  options?: ProviderOptions,
): Promise<Provider> {
  let behavior: Behavior = initial ?? { kind: 'stream-then-finish' }
  /** The next step of a `script`. */
  let scriptAt = 0
  const models = options?.models ?? DEFAULT_MODELS
  const connections: Connection[] = []
  const requests: RecordedRequest[] = []
  let nextRequestId = 1
  let nextConnectionId = 1
  let openWaiter: ((connection: Connection) => void) | undefined

  const handle = (req: IncomingMessage, res: ServerResponse) => {
    const chunks: Buffer[] = []
    req.on('data', (chunk: Buffer) => chunks.push(chunk))
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8')
      let body: unknown
      try {
        body = raw ? JSON.parse(raw) : undefined
      } catch {
        body = raw
      }
      const method = req.method ?? ''
      const url = req.url ?? ''
      requests.push({
        id: nextRequestId++,
        at: Date.now(),
        method,
        path: url,
        body,
      })

      if (method === 'GET' && url.startsWith('/v1/models')) {
        res.writeHead(200, { 'content-type': 'application/json' })
        res.end(JSON.stringify(modelsBody(models)))
        return
      }
      if (method === 'POST' && url === '/v1/messages') {
        const connection: Connection = {
          id: nextConnectionId++,
          openedAt: Date.now(),
          method,
          path: url,
        }
        connections.push(connection)
        // The socket's own close, not the response's, so an aborted stream is recorded when the
        // application drops the connection rather than when this server finishes writing.
        req.socket.on('close', () => {
          connection.closedAt = Date.now()
        })
        const waiter = openWaiter
        openWaiter = undefined
        waiter?.(connection)

        const current = behavior
        if (current.kind === 'error') {
          res.writeHead(current.status ?? 500, {
            'content-type': 'application/json',
          })
          res.end(JSON.stringify(errorBody(current)))
          return
        }
        if (current.kind === 'stream-forever') {
          streamForever(res, connection.id, current)
          return
        }
        if (current.kind === 'script') {
          const tools = (body as { tools?: unknown[] } | undefined)?.tools
          const step = tools?.length
            ? (current.steps[scriptAt++] ?? { text: 'done' })
            : { text: 'title' }
          scripted(res, connection.id, step, body)
          return
        }
        streamThenFinish(res, connection.id, current)
        return
      }
      res.writeHead(404, { 'content-type': 'application/json' })
      res.end(JSON.stringify({ error: 'not found' }))
    })
  }
  let refusedHandshakes = 0
  const server = options?.selfSignedTls
    ? https.createServer(selfSignedCertificate(), handle)
    : http.createServer(handle)
  server.on('tlsClientError', () => {
    refusedHandshakes += 1
  })

  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const address = server.address()
  if (address === null || typeof address === 'string') {
    throw new Error('the stand-in provider has no port')
  }
  reachFromDevice(address.port)

  return {
    baseUrl: `${options?.selfSignedTls ? 'https' : 'http'}://127.0.0.1:${address.port}`,
    refusedHandshakes: () => refusedHandshakes,
    modelId: models[0]!.id,
    behave: (next) => {
      behavior = next
      scriptAt = 0
    },
    connections: () => connections,
    requests: () => requests,
    waitForOpen: (options = {}) => {
      const open = connections.find((c) => c.closedAt === undefined)
      if (open !== undefined) return Promise.resolve(open)
      return new Promise((resolve, reject) => {
        const timeoutMs = options.timeoutMs ?? 5000
        const timer = setTimeout(() => {
          openWaiter = undefined
          reject(
            new Error(`no provider connection opened within ${timeoutMs} ms`),
          )
        }, timeoutMs)
        openWaiter = (connection) => {
          clearTimeout(timer)
          resolve(connection)
        }
      })
    },
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections()
        server.close(() => resolve())
      }),
  }
}
