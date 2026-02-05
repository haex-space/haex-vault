/**
 * Sync Pull Operations
 * Handles pulling remote changes from the sync server
 */

import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { decryptCrdtData } from '@haex-space/vault-sdk'
import type { ColumnChange } from '../tableScanner'
import { orchestratorLog as log, type BackendSyncState, type PullResult, syncMutex } from './types'
import { useExtensionBroadcastStore } from '~/stores/extensions/broadcast'
import { SYNC_TABLES_INTERNAL_EVENT } from '../syncEvents'
import type { PendingColumn } from '@bindings/PendingColumn'

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

  // Acquire mutex lock to prevent concurrent sync operations
  // This replaces the non-atomic check-then-set pattern
  const releaseLock = await syncMutex.acquire(backendId)

  // Double-check after acquiring lock (another operation might have just finished)
  if (state.isSyncing) {
    log.debug(`PULL SKIPPED: Already syncing with backend ${backendId}`)
    releaseLock()
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
      backend.serverUrl,
      backend.vaultId,
      lastPullServerTimestamp,
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

    // Step 2b: Check for and pull any pending columns
    // This handles schema version differences - if we previously skipped columns
    // that didn't exist locally (older app version), and now we have those columns
    // after an app update, we pull all data for them from the server
    const pendingColumnsPulled = await pullPendingColumnsAsync(
      backend.serverUrl,
      backend.vaultId,
      vaultKey,
      backendId,
      syncEngineStore,
    )
    if (pendingColumnsPulled > 0) {
      log.info(`Pulled ${pendingColumnsPulled} pending column changes`)
    }

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

    // Step 5: Emit sync events for store reloading and extensions
    if (tablesAffected.length > 0) {
      log.info('Emitting sync:tables-updated events for tables:', tablesAffected)

      // Emit internal event for main window stores (theme, locale, etc.)
      // This triggers the reload functions registered in syncEvents.ts
      await emit(SYNC_TABLES_INTERNAL_EVENT, { tables: tablesAffected })
      log.info('Internal sync:tables-updated event emitted for store reloading')

      // Emit filtered events to extensions (iframes + webviews)
      // Each extension only receives table names they have database permissions for
      const broadcastStore = useExtensionBroadcastStore()
      await broadcastStore.broadcastSyncTablesUpdated(tablesAffected)

      log.info('Filtered sync:tables-updated events emitted to extensions')
    }

    log.info(`========== PULL SUCCESS: ${allChanges.length} changes applied ==========`)
  } catch (error) {
    // Extract detailed error message for better debugging
    let errorMessage = 'Unknown error'
    if (error instanceof Error) {
      errorMessage = error.message
      // Check if it's a Tauri invoke error with more details
      if ('cause' in error && error.cause) {
        errorMessage += ` (cause: ${JSON.stringify(error.cause)})`
      }
    } else if (typeof error === 'object' && error !== null) {
      errorMessage = JSON.stringify(error)
    }
    log.error(`========== PULL FAILED ==========`, { message: errorMessage, error })
    state.error = errorMessage
    throw error
  } finally {
    state.isSyncing = false
    releaseLock()
  }
}

/**
 * Pulls column-level changes from server with pagination
 * Uses server timestamps (afterUpdatedAt) and secondary cursors (tableName, rowPks) for stable pagination
 * Returns both the changes and the server timestamp for storing as cursor
 *
 * @param serverUrl - Sync server URL
 * @param vaultId - Vault ID to pull changes for
 * @param lastPullServerTimestamp - Cursor for incremental sync (null for full sync)
 * @param syncEngineStore - Sync engine store for auth token
 */
