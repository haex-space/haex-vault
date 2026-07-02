/**
 * Sync Pull — top-level orchestrators (cursor advancement, pending columns)
 */

import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { fetch } from '@tauri-apps/plugin-http'
import { DidAuthAction } from '@haex-space/ucan'
import type { ColumnChange } from '../../tableScanner'
import { createDidAuthHeader, createFederatedDidAuthHeader } from '@/utils/auth/didAuth'
import { orchestratorLog as log, type BackendSyncState, syncMutex } from '../types'
import { useExtensionBroadcastStore } from '~/stores/extensions/broadcast'
import { SYNC_TABLES_INTERNAL_EVENT } from '../../syncEvents'
import type { PendingColumn } from '@bindings/PendingColumn'
import { streamPullAndApplyAsync } from './page'
import { applyRemoteChangesInTransactionAsync, verifyPulledChangesAsync } from './apply'

/**
 * Resolves the initial cursor for a pull cycle, applying the pending-tables
 * recovery logic.
 *
 * If any recoverable pending tables exist (tables that were skipped during a
 * prior apply because the extension was not yet installed, but are now present
 * locally), the cursor is reset to null so a full re-pull covers the gap.
 * Otherwise the persisted cursor is returned unchanged.
 *
 * @param persistedCursor - The last known server timestamp from the backend record.
 * @param recoverablePendingTables - Table names returned by get_recoverable_pending_tables.
 * @returns The cursor to use and the list of tables that triggered the reset.
 */
export function resolveInitialCursor(
  persistedCursor: string | null,
  recoverablePendingTables: string[],
): { cursor: string | null; recoveredTables: string[] } {
  if (recoverablePendingTables.length > 0) {
    return { cursor: null, recoveredTables: recoverablePendingTables }
  }
  return { cursor: persistedCursor, recoveredTables: [] }
}

/**
 * Pulls changes from a specific backend using column-level HLC comparison.
 *
 * Streams pages from the server and applies each one immediately, splitting
 * the page's "other-table" changes into per-source-transaction groups (one
 * HLC = one transaction) and committing each group in its own DB transaction.
 * The trailing (max-HLC) group of a page is held back across the page boundary
 * until the next page — or `hasMore = false` — confirms it is complete, so a
 * source transaction is never applied in pieces. This keeps the receive-side
 * memory bounded (one page + one held-back transaction, not the whole history)
 * and ensures one bad transaction can't halt the entire sync: a failing group
 * leaves the cursor at the last successfully-applied page, and the next pull
 * cycle resumes from there.
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
  const releaseLock = await syncMutex.acquire(backendId)
  state.isSyncing = true
  state.error = null

  try {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend?.spaceId) {
      log.error('PULL FAILED: Backend spaceId not configured')
      throw new Error('Backend spaceId not configured')
    }

    // Get encryption key: vault sync key from local DB
    const encryptionKey = await syncEngineStore.getSyncKeyFromDbAsync(backendId)
    if (!encryptionKey) {
      log.error('PULL FAILED: Vault sync key not available')
      throw new Error('Vault sync key not available')
    }

    const lastPullServerTimestamp = backend.lastPullServerTimestamp

    // Check for tables that were skipped in a prior apply because the extension
    // was not yet installed but are now present locally. If any are found,
    // reset the cursor to null so a full re-pull covers the gap.
    const recoverableTables = await invoke<string[]>('get_recoverable_pending_tables')
    const { cursor: initialCursor, recoveredTables } = resolveInitialCursor(
      lastPullServerTimestamp || null,
      recoverableTables,
    )
    if (recoveredTables.length > 0) {
      log.debug(`Pending-tables recovery: resetting cursor to null for tables: ${recoveredTables.join(', ')}`)
    }

    log.debug('Pull config:', {
      backendId,
      spaceId: backend.spaceId,
      lastPullServerTimestamp: initialCursor || '(none - full sync)',
    })

    const federation = backend.type === 'relay' && backend.homeServerDid && backend.originServerDid
      ? { serverDid: backend.homeServerDid, originServerDid: backend.originServerDid }
      : undefined

    const streamResult = await streamPullAndApplyAsync({
      homeServerUrl: backend.homeServerUrl,
      spaceId: backend.spaceId,
      initialCursor,
      encryptionKey,
      backendId,
      backendIdentityId: backend.identityId,
      federation,
      // Persist the cursor per FULLY-APPLIED page so a mid-stream failure
      // leaves the next cycle resuming from the last good page (re-applying
      // already-committed groups is a CRDT LWW no-op).
      onPageCommitted: async (pageServerTimestamp) => {
        if (pageServerTimestamp) {
          await syncBackendsStore.updateBackendAsync(backendId, {
            lastPullServerTimestamp: pageServerTimestamp,
          })
        }
      },
    })

    const { totalApplied, pageCount, tablesAffected } = streamResult

    // Pull succeeded — clear pending-table markers for tables that triggered a
    // cursor reset. Do this before the early-return so a zero-changes re-pull
    // (table existed but had no new data) still clears the marker.
    for (const tableName of recoveredTables) {
      await invoke('clear_pending_table', { tableName })
      log.debug(`Cleared pending-table marker: ${tableName}`)
    }

    if (totalApplied === 0) {
      log.info('PULL COMPLETE: No new changes from server')
      return
    }

    log.info(`Streamed ${totalApplied} changes across ${pageCount} pages (tables: ${tablesAffected.size})`)

    // Pull any pending columns (older app version skipped columns that now
    // exist after migrations). This runs AFTER the main streaming apply so the
    // base data is in place; it has its own pagination + apply path.
    const pendingColumnsPulled = await pullPendingColumnsAsync(
      backend.homeServerUrl,
      backend.spaceId,
      encryptionKey,
      backendId,
      syncEngineStore,
    )
    if (pendingColumnsPulled > 0) {
      log.info(`Pulled ${pendingColumnsPulled} pending column changes`)
    }

    // `lastPullServerTimestamp` was advanced per page inside the loop above; no
    // end-of-cycle update is needed.

    log.debug('Reloading backend config after pull...')
    await syncBackendsStore.loadBackendsAsync()

    // Emit sync events for store reloading + extensions.
    const tables = [...tablesAffected]
    if (tables.length > 0) {
      log.info('Emitting sync:tables-updated events for tables:', tables)
      // Internal event drives main-window store reloads.
      await emit(SYNC_TABLES_INTERNAL_EVENT, { tables })
      log.info('Internal sync:tables-updated event emitted for store reloading')

      // Filtered per-extension events (each extension only sees its tables).
      const broadcastStore = useExtensionBroadcastStore()
      await broadcastStore.broadcastSyncTablesUpdated(tables)
      log.info('Filtered sync:tables-updated events emitted to extensions')
    }

    if (tablesAffected.has('haex_device_mls_enrollments')) {
      const deviceStore = useDeviceStore()
      if (deviceStore.deviceId) {
        import('@/composables/useDeviceEnrollment').then(({ useDeviceEnrollment }) => {
          const { syncEnrollmentsAsync } = useDeviceEnrollment()
          syncEnrollmentsAsync(deviceStore.deviceId!).catch((e) => log.debug('Device enrollment sync failed:', e))
        })
      }
    }

    log.info(`========== PULL SUCCESS: ${totalApplied} changes applied across ${pageCount} pages ==========`)
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
    log.error(`========== PULL FAILED ==========`, { message: errorMessage, stack: error instanceof Error ? error.stack : undefined })
    state.error = errorMessage
    throw error
  } finally {
    state.isSyncing = false
    releaseLock()
  }
}

/**
 * Pulls data for pending columns that were skipped during sync
 *
 * When a device has an older schema version and receives changes with unknown columns,
 * those columns are tracked in haex_crdt_pending_columns_no_sync. After the app updates
 * and migrations add those columns, this function fetches ALL data for them from the server.
 *
 * @param homeServerUrl - Sync server URL
 * @param spaceId - Space ID to pull data for
 * @param vaultKey - Vault encryption key for decryption
 * @param backendId - Backend ID for applying changes
 * @param syncEngineStore - Sync engine store for auth token
 * @returns Number of columns successfully pulled
 */
