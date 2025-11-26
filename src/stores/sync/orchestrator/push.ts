/**
 * Sync Push Operations
 * Handles pushing local changes to the sync server
 */

import {
  getDirtyTablesAsync,
  scanTableForChangesAsync,
  clearDirtyTableAsync,
  type ColumnChange,
} from '../tableScanner'
import { log, type BackendSyncState } from './types'

/**
 * Pushes local changes to a specific backend using table-scanning approach
 */
export const pushToBackendAsync = async (
  backendId: string,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<void> => {
  log.info(`========== PUSH START (backend: ${backendId}) ==========`)

  if (!currentVaultId) {
    log.error('PUSH FAILED: No vault opened')
    throw new Error('No vault opened')
  }

  const state = syncStates[backendId]
  if (!state) {
    log.error('PUSH FAILED: Backend not initialized')
    throw new Error('Backend not initialized')
  }

  if (state.isSyncing) {
    log.debug(`PUSH SKIPPED: Already syncing with backend ${backendId}`)
    return
  }

  state.isSyncing = true
  state.error = null

  try {
    // Get backend configuration
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend?.vaultId) {
      log.error('PUSH FAILED: Backend vaultId not configured')
      throw new Error('Backend vaultId not configured')
    }

    const lastPushHlc = backend.lastPushHlcTimestamp
    log.debug('Backend config:', {
      backendId,
      vaultId: backend.vaultId,
      serverUrl: backend.serverUrl,
      lastPushHlc: lastPushHlc || '(none)',
    })

    // Get vault key from cache
    const vaultKey = syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
    if (!vaultKey) {
      log.error('PUSH FAILED: Vault key not available')
      throw new Error('Vault key not available. Please unlock vault first.')
    }
    log.debug('Vault key available: true')

    // Get current device ID
    const deviceStore = useDeviceStore()
    const deviceId = deviceStore.deviceId
    if (!deviceId) {
      log.error('PUSH FAILED: Device ID not available')
      throw new Error('Device ID not available')
    }
    log.debug('Device ID:', deviceId)

    // Get all dirty tables that need to be synced
    log.info('Fetching dirty tables...')
    const dirtyTables = await getDirtyTablesAsync()

    if (dirtyTables.length === 0) {
      log.info('PUSH COMPLETE: No dirty tables to push')
      return
    }

    log.info(`Found ${dirtyTables.length} dirty tables:`, dirtyTables.map((t) => t.tableName))

    // Generate a batch ID for this push - all changes in this push belong together
    const batchId = crypto.randomUUID()
    log.debug('Generated batch ID:', batchId)

    // Scan each dirty table for column-level changes (without batch seq numbers yet)
    const partialChanges: Omit<ColumnChange, 'batchSeq' | 'batchTotal'>[] = []
    let maxHlc = lastPushHlc || ''

    for (const { tableName } of dirtyTables) {
      try {
        log.info(`Scanning table: ${tableName}`)
        log.debug(`  lastPushHlc: ${lastPushHlc || '(none)'}`)

        const tableChanges = await scanTableForChangesAsync(
          tableName,
          lastPushHlc,
          vaultKey,
          batchId,
          deviceId,
        )

        partialChanges.push(...tableChanges)

        // Track max HLC timestamp
        for (const change of tableChanges) {
          if (change.hlcTimestamp > maxHlc) {
            maxHlc = change.hlcTimestamp
          }
        }

        log.info(`  Found ${tableChanges.length} column changes in ${tableName}`)
        if (tableChanges.length > 0) {
          log.debug(
            `  Sample changes:`,
            tableChanges.slice(0, 3).map((c) => ({
              column: c.columnName,
              hlc: c.hlcTimestamp,
            })),
          )
        }
      } catch (error) {
        log.error(`Failed to scan table ${tableName}:`, error)
        // Continue with other tables even if one fails
      }
    }

    // Add batch sequence numbers now that we know the total
    const batchTotal = partialChanges.length
    const allChanges: ColumnChange[] = partialChanges.map((change, index) => ({
      ...change,
      batchSeq: index + 1, // 1-based sequence
      batchTotal,
    }))

    if (allChanges.length === 0) {
      log.info('PUSH COMPLETE: No changes after scanning (tables may already be synced)')
      // Clear dirty tables even if no changes (they might have been synced already)
      for (const { tableName } of dirtyTables) {
        await clearDirtyTableAsync(tableName)
      }
      return
    }

    log.info(`Pushing ${allChanges.length} column changes to server...`)
    log.debug('Batch info:', { batchId, batchTotal })

    // Push changes to server using new format
    const serverTimestamp = await pushChangesToServerAsync(
      backendId,
      backend.vaultId,
      allChanges,
      syncBackendsStore,
      syncEngineStore,
    )

    // Update backend's lastPushHlcTimestamp (HLC for tracking what we've pushed)
    // Also set lastPullServerTimestamp from server response (for tracking what we've pulled)
    const updateData: { lastPushHlcTimestamp: string; lastPullServerTimestamp?: string } = {
      lastPushHlcTimestamp: maxHlc,
    }

    // Set lastPullServerTimestamp from server response
    // This ensures we don't re-download our own changes on the next pull
    if (serverTimestamp) {
      log.info('Setting lastPullServerTimestamp from server response:', serverTimestamp)
      updateData.lastPullServerTimestamp = serverTimestamp
    }

    log.debug('Updating backend timestamps:', updateData)
    await syncBackendsStore.updateBackendAsync(backendId, updateData)

    // Clear dirty tables after successful push
    log.debug('Clearing dirty tables...')
    for (const { tableName } of dirtyTables) {
      await clearDirtyTableAsync(tableName)
    }

    log.info(`========== PUSH SUCCESS: ${allChanges.length} changes pushed ==========`)
  } catch (error) {
    log.error(`========== PUSH FAILED ==========`, error)
    state.error = error instanceof Error ? error.message : 'Unknown error'
    throw error
  } finally {
    state.isSyncing = false
  }
}

