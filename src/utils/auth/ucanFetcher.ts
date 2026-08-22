import { fetch as tauriFetch } from '@tauri-apps/plugin-http'
import {
  decodeUcan,
  createUcanPopHeader,
  POP_HEADER_NAME,
} from '@haex-space/ucan'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { useIdentityStore } from '@/stores/identity'

/**
 * Vault-local glue for UCAN-authed HTTP calls.
 *
 * The SDK's `fetchWithUcanPop` cannot ship the vault-only pieces — how to
 * resolve a DID to its unlocked WebCrypto private key, and how to hook cache
 * invalidation into the vault-lock lifecycle. Both live here.
 *
 * The audience DID is decoded from the UCAN token, so feature code never
 * threads identity state through UCAN call sites: it hands over the token,
 * the resolver picks the matching key.
 *
 * Bodies are string-only (SDK-level contract): UCAN-authed routes carry
 * structured DB rows, and mixing binary bodies would make client/server
 * request-hash agreement unreliable.
 */

/**
 * Module-scoped cache. Once a DID's key is imported for the current unlocked
 * session it stays resident — the imported `CryptoKey` is not the raw bytes,
 * so it cannot be re-derived without another decrypt+import cycle.
 */
const identityKeyCache = new Map<string, CryptoKey>()

/**
 * Resolve `did` to its unlocked WebCrypto Ed25519 CryptoKey via the identity
 * store. Cached per DID for the vault-open session.
 *
 * Throws if the DID is not owned by this vault. Callers reaching this path
 * with a foreign DID have already made a logic mistake upstream (holding a
 * UCAN whose `aud` this vault cannot sign for), so a hard error is preferred
 * over a silent 401 at the server.
 */
export async function resolvePrivateKey(did: string): Promise<CryptoKey> {
  const cached = identityKeyCache.get(did)
  if (cached) return cached

  const identityStore = useIdentityStore()
  const identity = identityStore.identities.find(i => i.did === did && i.privateKey !== null)
  if (!identity) {
    throw new Error(`resolvePrivateKey: no unlocked own-identity for ${did}`)
  }
  const key = await importUserPrivateKeyAsync(identity.privateKey!)
  identityKeyCache.set(did, key)
  return key
}

/**
 * Drop every cached CryptoKey. Called by `useIdentityStore.reset()` on
 * vault-lock so a subsequent unlock re-imports fresh keys.
 */
export function clearIdentityKeyCache(): void {
  identityKeyCache.clear()
}

/**
 * Invalidate one DID's cached CryptoKey. Called when that identity is
 * deleted so a re-created identity with the same DID would not inherit a
 * stale imported key.
 */
export function invalidateIdentityKey(did: string): void {
  identityKeyCache.delete(did)
}

/** Signature for the bound fetcher returned by `createVaultUcanFetcher`. */
export type VaultUcanFetcher = (
  url: string,
  ucanToken: string,
  options?: RequestInit,
) => Promise<Response>

/**
 * Build a `fetch`-like function that attaches:
 *  - `Authorization: UCAN <token>`
 *  - `X-UCAN-PoP: <signed-payload>` signed by the UCAN's audience's private key
 *
 * URL/method/query/body are covered by `requestHash` so hand-crafted `fetch`
 * calls to UCAN-authed routes will 401 — call sites route through this
 * fetcher (or a wrapper around it) so PoP is inseparable from the request.
 *
 * The returned function uses Tauri's HTTP plugin under the hood (matches the
 * existing vault pattern in `ucanStore.ts` and `changes.ts`); the plugin's
 * fetch adheres to the standard `RequestInit` shape.
 */
export function createVaultUcanFetcher(): VaultUcanFetcher {
  return async function ucanFetcher(url, ucanToken, options = {}) {
    const aud = extractUcanAudience(ucanToken)
    const privateKey = await resolvePrivateKey(aud)

    const method = (options.method ?? 'GET').toUpperCase()
    const body = normaliseBody(options.body)

    const parsed = new URL(url, globalThis.location?.href)
    const path = parsed.pathname
    const rawQuery = parsed.search.startsWith('?')
      ? parsed.search.slice(1)
      : parsed.search

    const popHeader = await createUcanPopHeader({
      privateKey,
      ucanAud: aud,
      method,
      path,
      rawQuery,
      body,
    })

    const headers = new Headers(options.headers)
    headers.set('Authorization', `UCAN ${ucanToken}`)
    headers.set(POP_HEADER_NAME, popHeader)
    if (body.length > 0 && !headers.has('Content-Type')) {
      headers.set('Content-Type', 'application/json')
    }

    return tauriFetch(url, {
      ...options,
      method,
      headers,
      body: body.length > 0 ? body : undefined,
    })
  }
}

function extractUcanAudience(token: string): string {
  const decoded = decodeUcan(token)
  if (typeof decoded.payload.aud !== 'string' || decoded.payload.aud.length === 0) {
    throw new Error('createVaultUcanFetcher: UCAN token has no aud')
  }
  return decoded.payload.aud
}

function normaliseBody(body: BodyInit | null | undefined): string {
  if (body == null) return ''
  if (typeof body === 'string') return body
  throw new TypeError(
    'createVaultUcanFetcher only accepts string bodies. UCAN-authed routes carry '
    + 'structured DB rows; other body types would make client/server request-hash '
    + 'agreement unreliable.',
  )
}
