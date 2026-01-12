/**
 * Sync Engine Types
 * Shared types and interfaces for sync engine operations
 */

import { createLogger } from '@/stores/logging'

/**
 * Type for CRDT change entries used in sync operations
 * Contains the fields needed for push/pull with haex-sync-server
 */
export interface CrdtChange {
  tableName: string
  rowPks: string
  columnName: string | null
  hlcTimestamp: string
  deviceId: string | null
  encryptedValue: string | null
  nonce: string | null
  createdAt: string
}

export interface VaultKeyCache {
  [vaultId: string]: {
    vaultKey: Uint8Array
    timestamp: number
  }
}

export interface SyncChangeData {
  deviceId?: string | null
  encryptedData: string
  nonce: string
}

export interface PullChangesResponse {
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
export async function fetchWithNetworkErrorHandling(
  url: string,
  options?: RequestInit,
): Promise<Response> {
  try {
    return await fetch(url, options)
  } catch (_networkError) {
    throw new Error(
      'NETWORK_ERROR: Cannot connect to sync server. Please check your internet connection.',
    )
  }
}

export const log = createLogger('SYNC ENGINE')
