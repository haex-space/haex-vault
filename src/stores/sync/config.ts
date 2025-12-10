/**
 * Sync Configuration Store - Manages global sync behavior settings
 *
 * Sync mechanisms:
 * - Push: Local changes are pushed to the server after a debounce delay
 * - Realtime: Changes from other devices are received instantly via subscription
 * - Fallback Pull: Periodic fetch to catch missed changes if connection was interrupted
 */

import { haexVaultSettings } from '@/database/schemas/haex'
import { eq } from 'drizzle-orm'

// Setting keys as constants
export const SYNC_SETTING_KEYS = {
  CONTINUOUS_DEBOUNCE_MS: 'sync_continuous_debounce_ms',
  PERIODIC_INTERVAL_MS: 'sync_periodic_interval_ms',
} as const

export interface SyncConfig {
  continuousDebounceMs: number // Debounce time before pushing local changes
  periodicIntervalMs: number // Interval for pulling remote changes
}

export const DEFAULT_SYNC_CONFIG: SyncConfig = {
  continuousDebounceMs: 1000, // Wait 1s after last change before pushing
  periodicIntervalMs: 300000, // Pull every 5 minutes (300000ms)
}

export const useSyncConfigStore = defineStore('syncConfigStore', () => {
  const config = ref<SyncConfig>({ ...DEFAULT_SYNC_CONFIG })
  const vaultStore = useVaultStore()

  /**
   * Loads sync configuration from database settings
   */
  const loadConfigAsync = async (): Promise<void> => {
    try {
      const db = vaultStore.currentVault?.drizzle
      if (!db) return

      // Load continuous debounce (push delay)
      const debounceResult = await db
        .select()
        .from(haexVaultSettings)
        .where(eq(haexVaultSettings.key, SYNC_SETTING_KEYS.CONTINUOUS_DEBOUNCE_MS))
        .limit(1)

      if (debounceResult.length > 0 && debounceResult[0]) {
        const debounce = Number.parseInt(debounceResult[0].value || '', 10)
        if (!Number.isNaN(debounce) && debounce > 0) {
          config.value.continuousDebounceMs = debounce
        }
      }

      // Load periodic interval (pull interval)
      const intervalResult = await db
        .select()
        .from(haexVaultSettings)
        .where(eq(haexVaultSettings.key, SYNC_SETTING_KEYS.PERIODIC_INTERVAL_MS))
        .limit(1)

      if (intervalResult.length > 0 && intervalResult[0]) {
        const interval = Number.parseInt(intervalResult[0].value || '', 10)
        if (!Number.isNaN(interval) && interval > 0) {
          config.value.periodicIntervalMs = interval
        }
      }

      console.log('Loaded sync config:', config.value)
    } catch (error) {
      console.error('Failed to load sync config:', error)
    }
  }

  /**
   * Upsert helper - SQLite doesn't support qualified column names in ON CONFLICT
   * So we do a manual check: update if exists, insert if not
   */
  const upsertSettingAsync = async (
    db: NonNullable<typeof vaultStore.currentVault>['drizzle'],
    key: string,
    value: string,
  ): Promise<void> => {
    // Check if setting exists
    const existing = await db
      .select()
      .from(haexVaultSettings)
      .where(eq(haexVaultSettings.key, key))
      .limit(1)

    if (existing.length > 0) {
      // Update existing
      await db
        .update(haexVaultSettings)
        .set({ value })
        .where(eq(haexVaultSettings.key, key))
    } else {
      // Insert new
      await db.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key,
        value,
        type: 'system',
      })
    }
  }

  /**
   * Saves sync configuration to database settings
   */
  const saveConfigAsync = async (
    newConfig: Partial<SyncConfig>,
  ): Promise<void> => {
    try {
      const db = vaultStore.currentVault?.drizzle
      if (!db) {
        throw new Error('Database not available')
      }

      // Update local config
      config.value = { ...config.value, ...newConfig }

      // Save each setting using manual upsert
      if (newConfig.continuousDebounceMs !== undefined) {
        await upsertSettingAsync(
          db,
          SYNC_SETTING_KEYS.CONTINUOUS_DEBOUNCE_MS,
          newConfig.continuousDebounceMs.toString(),
        )
      }

      if (newConfig.periodicIntervalMs !== undefined) {
        await upsertSettingAsync(
          db,
          SYNC_SETTING_KEYS.PERIODIC_INTERVAL_MS,
          newConfig.periodicIntervalMs.toString(),
        )
      }

      console.log('Saved sync config:', config.value)
    } catch (error) {
      console.error('Failed to save sync config:', error)
      throw error
    }
  }

  /**
   * Resets configuration to defaults
   */
  const resetConfigAsync = async (): Promise<void> => {
    await saveConfigAsync(DEFAULT_SYNC_CONFIG)
  }

  return {
    config: readonly(config),
    loadConfigAsync,
    saveConfigAsync,
    resetConfigAsync,
  }
})
