import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import {
  createFederatedAuthHeader,
  type FederatedAuthParams,
  type CreateFederatedAuthOptions,
} from '@haex-space/federation-sdk'
import { toBase64Url } from '~/utils/encoding'

export type { FederatedAuthParams }

/**
 * Creates a signed DID auth header for API requests.
 *
 * Server-side contract:
 * - Tokens past `exp` MUST be rejected (clock skew tolerance: ±30s recommended)
 * - Seen `jti` values MUST be tracked and rejected to prevent replay attacks
 * - `jti` tracking can use a TTL cache matching the token's max lifetime (exp + skew)
 */
export async function createDidAuthHeader(
  privateKeyBase64: string,
  did: string,
  action: string,
  body?: string,
): Promise<string> {
  const bodyHash = toBase64Url(
    new Uint8Array(
      await crypto.subtle.digest('SHA-256', new TextEncoder().encode(body ?? '')),
    ),
  )

  const payload = JSON.stringify({
    did,
    action,
    timestamp: Date.now(),
    exp: Date.now() + 60_000,
    jti: crypto.randomUUID(),
    bodyHash,
  })

  const payloadBytes = new TextEncoder().encode(payload)
  const base64urlPayload = toBase64Url(payloadBytes)

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const signatureBuffer = await crypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(base64urlPayload),
  )
  const base64urlSignature = toBase64Url(new Uint8Array(signatureBuffer))

  return `DID ${base64urlPayload}.${base64urlSignature}`
}

export async function createDidAuthToken(
  privateKeyBase64: string,
  did: string,
): Promise<string> {
  const header = await createDidAuthHeader(privateKeyBase64, did, 'ws-connect')
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

export async function createFederatedDidAuthHeader(
  options: CreateFederatedAuthOptions,
): Promise<string> {
  return createFederatedAuthHeader(options)
}

export interface FetchWithFederatedDidAuthOptions {
  url: string
  privateKeyBase64: string
  did: string
  action: string
  federation: FederatedAuthParams
  options?: RequestInit
}

export async function fetchWithFederatedDidAuth(
  options: FetchWithFederatedDidAuthOptions,
): Promise<Response> {
  const { url, privateKeyBase64, did, action, federation, options: fetchOptions } = options
  const body = typeof fetchOptions?.body === 'string' ? fetchOptions.body : undefined
  const queryString = new URL(url).search.slice(1)

  const header = await createFederatedAuthHeader({
    did,
    privateKeyBase64,
    action,
    federation,
    body,
    queryString,
  })

  return fetch(url, {
    ...fetchOptions,
    headers: {
      ...fetchOptions?.headers,
      Authorization: header,
    },
  })
}
