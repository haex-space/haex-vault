/**
 * Sync Orchestrator Types
 * Shared types and interfaces for sync operations
 */

import type { ColumnChange } from '../tableScanner'
import { createLogger } from '@/stores/logging'

export interface SyncState {
  isConnected: boolean
  isSyncing: boolean
  error: string | null
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
 * FIFO mutex for sync operations.
 * Ensures only one sync operation runs at a time per backend.
 * Uses a queue-based approach to wake waiters one at a time,
 * preventing the race condition where multiple waiters resolve simultaneously.
 */
export class SyncMutex {
  private held: Set<string> = new Set()
  private queues: Map<string, Array<() => void>> = new Map()

  /**
   * Acquire lock for a backend. Returns a release function.
   * If lock is already held, waits in a FIFO queue until it's released.
   */
  async acquire(backendId: string): Promise<() => void> {
    if (this.held.has(backendId)) {
      await new Promise<void>((resolve) => {
        let queue = this.queues.get(backendId)
        if (!queue) {
          queue = []
          this.queues.set(backendId, queue)
        }
        queue.push(resolve)
      })
    }

    this.held.add(backendId)

    return () => {
      this.held.delete(backendId)
      const queue = this.queues.get(backendId)
      if (queue && queue.length > 0) {
        const next = queue.shift()!
        if (queue.length === 0) {
          this.queues.delete(backendId)
        }
        next()
      }
    }
  }

  /**
   * Check if a lock is currently held for a backend
   */
  isLocked(backendId: string): boolean {
    return this.held.has(backendId)
  }

  /**
   * Clear all locks and drain queues (use when resetting sync state)
   */
  reset(): void {
    this.held.clear()
    this.queues.clear()
  }
}

/**
 * Global sync mutex instance - shared across all sync operations
 */
export const syncMutex = new SyncMutex()

export class SpaceUnavailableError extends Error {
  constructor(public status: number, message: string) {
    super(message)
    this.name = 'SpaceUnavailableError'
  }
}
