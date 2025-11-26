/**
 * Sync Orchestrator Types
 * Shared types and interfaces for sync operations
 */

import type { RealtimeChannel } from '@supabase/supabase-js'
import type { ColumnChange } from '../tableScanner'

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
 * Batch accumulator for realtime changes
 */
export interface BatchAccumulator {
  backendId: string
  changes: ColumnChange[]
  receivedCount: number
  totalCount: number
  timeout?: ReturnType<typeof setTimeout>
}

/**
 * Result from pulling changes from server
 */
export interface PullResult {
  changes: ColumnChange[]
  serverTimestamp: string | null
}

/**
 * Structured logging helper
 */
export const log = {
  info: (...args: unknown[]) => console.log('[SYNC]', ...args),
  warn: (...args: unknown[]) => console.warn('[SYNC]', ...args),
  error: (...args: unknown[]) => console.error('[SYNC]', ...args),
  debug: (...args: unknown[]) => console.log('[SYNC DEBUG]', ...args),
}
