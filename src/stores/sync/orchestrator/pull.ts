/**
 * Sync Pull Operations
 * Handles pulling remote changes from the sync server
 */

import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { decryptCrdtData } from '@haex-space/vault-sdk'
import type { ColumnChange } from '../tableScanner'
import { log, type BackendSyncState, type PullResult } from './types'

/**
 * Pulls changes from a specific backend using column-level HLC comparison
 * Downloads ALL changes first, then applies them atomically in a transaction
 */
export const pullFromBackendAsync = async (
  backendId: string,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<void> => {
  log.info(`========== PULL START (backend: ${backendId}) ==========`)

  if (!currentVaultId) {
    log.error('PULL FAILED: No vault opened')
    throw new Error('No vault opened')
  }

  const state = syncStates[backendId]
  if (!state) {
    log.error('PULL FAILED: Backend not initialized')
    throw new Error('Backend not initialized')
  }

  if (state.isSyncing) {
    log.debug(`PULL SKIPPED: Already syncing with backend ${backendId}`)
    return
  }

  state.isSyncing = true
  state.error = null

  try {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend?.vaultId) {
      log.error('PULL FAILED: Backend vaultId not configured')
      throw new Error('Backend vaultId not configured')
    }

    // Get vault key from cache
    const vaultKey = syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
    if (!vaultKey) {
      log.error('PULL FAILED: Vault key not available')
      throw new Error('Vault key not available. Please unlock vault first.')
    }

    const lastPullServerTimestamp = backend.lastPullServerTimestamp
    log.debug('Pull config:', {
      backendId,
      vaultId: backend.vaultId,
      lastPullServerTimestamp: lastPullServerTimestamp || '(none - full sync)',
    })

    // Step 1: Download ALL changes from server (with pagination)
    log.info('Downloading changes from server...')
    const pullResult = await pullChangesFromServerAsync(
      backendId,
      backend.vaultId,
      lastPullServerTimestamp,
      syncBackendsStore,
      syncEngineStore,
    )

    const { changes: allChanges, serverTimestamp } = pullResult

    if (allChanges.length === 0) {
      // Even with no changes, update the serverTimestamp to avoid re-checking
      if (serverTimestamp) {
        log.debug('No changes, but updating lastPullServerTimestamp to:', serverTimestamp)
        await syncBackendsStore.updateBackendAsync(backendId, {
          lastPullServerTimestamp: serverTimestamp,
        })
      }
      log.info('PULL COMPLETE: No new changes from server')
      return
    }

    log.info(`Downloaded ${allChanges.length} changes from server`)

    // Log unique tables affected
    const tablesAffected = [...new Set(allChanges.map((c) => c.tableName))]
    log.debug('Tables affected:', tablesAffected)

    // Step 2: Apply all changes with proper migration ordering
    await applyAllChangesWithMigrationsAsync(allChanges, vaultKey, backendId)

    // Step 3: Update lastPullServerTimestamp with the server timestamp
    if (serverTimestamp) {
      log.debug('Updating lastPullServerTimestamp to:', serverTimestamp)
      await syncBackendsStore.updateBackendAsync(backendId, {
        lastPullServerTimestamp: serverTimestamp,
      })
    }

    // Step 4: Reload backend data from database
    log.debug('Reloading backend config after pull...')
    await syncBackendsStore.loadBackendsAsync()

    // Step 5: Emit event to notify frontend about changed tables
    if (tablesAffected.length > 0) {
      log.debug('Emitting sync:tables-updated event for tables:', tablesAffected)
      await emit('sync:tables-updated', { tables: tablesAffected })
    }

    log.info(`========== PULL SUCCESS: ${allChanges.length} changes applied ==========`)
  } catch (error) {
    log.error(`========== PULL FAILED ==========`, error)
    state.error = error instanceof Error ? error.message : 'Unknown error'
    throw error
  } finally {
    state.isSyncing = false
  }
}

/**
 * Pulls column-level changes from server with pagination
 * Uses server timestamps (afterUpdatedAt) instead of HLC for cursor
 * Returns both the changes and the server timestamp for storing as cursor
 */
