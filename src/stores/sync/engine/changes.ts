/**
 * CRDT Changes - Push/Pull Operations
 * Handles encryption and transmission of CRDT changes to/from server
 */

import { encryptCrdtData, decryptCrdtData } from '@haex-space/vault-sdk'
import { createDidAuthHeader } from '@/utils/auth/didAuth'
import { getUcanForSpaceAsync } from '@/utils/auth/ucanStore'
import { getVaultKeyCache } from './vaultKey'
import {
  engineLog as log,
  type CrdtChange,
  type SyncChangeData,
  type PullChangesResponse,
} from './types'

/** Build auth headers: UCAN for shared spaces, DID-Auth for personal vault */
const buildAuthHeaderAsync = async (
  spaceId: string,
  privateKey?: string,
  did?: string,
): Promise<Record<string, string>> => {
  // Try UCAN first (shared spaces)
  const ucan = getUcanForSpaceAsync(spaceId)
  if (ucan) {
    return { Authorization: `UCAN ${ucan}` }
  }
  // Fall back to DID-Auth (personal vault)
  if (privateKey && did) {
    const header = await createDidAuthHeader(privateKey, did, 'sync')
    return { Authorization: header }
  }
  throw new Error('No authentication available: no UCAN token for space and no DID credentials provided')
}

/**
 * Pushes CRDT changes to the server
 */
export const pushChangesAsync = async (
  serverUrl: string,
  spaceId: string,
  changes: CrdtChange[],
  privateKey?: string,
  did?: string,
): Promise<void> => {
  // Get vault key from cache
  const vaultKeyCache = getVaultKeyCache()
  const cached = vaultKeyCache[spaceId]
  if (!cached) {
    throw new Error('Vault key not available. Please unlock vault first.')
  }

  const vaultKey = cached.vaultKey

  // Build auth headers (UCAN for shared spaces, DID-Auth for personal vault)
  const authHeaders = await buildAuthHeaderAsync(spaceId, privateKey, did)

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
      ...authHeaders,
    },
    body: JSON.stringify({
      spaceId,
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
  spaceId: string,
  excludeDeviceId?: string,
  afterCreatedAt?: string,
  limit?: number,
  privateKey?: string,
  did?: string,
): Promise<CrdtChange[]> => {
  // Get vault key from cache
  const vaultKeyCache = getVaultKeyCache()
  const cached = vaultKeyCache[spaceId]
  if (!cached) {
    throw new Error('Vault key not available. Please unlock vault first.')
  }

  const vaultKey = cached.vaultKey

  // Build auth headers (UCAN for shared spaces, DID-Auth for personal vault)
  const authHeaders = await buildAuthHeaderAsync(spaceId, privateKey, did)

  // Fetch from server
  const response = await fetch(`${serverUrl}/sync/pull`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...authHeaders,
    },
    body: JSON.stringify({
      spaceId,
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
