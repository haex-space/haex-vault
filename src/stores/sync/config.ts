/**
 * Sync Configuration Store - Manages global sync behavior settings
 *
 * Sync mechanisms:
 * - Push: Local changes are pushed to the server after a debounce delay
 * - Realtime: Changes from other devices are received instantly via WebSocket
 * - Fallback Pull: Periodic fetch to catch missed changes if connection was interrupted
 */

import { haexVaultSettings } from '@/database/schemas'
import { and, eq, isNull } from 'drizzle-orm'
import { createLogger } from '@/stores/logging'

// Setting keys as constants
export const SYNC_SETTING_KEYS = {
  CONTINUOUS_DEBOUNCE_MS: 'sync_continuous_debounce_ms',
  PERIODIC_INTERVAL_MS: 'sync_periodic_interval_ms',
  MAX_UCAN_CHAIN_DEPTH: 'max_ucan_chain_depth',
} as const

// Range enforced on both TS-writes and Rust-reads (rust/src/ucan/config.rs).
// Keep in sync with `MAX_UCAN_CHAIN_DEPTH_{MIN,MAX}` on the Rust side.
export const MAX_UCAN_CHAIN_DEPTH_MIN = 1
export const MAX_UCAN_CHAIN_DEPTH_MAX = 20
export const MAX_UCAN_CHAIN_DEPTH_DEFAULT = 5

export interface SyncConfig {
  continuousDebounceMs: number // Debounce time before pushing local changes
  periodicIntervalMs: number // Interval for pulling remote changes
  maxUcanChainDepth: number // Max UCAN delegation chain depth verified during sync
}

export const DEFAULT_SYNC_CONFIG: SyncConfig = {
  continuousDebounceMs: 1000, // Wait 1s after last change before pushing
  periodicIntervalMs: 300000, // Pull every 5 minutes (300000ms)
  maxUcanChainDepth: MAX_UCAN_CHAIN_DEPTH_DEFAULT,
}

/**
 * Parses a stored `max_ucan_chain_depth` value, applying range-validation
 * fallback (out-of-range → default) and falling back to the default for
 * unparseable input. Exported for unit-testing.
 */
export const parseMaxUcanChainDepth = (raw: string | null | undefined): number => {
  if (raw === null || raw === undefined) return MAX_UCAN_CHAIN_DEPTH_DEFAULT
  const parsed = Number.parseInt(raw, 10)
  if (Number.isNaN(parsed)) return MAX_UCAN_CHAIN_DEPTH_DEFAULT
  if (parsed < MAX_UCAN_CHAIN_DEPTH_MIN || parsed > MAX_UCAN_CHAIN_DEPTH_MAX) {
    return MAX_UCAN_CHAIN_DEPTH_DEFAULT
  }
  return parsed
}

const log = createLogger('SYNC_CONFIG')

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

      // Load max UCAN chain depth (verification cap during pull-apply).
      const depthResult = await db
        .select()
        .from(haexVaultSettings)
        .where(eq(haexVaultSettings.key, SYNC_SETTING_KEYS.MAX_UCAN_CHAIN_DEPTH))
        .limit(1)

      if (depthResult.length > 0 && depthResult[0]) {
        config.value.maxUcanChainDepth = parseMaxUcanChainDepth(depthResult[0].value)
      }

    } catch (error) {
      log.error('Failed to load sync config:', error)
    }
  }

  /**
   * Upsert helper - SQLite doesn't support qualified column names in ON CONFLICT
   * So we do a manual check: update if exists, insert if not.
   *
   * Scoped to the global row (`device_id IS NULL`) to match the Rust reader
   * (see `src-tauri/src/ucan/config.rs::read_max_ucan_chain_depth`, which
   * filters `WHERE key = ? AND device_id IS NULL`). Without this, a
   * device-scoped row for the same `key` could shadow the global one and
   * writes would non-deterministically target the wrong row.
   */
  const upsertSettingAsync = async (
    db: NonNullable<typeof vaultStore.currentVault>['drizzle'],
    key: string,
    value: string,
  ): Promise<void> => {
    // Check if setting exists (global row only — deviceId IS NULL).
    const existing = await db
      .select()
      .from(haexVaultSettings)
      .where(and(eq(haexVaultSettings.key, key), isNull(haexVaultSettings.deviceId)))
      .limit(1)

    if (existing.length > 0) {
      // Update existing global row.
      await db
        .update(haexVaultSettings)
        .set({ value })
        .where(and(eq(haexVaultSettings.key, key), isNull(haexVaultSettings.deviceId)))
    } else {
      // Insert new global row (deviceId explicitly null for readability).
      await db.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key,
        value,
        deviceId: null,
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

      // Validate up-front — reject bad input before mutating local reactive state.
      if (newConfig.maxUcanChainDepth !== undefined) {
        if (
          !Number.isInteger(newConfig.maxUcanChainDepth) ||
          newConfig.maxUcanChainDepth < MAX_UCAN_CHAIN_DEPTH_MIN ||
          newConfig.maxUcanChainDepth > MAX_UCAN_CHAIN_DEPTH_MAX
        ) {
          throw new Error(
            `max_ucan_chain_depth out of range [${MAX_UCAN_CHAIN_DEPTH_MIN}, ${MAX_UCAN_CHAIN_DEPTH_MAX}]`,
          )
        }
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

      if (newConfig.maxUcanChainDepth !== undefined) {
        await upsertSettingAsync(
          db,
          SYNC_SETTING_KEYS.MAX_UCAN_CHAIN_DEPTH,
          newConfig.maxUcanChainDepth.toString(),
        )
      }

    } catch (error) {
      log.error('Failed to save sync config:', error)
      throw error
    }
  }

  /**
   * Resets configuration to defaults
   */
  const resetConfigAsync = async (): Promise<void> => {
    await saveConfigAsync(DEFAULT_SYNC_CONFIG)
  }

  const reset = () => {
    config.value = { ...DEFAULT_SYNC_CONFIG }
  }

  return {
    config: readonly(config),
    loadConfigAsync,
    saveConfigAsync,
    resetConfigAsync,
    reset,
  }
})
