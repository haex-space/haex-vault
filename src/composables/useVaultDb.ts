import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'

export type VaultDb = SqliteRemoteDatabase<typeof schema>

/**
 * Shared accessor for the currently open vault's Drizzle instance.
 *
 * Use `getDb()` when the caller can gracefully handle a closed vault
 * (e.g. read-only lookups that return empty defaults). Use `requireDb()`
 * when a closed vault is an error condition (writes, mutations).
 */
export function useVaultDb() {
  const { currentVault } = storeToRefs(useVaultStore())

  const getDb = (): VaultDb | undefined => currentVault.value?.drizzle

  const requireDb = (): VaultDb => {
    const db = getDb()
    if (!db) throw new Error('No vault open')
    return db
  }

  return { getDb, requireDb }
}
