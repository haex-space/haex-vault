// src/stores/vault/index.ts

import { drizzle } from 'drizzle-orm/sqlite-proxy'
import { eq } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { schema } from '~/database'
import type {
  AsyncRemoteCallback,
  SqliteRemoteDatabase,
} from 'drizzle-orm/sqlite-proxy'
import type { CleanupResult } from '~~/src-tauri/bindings/CleanupResult'
import { didAuthenticateAsync } from '~/stores/sync/engine/tokenManager'
import { loadUcansFromDbAsync } from '~/utils/auth/ucanStore'

interface IVault {
  name: string
  drizzle: SqliteRemoteDatabase<typeof schema>
  password: string
}
interface IOpenVaults {
  [vaultId: string]: IVault
}

export const useVaultStore = defineStore('vaultStore', () => {
  const {
    public: { haexVault },
  } = useRuntimeConfig()

  const router = useRouter()
  const currentVaultId = computed<string | undefined>({
    get: () =>
      getSingleRouteParam(router.currentRoute.value.params.vaultId),
    set: (newVaultId) => {
      router.currentRoute.value.params.vaultId = newVaultId ?? ''
    },
  })

  const currentVaultName = ref(haexVault.defaultVaultName || 'HaexSpace')

  const openVaults = ref<IOpenVaults>({})

  const currentVault = computed(
    () => openVaults.value?.[currentVaultId.value ?? ''],
  )

  /**
   * Reset all vault-specific stores to prevent stale data leaking between vaults.
   * Called from closeAsync and from the currentVault watcher as a safety net.
   */
  const resetAllVaultStores = () => {
    // Disable console logger first to prevent IPC errors during teardown
    const { $disableConsoleLogger } = useNuxtApp()
    if ($disableConsoleLogger) $disableConsoleLogger()

    useDesktopStore().reset()
    useExtensionsStore().reset()
    useWorkspaceStore().reset()
    useIdentityStore().reset()
    useDeviceStore().reset()
    useNotificationStore().reset()
    useNavigationStore().reset()
    useExtensionBroadcastStore().cleanup()
    useExtensionReadyStore().resetAll()
    useSpacesStore().clearCache()
    useSyncBackendsStore().reset()
    useSyncConfigStore().reset()
    usePeerStorageStore().reset()
  }

  // Watch for vault becoming unavailable (e.g., webview reload, navigation away)
  // This is the safety net: no matter HOW the vault disappears, stores get reset
  watch(currentVault, async (newVault) => {
    if (!newVault) {
      const windowManagerStore = useWindowManagerStore()
      await windowManagerStore.closeAllWindowsAsync()
      resetAllVaultStores()
    }
  }, { immediate: true })

  // Vault password from the currently open vault
  // Used for sync key encryption/decryption and vault name updates on server
  const currentVaultPassword = computed(
    () => currentVault.value?.password ?? null,
  )

  /**
   * Attempts to auto-login via challenge-response and start sync for all enabled backends
   */
  const autoLoginAndStartSyncAsync = async () => {
    try {
      const syncBackendsStore = useSyncBackendsStore()
      const syncEngineStore = useSyncEngineStore()
      const syncOrchestratorStore = useSyncOrchestratorStore()
      const identityStore = useIdentityStore()

      // Load all backends from database
      await syncBackendsStore.loadBackendsAsync()

      // Ensure identities are loaded
      await identityStore.loadIdentitiesAsync()

      const enabledBackends = syncBackendsStore.enabledBackends

      for (const backend of enabledBackends) {
        try {
          const identity = await identityStore.getIdentityByIdAsync(backend.identityId)
          if (!identity) {
            continue
          }

          // Initialize token manager and authenticate via DID challenge-response
          syncEngineStore.initTokenManagerAsync(backend.id)
          const session = await didAuthenticateAsync(backend.homeServerUrl, identity.did, identity.privateKey!)
          syncEngineStore.setSession(backend.id, session)

          // Ensure sync key exists
          if (backend.spaceId && currentVault.value?.name && currentVaultPassword.value) {
            await syncEngineStore.ensureSyncKeyAsync(
              backend.id,
              backend.spaceId,
              currentVault.value.name,
              currentVaultPassword.value,
            )
          } else if (!backend.spaceId) {
            console.warn(`[HaexSpace] Backend ${backend.name} has no spaceId configured`)
          }
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error)
          console.error(`[HaexSpace] Auto-login error for ${backend.name}:`, errorMessage)
        }
      }

      // Retry pending vault key updates (from previous password changes that failed)
      if (currentVaultPassword.value) {
        try {
          const { failedBackendIds } = await syncEngineStore.retryPendingVaultKeyUpdatesAsync(
            syncEngineStore.vaultKeyCache[currentVaultId.value ?? '']?.vaultKey ?? new Uint8Array(),
            currentVaultPassword.value,
          )

          if (failedBackendIds.length > 0) {
            console.warn(`[HaexSpace] Vault key update still pending for ${failedBackendIds.length} backend(s)`)
            const { add } = useToast()
            add({
              color: 'warning',
              title: 'Sync-Server nicht erreichbar',
              description: `${failedBackendIds.length} Server konnte(n) nicht aktualisiert werden. Die Aktualisierung wird erneut versucht, sobald die Server erreichbar sind.`,
            })
          }
        } catch (error) {
          console.error('[HaexSpace] Error retrying pending vault key updates:', error)
        }
      }

      // Start sync (initializes local sync listener + remote backends if any)
      await syncOrchestratorStore.startSyncAsync()
    } catch (error) {
      console.error('[HaexSpace] Auto-login and sync start error:', error)
    }
  }

  const openAsync = async ({
    path = '',
    password,
    vaultId: providedVaultId,
  }: {
    path: string
    password: string
    /** Optional: Use this vault ID instead of reading from DB (for remote sync) */
    vaultId?: string
  }) => {
    try {
      await invoke<string>('open_encrypted_database', {
        vaultPath: path,
        key: password,
      })

      const drizzleDb = drizzle<typeof schema>(drizzleCallback, {
        schema: schema,
        logger: false,
      })

      // For remote sync: use provided vaultId, skip DB lookup/creation
      // For normal open: read vaultId from DB (or create if not exists)
      const vaultId = providedVaultId ?? await getVaultIdAsync(drizzleDb)

      const fileName = getFileName(path) ?? path

      openVaults.value = {
        ...openVaults.value,
        [vaultId]: {
          name: fileName,
          drizzle: drizzleDb,
          password,
        },
      }

      return vaultId
    } catch (error) {
      console.error('Error openAsync ', error)
      throw error
    }
  }

  /**
   * Performs automatic cleanup of old tombstones and applied CRDT entries.
   * Default retention: 30 days for tombstones.
   */
  const performAutomaticCleanupAsync = async (
    retentionDays: number = 30,
  ): Promise<CleanupResult | null> => {
    try {
      const result = await invoke<CleanupResult>('crdt_cleanup_tombstones', {
        retentionDays,
      })

      // Also clean up old log entries
      await invoke<number>('log_cleanup')

      return result
    } catch (error) {
      console.error('[HaexSpace] Automatic cleanup error:', error)
      return null
    }
  }

  const createAsync = async ({
    vaultName,
    password,
    spaceId,
  }: {
    vaultName: string
    password: string
    /** Optional: Set a specific space ID (for connecting to remote vaults) */
    spaceId?: string
  }) => {
    const vaultPath = await invoke<string>('create_encrypted_database', {
      vaultName,
      key: password,
      spaceId: spaceId || null,
    })

    // Set the user-provided vault name BEFORE opening
    // This ensures syncVaultNameAsync uses the correct name when creating the DB entry
    currentVaultName.value = vaultName

    return await openAsync({ path: vaultPath, password })
  }

  const closeAsync = async () => {
    if (!currentVaultId.value) return

    // Stop P2P endpoint first
    const peerStorageStore = usePeerStorageStore()
    await peerStorageStore.stopAsync()

    // Stop sync to clear all sync-related state
    const syncOrchestratorStore = useSyncOrchestratorStore()
    await syncOrchestratorStore.stopSyncAsync()

    // Reset backends store (separate from stopSync so disabling a backend doesn't clear the list)
    const syncBackendsStore = useSyncBackendsStore()
    syncBackendsStore.reset()

    // Sync engine cleanup (token manager reset)
    const syncEngineStore = useSyncEngineStore()
    await syncEngineStore.reset()

    // Close the database connection on the Rust side
    // This clears the DB connection, HLC service, and extension manager caches
    try {
      await invoke('close_database')
    } catch (error) {
      console.error('[VAULT STORE] Failed to close database:', error)
    }

    // Removing vault from openVaults triggers the currentVault watcher,
    // which calls resetAllVaultStores() and closeAllWindowsAsync() as safety net
    delete openVaults.value?.[currentVaultId.value]
  }

  const existsVault = () => {
    if (!currentVault.value?.drizzle) {
      console.error('Kein Vault geöffnet')
      return
    }
  }

  /**
   * Checks if a vault with the given name already exists
   */
  const vaultExistsAsync = async (vaultName: string): Promise<boolean> => {
    try {
      const result = await invoke<boolean>('vault_exists', { vaultName })
      return result
    } catch (error) {
      console.error('Failed to check if vault exists:', error)
      return false
    }
  }

  /**
   * Changes the vault password.
   * This re-encrypts the local database and updates the vault key on all sync backends.
   *
   * @param currentPassword - The current vault password (for verification)
   * @param newPassword - The new vault password
   * @returns Object with success status and count of backends that need retry
   */
  const changePasswordAsync = async (
    currentPassword: string,
    newPassword: string,
  ): Promise<{ success: boolean; pendingBackends: number; error?: string }> => {
    if (!currentVaultId.value || !currentVault.value) {
      return { success: false, pendingBackends: 0, error: 'No vault opened' }
    }

    // Verify current password matches
    if (currentPassword !== currentVaultPassword.value) {
      return { success: false, pendingBackends: 0, error: 'Current password is incorrect' }
    }

    const syncEngineStore = useSyncEngineStore()
    const syncBackendsStore = useSyncBackendsStore()
    const { add: addToast } = useToast()

    try {
      // Step 1: Change local database password using SQLCipher rekey
      await invoke('change_vault_password', { newPassword })

      // Step 2: Update password in memory
      const vaultId = currentVaultId.value
      if (vaultId && openVaults.value[vaultId]) {
        openVaults.value[vaultId]!.password = newPassword
      }

      // Step 3: Re-encrypt vault key on all enabled sync backends
      const enabledBackends = syncBackendsStore.enabledBackends
      let pendingBackends = 0

      if (enabledBackends.length > 0) {
        // Get the current vault key from cache
        const cachedKey = syncEngineStore.vaultKeyCache[vaultId ?? '']
        if (!cachedKey?.vaultKey) {
          console.warn('[VAULT STORE] No vault key in cache, backends will be updated on next sync')
          // Mark all backends as pending
          for (const backend of enabledBackends) {
            await syncEngineStore.markBackendPendingVaultKeyUpdateAsync(backend.id, true)
          }
          pendingBackends = enabledBackends.length
        } else {
          // Try to update each backend
          for (const backend of enabledBackends) {
            if (!backend.spaceId) continue

            const success = await syncEngineStore.reEncryptVaultKeyOnBackendAsync(
              backend.id,
              backend.spaceId,
              cachedKey.vaultKey,
              newPassword,
            )

            if (!success) {
              // Mark for retry later
              await syncEngineStore.markBackendPendingVaultKeyUpdateAsync(backend.id, true)
              pendingBackends++
            }
          }
        }

        if (pendingBackends > 0) {
          addToast({
            color: 'warning',
            title: 'Sync-Server teilweise aktualisiert',
            description: `${pendingBackends} Server konnte(n) nicht erreicht werden. Die Aktualisierung wird beim nächsten Start erneut versucht.`,
          })
        }
      }

      return { success: true, pendingBackends }
    } catch (error) {
      console.error('[VAULT STORE] Failed to change password:', error)
      return {
        success: false,
        pendingBackends: 0,
        error: error instanceof Error ? error.message : 'Unknown error',
      }
    }
  }

  /**
   * Initialize vault after navigation to /vault/:vaultId.
   * Called from vault.vue onMounted — at this point currentVaultId is set
   * via the router and getDb() works correctly.
   */
  const initVaultAsync = async () => {
    if (!currentVaultId.value) return

    // Initialize device identity key
    await useDeviceStore().initDeviceIdAsync()

    // Set device ID for console interceptor logging
    const { $setConsoleLoggerDeviceId } = useNuxtApp()
    if ($setConsoleLoggerDeviceId && useDeviceStore().deviceId) {
      $setConsoleLoggerDeviceId(useDeviceStore().deviceId!)
    }

    // Initialize MLS subsystem (tables + identity) — must happen before any space
    // creation because createLocalSpaceAsync calls mls_create_group which needs identity
    await invoke('mls_init_tables')
    await invoke('mls_init_identity')

    // Ensure vault space exists in haex_spaces (FK target for sync backends)
    const spacesStore = useSpacesStore()
    await spacesStore.ensureVaultSpaceAsync(currentVaultId.value, currentVaultName.value)

    // Ensure at least one identity exists (needed for UCAN signing in spaces)
    const identityStore = useIdentityStore()
    await identityStore.ensureDefaultIdentityAsync()

    // Update device claims now that identity exists (initDeviceIdAsync ran before identity was created)
    if (useDeviceStore().deviceId) {
      await useDeviceStore().updateDeviceClaimsAsync()
    }

    // Load spaces from DB and ensure default local space exists
    await spacesStore.loadSpacesFromDbAsync()
    await spacesStore.ensureDefaultSpaceAsync()

    // Start leader mode for all local spaces (enables invite handling)
    await spacesStore.startLocalSpaceLeadersAsync()

    // Warm UCAN cache from DB (tokens survive app restarts)
    if (currentVault.value?.drizzle) {
      await loadUcansFromDbAsync(currentVault.value.drizzle)
    }

    // Auto-enroll this device into MLS groups for shared spaces (non-blocking)
    const deviceStore = useDeviceStore()
    if (deviceStore.deviceId) {
      const { useDeviceEnrollment } = await import('@/composables/useDeviceEnrollment')
      const { syncEnrollmentsAsync } = useDeviceEnrollment()
      syncEnrollmentsAsync(deviceStore.deviceId).catch((error) => {
        console.warn('[HaexSpace] Device MLS enrollment failed:', error)
      })
    }

    // Automatic cleanup (non-blocking)
    performAutomaticCleanupAsync().catch((error) => {
      console.warn('[HaexSpace] Automatic cleanup failed:', error)
    })
  }

  return {
    closeAsync,
    createAsync,
    currentVault,
    currentVaultId,
    currentVaultName,
    currentVaultPassword,
    existsVault,
    initVaultAsync,
    openAsync,
    openVaults,
    autoLoginAndStartSyncAsync,
    vaultExistsAsync,
    changePasswordAsync,
  }
})