export const pullChangesFromServerAsync = async (
  serverUrl: string,
  vaultId: string,
  lastPullServerTimestamp: string | null | undefined,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<PullResult> => {
  log.info('pullChangesFromServerAsync: Starting pull from', serverUrl, 'vault:', vaultId)
  const token = await syncEngineStore.getAuthTokenAsync()
  if (!token) {
    log.error('pullChangesFromServerAsync: Not authenticated')
    throw new Error('Not authenticated')
  }

  const allChanges: ColumnChange[] = []
  let hasMore = true
  let currentCursor: string | null = lastPullServerTimestamp || null
  let currentTableName: string | null = null
  let currentRowPks: string | null = null
  let pageCount = 0
  let lastServerTimestamp: string | null = null

  // Pagination loop - download ALL changes before applying
  log.info(`[PAGINATION] Starting pagination loop. Initial cursor: ${currentCursor || '(none)'}`)
  while (hasMore) {
    pageCount++
    // Build URL with all cursor parameters for stable pagination
    const params = new URLSearchParams({
      vaultId,
      limit: '1000',
    })
    if (currentCursor) params.set('afterUpdatedAt', currentCursor)
    if (currentTableName) params.set('afterTableName', currentTableName)
    if (currentRowPks) params.set('afterRowPks', currentRowPks)

    const url = `${serverUrl}/sync/pull?${params.toString()}`
    log.info(`[PAGINATION] Fetching page ${pageCount} with cursor: ${currentCursor || '(none)'}, tableName: ${currentTableName || '(none)'}, rowPks: ${currentRowPks || '(none)'}`)

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

    // Debug: Log full pagination response details
    log.info(`[PAGINATION DEBUG] Response: changes=${changes.length}, hasMore=${data.hasMore}, serverTimestamp=${data.serverTimestamp}, lastTableName=${data.lastTableName}, lastRowPks=${data.lastRowPks}`)

    allChanges.push(...changes)

    // Check if there are more pages
    hasMore = data.hasMore === true

    // Update all cursor components for next page
    lastServerTimestamp = data.serverTimestamp || null
    currentCursor = lastServerTimestamp
    currentTableName = data.lastTableName || null
    currentRowPks = data.lastRowPks || null

    log.info(`Page ${pageCount}: ${changes.length} changes (total: ${allChanges.length}, hasMore: ${hasMore})`)
  }

  log.info(`[PAGINATION] Loop complete. Total pages: ${pageCount}, Total changes: ${allChanges.length}`)
  return { changes: allChanges, serverTimestamp: lastServerTimestamp }
}

/**
 * @deprecated Use pullChangesFromServerAsync directly
 * Kept for backwards compatibility - just forwards to pullChangesFromServerAsync
 */
export const pullChangesFromServerWithConfigAsync = pullChangesFromServerAsync

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

  // Log unique tables for debugging
  const uniqueTables = [...new Set(allChanges.map((c) => c.tableName))]
  log.debug('Unique tables in changes:', uniqueTables)

  // Separate changes into categories with correct application order:
  // 1. haex_extensions (extension registrations - needed for FK in migrations)
  // 2. haex_extension_migrations (migration definitions)
  // 3. All other changes (including extension table data)
  const extensionChanges = allChanges.filter((c) => c.tableName === 'haex_extensions')
  const migrationChanges = allChanges.filter((c) => c.tableName === 'haex_extension_migrations')
  const otherChanges = allChanges.filter(
    (c) => c.tableName !== 'haex_extensions' && c.tableName !== 'haex_extension_migrations',
  )

  log.debug(`Separated: ${extensionChanges.length} extension, ${migrationChanges.length} migration, ${otherChanges.length} other`)

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

  // Step 3b: Ensure all CRDT tables have triggers set up
  // This is critical for extension tables created via sync - without triggers,
  // changes to these tables won't be marked as dirty and won't be pushed
  const triggersCreated = await invoke<number>('ensure_extension_triggers')
  if (triggersCreated > 0) {
    log.info(`Created CRDT triggers for ${triggersCreated} extension tables`)
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
  const startTime = performance.now()
  log.info(`[PERF] Starting decryption of ${changes.length} changes...`)

  // Calculate max HLC and decrypt all changes
  let maxHlc = ''
  const decryptedChanges = []
  let decryptCount = 0
  const failedDecryptions: Array<{ tableName: string; columnName: string; error: unknown }> = []

  for (const change of changes) {
    decryptCount++
    // Log every 100 changes for better visibility with smaller batches
    if (decryptCount % 100 === 0 || decryptCount === changes.length) {
      const elapsed = (performance.now() - startTime) / 1000
      const rate = decryptCount / elapsed
      log.info(`[PERF] Decrypted ${decryptCount}/${changes.length} (${elapsed.toFixed(1)}s, ${rate.toFixed(0)} changes/s)`)
    }
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
        // CRITICAL: Collect decryption failures - we will abort the entire transaction
        // Skipping individual changes would cause data inconsistency
        failedDecryptions.push({
          tableName: change.tableName,
          columnName: change.columnName,
          error: err,
        })
        log.error(`Failed to decrypt change for ${change.tableName}.${change.columnName}:`, err)
        continue
      }
    } else {
      // No encrypted value means the value is intentionally null (e.g., cleared field)
      decryptedValue = null
    }

    const changeObj = {
      tableName: change.tableName,
      rowPks: change.rowPks,
      columnName: change.columnName,
      hlcTimestamp: change.hlcTimestamp,
      batchId: change.batchId || crypto.randomUUID(), // Use existing or generate dummy
      batchSeq: change.batchSeq || 1, // Default to 1
      batchTotal: change.batchTotal || 1, // Default to 1
      decryptedValue,
    }

    decryptedChanges.push(changeObj)
  }

  // CRITICAL: If ANY decryption failed, abort the entire transaction
  // This ensures data consistency - we don't want to partially apply changes
  if (failedDecryptions.length > 0) {
    const errorDetails = failedDecryptions
      .slice(0, 5) // Show first 5 failures
      .map((f) => `${f.tableName}.${f.columnName}`)
      .join(', ')
    const moreCount = failedDecryptions.length > 5 ? ` (+${failedDecryptions.length - 5} more)` : ''

    throw new Error(
      `Decryption failed for ${failedDecryptions.length} change(s): ${errorDetails}${moreCount}. ` +
      `Transaction aborted to maintain data consistency. ` +
      `This may indicate an incorrect vault key or corrupted data on the server.`,
    )
  }

  const decryptionTime = (performance.now() - startTime) / 1000
  log.info(`[PERF] Decryption complete in ${decryptionTime.toFixed(1)}s. Max HLC: ${maxHlc}`)
  log.info(`[PERF] Invoking Rust: apply_remote_changes_in_transaction (${decryptedChanges.length} changes)`)

  // Call Tauri command to apply changes in a transaction
  const rustStartTime = performance.now()
  try {
    await invoke('apply_remote_changes_in_transaction', {
      changes: decryptedChanges,
      backendId,
      maxHlc,
    })
    const rustTime = (performance.now() - rustStartTime) / 1000
    log.info(`[PERF] Rust command completed in ${rustTime.toFixed(1)}s`)
  } catch (invokeError) {
    // Log detailed error from Rust - extract message for better visibility
    const errorMessage = invokeError instanceof Error
      ? invokeError.message
      : typeof invokeError === 'object' && invokeError !== null
        ? JSON.stringify(invokeError, null, 2)
        : String(invokeError)
    log.error('Rust command apply_remote_changes_in_transaction failed:', errorMessage)
    log.error('Full error object:', invokeError)
    throw invokeError
  }

  return maxHlc
}

