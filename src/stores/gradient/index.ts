import { and, eq } from 'drizzle-orm'
import * as schema from '~/database/schemas/haex'
import type { GradientVariant } from '~/types/gradient'
import { VaultSettingsTypeEnum } from '~/stores/vault/settings'

export enum GradientSettingsKeyEnum {
  gradientVariant = 'gradientVariant',
  gradientEnabled = 'gradientEnabled',
}

export const useGradientStore = defineStore('gradientStore', () => {
  const gradientVariant = ref<GradientVariant>('gitlab')
  const gradientEnabled = ref(true)

  const { currentVault } = storeToRefs(useVaultStore())
  const route = useRoute()

  // Check if we're in remote sync mode (don't create settings, wait for sync)
  const isRemoteSyncMode = computed(() => route.query.remoteSync === 'true')

  // Load gradient variant from database
  const syncGradientVariantAsync = async () => {
    try {
      const variantRow =
        await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
          where: and(
            eq(schema.haexVaultSettings.key, GradientSettingsKeyEnum.gradientVariant),
            eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.settings),
          ),
        })

      if (variantRow?.value && ['gitlab', 'ocean', 'sunset', 'forest'].includes(variantRow.value)) {
        gradientVariant.value = variantRow.value as GradientVariant
      } else if (!variantRow?.id && !isRemoteSyncMode.value) {
        // Only create default entry if NOT in remote sync mode
        await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
          key: GradientSettingsKeyEnum.gradientVariant,
          type: VaultSettingsTypeEnum.settings,
          value: 'gitlab',
        })
      }
    } catch (error) {
      console.error('Failed to sync gradient variant:', error)
    }
  }

  // Load gradient enabled state from database
  const syncGradientEnabledAsync = async () => {
    try {
      const enabledRow =
        await currentVault.value?.drizzle.query.haexVaultSettings.findFirst({
          where: and(
            eq(schema.haexVaultSettings.key, GradientSettingsKeyEnum.gradientEnabled),
            eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.settings),
          ),
        })

      if (enabledRow?.value !== undefined) {
        gradientEnabled.value = enabledRow.value === 'true'
      } else if (!enabledRow?.id && !isRemoteSyncMode.value) {
        // Only create default entry if NOT in remote sync mode
        await currentVault.value?.drizzle.insert(schema.haexVaultSettings).values({
          key: GradientSettingsKeyEnum.gradientEnabled,
          type: VaultSettingsTypeEnum.settings,
          value: 'true',
        })
      }
    } catch (error) {
      console.error('Failed to sync gradient enabled state:', error)
    }
  }

  // Update gradient variant in database
  const setGradientVariantAsync = async (variant: GradientVariant) => {
    try {
      await currentVault.value?.drizzle
        .update(schema.haexVaultSettings)
        .set({ value: variant })
        .where(
          and(
            eq(schema.haexVaultSettings.key, GradientSettingsKeyEnum.gradientVariant),
            eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.settings),
          ),
        )
      gradientVariant.value = variant
    } catch (error) {
      console.error('Failed to update gradient variant:', error)
      throw error
    }
  }

  // Update gradient enabled state in database
  const toggleGradientAsync = async (enabled: boolean) => {
    try {
      await currentVault.value?.drizzle
        .update(schema.haexVaultSettings)
        .set({ value: String(enabled) })
        .where(
          and(
            eq(schema.haexVaultSettings.key, GradientSettingsKeyEnum.gradientEnabled),
            eq(schema.haexVaultSettings.type, VaultSettingsTypeEnum.settings),
          ),
        )
      gradientEnabled.value = enabled
    } catch (error) {
      console.error('Failed to toggle gradient:', error)
      throw error
    }
  }

  return {
    gradientVariant,
    gradientEnabled,
    syncGradientVariantAsync,
    syncGradientEnabledAsync,
    setGradientVariantAsync,
    toggleGradientAsync,
  }
})
