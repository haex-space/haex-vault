/**
 * Sync Engine Store - Executes sync operations with haex-sync-server backends
 * Handles vault key storage and CRDT log synchronization
 */

import { createClient } from '@supabase/supabase-js'
import { eq } from 'drizzle-orm'
import { haexSyncBackends } from '~/database/schemas'
import {
  encryptVaultKey,
  decryptVaultKey,
  encryptCrdtData,
  decryptCrdtData,
  generateVaultKey,
  deriveKeyFromPassword,
  encryptString,
  base64ToArrayBuffer,
  arrayBufferToBase64,
} from '@haex-space/vault-sdk'

/**
 * Type for CRDT change entries used in sync operations
 * Contains the fields needed for push/pull with haex-sync-server
 */
interface CrdtChange {
  tableName: string
  rowPks: string
  columnName: string | null
  hlcTimestamp: string
  deviceId: string | null
  encryptedValue: string | null
  nonce: string | null
  createdAt: string
}

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

/**
 * Helper function to wrap fetch with network error handling
 * Catches network errors and throws a user-friendly error message
 */
async function fetchWithNetworkErrorHandling(
  url: string,
  options?: RequestInit,
): Promise<Response> {
  try {
    return await fetch(url, options)
  } catch (networkError) {
    // Network error (no internet, DNS failure, server unreachable, CORS, etc.)
    throw new Error('NETWORK_ERROR: Cannot connect to sync server. Please check your internet connection.')
  }
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
      realtime: {
        // Increase timeout for mobile connections (default is 10s)
        timeout: 30000,
        // Heartbeat interval to keep connection alive on mobile
        heartbeatIntervalMs: 15000,
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
   * Uploads encrypted vault key to the server and saves salts locally
   *
   * Uses two different passwords for encryption:
   * - Vault password: encrypts the vault key (for data access)
   * - Server password: encrypts the vault name (visible after login)
   */
  const uploadVaultKeyAsync = async (
    backendId: string,
    vaultId: string,
    vaultKey: Uint8Array,
    vaultName: string,
    vaultPassword: string,
    serverPassword: string,
  ): Promise<void> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    // Encrypt vault key with vault password
    const encryptedVaultKeyData = await encryptVaultKey(vaultKey, vaultPassword)

    // Generate separate salt for vault name encryption (server password)
    const vaultNameSalt = crypto.getRandomValues(new Uint8Array(32))
    const derivedServerKey = await deriveKeyFromPassword(serverPassword, vaultNameSalt)

    // Encrypt vault name with server password derived key
    const encryptedVaultNameData = await encryptString(
      vaultName,
      derivedServerKey,
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
        vaultKeySalt: encryptedVaultKeyData.salt,
        vaultNameSalt: arrayBufferToBase64(vaultNameSalt),
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

    // Save vault key salt locally for future vault key decryption
    await saveVaultKeySaltAsync(backendId, encryptedVaultKeyData.salt)

    console.log('✅ Vault key uploaded to server, vault key salt saved locally')
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
    const response = await fetchWithNetworkErrorHandling(
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

    // Decrypt vault key using vaultKeySalt
    const vaultKey = await decryptVaultKey(
      data.vaultKey.encryptedVaultKey,
      data.vaultKey.vaultKeySalt,
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
    changes: CrdtChange[],
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

    // Encrypt each change entry (exclude deviceId - it's sent separately)
    const encryptedChanges: SyncChangeData[] = []
    for (const change of changes) {
      // Remove deviceId before encrypting - it's sent separately
      const { deviceId, ...changeWithoutDeviceId } = change

      const { encryptedData, nonce } = await encryptCrdtData(
        changeWithoutDeviceId,
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
  ): Promise<CrdtChange[]> => {
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
    const decryptedLogs: CrdtChange[] = []
    for (const change of data.changes) {
      try {
        const decrypted = await decryptCrdtData<CrdtChange>(
          change.encryptedData,
          change.nonce,
          vaultKey,
        )

        decryptedLogs.push(decrypted)
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
   * Saves vault key salt for a backend to local vault's haex_sync_backends table
   * Salt is used for PBKDF2 key derivation from vault password
   */
  const saveVaultKeySaltAsync = async (
    backendId: string,
    vaultKeySalt: string,
  ): Promise<void> => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    await currentVault.value.drizzle
      .update(haexSyncBackends)
      .set({ vaultKeySalt })
      .where(eq(haexSyncBackends.id, backendId))
  }

  /**
   * Gets vault key salt for a backend from local vault's haex_sync_backends table
   */
  const getVaultKeySaltAsync = async (
    backendId: string,
  ): Promise<string | null> => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    const result = await currentVault.value.drizzle.query.haexSyncBackends.findFirst({
      where: eq(haexSyncBackends.id, backendId),
    })

    return result?.vaultKeySalt ?? null
  }

  /**
   * Fetches sync key directly from server (for initial sync)
   */
  const fetchSyncKeyFromServerAsync = async (
    serverUrl: string,
    vaultId: string,
    password: string,
  ): Promise<Uint8Array> => {
    const token = await getAuthTokenAsync()
    if (!token) {
      throw new Error('Not authenticated')
    }

    const response = await fetchWithNetworkErrorHandling(
      `${serverUrl}/sync/vault-key/${vaultId}`,
      {
        method: 'GET',
        headers: { 'Authorization': `Bearer ${token}` },
      },
    )

    if (response.status === 404) {
      throw new Error('Vault key not found on server. Cannot connect to vault without existing sync key.')
    }

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to get vault key: ${error.error || response.statusText}`)
    }

    const data = await response.json()

    return decryptVaultKey(
      data.vaultKey.encryptedVaultKey,
      data.vaultKey.vaultKeySalt,
      data.vaultKey.vaultKeyNonce,
      password,
    )
  }

  /**
   * Caches the sync key in memory
   */
  const cacheSyncKey = (vaultId: string, syncKey: Uint8Array): void => {
    vaultKeyCache.value[vaultId] = {
      vaultKey: syncKey,
      timestamp: Date.now(),
    }
  }

  /**
   * Generates new sync key, saves locally, and uploads to server
   *
   * @param vaultPassword - Password for vault key encryption
   * @param serverPassword - Password for vault name encryption
   */
  const generateAndUploadSyncKeyAsync = async (
    backendId: string,
    vaultId: string,
    vaultName: string,
    vaultPassword: string,
    serverPassword: string,
  ): Promise<Uint8Array> => {
    console.log('📤 Generating new sync key...')
    const syncKey = generateVaultKey()

    await saveSyncKeyToDbAsync(backendId, syncKey)
    cacheSyncKey(vaultId, syncKey)
    await uploadVaultKeyAsync(backendId, vaultId, syncKey, vaultName, vaultPassword, serverPassword)

    console.log('✅ New sync key generated, uploaded to server, and saved locally')
    return syncKey
  }

  /**
   * Ensures sync key exists for a backend (loads from cache/DB/server or generates new one)
   *
   * @param backendId - The backend ID
   * @param vaultId - The vault ID
   * @param vaultName - The vault name (for generating new key)
   * @param vaultPassword - The vault password (for vault key encryption/decryption)
   * @param serverUrl - Optional: If provided, fetches directly from server (for initial sync)
   * @param serverPassword - Optional: Required only when generating a new key (for vault name encryption)
   */
  const ensureSyncKeyAsync = async (
    backendId: string,
    vaultId: string,
    vaultName: string,
    vaultPassword: string,
    serverUrl?: string,
    serverPassword?: string,
  ): Promise<Uint8Array> => {
    // 1. Check cache first
    const cached = vaultKeyCache.value[vaultId]
    if (cached) {
      console.log('✅ Sync key found in cache')
      return cached.vaultKey
    }

    // 2. Initial sync mode: fetch directly from server
    if (serverUrl) {
      console.log('🔐 Initial sync mode: Fetching sync key from server...')
      const syncKey = await fetchSyncKeyFromServerAsync(serverUrl, vaultId, vaultPassword)
      cacheSyncKey(vaultId, syncKey)
      console.log('✅ Sync key downloaded from server and cached')
      return syncKey
    }

    // 3. Try to load from local DB
    const dbKey = await getSyncKeyFromDbAsync(backendId)
    if (dbKey) {
      // Verify the key also exists on the server
      // If not, re-upload it (handles server data loss scenarios)
      const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
      if (backend) {
        try {
          await getVaultKeyAsync(backendId, vaultId, vaultPassword)
          console.log('✅ Sync key verified on server')
        } catch (error) {
          if (error instanceof Error && error.message.includes('not found')) {
            // Server lost the vault key - re-upload it
            console.log('⚠️ Vault key missing on server, re-uploading...')
            const serverPwd = serverPassword || backend.password
            if (!serverPwd) {
              throw new Error('Server password required to re-upload vault key')
            }
            await reUploadVaultKeyAsync(
              backendId,
              vaultId,
              dbKey,
              vaultName,
              vaultPassword,
              serverPwd,
            )
            console.log('✅ Vault key re-uploaded to server')
          } else {
            // Other errors (network, auth) - log but continue with local key
            console.warn('⚠️ Could not verify vault key on server:', error)
          }
        }
      }

      cacheSyncKey(vaultId, dbKey)
      console.log('✅ Sync key loaded from local database')
      return dbKey
    }

    // 4. Try to fetch from server via backend
    try {
      const serverKey = await getVaultKeyAsync(backendId, vaultId, vaultPassword)
      await saveSyncKeyToDbAsync(backendId, serverKey)
      console.log('✅ Sync key downloaded from server and saved locally')
      return serverKey
    } catch (error) {
      // 5. Generate new key if not found on server
      if (error instanceof Error && error.message.includes('not found')) {
        if (!serverPassword) {
          throw new Error('Server password required to generate new sync key')
        }
        return generateAndUploadSyncKeyAsync(backendId, vaultId, vaultName, vaultPassword, serverPassword)
      }
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

  /**
   * Updates the vault name on the server
   * Fetches vaultNameSalt from server and uses server password to encrypt
   */
  const updateVaultNameOnServerAsync = async (
    backendId: string,
    vaultId: string,
    newVaultName: string,
    serverPassword: string,
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

    // Fetch vault key info from server to get vaultNameSalt
    const vaultKeyResponse = await fetch(
      `${backend.serverUrl}/sync/vault-key/${vaultId}`,
      {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${token}`,
        },
      },
    )

    if (!vaultKeyResponse.ok) {
      throw new Error('Failed to fetch vault key info from server')
    }

    const vaultKeyData = await vaultKeyResponse.json()
    const vaultNameSaltBase64 = vaultKeyData.vaultKey.vaultNameSalt

    if (!vaultNameSaltBase64) {
      throw new Error('Vault name salt not found on server. Cannot update vault name.')
    }

    // Derive key from server password using vaultNameSalt
    const vaultNameSalt = base64ToArrayBuffer(vaultNameSaltBase64)
    const derivedKey = await deriveKeyFromPassword(serverPassword, vaultNameSalt)

    // Encrypt new vault name with new nonce
    const encryptedVaultNameData = await encryptString(
      newVaultName,
      derivedKey,
    )

    // Send PATCH request to update vault name on server
    const response = await fetch(
      `${backend.serverUrl}/sync/vault-key/${vaultId}`,
      {
        method: 'PATCH',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${token}`,
        },
        body: JSON.stringify({
          encryptedVaultName: encryptedVaultNameData.encryptedData,
          vaultNameNonce: encryptedVaultNameData.nonce,
        }),
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(
        `Failed to update vault name on server: ${error.error || response.statusText}`,
      )
    }

    console.log('✅ Vault name updated on server')
  }

  /**
   * Re-encrypts the vault key on a specific backend with a new password.
   * The vault key itself stays the same, only the encryption changes.
   *
   * @param backendId - The backend ID
   * @param vaultId - The vault ID
   * @param vaultKey - The decrypted vault key
   * @param newPassword - The new vault password
   * @returns true if successful, false if server unreachable
   */
  const reEncryptVaultKeyOnBackendAsync = async (
    backendId: string,
    vaultId: string,
    vaultKey: Uint8Array,
    newPassword: string,
  ): Promise<boolean> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    try {
      // Get auth token
      const token = await getAuthTokenAsync()
      if (!token) {
        console.warn(`⚠️ Not authenticated for backend ${backendId}`)
        return false
      }

      // Re-encrypt the vault key with the new password (generates new salt and nonce)
      const encryptedVaultKeyData = await encryptVaultKey(vaultKey, newPassword)

      // Send PATCH request to update the encrypted vault key on server
      const response = await fetch(
        `${backend.serverUrl}/sync/vault-key/${vaultId}`,
        {
          method: 'PATCH',
          headers: {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${token}`,
          },
          body: JSON.stringify({
            encryptedVaultKey: encryptedVaultKeyData.encryptedVaultKey,
            vaultKeySalt: encryptedVaultKeyData.salt,
            vaultKeyNonce: encryptedVaultKeyData.vaultKeyNonce,
          }),
        },
      )

      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        console.error(`Failed to re-encrypt vault key on backend ${backendId}:`, error)
        return false
      }

      // Update local vault key salt
      await saveVaultKeySaltAsync(backendId, encryptedVaultKeyData.salt)

      console.log(`✅ Vault key re-encrypted on backend ${backendId}`)
      return true
    } catch (error) {
      console.error(`Failed to re-encrypt vault key on backend ${backendId}:`, error)
      return false
    }
  }

  /**
   * Marks a backend as having a pending vault key update.
   * Used when a backend is unreachable during password change.
   */
  const markBackendPendingVaultKeyUpdateAsync = async (
    backendId: string,
    pending: boolean,
  ): Promise<void> => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }

    await currentVault.value.drizzle
      .update(haexSyncBackends)
      .set({ pendingVaultKeyUpdate: pending })
      .where(eq(haexSyncBackends.id, backendId))
  }

  /**
   * Gets all backends that have pending vault key updates.
   */
  const getBackendsWithPendingVaultKeyUpdateAsync = async (): Promise<string[]> => {
    if (!currentVault.value?.drizzle) {
      return []
    }

    const results = await currentVault.value.drizzle
      .select({ id: haexSyncBackends.id })
      .from(haexSyncBackends)
      .where(eq(haexSyncBackends.pendingVaultKeyUpdate, true))

    return results.map((r) => r.id)
  }

  /**
   * Retries pending vault key updates for all backends.
   * Called on sync start to handle previously failed updates.
   *
   * @param vaultKey - The decrypted vault key
   * @param vaultPassword - The current vault password
   * @returns Object with success count and failed backend IDs
   */
  const retryPendingVaultKeyUpdatesAsync = async (
    vaultKey: Uint8Array,
    vaultPassword: string,
  ): Promise<{ successCount: number; failedBackendIds: string[] }> => {
    const pendingBackendIds = await getBackendsWithPendingVaultKeyUpdateAsync()

    if (pendingBackendIds.length === 0) {
      return { successCount: 0, failedBackendIds: [] }
    }

    console.log(`🔄 Retrying vault key update for ${pendingBackendIds.length} backends...`)

    let successCount = 0
    const failedBackendIds: string[] = []

    for (const backendId of pendingBackendIds) {
      const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
      if (!backend?.vaultId) {
        failedBackendIds.push(backendId)
        continue
      }

      const success = await reEncryptVaultKeyOnBackendAsync(
        backendId,
        backend.vaultId,
        vaultKey,
        vaultPassword,
      )

      if (success) {
        await markBackendPendingVaultKeyUpdateAsync(backendId, false)
        successCount++
      } else {
        failedBackendIds.push(backendId)
      }
    }

    return { successCount, failedBackendIds }
  }

  /**
   * Re-uploads the vault key to the server.
   * Used when the server data was deleted but local data still exists.
   *
   * @param backendId - The backend ID
   * @param vaultId - The vault ID
   * @param vaultKey - The decrypted vault key
   * @param vaultName - The vault name
   * @param vaultPassword - The vault password (for vault key encryption)
   * @param serverPassword - The server password (for vault name encryption)
   */
  const reUploadVaultKeyAsync = async (
    backendId: string,
    vaultId: string,
    vaultKey: Uint8Array,
    vaultName: string,
    vaultPassword: string,
    serverPassword: string,
  ): Promise<void> => {
    console.log('📤 Re-uploading vault key to server...')

    // Upload the vault key (this will create a new entry on the server)
    await uploadVaultKeyAsync(
      backendId,
      vaultId,
      vaultKey,
      vaultName,
      vaultPassword,
      serverPassword,
    )

    // Cache the sync key
    cacheSyncKey(vaultId, vaultKey)

    console.log('✅ Vault key re-uploaded to server')
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
    saveVaultKeySaltAsync,
    getVaultKeySaltAsync,
    ensureSyncKeyAsync,
    clearVaultKeyCache,
    healthCheckAsync,
    deleteRemoteVaultAsync,
    updateVaultNameOnServerAsync,
    reEncryptVaultKeyOnBackendAsync,
    markBackendPendingVaultKeyUpdateAsync,
    getBackendsWithPendingVaultKeyUpdateAsync,
    retryPendingVaultKeyUpdatesAsync,
    reUploadVaultKeyAsync,
  }
})
