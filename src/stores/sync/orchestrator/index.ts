/**
 * Sync Orchestrator Store - Orchestrates sync operations across all backends
 * Uses new table-scanning approach with column-level HLC timestamps
 */

import { useTimeoutPoll } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { RustEventGroup, RUST_EVENTS, type LocalSyncCompletedEvent } from '@/lib/rust-events'
import type { PeerConnectedEvent } from '@bindings/PeerConnectedEvent'
import { orchestratorLog as log, type BackendSyncState } from './types'
import { pushToBackendAsync, pushAllDataToBackendAsync } from './push'
import { pullFromBackendAsync } from './pull'
import {
  subscribeToBackendAsync,
  unsubscribeFromBackendAsync,
  disconnectRealtimeAsync,
  setupVisibilityListener,
  removeVisibilityListener,
  _resetReconnectionContext,
} from './realtime'
import { initSyncEventsAsync, stopSyncEvents, SYNC_TABLES_INTERNAL_EVENT } from '../syncEvents'
import { registerStoreReloadCallbacks } from './observer-wiring'
import { createScheduler } from './scheduler'
import { performInitialPullAsync as performInitialPullImplAsync } from './initial-sync'

// Re-export types
export * from './types'

export const useSyncOrchestratorStore = defineStore(
  'syncOrchestratorStore',
  () => {
    const { currentVaultId } = storeToRefs(useVaultStore())
    const syncBackendsStore = useSyncBackendsStore()
    const syncEngineStore = useSyncEngineStore()
    const syncConfigStore = useSyncConfigStore()
    const { add: addToast } = useToast()

    // Sync state per backend
    const syncStates = ref<BackendSyncState>({})

    // Per-store lifecycle handles (per-backend periodic polls, outbox poll,
    // local-event listeners). The dirty-tables watcher + adaptive debounce
    // live inside the scheduler factory below.
    let outboxProcessorPoll: ReturnType<typeof useTimeoutPoll> | null = null
    const periodicPullPolls: Map<string, ReturnType<typeof useTimeoutPoll>> = new Map()
    let localEvents: RustEventGroup | null = null


    /**
     * Wrapper for pushing to a backend
     */
    const pushToBackendWrapperAsync = async (backendId: string): Promise<void> => {
      return pushToBackendAsync(
        backendId,
        currentVaultId.value,
        syncStates.value,
        syncBackendsStore,
        syncEngineStore,
      )
    }

    /**
     * Wrapper for pulling from a backend
     */
    const pullFromBackendWrapperAsync = async (backendId: string): Promise<void> => {
      return pullFromBackendAsync(
        backendId,
        currentVaultId.value,
        syncStates.value,
        syncBackendsStore,
        syncEngineStore,
      )
    }

    /**
     * Wrapper for subscribing to a backend
     */
    const subscribeToBackendWrapperAsync = async (backendId: string): Promise<void> => {
      return subscribeToBackendAsync(
        backendId,
        currentVaultId.value,
        syncStates.value,
        syncBackendsStore,
        syncEngineStore,
      )
    }

    /**
     * Wrapper for unsubscribing from a backend
     */
    const unsubscribeFromBackendWrapperAsync = async (backendId: string): Promise<void> => {
      return unsubscribeFromBackendAsync(backendId, syncStates.value)
    }

    /**
     * Called after local write operations to push changes
     */
    const onLocalWriteAsync = async (): Promise<void> => {
      const callId = Math.random().toString(36).substring(7)
      log.info(`[PUSH:${callId}] onLocalWriteAsync TRIGGERED at ${new Date().toISOString()}`)

      // Don't push until initial sync is complete - all changes are from pulled data
      const vaultSettingsStore = useVaultSettingsStore()
      log.info(`[PUSH:${callId}] Querying DB for initial_sync_complete...`)
      const isInitialSyncComplete = await vaultSettingsStore.isInitialSyncCompleteAsync()
      log.info(`[PUSH:${callId}] isInitialSyncComplete = ${isInitialSyncComplete}`)

      if (!isInitialSyncComplete) {
        log.info(`[PUSH:${callId}] BLOCKED - initial sync not complete, returning early`)
        return
      }

      try {
        // Push to all enabled backends in parallel
        const enabledBackends = syncBackendsStore.enabledBackends
        log.info(`[PUSH:${callId}] EXECUTING push to ${enabledBackends.length} backends: ${enabledBackends.map(b => b.id).join(', ')}`)

        const results = await Promise.allSettled(
          enabledBackends.map((backend) => pushToBackendWrapperAsync(backend.id)),
        )

        const fulfilled = results.filter(r => r.status === 'fulfilled').length
        const rejected = results.filter(r => r.status === 'rejected').length
        log.info(`[PUSH:${callId}] Push complete - fulfilled: ${fulfilled}, rejected: ${rejected}`)
      } catch (error) {
        log.error(`[PUSH:${callId}] Failed to push local changes:`, error)
      }
    }

    // Scheduler owns the dirty-tables debounce timer + fallback pull poll.
    // Constructed once per setup() call so multi-vault instantiations stay isolated.
    const scheduler = createScheduler({
      syncConfigStore,
      syncBackendsStore,
      onLocalWriteAsync,
      pullFromBackendAsync: pullFromBackendWrapperAsync,
    })

    /**
     * Initializes sync for a backend
     */
    const initBackendAsync = async (backendId: string): Promise<void> => {
      log.info(`========== INIT BACKEND START (${backendId}) ==========`)

      // Check if state already exists (from performInitialPullAsync)
      const existingState = syncStates.value[backendId]
      const skipInitialSync = !!existingState

      if (skipInitialSync) {
        log.info(`INIT: Backend ${backendId} already has state from initial pull, skipping pull/push steps`)
      }

      // Initialize state if not exists
      if (!existingState) {
        syncStates.value[backendId] = {
          isConnected: false,
          isSyncing: false,
          error: null,
        }
        log.debug('INIT: State initialized')
      }

      try {
        // Ensure token manager is initialized for this backend
        if (!syncEngineStore.isTokenManagerInitialized || syncEngineStore.currentBackendId !== backendId) {
          log.info('INIT: Initializing token manager...')
          syncEngineStore.initTokenManagerAsync(backendId)
        } else {
          log.debug('INIT: Token manager already initialized, ensuring reauth resolver is registered')
          syncEngineStore.registerReauthResolver(backendId)
        }

        // Ensure vault key exists before any pull/push
        const vaultStore = useVaultStore()
        const { currentVaultPassword } = storeToRefs(vaultStore)
        const vaultName = vaultStore.currentVault?.name ?? 'Unknown'
        const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
        if (currentVaultId.value && currentVaultPassword.value && backend) {
          log.info('INIT: Ensuring sync key exists...')
          await syncEngineStore.ensureSyncKeyAsync(
            backendId,
            currentVaultId.value,
            vaultName,
            currentVaultPassword.value,
            backend.homeServerUrl,
          )
        }

        // Only do initial pull/push if this is a fresh init (not from performInitialPullAsync)
        if (!skipInitialSync) {
          // Initial pull to get all existing data from server
          log.info('INIT: Step 1 - Initial pull from server')
          try {
            await pullFromBackendWrapperAsync(backendId)
          } catch (pullError) {
            log.error(`INIT: Initial pull failed:`, pullError)
            addToast({
              color: 'error',
              description: `Sync pull failed: ${pullError instanceof Error ? pullError.message : 'Unknown error'}`,
            })
            throw pullError
          }

          // Push any pending local changes (dirty tables)
          log.info('INIT: Step 2 - Push pending local changes')
          try {
            await pushToBackendWrapperAsync(backendId)
          } catch (pushError) {
            log.error(`INIT: Push failed:`, pushError)
            addToast({
              color: 'error',
              description: `Sync push failed: ${pushError instanceof Error ? pushError.message : 'Unknown error'}`,
            })
            throw pushError
          }
        }

        // Always subscribe to realtime changes (even if initial pull was already done)
        // Skip if already connected via WebSocket
        if (!syncStates.value[backendId]?.isConnected) {
          log.info('INIT: Step 3 - Subscribe to realtime changes via WebSocket')
          await subscribeToBackendWrapperAsync(backendId)
        } else {
          log.info('INIT: Step 3 - Skipping realtime (already connected)')
        }

        // Always start periodic pull as fallback (even if initial pull was already done)
        // Skip if already running
        if (!periodicPullPolls.has(backendId)) {
          log.info('INIT: Step 4 - Setting up periodic pull (every 5 min)')
          const poll = useTimeoutPoll(async () => {
            try {
              log.info(`PERIODIC: Pull triggered for backend ${backendId} at ${new Date().toISOString()}`)
              await pullFromBackendWrapperAsync(backendId)
            } catch (error) {
              log.error(`PERIODIC: Pull failed for backend ${backendId}:`, error)
            }
          }, 5 * 60 * 1000)
          periodicPullPolls.set(backendId, poll)
        } else {
          log.info('INIT: Step 4 - Skipping periodic pull (already running)')
        }

        log.info(`========== INIT BACKEND SUCCESS (${backendId}) ==========`)
      } catch (error) {
        log.error(`========== INIT BACKEND FAILED (${backendId}) ==========`, error)
        throw error
      }
    }

    /**
     * Starts sync for all enabled backends
     */
    const startSyncAsync = async (): Promise<void> => {
      log.info('[START-SYNC] ========================================')
      log.info('[START-SYNC] startSyncAsync CALLED at ' + new Date().toISOString())
      log.info('[START-SYNC] ========================================')

      // Initialize sync events listener (for frontend refresh after pull and local sync)
      // This must happen before the backends check so local-only vaults also get store reloads
      log.debug('START: Initializing sync events listener...')
      await initSyncEventsAsync()

      // Register all stores for their respective tables
      // This is the central place where we define which stores reload on which table updates
      log.debug('START: Registering stores for sync events...')
      registerStoreReloadCallbacks()

      // Listen for local sync completions from Rust sync loop
      if (!localEvents) {
        const events = new RustEventGroup()
        try {
          await events.on<LocalSyncCompletedEvent>(RUST_EVENTS.localSyncCompleted, async ({ spaceId, tables }) => {
            log.info(`[LOCAL-SYNC] Received local-sync-completed for space ${spaceId}, tables: ${tables.join(', ')}`)
            if (tables && tables.length > 0) {
              await emit(SYNC_TABLES_INTERNAL_EVENT, { tables })
            }
          })

          // Connection-event-driven outbox flush: when Rust signals a peer's
          // DID-auth handshake just completed (accept-side or connect-side),
          // skip backoff for outbox rows targeting that endpoint and try
          // right away. Debounced per endpoint to absorb reconnect-flapping
          // bursts. The Map lives for the lifetime of this listener — when
          // events.dispose() runs on stop, pending timeouts are no-ops at
          // worst (the dynamic-imported composable just runs a SQL query).
          const endpointFlushDebounce = new Map<string, ReturnType<typeof setTimeout>>()
          await events.on<PeerConnectedEvent>(RUST_EVENTS.peerConnected, ({ endpointId }) => {
            if (!endpointId) return
            const existing = endpointFlushDebounce.get(endpointId)
            if (existing) clearTimeout(existing)
            endpointFlushDebounce.set(endpointId, setTimeout(async () => {
              endpointFlushDebounce.delete(endpointId)
              try {
                const { useInviteOutbox } = await import('@/composables/useInviteOutbox')
                const { processOutboxAsync } = useInviteOutbox()
                await processOutboxAsync({ filterTargetEndpointId: endpointId })
              } catch (error) {
                log.warn(`[OUTBOX-FLUSH] peer-connected flush failed for ${endpointId.slice(0, 12)}…: ${error}`)
              }
            }, 1_000))
          })

          localEvents = events
        } catch (err) {
          events.dispose()
          throw err
        }
        log.info('[START-SYNC] Local sync + peer-connected listeners registered')
      }

      // Always start the invite outbox processor (works for local-only vaults too)
      if (!outboxProcessorPoll) {
        outboxProcessorPoll = useTimeoutPoll(async () => {
          try {
            const { useInviteOutbox } = await import('@/composables/useInviteOutbox')
            const { processOutboxAsync, cleanupOldInvitesAsync } = useInviteOutbox()
            await processOutboxAsync()
            await cleanupOldInvitesAsync()
          } catch (error) {
            log.error('[WATCHER] Outbox processing failed:', error)
          }
        }, 30_000, { immediate: true })
        outboxProcessorPoll.resume()
        log.info('[START-SYNC] Invite outbox processor started')
      }

      const enabledBackends = syncBackendsStore.enabledBackends

      if (enabledBackends.length === 0) {
        log.info('[START-SYNC] No enabled backends to sync with')
        return
      }

      log.info(
        `[START-SYNC] Found ${enabledBackends.length} enabled backends:`,
        enabledBackends.map((b) => ({ id: b.id, name: b.name })),
      )

      // Load sync configuration
      log.info('[START-SYNC] Loading sync configuration...')
      await syncConfigStore.loadConfigAsync()

      // Start dirty tables watcher
      log.info('[START-SYNC] Starting dirty tables watcher...')
      await scheduler.startDirtyTablesWatcherAsync()

      // Setup visibility listener for mobile reconnection (Android/iOS)
      log.info('[START-SYNC] Setting up visibility listener for mobile reconnection...')
      setupVisibilityListener()

log.info('[START-SYNC] Initializing backends...')
      for (const backend of enabledBackends) {
        try {
          log.info(`[START-SYNC] Initializing backend ${backend.id}...`)
          await initBackendAsync(backend.id)
        } catch (error) {
          log.error(`[START-SYNC] Failed to init backend ${backend.id}:`, error)
        }
      }

      // CRITICAL: Clear dirty tables AFTER all initialization is complete
      // Store registrations and backend initialization above may trigger dirty table events.
      // We clear them here to prevent pushing initialization-related changes.
      log.info('[START-SYNC] Clearing dirty tables after initialization (1st clear)...')
      await invoke('clear_all_dirty_tables')
      log.info('[START-SYNC] clear_all_dirty_tables (1st) complete')

      // NOW mark initial sync as complete - pushes will be allowed from this point
      const vaultSettingsStore = useVaultSettingsStore()
      log.info('[START-SYNC] Checking if initial sync was already complete...')
      const wasAlreadyComplete = await vaultSettingsStore.isInitialSyncCompleteAsync()
      log.info(`[START-SYNC] wasAlreadyComplete = ${wasAlreadyComplete}`)
      if (!wasAlreadyComplete) {
        log.info('[START-SYNC] Marking initial sync as COMPLETE now (will trigger DB write)!')
        await vaultSettingsStore.setInitialSyncCompleteAsync()
        log.info('[START-SYNC] setInitialSyncCompleteAsync complete')
        // Clear dirty tables AGAIN after setting the flag, because setInitialSyncCompleteAsync
        // itself creates a dirty table entry
        log.info('[START-SYNC] Clearing dirty tables AGAIN after setInitialSyncCompleteAsync (2nd clear)...')
        await invoke('clear_all_dirty_tables')
        log.info('[START-SYNC] clear_all_dirty_tables (2nd) complete')
      }

      log.info('[START-SYNC] ========================================')
      log.info('[START-SYNC] startSyncAsync COMPLETE at ' + new Date().toISOString())
      log.info('[START-SYNC] ========================================')
    }

    /**
     * Stops sync for all backends
     */
    const stopSyncAsync = async (): Promise<void> => {
      log.info('========== STOP SYNC ==========')

      // Stop Rust event listeners
      localEvents?.dispose()
      localEvents = null

      // Remove visibility listener for mobile reconnection
      removeVisibilityListener()

      // Stop sync events listener (also clears all registered store reload functions)
      stopSyncEvents()

      // Stop dirty tables watcher
      scheduler.stopDirtyTablesWatcher()

      // Stop invite outbox processor
      if (outboxProcessorPoll) {
        outboxProcessorPoll.pause()
        outboxProcessorPoll = null
      }

      // Stop all periodic pull polls
      for (const [backendId, poll] of periodicPullPolls.entries()) {
        poll.pause()
        periodicPullPolls.delete(backendId)
      }

      // Unsubscribe individual backends and disconnect the shared WebSocket
      for (const backendId of Object.keys(syncStates.value)) {
        await unsubscribeFromBackendWrapperAsync(backendId)
      }
      await disconnectRealtimeAsync()
      _resetReconnectionContext()

      syncStates.value = {}
    }

    /**
     * Gets sync state for a specific backend
     */
    const getSyncState = (backendId: string) => {
      return syncStates.value[backendId] ?? null
    }

    /**
     * Checks if any backend is currently syncing
     */
    const isAnySyncing = computed(() => {
      return Object.values(syncStates.value).some((state) => state.isSyncing)
    })

    /**
     * Checks if all backends are connected
     */
    const areAllConnected = computed(() => {
      const enabledBackends = syncBackendsStore.enabledBackends
      if (enabledBackends.length === 0) return false

      return enabledBackends.every((backend) => {
        const state = syncStates.value[backend.id]
        return state?.isConnected ?? false
      })
    })

    /**
     * Performs initial pull using temporary backend configuration.
     * See `./initial-sync.ts` for the full implementation.
     */
    const performInitialPullAsync = async (): Promise<void> => {
      return performInitialPullImplAsync({
        currentVaultId,
        syncStates,
        syncBackendsStore,
        syncEngineStore,
      })
    }

    return {
      syncStates,
      isAnySyncing,
      areAllConnected,
      pushToBackendAsync: pushToBackendWrapperAsync,
      pullFromBackendAsync: pullFromBackendWrapperAsync,
      subscribeToBackendAsync: subscribeToBackendWrapperAsync,
      unsubscribeFromBackendAsync: unsubscribeFromBackendWrapperAsync,
      initBackendAsync,
      onLocalWriteAsync,
      startSyncAsync,
      stopSyncAsync,
      getSyncState,
      performInitialPullAsync,
      pushAllDataToBackendAsync,
    }
  },
)
