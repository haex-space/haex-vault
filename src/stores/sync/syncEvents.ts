/**
 * Sync Events - Central event bus for sync updates
 * Allows stores to register callbacks for specific table updates
 *
 * Also provides a central store reloader that automatically reloads
 * stores when their tables are updated via sync.
 */

import { listen } from '@tauri-apps/api/event'
import { createLogger } from '@/stores/logging'
import { createOnceListener } from '@/lib/once-listener'

// Internal event name for store reloading after sync pull
export const SYNC_TABLES_INTERNAL_EVENT = 'sync:tables-updated'

const log = createLogger('SYNC EVENTS')

type SyncUpdateCallback = (tables: string[]) => void | Promise<void>

interface TableSubscription {
  tables: string[] | '*' // '*' means all tables
  callback: SyncUpdateCallback
}

const subscriptions: Map<string, TableSubscription> = new Map()

// Central mapping of tables to store reload functions
// This is populated by registerStoreForTables()
const tableToReloadFn: Map<string, () => Promise<void>> = new Map()

// Coalescing window: a burst of sync:tables-updated events (e.g. one per
// peer-push during P2P sync) folds into a single flush. Without this, every
// event blocks the renderer through the sequential await-chain of reload
// functions and the UI freezes during sync activity.
const COALESCE_WINDOW_MS = 100

let pendingTables: Set<string> | null = null
let flushTimer: ReturnType<typeof setTimeout> | null = null
let isFlushing = false
// Bumped on stopSyncEvents(). A flush that was already running checks this
// after every await so it cannot notify subscriptions registered after a
// stop/restart cycle.
let flushGeneration = 0

const scheduleFlush = (): void => {
  if (flushTimer !== null || isFlushing) return
  flushTimer = setTimeout(() => {
    flushTimer = null
    void flushPendingAsync()
  }, COALESCE_WINDOW_MS)
}

const flushPendingAsync = async (): Promise<void> => {
  if (isFlushing || !pendingTables) return
  const generation = flushGeneration
  const tables = Array.from(pendingTables)
  pendingTables = null
  isFlushing = true

  try {
    log.debug('========== FLUSH sync:tables-updated ==========')
    log.debug('Tables (coalesced):', tables)
    log.debug('Registered tables:', Array.from(tableToReloadFn.keys()))

    const calledFns = new Set<() => Promise<void>>()
    for (const table of tables) {
      const reloadFn = tableToReloadFn.get(table)
      if (reloadFn && !calledFns.has(reloadFn)) {
        calledFns.add(reloadFn)
        try {
          await reloadFn()
        } catch (error) {
          log.error(`Error reloading store for table ${table}:`, error)
        }
        if (generation !== flushGeneration) return
      }
    }

    for (const [id, subscription] of subscriptions) {
      if (generation !== flushGeneration) return
      try {
        const isInterested =
          subscription.tables === '*' ||
          subscription.tables.some((t) => tables.includes(t))

        if (isInterested) {
          const relevantTables =
            subscription.tables === '*'
              ? tables
              : tables.filter((t) => subscription.tables.includes(t))

          log.debug(`Notifying subscription '${id}' for tables:`, relevantTables)
          await subscription.callback(relevantTables)
        }
      } catch (error) {
        log.error(`Error in subscription '${id}':`, error)
      }
    }
  } finally {
    isFlushing = false
    // Listener callbacks may have refilled `pendingTables` while we were
    // awaiting; reading through a helper avoids flow-narrowing the variable
    // to `null` based on the assignment in the try-block.
    if (generation === flushGeneration && hasPending()) {
      scheduleFlush()
    }
  }
}

const hasPending = (): boolean =>
  pendingTables !== null && pendingTables.size > 0

const listener = createOnceListener(() =>
  listen<{ tables: string[] }>(SYNC_TABLES_INTERNAL_EVENT, (event) => {
    const { tables } = event.payload
    if (!pendingTables) pendingTables = new Set()
    for (const table of tables) pendingTables.add(table)
    scheduleFlush()
  }),
)

/**
 * Register a store's reload function for specific tables.
 * When any of these tables are updated via sync, the reload function is called.
 * This is simpler than having each store subscribe individually.
 */
export const registerStoreForTables = (
  tables: string[],
  reloadFn: () => Promise<void>,
): void => {
  for (const table of tables) {
    tableToReloadFn.set(table, reloadFn)
  }
  log.debug('Registered reload function for tables:', tables)
}

/**
 * Initialize the sync events listener
 * Should be called once when the app starts
 */
export const initSyncEventsAsync = async (): Promise<void> => {
  await listener.initAsync()
  log.info('Initialized')
}

/**
 * Stop the sync events listener
 * Should be called when the app shuts down
 */
export const stopSyncEvents = (): void => {
  listener.dispose()
  flushGeneration += 1
  if (flushTimer !== null) {
    clearTimeout(flushTimer)
    flushTimer = null
  }
  pendingTables = null
  isFlushing = false
  subscriptions.clear()
  tableToReloadFn.clear()
  log.info('Stopped')
}

/**
 * Subscribe to sync updates for specific tables
 * @param id Unique identifier for this subscription
 * @param tables Array of table names to listen for, or '*' for all tables
 * @param callback Function to call when tables are updated
 */
export const subscribeToSyncUpdates = (
  id: string,
  tables: string[] | '*',
  callback: SyncUpdateCallback,
): void => {
  subscriptions.set(id, { tables, callback })
  log.debug(`Subscription '${id}' registered for tables:`, tables)
}

/**
 * Unsubscribe from sync updates
 * @param id The subscription identifier
 */
export const unsubscribeFromSyncUpdates = (id: string): void => {
  subscriptions.delete(id)
  log.debug(`Subscription '${id}' removed`)
}

/**
 * Composable for use in Vue components
 */
export const useSyncEvents = () => {
  return {
    initSyncEventsAsync,
    stopSyncEvents,
    subscribeToSyncUpdates,
    unsubscribeFromSyncUpdates,
    registerStoreForTables,
  }
}
