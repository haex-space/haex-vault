import { eq, and } from 'drizzle-orm'
import {
  haexSyncBackends,
  type InsertHaexSyncBackends,
  type SelectHaexSyncBackends,
} from '~/database/schemas'
import { createLogger } from '@/stores/logging'

const log = createLogger('SYNC BACKENDS')

export interface ISyncServerOption {
  label: string
  value: string
}

/**
 * Temporary backend configuration used during initial sync
 * before the backend is persisted to the database
 */
export interface TemporaryBackend {
  id: string
  name: string
  serverUrl: string
  vaultId: string
  email: string
  password: string
  enabled: boolean
}

export const useSyncBackendsStore = defineStore('syncBackendsStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const backends = ref<SelectHaexSyncBackends[]>([])

  /**
   * Temporary backend for initial sync (not persisted to DB yet)
   * Used when connecting to a remote vault - we need to pull data first
   * before we can safely insert the backend into the database
   */
  const temporaryBackend = ref<TemporaryBackend | null>(null)

  const enabledBackends = computed(() =>
    backends.value.filter((b) => b.enabled),
  )

  const sortedBackends = computed(() =>
    [...backends.value].sort((a, b) => (b.priority || 0) - (a.priority || 0)),
  )

  // Load all sync backends from database
  const loadBackendsAsync = async () => {
    if (!currentVault.value?.drizzle) {
      log.error('No vault opened')
      return
    }

    try {
      const result = await currentVault.value.drizzle
        .select()
        .from(haexSyncBackends)

      backends.value = result
    } catch (error) {
      log.error('Failed to load sync backends:', error)
      throw error
    }
  }

  // Add a new sync backend
  const addBackendAsync = async (backend: InsertHaexSyncBackends) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    try {
      const result = await currentVault.value.drizzle
        .insert(haexSyncBackends)
        .values(backend)
        .returning()

      if (result.length > 0 && result[0]) {
        backends.value.push(result[0])
        return result[0]
      }
    } catch (error) {
      log.error('Failed to add sync backend:', error)
      throw error
    }
  }

  // Update an existing sync backend
  const updateBackendAsync = async (
    id: string,
    updates: Partial<InsertHaexSyncBackends>,
  ) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    try {
      const result = await currentVault.value.drizzle
        .update(haexSyncBackends)
        .set(updates)
        .where(eq(haexSyncBackends.id, id))
        .returning()

      if (result.length > 0 && result[0]) {
        const index = backends.value.findIndex((b) => b.id === id)
        if (index !== -1) {
          backends.value[index] = result[0]
        }
        return result[0]
      }
    } catch (error) {
      log.error('Failed to update sync backend:', error)
      throw error
    }
  }

  // Delete a sync backend
  const deleteBackendAsync = async (id: string) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    try {
      await currentVault.value.drizzle
        .delete(haexSyncBackends)
        .where(eq(haexSyncBackends.id, id))

      backends.value = backends.value.filter((b) => b.id !== id)
    } catch (error) {
      log.error('Failed to delete sync backend:', error)
      throw error
    }
  }

  // Enable/disable a backend
  const toggleBackendAsync = async (id: string, enabled: boolean) => {
    return updateBackendAsync(id, { enabled })
  }

  // Update backend priority (for sync order)
  const updatePriorityAsync = async (id: string, priority: number) => {
    return updateBackendAsync(id, { priority })
  }

  // Find backend by server URL and email (for checking duplicates)
  const findBackendByCredentialsAsync = async (
    serverUrl: string,
    email: string,
  ): Promise<SelectHaexSyncBackends | null> => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    try {
      const result = await currentVault.value.drizzle
        .select()
        .from(haexSyncBackends)
        .where(
          and(
            eq(haexSyncBackends.serverUrl, serverUrl),
            eq(haexSyncBackends.email, email),
          ),
        )
        .limit(1)

      return result[0] ?? null
    } catch (error) {
      log.error('Failed to find backend by credentials:', error)
      throw error
    }
  }

  /**
   * Sets a temporary backend for initial sync.
   * This backend is used to perform the first pull before persisting to DB.
   */
  const setTemporaryBackend = (backend: TemporaryBackend | null) => {
    temporaryBackend.value = backend
    log.debug('Temporary backend set:', backend?.id ?? 'null')
  }

  /**
   * Clears the temporary backend after initial sync is complete.
   */
  const clearTemporaryBackend = () => {
    temporaryBackend.value = null
    log.debug('Temporary backend cleared')
  }

  /**
   * Resets the store state. Called when closing a vault.
   */
  const reset = () => {
    backends.value = []
    temporaryBackend.value = null
    log.debug('Store reset')
  }

  /**
   * Persists the temporary backend to the database after successful initial sync.
   * Checks if backend already exists (from remote data) and updates it if needed.
   */
  const persistTemporaryBackendAsync = async (): Promise<void> => {
    if (!temporaryBackend.value) {
      log.debug('No temporary backend to persist')
      return
    }

    const temp = temporaryBackend.value

    // Check if backend already exists in DB (from synced data)
    const existingBackend = await findBackendByCredentialsAsync(
      temp.serverUrl,
      temp.email,
    )

    if (existingBackend) {
      // Backend exists from remote sync - update password if needed
      log.debug('Backend already exists from sync, updating password')
      await updateBackendAsync(existingBackend.id, {
        password: temp.password,
        vaultId: temp.vaultId,
      })
    } else {
      // Backend doesn't exist - add it
      log.debug('Backend not found in synced data, adding new')
      await addBackendAsync({
        id: temp.id,
        name: temp.name,
        serverUrl: temp.serverUrl,
        vaultId: temp.vaultId,
        email: temp.email,
        password: temp.password,
        enabled: temp.enabled,
      })
    }

    // Clear temporary backend
    clearTemporaryBackend()

    // Reload backends from DB
    await loadBackendsAsync()
  }

  return {
    backends,
    enabledBackends,
    sortedBackends,
    temporaryBackend,
    loadBackendsAsync,
    addBackendAsync,
    updateBackendAsync,
    deleteBackendAsync,
    toggleBackendAsync,
    updatePriorityAsync,
    findBackendByCredentialsAsync,
    setTemporaryBackend,
    clearTemporaryBackend,
    persistTemporaryBackendAsync,
    reset,
  }
})
