/**
 * Sync Orchestrator Store - Orchestrates sync operations across all backends
 * Uses new table-scanning approach with column-level HLC timestamps
 */

import { listen } from '@tauri-apps/api/event'
import { log, type BackendSyncState, type BatchAccumulator } from './types'
import { pushToBackendAsync } from './push'
import {
  pullFromBackendAsync,
  pullChangesFromServerWithConfigAsync,
  applyRemoteChangesInTransactionAsync,
} from './pull'
import {
  subscribeToBackendAsync,
  unsubscribeFromBackendAsync,
} from './realtime'
import { initSyncEventsAsync, stopSyncEvents } from '../syncEvents'

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

    // Batch accumulators for realtime changes (keyed by batchId)
    const batchAccumulators = ref<Map<string, BatchAccumulator>>(new Map())

    // Dirty tables watcher
    let dirtyTablesDebounceTimer: ReturnType<typeof setTimeout> | null = null
    let periodicSyncInterval: ReturnType<typeof setInterval> | null = null
    const periodicPullIntervals: Map<string, ReturnType<typeof setInterval>> = new Map()
    let eventUnlisten: (() => void) | null = null

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
        batchAccumulators.value,
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
     * Initializes sync for a backend
     */
    const initBackendAsync = async (backendId: string): Promise<void> => {
      log.info(`========== INIT BACKEND START (${backendId}) ==========`)

      if (syncStates.value[backendId]) {
        log.debug(`INIT: Backend ${backendId} already initialized`)
        return
      }

      // Initialize state
      syncStates.value[backendId] = {
        isConnected: false,
        isSyncing: false,
        error: null,
        subscription: null,
      }
      log.debug('INIT: State initialized')

      try {
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

        // Subscribe to realtime changes
        log.info('INIT: Step 3 - Subscribe to realtime changes')
        await subscribeToBackendWrapperAsync(backendId)

        // Start periodic pull as fallback (every 5 minutes)
        log.info('INIT: Step 4 - Setting up periodic pull (every 5 min)')
        const periodicPullInterval = setInterval(
          async () => {
            try {
              log.debug(`PERIODIC: Pull for backend ${backendId}`)
              await pullFromBackendWrapperAsync(backendId)
            } catch (error) {
              log.error(`PERIODIC: Pull failed for backend ${backendId}:`, error)
            }
          },
          5 * 60 * 1000,
        ) // 5 minutes

        periodicPullIntervals.set(backendId, periodicPullInterval)

        log.info(`========== INIT BACKEND SUCCESS (${backendId}) ==========`)
      } catch (error) {
        log.error(`========== INIT BACKEND FAILED (${backendId}) ==========`, error)
        throw error
      }
    }

    /**
     * Called after local write operations to push changes
     */
    const onLocalWriteAsync = async (): Promise<void> => {
      log.debug('onLocalWriteAsync: Triggered')
      try {
        // Push to all enabled backends in parallel
        const enabledBackends = syncBackendsStore.enabledBackends
        log.debug(`onLocalWriteAsync: Pushing to ${enabledBackends.length} backends`)

        await Promise.allSettled(
          enabledBackends.map((backend) => pushToBackendWrapperAsync(backend.id)),
        )
      } catch (error) {
        log.error('onLocalWriteAsync: Failed to push local changes:', error)
      }
    }

    /**
     * Handles dirty tables event from Rust - triggers sync based on configuration
     */
    const onDirtyTablesChangedAsync = async (): Promise<void> => {
      const config = syncConfigStore.config
      log.debug(`DIRTY_TABLES: Event received (mode: ${config.mode})`)

      if (config.mode === 'continuous') {
        // In continuous mode, debounce to batch rapid changes
        if (dirtyTablesDebounceTimer) {
          log.debug('DIRTY_TABLES: Resetting debounce timer')
          clearTimeout(dirtyTablesDebounceTimer)
        }

        dirtyTablesDebounceTimer = setTimeout(async () => {
          log.info('DIRTY_TABLES: Debounce timer elapsed, triggering sync...')
          await onLocalWriteAsync()
          dirtyTablesDebounceTimer = null
        }, config.continuousDebounceMs)
      }
      // In periodic mode, do nothing - the interval will handle it
    }

    /**
     * Starts the dirty tables watcher based on sync configuration
     */
    const startDirtyTablesWatcherAsync = async (): Promise<void> => {
      log.info('WATCHER: Starting dirty tables watcher...')
      stopDirtyTablesWatcher()

      const config = syncConfigStore.config
      log.debug('WATCHER: Config:', config)

      // Listen to Tauri event from Rust
      eventUnlisten = await listen('crdt:dirty-tables-changed', async () => {
        await onDirtyTablesChangedAsync()
      })

      log.info(`WATCHER: Started in ${config.mode} mode`)

      if (config.mode === 'periodic') {
        // In periodic mode, sync at regular intervals
        periodicSyncInterval = setInterval(async () => {
          log.debug('WATCHER: Periodic sync timer elapsed')
          await onLocalWriteAsync()
        }, config.periodicIntervalMs)

        log.info(`WATCHER: Periodic interval set to ${config.periodicIntervalMs}ms`)
      }
    }

    /**
     * Stops the dirty tables watcher
     */
    const stopDirtyTablesWatcher = (): void => {
      if (dirtyTablesDebounceTimer) {
        clearTimeout(dirtyTablesDebounceTimer)
        dirtyTablesDebounceTimer = null
      }

      if (periodicSyncInterval) {
        clearInterval(periodicSyncInterval)
        periodicSyncInterval = null
      }

      if (eventUnlisten) {
        eventUnlisten()
        eventUnlisten = null
      }

      log.debug('WATCHER: Stopped')
    }

    /**
     * Starts sync for all enabled backends
     */
    const startSyncAsync = async (): Promise<void> => {
      log.info('========== START SYNC ==========')

      const enabledBackends = syncBackendsStore.enabledBackends

      if (enabledBackends.length === 0) {
        log.info('START: No enabled backends to sync with')
        return
      }

      log.info(
        `START: Found ${enabledBackends.length} enabled backends:`,
        enabledBackends.map((b) => ({ id: b.id, name: b.name })),
      )

      // Initialize sync events listener (for frontend refresh after pull)
      log.debug('START: Initializing sync events listener...')
      await initSyncEventsAsync()

      // Start vault settings sync listener
      const vaultSettingsStore = useVaultSettingsStore()
      vaultSettingsStore.startSyncListener()

      // Load sync configuration
      log.debug('START: Loading sync configuration...')
      await syncConfigStore.loadConfigAsync()

      // Start dirty tables watcher
      await startDirtyTablesWatcherAsync()

      for (const backend of enabledBackends) {
        try {
          await initBackendAsync(backend.id)
        } catch (error) {
          log.error(`START: Failed to init backend ${backend.id}:`, error)
        }
      }

      log.info('========== START SYNC COMPLETE ==========')
    }

    /**
     * Stops sync for all backends
     */
    const stopSyncAsync = async (): Promise<void> => {
      log.info('========== STOP SYNC ==========')

      // Stop vault settings sync listener
      const vaultSettingsStore = useVaultSettingsStore()
      vaultSettingsStore.stopSyncListener()

      // Stop sync events listener
      stopSyncEvents()

      // Stop dirty tables watcher
      stopDirtyTablesWatcher()

      // Stop all periodic pull intervals
      for (const [backendId, interval] of periodicPullIntervals.entries()) {
        clearInterval(interval)
        periodicPullIntervals.delete(backendId)
      }

      for (const backendId of Object.keys(syncStates.value)) {
        await unsubscribeFromBackendWrapperAsync(backendId)
      }

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
     * This is used when connecting to a remote vault - we need to pull all data
     * before the backend is persisted to the database.
     *
     * Flow:
     * 1. Uses temporary backend from syncBackendsStore
     * 2. Pulls all changes from remote server
     * 3. After successful pull, persists backend to DB (checking for duplicates from synced data)
     */
    const performInitialPullAsync = async (): Promise<void> => {
      log.info('========== INITIAL PULL START (temporary backend) ==========')

      const tempBackend = syncBackendsStore.temporaryBackend
      if (!tempBackend) {
        log.error('INITIAL PULL FAILED: No temporary backend configured')
        throw new Error('No temporary backend configured')
      }

      if (!currentVaultId.value) {
        log.error('INITIAL PULL FAILED: No vault opened')
        throw new Error('No vault opened')
      }

      const backendId = tempBackend.id

      // Initialize state for this backend
      syncStates.value[backendId] = {
        isConnected: false,
        isSyncing: true,
        error: null,
        subscription: null,
      }

      try {
        // Get vault key from cache
        const vaultKey = syncEngineStore.vaultKeyCache[tempBackend.vaultId]?.vaultKey
        if (!vaultKey) {
          log.error('INITIAL PULL FAILED: Vault key not available')
          throw new Error('Vault key not available. Please unlock vault first.')
        }

        log.debug('Initial pull config:', {
          backendId,
          vaultId: tempBackend.vaultId,
          serverUrl: tempBackend.serverUrl,
        })

        // Pull ALL changes (no cursor since this is initial sync)
        log.info('Downloading all changes from server...')
        const pullResult = await pullChangesFromServerWithConfigAsync(
          tempBackend.serverUrl,
          tempBackend.vaultId,
          null, // No lastPullServerTimestamp - get everything
          syncEngineStore,
        )

        const { changes: allChanges, serverTimestamp } = pullResult

        let maxHlc = ''
        if (allChanges.length === 0) {
          log.info('INITIAL PULL: No data on server (empty vault)')
        } else {
          log.info(`Downloaded ${allChanges.length} changes from server`)

          // Apply all changes atomically
          log.info('Applying changes to local database...')
          maxHlc = await applyRemoteChangesInTransactionAsync(allChanges, vaultKey, backendId)
        }

        // Now persist the backend to DB
        // This will check if backend already exists from synced data
        log.info('Persisting backend to database...')
        await syncBackendsStore.persistTemporaryBackendAsync()

        // Update timestamps on the persisted backend
        // The backend ID might be different (from synced data) so we need to find it
        // Reload backends to get the persisted one
        await syncBackendsStore.loadBackendsAsync()

        // Find the backend (could have different ID if it existed from sync)
        const persistedBackend = await syncBackendsStore.findBackendByCredentialsAsync(
          tempBackend.serverUrl,
          tempBackend.email,
        )

        if (persistedBackend) {
          const updates: { lastPullServerTimestamp?: string; lastPushHlcTimestamp?: string } = {}

          // Set lastPullServerTimestamp from server response
          if (serverTimestamp) {
            log.debug('Updating lastPullServerTimestamp on persisted backend:', serverTimestamp)
            updates.lastPullServerTimestamp = serverTimestamp
          }

          // Set lastPushHlcTimestamp to prevent re-pushing the pulled data
          // This is crucial - without this, all pulled data would be pushed back!
          if (maxHlc) {
            log.debug('Updating lastPushHlcTimestamp on persisted backend:', maxHlc)
            updates.lastPushHlcTimestamp = maxHlc
          }

          if (Object.keys(updates).length > 0) {
            await syncBackendsStore.updateBackendAsync(persistedBackend.id, updates)
          }
        }

        syncStates.value[backendId].isSyncing = false
        log.info(`========== INITIAL PULL SUCCESS: ${allChanges.length} changes applied ==========`)
      } catch (error) {
        log.error('========== INITIAL PULL FAILED ==========', error)
        syncStates.value[backendId].error = error instanceof Error ? error.message : 'Unknown error'
        syncStates.value[backendId].isSyncing = false
        throw error
      }
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
    }
  },
)
