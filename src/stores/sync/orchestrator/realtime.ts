/**
 * Sync Realtime Operations
 * Handles realtime subscriptions and batch accumulation from Supabase
 */

import { invoke } from '@tauri-apps/api/core'
import type {
  RealtimePostgresInsertPayload,
  RealtimePostgresUpdatePayload,
} from '@supabase/supabase-js'
import type { ColumnChange } from '../tableScanner'
import { log, type BackendSyncState, type BatchAccumulator } from './types'
import { applyRemoteChangesInTransactionAsync, pullFromBackendAsync } from './pull'

/**
 * Fetches missing changes from a batch by their sequence numbers
 */
export const fetchMissingBatchChangesAsync = async (
  backendId: string,
  batchId: string,
  receivedSeqNumbers: number[],
  totalCount: number,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<ColumnChange[]> => {
  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend) {
    throw new Error('Backend not found')
  }

  const token = await syncEngineStore.getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }

  // Calculate missing sequence numbers
  const allSeqs = Array.from({ length: totalCount }, (_, i) => i + 1)
  const missingSeqs = allSeqs.filter((seq) => !receivedSeqNumbers.includes(seq))

  if (missingSeqs.length === 0) {
    return []
  }

  log.info(`Fetching ${missingSeqs.length} missing changes for batch ${batchId}`)

  // Fetch missing changes from server
  const response = await fetch(
    `${backend.serverUrl}/sync/batch/${batchId}?seqs=${missingSeqs.join(',')}`,
    {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
      },
    },
  )

  if (!response.ok) {
    log.error('Failed to fetch missing batch changes:', response.statusText)
    throw new Error(`Failed to fetch missing batch changes: ${response.statusText}`)
  }

  const data = await response.json()
  log.debug(`Fetched ${data.changes?.length || 0} missing changes`)
  return data.changes
}

/**
 * Handles incoming realtime changes from Supabase
 * Accumulates changes by batchId and applies them once the batch is complete
 */
export const handleRealtimeChangeAsync = async (
  backendId: string,
  payload: RealtimePostgresInsertPayload<ColumnChange>,
  batchAccumulators: Map<string, BatchAccumulator>,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
) => {
  log.debug(`Realtime change from backend ${backendId}:`, payload)

  try {
    // Get backend configuration
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend?.vaultId) {
      log.error('REALTIME: Backend vaultId not configured')
      return
    }

    // Get vault key from cache
    const vaultKey = syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
    if (!vaultKey) {
      log.error('REALTIME: Vault key not available')
      return
    }

    // Extract the new record from Supabase realtime payload
    const change = payload.new
    if (!change) {
      log.warn('REALTIME: No new record in payload')
      return
    }

    // Skip if this change was made by our own device to avoid applying our own changes
    const currentDeviceId = await invoke<string>('get_device_id')
    if (change.deviceId === currentDeviceId) {
      log.debug('REALTIME: Skipping - change originated from this device')
      return
    }

    const batchId = change.batchId
    const batchTotal = change.batchTotal
    const batchSeq = change.batchSeq

    // Realtime changes must have batch info
    if (!batchId || batchTotal === undefined || batchSeq === undefined) {
      log.error('REALTIME: Change missing batch info:', change)
      return
    }

    // Get or create batch accumulator
    let accumulator = batchAccumulators.get(batchId)
    if (!accumulator) {
      accumulator = {
        backendId,
        changes: [],
        receivedCount: 0,
        totalCount: batchTotal,
        timeout: undefined,
      }
      batchAccumulators.set(batchId, accumulator)
      log.debug(`REALTIME: New batch ${batchId} started (total: ${batchTotal})`)
    }

    // Add change to accumulator
    accumulator.changes.push(change)
    accumulator.receivedCount++

    log.debug(`REALTIME: Batch ${batchId}: ${accumulator.receivedCount}/${batchTotal}`)

    // Clear any existing timeout
    if (accumulator.timeout) {
      clearTimeout(accumulator.timeout)
    }

    // Check if batch is complete
    if (accumulator.receivedCount >= batchTotal) {
      // Batch complete - apply all changes
      log.info(`REALTIME: Batch ${batchId} complete, applying ${accumulator.changes.length} changes`)

      // Sort by batchSeq to ensure correct order
      const sortedChanges = accumulator.changes.sort(
        (a, b) => (a.batchSeq ?? 0) - (b.batchSeq ?? 0),
      )

      await applyRemoteChangesInTransactionAsync(sortedChanges, vaultKey, backendId)

      // Clean up accumulator
      batchAccumulators.delete(batchId)

      log.info(`REALTIME: Batch ${batchId} applied successfully`)
    } else {
      // Set a timeout to fetch missing changes after 10 seconds
      // This handles cases where some batch items get lost in realtime
      accumulator.timeout = setTimeout(async () => {
        const acc = batchAccumulators.get(batchId)
        if (!acc) return

        log.warn(
          `REALTIME: Batch ${batchId} timeout - fetching ${acc.totalCount - acc.receivedCount} missing changes`,
        )

        try {
          // Fetch missing changes from server
          const receivedSeqs = acc.changes
            .map((c) => c.batchSeq)
            .filter((seq): seq is number => seq !== undefined)
          const missingChanges = await fetchMissingBatchChangesAsync(
            acc.backendId,
            batchId,
            receivedSeqs,
            acc.totalCount,
            syncBackendsStore,
            syncEngineStore,
          )

          // Combine with received changes
          const allChanges = [...acc.changes, ...missingChanges].sort(
            (a, b) => (a.batchSeq ?? 0) - (b.batchSeq ?? 0),
          )

          // Apply the complete batch
          await applyRemoteChangesInTransactionAsync(allChanges, vaultKey, acc.backendId)

          // Clean up
          batchAccumulators.delete(batchId)

          log.info(`REALTIME: Batch ${batchId} applied after fetching missing changes`)
        } catch (error) {
          log.error(`REALTIME: Failed to fetch missing batch ${batchId}:`, error)
          // Last resort: trigger full pull
          pullFromBackendAsync(
            acc.backendId,
            currentVaultId,
            syncStates,
            syncBackendsStore,
            syncEngineStore,
          ).catch((pullError) => {
            log.error('REALTIME: Fallback pull also failed:', pullError)
          })
        }
      }, 10000) // 10 second timeout
    }
  } catch (error) {
    log.error('REALTIME: Failed to handle change:', error)
    // Fallback: trigger a full pull if direct processing fails
    log.info('REALTIME: Falling back to full pull...')
    try {
      await pullFromBackendAsync(
        backendId,
        currentVaultId,
        syncStates,
        syncBackendsStore,
        syncEngineStore,
      )
    } catch (pullError) {
      log.error('REALTIME: Fallback pull also failed:', pullError)
    }
  }
}