/**
 * Gets or creates a UUID for this vault
 * The UUID is stored in haex_settings and persists across sessions
 */
const getVaultIdAsync = async (
  drizzleDb: SqliteRemoteDatabase<typeof schema>,
): Promise<string> => {
  const { haexVaultSettings } = schema

  // Try to get existing vault ID from settings
  const existingSettings = await drizzleDb
    .select()
    .from(haexVaultSettings)
    .where(eq(haexVaultSettings.key, 'space_id'))
    .limit(1)

  if (existingSettings[0]?.value) {
    return existingSettings[0].value
  }

  // Generate new UUID for this vault
  const vaultId = crypto.randomUUID()

  // Store it in settings
  await drizzleDb.insert(haexVaultSettings).values({
    key: 'space_id',
    value: vaultId,
  })

  return vaultId
}

/**
 * Unified Drizzle callback using sql_with_crdt
 *
 * The Rust backend (sql_with_crdt) handles all SQL statement type detection
 * via AST parsing - no string matching needed here.
 *
 * - SELECT: Automatically filtered for tombstones
 * - INSERT/UPDATE/DELETE: CRDT timestamps applied, RETURNING handled correctly
 */
const drizzleCallback = (async (
  sql: string,
  params: unknown[],
  method: 'get' | 'run' | 'all' | 'values',
) => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let rows: any[] = []

  try {
    // Single unified command - Rust handles statement type detection via AST
    rows = await invoke<unknown[]>('sql_with_crdt', {
      sql,
      params,
    })
  } catch (error) {
    console.error('SQL Error:', error, { sql, params, method })
    throw error
  }

  if (method === 'get') {
    // For 'get' method (used by findFirst), return the first row or undefined
    // IMPORTANT: Must return undefined (not empty array) when no rows found,
    // otherwise Drizzle interprets [] as a valid result and returns {}
    return { rows: rows.at(0) }
  }
  return { rows }
}) satisfies AsyncRemoteCallback
