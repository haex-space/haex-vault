import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { createSignedAuthHeader } from '@haex-space/ucan'
import {
  createFederatedAuthHeader,
  type FederatedAuthParams,
  type CreateFederatedAuthOptions,
} from '@haex-space/federation-sdk'
import { fetch } from '@tauri-apps/plugin-http'

export type { FederatedAuthParams }

/**
 * Creates a signed DID auth header for API requests.
 *
 * Server-side contract:
 * - Tokens past `exp` MUST be rejected (clock skew tolerance: ±30s recommended)
 * - Seen `jti` values MUST be tracked and rejected to prevent replay attacks
 * - `jti` tracking can use a TTL cache matching the token's max lifetime (exp + skew)
 */
export interface DidAuthRequest {
  method: string
  url: string
  body?: string
}

function requestTarget(url: string): { path: string; rawQuery: string } {
  const parsed = new URL(url, 'https://did-auth.invalid')
  return { path: parsed.pathname, rawQuery: parsed.search.slice(1) }
}

function signedRequestBody(body: RequestInit['body']): string {
  if (body == null) return ''
  if (typeof body !== 'string') {
    throw new TypeError('DID-authenticated requests require string bodies')
  }
  return body
}

export async function createDidAuthHeader(
  privateKeyBase64: string,
  did: string,
  request: DidAuthRequest,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const { path, rawQuery } = requestTarget(request.url)
  const headerValue = await createSignedAuthHeader({
    privateKey,
    did,
    method: request.method,
    path,
    rawQuery,
    body: request.body ?? '',
  })

  return `DID ${headerValue}`
}

export async function createDidAuthToken(
  privateKeyBase64: string,
  did: string,
): Promise<string> {
  const header = await createDidAuthHeader(
    privateKeyBase64,
    did,
    {
      method: 'GET',
      url: '/ws',
    },
  )
  return header.slice(4)
}

export async function fetchWithDidAuth(
  url: string,
  privateKeyBase64: string,
  did: string,
  options?: RequestInit,
): Promise<Response> {
  const body = signedRequestBody(options?.body)
  const header = await createDidAuthHeader(privateKeyBase64, did, {
    method: options?.method ?? 'GET',
    url,
    body,
  })

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
  federation: FederatedAuthParams
  options?: RequestInit
}

export async function fetchWithFederatedDidAuth(
  options: FetchWithFederatedDidAuthOptions,
): Promise<Response> {
  const {
    url,
    privateKeyBase64,
    did,
    federation,
    options: fetchOptions,
  } = options
  const body = signedRequestBody(fetchOptions?.body)
  const { path, rawQuery } = requestTarget(url)

  const header = await createFederatedAuthHeader({
    did,
    privateKeyBase64,
    federation,
    method: fetchOptions?.method ?? 'GET',
    path,
    rawQuery,
    body,
  })

  return fetch(url, {
    ...fetchOptions,
    headers: {
      ...fetchOptions?.headers,
      Authorization: header,
    },
  })
}
