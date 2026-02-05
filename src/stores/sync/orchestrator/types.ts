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
export const orchestratorLog = createLogger('SYNC')

/**
 * Simple mutex for sync operations.
 * Ensures only one sync operation runs at a time per backend.
 * Uses a Promise-based lock to handle concurrent async calls correctly.
 */
export class SyncMutex {
  private locks: Map<string, Promise<void>> = new Map()

  /**
   * Acquire lock for a backend. Returns a release function.
   * If lock is already held, waits until it's released.
   */
  async acquire(backendId: string): Promise<() => void> {
    // Wait for existing lock to be released (if any)
    while (this.locks.has(backendId)) {
      await this.locks.get(backendId)
    }

    // Create new lock
    let releaseFn: () => void
    const lockPromise = new Promise<void>((resolve) => {
      releaseFn = resolve
    })
    this.locks.set(backendId, lockPromise)

    // Return release function
    return () => {
      this.locks.delete(backendId)
      releaseFn!()
    }
  }

  /**
   * Check if a lock is currently held for a backend
   */
  isLocked(backendId: string): boolean {
    return this.locks.has(backendId)
  }

  /**
   * Clear all locks (use when resetting sync state)
   */
  reset(): void {
    this.locks.clear()
  }
}

/**
 * Global sync mutex instance - shared across all sync operations
 */
export const syncMutex = new SyncMutex()
