import { eq } from 'drizzle-orm'
import * as schema from '~/database/schemas'
import * as crdtSchema from '~/database/schemas/crdt'
import type { Locale } from 'vue-i18n'
import { haexSyncBackends } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'

import {
  VaultSettingsKeyEnum,
  DesktopIconSizePreset,
} from '~/config/vault-settings'

export {
  VaultSettingsKeyEnum,
  DesktopIconSizePreset,
  iconSizePresetValues,
} from '~/config/vault-settings'

const log = createLogger('VAULT_SETTINGS')

export const useVaultSettingsStore = defineStore('vaultSettingsStore', () => {
  const { currentVault, currentVaultName } = storeToRefs(useVaultStore())
  const route = useRoute()

  // Check if we're in remote sync mode (don't create settings, wait for sync)
  const isRemoteSyncMode = computed(() => route.query.remoteSync === 'true')

  const {
    public: { haexVault },
  } = useRuntimeConfig()

  const syncLocaleAsync = async () => {
    try {
      const app = useNuxtApp()
      const db = requireDb()

      const currentLocaleRow =
        await db.query.haexVaultSettings.findFirst({
          where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.locale),
        })

      if (currentLocaleRow?.value) {
        const currentLocale = app.$i18n.availableLocales.find(
          (locale) => locale === currentLocaleRow.value,
        )
        await app.$i18n.setLocale(currentLocale ?? app.$i18n.defaultLocale)
      } else if (!isRemoteSyncMode.value) {
        // Only create new settings if NOT in remote sync mode
        // In remote sync mode, settings should come from the server
        await db.insert(schema.haexVaultSettings).values({
          id: crypto.randomUUID(),
          key: VaultSettingsKeyEnum.locale,
          value: app.$i18n.locale.value,
        })
      }
    } catch (error) {
      log.error('syncLocaleAsync failed:', error)
    }
  }

  const updateLocaleAsync = async (locale: Locale) => {
    const db = requireDb()
    await db
      .update(schema.haexVaultSettings)
      .set({ value: locale })
      .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.locale))
  }
  const syncThemeAsync = async () => {
    const { defaultTheme, currentTheme, currentThemeName, availableThemes } =
      storeToRefs(useUiStore())

    const db = requireDb()
    const currentThemeRow =
      await db.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.theme),
      })

    if (currentThemeRow?.value) {
      const theme = availableThemes.value.find(
        (theme) => theme.value === currentThemeRow.value,
      )
      currentThemeName.value = theme?.value || defaultTheme.value
    } else if (!isRemoteSyncMode.value) {
      // Only create new settings if NOT in remote sync mode
      // In remote sync mode, settings should come from the server
      await db.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.theme,
        value: currentTheme.value?.value,
      })
    }
  }

  const updateThemeAsync = async (theme: string) => {
    const db = requireDb()
    return await db
      .update(schema.haexVaultSettings)
      .set({ key: VaultSettingsKeyEnum.theme, value: theme })
      .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.theme))
  }

  const syncVaultNameAsync = async () => {
    const db = requireDb()
    const currentVaultNameRow =
      await db.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.vaultName),
      })

    if (currentVaultNameRow?.value) {
      currentVaultName.value =
        currentVaultNameRow.value || haexVault.defaultVaultName || 'HaexSpace'
    } else if (!isRemoteSyncMode.value) {
      // Only create new settings if NOT in remote sync mode
      // In remote sync mode, settings should come from the server
      await db.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.vaultName,
        value: currentVaultName.value,
      })
    }
  }

  const updateVaultNameAsync = async (newVaultName?: string | null) => {
    const vaultName = newVaultName || haexVault.defaultVaultName || 'HaexSpace'
    const db = requireDb()

    // Update locally in haex_vault_settings
    await db
      .update(schema.haexVaultSettings)
      .set({ value: vaultName })
      .where(eq(schema.haexVaultSettings.key, 'vaultName'))

    // Also update on sync server(s) if vault password is available
    await updateVaultNameOnServersAsync(vaultName)
  }

  /**
   * Updates the vault name on all enabled sync backends
   * Encrypts vault name with the identity's public key (ECDH)
   */
  const updateVaultNameOnServersAsync = async (newVaultName: string) => {
    const { currentVaultId } = storeToRefs(useVaultStore())
    const syncEngineStore = useSyncEngineStore()

    if (!currentVaultId.value) {
      return
    }

    const db = requireDb()

    // Get all enabled backends
    const backends = await db.query.haexSyncBackends.findMany({
      where: eq(haexSyncBackends.enabled, true),
    })

    for (const backend of backends) {
      if (!backend.spaceId) {
        continue
      }

      try {
        await syncEngineStore.updateVaultNameOnServerAsync(
          backend.id,
          backend.spaceId,
          newVaultName,
        )
      } catch (error) {
        log.error(`Failed to update vault name on server ${backend.name}:`, error)
        // Continue with other backends even if one fails
      }
    }
  }

  const syncDesktopIconSizeAsync = async () => {
    const db = requireDb()
    const iconSizeRow =
      await db.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.desktopIconSize),
      })

    if (!iconSizeRow?.id) {
      // Only create new settings if NOT in remote sync mode
      if (!isRemoteSyncMode.value) {
        await db.insert(schema.haexVaultSettings).values({
          id: crypto.randomUUID(),
          key: VaultSettingsKeyEnum.desktopIconSize,
          value: DesktopIconSizePreset.medium,
        })
      }
      return DesktopIconSizePreset.medium
    }

    return iconSizeRow.value as DesktopIconSizePreset
  }

  const updateDesktopIconSizeAsync = async (preset: DesktopIconSizePreset) => {
    const db = requireDb()
    return await db
      .update(schema.haexVaultSettings)
      .set({ value: preset })
      .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.desktopIconSize))
  }

  const DEFAULT_TOMBSTONE_RETENTION_DAYS = 30
  const DEFAULT_EXTERNAL_BRIDGE_PORT = 19455

  const getTombstoneRetentionDaysAsync = async (): Promise<number> => {
    const db = requireDb()
    const retentionRow =
      await db.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.tombstoneRetentionDays),
      })

    if (!retentionRow?.id) {
      // No entry exists, create one with default
      await db.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.tombstoneRetentionDays,
        value: String(DEFAULT_TOMBSTONE_RETENTION_DAYS),
      })
      return DEFAULT_TOMBSTONE_RETENTION_DAYS
    }

    return parseInt(retentionRow.value ?? String(DEFAULT_TOMBSTONE_RETENTION_DAYS), 10)
  }

  const updateTombstoneRetentionDaysAsync = async (days: number) => {
    const db = requireDb()
    const clampedDays = Math.max(1, Math.min(365, days))

    // Check if entry exists
    const existingRow =
      await db.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.tombstoneRetentionDays),
      })

    if (existingRow?.id) {
      await db
        .update(schema.haexVaultSettings)
        .set({ value: String(clampedDays) })
        .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.tombstoneRetentionDays))
    } else {
      await db.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.tombstoneRetentionDays,
        value: String(clampedDays),
      })
    }
  }

  const getExternalBridgePortAsync = async (): Promise<number> => {
    const db = requireDb()
    const portRow =
      await db.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.externalBridgePort),
      })

    if (!portRow?.id) {
      // No entry exists, create one with default
      await db.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.externalBridgePort,
        value: String(DEFAULT_EXTERNAL_BRIDGE_PORT),
      })
      return DEFAULT_EXTERNAL_BRIDGE_PORT
    }

    return parseInt(portRow.value ?? String(DEFAULT_EXTERNAL_BRIDGE_PORT), 10)
  }

  const updateExternalBridgePortAsync = async (port: number) => {
    const db = requireDb()
    // Validate port range (1024-65535, avoid system ports)
    const clampedPort = Math.max(1024, Math.min(65535, port))

    // Check if entry exists
    const existingRow =
      await db.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.externalBridgePort),
      })

    if (existingRow?.id) {
      await db
        .update(schema.haexVaultSettings)
        .set({ value: String(clampedPort) })
        .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.externalBridgePort))
    } else {
      await db.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.externalBridgePort,
        value: String(clampedPort),
      })
    }

    return clampedPort
  }

  /**
   * Check if initial sync has completed for this vault on THIS DEVICE.
   * Uses haex_crdt_configs_no_sync table which is local-only (not synced).
   * Each device tracks its own initial sync status independently.
   */
  const isInitialSyncCompleteAsync = async (): Promise<boolean> => {
    try {
      const db = requireDb()
      const result = await db.query.haexCrdtConfigs.findFirst({
        where: eq(crdtSchema.haexCrdtConfigs.key, 'initial_sync_complete'),
      })

      return result?.value === 'true'
    } catch (error) {
      log.error('Failed to check initial sync status:', error)
      return false
    }
  }

  /**
   * Mark initial sync as complete for this vault on THIS DEVICE.
   * Uses haex_crdt_configs_no_sync table which is local-only (not synced).
   * Each device tracks its own initial sync status independently.
   */
  const setInitialSyncCompleteAsync = async (): Promise<void> => {
    try {
      const db = requireDb()
      // Check if entry exists first (Drizzle's onConflictDoUpdate generates invalid SQLite syntax)
      const existing = await db.query.haexCrdtConfigs.findFirst({
        where: eq(crdtSchema.haexCrdtConfigs.key, 'initial_sync_complete'),
      })

      // Check for existing.key instead of just existing, because Drizzle findFirst
      // returns undefined when no rows are found (after drizzleCallback fix)
      if (existing?.key) {
        await db
          .update(crdtSchema.haexCrdtConfigs)
          .set({ value: 'true' })
          .where(eq(crdtSchema.haexCrdtConfigs.key, 'initial_sync_complete'))
      } else {
        await db
          .insert(crdtSchema.haexCrdtConfigs)
          .values({
            key: 'initial_sync_complete',
            type: 'sync',
            value: 'true',
          })
      }
    } catch (error) {
      log.error('Failed to set initial sync complete:', error)
    }
  }

  return {
    syncLocaleAsync,
    syncThemeAsync,
    syncVaultNameAsync,
    updateLocaleAsync,
    updateThemeAsync,
    updateVaultNameAsync,
    syncDesktopIconSizeAsync,
    updateDesktopIconSizeAsync,
    getTombstoneRetentionDaysAsync,
    updateTombstoneRetentionDaysAsync,
    getExternalBridgePortAsync,
    updateExternalBridgePortAsync,
    DEFAULT_EXTERNAL_BRIDGE_PORT,
    isInitialSyncCompleteAsync,
    setInitialSyncCompleteAsync,
  }
})
