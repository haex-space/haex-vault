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

  // Watch for vault becoming unavailable (e.g., webview reload, explicit close)
  // Close all extension windows when no vault is available
  watch(currentVault, async (newVault) => {
    if (!newVault) {
      const windowManagerStore = useWindowManagerStore()
      await windowManagerStore.closeAllWindowsAsync()
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

      if (enabledBackends.length === 0) {
        return
      }

      for (const backend of enabledBackends) {
        try {
          // Check if backend has an identity for auth
          if (!backend.identityId) {
            continue
          }

          const identity = await identityStore.getIdentityAsync(backend.identityId)
          if (!identity) {
            continue
          }

          // Initialize Supabase client
          await syncEngineStore.initSupabaseClientAsync(backend.id)

          if (!syncEngineStore.supabaseClient) {
            console.warn(`[HaexSpace] Failed to initialize Supabase for ${backend.name}`)
            continue
          }

          // 1. Request challenge
          const challengeRes = await fetch(`${backend.serverUrl}/identity-auth/challenge`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ did: identity.did }),
          })

          if (!challengeRes.ok) {
            const errorData = await challengeRes.json().catch(() => ({ error: 'Unknown error' }))
            console.error(`[HaexSpace] Challenge request failed for ${backend.name}:`, errorData.error)
            continue
          }

          const { nonce } = await challengeRes.json()

          // 2. Sign nonce with identity's private key
          const { importUserPrivateKeyAsync } = await import('@haex-space/vault-sdk')
          const privateKey = await importUserPrivateKeyAsync(identity.privateKey)
          const sig = await crypto.subtle.sign(
            { name: 'ECDSA', hash: 'SHA-256' },
            privateKey,
            new TextEncoder().encode(nonce),
          )
          const signature = btoa(String.fromCharCode(...new Uint8Array(sig)))

          // 3. Verify and get JWT
          const verifyRes = await fetch(`${backend.serverUrl}/identity-auth/verify`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ did: identity.did, nonce, signature }),
          })

          if (!verifyRes.ok) {
            const errorData = await verifyRes.json().catch(() => ({ error: 'Unknown error' }))
            console.error(`[HaexSpace] Verify failed for ${backend.name}:`, errorData.error)
            continue
          }

          const session = await verifyRes.json()

          // Set the session from the server response
          await syncEngineStore.supabaseClient.auth.setSession({
            access_token: session.access_token,
            refresh_token: session.refresh_token,
          })

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

      // Start sync after all logins are attempted
      if (enabledBackends.length > 0) {
        await syncOrchestratorStore.startSyncAsync()
      }
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

      // Initialize device identity key and populate device store
      await useDeviceStore().initDeviceIdAsync()

      // Set device ID for console interceptor logging
      const { $setConsoleLoggerDeviceId } = useNuxtApp()
      if ($setConsoleLoggerDeviceId && useDeviceStore().deviceId) {
        $setConsoleLoggerDeviceId(useDeviceStore().deviceId!)
      }

      // Ensure default local space exists
      const spacesStore = useSpacesStore()
      await spacesStore.ensureDefaultSpaceAsync()

      // Automatic cleanup on vault open (non-blocking)
      performAutomaticCleanupAsync().catch((error) => {
        console.warn('[HaexSpace] Automatic cleanup failed:', error)
      })

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

    // Pass vaultId to openAsync so it doesn't create a new one from DB
    return await openAsync({ path: vaultPath, password, vaultId })
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

    // Close ALL windows (system + extension) before closing the vault
    const windowManagerStore = useWindowManagerStore()
    await windowManagerStore.closeAllWindowsAsync()

    // Reset all vault-specific stores to clear cached data
    // This prevents stale data from appearing when opening a different vault
    const desktopStore = useDesktopStore()
    const extensionsStore = useExtensionsStore()
    const workspaceStore = useWorkspaceStore()
    const syncEngineStore = useSyncEngineStore()

    desktopStore.reset()
    extensionsStore.reset()
    workspaceStore.reset()
    await syncEngineStore.reset()

    // Reset additional stores with cached vault data
    const spacesStore = useSpacesStore()
    spacesStore.clearCache()

    // Close the database connection on the Rust side
    // This clears the DB connection, HLC service, and extension manager caches
    try {
      await invoke('close_database')
    } catch (error) {
      console.error('[VAULT STORE] Failed to close database:', error)
    }

    // Removing vault from openVaults also clears the password from memory
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

  return {
    closeAsync,
    createAsync,
    currentVault,
    currentVaultId,
    currentVaultName,
    currentVaultPassword,
    existsVault,
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
    type: 'system',
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
