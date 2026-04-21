/**
 * Sync Push Operations
 * Handles pushing local changes to the sync server
 */

import { invoke } from '@tauri-apps/api/core'
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import {
  getDirtyTablesAsync,
  getAllCrdtTablesAsync,
  scanTableForChangesAsync,
  clearDirtyTableAsync,
  type ColumnChange,
} from '../tableScanner'
import { hlcIsNewer } from '@/utils/hlc'
import { DidAuthAction } from '@haex-space/ucan'
import { createDidAuthHeader, createFederatedDidAuthHeader } from '@/utils/auth/didAuth'
import { orchestratorLog as log, type BackendSyncState, syncMutex } from './types'
import type { MlsEpochKey } from '@bindings/MlsEpochKey'

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
  const releaseLock = await syncMutex.acquire(backendId)
  state.isSyncing = true
  state.error = null

  try {
    // Get backend configuration
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend?.spaceId) {
      log.error('PUSH FAILED: Backend spaceId not configured')
      throw new Error('Backend spaceId not configured')
    }

    const lastPushHlc = backend.lastPushHlcTimestamp
    log.debug('Backend config:', {
      backendId,
      spaceId: backend.spaceId,
      homeServerUrl: backend.homeServerUrl,
      lastPushHlc: lastPushHlc || '(none)',
    })

    // Determine encryption key: MLS epoch key for shared spaces, vault key for personal
    const isSharedSpace = backend.spaceId !== currentVaultId
    let encryptionKey: Uint8Array
    let epoch: number | undefined

    if (isSharedSpace) {
      const epochKey: MlsEpochKey = await invoke('mls_export_epoch_key', { spaceId: backend.spaceId })
      encryptionKey = new Uint8Array(epochKey.key)
      epoch = Number(epochKey.epoch)
      log.debug(`Using MLS epoch key (epoch ${epoch}) for shared space`)
    } else {
      const vaultKey = await syncEngineStore.getSyncKeyFromDbAsync(backendId)
      if (!vaultKey) throw new Error('Vault sync key not available')
      encryptionKey = vaultKey
    }

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

    // Capture the max last_modified timestamp from dirty tables BEFORE scanning
    // This ensures we only clear entries that existed at scan start, not new ones added during push
    const maxDirtyTimestamp = dirtyTables.reduce((max, t) => {
      return t.lastModified > max ? t.lastModified : max
    }, '')
    log.debug(`[PUSH-SCAN] Max dirty timestamp at scan start: ${maxDirtyTimestamp}`)

    // Scan each dirty table for column-level changes. HLC is the grouping
    // key — no separate batch id anymore.
    const allChanges: ColumnChange[] = []
    let maxHlc = lastPushHlc || ''

    for (const { tableName } of dirtyTables) {
      try {
        log.info(`[PUSH-SCAN] Scanning table: ${tableName}`)
        log.debug(`[PUSH-SCAN]   lastPushHlc: ${lastPushHlc || '(none)'}`)

        const tableChanges = await scanTableForChangesAsync(tableName, lastPushHlc, encryptionKey, deviceId, epoch)

        allChanges.push(...tableChanges)

        // Track max HLC timestamp
        for (const change of tableChanges) {
          if (hlcIsNewer(change.hlcTimestamp, maxHlc)) {
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

    if (allChanges.length === 0) {
      log.info('PUSH COMPLETE: No changes after scanning (tables may already be synced)')
      // Clear dirty tables even if no changes (they might have been synced already)
      for (const { tableName } of dirtyTables) {
        await clearDirtyTableAsync(tableName, maxDirtyTimestamp)
      }
      return
    }

    log.info(`Pushing ${allChanges.length} column changes to server...`)

    // Push changes to server using new format
    const serverTimestamp = await pushChangesToServerAsync(
      backendId,
      backend.spaceId,
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
    const errMsg = error instanceof Error ? error.message : String(error)
    const errStack = error instanceof Error ? error.stack : undefined
    log.error(`========== PUSH FAILED ==========`, { message: errMsg, stack: errStack })
    state.error = errMsg
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
  spaceId: string,
  changes: ColumnChange[],
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  _syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<string | null> => {
  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend) {
    log.error('pushChangesToServerAsync: Backend not found')
    throw new Error('Backend not found')
  }

  // Get current device ID
  const deviceStore = useDeviceStore()
  const deviceId = deviceStore.deviceId

  // Resolve identity for record signing + auth. Every push MUST be signed;
  // an unsigned backend identity is a configuration error and aborts the push.
  const identityStore = useIdentityStore()
  const identity = await identityStore.getIdentityByIdAsync(backend.identityId)
  if (!identity?.privateKey) {
    throw new Error(`Cannot push: backend ${backend.id} has no identity private key — records cannot be signed`)
  }
  const identityPrivateKey = identity.privateKey
  const identityPublicKey = await didKeyToPublicKeyAsync(identity.did)

  const { signRecordAsync } = await import('@haex-space/vault-sdk')

  // Format changes for server API — every change is signed.
  const formattedChanges = await Promise.all(changes.map(async (change) => {
    const signature = await signRecordAsync(
      {
        tableName: change.tableName,
        rowPks: change.rowPks,
        columnName: change.columnName,
        encryptedValue: change.encryptedValue ?? null,
        hlcTimestamp: change.hlcTimestamp,
      },
      identityPrivateKey,
    )

    const formatted: Record<string, unknown> = {
      tableName: change.tableName,
      rowPks: change.rowPks,
      columnName: change.columnName,
      hlcTimestamp: change.hlcTimestamp,
      deviceId,
      encryptedValue: change.encryptedValue,
      nonce: change.nonce,
      signature,
      signedBy: identityPublicKey,
    }
    if (change.epoch !== undefined) formatted.epoch = change.epoch

    return formatted
  }))

  const url = `${backend.homeServerUrl}/sync/push`
  const requestBody = JSON.stringify({ spaceId, changes: formattedChanges })

  log.debug('Sending POST to:', url)
  log.debug('Request payload:', { spaceId, changesCount: formattedChanges.length })

  // Send to server with appropriate auth
  const authHeader = backend.type === 'relay' && backend.homeServerDid && backend.originServerDid
    ? await createFederatedDidAuthHeader({
        did: identity.did,
        privateKeyBase64: identity.privateKey,
        action: DidAuthAction.SyncPush,
        federation: {
          spaceId,
          serverDid: backend.originServerDid,
          relayDid: backend.homeServerDid,
        },
        body: requestBody,
      })
    : await createDidAuthHeader(identity.privateKey, identity.did, DidAuthAction.SyncPush, requestBody)

  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: authHeader },
    body: requestBody,
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

  // Acquire mutex lock to prevent concurrent sync operations
  const releaseLock = await syncMutex.acquire(backendId)

  try {
    // Get backend configuration
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend?.spaceId) {
      log.error('FULL PUSH FAILED: Backend spaceId not configured')
      throw new Error('Backend spaceId not configured')
    }

    log.debug('Backend config:', {
      backendId,
      spaceId: backend.spaceId,
      homeServerUrl: backend.homeServerUrl,
    })

    // Get encryption key: vault sync key from local DB
    const encryptionKey = await syncEngineStore.getSyncKeyFromDbAsync(backendId)
    if (!encryptionKey) {
      log.error('FULL PUSH FAILED: Vault sync key not available')
      throw new Error('Vault sync key not available')
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

    // Scan each table for ALL data (null = no lastPushHlc filter)
    const allChanges: ColumnChange[] = []
    let maxHlc = ''

    for (const tableName of allTables) {
      try {
        log.info(`Scanning table: ${tableName} (full scan)`)

        const tableChanges = await scanTableForChangesAsync(tableName, null, encryptionKey, deviceId)

        allChanges.push(...tableChanges)

        // Track max HLC timestamp
        for (const change of tableChanges) {
          if (hlcIsNewer(change.hlcTimestamp, maxHlc)) {
            maxHlc = change.hlcTimestamp
          }
        }

        log.info(`  Found ${tableChanges.length} column changes in ${tableName}`)
      } catch (error) {
        log.error(`Failed to scan table ${tableName}:`, error)
        // Continue with other tables even if one fails
      }
    }

    if (allChanges.length === 0) {
      log.info('FULL PUSH COMPLETE: No data to push')
      return
    }

    log.info(`Pushing ${allChanges.length} column changes to server...`)

    // Push changes to server
    const serverTimestamp = await pushChangesToServerAsync(
      backendId,
      backend.spaceId,
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
  } finally {
    releaseLock()
  }
}
