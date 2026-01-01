/**
 * Sync Push Operations
 * Handles pushing local changes to the sync server
 */

import {
  getDirtyTablesAsync,
  getAllCrdtTablesAsync,
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
    log.info('[PUSH-SCAN] Fetching dirty tables...')
    const dirtyTables = await getDirtyTablesAsync()

    if (dirtyTables.length === 0) {
      log.info('[PUSH-SCAN] PUSH COMPLETE: No dirty tables to push')
      return
    }

    log.info(`[PUSH-SCAN] Found ${dirtyTables.length} dirty tables:`, dirtyTables.map((t) => t.tableName))
    // Log extra details for haex_vault_settings
    const hasVaultSettings = dirtyTables.some(t => t.tableName === 'haex_vault_settings')
    if (hasVaultSettings) {
      log.info(`[PUSH-SCAN] ⚠️ haex_vault_settings IS DIRTY! Stack trace:`, new Error().stack)
    }

    // Generate a batch ID for this push - all changes in this push belong together
    const batchId = crypto.randomUUID()
    log.debug('Generated batch ID:', batchId)

    // Scan each dirty table for column-level changes (without batch seq numbers yet)
    const partialChanges: Omit<ColumnChange, 'batchSeq' | 'batchTotal'>[] = []
    let maxHlc = lastPushHlc || ''

    for (const { tableName } of dirtyTables) {
      try {
        log.info(`[PUSH-SCAN] Scanning table: ${tableName}`)
        log.debug(`[PUSH-SCAN]   lastPushHlc: ${lastPushHlc || '(none)'}`)

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

        log.info(`[PUSH-SCAN]   Found ${tableChanges.length} column changes in ${tableName}`)
        if (tableChanges.length > 0) {
          // For haex_vault_settings, log ALL changes to see what's happening
          if (tableName === 'haex_vault_settings') {
            log.info(`[PUSH-SCAN] ⚠️ haex_vault_settings CHANGES:`, tableChanges.map((c) => ({
              rowPks: c.rowPks,
              column: c.columnName,
              hlc: c.hlcTimestamp,
            })))
          } else {
            log.debug(
              `[PUSH-SCAN]   Sample changes:`,
              tableChanges.slice(0, 3).map((c) => ({
                column: c.columnName,
                hlc: c.hlcTimestamp,
              })),
            )
          }
        }
      } catch (error) {
        log.error(`[PUSH-SCAN] Failed to scan table ${tableName}:`, error)
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
    const updateData: { lastPushHlcTimestamp: string; lastPullServerTimestamp?: string } = {
      lastPushHlcTimestamp: maxHlc,
    }

    // Only set lastPullServerTimestamp on the FIRST push (when it's not yet set)
    // This prevents re-downloading our own initial data.
    // For subsequent pushes, we must NOT update it - otherwise we'd skip changes
    // from other devices that happened between our last pull and this push.
    if (serverTimestamp && !backend.lastPullServerTimestamp) {
      log.info('First push: Setting initial lastPullServerTimestamp:', serverTimestamp)
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

/**
 * Pushes ALL local data to a backend (full re-upload).
 * Unlike pushToBackendAsync, this ignores lastPushHlcTimestamp and scans ALL CRDT tables.
 * Used when server data was deleted but local data still exists.
 */
export const pushAllDataToBackendAsync = async (
  backendId: string,
): Promise<void> => {
  const { currentVaultId } = storeToRefs(useVaultStore())
  const syncBackendsStore = useSyncBackendsStore()
  const syncEngineStore = useSyncEngineStore()

  log.info(`========== FULL PUSH START (backend: ${backendId}) ==========`)

  if (!currentVaultId.value) {
    log.error('FULL PUSH FAILED: No vault opened')
    throw new Error('No vault opened')
  }

  // Get backend configuration
  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend?.vaultId) {
    log.error('FULL PUSH FAILED: Backend vaultId not configured')
    throw new Error('Backend vaultId not configured')
  }

  log.debug('Backend config:', {
    backendId,
    vaultId: backend.vaultId,
    serverUrl: backend.serverUrl,
  })

  // Get vault key from cache
  const vaultKey = syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
  if (!vaultKey) {
    log.error('FULL PUSH FAILED: Vault key not available')
    throw new Error('Vault key not available. Please unlock vault first.')
  }
  log.debug('Vault key available: true')

  // Get current device ID
  const deviceStore = useDeviceStore()
  const deviceId = deviceStore.deviceId
  if (!deviceId) {
    log.error('FULL PUSH FAILED: Device ID not available')
    throw new Error('Device ID not available')
  }
  log.debug('Device ID:', deviceId)

  // Get ALL CRDT tables (not just dirty ones)
  log.info('Fetching all CRDT tables...')
  const allTables = await getAllCrdtTablesAsync()

  if (allTables.length === 0) {
    log.info('FULL PUSH COMPLETE: No CRDT tables found')
    return
  }

  log.info(`Found ${allTables.length} CRDT tables:`, allTables)

  // Generate a batch ID for this push
  const batchId = crypto.randomUUID()
  log.debug('Generated batch ID:', batchId)

  // Scan each table for ALL data (null = no lastPushHlc filter)
  const partialChanges: Omit<ColumnChange, 'batchSeq' | 'batchTotal'>[] = []
  let maxHlc = ''

  for (const tableName of allTables) {
    try {
      log.info(`Scanning table: ${tableName} (full scan)`)

      const tableChanges = await scanTableForChangesAsync(
        tableName,
        null, // null = scan ALL data, not just newer than lastPushHlc
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
    } catch (error) {
      log.error(`Failed to scan table ${tableName}:`, error)
      // Continue with other tables even if one fails
    }
  }

  // Add batch sequence numbers
  const batchTotal = partialChanges.length
  const allChanges: ColumnChange[] = partialChanges.map((change, index) => ({
    ...change,
    batchSeq: index + 1,
    batchTotal,
  }))

  if (allChanges.length === 0) {
    log.info('FULL PUSH COMPLETE: No data to push')
    return
  }

  log.info(`Pushing ${allChanges.length} column changes to server...`)
  log.debug('Batch info:', { batchId, batchTotal })

  // Push changes to server
  const serverTimestamp = await pushChangesToServerAsync(
    backendId,
    backend.vaultId,
    allChanges,
    syncBackendsStore,
    syncEngineStore,
  )

  // Update backend timestamps
  const updateData: { lastPushHlcTimestamp: string; lastPullServerTimestamp?: string } = {
    lastPushHlcTimestamp: maxHlc,
  }

  // Set lastPullServerTimestamp to prevent re-downloading our own data
  if (serverTimestamp) {
    log.info('Setting lastPullServerTimestamp:', serverTimestamp)
    updateData.lastPullServerTimestamp = serverTimestamp
  }

  log.debug('Updating backend timestamps:', updateData)
  await syncBackendsStore.updateBackendAsync(backendId, updateData)

  // Clear all dirty tables after successful push
  log.debug('Clearing all dirty tables...')
  const dirtyTables = await getDirtyTablesAsync()
  for (const { tableName } of dirtyTables) {
    await clearDirtyTableAsync(tableName)
  }

  log.info(`========== FULL PUSH SUCCESS: ${allChanges.length} changes pushed ==========`)
}
