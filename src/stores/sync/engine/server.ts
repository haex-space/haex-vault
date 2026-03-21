/**
 * Server API Operations
 * Handles server-side operations like health check, vault deletion, vault name updates
 */

import { encryptWithPublicKeyAsync } from '@haex-space/vault-sdk'
import { getAuthTokenAsync, fetchWithReauthAsync } from './supabase'
import { clearVaultKeyCache } from './vaultKey'
import { engineLog as log } from './types'

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
  const response = await fetchWithReauthAsync(`${serverUrl}/sync/vault/${vaultId}`, {
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
 * Deletes all vault data (vault keys + sync changes) from the sync server.
 * Keeps the account (identity, spaces, etc.) intact.
 */
export const deleteAllVaultDataAsync = async (
  serverUrl: string,
): Promise<void> => {
  const token = await getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }

  const response = await fetchWithReauthAsync(`${serverUrl}/sync/vaults`, {
    method: 'DELETE',
    headers: {
      Authorization: `Bearer ${token}`,
    },
  })

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(
      `Failed to delete vault data: ${error.error || response.statusText}`,
    )
  }

  log.info('All vault data deleted from server')
}

/**
 * Updates the vault name on the server
 * Re-encrypts with identity public key (ECDH)
 */
export const updateVaultNameOnServerAsync = async (
  serverUrl: string,
  vaultId: string,
  newVaultName: string,
  identityPublicKey: string,
): Promise<void> => {
  const token = await getAuthTokenAsync()
  if (!token) {
    throw new Error('Not authenticated')
  }

  // Encrypt new vault name with identity public key (ECDH)
  const encodedName = new TextEncoder().encode(newVaultName)
  const sealedName = await encryptWithPublicKeyAsync(encodedName, identityPublicKey)

  // Send PATCH request to update vault name on server
  const response = await fetchWithReauthAsync(`${serverUrl}/sync/vault-key/${vaultId}`, {
    method: 'PATCH',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({
      encryptedVaultName: sealedName.encryptedData,
      vaultNameNonce: sealedName.nonce,
      ephemeralPublicKey: sealedName.ephemeralPublicKey,
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
