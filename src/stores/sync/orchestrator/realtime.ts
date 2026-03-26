/**
 * Sync Realtime Operations
 * Handles realtime subscriptions from Supabase
 *
 * Realtime events are used as triggers to initiate a pull from the server.
 * This ensures data consistency by always using the pull endpoint which
 * guarantees complete data delivery.
 *
 * Android/Mobile considerations:
 * - WebSocket connections are killed when app goes to background (Doze mode)
 * - We listen for visibility changes to reconnect when app resumes
 * - Reconnection resets retry counts for a fresh start
 */

import type { RealtimeChannel, RealtimePostgresChangesPayload, SupabaseClient } from '@supabase/supabase-js'
import { orchestratorLog as log, type BackendSyncState } from './types'
import { pullFromBackendAsync } from './pull'

/** Debounce timers per backend */
const pullDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>()

/** Retry timers per backend */
const subscriptionRetryTimers = new Map<string, ReturnType<typeof setTimeout>>()

/** Retry counts per backend */
const subscriptionRetryCounts = new Map<string, number>()

/** Guard against concurrent subscribe attempts per backend */
const subscriptionInProgress = new Set<string>()

/** Tracks the currently active channel per backend to ignore zombie callbacks */
const activeChannels = new Map<string, RealtimeChannel>()

/** Debounce delay in milliseconds */
const PULL_DEBOUNCE_MS = 500

/** Max retry attempts for subscription */
const MAX_SUBSCRIPTION_RETRIES = 5

/** Base delay for retry (exponential backoff) */
const RETRY_BASE_DELAY_MS = 2000

/**
 * Reconnection context - stores references needed for visibility-based reconnection.
 * All backends share this context since they use the same stores.
 */
interface ReconnectionContext {
  syncStates: BackendSyncState
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>
  syncEngineStore: ReturnType<typeof useSyncEngineStore>
  currentVaultId: string | undefined
  visibilityHandler: (() => void) | null
  reconnectionPending: boolean
}

const reconnectionContext: ReconnectionContext = {
  syncStates: {},
  syncBackendsStore: null as unknown as ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: null as unknown as ReturnType<typeof useSyncEngineStore>,
  currentVaultId: undefined,
  visibilityHandler: null,
  reconnectionPending: false,
}

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
  log.info('REALTIME: Change event received, payload:', JSON.stringify(payload))

  // Skip if this change was made by our own device
  // Supabase Realtime returns snake_case column names
  const deviceStore = useDeviceStore()
  const newRecord = payload.new as Record<string, unknown> | undefined
  const deviceId = newRecord?.device_id as string | undefined
  log.info(`REALTIME: Our deviceId=${deviceStore.deviceId}, event deviceId=${deviceId}`)
  if (deviceId === deviceStore.deviceId) {
    log.info('REALTIME: Skipping - change originated from this device')
    return
  }
  log.info('REALTIME: Processing change from another device, triggering pull...')

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
 * Schedules a retry for a failed/closed channel with deduplication.
 * Clears any existing retry timer before scheduling a new one to prevent
 * multiple parallel retry attempts from cascading CLOSED events.
 */
