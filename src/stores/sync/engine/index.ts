/**
 * Sync Engine Store - Executes sync operations with haex-sync-server backends
 * Handles vault key storage and CRDT log synchronization
 *
 * This store combines functionality from:
 * - supabase.ts: Supabase client management
 * - vaultKey.ts: Vault key encryption/decryption
 * - changes.ts: CRDT push/pull operations
 * - database.ts: Local database operations
 * - server.ts: Server API operations
 */

import { engineLog, type VaultKeyCache } from './types'
import {
  initSupabaseClientAsync as initClient,
  getAuthTokenAsync as getToken,
  supabaseClientRef,
  currentBackendIdRef,
  resetSupabaseClient,
  setSupabaseClient as setClient,
  cacheAccessToken,
  setReauthResolver,
} from './supabase'
import {
  getVaultKeyCache,
  cacheSyncKey,
  clearVaultKeyCache,
  uploadVaultKeyAsync,
  getVaultKeyFromServerAsync,
  fetchSyncKeyFromServerAsync,
  generateNewVaultKey,
  reEncryptVaultKeyAsync,
} from './vaultKey'
import { pushChangesAsync as pushChanges, pullChangesAsync as pullChanges } from './changes'
import {
  getSyncKeyFromDbAsync,
  saveSyncKeyToDbAsync,
  saveVaultKeySaltAsync,
  getVaultKeySaltAsync,
  markBackendPendingVaultKeyUpdateAsync as markPending,
  getBackendsWithPendingVaultKeyUpdateAsync as getPending,
} from './database'
import {
  healthCheckAsync as healthCheck,
  deleteRemoteVaultAsync as deleteRemote,
  deleteAllVaultDataAsync as deleteAllVaults,
  updateVaultNameOnServerAsync as updateName,
} from './server'

const log = engineLog

// Re-export types
export * from './types'

