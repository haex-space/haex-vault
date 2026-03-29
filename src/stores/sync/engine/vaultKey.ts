/**
 * Vault Key Management
 * Handles vault key encryption, decryption, caching, and server operations
 */

import {
  encryptVaultKey,
  decryptVaultKey,
  generateVaultKey,
} from '@haex-space/vault-sdk'
import { encryptVaultNameAsync } from '@/utils/crypto/vaultName'
import { createDidAuthHeader, fetchWithDidAuth } from '@/utils/auth/didAuth'
import { engineLog as log, type VaultKeyCache } from './types'

/** Simple network error wrapper for DID-Auth requests (no JWT reauth needed) */
const fetchWithNetworkErrorHandlingAsync = async (url: string, options?: RequestInit): Promise<Response> => {
  try {
    return await fetch(url, options)
  } catch {
    throw new Error('NETWORK_ERROR: Cannot connect to sync server. Please check your internet connection.')
  }
}

// In-memory cache for decrypted vault keys (cleared on logout/vault close)
const vaultKeyCache: VaultKeyCache = {}

/**
 * Gets the vault key cache (for store exposure)
 */
export const getVaultKeyCache = (): VaultKeyCache => vaultKeyCache

/**
 * Caches the sync key in memory
 */
export const cacheSyncKey = (spaceId: string, syncKey: Uint8Array): void => {
  vaultKeyCache[spaceId] = {
    vaultKey: syncKey,
    timestamp: Date.now(),
  }
}

/**
 * Clears vault key from cache
 */
export const clearVaultKeyCache = (spaceId?: string): void => {
  if (spaceId) {
    Reflect.deleteProperty(vaultKeyCache, spaceId)
  } else {
    Object.keys(vaultKeyCache).forEach((key) => Reflect.deleteProperty(vaultKeyCache, key))
  }
}

/**
 * Uploads encrypted vault key to the server and saves salts locally
 *
 * Encryption:
 * - Vault key: encrypted with vault password (symmetric, for data access)
 * - Vault name: encrypted with identity public key (ECDH, readable after login)
 */
export const uploadVaultKeyAsync = async (
  serverUrl: string,
  spaceId: string,
  vaultKey: Uint8Array,
  vaultName: string,
  vaultPassword: string,
  identityPublicKey: string,
  privateKey: string,
  did: string,
): Promise<{ vaultKeySalt: string }> => {
  // Encrypt vault key with vault password
  const encryptedVaultKeyData = await encryptVaultKey(vaultKey, vaultPassword)

  // Encrypt vault name with identity agreement key (X25519 ECDH via Rust)
  const sealedName = await encryptVaultNameAsync(vaultName, identityPublicKey)

  // Send to server with DID-Auth
  const body = JSON.stringify({
    spaceId,
    encryptedVaultKey: encryptedVaultKeyData.encryptedVaultKey,
    encryptedVaultName: sealedName.encryptedData,
    vaultKeySalt: encryptedVaultKeyData.salt,
    ephemeralPublicKey: sealedName.ephemeralPublicKey,
    vaultKeyNonce: encryptedVaultKeyData.vaultKeyNonce,
    vaultNameNonce: sealedName.nonce,
  })
  const response = await fetchWithDidAuth(`${serverUrl}/sync/vault-key`, privateKey, did, 'vault-key', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
  })

  if (!response.ok) {
    const errorData = await response.json().catch(() => ({}))
    const message = errorData.error
      ?? (errorData.success === false && errorData.error?.message)
      ?? JSON.stringify(errorData)
    log.error('Upload vault key failed:', { status: response.status, errorData })
    throw new Error(`Failed to upload vault key: ${message}`)
  }

  log.info('Vault key uploaded to server')

  return { vaultKeySalt: encryptedVaultKeyData.salt }
}

/**
 * Retrieves and decrypts vault key from the server
 */
export const getVaultKeyFromServerAsync = async (
  serverUrl: string,
  spaceId: string,
  password: string,
  privateKey: string,
  did: string,
): Promise<Uint8Array> => {
  // Check cache first
  const cached = vaultKeyCache[spaceId]
  if (cached) {
    return cached.vaultKey
  }

  // Fetch from server with DID-Auth
  const authHeader = await createDidAuthHeader(privateKey, did, 'get-vault-key')
  const response = await fetchWithNetworkErrorHandlingAsync(
    `${serverUrl}/sync/vault-key/${spaceId}`,
    { method: 'GET', headers: { Authorization: authHeader } },
  )

  if (response.status === 404) {
    throw new Error('Vault key not found on server')
  }

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    log.error('Get vault key error:', {
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
  vaultKeyCache[spaceId] = {
    vaultKey,
    timestamp: Date.now(),
  }

  return vaultKey
}

/**
 * Fetches sync key directly from server (for initial sync)
 */
export const fetchSyncKeyFromServerAsync = async (
  serverUrl: string,
  spaceId: string,
  password: string,
  privateKey: string,
  did: string,
): Promise<Uint8Array> => {
  const authHeader = await createDidAuthHeader(privateKey, did, 'get-vault-key')
  const response = await fetchWithNetworkErrorHandlingAsync(
    `${serverUrl}/sync/vault-key/${spaceId}`,
    {
      method: 'GET',
      headers: { Authorization: authHeader },
    },
  )

  if (response.status === 404) {
    throw new Error(
      'Vault key not found on server. Cannot connect to vault without existing sync key.',
    )
  }

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(
      `Failed to get vault key: ${error.error || response.statusText}`,
    )
  }

  const data = await response.json()

  try {
    return await decryptVaultKey(
      data.vaultKey.encryptedVaultKey,
      data.vaultKey.vaultKeySalt,
      data.vaultKey.vaultKeyNonce,
      password,
    )
  } catch (error) {
    // WebCrypto throws OperationError for decryption failures (wrong password)
    if (error instanceof Error && error.name === 'OperationError') {
      throw new Error(
        'Wrong vault password. Please enter the password you used when you created this vault.',
      )
    }
    throw error
  }
}

/**
 * Generates a new vault key
 */
export const generateNewVaultKey = (): Uint8Array => {
  return generateVaultKey()
}

/**
 * Re-encrypts the vault key on a specific backend with a new password.
 * The vault key itself stays the same, only the encryption changes.
 *
 * @returns Object with success status and new salt if successful
 */
export const reEncryptVaultKeyAsync = async (
  serverUrl: string,
  spaceId: string,
  vaultKey: Uint8Array,
  newPassword: string,
  privateKey: string,
  did: string,
): Promise<{ success: boolean; vaultKeySalt?: string }> => {
  try {
    // Re-encrypt the vault key with the new password (generates new salt and nonce)
    const encryptedVaultKeyData = await encryptVaultKey(vaultKey, newPassword)

    // Send PATCH request to update the encrypted vault key on server with DID-Auth
    const body = JSON.stringify({
      encryptedVaultKey: encryptedVaultKeyData.encryptedVaultKey,
      vaultKeySalt: encryptedVaultKeyData.salt,
      vaultKeyNonce: encryptedVaultKeyData.vaultKeyNonce,
    })
    const response = await fetchWithDidAuth(`${serverUrl}/sync/vault-key/${spaceId}`, privateKey, did, 'update-vault-key', {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body,
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      log.error('Failed to re-encrypt vault key:', error)
      return { success: false }
    }

    log.info('Vault key re-encrypted successfully')
    return { success: true, vaultKeySalt: encryptedVaultKeyData.salt }
  } catch (error) {
    log.error('Failed to re-encrypt vault key:', error)
    return { success: false }
  }
}
