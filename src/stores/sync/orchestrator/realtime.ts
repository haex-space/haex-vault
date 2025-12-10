/**
 * Sync Realtime Operations
 * Handles realtime subscriptions from Supabase
 *
 * Realtime events are used as triggers to initiate a pull from the server.
 * This ensures data consistency by always using the pull endpoint which
 * guarantees complete data delivery.
 */

import type { RealtimePostgresChangesPayload } from '@supabase/supabase-js'
import { log, type BackendSyncState } from './types'
import { pullFromBackendAsync } from './pull'

/** Debounce timers per backend */
const pullDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>()

/** Debounce delay in milliseconds */
const PULL_DEBOUNCE_MS = 500

/**
 * Triggers a debounced pull from the backend.
 * Multiple rapid realtime events will only trigger one pull.
 */
const triggerDebouncedPullAsync = (
  backendId: string,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
) => {
  // Clear existing timer for this backend
  const existingTimer = pullDebounceTimers.get(backendId)
  if (existingTimer) {
    clearTimeout(existingTimer)
  }

  // Set new debounced timer
  const timer = setTimeout(async () => {
    pullDebounceTimers.delete(backendId)

    try {
      log.info(`REALTIME: Triggering pull for backend ${backendId}`)
      await pullFromBackendAsync(
        backendId,
        currentVaultId,
        syncStates,
        syncBackendsStore,
        syncEngineStore,
      )
    } catch (error) {
      log.error(`REALTIME: Pull failed for backend ${backendId}:`, error)
    }
  }, PULL_DEBOUNCE_MS)

  pullDebounceTimers.set(backendId, timer)
}

/**
 * Handles incoming realtime changes from Supabase.
 * Simply triggers a debounced pull - actual data processing happens in pull.ts
 */
const handleRealtimeChangeAsync = async (
  backendId: string,
  payload: RealtimePostgresChangesPayload<Record<string, unknown>>,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
) => {
  log.debug('REALTIME: Change event received')

  // Skip if this change was made by our own device
  // Supabase Realtime returns snake_case column names
  const deviceStore = useDeviceStore()
  const newRecord = payload.new as Record<string, unknown> | undefined
  const deviceId = newRecord?.device_id as string | undefined
  if (deviceId === deviceStore.deviceId) {
    log.debug('REALTIME: Skipping - change originated from this device')
    return
  }

  // Trigger a debounced pull
  triggerDebouncedPullAsync(
    backendId,
    currentVaultId,
    syncStates,
    syncBackendsStore,
    syncEngineStore,
  )
}

/**
 * Subscribes to realtime changes from a backend
 */
export const subscribeToBackendAsync = async (
  backendId: string,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<void> => {
  log.info(`SUBSCRIBE: Setting up realtime subscription for backend ${backendId}`)

  if (!currentVaultId) {
    log.error('SUBSCRIBE: No vault opened')
    throw new Error('No vault opened')
  }

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
    // Ensure auth token is set for realtime connection
    const token = await syncEngineStore.getAuthTokenAsync()
    if (token) {
      log.info('SUBSCRIBE: Setting auth token for realtime connection')
      client.realtime.setAuth(token)
    } else {
      log.error('SUBSCRIBE: No auth token available for realtime connection - subscription will likely fail')
    }

    // The sync_changes table is partitioned by vault_id
    // Each partition is named: sync_changes_<vault_id_with_underscores>
    // We need to subscribe to the specific partition, not the parent table
    const partitionName = `sync_changes_${backend.vaultId.replace(/-/g, '_')}`
    const channelName = `sync_changes:${backend.vaultId}`
    log.info(`SUBSCRIBE: Creating channel "${channelName}" for partition "${partitionName}"`)

    // Subscribe to the vault's specific partition
    // Listen to both INSERT and UPDATE (UPSERT triggers UPDATE for existing records)
    // Note: No filter needed since each partition only contains data for one vault_id
    const channel = client
      .channel(channelName)
      .on(
        'postgres_changes',
        {
          event: 'INSERT',
          schema: 'public',
          table: partitionName,
        },
        (payload) => {
          log.info(`REALTIME: INSERT event received on ${partitionName}`)
          handleRealtimeChangeAsync(
            backendId,
            payload,
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
          table: partitionName,
        },
        (payload) => {
          log.info(`REALTIME: UPDATE event received on ${partitionName}`)
          handleRealtimeChangeAsync(
            backendId,
            payload,
            currentVaultId,
            syncStates,
            syncBackendsStore,
            syncEngineStore,
          ).catch(log.error)
        },
      )
      .subscribe((status, err) => {
        log.debug(`SUBSCRIBE: Channel status changed to ${status}`, err ? `Error: ${JSON.stringify(err)}` : '')
        if (status === 'SUBSCRIBED') {
          state.isConnected = true
          log.info(`SUBSCRIBE: Successfully subscribed to backend ${backendId}`)
        } else if (status === 'CHANNEL_ERROR' || status === 'TIMED_OUT') {
          state.isConnected = false
          const errorDetails = err ? JSON.stringify(err) : 'unknown'
          state.error = `Subscription error: ${status} - ${errorDetails}`
          // Log as warning instead of error - realtime is optional, periodic pull is the fallback
          log.warn(`SUBSCRIBE: Realtime subscription failed for backend ${backendId}: ${status} (periodic pull will be used as fallback)`)
        } else if (status === 'CLOSED') {
          state.isConnected = false
          log.debug(`SUBSCRIBE: Channel closed for backend ${backendId}`)
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

  // Clear any pending debounce timer
  const timer = pullDebounceTimers.get(backendId)
  if (timer) {
    clearTimeout(timer)
    pullDebounceTimers.delete(backendId)
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
