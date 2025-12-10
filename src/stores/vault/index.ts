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

  // Vault password from the currently open vault
  // Used for sync key encryption/decryption and vault name updates on server
  const currentVaultPassword = computed(
    () => currentVault.value?.password ?? null,
  )

  /**
   * Attempts to auto-login and start sync for all enabled backends with saved credentials
   */
  const autoLoginAndStartSyncAsync = async () => {
    try {
      const syncBackendsStore = useSyncBackendsStore()
      const syncEngineStore = useSyncEngineStore()
      const syncOrchestratorStore = useSyncOrchestratorStore()

      // Load all backends from database
      await syncBackendsStore.loadBackendsAsync()

      const enabledBackends = syncBackendsStore.enabledBackends

      if (enabledBackends.length === 0) {
        console.log('[HaexSpace] No enabled sync backends found')
        return
      }

      for (const backend of enabledBackends) {
        try {
          // Check if backend has credentials
          if (!backend.email || !backend.password) {
            console.log(`[HaexSpace] No credentials for backend ${backend.name}`)
            continue
          }

          console.log(`[HaexSpace] Auto-login for backend ${backend.name}...`)

          // Initialize Supabase client
          await syncEngineStore.initSupabaseClientAsync(backend.id)

          if (!syncEngineStore.supabaseClient) {
            console.warn(`[HaexSpace] Failed to initialize Supabase for ${backend.name}`)
            continue
          }

          // Attempt login with saved credentials
          const { error } = await syncEngineStore.supabaseClient.auth.signInWithPassword({
            email: backend.email,
            password: backend.password,
          })

          if (error) {
            console.error(`[HaexSpace] Auto-login failed for ${backend.name}:`, error.message)
            continue
          }

          console.log(`[HaexSpace] ✅ Auto-login successful for ${backend.name}`)

          // Ensure sync key exists
          // Use vault password (from memory) for sync key decryption
          if (currentVaultId.value && currentVault.value?.name && currentVaultPassword.value) {
            await syncEngineStore.ensureSyncKeyAsync(
              backend.id,
              currentVaultId.value,
              currentVault.value.name,
              currentVaultPassword.value, // Vault password for sync key decryption
            )
          }
        } catch (error) {
          console.error(`[HaexSpace] Auto-login error for ${backend.name}:`, error)
        }
      }

      // Retry pending vault key updates (from previous password changes that failed)
      if (currentVaultPassword.value) {
        try {
          const { successCount, failedBackendIds } = await syncEngineStore.retryPendingVaultKeyUpdatesAsync(
            syncEngineStore.vaultKeyCache[currentVaultId.value ?? '']?.vaultKey ?? new Uint8Array(),
            currentVaultPassword.value,
          )

          if (successCount > 0) {
            console.log(`[HaexSpace] ✅ Retried vault key update for ${successCount} backend(s)`)
          }

          if (failedBackendIds.length > 0) {
            console.warn(`[HaexSpace] ⚠️ Vault key update still pending for ${failedBackendIds.length} backend(s)`)
            // Show toast to user
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
        console.log('[HaexSpace] ✅ Sync started with auto-login')
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
      console.log('[VAULT STORE] openAsync called with path:', path)
      if (providedVaultId) {
        console.log('[VAULT STORE] Using provided vault ID (remote sync):', providedVaultId)
      }
      console.log('[VAULT STORE] Invoking open_encrypted_database...')

      await invoke<string>('open_encrypted_database', {
        vaultPath: path,
        key: password,
      })

      console.log('[VAULT STORE] open_encrypted_database completed')

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

      if (result.totalDeleted > 0) {
        console.log(
          `[HaexSpace] Automatic cleanup completed: ${result.totalDeleted} entries removed ` +
            `(${result.tombstonesDeleted} tombstones, ${result.appliedDeleted} applied)`,
        )
      }

      return result
    } catch (error) {
      console.error('[HaexSpace] Automatic cleanup error:', error)
      return null
    }
  }

  const createAsync = async ({
    vaultName,
    password,
    vaultId,
  }: {
    vaultName: string
    password: string
    /** Optional: Set a specific vault ID (for connecting to remote vaults) */
    vaultId?: string
  }) => {
    console.log('[VAULT STORE] createAsync called with vaultName:', vaultName)
    if (vaultId) {
      console.log('[VAULT STORE] Using provided vault ID:', vaultId)
    }
    console.log('[VAULT STORE] Invoking create_encrypted_database...')

    const vaultPath = await invoke<string>('create_encrypted_database', {
      vaultName,
      key: password,
      vaultId: vaultId || null,
    })

    console.log('[VAULT STORE] create_encrypted_database returned path:', vaultPath)
    console.log('[VAULT STORE] Now calling openAsync...')

    // Pass vaultId to openAsync so it doesn't create a new one from DB
    return await openAsync({ path: vaultPath, password, vaultId })
  }

  const closeAsync = async () => {
    if (!currentVaultId.value) return

    // Close all extension windows before closing the vault
    const windowManagerStore = useWindowManagerStore()
    await windowManagerStore.closeAllExtensionWindowsAsync()

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
      console.log('[VAULT STORE] Changing local vault password...')
      await invoke('change_vault_password', { newPassword })
      console.log('[VAULT STORE] ✅ Local vault password changed')

      // Step 2: Update password in memory
      const vaultId = currentVaultId.value
      if (vaultId && openVaults.value[vaultId]) {
        openVaults.value[vaultId]!.password = newPassword
      }

      // Step 3: Re-encrypt vault key on all enabled sync backends
      const enabledBackends = syncBackendsStore.enabledBackends
      let pendingBackends = 0

      if (enabledBackends.length > 0) {
        console.log(`[VAULT STORE] Updating vault key on ${enabledBackends.length} sync backend(s)...`)

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
            if (!backend.vaultId) continue

            const success = await syncEngineStore.reEncryptVaultKeyOnBackendAsync(
              backend.id,
              backend.vaultId,
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

      console.log('[VAULT STORE] ✅ Password change complete')
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
    .where(eq(haexVaultSettings.key, 'vault_id'))
    .limit(1)

  if (existingSettings[0]?.value) {
    return existingSettings[0].value
  }

  // Generate new UUID for this vault
  const vaultId = crypto.randomUUID()

  // Store it in settings
  await drizzleDb.insert(haexVaultSettings).values({
    key: 'vault_id',
    type: 'system',
    value: vaultId,
  })

  return vaultId
}

const isSelectQuery = (sql: string) => {
  const selectRegex = /^\s*SELECT\b/i
  return selectRegex.test(sql)
}

const hasReturning = (sql: string) => {
  const returningRegex = /\bRETURNING\b/i
  return returningRegex.test(sql)
}

const drizzleCallback = (async (
  sql: string,
  params: unknown[],
  method: 'get' | 'run' | 'all' | 'values',
) => {
  // Wir MÜSSEN 'any[]' verwenden, um Drizzle's Typ zu erfüllen.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let rows: any[] = []

  try {
    if (isSelectQuery(sql)) {
      // SELECT statements
      rows = await invoke<unknown[]>('sql_select_with_crdt', {
        sql,
        params,
      }).catch((e) => {
        console.error('SQL select Error:', e, sql, params)
        throw e // Re-throw the error so it can be caught by the caller
      })
    } else if (hasReturning(sql)) {
      // INSERT/UPDATE/DELETE with RETURNING → use query
      rows = await invoke<unknown[]>('sql_query_with_crdt', {
        sql,
        params,
      }).catch((e) => {
        console.error('SQL query with CRDT Error:', e, sql, params)
        throw e // Re-throw the error so it can be caught by the caller
      })
    } else {
      // INSERT/UPDATE/DELETE without RETURNING → use execute
      await invoke<unknown[]>('sql_execute_with_crdt', {
        sql,
        params,
      }).catch((e) => {
        console.error('SQL execute with CRDT Error:', e, sql, params, rows)
        throw e // Re-throw the error so it can be caught by the caller
      })
    }
  } catch (error) {
    console.error('Fehler im drizzleCallback invoke:', error, {
      sql,
      params,
      method,
    })
    throw error // Re-throw the error so it can be caught by the caller
  }

  /* console.log('drizzleCallback', method, sql, params)
  console.log('drizzleCallback rows', rows, rows.slice(0, 1)) */

  if (method === 'get') {
    return rows.length > 0 ? { rows: rows.at(0) } : { rows }
  }
  return { rows }
}) satisfies AsyncRemoteCallback
