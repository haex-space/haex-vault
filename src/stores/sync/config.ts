/**
 * Sync Configuration Store - Manages global sync behavior settings
 * Controls how and when sync operations are triggered
 */

import { haexVaultSettings } from '@/database/schemas/haex'
import { eq } from 'drizzle-orm'

export type SyncMode = 'continuous' | 'periodic'

// Setting keys as constants
export const SYNC_SETTING_KEYS = {
  MODE: 'sync_mode',
  CONTINUOUS_DEBOUNCE_MS: 'sync_continuous_debounce_ms',
  PERIODIC_INTERVAL_MS: 'sync_periodic_interval_ms',
} as const

export interface SyncConfig {
  mode: SyncMode
  continuousDebounceMs: number // Debounce time in continuous mode to batch rapid changes
  periodicIntervalMs: number // Interval for periodic sync mode
}

export const DEFAULT_SYNC_CONFIG: SyncConfig = {
  mode: 'continuous',
  continuousDebounceMs: 1000, // Wait 1s after last change before syncing (batch rapid changes)
  periodicIntervalMs: 30000, // Sync every 30 seconds in periodic mode
}

export const useSyncConfigStore = defineStore('syncConfigStore', () => {
  const config = ref<SyncConfig>({ ...DEFAULT_SYNC_CONFIG })
  const vaultStore = useVaultStore()

  /**
   * Loads sync configuration from database settings
   */
  const loadConfigAsync = async (): Promise<void> => {
    try {
      const db = vaultStore.db
      if (!db) return

      // Load sync mode
      const modeResult = await db
        .select()
        .from(haexVaultSettings)
        .where(eq(haexVaultSettings.key, SYNC_SETTING_KEYS.MODE))
        .limit(1)

      if (modeResult.length > 0 && modeResult[0]) {
        const mode = modeResult[0].value
        if (mode === 'continuous' || mode === 'periodic') {
          config.value.mode = mode
        }
      }

      // Load continuous debounce
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

      // Load periodic interval
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
   * Saves sync configuration to database settings
   */
  const saveConfigAsync = async (
    newConfig: Partial<SyncConfig>,
  ): Promise<void> => {
    try {
      const db = vaultStore.db
      if (!db) {
        throw new Error('Database not available')
      }

      // Update local config
      config.value = { ...config.value, ...newConfig }

      // Save each setting
      if (newConfig.mode !== undefined) {
        await db
          .insert(haexVaultSettings)
          .values({
            id: crypto.randomUUID(),
            key: SYNC_SETTING_KEYS.MODE,
            value: newConfig.mode,
            type: 'system',
          })
          .onConflictDoUpdate({
            target: haexVaultSettings.key,
            set: { value: newConfig.mode },
          })
      }

      if (newConfig.continuousDebounceMs !== undefined) {
        await db
          .insert(haexVaultSettings)
          .values({
            id: crypto.randomUUID(),
            key: SYNC_SETTING_KEYS.CONTINUOUS_DEBOUNCE_MS,
            value: newConfig.continuousDebounceMs.toString(),
            type: 'system',
          })
          .onConflictDoUpdate({
            target: haexVaultSettings.key,
            set: { value: newConfig.continuousDebounceMs.toString() },
          })
      }

      if (newConfig.periodicIntervalMs !== undefined) {
        await db
          .insert(haexVaultSettings)
          .values({
            id: crypto.randomUUID(),
            key: SYNC_SETTING_KEYS.PERIODIC_INTERVAL_MS,
            value: newConfig.periodicIntervalMs.toString(),
            type: 'system',
          })
          .onConflictDoUpdate({
            target: haexVaultSettings.key,
            set: { value: newConfig.periodicIntervalMs.toString() },
          })
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
