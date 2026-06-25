/**
 * Initial Sync
 * Streams the full server state into a freshly-connected vault before the
 * sync backend is persisted, then reloads the stores so the UI can render.
 */

import type { Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { streamPullAndApplyAsync } from './pull/page'
import { orchestratorLog as log, type BackendSyncState } from './types'

export interface InitialSyncDeps {
  currentVaultId: Ref<string | null | undefined>
  syncStates: Ref<BackendSyncState>
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>
  syncEngineStore: ReturnType<typeof useSyncEngineStore>
}

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
export const performInitialPullAsync = async (deps: InitialSyncDeps): Promise<void> => {
  const { currentVaultId, syncStates, syncBackendsStore, syncEngineStore } = deps

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
  }

  try {
    // Get vault key from cache
    const vaultKey = syncEngineStore.vaultKeyCache[tempBackend.spaceId]?.vaultKey
    if (!vaultKey) {
      log.error('INITIAL PULL FAILED: Vault key not available')
      throw new Error('Vault key not available. Please unlock vault first.')
    }

    log.debug('Initial pull config:', {
      backendId,
      spaceId: tempBackend.spaceId,
      homeServerUrl: tempBackend.homeServerUrl,
    })

    // Stream pages from the server, applying each per-HLC-group with
    // cross-page hold-back. No `onPageCommitted` callback: the temporary
    // backend isn't persisted yet, so cursor advances happen at the end via
    // `persistedBackend` updates below. On mid-stream failure the partial
    // commits on disk remain (idempotent under CRDT LWW) and the user can
    // retry the initial sync.
    log.info('Streaming all changes from server (initial pull)...')
    const streamResult = await streamPullAndApplyAsync({
      homeServerUrl: tempBackend.homeServerUrl,
      spaceId: tempBackend.spaceId,
      initialCursor: null,
      encryptionKey: vaultKey,
      backendId,
      backendIdentityId: tempBackend.identityId,
    })

    const { totalApplied, pageCount, tablesAffected, maxHlc, lastServerTimestamp: serverTimestamp } = streamResult
    if (totalApplied === 0) {
      log.info('INITIAL PULL: No data on server (empty vault)')
    } else {
      log.info(
        `INITIAL PULL: Streamed ${totalApplied} changes across ${pageCount} pages ` +
        `(tables: ${tablesAffected.size})`,
      )
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
    const persistedBackend = await syncBackendsStore.findBackendByServerUrlAsync(
      tempBackend.homeServerUrl,
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

    log.info(`========== INITIAL PULL SUCCESS: ${totalApplied} changes applied ==========`)
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
