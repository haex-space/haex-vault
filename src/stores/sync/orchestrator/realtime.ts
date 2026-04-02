/**
 * Sync Realtime Operations
 * Handles realtime subscriptions via WebSocket (useRealtime composable)
 *
 * WebSocket events are used as triggers to initiate a pull from the server.
 * This ensures data consistency by always using the pull endpoint which
 * guarantees complete data delivery.
 *
 * Android/Mobile considerations:
 * - WebSocket connections are killed when app goes to background (Doze mode)
 * - We listen for visibility changes to reconnect when app resumes
 * - The useRealtime composable handles reconnection with exponential backoff
 */

import { invoke } from '@tauri-apps/api/core'
import { useRealtime, type RealtimeEvent } from '@/composables/useRealtime'
import { useMlsDelivery, type MlsMessage } from '@/composables/useMlsDelivery'
import { orchestratorLog as log, type BackendSyncState } from './types'
import { pullFromBackendAsync } from './pull'

/** Debounce timers per backend */
const pullDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>()

/** Debounce delay in milliseconds */
const PULL_DEBOUNCE_MS = 500

/** Singleton realtime instance (created outside component lifecycle) */
let realtimeInstance: ReturnType<typeof useRealtime> | null = null

/** Cleanup functions for event handlers */
const eventCleanups: Array<() => void> = []

/**
 * Gets or creates the singleton realtime instance.
 * useRealtime() uses onUnmounted which is a no-op outside components,
 * so we manage the lifecycle manually via startRealtimeAsync/stopRealtimeAsync.
 */
const getRealtimeInstance = () => {
  if (!realtimeInstance) {
    realtimeInstance = useRealtime()
  }
  return realtimeInstance
}

/**
 * Reconnection context - stores references needed for visibility-based reconnection.
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
 * Finds the backend ID for a given spaceId
 */
const findBackendBySpaceId = (
  spaceId: string,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
): string | null => {
  const backend = syncBackendsStore.enabledBackends.find((b) => b.spaceId === spaceId)
  return backend?.id ?? null
}

/**
 * Starts the WebSocket realtime connection and registers event handlers.
 * Replaces the per-backend Supabase channel subscriptions with a single
 * WebSocket connection that handles all events.
 */