export const pullPendingColumnsAsync = async (
  homeServerUrl: string,
  spaceId: string,
  vaultKey: Uint8Array,
  backendId: string,
  _syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<number> => {
  // Step 1: Get list of pending columns from local database
  const pendingColumns = await invoke<PendingColumn[]>('get_pending_columns')

  if (pendingColumns.length === 0) {
    log.debug('No pending columns to pull')
    return 0
  }

  log.info(`Found ${pendingColumns.length} pending columns to pull:`, pendingColumns)

  const syncBackendsStore = useSyncBackendsStore()
  const backendRecord = syncBackendsStore.backends.find(b => b.id === backendId)
  const identityStore = useIdentityStore()
  const resolved = backendRecord?.identityId ? await identityStore.getIdentityByIdAsync(backendRecord.identityId) : null
  if (!resolved?.privateKey) throw new Error('No identity configured for this backend')
  const identity = { privateKey: resolved.privateKey, did: resolved.did }

  const federation = backendRecord?.type === 'relay' && backendRecord.homeServerDid && backendRecord.originServerDid
    ? { serverDid: backendRecord.homeServerDid, originServerDid: backendRecord.originServerDid }
    : undefined

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
      const requestBody = JSON.stringify({
        spaceId,
        columns: [{ tableName: pendingCol.tableName, columnName: pendingCol.columnName }],
        limit: 1000,
        afterTableName: lastTableName,
        afterRowPks: lastRowPks,
      })
      const authHeader = federation
        ? await createFederatedDidAuthHeader({
            did: identity.did,
            privateKeyBase64: identity.privateKey,
            action: DidAuthAction.SyncPullColumns,
            federation: { spaceId, serverDid: federation.originServerDid, relayDid: federation.serverDid },
            body: requestBody,
          })
        : await createDidAuthHeader(identity.privateKey, identity.did, DidAuthAction.SyncPullColumns, requestBody)

      const response = await fetch(`${homeServerUrl}/sync/pull-columns`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: authHeader },
        body: requestBody,
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

    // Step 3: Verify signatures + UCAN authorization, then apply.
    // A BatchVerificationError here aborts pullPendingColumnsAsync before clear_pending_column
    // runs, so this pending column stays flagged for retry on the next pull.
    if (allChanges.length > 0) {
      await verifyPulledChangesAsync(allChanges, spaceId)
      log.info(`Applying ${allChanges.length} changes for ${pendingCol.tableName}.${pendingCol.columnName}`)
      await applyRemoteChangesInTransactionAsync(allChanges, vaultKey, backendId, spaceId)
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
