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
  [spaceId: string]: {
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

export const engineLog = createLogger('SYNC ENGINE')