export const subscribeToBackendAsync = async (
  backendId: string,
  currentVaultId: string | undefined,
  syncStates: BackendSyncState,
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>,
  syncEngineStore: ReturnType<typeof useSyncEngineStore>,
): Promise<void> => {
  log.info(`SUBSCRIBE: Setting up WebSocket realtime for backend ${backendId}`)

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

  if (state.isConnected) {
    log.debug(`SUBSCRIBE: Already connected for backend ${backendId}`)
    return
  }

  // Resolve identity for DID-Auth WebSocket connection
  const identityStore = useIdentityStore()
  const identity = await identityStore.getIdentityByIdAsync(backend.identityId)
  if (!identity?.privateKey) {
    throw new Error(`Identity not found or incomplete for backend ${backendId}`)
  }

  const realtime = getRealtimeInstance()

  // Only connect if not already connected (multiple backends share one WS)
  if (!realtime.connected.value) {
    log.info(`SUBSCRIBE: Connecting WebSocket to ${backend.homeServerUrl}`)
    await realtime.connect(backend.homeServerUrl, identity.privateKey, identity.did)
  }

  // Register event handlers for this backend
  const cleanupSync = realtime.on('sync', (event: RealtimeEvent) => {
    const targetBackendId = findBackendBySpaceId(event.spaceId, syncBackendsStore)
    if (!targetBackendId) return

    log.info(`REALTIME: sync event for space ${event.spaceId}, triggering pull`)
    triggerDebouncedPullAsync(
      targetBackendId,
      reconnectionContext.currentVaultId,
      reconnectionContext.syncStates,
      syncBackendsStore,
      syncEngineStore,
    )
  })

  const cleanupMembership = realtime.on('membership', async (event: RealtimeEvent) => {
    log.info(`REALTIME: membership event for space ${event.spaceId}`)
    const targetBackendId = findBackendBySpaceId(event.spaceId, syncBackendsStore)
    if (!targetBackendId) return

    const backend = syncBackendsStore.enabledBackends.find((b) => b.spaceId === event.spaceId)
    if (!backend?.identityId) return

    // Auto-finalize accepted invites (admin adds new member to MLS group)
    try {
      const identityStore = useIdentityStore()
      const identity = await identityStore.getIdentityByIdAsync(backend.identityId)
      if (identity) {
        const { getUcanForSpaceAsync } = await import('@/utils/auth/ucanStore')
        const ucan = getUcanForSpaceAsync(event.spaceId)
        if (ucan) {
          const { fetchWithUcanAuth } = await import('@/utils/auth/ucanStore')
          const response = await fetchWithUcanAuth(
            `${backend.homeServerUrl}/spaces/${event.spaceId}/invites`,
            event.spaceId,
            ucan,
          )
          if (response.ok) {
            const data = await response.json()
            const accepted = (data.invites ?? []).filter((i: any) => i.status === 'accepted')
            const spacesStore = useSpacesStore()
            for (const invite of accepted) {
              try {
                // For token invites without UCAN: pass inviteId + capability so finalize creates one
                const needsUcan = !invite.ucan
                await spacesStore.finalizeInviteAsync(
                  backend.homeServerUrl,
                  event.spaceId,
                  invite.inviteeDid,
                  backend.identityId,
                  needsUcan ? invite.id : undefined,
                  needsUcan ? (invite.capability ?? 'space/write') : undefined,
                )
                log.info(`Auto-finalized invite for ${invite.inviteeDid} in space ${event.spaceId}`)
              } catch (err) {
                log.error(`Failed to auto-finalize invite for ${invite.inviteeDid}:`, err)
              }
            }
          }
        }
      }
    } catch (error) {
      log.error(`Failed to auto-finalize invites for space ${event.spaceId}:`, error)
    }

    // Refresh memberships by triggering a pull
    triggerDebouncedPullAsync(
      targetBackendId,
      reconnectionContext.currentVaultId,
      reconnectionContext.syncStates,
      syncBackendsStore,
      syncEngineStore,
    )
  })

  const cleanupInvite = realtime.on('invite', (event: RealtimeEvent) => {
    log.info(`REALTIME: invite event for space ${event.spaceId}, inviteId=${event.inviteId}`)
    // Pending invites are fetched from server on next spaces load
  })

  const cleanupMls = realtime.on('mls', async (event: RealtimeEvent) => {
    log.info(`REALTIME: mls event for space ${event.spaceId}`)

    const backend = syncBackendsStore.enabledBackends.find((b) => b.spaceId === event.spaceId)
    if (!backend?.identityId) return

    try {
      const identityStore = useIdentityStore()
      const identity = await identityStore.getIdentityByIdAsync(backend.identityId)
      if (!identity?.privateKey) return

      const delivery = useMlsDelivery(backend.homeServerUrl, event.spaceId, {
        privateKey: identity.privateKey,
        did: identity.did,
      })

      // Fetch and process welcome messages (new member joining)
      const welcomes = await delivery.fetchWelcomesAsync()
      for (const welcome of welcomes) {
        await invoke('mls_process_message', { spaceId: event.spaceId, message: Array.from(welcome) })
      }
      if (welcomes.length > 0) {
        log.info(`Processed ${welcomes.length} MLS welcome(s) for space ${event.spaceId}`)
      }

      // Fetch and process MLS messages (commits, application data)
      const messages = await delivery.fetchMessagesAsync()
      for (const msg of messages) {
        const payload = Uint8Array.from(atob(msg.payload), (c) => c.charCodeAt(0))
        await invoke('mls_process_message', { spaceId: event.spaceId, message: Array.from(payload) })
      }
      if (messages.length > 0) {
        log.info(`Processed ${messages.length} MLS message(s) for space ${event.spaceId}`)
      }
    } catch (error) {
      log.error(`Failed to process MLS event for space ${event.spaceId}:`, error)
    }
  })

  eventCleanups.push(cleanupSync, cleanupMembership, cleanupInvite, cleanupMls)

  // Mark as connected (the WS composable handles reconnection internally)
  state.isConnected = realtime.connected.value
  // Use a watcher-like approach: poll connected state briefly
  // The composable's connected ref will update on open/close
  if (!state.isConnected) {
    // Wait briefly for connection to establish
    await new Promise<void>((resolve) => {
      const check = setInterval(() => {
        if (realtime.connected.value) {
          state.isConnected = true
          clearInterval(check)
          resolve()
        }
      }, 100)
      // Timeout after 10s
      setTimeout(() => {
        clearInterval(check)
        resolve()
      }, 10_000)
    })
  }

  log.info(`SUBSCRIBE: WebSocket connected=${realtime.connected.value} for backend ${backendId}`)
}

/**
 * Unsubscribes from realtime changes for a backend
 */
export const unsubscribeFromBackendAsync = async (
  backendId: string,
  syncStates: BackendSyncState,
): Promise<void> => {
  // Clear any pending debounce timer
  const timer = pullDebounceTimers.get(backendId)
  if (timer) {
    clearTimeout(timer)
    pullDebounceTimers.delete(backendId)
  }

  const state = syncStates[backendId]
  if (!state) {
    return
  }

  state.isConnected = false
  log.info(`UNSUBSCRIBE: Unsubscribed from backend ${backendId}`)
}

/**
 * Disconnects the shared WebSocket and cleans up all event handlers.
 * Called when sync is fully stopped (all backends).
 */
export const disconnectRealtimeAsync = async (): Promise<void> => {
  // Clean up all event handlers
  for (const cleanup of eventCleanups) {
    cleanup()
  }
  eventCleanups.length = 0

  // Disconnect the WebSocket
  if (realtimeInstance) {
    realtimeInstance.disconnect()
    realtimeInstance = null
  }

  // Clear all debounce timers
  for (const [, timer] of pullDebounceTimers) {
    clearTimeout(timer)
  }
  pullDebounceTimers.clear()

  log.info('REALTIME: WebSocket disconnected and all handlers cleaned up')
}

/**
 * Reconnects all backends after app resumes from background.
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
  log.info('RECONNECT: App resumed, reconnecting...')

  try {
    const enabledBackends = syncBackendsStore.enabledBackends

    // The useRealtime composable handles reconnection automatically on close.
    // But after a long background period the WS may be dead. Force reconnect.
    if (realtimeInstance && !realtimeInstance.connected.value) {
      // Disconnect cleanly and reconnect
      realtimeInstance.disconnect()
      realtimeInstance = null

      // Re-subscribe (will create new instance and connect)
      for (const backend of enabledBackends) {
        const state = syncStates[backend.id]
        if (!state) continue
        state.isConnected = false

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
    }

    // Trigger a pull to catch any changes missed while in background
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
