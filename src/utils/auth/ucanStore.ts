import {
  createUcan,
  createWebCryptoSigner,
  spaceResource,
  decodeUcan,
  type Capability,
} from '@haex-space/ucan'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { eq, gt } from 'drizzle-orm'
import { haexUcanTokens } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import { createLogger } from '@/stores/logging'

const log = createLogger('UCAN_STORE')

// UCAN tokens are effectively permanent — revocation is driven by the
// active-membership check on the server side (see is_active_space_member in
// Rust). A member removed from `haex_space_members` (which is what happens
// when an admin kicks them out + MLS commit) loses sync access immediately
// regardless of `exp`. The `exp` field stays present for UCAN-standard
// conformance and as a failsafe if the membership check is ever bypassed;
// we pick the year 9999 sentinel (2^63-1 seconds would overflow some JS
// consumers) which is indistinguishable from "never" in practice.
const NEVER_EXPIRES_UNIX_SECONDS = 253_402_300_799 // 9999-12-31T23:59:59Z

// In-memory cache: spaceId -> encoded UCAN token
const ucanCache = new Map<string, string>()

/**
 * Create a self-signed root UCAN where issuer === audience (admin of own space).
 *
 * @param expiresAtUnixSeconds Absolute Unix timestamp in seconds (NOT a duration).
 *   Defaults to `NEVER_EXPIRES_UNIX_SECONDS` — see top-of-file note on why UCAN
 *   expiry is effectively disabled in favour of membership-driven revocation.
 */
export async function createRootUcanAsync(
  did: string,
  privateKeyBase64: string,
  spaceId: string,
  expiresAtUnixSeconds: number = NEVER_EXPIRES_UNIX_SECONDS,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const token = await createUcan(
    {
      issuer: did,
      audience: did,
      capabilities: { [spaceResource(spaceId)]: 'space/admin' },
      expiration: expiresAtUnixSeconds,
    },
    sign,
  )

  cacheUcan(spaceId, token)
  return token
}

/**
 * Create a delegated UCAN with the parent as proof.
 * Used when inviting members to a space.
 *
 * @param expiresAtUnixSeconds Absolute Unix timestamp in seconds (NOT a duration).
 *   Defaults to `NEVER_EXPIRES_UNIX_SECONDS`.
 */
export async function delegateUcanAsync(
  issuerDid: string,
  privateKeyBase64: string,
  audienceDid: string,
  spaceId: string,
  capability: Capability,
  parentUcan: string,
  expiresAtUnixSeconds: number = NEVER_EXPIRES_UNIX_SECONDS,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const token = await createUcan(
    {
      issuer: issuerDid,
      audience: audienceDid,
      capabilities: { [spaceResource(spaceId)]: capability },
      proofs: [parentUcan],
      expiration: expiresAtUnixSeconds,
    },
    sign,
  )

  return token
}

/**
 * Create a server/relay UCAN delegating relay capability for a specific space to a server.
 * Resource is space:<spaceId> so relay access is scoped per space, not per server.
 *
 * @param expiresAtUnixSeconds Absolute Unix timestamp in seconds (NOT a duration).
 *   Defaults to `NEVER_EXPIRES_UNIX_SECONDS`.
 */
export async function createServerRelayUcanAsync(
  issuerDid: string,
  privateKeyBase64: string,
  serverDid: string,
  spaceId: string,
  parentUcan: string,
  expiresAtUnixSeconds: number = NEVER_EXPIRES_UNIX_SECONDS,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const token = await createUcan(
    {
      issuer: issuerDid,
      audience: serverDid,
      capabilities: { [spaceResource(spaceId)]: 'server/relay' },
      proofs: [parentUcan],
      expiration: expiresAtUnixSeconds,
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
  if (!token) {
    log.warn(`UCAN-DIAG getUcanForSpaceAsync outcome=cache-miss spaceId=${spaceId.slice(0, 8)} cacheSize=${ucanCache.size}`)
    return null
  }

  // Check expiry
  try {
    const decoded = decodeUcan(token)
    const now = Math.floor(Date.now() / 1000)
    if (decoded.payload.exp <= now) {
      ucanCache.delete(spaceId)
      log.warn(`UCAN-DIAG getUcanForSpaceAsync outcome=expired spaceId=${spaceId.slice(0, 8)} exp=${decoded.payload.exp} now=${now}`)
      return null
    }
  } catch (err) {
    ucanCache.delete(spaceId)
    log.warn(`UCAN-DIAG getUcanForSpaceAsync outcome=decode-error spaceId=${spaceId.slice(0, 8)} err=${(err as Error).message?.slice(0, 80)}`)
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
  log.warn(`UCAN-DIAG cacheUcan spaceId=${spaceId.slice(0, 8)} cacheSize=${ucanCache.size}`)
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
  db: SqliteRemoteDatabase<Record<string, unknown>>,
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
export async function loadUcansFromDbAsync(db: SqliteRemoteDatabase<Record<string, unknown>>): Promise<void> {
  const now = Math.floor(Date.now() / 1000)
  const sizeBefore = ucanCache.size
  const rows = await db
    .select({ spaceId: haexUcanTokens.spaceId, token: haexUcanTokens.token })
    .from(haexUcanTokens)
    .where(gt(haexUcanTokens.expiresAt, now))

  for (const row of rows) {
    ucanCache.set(row.spaceId, row.token)
  }
  log.warn(`UCAN-DIAG loadUcansFromDbAsync dbRows=${rows.length} cacheBefore=${sizeBefore} cacheAfter=${ucanCache.size}`)
}

// Test-only hook: expose cache mutators to the page runtime so e2e specs
// can inject Race A / Race B scenarios (see variant-a-race.spec.ts in
// haex-e2e-tests). Lives only on debug/ucan-subpath-logging — drop before
// merging to main.
//
// Defense-in-depth gate: `import.meta.dev` is a build-time constant that
// Vite tree-shakes to `false` in production builds, so even if the hook
// code accidentally lands on main, the assignment is dead-eliminated. The
// e2e variant-a-race test currently builds the production binary, so it
// would need `pnpm tauri dev` mode or a VITE_-prefixed env override to
// re-arm — accept the trade-off here since the diagnostic value of the
// race test is already captured (16/16 GREEN on the diag branch).
if (import.meta.dev && typeof window !== 'undefined') {
  ;(window as Window & {
    __ucanCacheDebug?: {
      clear: (spaceId?: string) => void
      size: () => number
      keys: () => string[]
    }
  }).__ucanCacheDebug = {
    clear: (spaceId?: string) => clearUcanCache(spaceId),
    size: () => ucanCache.size,
    keys: () => Array.from(ucanCache.keys()),
  }
}
