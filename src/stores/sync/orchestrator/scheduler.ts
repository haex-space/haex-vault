/**
 * Sync Scheduler
 * Owns the dirty-tables debounce timer, the adaptive debounce policy,
 * and the fallback-pull poll. Created once per store via createScheduler().
 */

import { useTimeoutPoll } from '@vueuse/core'
import { listen } from '@tauri-apps/api/event'
import { enterBulkMode, exitBulkMode } from '@/stores/logging'
import { orchestratorLog as log } from './types'

// Adaptive debouncing for bulk operations
// Tracks event frequency to detect bulk imports and increase debounce accordingly
const EVENT_WINDOW_MS = 1000 // Time window to count events
const BULK_THRESHOLD = 10 // Events in window to trigger bulk mode
const MAX_DEBOUNCE_MS = 5000 // Maximum debounce time during bulk operations

export interface SchedulerDeps {
  syncConfigStore: ReturnType<typeof useSyncConfigStore>
  syncBackendsStore: ReturnType<typeof useSyncBackendsStore>
  onLocalWriteAsync: () => Promise<void>
  pullFromBackendAsync: (backendId: string) => Promise<void>
}

export interface Scheduler {
  startDirtyTablesWatcherAsync: () => Promise<void>
  stopDirtyTablesWatcher: () => void
}

/**
 * Creates a scheduler scoped to a single store instance. All timers and
 * event-listener handles live inside this closure so multiple stores never
 * share state.
 */
export const createScheduler = (deps: SchedulerDeps): Scheduler => {
  const { syncConfigStore, syncBackendsStore, onLocalWriteAsync, pullFromBackendAsync } = deps

  let dirtyTablesDebounceTimer: ReturnType<typeof setTimeout> | null = null
  let fallbackPullPoll: ReturnType<typeof useTimeoutPoll> | null = null
  let eventUnlisten: (() => void) | null = null

  let eventTimestamps: number[] = []
  let currentDebounceMs: number | null = null // null = use config default
  let isInBulkMode = false

  /**
   * Calculates adaptive debounce time based on event frequency.
   * During bulk operations (like KeePass import), events flood in rapidly.
   * We detect this and increase debounce to prevent UI blocking.
   * Also activates bulk logging mode to suppress verbose logs.
   */
  const getAdaptiveDebounceMs = (): number => {
    const now = Date.now()
    const config = syncConfigStore.config

    // Add current timestamp
    eventTimestamps.push(now)

    // Remove old timestamps outside the window
    eventTimestamps = eventTimestamps.filter(t => now - t < EVENT_WINDOW_MS)

    // Calculate event rate
    const eventsInWindow = eventTimestamps.length

    if (eventsInWindow >= BULK_THRESHOLD) {
      // Bulk operation detected - scale debounce based on event rate
      // More events = longer debounce (up to MAX_DEBOUNCE_MS)
      const scaleFactor = Math.min(eventsInWindow / BULK_THRESHOLD, 5)
      currentDebounceMs = Math.min(config.continuousDebounceMs * scaleFactor, MAX_DEBOUNCE_MS)

      // Enter bulk logging mode to suppress verbose logs
      if (!isInBulkMode) {
        isInBulkMode = true
        enterBulkMode()
      }

      return currentDebounceMs
    }

    // Normal operation - use config default
    currentDebounceMs = null

    // Exit bulk logging mode if we were in it
    if (isInBulkMode) {
      isInBulkMode = false
      exitBulkMode()
    }

    return config.continuousDebounceMs
  }

  /**
   * Handles dirty tables event from Rust - triggers push with debounce
   * This runs in parallel with periodic pulls
   *
   * Uses adaptive debouncing: During bulk operations (many events in short time),
   * the debounce interval is automatically increased to prevent UI blocking.
   */
  const onDirtyTablesChangedAsync = async (): Promise<void> => {
    const config = syncConfigStore.config
    const adaptiveDebounce = getAdaptiveDebounceMs()
    const isBulkMode = adaptiveDebounce > config.continuousDebounceMs

    // Only log occasionally during bulk operations to reduce console spam
    if (!isBulkMode || eventTimestamps.length % 50 === 0) {
      const eventId = Math.random().toString(36).substring(7)
      if (isBulkMode) {
        log.info(`[DIRTY:${eventId}] Bulk operation detected (${eventTimestamps.length} events) - using ${adaptiveDebounce}ms debounce`)
      } else {
        log.debug(`[DIRTY:${eventId}] Event received, debounce: ${adaptiveDebounce}ms`)
      }
    }

    // Debounce to batch rapid changes before pushing
    if (dirtyTablesDebounceTimer) {
      clearTimeout(dirtyTablesDebounceTimer)
    }

    dirtyTablesDebounceTimer = setTimeout(async () => {
      // Reset event tracking after debounce fires
      eventTimestamps = []
      currentDebounceMs = null

      // Exit bulk logging mode
      if (isInBulkMode) {
        isInBulkMode = false
        exitBulkMode()
      }

      log.info(`[DIRTY] Debounce elapsed after ${adaptiveDebounce}ms, pushing changes...`)
      await onLocalWriteAsync()
      dirtyTablesDebounceTimer = null
    }, adaptiveDebounce)
  }

  /**
   * Stops the dirty tables watcher
   */
  const stopDirtyTablesWatcher = (): void => {
    if (dirtyTablesDebounceTimer) {
      clearTimeout(dirtyTablesDebounceTimer)
      dirtyTablesDebounceTimer = null
    }

    if (fallbackPullPoll) {
      fallbackPullPoll.pause()
      fallbackPullPoll = null
    }

    if (eventUnlisten) {
      eventUnlisten()
      eventUnlisten = null
    }

    log.debug('WATCHER: Stopped')
  }

  /**
   * Starts sync watchers:
   * - Push: Listens for dirty tables and pushes local changes with debounce
   * - Fallback Pull: Periodically fetches to catch missed realtime updates
   */
  const startDirtyTablesWatcherAsync = async (): Promise<void> => {
    log.info('[WATCHER] Starting sync watchers...')
    stopDirtyTablesWatcher()

    const config = syncConfigStore.config
    log.info('[WATCHER] Config:', config)

    // Start push watcher: Listen to dirty tables events.
    // Backend emits via emit_to("main", …); pin to the main window
    // because Tauri v2 drops bare default-Any listeners in prod.
    log.info('[WATCHER] Registering listener for crdt:dirty-tables-changed...')
    eventUnlisten = await listen('crdt:dirty-tables-changed', async () => {
      await onDirtyTablesChangedAsync()
    }, { target: 'main' })
    log.info(`[WATCHER] Push listener REGISTERED (debounce: ${config.continuousDebounceMs}ms)`)

    // Start fallback pull: Catch missed realtime updates
    // Start fallback pull: Catch missed realtime updates
    fallbackPullPoll = useTimeoutPoll(async () => {
      log.info('[WATCHER] Fallback pull timer elapsed - pulling from all backends')
      const enabledBackends = syncBackendsStore.enabledBackends
      for (const backend of enabledBackends) {
        try {
          await pullFromBackendAsync(backend.id)
        } catch (error) {
          log.error(`[WATCHER] Fallback pull failed for backend ${backend.id}:`, error)
        }
      }
    }, config.periodicIntervalMs)
    log.info(
      `[WATCHER] Fallback pull started (interval: ${config.periodicIntervalMs}ms)`,
    )

  }

  return {
    startDirtyTablesWatcherAsync,
    stopDirtyTablesWatcher,
  }
}
