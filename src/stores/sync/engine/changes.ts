/**
 * CRDT Changes - Push/Pull Operations
 * Handles encryption and transmission of CRDT changes to/from server
 */

import { encryptCrdtData, decryptCrdtData } from '@haex-space/vault-sdk'
import { getAuthTokenAsync } from './supabase'
import { getVaultKeyCache } from './vaultKey'
import {
  log,
  type CrdtChange,
  type SyncChangeData,
  type PullChangesResponse,
} from './types'

/**
 * Pushes CRDT changes to the server
 */
export const pushChangesAsync = async (
  serverUrl: string,
  vaultId: string,
  changes: CrdtChange[],
): Promise<void> => {
  // Get vault key from cache
  const vaultKeyCache = getVaultKeyCache()
  const cached = vaultKeyCache[vaultId]
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
  const response = await fetch(`${serverUrl}/sync/push`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
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
export const pullChangesAsync = async (
  serverUrl: string,
  vaultId: string,
  excludeDeviceId?: string,
  afterCreatedAt?: string,
  limit?: number,
): Promise<CrdtChange[]> => {
  // Get vault key from cache
  const vaultKeyCache = getVaultKeyCache()
  const cached = vaultKeyCache[vaultId]
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
  const response = await fetch(`${serverUrl}/sync/pull`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
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
      log.error('Failed to decrypt log entry:', change.id, error)
      // Skip corrupted entries
    }
  }

  return decryptedLogs
}
