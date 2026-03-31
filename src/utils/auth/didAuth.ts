import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'

function base64urlEncode(data: Uint8Array): string {
  const base64 = btoa(String.fromCharCode(...data))
  return base64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

function hexEncode(bytes: Uint8Array): string {
  return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
}

export async function createDidAuthHeader(
  privateKeyBase64: string,
  did: string,
  action: string,
  body?: string,
): Promise<string> {
  const bodyHash = base64urlEncode(
    new Uint8Array(
      await crypto.subtle.digest('SHA-256', new TextEncoder().encode(body ?? '')),
    ),
  )

  const payload = JSON.stringify({
    did,
    action,
    timestamp: Date.now(),
    bodyHash,
  })

  const payloadBytes = new TextEncoder().encode(payload)
  const base64urlPayload = base64urlEncode(payloadBytes)

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const signatureBuffer = await crypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(base64urlPayload),
  )
  const base64urlSignature = base64urlEncode(new Uint8Array(signatureBuffer))

  return `DID ${base64urlPayload}.${base64urlSignature}`
}

export async function createDidAuthToken(
  privateKeyBase64: string,
  did: string,
): Promise<string> {
  const header = await createDidAuthHeader(privateKeyBase64, did, 'ws-connect')
  // Strip the "DID " prefix, return just the token
  return header.slice(4)
}

export async function fetchWithDidAuth(
  url: string,
  privateKeyBase64: string,
  did: string,
  action: string,
  options?: RequestInit,
): Promise<Response> {
  const body = typeof options?.body === 'string' ? options.body : undefined
  const header = await createDidAuthHeader(privateKeyBase64, did, action, body)

  return fetch(url, {
    ...options,
    headers: {
      ...options?.headers,
      Authorization: header,
    },
  })
}

export interface FederatedAuthParams {
  spaceId: string
  serverDid: string
  relayDid: string
}

export async function createFederatedDidAuthHeader(
  privateKeyBase64: string,
  did: string,
  action: string,
  federation: FederatedAuthParams,
  body?: string,
  queryString?: string,
): Promise<string> {
  const params = new URLSearchParams(queryString ?? '')
  const sorted = new URLSearchParams([...params.entries()].sort())
  const hashInput = (body ?? '') + '?' + sorted.toString()
  const requestHash = hexEncode(
    new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(hashInput))),
  )

  const now = Date.now()
  const payload = JSON.stringify({
    did,
    action,
    timestamp: now,
    expiresAt: now + 10_000,
    requestHash,
    spaceId: federation.spaceId,
    serverDid: federation.serverDid,
    relayDid: federation.relayDid,
  })

  const payloadBytes = new TextEncoder().encode(payload)
  const base64urlPayload = base64urlEncode(payloadBytes)

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const signatureBuffer = await crypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(base64urlPayload),
  )

  return `DID ${base64urlPayload}.${base64urlEncode(new Uint8Array(signatureBuffer))}`
}

export async function fetchWithFederatedDidAuth(
  url: string,
  privateKeyBase64: string,
  did: string,
  action: string,
  federation: FederatedAuthParams,
  options?: RequestInit,
): Promise<Response> {
  const body = typeof options?.body === 'string' ? options.body : undefined
  const queryString = new URL(url).search.slice(1)
  const header = await createFederatedDidAuthHeader(
    privateKeyBase64, did, action, federation, body, queryString,
  )

  return fetch(url, {
    ...options,
    headers: {
      ...options?.headers,
      Authorization: header,
    },
  })
}
