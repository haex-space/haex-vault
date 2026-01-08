/**
 * Sync Events - Central event bus for sync updates
 * Allows stores to register callbacks for specific table updates
 *
 * Also provides a central store reloader that automatically reloads
 * stores when their tables are updated via sync.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// Internal event name for store reloading after sync pull
export const SYNC_TABLES_INTERNAL_EVENT = 'sync:tables-updated'

type SyncUpdateCallback = (tables: string[]) => void | Promise<void>

interface TableSubscription {
  tables: string[] | '*' // '*' means all tables
  callback: SyncUpdateCallback
}

const subscriptions: Map<string, TableSubscription> = new Map()
let eventUnlisten: UnlistenFn | null = null
let isInitialized = false

// Central mapping of tables to store reload functions
// This is populated by registerStoreForTables()
const tableToReloadFn: Map<string, () => Promise<void>> = new Map()

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
  console.log(`[SyncEvents] Registered reload function for tables:`, tables)
}

/**
 * Unregister tables from the central reloader
 */
export const unregisterTablesFromReloader = (tables: string[]): void => {
  for (const table of tables) {
    tableToReloadFn.delete(table)
  }
}

/**
 * Initialize the sync events listener
 * Should be called once when the app starts
 */
export const initSyncEventsAsync = async (): Promise<void> => {
  if (isInitialized) return

  eventUnlisten = await listen<{ tables: string[] }>(
    SYNC_TABLES_INTERNAL_EVENT,
    async (event) => {
      const { tables } = event.payload
      console.log('[SyncEvents] ========== RECEIVED sync:tables-updated ==========')
      console.log('[SyncEvents] Tables:', tables)
      console.log('[SyncEvents] Registered tables:', Array.from(tableToReloadFn.keys()))

      // Track which reload functions we've already called to avoid duplicates
      const calledFns = new Set<() => Promise<void>>()

      // First, call the central reloader for each affected table
      for (const table of tables) {
        const reloadFn = tableToReloadFn.get(table)
        console.log(`[SyncEvents] Checking table "${table}" - has reload fn: ${!!reloadFn}, already called: ${calledFns.has(reloadFn!)}`)
        if (reloadFn && !calledFns.has(reloadFn)) {
          try {
            console.log(`[SyncEvents] >>> RELOADING store for table: ${table}`)
            await reloadFn()
            console.log(`[SyncEvents] <<< RELOAD COMPLETE for table: ${table}`)
            calledFns.add(reloadFn)
          } catch (error) {
            console.error(`[SyncEvents] Error reloading store for table ${table}:`, error)
          }
        }
      }

      // Then notify custom subscriptions (for stores that need special handling)
      for (const [id, subscription] of subscriptions) {
        try {
          // Check if this subscription is interested in any of the updated tables
          const isInterested =
            subscription.tables === '*' ||
            subscription.tables.some((t) => tables.includes(t))

          if (isInterested) {
            // Pass only the tables this subscription cares about
            const relevantTables =
              subscription.tables === '*'
                ? tables
                : tables.filter((t) => subscription.tables.includes(t))

            console.log(`[SyncEvents] Notifying subscription '${id}' for tables:`, relevantTables)
            await subscription.callback(relevantTables)
          }
        } catch (error) {
          console.error(`[SyncEvents] Error in subscription '${id}':`, error)
        }
      }
    },
  )

  isInitialized = true
  console.log('[SyncEvents] Initialized')
}

/**
 * Stop the sync events listener
 * Should be called when the app shuts down
 */
export const stopSyncEvents = (): void => {
  if (eventUnlisten) {
    eventUnlisten()
    eventUnlisten = null
  }
  subscriptions.clear()
  tableToReloadFn.clear()
  isInitialized = false
  console.log('[SyncEvents] Stopped')
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
  console.log(`[SyncEvents] Subscription '${id}' registered for tables:`, tables)
}

/**
 * Unsubscribe from sync updates
 * @param id The subscription identifier
 */
export const unsubscribeFromSyncUpdates = (id: string): void => {
  subscriptions.delete(id)
  console.log(`[SyncEvents] Subscription '${id}' removed`)
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
    unregisterTablesFromReloader,
  }
}
