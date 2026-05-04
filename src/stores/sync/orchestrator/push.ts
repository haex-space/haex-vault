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
  scanTableForSpaceColumnChangesAsync,
  scanTableForSpaceChangesAsync,
  clearDirtyTableAsync,
  type ColumnChange,
} from '../tableScanner'
import {
  SHARED_SPACE_BUILTIN_TABLES,
  isBuiltinSharedSpaceTable,
} from '../sharedSpaceScope'
import { hlcIsNewer } from '@/utils/hlc'
import { DidAuthAction } from '@haex-space/ucan'
import { createDidAuthHeader, createFederatedDidAuthHeader } from '@/utils/auth/didAuth'
import { orchestratorLog as log, type BackendSyncState, syncMutex } from './types'
import type { MlsEpochKey } from '@bindings/MlsEpochKey'

/**
 * Soft cap for changes per push request. Keeps single requests comfortably
 * below the sync-server's internal 5000-change-per-insert chunk (PG 65534
 * parameter limit) and below typical HTTP body-size ceilings. A single
 * transaction-HLC group that exceeds this limit is still sent in one piece
 * — we never split an HLC group across requests.
 */
const PUSH_CHUNK_SOFT_LIMIT = 2000

/**
 * Splits an HLC-sorted change stream into HLC-aligned request chunks.
 *
 * Contract:
 * - Input must be sorted by hlcTimestamp ascending (the Rust scanner does
 *   this globally across tables).
 * - Every returned chunk contains whole HLC groups. An HLC group is never
 *   split between two chunks.
 * - A group larger than `softLimit` is emitted as its own (oversized) chunk
 *   rather than split.
 */
