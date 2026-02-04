import { and, eq } from 'drizzle-orm'
import { z } from 'zod'
import * as schema from '~/database/schemas/haex'
import * as crdtSchema from '~/database/schemas/crdt'
import type { Locale } from 'vue-i18n'
import { haexSyncBackends } from '~/database/schemas'

// Import for local use
import {
  VaultSettingsTypeEnum,
  VaultSettingsKeyEnum,
  DesktopIconSizePreset,
} from '~/config/vault-settings'

// Re-export constants from dedicated module for backwards compatibility
export {
  VaultSettingsTypeEnum,
  VaultSettingsKeyEnum,
  DesktopIconSizePreset,
  iconSizePresetValues,
} from '~/config/vault-settings'

export const vaultDeviceNameSchema = z.string().min(3).max(255)

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

      const currentLocaleRow =
        await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
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
        await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
          id: crypto.randomUUID(),
          key: VaultSettingsKeyEnum.locale,
          type: VaultSettingsTypeEnum.settings,
          value: app.$i18n.locale.value,
        })
      }
    } catch (error) {
      console.log('ERROR syncLocaleAsync', error)
    }
  }

  const updateLocaleAsync = async (locale: Locale) => {
    await currentVault.value?.drizzle
      .update(schema.haexVaultSettings)
      .set({ key: VaultSettingsKeyEnum.locale, value: locale })
      .where(
        and(
          eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.locale),
          eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.settings),
        ),
      )
  }
  const syncThemeAsync = async () => {
    const { defaultTheme, currentTheme, currentThemeName, availableThemes } =
      storeToRefs(useUiStore())

    const currentThemeRow =
      await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
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
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.theme,
        type: VaultSettingsTypeEnum.settings,
        value: currentTheme.value?.value,
      })
    }
  }

  const updateThemeAsync = async (theme: string) => {
    return await currentVault.value?.drizzle
      .update(schema.haexVaultSettings)
      .set({ key: VaultSettingsKeyEnum.theme, value: theme })
      .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.theme))
  }

  const syncVaultNameAsync = async () => {
    const currentVaultNameRow =
      await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
        where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.vaultName),
      })

    if (currentVaultNameRow?.value) {
      currentVaultName.value =
        currentVaultNameRow.value || haexVault.defaultVaultName || 'HaexSpace'
    } else if (!isRemoteSyncMode.value) {
      // Only create new settings if NOT in remote sync mode
      // In remote sync mode, settings should come from the server
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.vaultName,
        type: VaultSettingsTypeEnum.settings,
        value: currentVaultName.value,
      })
    }
  }

  const updateVaultNameAsync = async (newVaultName?: string | null) => {
    const vaultName = newVaultName || haexVault.defaultVaultName || 'HaexSpace'

    // Update locally in haex_vault_settings
    await currentVault.value?.drizzle
      .update(schema.haexVaultSettings)
      .set({ value: vaultName })
      .where(eq(schema.haexVaultSettings.key, 'vaultName'))

    // Also update on sync server(s) if vault password is available
    await updateVaultNameOnServersAsync(vaultName)
  }

  /**
   * Updates the vault name on all enabled sync backends
   * Uses the server password (stored in backend) to encrypt the vault name
   */
  const updateVaultNameOnServersAsync = async (newVaultName: string) => {
    const { currentVaultId } = storeToRefs(useVaultStore())
    const syncEngineStore = useSyncEngineStore()

    if (!currentVaultId.value || !currentVault.value?.drizzle) {
      console.log('[VaultSettings] No vault open, skipping server update')
      return
    }

    // Get all enabled backends
    const backends = await currentVault.value.drizzle.query.haexSyncBackends.findMany({
      where: eq(haexSyncBackends.enabled, true),
    })

    for (const backend of backends) {
      if (!backend.vaultId || !backend.password) {
        console.log(`[VaultSettings] Skipping ${backend.name}: missing vaultId or server password`)
        continue
      }

      try {
        await syncEngineStore.updateVaultNameOnServerAsync(
          backend.id,
          backend.vaultId,
          newVaultName,
          backend.password, // Server password for vault name encryption
        )
        console.log(`[VaultSettings] Vault name updated on server: ${backend.name}`)
      } catch (error) {
        console.error(`[VaultSettings] Failed to update vault name on server ${backend.name}:`, error)
        // Continue with other backends even if one fails
      }
    }
  }

  const syncDesktopIconSizeAsync = async () => {
    const iconSizeRow =
      await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.desktopIconSize),
          eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
        ),
      })

    if (!iconSizeRow?.id) {
      // Only create new settings if NOT in remote sync mode
      if (!isRemoteSyncMode.value) {
        await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
          id: crypto.randomUUID(),
          key: VaultSettingsKeyEnum.desktopIconSize,
          type: VaultSettingsTypeEnum.system,
          value: DesktopIconSizePreset.medium,
        })
      }
      return DesktopIconSizePreset.medium
    }

    return iconSizeRow.value as DesktopIconSizePreset
  }

  const updateDesktopIconSizeAsync = async (preset: DesktopIconSizePreset) => {
    return await currentVault.value?.drizzle
      .update(schema.haexVaultSettings)
      .set({ value: preset })
      .where(
        and(
          eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.desktopIconSize),
          eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
        ),
      )
  }

  const DEFAULT_TOMBSTONE_RETENTION_DAYS = 30
  const DEFAULT_EXTERNAL_BRIDGE_PORT = 19455

  const getTombstoneRetentionDaysAsync = async (): Promise<number> => {
    const retentionRow =
      await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.tombstoneRetentionDays),
          eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
        ),
      })

    if (!retentionRow?.id) {
      // No entry exists, create one with default
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.tombstoneRetentionDays,
        type: VaultSettingsTypeEnum.system,
        value: String(DEFAULT_TOMBSTONE_RETENTION_DAYS),
      })
      return DEFAULT_TOMBSTONE_RETENTION_DAYS
    }

    return parseInt(retentionRow.value ?? String(DEFAULT_TOMBSTONE_RETENTION_DAYS), 10)
  }

  const updateTombstoneRetentionDaysAsync = async (days: number) => {
    const clampedDays = Math.max(1, Math.min(365, days))

    // Check if entry exists
    const existingRow =
      await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.tombstoneRetentionDays),
          eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
        ),
      })

    if (existingRow?.id) {
      await currentVault.value?.drizzle
        .update(schema.haexVaultSettings)
        .set({ value: String(clampedDays) })
        .where(
          and(
            eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.tombstoneRetentionDays),
            eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
          ),
        )
    } else {
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.tombstoneRetentionDays,
        type: VaultSettingsTypeEnum.system,
        value: String(clampedDays),
      })
    }
  }

  const getExternalBridgePortAsync = async (): Promise<number> => {
    const portRow =
      await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.externalBridgePort),
          eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
        ),
      })

    if (!portRow?.id) {
      // No entry exists, create one with default
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.externalBridgePort,
        type: VaultSettingsTypeEnum.system,
        value: String(DEFAULT_EXTERNAL_BRIDGE_PORT),
      })
      return DEFAULT_EXTERNAL_BRIDGE_PORT
    }

    return parseInt(portRow.value ?? String(DEFAULT_EXTERNAL_BRIDGE_PORT), 10)
  }

  const updateExternalBridgePortAsync = async (port: number) => {
    // Validate port range (1024-65535, avoid system ports)
    const clampedPort = Math.max(1024, Math.min(65535, port))

    // Check if entry exists
    const existingRow =
      await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.externalBridgePort),
          eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
        ),
      })

    if (existingRow?.id) {
      await currentVault.value?.drizzle
        .update(schema.haexVaultSettings)
        .set({ value: String(clampedPort) })
        .where(
          and(
            eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.externalBridgePort),
            eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.system),
          ),
        )
    } else {
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.externalBridgePort,
        type: VaultSettingsTypeEnum.system,
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
    if (!currentVault.value?.drizzle) {
      return false
    }

    try {
      const result = await currentVault.value.drizzle.query.haexCrdtConfigs.findFirst({
        where: eq(crdtSchema.haexCrdtConfigs.key, 'initial_sync_complete'),
      })

      return result?.value === 'true'
    } catch (error) {
      console.error('[VaultSettings] Failed to check initial sync status:', error)
      return false
    }
  }

  /**
   * Mark initial sync as complete for this vault on THIS DEVICE.
   * Uses haex_crdt_configs_no_sync table which is local-only (not synced).
   * Each device tracks its own initial sync status independently.
   */
  const setInitialSyncCompleteAsync = async (): Promise<void> => {
    if (!currentVault.value?.drizzle) {
      return
    }

    try {
      // Check if entry exists first (Drizzle's onConflictDoUpdate generates invalid SQLite syntax)
      const existing = await currentVault.value.drizzle.query.haexCrdtConfigs.findFirst({
        where: eq(crdtSchema.haexCrdtConfigs.key, 'initial_sync_complete'),
      })

      // Check for existing.key instead of just existing, because Drizzle findFirst
      // returns undefined when no rows are found (after drizzleCallback fix)
      if (existing?.key) {
        await currentVault.value.drizzle
          .update(crdtSchema.haexCrdtConfigs)
          .set({ value: 'true' })
          .where(eq(crdtSchema.haexCrdtConfigs.key, 'initial_sync_complete'))
      } else {
        await currentVault.value.drizzle
          .insert(crdtSchema.haexCrdtConfigs)
          .values({
            key: 'initial_sync_complete',
            type: 'sync',
            value: 'true',
          })
      }
    } catch (error) {
      console.error('[VaultSettings] Failed to set initial sync complete:', error)
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
