import { and, eq } from 'drizzle-orm'
import { z } from 'zod'
import * as schema from '~/database/schemas/haex'
import type { Locale } from 'vue-i18n'
import { subscribeToSyncUpdates, unsubscribeFromSyncUpdates } from '~/stores/sync/syncEvents'

export enum VaultSettingsTypeEnum {
  settings = 'settings',
  system = 'system',
}

export enum VaultSettingsKeyEnum {
  locale = 'locale',
  theme = 'theme',
  vaultName = 'vaultName',
  desktopIconSize = 'desktopIconSize',
}

export enum DesktopIconSizePreset {
  small = 'small',
  medium = 'medium',
  large = 'large',
  extraLarge = 'extra-large',
}

export const iconSizePresetValues: Record<DesktopIconSizePreset, number> = {
  [DesktopIconSizePreset.small]: 60,
  [DesktopIconSizePreset.medium]: 80,
  [DesktopIconSizePreset.large]: 120,
  [DesktopIconSizePreset.extraLarge]: 160,
}

export const vaultDeviceNameSchema = z.string().min(3).max(255)

export const useVaultSettingsStore = defineStore('vaultSettingsStore', () => {
  const { currentVault, currentVaultName } = storeToRefs(useVaultStore())

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
      } else {
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
    } else {
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
    } else {
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.vaultName,
        type: VaultSettingsTypeEnum.settings,
        value: currentVaultName.value,
      })
    }
  }

  const updateVaultNameAsync = async (newVaultName?: string | null) => {
    return currentVault.value?.drizzle
      .update(schema.haexVaultSettings)
      .set({ value: newVaultName || haexVault.defaultVaultName || 'HaexSpace' })
      .where(eq(schema.haexVaultSettings.key, 'vaultName'))
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
      // Kein Eintrag vorhanden, erstelle einen mit Default (medium)
      await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
        key: VaultSettingsKeyEnum.desktopIconSize,
        type: VaultSettingsTypeEnum.system,
        value: DesktopIconSizePreset.medium,
      })
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

  // Register for sync updates using the central event system
  const SUBSCRIPTION_ID = 'vaultSettingsStore'

  const startSyncListener = () => {
    subscribeToSyncUpdates(
      SUBSCRIPTION_ID,
      ['haex_vault_settings'],
      async () => {
        console.log('[VaultSettings] Sync update detected, reloading settings...')
        await syncThemeAsync()
        await syncLocaleAsync()
        await syncVaultNameAsync()
      },
    )
  }

  const stopSyncListener = () => {
    unsubscribeFromSyncUpdates(SUBSCRIPTION_ID)
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
    startSyncListener,
    stopSyncListener,
  }
})
