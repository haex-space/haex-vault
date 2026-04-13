import { eq } from 'drizzle-orm'
import * as schema from '~/database/schemas'
import type { GradientVariant } from '~/types/gradient'
import { VaultSettingsKeyEnum } from '~/stores/vault/settings'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'

const log = createLogger('GRADIENT')

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
      const db = requireDb()
      const variantRow =
        await db.query.haexVaultSettings.findFirst({
          where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.gradientVariant),
        })

      if (variantRow?.value && ['gitlab', 'ocean', 'sunset', 'forest'].includes(variantRow.value)) {
        gradientVariant.value = variantRow.value as GradientVariant
      } else if (!variantRow?.id && !isRemoteSyncMode.value) {
        // Only create default entry if NOT in remote sync mode
        await db.insert(schema.haexVaultSettings).values({
          key: VaultSettingsKeyEnum.gradientVariant,
          value: 'gitlab',
        })
      }
    } catch (error) {
      log.error('Failed to sync gradient variant:', error)
    }
  }

  // Load gradient enabled state from database
  const syncGradientEnabledAsync = async () => {
    try {
      const db = requireDb()
      const enabledRow =
        await db.query.haexVaultSettings.findFirst({
          where: eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.gradientEnabled),
        })

      if (enabledRow?.value !== undefined) {
        gradientEnabled.value = enabledRow.value === 'true'
      } else if (!enabledRow?.id && !isRemoteSyncMode.value) {
        // Only create default entry if NOT in remote sync mode
        await db.insert(schema.haexVaultSettings).values({
          key: VaultSettingsKeyEnum.gradientEnabled,
          value: 'true',
        })
      }
    } catch (error) {
      log.error('Failed to sync gradient enabled state:', error)
    }
  }

  // Update gradient variant in database
  const setGradientVariantAsync = async (variant: GradientVariant) => {
    try {
      const db = requireDb()
      await db
        .update(schema.haexVaultSettings)
        .set({ value: variant })
        .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.gradientVariant))
      gradientVariant.value = variant
    } catch (error) {
      log.error('Failed to update gradient variant:', error)
      throw error
    }
  }

  // Update gradient enabled state in database
  const toggleGradientAsync = async (enabled: boolean) => {
    try {
      const db = requireDb()
      await db
        .update(schema.haexVaultSettings)
        .set({ value: String(enabled) })
        .where(eq(schema.haexVaultSettings.key, VaultSettingsKeyEnum.gradientEnabled))
      gradientEnabled.value = enabled
    } catch (error) {
      log.error('Failed to toggle gradient:', error)
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
