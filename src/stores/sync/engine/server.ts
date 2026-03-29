/**
 * Server API Operations
 * Handles server-side operations like health check, vault deletion, vault name updates
 */

import { DidAuthAction } from '@haex-space/ucan'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { encryptVaultNameAsync } from '@/utils/crypto/vaultName'
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
  spaceId: string,
  privateKey: string,
  did: string,
): Promise<void> => {
  const response = await fetchWithDidAuth(
    `${serverUrl}/sync/vault/${spaceId}`,
    privateKey,
    did,
    DidAuthAction.VaultDelete,
    { method: 'DELETE' },
  )

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(
      `Failed to delete remote vault: ${error.error || response.statusText}`,
    )
  }

  clearVaultKeyCache(spaceId)
  log.info(`Remote vault ${spaceId} deleted from server`)
}

/**
 * Deletes all vault data (vault keys + sync changes) from the sync server.
 * Keeps the account (identity, spaces, etc.) intact.
 */
export const deleteAllVaultDataAsync = async (
  serverUrl: string,
  privateKey: string,
  did: string,
): Promise<void> => {
  const response = await fetchWithDidAuth(
    `${serverUrl}/sync/vaults`,
    privateKey,
    did,
    DidAuthAction.VaultDeleteAll,
    { method: 'DELETE' },
  )

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
  spaceId: string,
  newVaultName: string,
  identityPublicKey: string,
  privateKey: string,
  did: string,
): Promise<void> => {
  // Encrypt new vault name with identity agreement key (X25519 via Rust)
  const sealedName = await encryptVaultNameAsync(newVaultName, identityPublicKey)

  const body = JSON.stringify({
    encryptedVaultName: sealedName.encryptedData,
    vaultNameNonce: sealedName.nonce,
    ephemeralPublicKey: sealedName.ephemeralPublicKey,
  })

  const response = await fetchWithDidAuth(
    `${serverUrl}/sync/vault-key/${spaceId}`,
    privateKey,
    did,
    DidAuthAction.VaultKeyUpdate,
    {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body,
    },
  )

  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(
      `Failed to update vault name on server: ${error.error || response.statusText}`,
    )
  }

  log.info('Vault name updated on server')
}