/**
 * Pulls data for pending columns that were skipped during sync
 *
 * When a device has an older schema version and receives changes with unknown columns,
 * those columns are tracked in haex_crdt_pending_columns_no_sync. After the app updates
 * and migrations add those columns, this function fetches ALL data for them from the server.
 *
 * @param serverUrl - Sync server URL
 * @param vaultId - Vault ID to pull data for
 * @param vaultKey - Vault encryption key for decryption
 * @param backendId - Backend ID for applying changes
 * @param syncEngineStore - Sync engine store for auth token
 * @returns Number of columns successfully pulled
 */
export const pullPendingColumnsAsync = async (
  serverUrl: string,
  vaultId: string,
  vaultKey: Uint8Array,
  backendId: string,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<number> => {
  // Step 1: Get list of pending columns from local database
  const pendingColumns = await invoke<PendingColumn[]>('get_pending_columns')

  if (pendingColumns.length === 0) {
    log.debug('No pending columns to pull')
    return 0
  }

  log.info(`Found ${pendingColumns.length} pending columns to pull:`, pendingColumns)

  const token = await syncEngineStore.getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }

  // Step 2: Pull data for each column from server (with pagination)
  let totalPulled = 0

  for (const pendingCol of pendingColumns) {
    log.info(`Pulling data for column: ${pendingCol.tableName}.${pendingCol.columnName}`)

    const allChanges: ColumnChange[] = []
    let hasMore = true
    let lastTableName: string | undefined
    let lastRowPks: string | undefined

    // Pagination loop for this column
    while (hasMore) {
      const response = await fetch(`${serverUrl}/sync/pull-columns`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          vaultId,
          columns: [{ tableName: pendingCol.tableName, columnName: pendingCol.columnName }],
          limit: 1000,
          afterTableName: lastTableName,
          afterRowPks: lastRowPks,
        }),
      })

      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        log.error(`Failed to pull column ${pendingCol.tableName}.${pendingCol.columnName}:`, error)
        throw new Error(`Failed to pull column data: ${error.error || response.statusText}`)
      }

      const data = await response.json()
      const changes: ColumnChange[] = data.changes || []

      allChanges.push(...changes)
      hasMore = data.hasMore === true
      lastTableName = data.lastTableName
      lastRowPks = data.lastRowPks

      log.debug(`Fetched ${changes.length} changes for ${pendingCol.tableName}.${pendingCol.columnName} (total: ${allChanges.length}, hasMore: ${hasMore})`)
    }

    // Step 3: Apply the pulled changes
    if (allChanges.length > 0) {
      log.info(`Applying ${allChanges.length} changes for ${pendingCol.tableName}.${pendingCol.columnName}`)
      await applyRemoteChangesInTransactionAsync(allChanges, vaultKey, backendId)
      totalPulled += allChanges.length
    }

    // Step 4: Clear this pending column from tracking table
    await invoke('clear_pending_column', {
      tableName: pendingCol.tableName,
      columnName: pendingCol.columnName,
    })
    log.info(`Cleared pending column: ${pendingCol.tableName}.${pendingCol.columnName}`)
  }

  log.info(`Finished pulling pending columns. Total changes applied: ${totalPulled}`)
  return totalPulled
}