export const pullChangesFromServerAsync = async (
  backendId: string,
  vaultId: string,
  lastPullServerTimestamp: string | null | undefined,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<PullResult> => {
  log.debug('pullChangesFromServerAsync: Getting auth token...')
  const token = await syncEngineStore.getAuthTokenAsync()
  if (!token) {
    log.error('pullChangesFromServerAsync: Not authenticated')
    throw new Error('Not authenticated')
  }

  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend) {
    log.error('pullChangesFromServerAsync: Backend not found')
    throw new Error('Backend not found')
  }

  const allChanges: ColumnChange[] = []
  let hasMore = true
  let currentCursor: string | null = lastPullServerTimestamp || null
  let pageCount = 0
  let lastServerTimestamp: string | null = null

  // Pagination loop - download ALL changes before applying
  while (hasMore) {
    pageCount++
    // Use afterUpdatedAt (server timestamp) instead of since (HLC)
    const url = `${backend.serverUrl}/sync/pull?vaultId=${vaultId}&afterUpdatedAt=${currentCursor || ''}&limit=1000`
    log.debug(`Fetching page ${pageCount}:`, url)

    const response = await fetch(url, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
      },
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      log.error('Server returned error:', { status: response.status, error })
      throw new Error(`Failed to pull changes: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    const changes: ColumnChange[] = data.changes || []

    allChanges.push(...changes)

    // Check if there are more pages
    hasMore = data.hasMore === true

    // Use serverTimestamp from response as cursor for next page and final storage
    lastServerTimestamp = data.serverTimestamp || null
    currentCursor = lastServerTimestamp

    log.info(`Page ${pageCount}: ${changes.length} changes (total: ${allChanges.length}, hasMore: ${hasMore})`)
  }

  return { changes: allChanges, serverTimestamp: lastServerTimestamp }
}

/**
 * Pulls changes from server using explicit config (for initial pull with temporary backend)
 */
export const pullChangesFromServerWithConfigAsync = async (
  serverUrl: string,
  vaultId: string,
  lastPullServerTimestamp: string | null,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<PullResult> => {
  log.debug('pullChangesFromServerWithConfigAsync: Getting auth token...')
  const token = await syncEngineStore.getAuthTokenAsync()
  if (!token) {
    log.error('pullChangesFromServerWithConfigAsync: Not authenticated')
    throw new Error('Not authenticated')
  }

  const allChanges: ColumnChange[] = []
  let hasMore = true
  let currentCursor: string | null = lastPullServerTimestamp
  let pageCount = 0
  let lastServerTimestamp: string | null = null

  // Pagination loop - download ALL changes before applying
  while (hasMore) {
    pageCount++
    const url = `${serverUrl}/sync/pull?vaultId=${vaultId}&afterUpdatedAt=${currentCursor || ''}&limit=1000`
    log.debug(`Fetching page ${pageCount}:`, url)

    const response = await fetch(url, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
      },
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      log.error('Server returned error:', { status: response.status, error })
      throw new Error(`Failed to pull changes: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    const changes: ColumnChange[] = data.changes || []

    allChanges.push(...changes)

    // Check if there are more pages
    hasMore = data.hasMore === true

    // Use serverTimestamp from response as cursor for next page and final storage
    lastServerTimestamp = data.serverTimestamp || null
    currentCursor = lastServerTimestamp

    log.info(`Page ${pageCount}: ${changes.length} changes (total: ${allChanges.length}, hasMore: ${hasMore})`)
  }

  return { changes: allChanges, serverTimestamp: lastServerTimestamp }
}

/**
 * Applies all remote changes with proper ordering for extension tables
 *
 * This function ensures extension migrations are applied BEFORE extension table data.
 * Without this ordering, extension table data would be skipped because the tables
 * don't exist yet when the data is being applied.
 *
 * Order of operations:
 * 1. Apply haex_extensions changes (extension registrations - needed for FK)
 * 2. Apply haex_extension_migrations changes (migration definitions)
 * 3. Run apply_synced_extension_migrations to create extension tables
 * 4. Apply remaining changes (including extension table data)
 *
 * @returns The maximum HLC timestamp from all applied changes
 */
export const applyAllChangesWithMigrationsAsync = async (
  allChanges: ColumnChange[],
  vaultKey: Uint8Array,
  backendId: string,
): Promise<string> => {
  if (allChanges.length === 0) {
    log.info('No changes to apply')
    return ''
  }

  log.info(`Processing ${allChanges.length} changes...`)

  // Separate changes into categories with correct application order:
  // 1. haex_extensions (extension registrations - needed for FK in migrations)
  // 2. haex_extension_migrations (migration definitions)
  // 3. All other changes (including extension table data)
  const extensionChanges = allChanges.filter((c) => c.tableName === 'haex_extensions')
  const migrationChanges = allChanges.filter((c) => c.tableName === 'haex_extension_migrations')
  const otherChanges = allChanges.filter(
    (c) => c.tableName !== 'haex_extensions' && c.tableName !== 'haex_extension_migrations',
  )

  let maxHlc = ''

  // Step 1: Apply extension registrations first (haex_extensions)
  // This is required because haex_extension_migrations has a FK to haex_extensions
  if (extensionChanges.length > 0) {
    log.info(`Applying ${extensionChanges.length} extension registration changes first...`)
    maxHlc = await applyRemoteChangesInTransactionAsync(extensionChanges, vaultKey, backendId)
  }

  // Step 2: Apply extension migration definitions (haex_extension_migrations)
  if (migrationChanges.length > 0) {
    log.info(`Applying ${migrationChanges.length} extension migration changes...`)
    const migrationMaxHlc = await applyRemoteChangesInTransactionAsync(migrationChanges, vaultKey, backendId)
    if (migrationMaxHlc > maxHlc) {
      maxHlc = migrationMaxHlc
    }
  }

  // Step 3: Always try to apply synced extension migrations
  // This creates extension tables from synced migration definitions
  // We run this even if no new migration changes came in, because:
  // - haex_extensions might have been synced in a previous batch
  // - haex_extension_migrations might have been synced in a previous batch
  // - The tables might not have been created yet on this device
  log.info('Checking for pending synced extension migrations...')
  const migrationResult = await invoke<{
    appliedCount: number
    alreadyAppliedCount: number
    appliedMigrations: string[]
  }>('apply_synced_extension_migrations')
  if (migrationResult.appliedCount > 0) {
    log.info(
      `Applied ${migrationResult.appliedCount} synced extension migrations:`,
      migrationResult.appliedMigrations,
    )
  } else {
    log.debug('No pending extension migrations to apply')
  }

  // Step 4: Now apply all other changes (including extension table data)
  // Extension tables now exist, so data won't be skipped
  if (otherChanges.length > 0) {
    log.info(`Applying ${otherChanges.length} remaining changes to local database...`)
    const otherMaxHlc = await applyRemoteChangesInTransactionAsync(otherChanges, vaultKey, backendId)
    if (otherMaxHlc > maxHlc) {
      maxHlc = otherMaxHlc
    }
  }

  return maxHlc
}

/**
 * Applies remote changes atomically in a single transaction
 * Returns the max HLC timestamp from all changes (for updating lastPushHlcTimestamp)
 *
 * NOTE: For applying changes that may include extension tables, use
 * applyAllChangesWithMigrationsAsync instead to ensure proper ordering.
 */
export const applyRemoteChangesInTransactionAsync = async (
  changes: ColumnChange[],
  vaultKey: Uint8Array,
  backendId: string,
): Promise<string> => {
  log.debug(`Decrypting ${changes.length} changes...`)

  // Calculate max HLC and decrypt all changes
  let maxHlc = ''
  const decryptedChanges = []
  let decryptErrors = 0

  for (const change of changes) {
    // Track max HLC
    if (change.hlcTimestamp > maxHlc) {
      maxHlc = change.hlcTimestamp
    }

    // Decrypt the value
    let decryptedValue
    if (change.encryptedValue && change.nonce) {
      try {
        const decryptedData = await decryptCrdtData<{ value: unknown }>(
          change.encryptedValue,
          change.nonce,
          vaultKey,
        )
        decryptedValue = decryptedData.value
      } catch (err) {
        decryptErrors++
        log.error(`Failed to decrypt change for ${change.tableName}.${change.columnName}:`, err)
        decryptedValue = null
      }
    } else {
      decryptedValue = null
    }

    decryptedChanges.push({
      tableName: change.tableName,
      rowPks: change.rowPks,
      columnName: change.columnName,
      hlcTimestamp: change.hlcTimestamp,
      batchId: change.batchId || crypto.randomUUID(), // Use existing or generate dummy
      batchSeq: change.batchSeq || 1, // Default to 1
      batchTotal: change.batchTotal || 1, // Default to 1
      decryptedValue,
    })
  }

  if (decryptErrors > 0) {
    log.warn(`${decryptErrors} changes failed to decrypt`)
  }

  log.debug(`Decryption complete. Max HLC: ${maxHlc}`)
  log.info(`Invoking Rust: apply_remote_changes_in_transaction (${decryptedChanges.length} changes)`)

  // Call Tauri command to apply changes in a transaction
  await invoke('apply_remote_changes_in_transaction', {
    changes: decryptedChanges,
    backendId,
    maxHlc,
  })

  log.debug('Rust command completed successfully')
  return maxHlc
}
