/**
 * Sync Events - Central event bus for sync updates
 * Allows stores to register callbacks for specific table updates
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'

type SyncUpdateCallback = (tables: string[]) => void | Promise<void>

interface TableSubscription {
  tables: string[] | '*' // '*' means all tables
  callback: SyncUpdateCallback
}

const subscriptions: Map<string, TableSubscription> = new Map()
let eventUnlisten: UnlistenFn | null = null
let isInitialized = false

/**
 * Initialize the sync events listener
 * Should be called once when the app starts
 */
export const initSyncEventsAsync = async (): Promise<void> => {
  if (isInitialized) return

  eventUnlisten = await listen<{ tables: string[] }>(
    'sync:tables-updated',
    async (event) => {
      const { tables } = event.payload
      console.log('[SyncEvents] Tables updated:', tables)

      // Notify all subscriptions
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
  }
}