export const chunkChangesByHlc = (
  changes: ColumnChange[],
  softLimit: number,
): ColumnChange[][] => {
  if (changes.length === 0) return []

  const chunks: ColumnChange[][] = []
  let currentChunk: ColumnChange[] = []
  let groupStart = 0

  const flushGroup = (groupEnd: number): void => {
    const groupSize = groupEnd - groupStart
    if (groupSize === 0) return

    // Would appending this group exceed the soft limit? If so, emit the
    // current chunk first so the new group starts a fresh chunk. If the
    // group alone exceeds the limit, it still lands in a single (oversized)
    // chunk — preserving HLC atomicity over chunk size.
    if (currentChunk.length > 0 && currentChunk.length + groupSize > softLimit) {
      chunks.push(currentChunk)
      currentChunk = []
    }
    for (let i = groupStart; i < groupEnd; i++) currentChunk.push(changes[i]!)
  }

  for (let i = 1; i < changes.length; i++) {
    if (changes[i]!.hlcTimestamp !== changes[i - 1]!.hlcTimestamp) {
      flushGroup(i)
      groupStart = i
    }
  }
  flushGroup(changes.length)

  if (currentChunk.length > 0) chunks.push(currentChunk)
  return chunks
}

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

    // Discover extension tables that registered rows for THIS space via
    // `haex_shared_space_sync`. They follow the registry-based scanner so
    // an extension cannot leak rows belonging to a foreign space even when
    // the user happens to be a member of both. Only relevant for shared
    // backends — vault sync ships the whole DB by design.
    let extensionTablesForSpace = new Set<string>()
    if (isSharedSpace) {
      const rows = await invoke<unknown[][]>('sql_select', {
        sql: 'SELECT DISTINCT "table_name" FROM "haex_shared_space_sync" WHERE "space_id" = ?',
        params: [backend.spaceId],
      })
      extensionTablesForSpace = new Set(
        rows.map((r) => String(r[0])).filter((name) => !isBuiltinSharedSpaceTable(name)),
      )
    }

    // Scan each dirty table for column-level changes. HLC is the grouping
    // key — no separate batch id anymore.
    const allChanges: ColumnChange[] = []
    let maxHlc = lastPushHlc || ''

    for (const { tableName } of dirtyTables) {
      try {
        log.info(`[PUSH-SCAN] Scanning table: ${tableName}`)
        log.debug(`[PUSH-SCAN]   lastPushHlc: ${lastPushHlc || '(none)'}`)

        let tableChanges: ColumnChange[]
        if (!isSharedSpace) {
          // Personal vault sync replicates the entire DB to the user's own
          // backend — no per-space filter applies.
          tableChanges = await scanTableForChangesAsync(tableName, lastPushHlc, encryptionKey, deviceId, epoch)
        }
        else {
          const policy = SHARED_SPACE_BUILTIN_TABLES[tableName]
          if (policy) {
            const spaceColumn = policy.kind === 'self' ? 'id' : policy.column
            tableChanges = await scanTableForSpaceColumnChangesAsync(
              tableName,
              backend.spaceId,
              spaceColumn,
              lastPushHlc,
              encryptionKey,
              deviceId,
              epoch,
            )
          }
          else if (extensionTablesForSpace.has(tableName)) {
            tableChanges = await scanTableForSpaceChangesAsync(
              tableName,
              backend.spaceId,
              lastPushHlc,
              encryptionKey,
              deviceId,
              epoch,
            )
          }
          else {
            // Vault-private table (identities, vault settings, sync backends, …)
            // or extension table without space registration. Either way, it
            // must NEVER travel over a shared-space backend, even though it
            // is dirty in the local tracker — sharing here would leak data
            // outside the space's MLS group.
            log.info(`[PUSH-SCAN]   Skipping ${tableName} on shared-space backend (not in shared scope)`)
            continue
          }
        }

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

    // Chunk at HLC boundaries so a transaction-HLC group is never split across
    // requests. Each push (HTTP) is atomic server-side; chunking per HLC gives
    // us HLC-group atomicity across multiple pushes too. A single HLC group
    // larger than the soft limit is kept intact — oversized chunks beat split
    // groups (see crdt-refactor plan, Commit 6).
    const chunks = chunkChangesByHlc(allChanges, PUSH_CHUNK_SOFT_LIMIT)
    log.info(`Pushing ${allChanges.length} column changes in ${chunks.length} HLC-aligned chunk(s)…`)

    let firstServerTimestamp: string | null = null
    let lastSuccessfulMaxHlc = lastPushHlc || ''

    for (let i = 0; i < chunks.length; i++) {
      const chunk = chunks[i]!
      const chunkMaxHlc = chunk.reduce(
        (max, c) => (hlcIsNewer(c.hlcTimestamp, max) ? c.hlcTimestamp : max),
        chunk[0]!.hlcTimestamp,
      )
      log.info(`[PUSH-CHUNK ${i + 1}/${chunks.length}] ${chunk.length} changes, maxHlc=${chunkMaxHlc}`)

      const serverTimestamp = await pushChangesToServerAsync(
        backendId,
        backend.spaceId,
        chunk,
        syncBackendsStore,
        syncEngineStore,
      )
      if (firstServerTimestamp === null && serverTimestamp) firstServerTimestamp = serverTimestamp
      lastSuccessfulMaxHlc = chunkMaxHlc

      // Checkpoint after each chunk so a later failure does not re-push the
      // groups we already delivered. The dirty-table cleanup happens only
      // after all chunks succeed — any mid-loop throw leaves the remaining
      // groups in the scanner's next sweep.
      await syncBackendsStore.updateBackendAsync(backendId, { lastPushHlcTimestamp: chunkMaxHlc })
    }

    // Final bookkeeping: lastPullServerTimestamp init + dirty-table cleanup
    const updateData: { lastPushHlcTimestamp: string; lastPullServerTimestamp?: string } = {
      lastPushHlcTimestamp: lastSuccessfulMaxHlc,
    }

    // Only set lastPullServerTimestamp on the FIRST push (when it's not yet set)
    // This prevents re-downloading our own initial data.
    // For subsequent pushes, we must NOT update it - otherwise we'd skip changes
    // from other devices that happened between our last pull and this push.
    if (firstServerTimestamp && !backend.lastPullServerTimestamp) {
      log.info('First push: Setting initial lastPullServerTimestamp:', firstServerTimestamp)
      updateData.lastPullServerTimestamp = firstServerTimestamp
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

    // Full re-upload uses the vault sync key and is only meaningful for the
    // personal vault backend. For shared-space backends the encryption key is
    // an MLS epoch key (not the vault key) and a "push everything" pass would
    // ship rows from foreign spaces — exactly the leak we are guarding
    // against. Refuse here rather than silently falling through.
    if (backend.spaceId !== currentVaultId.value) {
      log.error('FULL PUSH FAILED: Shared-space backends do not support full re-upload')
      throw new Error('Full re-upload is only supported for the personal vault backend')
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