const scheduleRetry = (
  backendId: string,
  channel: RealtimeChannel,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
  client: SupabaseClient,
  status: string,
): void => {
  const retryCount = subscriptionRetryCounts.get(backendId) ?? 0
  if (retryCount >= MAX_SUBSCRIPTION_RETRIES) {
    log.warn(`SUBSCRIBE: Realtime reconnection failed for backend ${backendId} after ${MAX_SUBSCRIPTION_RETRIES} attempts. Periodic pull will be used as fallback.`)
    return
  }

  const delay = RETRY_BASE_DELAY_MS * Math.pow(2, retryCount)
  log.warn(`SUBSCRIBE: ${status} for ${backendId}. Retrying in ${delay}ms (attempt ${retryCount + 1}/${MAX_SUBSCRIPTION_RETRIES})`)

  // Clear existing retry timer to prevent duplicate retries
  const existingTimer = subscriptionRetryTimers.get(backendId)
  if (existingTimer) {
    clearTimeout(existingTimer)
    subscriptionRetryTimers.delete(backendId)
  }

  syncStates[backendId]!.subscription = null

  const retryTimer = setTimeout(async () => {
    subscriptionRetryTimers.delete(backendId)
    subscriptionRetryCounts.set(backendId, retryCount + 1)
    try {
      await client.removeChannel(channel).catch(() => {})
      await subscribeToBackendAsync(backendId, currentVaultId, syncStates, syncBackendsStore, syncEngineStore)
    } catch (retryError) {
      log.error(`SUBSCRIBE: Retry failed for backend ${backendId}:`, retryError)
    }
  }, delay)
  subscriptionRetryTimers.set(backendId, retryTimer)
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
  log.info(`SUBSCRIBE: Platform info - userAgent: ${navigator.userAgent.substring(0, 100)}`)

  // Update reconnection context for visibility-based reconnection
  updateReconnectionContext(currentVaultId, syncStates, syncBackendsStore, syncEngineStore)

  if (!currentVaultId) {
    log.error('SUBSCRIBE: No vault opened')
    throw new Error('No vault opened')
  }

  const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
  if (!backend?.spaceId) {
    log.error('SUBSCRIBE: Backend spaceId not configured')
    throw new Error('Backend spaceId not configured')
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

  // Prevent concurrent subscription attempts (race condition guard)
  if (subscriptionInProgress.has(backendId)) {
    log.debug(`SUBSCRIBE: Subscription already in progress for ${backendId}, skipping`)
    return
  }
  subscriptionInProgress.add(backendId)

  const client = syncEngineStore.supabaseClient
  if (!client) {
    log.error('SUBSCRIBE: Supabase client not initialized')
    throw new Error('Supabase client not initialized')
  }

  try {
    // Ensure auth token is set for realtime connection
    const token = await syncEngineStore.getAuthTokenAsync()
    if (token) {
      // Decode JWT to check expiration (for debugging)
      try {
        const tokenParts = token.split('.')
        const payloadPart = tokenParts[1]
        if (!payloadPart) throw new Error('Invalid token format')
        const payload = JSON.parse(atob(payloadPart))
        const exp = payload.exp ? new Date(payload.exp * 1000) : null
        const now = new Date()
        const expiresIn = exp ? Math.round((exp.getTime() - now.getTime()) / 1000) : 'unknown'
        log.info(`SUBSCRIBE: Auth token present, expires in ${expiresIn}s (at ${exp?.toISOString() ?? 'unknown'})`)
        log.info(`SUBSCRIBE: Token sub=${payload.sub}, role=${payload.role}, aud=${payload.aud}`)
      } catch (decodeErr) {
        log.warn('SUBSCRIBE: Could not decode token for debugging:', decodeErr)
      }
    } else {
      log.error('SUBSCRIBE: No auth token available — auth session is invalid. User needs to re-login. Skipping subscription.')
      state.error = 'Auth session expired. Please re-login to the sync backend.'
      return
    }

    // Note: We intentionally skip client.auth.getSession() here.
    // With persistSession: false, GoTrue's in-memory session can be lost
    // (especially on Android WebView), causing getSession() to return null
    // even though we have a valid token. This triggered redundant DID re-auth
    // attempts that failed on the 30s cooldown.
    // Instead, we rely on setAuth(token) below which directly sets the token
    // on the Realtime WebSocket — the actual auth mechanism used by Supabase.

    // Log realtime connection state
    log.info(`SUBSCRIBE: Realtime connection state: ${client.realtime.connectionState()}`)
    log.info(`SUBSCRIBE: Realtime channels count: ${client.realtime.channels.length}`)

    // If there's an existing dead connection, clean up channels and explicitly
    // reconnect. We must NOT call disconnect() here — on Android WebView it puts
    // the Realtime client into a state that prevents automatic reconnection.
    // Instead, remove stale channels and call connect() to re-establish the WebSocket.
    const connState = client.realtime.connectionState()
    if (connState === 'closed' || connState === 'disconnected') {
      log.info(`SUBSCRIBE: Realtime connection is ${connState}, cleaning up and reconnecting`)
      client.realtime.removeAllChannels()
      client.realtime.connect()
      // Give the WebSocket time to establish before subscribing
      await new Promise((resolve) => setTimeout(resolve, 1000))
      log.info(`SUBSCRIBE: After reconnect attempt, state: ${client.realtime.connectionState()}`)
    }

    // Subscribe via Realtime Broadcast instead of postgres_changes.
    // A DB trigger on sync_changes calls realtime.broadcast_changes() which
    // writes to realtime.messages — always in the publication, no cache issues.
    // The trigger is on the parent table and PostgreSQL 15 auto-clones it to
    // all partitions. New partitions work immediately without Realtime restart.
    const channelName = `sync:${backend.spaceId}`

    // Remove ALL existing channels with this topic to prevent zombie accumulation
    const existingChannels = client.realtime.channels.filter(
      (ch) => ch.topic === `realtime:${channelName}`,
    )
    if (existingChannels.length > 0) {
      log.info(`SUBSCRIBE: Removing ${existingChannels.length} existing channel(s) for ${channelName}`)
      for (const ch of existingChannels) {
        await client.removeChannel(ch).catch(() => {})
      }
    }

    log.info(`SUBSCRIBE: Subscribing to broadcast channel="${channelName}" backendId=${backendId}`)

    // Set auth token for Realtime Authorization (broadcast uses private channels)
    await client.realtime.setAuth(token)

    const channel = client
      .channel(channelName, { config: { private: true } })
      .on(
        'broadcast',
        { event: 'INSERT' },
        (payload) => {
          log.info(`REALTIME: Broadcast INSERT received on ${channelName}`)
          handleRealtimeChangeAsync(
            backendId,
            payload.payload as any,
            currentVaultId,
            syncStates,
            syncBackendsStore,
            syncEngineStore,
          ).catch(log.error)
        },
      )
      .on(
        'broadcast',
        { event: 'UPDATE' },
        (payload) => {
          log.info(`REALTIME: Broadcast UPDATE received on ${channelName}`)
          handleRealtimeChangeAsync(
            backendId,
            payload.payload as any,
            currentVaultId,
            syncStates,
            syncBackendsStore,
            syncEngineStore,
          ).catch(log.error)
        },
      )
      .subscribe((status, err) => {
        // Ignore callbacks from zombie channels (replaced by a newer subscription)
        if (activeChannels.get(backendId) !== channel) {
          log.debug(`SUBSCRIBE: Ignoring ${status} from zombie channel for ${backendId}`)
          client.removeChannel(channel).catch(() => {})
          return
        }

        log.info(`SUBSCRIBE: Channel status changed to ${status}`, err ? `Error: ${JSON.stringify(err)}` : '')
        if (status === 'SUBSCRIBED') {
          state.isConnected = true
          subscriptionRetryCounts.set(backendId, 0)
          log.info(`SUBSCRIBE: Successfully subscribed to backend ${backendId}`)
        } else if (status === 'CHANNEL_ERROR' || status === 'TIMED_OUT') {
          state.isConnected = false
          const errorDetails = err ? JSON.stringify(err) : 'unknown'
          state.error = `Subscription error: ${status} - ${errorDetails}`
          log.error(`SUBSCRIBE: ${status} details - errorDetails=${errorDetails}`)
          log.error(`SUBSCRIBE: Realtime connection state at error: ${client.realtime.connectionState()}`)

          scheduleRetry(backendId, channel, currentVaultId, syncStates, syncBackendsStore, syncEngineStore, client, status)
        } else if (status === 'CLOSED') {
          state.isConnected = false
          state.subscription = null
          log.warn(`SUBSCRIBE: Channel closed for backend ${backendId}, will attempt to reconnect`)

          scheduleRetry(backendId, channel, currentVaultId, syncStates, syncBackendsStore, syncEngineStore, client, status)
        }
      })

    activeChannels.set(backendId, channel)
    state.subscription = channel
  } catch (error) {
    log.error(`SUBSCRIBE: Failed to subscribe to backend ${backendId}:`, error)
    state.error = error instanceof Error ? error.message : 'Unknown error'
    throw error
  } finally {
    subscriptionInProgress.delete(backendId)
  }
}

/**
 * Unsubscribes from realtime changes
 */
export const unsubscribeFromBackendAsync = async (
  backendId: string,
  syncStates: BackendSyncState,
): Promise<void> => {
  // Clear any pending retry timer
  const retryTimer = subscriptionRetryTimers.get(backendId)
  if (retryTimer) {
    clearTimeout(retryTimer)
    subscriptionRetryTimers.delete(backendId)
  }
  subscriptionRetryCounts.delete(backendId)
  activeChannels.delete(backendId)

  // Clear any pending debounce timer
  const timer = pullDebounceTimers.get(backendId)
  if (timer) {
    clearTimeout(timer)
    pullDebounceTimers.delete(backendId)
  }

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

/**
 * Reconnects all backends after app resumes from background.
 * Resets retry counts and attempts fresh connections.
 */
const reconnectAllBackendsAsync = async (): Promise<void> => {
  const { syncStates, syncBackendsStore, syncEngineStore, currentVaultId } = reconnectionContext

  if (!syncBackendsStore || !syncEngineStore) {
    log.warn('RECONNECT: Context not initialized, skipping')
    return
  }

  if (reconnectionContext.reconnectionPending) {
    log.debug('RECONNECT: Already in progress, skipping')
    return
  }

  reconnectionContext.reconnectionPending = true
  log.info('RECONNECT: App resumed, reconnecting all backends...')

  try {
    const enabledBackends = syncBackendsStore.enabledBackends

    for (const backend of enabledBackends) {
      const state = syncStates[backend.id]
      if (!state) continue

      // Reset retry count and guards for fresh start
      subscriptionRetryCounts.delete(backend.id)
      subscriptionInProgress.delete(backend.id)

      // Clear any pending retry timers
      const retryTimer = subscriptionRetryTimers.get(backend.id)
      if (retryTimer) {
        clearTimeout(retryTimer)
        subscriptionRetryTimers.delete(backend.id)
      }

      // Clean up existing (likely dead) channel — use removeChannel instead of
      // unsubscribe() which can hang on dead WebSocket connections (Android)
      if (state.subscription) {
        activeChannels.delete(backend.id)
        await syncEngineStore.supabaseClient?.removeChannel(state.subscription).catch(() => {})
        state.subscription = null
      }

      // Re-subscribe
      try {
        log.info(`RECONNECT: Re-subscribing to backend ${backend.id}`)
        await subscribeToBackendAsync(
          backend.id,
          currentVaultId,
          syncStates,
          syncBackendsStore,
          syncEngineStore,
        )
      } catch (error) {
        log.error(`RECONNECT: Failed to re-subscribe to ${backend.id}:`, error)
      }
    }

    // Also trigger a pull to catch any changes missed while in background
    log.info('RECONNECT: Triggering pull for all backends to catch missed changes')
    for (const backend of enabledBackends) {
      try {
        await pullFromBackendAsync(
          backend.id,
          currentVaultId,
          syncStates,
          syncBackendsStore,
          syncEngineStore,
        )
      } catch (error) {
        log.error(`RECONNECT: Pull failed for ${backend.id}:`, error)
      }
    }
  } finally {
    reconnectionContext.reconnectionPending = false
  }
}

/**
 * Sets up visibility change listener for mobile reconnection.
 * Should be called once when sync starts.
 */
export const setupVisibilityListener = (): void => {
  if (reconnectionContext.visibilityHandler) {
    log.debug('VISIBILITY: Handler already registered')
    return
  }

  reconnectionContext.visibilityHandler = () => {
    if (document.visibilityState === 'visible') {
      log.info('VISIBILITY: App became visible, scheduling reconnection')
      // Small delay to let the app fully resume
      setTimeout(() => {
        reconnectAllBackendsAsync().catch((error) => {
          log.error('VISIBILITY: Reconnection failed:', error)
        })
      }, 500)
    } else {
      log.debug('VISIBILITY: App went to background')
    }
  }

  document.addEventListener('visibilitychange', reconnectionContext.visibilityHandler)
  log.info('VISIBILITY: Listener registered for mobile reconnection')
}

/**
 * Removes visibility change listener.
 * Should be called when sync stops.
 */
export const removeVisibilityListener = (): void => {
  if (reconnectionContext.visibilityHandler) {
    document.removeEventListener('visibilitychange', reconnectionContext.visibilityHandler)
    reconnectionContext.visibilityHandler = null
    log.debug('VISIBILITY: Listener removed')
  }
}

/**
 * Updates the reconnection context with current store references.
 * Called internally when subscribing to backends.
 */
const updateReconnectionContext = (
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): void => {
  reconnectionContext.currentVaultId = currentVaultId
  reconnectionContext.syncStates = syncStates
  reconnectionContext.syncBackendsStore = syncBackendsStore
  reconnectionContext.syncEngineStore = syncEngineStore
}

// ============================================================================
// Test Exports - Internal functions exported for testing
// ============================================================================

/**
 * @internal - Exported for testing only
 * Returns the current reconnection context for assertions
 */
export const _getReconnectionContext = (): ReconnectionContext => reconnectionContext

/**
 * @internal - Exported for testing only
 * Resets reconnection context to initial state
 */
export const _resetReconnectionContext = (): void => {
  reconnectionContext.syncStates = {}
  reconnectionContext.syncBackendsStore = null as unknown as ReturnType<typeof useSyncBackendsStore>
  reconnectionContext.syncEngineStore = null as unknown as ReturnType<typeof useSyncEngineStore>
  reconnectionContext.currentVaultId = undefined
  reconnectionContext.visibilityHandler = null
  reconnectionContext.reconnectionPending = false
}

/**
 * @internal - Exported for testing only
 * Triggers reconnection (normally called by visibility handler)
 */
export const _reconnectAllBackendsAsync = reconnectAllBackendsAsync
