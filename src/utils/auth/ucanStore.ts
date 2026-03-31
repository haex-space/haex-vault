import {
  createUcan,
  createWebCryptoSigner,
  spaceResource,
  serverResource,
  decodeUcan,
  type Capability,
} from '@haex-space/ucan'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { eq, gt } from 'drizzle-orm'
import { haexUcanTokens } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'

const DEFAULT_EXPIRY_SECONDS = 30 * 24 * 60 * 60 // 30 days

// In-memory cache: spaceId -> encoded UCAN token
const ucanCache = new Map<string, string>()

/**
 * Create a self-signed root UCAN where issuer === audience (admin of own space).
 */
export async function createRootUcanAsync(
  did: string,
  privateKeyBase64: string,
  spaceId: string,
  expiresInSeconds: number = DEFAULT_EXPIRY_SECONDS,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const now = Math.floor(Date.now() / 1000)
  const token = await createUcan(
    {
      issuer: did,
      audience: did,
      capabilities: { [spaceResource(spaceId)]: 'space/admin' },
      expiration: now + expiresInSeconds,
    },
    sign,
  )

  cacheUcan(spaceId, token)
  return token
}

/**
 * Create a delegated UCAN with the parent as proof.
 * Used when inviting members to a space.
 */
export async function delegateUcanAsync(
  issuerDid: string,
  privateKeyBase64: string,
  audienceDid: string,
  spaceId: string,
  capability: Capability,
  parentUcan: string,
  expiresInSeconds: number = DEFAULT_EXPIRY_SECONDS,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const now = Math.floor(Date.now() / 1000)
  const token = await createUcan(
    {
      issuer: issuerDid,
      audience: audienceDid,
      capabilities: { [spaceResource(spaceId)]: capability },
      proofs: [parentUcan],
      expiration: now + expiresInSeconds,
    },
    sign,
  )

  return token
}

/**
 * Create a server/relay UCAN delegating relay capability to a server DID.
 * Used to authorize the relay server to act on behalf of this user for federation.
 */
export async function createServerRelayUcanAsync(
  issuerDid: string,
  privateKeyBase64: string,
  serverDid: string,
  parentUcan: string,
  expiresInSeconds: number = DEFAULT_EXPIRY_SECONDS,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const now = Math.floor(Date.now() / 1000)
  const token = await createUcan(
    {
      issuer: issuerDid,
      audience: serverDid,
      capabilities: { [serverResource(serverDid)]: 'server/relay' },
      proofs: [parentUcan],
      expiration: now + expiresInSeconds,
    },
    sign,
  )

  return token
}

/**
 * Get a cached UCAN for a space. Returns null if not found or expired.
 */
export function getUcanForSpaceAsync(spaceId: string): string | null {
  const token = ucanCache.get(spaceId)
  if (!token) return null

  // Check expiry
  try {
    const decoded = decodeUcan(token)
    const now = Math.floor(Date.now() / 1000)
    if (decoded.payload.exp <= now) {
      ucanCache.delete(spaceId)
      return null
    }
  } catch {
    ucanCache.delete(spaceId)
    return null
  }

  return token
}

/**
 * Fetch wrapper that adds UCAN authorization header.
 */
export async function fetchWithUcanAuth(
  url: string,
  spaceId: string,
  ucanToken: string,
  options?: RequestInit,
): Promise<Response> {
  return fetch(url, {
    ...options,
    headers: {
      ...options?.headers,
      Authorization: `UCAN ${ucanToken}`,
    },
  })
}

/**
 * Cache a UCAN token for a space.
 */
export function cacheUcan(spaceId: string, token: string): void {
  ucanCache.set(spaceId, token)
}

/**
 * Clear cached UCAN tokens. If spaceId is provided, only that entry is removed.
 */
export function clearUcanCache(spaceId?: string): void {
  if (spaceId) {
    ucanCache.delete(spaceId)
  } else {
    ucanCache.clear()
  }
}

/**
 * Persist a UCAN token to the database (upsert by spaceId).
 * Also caches the token in memory.
 */
export async function persistUcanAsync(
  db: SqliteRemoteDatabase,
  spaceId: string,
  token: string,
): Promise<void> {
  const decoded = decodeUcan(token)
  const { iss, aud, exp, iat } = decoded.payload

  // Extract capability from the token's capabilities map
  const caps = decoded.payload.cap as Record<string, string>
  const capability = Object.values(caps)[0] ?? 'space/admin'

  // Delete existing token for this space, then insert new one
  await db.delete(haexUcanTokens).where(eq(haexUcanTokens.spaceId, spaceId))
  await db.insert(haexUcanTokens).values({
    id: crypto.randomUUID(),
    spaceId,
    token,
    capability,
    issuerDid: iss,
    audienceDid: aud,
    issuedAt: iat ?? Math.floor(Date.now() / 1000),
    expiresAt: exp,
  })

  cacheUcan(spaceId, token)
}

/**
 * Load all non-expired UCAN tokens from DB into the in-memory cache.
 * Call this on vault open to warm the cache.
 */
export async function loadUcansFromDbAsync(db: SqliteRemoteDatabase): Promise<void> {
  const now = Math.floor(Date.now() / 1000)
  const rows = await db
    .select({ spaceId: haexUcanTokens.spaceId, token: haexUcanTokens.token })
    .from(haexUcanTokens)
    .where(gt(haexUcanTokens.expiresAt, now))

  for (const row of rows) {
    ucanCache.set(row.spaceId, row.token)
  }
}