export const useSyncEngineStore = defineStore('syncEngineStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())
  const syncBackendsStore = useSyncBackendsStore()

  // Expose cache as ref for reactivity
  const vaultKeyCache = ref<VaultKeyCache>(getVaultKeyCache())

  // Expose Supabase client state as reactive refs (imported from supabase.ts)
  const supabaseClient = supabaseClientRef
  const currentBackendId = currentBackendIdRef

  /**
   * Helper to get drizzle instance
   */
  const getDrizzle = () => {
    if (!currentVault.value?.drizzle) {
      throw new Error('No vault opened')
    }
    return currentVault.value.drizzle
  }

  /**
   * Helper to find backend by ID
   */
  const findBackend = (backendId: string) => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }
    return backend
  }

  /**
   * Resolves the identity public key for a backend from its identityId
   */
  const getIdentityAgreementPublicKeyAsync = async (backendId: string): Promise<string> => {
    const backend = findBackend(backendId)
    if (!backend.identityId) {
      throw new Error(`Backend ${backendId} has no identity configured`)
    }
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityAsync(backend.identityId)
    if (!identity?.agreementPublicKey) {
      throw new Error(`Identity not found or missing agreement public key for backend ${backendId}`)
    }
    return identity.agreementPublicKey
  }

  /**
   * Resolves the full identity (publicKey, privateKey, did) for a backend
   */
  const resolveBackendIdentityAsync = async (backendId: string) => {
    const backend = findBackend(backendId)
    if (!backend.identityId) {
      throw new Error(`Backend ${backendId} has no identity configured`)
    }
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityAsync(backend.identityId)
    if (!identity?.publicKey || !identity?.privateKey || !identity?.did) {
      throw new Error(`Identity not found or incomplete for backend ${backendId}`)
    }
    return identity
  }

  /**
   * Registers the DID re-auth resolver for a backend.
   * Must be called whenever a Supabase client is set (both initSupabaseClientAsync
   * and setSupabaseClient paths) so expired sessions can be automatically recovered.
   */
  const registerReauthResolver = (backendId: string): void => {
    setReauthResolver(async () => {
      try {
        const b = syncBackendsStore.backends.find((x) => x.id === backendId)
        if (!b?.identityId) {
          engineLog.warn('DID re-auth resolver: no identityId on backend', backendId)
          return null
        }
        const identityStore = useIdentityStore()
        const identity = await identityStore.getIdentityAsync(b.identityId)
        if (!identity?.did || !identity?.privateKey) {
          engineLog.warn('DID re-auth resolver: identity missing did or privateKey', { identityId: b.identityId, hasDid: !!identity?.did, hasKey: !!identity?.privateKey })
          return null
        }
        engineLog.info('DID re-auth resolver: context ready', { serverUrl: b.serverUrl, did: identity.did.slice(0, 20) + '...' })
        return { serverUrl: b.serverUrl, did: identity.did, privateKey: identity.privateKey }
      } catch (e) {
        engineLog.error('DID re-auth resolver: exception', e)
        return null
      }
    })
  }

  /**
   * Initializes Supabase client for a specific backend
   */
  const initSupabaseClientAsync = async (backendId: string): Promise<void> => {
    const backend = findBackend(backendId)
    await initClient(backendId, backend.serverUrl)
    registerReauthResolver(backendId)
  }

  /**
   * Sets an existing Supabase client (for cases where client is created externally)
   * This is used in the connect wizard where the client is already authenticated
   */
  const setSupabaseClient = (
    client: Parameters<typeof setClient>[0],
    backendId: string,
  ): void => {
    setClient(client, backendId)
    registerReauthResolver(backendId)
  }

  /**
   * Gets the current Supabase auth token
   */
  const getAuthTokenAsync = async (): Promise<string | null> => {
    return getToken()
  }

  /**
   * Uploads encrypted vault key to the server
   */
  const uploadVaultKeyToServerAsync = async (
    backendId: string,
    spaceId: string,
    vaultKey: Uint8Array,
    vaultName: string,
    vaultPassword: string,
  ): Promise<void> => {
    const backend = findBackend(backendId)
    const identity = await resolveBackendIdentityAsync(backendId)
    const { vaultKeySalt } = await uploadVaultKeyAsync(
      backend.serverUrl,
      spaceId,
      vaultKey,
      vaultName,
      vaultPassword,
      identity.agreementPublicKey,
      identity.privateKey,
      identity.did,
    )
    // Save vault key salt locally
    await saveVaultKeySaltAsync(getDrizzle(), backendId, vaultKeySalt)
    log.info('Vault key uploaded to server, vault key salt saved locally')
  }

  /**
   * Retrieves and decrypts vault key from the server
   */
  const getVaultKeyAsync = async (
    backendId: string,
    spaceId: string,
    password: string,
  ): Promise<Uint8Array> => {
    const backend = findBackend(backendId)
    const identity = await resolveBackendIdentityAsync(backendId)
    return getVaultKeyFromServerAsync(backend.serverUrl, spaceId, password, identity.privateKey, identity.did)
  }

  /**
   * Pushes CRDT changes to the server
   */
  const pushChangesAsync = async (
    backendId: string,
    spaceId: string,
    changes: Parameters<typeof pushChanges>[2],
  ): Promise<void> => {
    const backend = findBackend(backendId)
    const identity = await resolveBackendIdentityAsync(backendId)
    return pushChanges(backend.serverUrl, spaceId, changes, identity.privateKey, identity.did)
  }

  /**
   * Pulls CRDT changes from the server
   */
  const pullChangesAsync = async (
    backendId: string,
    spaceId: string,
    excludeDeviceId?: string,
    afterCreatedAt?: string,
    limit?: number,
  ): Promise<ReturnType<typeof pullChanges>> => {
    const backend = findBackend(backendId)
    const identity = await resolveBackendIdentityAsync(backendId)
    return pullChanges(backend.serverUrl, spaceId, excludeDeviceId, afterCreatedAt, limit, identity.privateKey, identity.did)
  }

  /**
   * Gets sync key from local DB
   */
  const getSyncKeyFromDb = async (backendId: string): Promise<Uint8Array | null> => {
    return getSyncKeyFromDbAsync(getDrizzle(), backendId)
  }

  /**
   * Saves sync key to local DB
   */
  const saveSyncKeyToDb = async (backendId: string, syncKey: Uint8Array): Promise<void> => {
    return saveSyncKeyToDbAsync(getDrizzle(), backendId, syncKey)
  }

  /**
   * Saves vault key salt to local DB
   */
  const saveVaultKeySalt = async (backendId: string, vaultKeySalt: string): Promise<void> => {
    return saveVaultKeySaltAsync(getDrizzle(), backendId, vaultKeySalt)
  }

  /**
   * Gets vault key salt from local DB
   */
  const getVaultKeySalt = async (backendId: string): Promise<string | null> => {
    return getVaultKeySaltAsync(getDrizzle(), backendId)
  }

  /**
   * Generates new sync key, saves locally, and uploads to server
   */
  const generateAndUploadSyncKeyAsync = async (
    backendId: string,
    spaceId: string,
    vaultName: string,
    vaultPassword: string,
  ): Promise<Uint8Array> => {
    log.info('Generating new sync key...')
    const syncKey = generateNewVaultKey()

    await saveSyncKeyToDb(backendId, syncKey)
    cacheSyncKey(spaceId, syncKey)
    await uploadVaultKeyToServerAsync(
      backendId,
      spaceId,
      syncKey,
      vaultName,
      vaultPassword,
    )

    log.info('New sync key generated, uploaded to server, and saved locally')
    return syncKey
  }

  /**
   * Ensures sync key exists for a backend (loads from cache/DB/server or generates new one)
   */
  const ensureSyncKeyAsync = async (
    backendId: string,
    spaceId: string,
    vaultName: string,
    vaultPassword: string,
    serverUrl?: string,
  ): Promise<Uint8Array> => {
    // 1. Check cache first
    const cache = getVaultKeyCache()
    const cached = cache[spaceId]
    if (cached) {
      // Ensure the key is also saved in DB for this backend
      const dbKey = await getSyncKeyFromDb(backendId)
      if (!dbKey) {
        log.info('Sync key found in cache but not in DB for this backend, saving...')
        await saveSyncKeyToDb(backendId, cached.vaultKey)
      }
      // Verify the key exists on the server, re-upload if missing
      const resolvedServerUrl = serverUrl ?? syncBackendsStore.backends.find((b) => b.id === backendId)?.serverUrl
      if (resolvedServerUrl) {
        try {
          const identity = await resolveBackendIdentityAsync(backendId)
          await fetchSyncKeyFromServerAsync(resolvedServerUrl, spaceId, vaultPassword, identity.privateKey, identity.did)
        } catch (error) {
          if (error instanceof Error && error.message.includes('not found')) {
            log.warn('Vault key missing on server, re-uploading...')
            await uploadVaultKeyToServerAsync(backendId, spaceId, cached.vaultKey, vaultName, vaultPassword)
          }
        }
      }
      log.info('Sync key ready')
      return cached.vaultKey
    }

    // 2. Initial sync mode: fetch directly from server
    if (serverUrl) {
      log.info('Initial sync mode: Fetching sync key from server...')
      try {
        const identity = await resolveBackendIdentityAsync(backendId)
        const syncKey = await fetchSyncKeyFromServerAsync(
          serverUrl,
          spaceId,
          vaultPassword,
          identity.privateKey,
          identity.did,
        )
        cacheSyncKey(spaceId, syncKey)
        log.info('Sync key downloaded from server and cached')
        return syncKey
      } catch (error) {
        // First-time connection: no key on server yet — generate one
        if (error instanceof Error && error.message.includes('not found')) {
          log.info('No sync key on server yet, generating new one...')
          return generateAndUploadSyncKeyAsync(
            backendId,
            spaceId,
            vaultName,
            vaultPassword,
          )
        }
        throw error
      }
    }

    // 3. Try to load from local DB
    const dbKey = await getSyncKeyFromDb(backendId)
    if (dbKey) {
      // Verify the key also exists on the server
      const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
      if (backend) {
        try {
          await getVaultKeyAsync(backendId, spaceId, vaultPassword)
          log.info('Sync key verified on server')
        } catch (error) {
          if (error instanceof Error && error.message.includes('not found')) {
            // Server lost the vault key - re-upload it
            log.warn('Vault key missing on server, re-uploading...')
            await reUploadVaultKeyAsync(
              backendId,
              spaceId,
              dbKey,
              vaultName,
              vaultPassword,
            )
            log.info('Vault key re-uploaded to server')
          } else {
            // Other errors (network, auth) - log but continue with local key
            log.warn('Could not verify vault key on server:', error)
          }
        }
      }

      cacheSyncKey(spaceId, dbKey)
      log.info('Sync key loaded from local database')
      return dbKey
    }

    // 4. Try to fetch from server via backend
    try {
      const serverKey = await getVaultKeyAsync(backendId, spaceId, vaultPassword)
      await saveSyncKeyToDb(backendId, serverKey)
      log.info('Sync key downloaded from server and saved locally')
      return serverKey
    } catch (error) {
      // 5. Generate new key if not found on server
      if (error instanceof Error && error.message.includes('not found')) {
        return generateAndUploadSyncKeyAsync(
          backendId,
          spaceId,
          vaultName,
          vaultPassword,
        )
      }
      throw error
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
    return healthCheck(backend.serverUrl)
  }

  /**
   * Deletes a remote vault from the sync backend
   */
  const deleteRemoteVaultAsync = async (
    backendId: string,
    spaceId: string,
    serverUrl?: string,
  ): Promise<void> => {
    let resolvedServerUrl = serverUrl
    if (!resolvedServerUrl) {
      const backend = findBackend(backendId)
      resolvedServerUrl = backend.serverUrl
    }
    const identity = await resolveBackendIdentityAsync(backendId)
    return deleteRemote(resolvedServerUrl, spaceId, identity.privateKey, identity.did)
  }

  /**
   * Deletes all vault data (vault keys + sync changes) from the sync server
   */
  const deleteAllVaultDataAsync = async (
    backendId: string,
    serverUrl?: string,
  ): Promise<void> => {
    let resolvedServerUrl = serverUrl
    if (!resolvedServerUrl) {
      const backend = findBackend(backendId)
      resolvedServerUrl = backend.serverUrl
    }
    const identity = await resolveBackendIdentityAsync(backendId)
    return deleteAllVaults(resolvedServerUrl, identity.privateKey, identity.did)
  }

  /**
   * Updates the vault name on the server
   */
  const updateVaultNameOnServerAsync = async (
    backendId: string,
    spaceId: string,
    newVaultName: string,
  ): Promise<void> => {
    const backend = findBackend(backendId)
    const identity = await resolveBackendIdentityAsync(backendId)
    const identityPublicKey = await getIdentityAgreementPublicKeyAsync(backendId)
    return updateName(backend.serverUrl, spaceId, newVaultName, identityPublicKey, identity.privateKey, identity.did)
  }

  /**
   * Re-encrypts the vault key on a specific backend with a new password
   */
  const reEncryptVaultKeyOnBackendAsync = async (
    backendId: string,
    spaceId: string,
    vaultKey: Uint8Array,
    newPassword: string,
  ): Promise<boolean> => {
    const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
    if (!backend) {
      throw new Error('Backend not found')
    }

    const identity = await resolveBackendIdentityAsync(backendId)
    const result = await reEncryptVaultKeyAsync(
      backend.serverUrl,
      spaceId,
      vaultKey,
      newPassword,
      identity.privateKey,
      identity.did,
    )

    if (result.success && result.vaultKeySalt) {
      await saveVaultKeySalt(backendId, result.vaultKeySalt)
      log.info(`Vault key re-encrypted on backend ${backendId}`)
    }

    return result.success
  }

  /**
   * Marks a backend as having a pending vault key update
   */
  const markBackendPendingVaultKeyUpdateAsync = async (
    backendId: string,
    pending: boolean,
  ): Promise<void> => {
    return markPending(getDrizzle(), backendId, pending)
  }

  /**
   * Gets all backends that have pending vault key updates
   */
  const getBackendsWithPendingVaultKeyUpdateAsync = async (): Promise<string[]> => {
    if (!currentVault.value?.drizzle) {
      return []
    }
    return getPending(currentVault.value.drizzle)
  }

  /**
   * Retries pending vault key updates for all backends
   */
  const retryPendingVaultKeyUpdatesAsync = async (
    vaultKey: Uint8Array,
    vaultPassword: string,
  ): Promise<{ successCount: number; failedBackendIds: string[] }> => {
    const pendingBackendIds = await getBackendsWithPendingVaultKeyUpdateAsync()

    if (pendingBackendIds.length === 0) {
      return { successCount: 0, failedBackendIds: [] }
    }

    log.info(`Retrying vault key update for ${pendingBackendIds.length} backends...`)

    let successCount = 0
    const failedBackendIds: string[] = []

    for (const backendId of pendingBackendIds) {
      const backend = syncBackendsStore.backends.find((b) => b.id === backendId)
      if (!backend?.spaceId) {
        failedBackendIds.push(backendId)
        continue
      }

      const success = await reEncryptVaultKeyOnBackendAsync(
        backendId,
        backend.spaceId,
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
   * Re-uploads the vault key to the server
   */
  const reUploadVaultKeyAsync = async (
    backendId: string,
    spaceId: string,
    vaultKey: Uint8Array,
    vaultName: string,
    vaultPassword: string,
  ): Promise<void> => {
    log.info('Re-uploading vault key to server...')

    await uploadVaultKeyToServerAsync(
      backendId,
      spaceId,
      vaultKey,
      vaultName,
      vaultPassword,
    )

    cacheSyncKey(spaceId, vaultKey)
    log.info('Vault key re-uploaded to server')
  }

  /**
   * Resets all store state. Called when closing a vault.
   */
  const reset = async (): Promise<void> => {
    clearVaultKeyCache()
    setReauthResolver(null)
    await resetSupabaseClient()
    // Sync the ref with the actual cache
    vaultKeyCache.value = getVaultKeyCache()
    log.info('Store reset')
  }

  return {
    vaultKeyCache,
    supabaseClient,
    currentBackendId,
    initSupabaseClientAsync,
    setSupabaseClient,
    registerReauthResolver,
    getAuthTokenAsync,
    uploadVaultKeyAsync: uploadVaultKeyToServerAsync,
    getVaultKeyAsync,
    pushChangesAsync,
    pullChangesAsync,
    getSyncKeyFromDbAsync: getSyncKeyFromDb,
    saveSyncKeyToDbAsync: saveSyncKeyToDb,
    saveVaultKeySaltAsync: saveVaultKeySalt,
    getVaultKeySaltAsync: getVaultKeySalt,
    ensureSyncKeyAsync,
    clearVaultKeyCache,
    healthCheckAsync,
    deleteRemoteVaultAsync,
    deleteAllVaultDataAsync,
    updateVaultNameOnServerAsync,
    reEncryptVaultKeyOnBackendAsync,
    markBackendPendingVaultKeyUpdateAsync,
    getBackendsWithPendingVaultKeyUpdateAsync,
    retryPendingVaultKeyUpdatesAsync,
    reUploadVaultKeyAsync,
    cacheAccessToken,
    reset,
  }
})
