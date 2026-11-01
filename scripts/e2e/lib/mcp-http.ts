// An MCP client on holzi's HTTP way for the scenarios (spec 021 T097, contracts/http-auth.md): it
// registers itself, opens the authorization page as a browser would, waits while the person
// answers the registration dialog in holzi, exchanges the code with PKCE and speaks streamable HTTP.
// Plain `fetch`, no `Origin` header, the way a native client does.
import { createHash, randomBytes } from 'node:crypto'

/** The redirect URI the client registers; holzi only redirects there, nobody listens. */
const REDIRECT = 'http://127.0.0.1:1/callback'

export interface HttpMcpClient {
  /** A JSON-RPC request; resolves with its `result`, throws on an `error`. */
  request<T>(method: string, params?: unknown): Promise<T>
  /** The agent's credential, to check that it rotates. */
  refreshToken: string
}

const base64url = (bytes: Buffer) => bytes.toString('base64url')

/** The JSON-RPC message with `id` in a JSON or event-stream answer. */
function messageOf(contentType: string, body: string, id: number): unknown {
  if (contentType.includes('application/json')) return JSON.parse(body)
  for (const line of body.split('\n')) {
    if (!line.startsWith('data:')) continue
    const data = line.slice('data:'.length).trim()
    if (data === '') continue
    const message = JSON.parse(data) as { id?: number }
    if (message.id === id) return message
  }
  throw new Error(`no answer to request ${id} in: ${body.slice(0, 500)}`)
}

/**
 * Registers `clientName` on `base` (e.g. `http://127.0.0.1:47011`) and signs in; `confirm` answers
 * the registration dialog in holzi once the page waits for it.
 */
export async function connectOverHttp(
  base: string,
  clientName: string,
  confirm: () => Promise<void>,
): Promise<HttpMcpClient> {
  const unauthorized = await fetch(`${base}/mcp`, { method: 'POST' })
  if (unauthorized.status !== 401)
    throw new Error(`/mcp without a token answered ${unauthorized.status}`)

  const registered = (await (
    await fetch(`${base}/oauth/register`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        client_name: clientName,
        redirect_uris: [REDIRECT],
      }),
    })
  ).json()) as { client_id: string }
  const verifier = base64url(randomBytes(32))
  const challenge = base64url(createHash('sha256').update(verifier).digest())
  const authorize = new URL(`${base}/oauth/authorize`)
  for (const [name, value] of Object.entries({
    response_type: 'code',
    client_id: registered.client_id,
    redirect_uri: REDIRECT,
    code_challenge: challenge,
    code_challenge_method: 'S256',
    state: 'e2e',
  }))
    authorize.searchParams.set(name, value)
  const page = await fetch(authorize, { redirect: 'manual' })
  const html = await page.text()
  const wait = /url=([^"]+)"/.exec(html)?.[1]
  if (page.status !== 200 || wait === undefined)
    throw new Error(
      `the authorization page: ${page.status} ${html.slice(0, 300)}`,
    )

  await confirm()
  let location: string | null = null
  for (let attempt = 0; attempt < 200 && location === null; attempt++) {
    const waited = await fetch(new URL(wait, base), { redirect: 'manual' })
    if (waited.status === 302) location = waited.headers.get('location')
    else await new Promise((resolve) => setTimeout(resolve, 100))
  }
  if (location === null) throw new Error('holzi never redirected')
  const code = new URL(location).searchParams.get('code')
  if (code === null)
    throw new Error(`the authorization was refused: ${location}`)

  const tokens = (await (
    await fetch(`${base}/oauth/token`, {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code,
        client_id: registered.client_id,
        redirect_uri: REDIRECT,
        code_verifier: verifier,
      }),
    })
  ).json()) as { access_token: string; refresh_token: string }

  let session: string | null = null
  let next = 1
  const post = async (message: object) => {
    const headers: Record<string, string> = {
      authorization: `Bearer ${tokens.access_token}`,
      'content-type': 'application/json',
      accept: 'application/json, text/event-stream',
    }
    if (session !== null) headers['mcp-session-id'] = session
    const response = await fetch(`${base}/mcp`, {
      method: 'POST',
      headers,
      body: JSON.stringify(message),
    })
    session = response.headers.get('mcp-session-id') ?? session
    return response
  }
  const request = async <T>(method: string, params?: unknown): Promise<T> => {
    const id = next++
    const response = await post({ jsonrpc: '2.0', id, method, params })
    const body = await response.text()
    if (!response.ok) throw new Error(`${method}: ${response.status} ${body}`)
    const message = messageOf(
      response.headers.get('content-type') ?? '',
      body,
      id,
    ) as { result?: T; error?: unknown }
    if (message.error !== undefined)
      throw new Error(`${method}: ${JSON.stringify(message.error)}`)
    return message.result as T
  }

  await request('initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: { name: clientName, version: '1' },
  })
  const initialized = await post({
    jsonrpc: '2.0',
    method: 'notifications/initialized',
  })
  if (!initialized.ok)
    throw new Error(`notifications/initialized: ${initialized.status}`)
  return { request, refreshToken: tokens.refresh_token }
}
