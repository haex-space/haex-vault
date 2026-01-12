/**
 * Sync Orchestrator Types
 * Shared types and interfaces for sync operations
 */

import type { RealtimeChannel } from '@supabase/supabase-js'
import type { ColumnChange } from '../tableScanner'
import { createLogger } from '@/stores/logging'

export interface SyncState {
  isConnected: boolean
  isSyncing: boolean
  error: string | null
  subscription: RealtimeChannel | null
}

export interface BackendSyncState {
  [backendId: string]: SyncState
}

/**
 * Result from pulling changes from server
 */
export interface PullResult {
  changes: ColumnChange[]
  serverTimestamp: string | null
}

/**
 * Structured logging helper using central logger
 */
export const log = createLogger('SYNC')
