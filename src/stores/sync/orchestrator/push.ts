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
import { orchestratorLog as log, type BackendSyncState, syncMutex, SpaceUnavailableError } from './types'
import { buildAuthHeadersAsync } from './spaceAuth'

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

  // Acquire mutex lock to prevent concurrent sync operations
  // This replaces the non-atomic check-then-set pattern
  const releaseLock = await syncMutex.acquire(backendId)

  // Double-check after acquiring lock (another operation might have just finished)
  if (state.isSyncing) {
    log.debug(`PUSH SKIPPED: Already syncing with backend ${backendId}`)
    releaseLock()
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

    const isSpaceBackend = backend.type === 'space'
    const lastPushHlc = backend.lastPushHlcTimestamp
    log.debug('Backend config:', {
      backendId,
      vaultId: backend.vaultId,
      serverUrl: backend.serverUrl,
      lastPushHlc: lastPushHlc || '(none)',
      type: backend.type || 'personal',
    })

    // Get encryption key: space key for space backends, vault key for personal
    let encryptionKey: Uint8Array
    if (isSpaceBackend) {
      if (!backend.spaceId) {
        throw new Error('Space backend is missing spaceId configuration')
      }
      const spacesStore = useSpacesStore()
      const spaceKey = spacesStore.getSpaceKey(backend.spaceId, 1) // TODO: proper generation lookup
      if (!spaceKey) {
        log.error('PUSH FAILED: Space key not available')
        throw new Error('Space key not available')
      }
      encryptionKey = spaceKey
    } else {
      const vaultKey = syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
      if (!vaultKey) {
        log.error('PUSH FAILED: Vault key not available')
        throw new Error('Vault key not available. Please unlock vault first.')
      }
      encryptionKey = vaultKey
    }
    log.debug('Encryption key available: true')

    // Get current device ID
    const deviceStore = useDeviceStore()
    const deviceId = deviceStore.deviceId
    if (!deviceId) {
      log.error('PUSH FAILED: Device ID not available')
      throw new Error('Device ID not available')
    }
    log.debug('Device ID:', deviceId)

    // Get all dirty tables that need to be synced
    // TODO: Table filtering for space vs personal backends
    // Currently ALL dirty tables are pushed to ANY backend. Once extension space table
    // registration exists (Task 13+), this needs to be filtered:
    // - Personal backends: push only non-space tables (tables not claimed by any space)
    // - Space backends: push only tables registered for this specific space
    // Until then, all dirty tables are pushed to all backends (current behavior).
    log.info('[PUSH-SCAN] Fetching dirty tables...')
    const dirtyTables = await getDirtyTablesAsync()

    if (dirtyTables.length === 0) {
      log.info('[PUSH-SCAN] PUSH COMPLETE: No dirty tables to push')
      return
    }

    log.info(`[PUSH-SCAN] Found ${dirtyTables.length} dirty tables:`, dirtyTables.map((t) => t.tableName))

    // Capture the max last_modified timestamp from dirty tables BEFORE scanning
    // This ensures we only clear entries that existed at scan start, not new ones added during push
    const maxDirtyTimestamp = dirtyTables.reduce((max, t) => {
      return t.lastModified > max ? t.lastModified : max
    }, '')
    log.debug(`[PUSH-SCAN] Max dirty timestamp at scan start: ${maxDirtyTimestamp}`)

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
          encryptionKey,
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
      // Only clear entries that existed at scan start
      for (const { tableName } of dirtyTables) {
        await clearDirtyTableAsync(tableName, maxDirtyTimestamp)
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
    // IMPORTANT: Only clear entries that existed at scan start (before maxDirtyTimestamp)
    // This prevents clearing entries added AFTER we started scanning (which would cause data loss)
    log.debug(`Clearing dirty tables with timestamp <= ${maxDirtyTimestamp}...`)
    for (const { tableName } of dirtyTables) {
      await clearDirtyTableAsync(tableName, maxDirtyTimestamp)
    }

    log.info(`========== PUSH SUCCESS: ${allChanges.length} changes pushed ==========`)
  } catch (error) {
    if (error instanceof SpaceUnavailableError) {
      log.warn(`Space backend ${backendId} unavailable (${error.status}), disabling. Local data is preserved.`)
      await syncBackendsStore.updateBackendAsync(backendId, { enabled: false })
      state.error = error.message
      return
    }
    log.error(`========== PUSH FAILED ==========`, error)
    state.error = error instanceof Error ? error.message : 'Unknown error'
    throw error
  } finally {
    state.isSyncing = false
    releaseLock()
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
  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend) {
    log.error('pushChangesToServerAsync: Backend not found')
    throw new Error('Backend not found')
  }

  const isSpaceBackend = backend.type === 'space'

  // Get current device ID
  const deviceStore = useDeviceStore()
  const deviceId = deviceStore.deviceId

  // Hoist dynamic import outside per-change loop
  const signRecordAsync = isSpaceBackend
    ? (await import('@haex-space/vault-sdk')).signRecordAsync
    : null

  // Format changes for server API, with optional signing for space backends
  const formattedChanges = await Promise.all(changes.map(async (change) => {
    const base = {
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
    }

    if (isSpaceBackend && signRecordAsync) {
      const userKeypairStore = useUserKeypairStore()
      if (!userKeypairStore.privateKeyBase64 || !userKeypairStore.publicKeyBase64) {
        throw new Error('User keypair not available for signing')
      }

      const signature = await signRecordAsync(
        {
          tableName: change.tableName,
          rowPks: change.rowPks,
          columnName: change.columnName,
          encryptedValue: change.encryptedValue ?? null,
          hlcTimestamp: change.hlcTimestamp,
        },
        userKeypairStore.privateKeyBase64,
      )

      return {
        ...base,
        signature,
        signedBy: userKeypairStore.publicKeyBase64,
      }
    }

    return base
  }))

  // Use appropriate auth header
  const authHeaders = await buildAuthHeadersAsync(
    backend.spaceToken ?? null, backend.spaceId ?? null, () => syncEngineStore.getAuthTokenAsync(),
  )
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...authHeaders,
  }

  const url = `${backend.serverUrl}/sync/push`
  log.debug('Sending POST to:', url)
  log.debug('Request payload:', { vaultId, changesCount: formattedChanges.length, isSpaceBackend })

  // Send to server
  const response = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      vaultId,
      changes: formattedChanges,
    }),
  })

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    if (backend.spaceToken && (response.status === 401 || response.status === 404)) {
      throw new SpaceUnavailableError(
        response.status,
        `Space backend unavailable (HTTP ${response.status}): ${error.error || response.statusText}`,
      )
    }
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

  const isSpaceBackend = backend.type === 'space'
  log.debug('Backend config:', {
    backendId,
    vaultId: backend.vaultId,
    serverUrl: backend.serverUrl,
    type: backend.type || 'personal',
  })

  // Get encryption key: space key for space backends, vault key for personal
  let encryptionKey: Uint8Array
  if (isSpaceBackend) {
    if (!backend.spaceId) {
      throw new Error('Space backend is missing spaceId configuration')
    }
    const spacesStore = useSpacesStore()
    const spaceKey = spacesStore.getSpaceKey(backend.spaceId, 1) // TODO: proper generation lookup
    if (!spaceKey) {
      log.error('FULL PUSH FAILED: Space key not available')
      throw new Error('Space key not available')
    }
    encryptionKey = spaceKey
  } else {
    const vaultKey = syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
    if (!vaultKey) {
      log.error('FULL PUSH FAILED: Vault key not available')
      throw new Error('Vault key not available. Please unlock vault first.')
    }
    encryptionKey = vaultKey
  }
  log.debug('Encryption key available: true')

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
        encryptionKey,
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
