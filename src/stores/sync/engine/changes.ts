/**
 * CRDT Changes - Push/Pull Operations
 * Handles encryption and transmission of CRDT changes to/from server
 */

import { encryptCrdtData, decryptCrdtData } from '@haex-space/vault-sdk'
import { fetch } from '@tauri-apps/plugin-http'
import { createDidAuthHeader } from '@/utils/auth/didAuth'
import { getUcanForSpaceAsync } from '@/utils/auth/ucanStore'
import { createVaultUcanFetcher } from '@/utils/auth/ucanFetcher'
import { getVaultKeyCache } from './vaultKey'
import {
  engineLog as log,
  type CrdtChange,
  type SyncChangeData,
  type PullChangesResponse,
} from './types'

const ucanFetcher = createVaultUcanFetcher()

/**
 * Dispatch a JSON-body sync request. For shared spaces the request is routed
 * through the UCAN + `X-UCAN-PoP` fetcher (server rejects UCAN traffic
 * without a matching PoP). For the personal vault DID-Auth is signed inline.
 * Throws when no auth is available — reaching this path unauthenticated is
 * always a caller bug.
 */
const sendSyncRequestAsync = async (
  url: string,
  body: string,
  spaceId: string,
  privateKey?: string,
  did?: string,
): Promise<Response> => {
  const ucan = getUcanForSpaceAsync(spaceId)
  if (ucan) {
    return ucanFetcher(url, ucan, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body,
    })
  }
  if (privateKey && did) {
    const header = await createDidAuthHeader(privateKey, did, 'sync', body)
    return fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: header,
      },
      body,
    })
  }
  throw new Error('No authentication available: no UCAN token for space and no DID credentials provided')
}

/**
 * Pushes CRDT changes to the server
 */
export const pushChangesAsync = async (
  homeServerUrl: string,
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

  const body = JSON.stringify({ spaceId, changes: encryptedChanges })
  const response = await sendSyncRequestAsync(
    `${homeServerUrl}/sync/push`,
    body,
    spaceId,
    privateKey,
    did,
  )

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
  homeServerUrl: string,
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

  const body = JSON.stringify({
    spaceId,
    excludeDeviceId,
    afterCreatedAt,
    limit: limit ?? 100,
  })
  const response = await sendSyncRequestAsync(
    `${homeServerUrl}/sync/pull`,
    body,
    spaceId,
    privateKey,
    did,
  )

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
