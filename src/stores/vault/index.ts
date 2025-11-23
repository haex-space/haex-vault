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
          if (currentVaultId.value && currentVault.value?.name) {
            await syncEngineStore.ensureSyncKeyAsync(
              backend.id,
              currentVaultId.value,
              currentVault.value.name,
              backend.password,
            )
          }
        } catch (error) {
          console.error(`[HaexSpace] Auto-login error for ${backend.name}:`, error)
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
  }: {
    path: string
    password: string
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

      const vaultId = await getVaultIdAsync(drizzleDb)

      const fileName = getFileName(path) ?? path

      openVaults.value = {
        ...openVaults.value,
        [vaultId]: {
          name: fileName,
          drizzle: drizzleDb,
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
  }: {
    vaultName: string
    password: string
  }) => {
    const vaultPath = await invoke<string>('create_encrypted_database', {
      vaultName,
      key: password,
    })
    return await openAsync({ path: vaultPath, password })
  }

  const closeAsync = async () => {
    if (!currentVaultId.value) return

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

  return {
    closeAsync,
    createAsync,
    currentVault,
    currentVaultId,
    currentVaultName,
    existsVault,
    openAsync,
    openVaults,
    autoLoginAndStartSyncAsync,
    vaultExistsAsync,
  }
})

/**
 * Gets or creates a UUID for this vault
 * The UUID is stored in haex_settings and persists across sessions
 */
const getVaultIdAsync = async (
  drizzleDb: SqliteRemoteDatabase<typeof schema>,
): Promise<string> => {
  const { haexSettings } = schema

  // Try to get existing vault ID from settings
  const existingSettings = await drizzleDb
    .select()
    .from(haexSettings)
    .where(eq(haexSettings.key, 'vault_id'))
    .limit(1)

  if (existingSettings[0]?.value) {
    return existingSettings[0].value
  }

  // Generate new UUID for this vault
  const vaultId = crypto.randomUUID()

  // Store it in settings
  await drizzleDb.insert(haexSettings).values({
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