/**
 * Subscribes to realtime changes from a backend
 */
export const subscribeToBackendAsync = async (
  backendId: string,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  batchAccumulators: Map<string, BatchAccumulator>,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<void> => {
  log.info(`SUBSCRIBE: Setting up realtime subscription for backend ${backendId}`)

  if (!currentVaultId) {
    log.error('SUBSCRIBE: No vault opened')
    throw new Error('No vault opened')
  }

  // Get the vaultId for this backend from the backend configuration
  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend?.vaultId) {
    log.error('SUBSCRIBE: Backend vaultId not configured')
    throw new Error('Backend vaultId not configured')
  }

  const state = syncStates[backendId]
  if (!state) {
    log.error('SUBSCRIBE: Backend not initialized')
    throw new Error('Backend not initialized')
  }

  if (state.subscription) {
    log.debug(`SUBSCRIBE: Already subscribed to backend ${backendId}`)
    return
  }

  const client = syncEngineStore.supabaseClient
  if (!client) {
    log.error('SUBSCRIBE: Supabase client not initialized')
    throw new Error('Supabase client not initialized')
  }

  try {
    log.debug(`SUBSCRIBE: Creating channel sync_changes:${backend.vaultId}`)
    // Subscribe to sync changes table for this vault (using backend vaultId)
    // Listen to both INSERT (new records) and UPDATE (UPSERT updates existing records)
    const channel = client
      .channel(`sync_changes:${backend.vaultId}`)
      .on(
        'postgres_changes',
        {
          event: 'INSERT',
          schema: 'public',
          table: 'sync_changes',
          filter: `vault_id=eq.${backend.vaultId}`,
        },
        (payload: RealtimePostgresInsertPayload<ColumnChange>) => {
          handleRealtimeChangeAsync(
            backendId,
            payload,
            batchAccumulators,
            currentVaultId,
            syncStates,
            syncBackendsStore,
            syncEngineStore,
          ).catch(log.error)
        },
      )
      .on(
        'postgres_changes',
        {
          event: 'UPDATE',
          schema: 'public',
          table: 'sync_changes',
          filter: `vault_id=eq.${backend.vaultId}`,
        },
        (payload: RealtimePostgresUpdatePayload<ColumnChange>) => {
          // UPDATE events have the same structure - .new contains the updated record
          handleRealtimeChangeAsync(
            backendId,
            payload as unknown as RealtimePostgresInsertPayload<ColumnChange>,
            batchAccumulators,
            currentVaultId,
            syncStates,
            syncBackendsStore,
            syncEngineStore,
          ).catch(log.error)
        },
      )
      .subscribe((status) => {
        if (status === 'SUBSCRIBED') {
          state.isConnected = true
          log.info(`SUBSCRIBE: Successfully subscribed to backend ${backendId}`)
        } else if (status === 'CHANNEL_ERROR' || status === 'TIMED_OUT') {
          state.isConnected = false
          state.error = `Subscription error: ${status}`
          log.error(`SUBSCRIBE: Subscription to backend ${backendId} failed: ${status}`)
        } else {
          log.debug(`SUBSCRIBE: Status changed to ${status}`)
        }
      })

    state.subscription = channel
  } catch (error) {
    log.error(`SUBSCRIBE: Failed to subscribe to backend ${backendId}:`, error)
    state.error = error instanceof Error ? error.message : 'Unknown error'
    throw error
  }
}

/**
 * Unsubscribes from realtime changes
 */
export const unsubscribeFromBackendAsync = async (
  backendId: string,
  syncStates: BackendSyncState,
): Promise<void> => {
  const state = syncStates[backendId]
  if (!state || !state.subscription) {
    return
  }

  try {
    await state.subscription.unsubscribe()
    state.subscription = null
    state.isConnected = false
    log.info(`UNSUBSCRIBE: Unsubscribed from backend ${backendId}`)
  } catch (error) {
    log.error(`UNSUBSCRIBE: Failed to unsubscribe from backend ${backendId}:`, error)
  }
}
