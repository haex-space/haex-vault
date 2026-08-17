import {
  createUcan,
  createWebCryptoSigner,
  spaceResource,
  decodeUcan,
  spaceCapabilitySet,
  isSpaceCapValue,
  ServerCapabilities,
  type SpaceCap,
  type SpaceCapabilitySet,
} from '@haex-space/ucan'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { fetch } from '@tauri-apps/plugin-http'
import { eq, gt } from 'drizzle-orm'
import { haexUcanTokens } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'

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
      // Owner root UCAN grants the full orthogonal capability set. Under
      // the orthogonal model (post W4 PR-3), holding `admin` does NOT
      // imply the other caps — the owner must hold each cap explicitly to
      // sign changes at its level. All entries are `delegatable: true` so
      // the owner can hand out any subset to invitees.
      capabilities: {
        [spaceResource(spaceId)]: spaceCapabilitySet()
          .read(true)
          .write(true)
          .invite(true)
          .admin(true)
          .build(),
      },
      expiration: expiresAtUnixSeconds,
    },
    sign,
  )

  cacheUcan(spaceId, token)
  return token
}

/**
 * Build the set for a single requested member capability. Read is the
 * explicit baseline for a peer session (Announce), and is terminal just like
 * Write; Invite/Admin remain delegatable. Explicit sets can still be passed
 * directly.
 */
const capsFromSingle = (cap: SpaceCap): SpaceCapabilitySet => {
  const builder = spaceCapabilitySet().read(false)
  switch (cap) {
    case 'read': return builder.build()
    case 'write': return builder.write(false).build()
    case 'invite': return builder.invite(true).build()
    case 'admin': return builder.admin(true).build()
  }
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
  capabilities: SpaceCap | SpaceCapabilitySet,
  parentUcan: string,
  expiresAtUnixSeconds: number = NEVER_EXPIRES_UNIX_SECONDS,
): Promise<string> {
  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sign = createWebCryptoSigner(privateKey)

  const capSet = typeof capabilities === 'string'
    ? capsFromSingle(capabilities)
    : capabilities

  const token = await createUcan(
    {
      issuer: issuerDid,
      audience: audienceDid,
      capabilities: { [spaceResource(spaceId)]: capSet },
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
      // ServerCapability stays a bare string on the wire (Rust side keeps
      // the `server/relay` shape); only space:* capabilities became sets
      // in the W4 PR-3 wire migration.
      capabilities: { [spaceResource(spaceId)]: ServerCapabilities.RELAY },
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
  db: SqliteRemoteDatabase<Record<string, unknown>>,
  spaceId: string,
  token: string,
): Promise<void> {
  const decoded = decodeUcan(token)
  const { iss, aud, exp, iat } = decoded.payload

  // Task 8b: `haex_ucan_tokens.capabilities` (renamed from `capability`)
  // stores a serialized `SpaceCapabilitySet` — a JSON array of
  // `{cap, delegatable}` entries — mirroring the wire form Tasks 2/4 landed
  // on for the UCAN payload's `cap` map value. One row per delegation
  // (not per cap): the row's `capabilities` matches exactly the set the
  // UCAN itself carries, so a downstream `holdsSpaceCap` check on the
  // parsed set answers the same question as a `holdsSpaceCap` on the
  // decoded token.
  //
  // A server-only token (or a token for another space) is not evidence of
  // any space capability. Never fabricate a fallback set here: that would
  // turn malformed input into an admin cache entry.
  const capsMap = decoded.payload.cap
  const spaceValue = capsMap[spaceResource(spaceId)]
  if (!isSpaceCapValue(spaceValue)) {
    throw new Error(`UCAN does not grant a SpaceCapabilitySet for space ${spaceId}`)
  }
  const capabilitySet: SpaceCapabilitySet = spaceValue

  const issuedAt = iat ?? Math.floor(Date.now() / 1000)

  // Delete existing tokens for this space, then insert ONE row per token
  // carrying the full serialized set — no more one-row-per-cap fan-out.
  await db.delete(haexUcanTokens).where(eq(haexUcanTokens.spaceId, spaceId))
  await db.insert(haexUcanTokens).values({
    id: crypto.randomUUID(),
    spaceId,
    token,
    capabilities: JSON.stringify(capabilitySet),
    issuerDid: iss,
    audienceDid: aud,
    issuedAt,
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
  const rows = await db
    .select({ spaceId: haexUcanTokens.spaceId, token: haexUcanTokens.token })
    .from(haexUcanTokens)
    .where(gt(haexUcanTokens.expiresAt, now))

  for (const row of rows) {
    ucanCache.set(row.spaceId, row.token)
  }
}
