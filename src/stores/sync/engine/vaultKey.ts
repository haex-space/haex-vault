/**
 * Vault Key Management
 * Handles vault key encryption, decryption, caching, and server operations
 */

import {
  encryptVaultKey,
  decryptVaultKey,
  generateVaultKey,
  deriveKeyFromPassword,
  encryptString,
  arrayBufferToBase64,
} from '@haex-space/vault-sdk'
import { getAuthTokenAsync } from './supabase'
import { fetchWithNetworkErrorHandling, log, type VaultKeyCache } from './types'

// In-memory cache for decrypted vault keys (cleared on logout/vault close)
const vaultKeyCache: VaultKeyCache = {}

/**
 * Gets the vault key cache (for store exposure)
 */
export const getVaultKeyCache = (): VaultKeyCache => vaultKeyCache

/**
 * Caches the sync key in memory
 */
export const cacheSyncKey = (vaultId: string, syncKey: Uint8Array): void => {
  vaultKeyCache[vaultId] = {
    vaultKey: syncKey,
    timestamp: Date.now(),
  }
}

/**
 * Clears vault key from cache
 */
export const clearVaultKeyCache = (vaultId?: string): void => {
  if (vaultId) {
    delete vaultKeyCache[vaultId]
  } else {
    Object.keys(vaultKeyCache).forEach((key) => delete vaultKeyCache[key])
  }
}

/**
 * Uploads encrypted vault key to the server and saves salts locally
 *
 * Uses two different passwords for encryption:
 * - Vault password: encrypts the vault key (for data access)
 * - Server password: encrypts the vault name (visible after login)
 */
export const uploadVaultKeyAsync = async (
  serverUrl: string,
  vaultId: string,
  vaultKey: Uint8Array,
  vaultName: string,
  vaultPassword: string,
  serverPassword: string,
): Promise<{ vaultKeySalt: string }> => {
  // Encrypt vault key with vault password
  const encryptedVaultKeyData = await encryptVaultKey(vaultKey, vaultPassword)

  // Generate separate salt for vault name encryption (server password)
  const vaultNameSalt = crypto.getRandomValues(new Uint8Array(32))
  const derivedServerKey = await deriveKeyFromPassword(
    serverPassword,
    vaultNameSalt,
  )

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
  const response = await fetch(`${serverUrl}/sync/vault-key`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
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

  log.info('Vault key uploaded to server')

  return { vaultKeySalt: encryptedVaultKeyData.salt }
}

/**
 * Retrieves and decrypts vault key from the server
 */
export const getVaultKeyFromServerAsync = async (
  serverUrl: string,
  vaultId: string,
  password: string,
): Promise<Uint8Array> => {
  // Check cache first
  const cached = vaultKeyCache[vaultId]
  if (cached) {
    return cached.vaultKey
  }

  // Get auth token
  const token = await getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }

  // Fetch from server
  const response = await fetchWithNetworkErrorHandling(
    `${serverUrl}/sync/vault-key/${vaultId}`,
    {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
      },
    },
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
  vaultKeyCache[vaultId] = {
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
      headers: { Authorization: `Bearer ${token}` },
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
  vaultId: string,
  vaultKey: Uint8Array,
  newPassword: string,
): Promise<{ success: boolean; vaultKeySalt?: string }> => {
  try {
    // Get auth token
    const token = await getAuthTokenAsync()
    if (!token) {
      log.warn('Not authenticated for re-encryption')
      return { success: false }
    }

    // Re-encrypt the vault key with the new password (generates new salt and nonce)
    const encryptedVaultKeyData = await encryptVaultKey(vaultKey, newPassword)

    // Send PATCH request to update the encrypted vault key on server
    const response = await fetch(`${serverUrl}/sync/vault-key/${vaultId}`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({
        encryptedVaultKey: encryptedVaultKeyData.encryptedVaultKey,
        vaultKeySalt: encryptedVaultKeyData.salt,
        vaultKeyNonce: encryptedVaultKeyData.vaultKeyNonce,
      }),
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
