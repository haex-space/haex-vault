/**
 * Server API Operations
 * Handles server-side operations like health check, vault deletion, vault name updates
 */

import { deriveKeyFromPassword, encryptString, base64ToArrayBuffer } from '@haex-space/vault-sdk'
import { getAuthTokenAsync } from './supabase'
import { clearVaultKeyCache } from './vaultKey'
import { log } from './types'

/**
 * Health check - verifies server is reachable
 */
export const healthCheckAsync = async (serverUrl: string): Promise<boolean> => {
  try {
    const response = await fetch(serverUrl)
    return response.ok
  } catch {
    return false
  }
}

/**
 * Deletes a remote vault from the sync backend
 * This will delete all CRDT changes, vault keys, and vault configuration from the server
 */
export const deleteRemoteVaultAsync = async (
  serverUrl: string,
  vaultId: string,
): Promise<void> => {
  // Get auth token
  const token = await getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }

  // Send delete request to server
  const response = await fetch(`${serverUrl}/sync/vault/${vaultId}`, {
    method: 'DELETE',
    headers: {
      Authorization: `Bearer ${token}`,
    },
  })

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(
      `Failed to delete remote vault: ${error.error || response.statusText}`,
    )
  }

  // Clear vault key from cache
  clearVaultKeyCache(vaultId)

  log.info(`Remote vault ${vaultId} deleted from server`)
}

/**
 * Updates the vault name on the server
 * Fetches vaultNameSalt from server and uses server password to encrypt
 */
export const updateVaultNameOnServerAsync = async (
  serverUrl: string,
  vaultId: string,
  newVaultName: string,
  serverPassword: string,
): Promise<void> => {
  // Get auth token
  const token = await getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }

  // Fetch vault key info from server to get vaultNameSalt
  const vaultKeyResponse = await fetch(
    `${serverUrl}/sync/vault-key/${vaultId}`,
    {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
      },
    },
  )

  if (!vaultKeyResponse.ok) {
    throw new Error('Failed to fetch vault key info from server')
  }

  const vaultKeyData = await vaultKeyResponse.json()
  const vaultNameSaltBase64 = vaultKeyData.vaultKey.vaultNameSalt

  if (!vaultNameSaltBase64) {
    throw new Error(
      'Vault name salt not found on server. Cannot update vault name.',
    )
  }

  // Derive key from server password using vaultNameSalt
  const vaultNameSalt = base64ToArrayBuffer(vaultNameSaltBase64)
  const derivedKey = await deriveKeyFromPassword(
    serverPassword,
    vaultNameSalt,
  )

  // Encrypt new vault name with new nonce
  const encryptedVaultNameData = await encryptString(newVaultName, derivedKey)

  // Send PATCH request to update vault name on server
  const response = await fetch(`${serverUrl}/sync/vault-key/${vaultId}`, {
    method: 'PATCH',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({
      encryptedVaultName: encryptedVaultNameData.encryptedData,
      vaultNameNonce: encryptedVaultNameData.nonce,
    }),
  })

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(
      `Failed to update vault name on server: ${error.error || response.statusText}`,
    )
  }

  log.info('Vault name updated on server')
}
