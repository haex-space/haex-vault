/**
 * Sync Orchestrator Store - Orchestrates sync operations across all backends
 * Uses new table-scanning approach with column-level HLC timestamps
 */

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  RealtimeChannel,
  RealtimePostgresInsertPayload,
} from '@supabase/supabase-js'
import {
  getDirtyTablesAsync,
  scanTableForChangesAsync,
  clearDirtyTableAsync,
  type ColumnChange,
} from './tableScanner'
import { decryptCrdtDataAsync } from '~/utils/crypto/vaultKey'

interface SyncState {
  isConnected: boolean
  isSyncing: boolean
  error: string | null
  subscription: RealtimeChannel | null
}

interface BackendSyncState {
  [backendId: string]: SyncState
}

// Batch accumulator for realtime changes
interface BatchAccumulator {
  backendId: string
  changes: ColumnChange[]
  receivedCount: number
  totalCount: number
  timeout?: ReturnType<typeof setTimeout>
}

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
    const periodicPullIntervals: Map<
      string,
      ReturnType<typeof setInterval>
    > = new Map()
    let eventUnlisten: (() => void) | null = null

    /**
     * Pushes local changes to a specific backend using table-scanning approach
     */
    const pushToBackendAsync = async (backendId: string): Promise<void> => {
      if (!currentVaultId.value) {
        throw new Error('No vault opened')
      }

      const state = syncStates.value[backendId]
      if (!state) {
        throw new Error('Backend not initialized')
      }

      if (state.isSyncing) {
        console.log(`Already syncing with backend ${backendId}`)
        return
      }

      state.isSyncing = true
      state.error = null

      try {
        // Get backend configuration
        const backend = syncBackendsStore.backends.find(
          (b) => b.id === backendId,
        )
        if (!backend?.vaultId) {
          throw new Error('Backend vaultId not configured')
        }

        const lastPushHlc = backend.lastPushHlcTimestamp

        // Get vault key from cache
        const vaultKey =
          syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
        if (!vaultKey) {
          throw new Error('Vault key not available. Please unlock vault first.')
        }

        // Get current device ID
        const deviceStore = useDeviceStore()
        const deviceId = deviceStore.deviceId
        if (!deviceId) {
          throw new Error('Device ID not available')
        }

        // Get all dirty tables that need to be synced
        const dirtyTables = await getDirtyTablesAsync()

        if (dirtyTables.length === 0) {
          console.log(`No dirty tables to push to backend ${backendId}`)
          return
        }

        console.log(
          `Found ${dirtyTables.length} dirty tables to scan for changes`,
        )

        // Generate a batch ID for this push - all changes in this push belong together
        const batchId = crypto.randomUUID()

        // Scan each dirty table for column-level changes (without batch seq numbers yet)
        const partialChanges: Omit<ColumnChange, 'batchSeq' | 'batchTotal'>[] =
          []
        let maxHlc = lastPushHlc || ''

        for (const { tableName } of dirtyTables) {
          try {
            console.log(
              `Scanning table ${tableName} with lastPushHlc:`,
              lastPushHlc === null
                ? 'null'
                : lastPushHlc === undefined
                  ? 'undefined'
                  : lastPushHlc === ''
                    ? '(empty string)'
                    : lastPushHlc,
            )
            const tableChanges = await scanTableForChangesAsync(
              tableName,
              lastPushHlc,
              vaultKey,
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

            console.log(
              `Found ${tableChanges.length} changes in table ${tableName}`,
            )
          } catch (error) {
            console.error(`Failed to scan table ${tableName}:`, error)
            // Continue with other tables even if one fails
          }
        }

        // Add batch sequence numbers now that we know the total
        const batchTotal = partialChanges.length
        const allChanges: ColumnChange[] = partialChanges.map(
          (change, index) => ({
            ...change,
            batchSeq: index + 1, // 1-based sequence
            batchTotal,
          }),
        )

        if (allChanges.length === 0) {
          console.log(`No changes to push to backend ${backendId}`)
          // Clear dirty tables even if no changes (they might have been synced already)
          for (const { tableName } of dirtyTables) {
            await clearDirtyTableAsync(tableName)
          }
          return
        }

        console.log(
          `Pushing ${allChanges.length} column-level changes to backend ${backendId}`,
        )

        // Push changes to server using new format
        await pushChangesToServerAsync(backendId, backend.vaultId, allChanges)

        // Update backend's lastPushHlcTimestamp
        await syncBackendsStore.updateBackendAsync(backendId, {
          lastPushHlcTimestamp: maxHlc,
        })

        // Clear dirty tables after successful push
        for (const { tableName } of dirtyTables) {
          await clearDirtyTableAsync(tableName)
        }

        console.log(
          `Successfully pushed ${allChanges.length} changes to backend ${backendId}`,
        )
      } catch (error) {
        console.error(`Failed to push to backend ${backendId}:`, error)
        state.error = error instanceof Error ? error.message : 'Unknown error'
        throw error
      } finally {
        state.isSyncing = false
      }
    }

    /**
     * Pushes column-level changes to server
     */
    const pushChangesToServerAsync = async (
      backendId: string,
      vaultId: string,
      changes: ColumnChange[],
    ): Promise<void> => {
      // Get auth token
      const token = await syncEngineStore.getAuthTokenAsync()
      if (!token) {
        throw new Error('Not authenticated')
      }

      const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
      if (!backend) {
        throw new Error('Backend not found')
      }

      // Get current device ID
      const deviceStore = useDeviceStore()
      const deviceId = deviceStore.deviceId

      // Format changes for server API
      const formattedChanges = changes.map((change) => ({
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
      }))

      // Send to server
      const response = await fetch(`${backend.serverUrl}/sync/push`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify({
          vaultId,
          changes: formattedChanges,
        }),
      })

      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        throw new Error(
          `Failed to push changes: ${error.error || response.statusText}`,
        )
      }

      console.log(`✅ Pushed ${changes.length} changes to server`)
    }

    /**
     * Pulls changes from a specific backend using column-level HLC comparison
     * Downloads ALL changes first, then applies them atomically in a transaction
     */
    const pullFromBackendAsync = async (backendId: string): Promise<void> => {
      if (!currentVaultId.value) {
        throw new Error('No vault opened')
      }

      const state = syncStates.value[backendId]
      if (!state) {
        throw new Error('Backend not initialized')
      }

      if (state.isSyncing) {
        console.log(`Already syncing with backend ${backendId}`)
        return
      }

      state.isSyncing = true
      state.error = null

      try {
        const backend = syncBackendsStore.backends.find(
          (b) => b.id === backendId,
        )
        if (!backend?.vaultId) {
          throw new Error('Backend vaultId not configured')
        }

        // Get vault key from cache
        const vaultKey =
          syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
        if (!vaultKey) {
          throw new Error('Vault key not available. Please unlock vault first.')
        }

        const lastPullHlc = backend.lastPullHlcTimestamp

        console.log(
          `Pulling changes from backend ${backendId} since ${lastPullHlc || 'beginning'}`,
        )

        // Step 1: Download ALL changes from server (with pagination)
        const allChanges = await pullChangesFromServerAsync(
          backendId,
          backend.vaultId,
          lastPullHlc,
        )

        if (allChanges.length === 0) {
          console.log(`No new changes from backend ${backendId}`)
          return
        }

        console.log(
          `Downloaded ${allChanges.length} changes from backend ${backendId}`,
        )

        // Step 2: Apply ALL changes atomically in a single transaction
        // The transaction also updates the lastPullHlcTimestamp in the database
        await applyRemoteChangesInTransactionAsync(
          allChanges,
          vaultKey,
          backendId,
        )

        // Step 3: Reload backend data from database to get updated lastPullHlcTimestamp
        await syncBackendsStore.loadBackendsAsync()

        console.log(
          `Successfully pulled and applied ${allChanges.length} changes from backend ${backendId}`,
        )
      } catch (error) {
        console.error(`Failed to pull from backend ${backendId}:`, error)
        state.error = error instanceof Error ? error.message : 'Unknown error'
        throw error
      } finally {
        state.isSyncing = false
      }
    }

    /**
     * Pulls column-level changes from server with pagination
     */
    const pullChangesFromServerAsync = async (
      backendId: string,
      vaultId: string,
      lastPullHlc: string | null,
    ): Promise<ColumnChange[]> => {
      const token = await syncEngineStore.getAuthTokenAsync()
      if (!token) {
        throw new Error('Not authenticated')
      }

      const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
      if (!backend) {
        throw new Error('Backend not found')
      }

      const allChanges: ColumnChange[] = []
      let hasMore = true
      let currentCursor: string | null = lastPullHlc

      // Pagination loop - download ALL changes before applying
      while (hasMore) {
        const response = await fetch(
          `${backend.serverUrl}/sync/pull?vaultId=${vaultId}&since=${currentCursor || ''}&limit=1000`,
          {
            method: 'GET',
            headers: {
              Authorization: `Bearer ${token}`,
            },
          },
        )

        if (!response.ok) {
          const error = await response.json().catch(() => ({}))
          throw new Error(
            `Failed to pull changes: ${error.error || response.statusText}`,
          )
        }

        const data = await response.json()
        const changes: ColumnChange[] = data.changes || []

        allChanges.push(...changes)

        // Check if there are more pages
        hasMore = data.hasMore === true
        currentCursor = data.nextCursor || null

        console.log(
          `Downloaded page with ${changes.length} changes (total: ${allChanges.length}, hasMore: ${hasMore})`,
        )
      }

      return allChanges
    }

    /**
     * Applies remote changes atomically in a single transaction
     * Also updates the lastPullHlcTimestamp in the same transaction
     */
    const applyRemoteChangesInTransactionAsync = async (
      changes: ColumnChange[],
      vaultKey: Uint8Array,
      backendId: string,
    ): Promise<void> => {
      // Calculate max HLC and decrypt all changes
      let maxHlc = ''
      const decryptedChanges = []

      for (const change of changes) {
        // Track max HLC
        if (change.hlcTimestamp > maxHlc) {
          maxHlc = change.hlcTimestamp
        }

        // Decrypt the value
        let decryptedValue
        if (change.encryptedValue && change.nonce) {
          const decryptedData = await decryptCrdtDataAsync<{ value: unknown }>(
            change.encryptedValue,
            change.nonce,
            vaultKey,
          )
          decryptedValue = decryptedData.value
        } else {
          decryptedValue = null
        }

        decryptedChanges.push({
          tableName: change.tableName,
          rowPks: change.rowPks,
          columnName: change.columnName,
          hlcTimestamp: change.hlcTimestamp,
          batchId: change.batchId || crypto.randomUUID(), // Use existing or generate dummy
          batchSeq: change.batchSeq || 1, // Default to 1
          batchTotal: change.batchTotal || 1, // Default to 1
          decryptedValue,
        })
      }

      // Call Tauri command to apply changes in a transaction
      await invoke('apply_remote_changes_in_transaction', {
        changes: decryptedChanges,
        backendId,
        maxHlc,
      })
    }

    /**
     * Fetches missing changes from a batch by their sequence numbers
     */
    const fetchMissingBatchChangesAsync = async (
      backendId: string,
      batchId: string,
      receivedSeqNumbers: number[],
      totalCount: number,
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
      const missingSeqs = allSeqs.filter(
        (seq) => !receivedSeqNumbers.includes(seq),
      )

      if (missingSeqs.length === 0) {
        return []
      }

      console.log(
        `Fetching ${missingSeqs.length} missing changes for batch ${batchId}`,
      )

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
        throw new Error(
          `Failed to fetch missing batch changes: ${response.statusText}`,
        )
      }

      const data = await response.json()
      return data.changes
    }

    /**
     * Handles incoming realtime changes from Supabase
     * Accumulates changes by batchId and applies them once the batch is complete
     */
    const handleRealtimeChangeAsync = async (
      backendId: string,
      payload: RealtimePostgresInsertPayload<ColumnChange>,
    ) => {
      console.log(`Realtime change from backend ${backendId}:`, payload)

      try {
        // Get backend configuration
        const backend = syncBackendsStore.backends.find(
          (b) => b.id === backendId,
        )
        if (!backend?.vaultId) {
          console.error('Backend vaultId not configured')
          return
        }

        // Get vault key from cache
        const vaultKey =
          syncEngineStore.vaultKeyCache[backend.vaultId]?.vaultKey
        if (!vaultKey) {
          console.error('Vault key not available')
          return
        }

        // Extract the new record from Supabase realtime payload
        const change = payload.new
        if (!change) {
          console.warn('No new record in realtime payload')
          return
        }

        // Skip if this change was made by our own device to avoid applying our own changes
        const currentDeviceId = await invoke<string>('get_device_id')
        if (change.deviceId === currentDeviceId) {
          console.log('Skipping realtime change - originated from this device')
          return
        }

        const batchId = change.batchId
        const batchTotal = change.batchTotal
        const batchSeq = change.batchSeq

        // Realtime changes must have batch info
        if (!batchId || batchTotal === undefined || batchSeq === undefined) {
          console.error('Realtime change missing batch info:', change)
          return
        }

        // Get or create batch accumulator
        let accumulator = batchAccumulators.value.get(batchId)
        if (!accumulator) {
          accumulator = {
            backendId,
            changes: [],
            receivedCount: 0,
            totalCount: batchTotal,
            timeout: undefined,
          }
          batchAccumulators.value.set(batchId, accumulator)
        }

        // Add change to accumulator
        accumulator.changes.push(change)
        accumulator.receivedCount++

        console.log(
          `Batch ${batchId}: received ${accumulator.receivedCount}/${batchTotal} changes`,
        )

        // Clear any existing timeout
        if (accumulator.timeout) {
          clearTimeout(accumulator.timeout)
        }

        // Check if batch is complete
        if (accumulator.receivedCount >= batchTotal) {
          // Batch complete - apply all changes
          console.log(
            `Batch ${batchId} complete, applying ${accumulator.changes.length} changes`,
          )

          // Sort by batchSeq to ensure correct order
          const sortedChanges = accumulator.changes.sort(
            (a, b) => (a.batchSeq ?? 0) - (b.batchSeq ?? 0),
          )

          await applyRemoteChangesInTransactionAsync(
            sortedChanges,
            vaultKey,
            backendId,
          )

          // Clean up accumulator
          batchAccumulators.value.delete(batchId)

          console.log(`✅ Applied batch ${batchId} successfully`)
        } else {
          // Set a timeout to fetch missing changes after 10 seconds
          // This handles cases where some batch items get lost in realtime
          accumulator.timeout = setTimeout(async () => {
            const acc = batchAccumulators.value.get(batchId)
            if (!acc) return

            console.warn(
              `Batch ${batchId} timeout - fetching ${acc.totalCount - acc.receivedCount} missing changes`,
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
              )

              // Combine with received changes
              const allChanges = [...acc.changes, ...missingChanges].sort(
                (a, b) => (a.batchSeq ?? 0) - (b.batchSeq ?? 0),
              )

              // Apply the complete batch
              await applyRemoteChangesInTransactionAsync(
                allChanges,
                vaultKey,
                acc.backendId,
              )

              // Clean up
              batchAccumulators.value.delete(batchId)

              console.log(
                `✅ Applied batch ${batchId} after fetching missing changes`,
              )
            } catch (error) {
              console.error(`Failed to fetch missing batch ${batchId}:`, error)
              // Last resort: trigger full pull
              pullFromBackendAsync(acc.backendId).catch((pullError) => {
                console.error('Fallback pull also failed:', pullError)
              })
            }
          }, 10000) // 10 second timeout
        }
      } catch (error) {
        console.error('Failed to handle realtime change:', error)
        // Fallback: trigger a full pull if direct processing fails
        console.log('Falling back to full pull...')
        try {
          await pullFromBackendAsync(backendId)
        } catch (pullError) {
          console.error('Fallback pull also failed:', pullError)
        }
      }
    }

    /**
     * Subscribes to realtime changes from a backend
     */
    const subscribeToBackendAsync = async (
      backendId: string,
    ): Promise<void> => {
      if (!currentVaultId.value) {
        throw new Error('No vault opened')
      }

      // Get the vaultId for this backend from the backend configuration
      const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
      if (!backend?.vaultId) {
        throw new Error('Backend vaultId not configured')
      }

      const state = syncStates.value[backendId]
      if (!state) {
        throw new Error('Backend not initialized')
      }

      if (state.subscription) {
        console.log(`Already subscribed to backend ${backendId}`)
        return
      }

      const client = syncEngineStore.supabaseClient
      if (!client) {
        throw new Error('Supabase client not initialized')
      }

      try {
        // Subscribe to sync changes table for this vault (using backend vaultId)
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
              handleRealtimeChangeAsync(backendId, payload).catch(console.error)
            },
          )
          .subscribe((status) => {
            if (status === 'SUBSCRIBED') {
              state.isConnected = true
              console.log(`Subscribed to backend ${backendId}`)
            } else if (status === 'CHANNEL_ERROR' || status === 'TIMED_OUT') {
              state.isConnected = false
              state.error = `Subscription error: ${status}`
              console.error(
                `Subscription to backend ${backendId} failed: ${status}`,
              )
            }
          })

        state.subscription = channel
      } catch (error) {
        console.error(`Failed to subscribe to backend ${backendId}:`, error)
        state.error = error instanceof Error ? error.message : 'Unknown error'
        throw error
      }
    }

    /**
     * Unsubscribes from realtime changes
     */
    const unsubscribeFromBackendAsync = async (
      backendId: string,
    ): Promise<void> => {
      const state = syncStates.value[backendId]
      if (!state || !state.subscription) {
        return
      }

      try {
        await state.subscription.unsubscribe()
        state.subscription = null
        state.isConnected = false
        console.log(`Unsubscribed from backend ${backendId}`)
      } catch (error) {
        console.error(`Failed to unsubscribe from backend ${backendId}:`, error)
      }
    }

    /**
     * Initializes sync for a backend
     */
    const initBackendAsync = async (backendId: string): Promise<void> => {
      if (syncStates.value[backendId]) {
        console.log(`Backend ${backendId} already initialized`)
        return
      }

      // Initialize state
      syncStates.value[backendId] = {
        isConnected: false,
        isSyncing: false,
        error: null,
        subscription: null,
      }

      try {
        // Initial pull to get all existing data from server
        try {
          await pullFromBackendAsync(backendId)
        } catch (pullError) {
          console.error(
            `Failed to pull during init for backend ${backendId}:`,
            pullError,
          )
          addToast({
            color: 'error',
            description: `Sync pull failed: ${pullError instanceof Error ? pullError.message : 'Unknown error'}`,
          })
          throw pullError
        }

        // Push any pending local changes (dirty tables)
        try {
          await pushToBackendAsync(backendId)
        } catch (pushError) {
          console.error(
            `Failed to push during init for backend ${backendId}:`,
            pushError,
          )
          addToast({
            color: 'error',
            description: `Sync push failed: ${pushError instanceof Error ? pushError.message : 'Unknown error'}`,
          })
          throw pushError
        }

        // Subscribe to realtime changes
        await subscribeToBackendAsync(backendId)

        // Start periodic pull as fallback (every 5 minutes)
        const periodicPullInterval = setInterval(
          async () => {
            try {
              console.log(`Periodic pull for backend ${backendId}`)
              await pullFromBackendAsync(backendId)
            } catch (error) {
              console.error(
                `Periodic pull failed for backend ${backendId}:`,
                error,
              )
            }
          },
          5 * 60 * 1000,
        ) // 5 minutes

        periodicPullIntervals.set(backendId, periodicPullInterval)
      } catch (error) {
        console.error(`Failed to initialize backend ${backendId}:`, error)
        throw error
      }
    }

    /**
     * Called after local write operations to push changes
     */
    const onLocalWriteAsync = async (): Promise<void> => {
      try {
        // Push to all enabled backends in parallel
        const enabledBackends = syncBackendsStore.enabledBackends

        await Promise.allSettled(
          enabledBackends.map((backend) => pushToBackendAsync(backend.id)),
        )
      } catch (error) {
        console.error('Failed to push local changes:', error)
      }
    }

    /**
     * Handles dirty tables event from Rust - triggers sync based on configuration
     */
    const onDirtyTablesChangedAsync = async (): Promise<void> => {
      const config = syncConfigStore.config

      if (config.mode === 'continuous') {
        // In continuous mode, debounce to batch rapid changes
        if (dirtyTablesDebounceTimer) {
          clearTimeout(dirtyTablesDebounceTimer)
        }

        dirtyTablesDebounceTimer = setTimeout(async () => {
          console.log('Debounce timer elapsed, triggering sync...')
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
      stopDirtyTablesWatcher()

      const config = syncConfigStore.config

      // Listen to Tauri event from Rust
      eventUnlisten = await listen('crdt:dirty-tables-changed', async () => {
        await onDirtyTablesChangedAsync()
      })

      console.log(`Started dirty tables watcher in ${config.mode} mode`)

      if (config.mode === 'periodic') {
        // In periodic mode, sync at regular intervals
        periodicSyncInterval = setInterval(async () => {
          console.log('Periodic sync timer elapsed, triggering sync...')
          await onLocalWriteAsync()
        }, config.periodicIntervalMs)

        console.log(
          `Periodic sync interval set to ${config.periodicIntervalMs}ms`,
        )
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

      console.log('Stopped dirty tables watcher')
    }

    /**
     * Starts sync for all enabled backends
     */
    const startSyncAsync = async (): Promise<void> => {
      const enabledBackends = syncBackendsStore.enabledBackends

      if (enabledBackends.length === 0) {
        console.log('No enabled backends to sync with')
        return
      }

      console.log(`Starting sync with ${enabledBackends.length} backends`)

      // Load sync configuration
      await syncConfigStore.loadConfigAsync()

      // Start dirty tables watcher
      await startDirtyTablesWatcherAsync()

      for (const backend of enabledBackends) {
        try {
          await initBackendAsync(backend.id)
        } catch (error) {
          console.error(
            `Failed to start sync with backend ${backend.id}:`,
            error,
          )
        }
      }
    }

    /**
     * Stops sync for all backends
     */
    const stopSyncAsync = async (): Promise<void> => {
      console.log('Stopping sync for all backends')

      // Stop dirty tables watcher
      stopDirtyTablesWatcher()

      // Stop all periodic pull intervals
      for (const [backendId, interval] of periodicPullIntervals.entries()) {
        clearInterval(interval)
        periodicPullIntervals.delete(backendId)
      }

      for (const backendId of Object.keys(syncStates.value)) {
        await unsubscribeFromBackendAsync(backendId)
      }

      syncStates.value = {}
    }

    /**
     * Gets sync state for a specific backend
     */
    const getSyncState = (backendId: string): SyncState | null => {
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

    return {
      syncStates,
      isAnySyncing,
      areAllConnected,
      pushToBackendAsync,
      pullFromBackendAsync,
      subscribeToBackendAsync,
      unsubscribeFromBackendAsync,
      initBackendAsync,
      onLocalWriteAsync,
      startSyncAsync,
      stopSyncAsync,
      getSyncState,
    }
  },
)
