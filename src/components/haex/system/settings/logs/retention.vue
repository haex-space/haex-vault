<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
    show-back
    @back="$emit('back')"
  >
    <!-- System Log Retention -->
    <div class="space-y-6">
      <div>
        <h3 class="text-sm font-medium mb-1">{{ t('system.title') }}</h3>
        <p class="text-xs text-muted mb-3">{{ t('system.description') }}</p>
        <div class="flex items-center gap-3">
          <span class="text-sm shrink-0">{{
            t('retention')
          }}</span>
          <USelect
            v-model="retentionDays"
            :items="retentionOptions"
            class="w-24"
            @update:model-value="saveRetentionAsync"
          />
          <span class="text-sm text-muted">{{ t('days') }}</span>
        </div>
      </div>

      <!-- Extension Log Retention -->
      <div v-if="extensionStore.availableExtensions.length > 0">
        <h3 class="text-sm font-medium mb-1">{{ t('extensions.title') }}</h3>
        <p class="text-xs text-muted mb-3">{{ t('extensions.description') }}</p>
        <div class="space-y-4">
          <div
            v-for="ext in extensionStore.availableExtensions"
            :key="ext.id"
            class="flex items-center justify-between gap-4"
          >
            <div class="flex items-center gap-3 min-w-0">
              <img
                v-if="ext.iconUrl"
                :src="ext.iconUrl"
                class="w-6 h-6 rounded"
              />
              <UIcon
                v-else
                name="i-lucide-puzzle"
                class="w-6 h-6 text-muted shrink-0"
              />
              <span class="text-sm font-medium truncate">{{
                ext.name
              }}</span>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <USelect
                :model-value="extensionRetention[ext.id] || retentionDays"
                :items="retentionOptions"
                class="w-24"
                @update:model-value="
                  (v: string) => saveExtensionRetentionAsync(ext.id, v)
                "
              />
              <span class="text-sm text-muted">{{
                t('days')
              }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexVaultSettings } from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const extensionStore = useExtensionsStore()
const { currentVault } = storeToRefs(useVaultStore())

const retentionDays = ref('14')
const extensionRetention = ref<Record<string, string>>({})

const retentionOptions = [
  { label: '1', value: '1' },
  { label: '3', value: '3' },
  { label: '7', value: '7' },
  { label: '14', value: '14' },
  { label: '30', value: '30' },
  { label: '60', value: '60' },
  { label: '90', value: '90' },
]

const loadRetentionAsync = async () => {
  if (!currentVault.value?.drizzle) return

  // System retention
  const row =
    await currentVault.value.drizzle.query.haexVaultSettings.findFirst({
      where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.logRetentionDays),
    })
  if (row?.value) retentionDays.value = row.value

  // Extension-specific retention
  const extRows = await currentVault.value.drizzle
    .select()
    .from(haexVaultSettings)

  for (const r of extRows) {
    if (r.key.startsWith('log_retention_days:') && r.value) {
      const extId = r.key.replace('log_retention_days:', '')
      extensionRetention.value[extId] = r.value
    }
  }
}

const saveRetentionAsync = async (value: string) => {
  if (!currentVault.value?.drizzle) return
  try {
    const existing =
      await currentVault.value.drizzle.query.haexVaultSettings.findFirst({
        where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.logRetentionDays),
      })

    if (existing) {
      await currentVault.value.drizzle
        .update(haexVaultSettings)
        .set({ value })
        .where(eq(haexVaultSettings.key, VaultSettingsKeyEnum.logRetentionDays))
    } else {
      await currentVault.value.drizzle.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.logRetentionDays,
        value,
      })
    }
    add({ title: t('saved'), color: 'success' })
  } catch (error) {
    console.error('Failed to save retention:', error)
    add({ title: t('saveFailed'), color: 'error' })
  }
}

const saveExtensionRetentionAsync = async (
  extensionId: string,
  value: string,
) => {
  if (!currentVault.value?.drizzle) return
  const key = `log_retention_days:${extensionId}`
  try {
    const existing =
      await currentVault.value.drizzle.query.haexVaultSettings.findFirst({
        where: eq(haexVaultSettings.key, key),
      })

    if (existing) {
      await currentVault.value.drizzle
        .update(haexVaultSettings)
        .set({ value })
        .where(eq(haexVaultSettings.key, key))
    } else {
      await currentVault.value.drizzle.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key,
        value,
      })
    }
    extensionRetention.value[extensionId] = value
    add({ title: t('saved'), color: 'success' })
  } catch (error) {
    console.error('Failed to save extension retention:', error)
    add({ title: t('saveFailed'), color: 'error' })
  }
}

onMounted(async () => {
  await loadRetentionAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Log-Einstellungen
  description: Aufbewahrungszeiten für System- und Erweiterungs-Logs
  retention: Aufbewahrungszeit
  days: Tage
  saved: Einstellung gespeichert
  saveFailed: Fehler beim Speichern
  system:
    title: System-Logs
    description: Aufbewahrungszeit für System- und Konsolen-Logs
  extensions:
    title: Erweiterungs-Logs
    description: Individuelle Aufbewahrungszeit pro Erweiterung. Wenn nicht gesetzt, gilt die System-Einstellung.
en:
  title: Log Settings
  description: Retention periods for system and extension logs
  retention: Retention
  days: days
  saved: Setting saved
  saveFailed: Failed to save
  system:
    title: System Logs
    description: Retention period for system and console logs
  extensions:
    title: Extension Logs
    description: Individual retention per extension. Falls back to the system setting if not configured.
</i18n>
