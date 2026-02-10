/**
 * Sync Orchestrator Store - Orchestrates sync operations across all backends
 * Uses new table-scanning approach with column-level HLC timestamps
 */

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { orchestratorLog as log, type BackendSyncState } from './types'
import { enterBulkMode, exitBulkMode } from '@/stores/logging'
import { pushToBackendAsync, pushAllDataToBackendAsync } from './push'
import {
  pullFromBackendAsync,
  pullChangesFromServerWithConfigAsync,
  applyAllChangesWithMigrationsAsync,
} from './pull'
import {
  subscribeToBackendAsync,
  unsubscribeFromBackendAsync,
  setupVisibilityListener,
  removeVisibilityListener,
} from './realtime'
import { initSyncEventsAsync, stopSyncEvents, registerStoreForTables } from '../syncEvents'

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

    // Dirty tables watcher
    let dirtyTablesDebounceTimer: ReturnType<typeof setTimeout> | null = null
    let periodicSyncInterval: ReturnType<typeof setInterval> | null = null
    const periodicPullIntervals: Map<string, ReturnType<typeof setInterval>> = new Map()
    let eventUnlisten: (() => void) | null = null

    // Adaptive debouncing for bulk operations
    // Tracks event frequency to detect bulk imports and increase debounce accordingly
    const EVENT_WINDOW_MS = 1000 // Time window to count events
    const BULK_THRESHOLD = 10 // Events in window to trigger bulk mode
    const MAX_DEBOUNCE_MS = 5000 // Maximum debounce time during bulk operations
    let eventTimestamps: number[] = []
    let currentDebounceMs: number | null = null // null = use config default


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
          subscription: null,
        }
        log.debug('INIT: State initialized')
      }

      try {
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
        // Skip if already subscribed
        if (!syncStates.value[backendId]?.subscription) {
          log.info('INIT: Step 3 - Subscribe to realtime changes')
          await subscribeToBackendWrapperAsync(backendId)
        } else {
          log.info('INIT: Step 3 - Skipping realtime (already subscribed)')
        }

        // Always start periodic pull as fallback (even if initial pull was already done)
        // Skip if already running
        if (!periodicPullIntervals.has(backendId)) {
          log.info('INIT: Step 4 - Setting up periodic pull (every 5 min)')
          const periodicPullInterval = setInterval(
            async () => {
              try {
                log.info(`PERIODIC: Pull triggered for backend ${backendId} at ${new Date().toISOString()}`)
                await pullFromBackendWrapperAsync(backendId)
              } catch (error) {
                log.error(`PERIODIC: Pull failed for backend ${backendId}:`, error)
              }
            },
            5 * 60 * 1000,
          ) // 5 minutes

          periodicPullIntervals.set(backendId, periodicPullInterval)
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

    // Track whether we're currently in bulk mode for logging
    let isInBulkMode = false

    /**
     * Calculates adaptive debounce time based on event frequency.
     * During bulk operations (like KeePass import), events flood in rapidly.
     * We detect this and increase debounce to prevent UI blocking.
     * Also activates bulk logging mode to suppress verbose logs.
     */
    const getAdaptiveDebounceMs = (): number => {
      const now = Date.now()
      const config = syncConfigStore.config

      // Add current timestamp
      eventTimestamps.push(now)

      // Remove old timestamps outside the window
      eventTimestamps = eventTimestamps.filter(t => now - t < EVENT_WINDOW_MS)

      // Calculate event rate
      const eventsInWindow = eventTimestamps.length

      if (eventsInWindow >= BULK_THRESHOLD) {
        // Bulk operation detected - scale debounce based on event rate
        // More events = longer debounce (up to MAX_DEBOUNCE_MS)
        const scaleFactor = Math.min(eventsInWindow / BULK_THRESHOLD, 5)
        currentDebounceMs = Math.min(config.continuousDebounceMs * scaleFactor, MAX_DEBOUNCE_MS)

        // Enter bulk logging mode to suppress verbose logs
        if (!isInBulkMode) {
          isInBulkMode = true
          enterBulkMode()
        }

        return currentDebounceMs
      }

      // Normal operation - use config default
      currentDebounceMs = null

      // Exit bulk logging mode if we were in it
      if (isInBulkMode) {
        isInBulkMode = false
        exitBulkMode()
      }

      return config.continuousDebounceMs
    }

    /**
     * Handles dirty tables event from Rust - triggers push with debounce
     * This runs in parallel with periodic pulls
     *
     * Uses adaptive debouncing: During bulk operations (many events in short time),
     * the debounce interval is automatically increased to prevent UI blocking.
     */
    const onDirtyTablesChangedAsync = async (): Promise<void> => {
      const config = syncConfigStore.config
      const adaptiveDebounce = getAdaptiveDebounceMs()
      const isBulkMode = adaptiveDebounce > config.continuousDebounceMs

      // Only log occasionally during bulk operations to reduce console spam
      if (!isBulkMode || eventTimestamps.length % 50 === 0) {
        const eventId = Math.random().toString(36).substring(7)
        if (isBulkMode) {
          log.info(`[DIRTY:${eventId}] Bulk operation detected (${eventTimestamps.length} events) - using ${adaptiveDebounce}ms debounce`)
        } else {
          log.debug(`[DIRTY:${eventId}] Event received, debounce: ${adaptiveDebounce}ms`)
        }
      }

      // Debounce to batch rapid changes before pushing
      if (dirtyTablesDebounceTimer) {
        clearTimeout(dirtyTablesDebounceTimer)
      }

      dirtyTablesDebounceTimer = setTimeout(async () => {
        // Reset event tracking after debounce fires
        eventTimestamps = []
        currentDebounceMs = null

        // Exit bulk logging mode
        if (isInBulkMode) {
          isInBulkMode = false
          exitBulkMode()
        }

        log.info(`[DIRTY] Debounce elapsed after ${adaptiveDebounce}ms, pushing changes...`)
        await onLocalWriteAsync()
        dirtyTablesDebounceTimer = null
      }, adaptiveDebounce)
    }

    /**
     * Starts sync watchers:
     * - Push: Listens for dirty tables and pushes local changes with debounce
     * - Fallback Pull: Periodically fetches to catch missed realtime updates
     */
    const startDirtyTablesWatcherAsync = async (): Promise<void> => {
      log.info('[WATCHER] Starting sync watchers...')
      stopDirtyTablesWatcher()

      const config = syncConfigStore.config
      log.info('[WATCHER] Config:', config)

      // Start push watcher: Listen to dirty tables events
      log.info('[WATCHER] Registering listener for crdt:dirty-tables-changed...')
      eventUnlisten = await listen('crdt:dirty-tables-changed', async () => {
        await onDirtyTablesChangedAsync()
      })
      log.info(`[WATCHER] Push listener REGISTERED (debounce: ${config.continuousDebounceMs}ms)`)

      // Start fallback pull: Catch missed realtime updates
      periodicSyncInterval = setInterval(async () => {
        log.info('[WATCHER] Fallback pull timer elapsed - pulling from all backends')
        const enabledBackends = syncBackendsStore.enabledBackends
        for (const backend of enabledBackends) {
          try {
            await pullFromBackendWrapperAsync(backend.id)
          } catch (error) {
            log.error(`[WATCHER] Fallback pull failed for backend ${backend.id}:`, error)
          }
        }
      }, config.periodicIntervalMs)
      log.info(
        `[WATCHER] Fallback pull started (interval: ${config.periodicIntervalMs}ms)`,
      )
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
      log.info('[START-SYNC] ========================================')
      log.info('[START-SYNC] startSyncAsync CALLED at ' + new Date().toISOString())
      log.info('[START-SYNC] ========================================')

      const enabledBackends = syncBackendsStore.enabledBackends

      if (enabledBackends.length === 0) {
        log.info('[START-SYNC] No enabled backends to sync with')
        return
      }

      log.info(
        `[START-SYNC] Found ${enabledBackends.length} enabled backends:`,
        enabledBackends.map((b) => ({ id: b.id, name: b.name })),
      )

      // Initialize sync events listener (for frontend refresh after pull)
      log.debug('START: Initializing sync events listener...')
      await initSyncEventsAsync()

      // Register all stores for their respective tables
      // This is the central place where we define which stores reload on which table updates
      log.debug('START: Registering stores for sync events...')
      registerStoreForTables(
        ['haex_extensions', 'haex_extension_migrations'],
        async () => {
          const extensionsStore = useExtensionsStore()
          await extensionsStore.loadExtensionsAsync()
        },
      )
      registerStoreForTables(
        ['haex_workspaces'],
        async () => {
          const workspaceStore = useWorkspaceStore()
          await workspaceStore.loadWorkspacesAsync()
        },
      )
      registerStoreForTables(
        ['haex_desktop_items'],
        async () => {
          const desktopStore = useDesktopStore()
          await desktopStore.loadDesktopItemsAsync()
        },
      )
      registerStoreForTables(
        ['haex_vault_settings'],
        async () => {
          const vaultSettingsStore = useVaultSettingsStore()
          await vaultSettingsStore.syncThemeAsync()
          await vaultSettingsStore.syncLocaleAsync()
          await vaultSettingsStore.syncVaultNameAsync()
        },
      )

      // Load sync configuration
      log.info('[START-SYNC] Loading sync configuration...')
      await syncConfigStore.loadConfigAsync()

      // Start dirty tables watcher
      log.info('[START-SYNC] Starting dirty tables watcher...')
      await startDirtyTablesWatcherAsync()

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

      // Remove visibility listener for mobile reconnection
      removeVisibilityListener()

      // Stop sync events listener (also clears all registered store reload functions)
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

      // Reset backends store to clear cached backends
      syncBackendsStore.reset()
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
      log.info('[INITIAL-PULL] ========================================')
      log.info('[INITIAL-PULL] performInitialPullAsync CALLED at ' + new Date().toISOString())
      log.info('[INITIAL-PULL] ========================================')

      // Note: Pushes are blocked until initial_sync_complete is set to 'true' in vault settings
      // This happens at the end of this function via setInitialSyncCompleteAsync()

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

          // Log unique tables for debugging
          const uniqueTables = [...new Set(allChanges.map((c) => c.tableName))]
          log.info('INITIAL PULL: Tables in server data:', uniqueTables)

          // Apply all changes with proper migration ordering
          // This ensures extension tables are created before their data is applied
          maxHlc = await applyAllChangesWithMigrationsAsync(allChanges, vaultKey, backendId)
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

          // CRITICAL: If the persisted backend has a different ID than the temp backend,
          // we need to transfer the sync state to prevent initBackendAsync from running again.
          // This can happen when the backend was already synced from the server with a different ID.
          if (persistedBackend.id !== backendId) {
            log.info(`Backend ID changed: ${backendId} -> ${persistedBackend.id}, transferring sync state`)
            syncStates.value[persistedBackend.id] = syncStates.value[backendId]
            Reflect.deleteProperty(syncStates.value, backendId)
          }
        }

        // CRITICAL: Reload all stores with synced data BEFORE setting isSyncing = false
        // This ensures vault.vue's waitForInitialSyncAsync() doesn't resolve until stores are loaded
        // Otherwise, desktop/index.vue might load empty stores before sync data is available
        // Note: We reload stores directly here instead of using sync:tables-updated event
        // because the event listeners aren't registered yet during initial pull
        log.info('Reloading stores with synced data (before signaling sync complete)...')
        try {
          const extensionsStore = useExtensionsStore()
          const workspaceStore = useWorkspaceStore()
          const desktopStore = useDesktopStore()
          const vaultSettingsStore = useVaultSettingsStore()

          await extensionsStore.loadExtensionsAsync()
          log.debug(`Extensions loaded: ${extensionsStore.availableExtensions.length}`)

          await workspaceStore.loadWorkspacesAsync()
          log.debug(`Workspaces loaded: ${workspaceStore.workspaces.length}`)

          await desktopStore.loadDesktopItemsAsync()
          log.debug(`Desktop items loaded: ${desktopStore.desktopItems.length}`)

          // Also sync vault settings
          await vaultSettingsStore.syncThemeAsync()
          await vaultSettingsStore.syncLocaleAsync()
          await vaultSettingsStore.syncVaultNameAsync()
        } catch (reloadError) {
          log.error('Failed to reload stores after initial pull:', reloadError)
          // Don't throw - the data is in DB, UI can retry loading
        }

        // Use the persisted backend ID if available (it may be different from tempBackend.id)
        const finalBackendId = persistedBackend?.id ?? backendId
        if (syncStates.value[finalBackendId]) {
          syncStates.value[finalBackendId].isSyncing = false
        }

        // Clear ALL dirty tables AFTER all store operations to prevent re-pushing pulled data
        // This is critical: store operations above (updateBackendAsync, syncThemeAsync, etc.)
        // trigger dirty table events. We clear them here to prevent pushing local-only data.
        log.info('Clearing all dirty tables after initial pull and store operations...')
        await invoke('clear_all_dirty_tables')

        // NOTE: initial_sync_complete is NOT set here anymore.
        // It will be set at the end of startSyncAsync() AFTER:
        // 1. The dirty tables watcher is started
        // 2. All backends are initialized
        // 3. Dirty tables are cleared again
        // This ensures no pushes happen during the initialization phase.

        log.info(`========== INITIAL PULL SUCCESS: ${allChanges.length} changes applied ==========`)
      } catch (error) {
        log.error('========== INITIAL PULL FAILED ==========', error)
        syncStates.value[backendId].error = error instanceof Error ? error.message : 'Unknown error'
        syncStates.value[backendId].isSyncing = false

        // NOTE: We intentionally do NOT set initial_sync_complete on error.
        // The caller (connect.vue) will handle the error and clean up the vault.
        // If the user retries, a fresh initial pull will be attempted.

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
      pushAllDataToBackendAsync,
    }
  },
)
