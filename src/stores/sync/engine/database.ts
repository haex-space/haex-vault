/**
 * Local Database Operations
 * Handles sync-related database operations on the haex_sync_backends table
 */

import { eq } from 'drizzle-orm'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import { haexSyncBackends } from '~/database/schemas'
import { schema } from '~/database'

type DrizzleDatabase = SqliteRemoteDatabase<typeof schema>

/**
 * Gets sync key for a backend from haex_sync_backends table
 * Returns null if not found
 */
export const getSyncKeyFromDbAsync = async (
  drizzle: DrizzleDatabase,
  backendId: string,
): Promise<Uint8Array | null> => {
  const results = await drizzle
    .select()
    .from(haexSyncBackends)
    .where(eq(haexSyncBackends.id, backendId))
    .limit(1)

  if (!results[0]?.syncKey) {
    return null
  }

  // Stored as Base64, convert back to Uint8Array
  return Uint8Array.from(atob(results[0].syncKey), (c) => c.charCodeAt(0))
}

/**
 * Saves sync key for a backend to haex_sync_backends table
 */
export const saveSyncKeyToDbAsync = async (
  drizzle: DrizzleDatabase,
  backendId: string,
  syncKey: Uint8Array,
): Promise<void> => {
  // Convert Uint8Array to Base64
  const base64 = btoa(String.fromCharCode(...syncKey))

  // Update the backend with the sync key
  await drizzle
    .update(haexSyncBackends)
    .set({ syncKey: base64 })
    .where(eq(haexSyncBackends.id, backendId))
}

/**
 * Saves vault key salt for a backend to local vault's haex_sync_backends table
 * Salt is used for PBKDF2 key derivation from vault password
 */
export const saveVaultKeySaltAsync = async (
  drizzle: DrizzleDatabase,
  backendId: string,
  vaultKeySalt: string,
): Promise<void> => {
  await drizzle
    .update(haexSyncBackends)
    .set({ vaultKeySalt })
    .where(eq(haexSyncBackends.id, backendId))
}

/**
 * Gets vault key salt for a backend from local vault's haex_sync_backends table
 */
export const getVaultKeySaltAsync = async (
  drizzle: DrizzleDatabase,
  backendId: string,
): Promise<string | null> => {
  const result = await drizzle.query.haexSyncBackends.findFirst({
    where: eq(haexSyncBackends.id, backendId),
  })

  return result?.vaultKeySalt ?? null
}

/**
 * Marks a backend as having a pending vault key update.
 * Used when a backend is unreachable during password change.
 */
export const markBackendPendingVaultKeyUpdateAsync = async (
  drizzle: DrizzleDatabase,
  backendId: string,
  pending: boolean,
): Promise<void> => {
  await drizzle
    .update(haexSyncBackends)
    .set({ pendingVaultKeyUpdate: pending })
    .where(eq(haexSyncBackends.id, backendId))
}

/**
 * Gets all backends that have pending vault key updates.
 */
export const getBackendsWithPendingVaultKeyUpdateAsync = async (
  drizzle: DrizzleDatabase,
): Promise<string[]> => {
  const results = await drizzle
    .select({ id: haexSyncBackends.id })
    .from(haexSyncBackends)
    .where(eq(haexSyncBackends.pendingVaultKeyUpdate, true))

  return results.map((r) => r.id)
}