/**
 * Pushes column-level changes to server
 * Returns the server timestamp from the response for use as pull cursor
 */
export const pushChangesToServerAsync = async (
  backendId: string,
  vaultId: string,
  changes: ColumnChange[],
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<string | null> => {
  log.debug('pushChangesToServerAsync: Getting auth token...')
  const token = await syncEngineStore.getAuthTokenAsync()
  if (!token) {
    log.error('pushChangesToServerAsync: Not authenticated')
    throw new Error('Not authenticated')
  }

  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend) {
    log.error('pushChangesToServerAsync: Backend not found')
    throw new Error('Backend not found')
  }

  // Get current device ID
  const deviceStore = useDeviceStore()
  const deviceId = deviceStore.deviceId

  // Format changes for server API
  const formattedChanges = changes.map((change) => ({
    tableName: change.tableName,
    rowPks: change.rowPks,
    columnName: change.columnName,
    hlcTimestamp: change.hlcTimestamp,
    batchId: change.batchId,
    batchSeq: change.batchSeq,
    batchTotal: change.batchTotal,
    deviceId,
    encryptedValue: change.encryptedValue,
    nonce: change.nonce,
  }))

  const url = `${backend.serverUrl}/sync/push`
  log.debug('Sending POST to:', url)
  log.debug('Request payload:', { vaultId, changesCount: formattedChanges.length })

  // Send to server
  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({
      vaultId,
      changes: formattedChanges,
    }),
  })

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    log.error('Server returned error:', { status: response.status, error })
    throw new Error(`Failed to push changes: ${error.error || response.statusText}`)
  }

  const responseData = await response.json().catch(() => ({}))
  log.info(`Server accepted ${changes.length} changes`, responseData)

  // Return the server timestamp for use as pull cursor
  return responseData.serverTimestamp || null
}
