/**
 * Sync Engine Store - Executes sync operations with haex-sync-server backends
 * Handles vault key storage and CRDT log synchronization
 */

import { createClient } from '@supabase/supabase-js'
import { eq } from 'drizzle-orm'
import type { SelectHaexCrdtChanges } from '~/database/schemas'
import { haexSyncBackends } from '~/database/schemas'
import {
  encryptVaultKeyAsync,
  decryptVaultKeyAsync,
  encryptCrdtDataAsync,
  decryptCrdtDataAsync,
  generateVaultKey,
  deriveKeyFromPasswordAsync,
  encryptStringAsync,
  base64ToArrayBuffer,
} from '~/utils/crypto/vaultKey'

interface VaultKeyCache {
  [vaultId: string]: {
    vaultKey: Uint8Array
    timestamp: number
  }
}

interface SyncChangeData {
  deviceId?: string | null
  encryptedData: string
  nonce: string
}

interface PullChangesResponse {
  changes: Array<{
    id: string
    encryptedData: string
    nonce: string
    createdAt: string
  }>
  hasMore: boolean
}

export const useSyncEngineStore = defineStore('syncEngineStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())
  const syncBackendsStore = useSyncBackendsStore()

  // In-memory cache for decrypted vault keys (cleared on logout/vault close)
  const vaultKeyCache = ref<VaultKeyCache>({})

  // Supabase client (initialized with config from backend)
  const supabaseClient = ref<ReturnType<typeof createClient> | null>(null)

  // Track current backend to avoid recreating client for same backend
  const currentBackendId = ref<string | null>(null)

  /**
   * Initializes Supabase client for a specific backend
   * Reuses existing client if already initialized for the same backend
   */
  const initSupabaseClientAsync = async (backendId: string) => {
    // If client already exists for this backend, reuse it
    if (supabaseClient.value && currentBackendId.value === backendId) {
      return
    }

    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    // Get Supabase URL and anon key from server health check
    const response = await fetch(backend.serverUrl)
    if (!response.ok) {
      throw new Error('Failed to connect to sync server')
    }

    const serverInfo = await response.json()
    const supabaseUrl = serverInfo.supabaseUrl
    const supabaseAnonKey = serverInfo.supabaseAnonKey

    if (!supabaseUrl || !supabaseAnonKey) {
      throw new Error('Supabase configuration missing from server')
    }

    // Only create new client if URL/key changed
    supabaseClient.value = createClient(supabaseUrl, supabaseAnonKey, {
      auth: {
        // Use backend-specific storage key to avoid conflicts
        storageKey: `sb-${backendId}-auth-token`,
      },
    })
    currentBackendId.value = backendId
  }

  /**
   * Gets the current Supabase auth token
   */
  const getAuthTokenAsync = async (): Promise<string | null> => {
    if (!supabaseClient.value) {
      return null
    }

    const {
      data: { session },
    } = await supabaseClient.value.auth.getSession()
    return session?.access_token ?? null
  }

  /**
   * Uploads encrypted vault key to the server
   */
  const uploadVaultKeyAsync = async (
    backendId: string,
    vaultId: string,
    vaultKey: Uint8Array,
    vaultName: string,
    password: string,
  ): Promise<void> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    // Encrypt vault key with password
    const encryptedVaultKeyData = await encryptVaultKeyAsync(vaultKey, password)

    // Derive key from password to encrypt vault name (use same salt as vault key)
    const salt = base64ToArrayBuffer(encryptedVaultKeyData.salt)
    const derivedKey = await deriveKeyFromPasswordAsync(password, salt)

    // Encrypt vault name with derived key
    const encryptedVaultNameData = await encryptStringAsync(
      vaultName,
      derivedKey,
    )

    // Get auth token
    const token = await getAuthTokenAsync()
    if (!token) {
      throw new Error('Not authenticated')
    }

    // Send to server
    const response = await fetch(`${backend.serverUrl}/sync/vault-key`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({
        vaultId,
        encryptedVaultKey: encryptedVaultKeyData.encryptedVaultKey,
        encryptedVaultName: encryptedVaultNameData.encryptedData,
        salt: encryptedVaultKeyData.salt,
        vaultKeyNonce: encryptedVaultKeyData.vaultKeyNonce,
        vaultNameNonce: encryptedVaultNameData.nonce,
      }),
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(
        `Failed to upload vault key: ${error.error || response.statusText}`,
      )
    }

    console.log('✅ Vault key uploaded to server')
  }

  /**
   * Retrieves and decrypts vault key from the server
   */
  const getVaultKeyAsync = async (
    backendId: string,
    vaultId: string,
    password: string,
  ): Promise<Uint8Array> => {
    // Check cache first
    const cached = vaultKeyCache.value[vaultId]
    if (cached) {
      return cached.vaultKey
    }

    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    // Get auth token
    const token = await getAuthTokenAsync()
    if (!token) {
      throw new Error('Not authenticated')
    }

    // Fetch from server
    const response = await fetch(
      `${backend.serverUrl}/sync/vault-key/${vaultId}`,
      {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${token}`,
        },
      },
    )

    if (response.status === 404) {
      throw new Error('Vault key not found on server')
    }

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      console.error('Get vault key error:', {
        status: response.status,
        statusText: response.statusText,
        error,
      })
      throw new Error(
        `Failed to get vault key: ${error.error || response.statusText}`,
      )
    }

    const data = await response.json()

    // Decrypt vault key
    const vaultKey = await decryptVaultKeyAsync(
      data.vaultKey.encryptedVaultKey,
      data.vaultKey.salt,
      data.vaultKey.vaultKeyNonce,
      password,
    )

    // Cache decrypted vault key
    vaultKeyCache.value[vaultId] = {
      vaultKey,
      timestamp: Date.now(),
    }

    return vaultKey
  }

  /**
   * Pushes CRDT changes to the server
   */
  const pushChangesAsync = async (
    backendId: string,
    vaultId: string,
    changes: SelectHaexCrdtChanges[],
  ): Promise<void> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    // Get vault key from cache
    const cached = vaultKeyCache.value[vaultId]
    if (!cached) {
      throw new Error('Vault key not available. Please unlock vault first.')
    }

    const vaultKey = cached.vaultKey

    // Get auth token
    const token = await getAuthTokenAsync()
    if (!token) {
      throw new Error('Not authenticated')
    }

    // Encrypt each change entry (exclude syncState and deviceId - they're sent separately/client-specific)
    const encryptedChanges: SyncChangeData[] = []
    for (const change of changes) {
      // Remove syncState and deviceId before encrypting - deviceId is sent separately, syncState is client-specific
      const { syncState, deviceId, ...changeWithoutClientData } = change

      const { encryptedData, nonce } = await encryptCrdtDataAsync(
        changeWithoutClientData,
        vaultKey,
      )

      encryptedChanges.push({
        deviceId,
        encryptedData,
        nonce,
      })
    }

    // Send to server
    const response = await fetch(`${backend.serverUrl}/sync/push`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({
        vaultId,
        changes: encryptedChanges,
      }),
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(
        `Failed to push logs: ${error.error || response.statusText}`,
      )
    }
  }

  /**
   * Pulls CRDT changes from the server
   */
  const pullChangesAsync = async (
    backendId: string,
    vaultId: string,
    excludeDeviceId?: string,
    afterCreatedAt?: string,
    limit?: number,
  ): Promise<SelectHaexCrdtChanges[]> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    // Get vault key from cache
    const cached = vaultKeyCache.value[vaultId]
    if (!cached) {
      throw new Error('Vault key not available. Please unlock vault first.')
    }

    const vaultKey = cached.vaultKey

    // Get auth token
    const token = await getAuthTokenAsync()
    if (!token) {
      throw new Error('Not authenticated')
    }

    // Fetch from server
    const response = await fetch(`${backend.serverUrl}/sync/pull`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({
        vaultId,
        excludeDeviceId,
        afterCreatedAt,
        limit: limit ?? 100,
      }),
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(
        `Failed to pull logs: ${error.error || response.statusText}`,
      )
    }

    const data: PullChangesResponse = await response.json()

    // Decrypt each log entry
    const decryptedLogs: SelectHaexCrdtChanges[] = []
    for (const change of data.changes) {
      try {
        const decrypted = await decryptCrdtDataAsync<Omit<SelectHaexCrdtChanges, 'syncState'>>(
          change.encryptedData,
          change.nonce,
          vaultKey,
        )

        // Add syncState for downloaded changes - they need to be applied locally
        decryptedLogs.push({
          ...decrypted,
          syncState: 'pending_apply',
        })
      } catch (error) {
        console.error('Failed to decrypt log entry:', change.id, error)
        // Skip corrupted entries
      }
    }

    return decryptedLogs
  }

  /**
   * Gets sync key for a backend from haex_sync_backends table
   * Returns null if not found
   */
  const getSyncKeyFromDbAsync = async (
    backendId: string,
  ): Promise<Uint8Array | null> => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    const results = await currentVault.value.drizzle
      .select()
      .from(haexSyncBackends)
      .where(eq(haexSyncBackends.id, backendId))
      .limit(1)

    if (!results[0]?.syncKey) {
      return null
    }

    // Stored as Base64, convert back to Uint8Array
    return Uint8Array.from(atob(results[0].syncKey), (c) => c.charCodeAt(0))
  }

  /**
   * Saves sync key for a backend to haex_sync_backends table
   */
  const saveSyncKeyToDbAsync = async (
    backendId: string,
    syncKey: Uint8Array,
  ): Promise<void> => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    // Convert Uint8Array to Base64
    const base64 = btoa(String.fromCharCode(...syncKey))

    // Update the backend with the sync key
    await currentVault.value.drizzle
      .update(haexSyncBackends)
      .set({ syncKey: base64 })
      .where(eq(haexSyncBackends.id, backendId))
  }

  /**
   * Ensures sync key exists for a backend (loads from DB or generates new one)
   * Also ensures the key is uploaded to the server
   */
  const ensureSyncKeyAsync = async (
    backendId: string,
    vaultId: string,
    vaultName: string,
    password: string,
  ): Promise<Uint8Array> => {
    // 1. Try to load from local DB
    let syncKey = await getSyncKeyFromDbAsync(backendId)

    if (syncKey) {
      // Key exists locally, cache it
      vaultKeyCache.value[vaultId] = {
        vaultKey: syncKey,
        timestamp: Date.now(),
      }
      console.log('✅ Sync key loaded from local database')
      return syncKey
    }

    // 2. Key doesn't exist locally - check if it exists on server
    try {
      syncKey = await getVaultKeyAsync(backendId, vaultId, password)
      // Key exists on server, save it locally
      await saveSyncKeyToDbAsync(backendId, syncKey)
      console.log('✅ Sync key downloaded from server and saved locally')
      return syncKey
    } catch (error) {
      // Key doesn't exist on server either
      if (error instanceof Error && error.message.includes('not found')) {
        // 3. Generate new key and upload to server
        console.log('📤 Generating new sync key...')
        syncKey = generateVaultKey()

        // Save locally first
        await saveSyncKeyToDbAsync(backendId, syncKey)

        // Cache it
        vaultKeyCache.value[vaultId] = {
          vaultKey: syncKey,
          timestamp: Date.now(),
        }

        // Upload to server with the same key we just generated
        await uploadVaultKeyAsync(backendId, vaultId, syncKey, vaultName, password)

        console.log(
          '✅ New sync key generated, uploaded to server, and saved locally',
        )
        return syncKey
      }

      // Other errors should be propagated
      throw error
    }
  }

  /**
   * Clears vault key from cache
   */
  const clearVaultKeyCache = (vaultId?: string) => {
    if (vaultId) {
      delete vaultKeyCache.value[vaultId]
    } else {
      vaultKeyCache.value = {}
    }
  }

  /**
   * Health check - verifies server is reachable
   */
  const healthCheckAsync = async (backendId: string): Promise<boolean> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      return false
    }

    try {
      const response = await fetch(backend.serverUrl)
      return response.ok
    } catch {
      return false
    }
  }

  /**
   * Deletes a remote vault from the sync backend
   * This will delete all CRDT changes, vault keys, and vault configuration from the server
   */
  const deleteRemoteVaultAsync = async (
    backendId: string,
    vaultId: string,
  ): Promise<void> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    // Get auth token
    const token = await getAuthTokenAsync()
    if (!token) {
      throw new Error('Not authenticated')
    }

    // Send delete request to server
    const response = await fetch(
      `${backend.serverUrl}/sync/vault/${vaultId}`,
      {
        method: 'DELETE',
        headers: {
          'Authorization': `Bearer ${token}`,
        },
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(
        `Failed to delete remote vault: ${error.error || response.statusText}`,
      )
    }

    // Clear vault key from cache
    clearVaultKeyCache(vaultId)

    console.log(`✅ Remote vault ${vaultId} deleted from server`)
  }

  return {
    vaultKeyCache,
    supabaseClient,
    initSupabaseClientAsync,
    getAuthTokenAsync,
    uploadVaultKeyAsync,
    getVaultKeyAsync,
    pushChangesAsync,
    pullChangesAsync,
    getSyncKeyFromDbAsync,
    saveSyncKeyToDbAsync,
    ensureSyncKeyAsync,
    clearVaultKeyCache,
    healthCheckAsync,
    deleteRemoteVaultAsync,
  }
})
