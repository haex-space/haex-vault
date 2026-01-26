<template>
  <UCard>
    <template #header>
      <div class="flex items-center justify-between">
        <h3 class="text-lg font-semibold">{{ t('limits') }}</h3>
        <UBadge
          v-if="limits?.isCustom"
          color="info"
          variant="subtle"
          size="xs"
        >
          {{ t('customLimits') }}
        </UBadge>
      </div>
    </template>

    <div
      v-if="loading"
      class="flex justify-center py-4"
    >
      <UIcon
        name="i-heroicons-arrow-path"
        class="w-6 h-6 animate-spin text-primary"
      />
    </div>

    <div
      v-else-if="limits"
      class="space-y-4"
    >
      <!-- Query Timeout -->
      <div class="flex items-center justify-between gap-4">
        <div class="flex-1">
          <div class="font-medium text-sm">{{ t('queryTimeout') }}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            {{ t('queryTimeoutDescription') }}
          </div>
        </div>
        <UInput
          v-model="editableLimits.queryTimeoutMs"
          type="number"
          :min="1000"
          :step="1000"
          class="w-28"
          size="sm"
        />
      </div>

      <!-- Max Result Rows -->
      <div class="flex items-center justify-between gap-4">
        <div class="flex-1">
          <div class="font-medium text-sm">{{ t('maxResultRows') }}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            {{ t('maxResultRowsDescription') }}
          </div>
        </div>
        <UInput
          v-model="editableLimits.maxResultRows"
          type="number"
          :min="100"
          :step="1000"
          class="w-28"
          size="sm"
        />
      </div>

      <!-- Max Concurrent Queries -->
      <div class="flex items-center justify-between gap-4">
        <div class="flex-1">
          <div class="font-medium text-sm">{{ t('maxConcurrentQueries') }}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            {{ t('maxConcurrentQueriesDescription') }}
          </div>
        </div>
        <UInput
          v-model="editableLimits.maxConcurrentQueries"
          type="number"
          :min="1"
          :max="50"
          class="w-28"
          size="sm"
        />
      </div>

      <!-- Max Query Size -->
      <div class="flex items-center justify-between gap-4">
        <div class="flex-1">
          <div class="font-medium text-sm">{{ t('maxQuerySize') }}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            {{ t('maxQuerySizeDescription') }}
          </div>
        </div>
        <UInput
          v-model="editableLimits.maxQuerySizeKb"
          type="number"
          :min="1"
          :step="64"
          class="w-28"
          size="sm"
        />
      </div>

      <!-- Action Buttons -->
      <div class="flex flex-col @md:flex-row @md:justify-end gap-2 pt-2">
        <UiButton
          v-if="limits.isCustom"
          :label="t('resetToDefaults')"
          icon="i-heroicons-arrow-path"
          variant="outline"
          :loading="resetting"
          block
          class="@md:w-auto"
          @click="resetLimitsAsync"
        />
        <UiButton
          :label="t('saveLimits')"
          :loading="saving"
          :disabled="!hasChanges"
          block
          class="@md:w-auto"
          @click="saveLimitsAsync"
        />
      </div>
    </div>
  </UCard>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { ExtensionLimitsResponse } from '~~/src-tauri/bindings/ExtensionLimitsResponse'

interface EditableLimits {
  queryTimeoutMs: number
  maxResultRows: number
  maxConcurrentQueries: number
  maxQuerySizeKb: number
}

const props = defineProps<{
  extensionId: string
}>()

const { t } = useI18n()
const { add } = useToast()

const loading = ref(true)
const saving = ref(false)
const resetting = ref(false)
const limits = ref<ExtensionLimitsResponse | null>(null)
const originalLimits = ref<EditableLimits | null>(null)
const editableLimits = ref<EditableLimits>({
  queryTimeoutMs: 30000,
  maxResultRows: 10000,
  maxConcurrentQueries: 5,
  maxQuerySizeKb: 1024,
})

const hasChanges = computed(() => {
  if (!originalLimits.value) return false
  return (
    editableLimits.value.queryTimeoutMs !== originalLimits.value.queryTimeoutMs ||
    editableLimits.value.maxResultRows !== originalLimits.value.maxResultRows ||
    editableLimits.value.maxConcurrentQueries !== originalLimits.value.maxConcurrentQueries ||
    editableLimits.value.maxQuerySizeKb !== originalLimits.value.maxQuerySizeKb
  )
})

const loadLimitsAsync = async () => {
  loading.value = true
  try {
    const response = await invoke<ExtensionLimitsResponse>(
      'get_extension_limits',
      { extensionId: props.extensionId },
    )
    limits.value = response

    const editable: EditableLimits = {
      queryTimeoutMs: Number(response.queryTimeoutMs),
      maxResultRows: Number(response.maxResultRows),
      maxConcurrentQueries: Number(response.maxConcurrentQueries),
      maxQuerySizeKb: Math.round(Number(response.maxQuerySizeBytes) / 1024),
    }
    editableLimits.value = editable
    originalLimits.value = { ...editable }
  } catch (error) {
    console.error('Error loading limits:', error)
    add({ description: t('limitsLoadError'), color: 'error' })
  } finally {
    loading.value = false
  }
}

const saveLimitsAsync = async () => {
  saving.value = true
  try {
    const response = await invoke<ExtensionLimitsResponse>(
      'update_extension_limits',
      {
        request: {
          extensionId: props.extensionId,
          queryTimeoutMs: BigInt(editableLimits.value.queryTimeoutMs),
          maxResultRows: BigInt(editableLimits.value.maxResultRows),
          maxConcurrentQueries: BigInt(editableLimits.value.maxConcurrentQueries),
          maxQuerySizeBytes: BigInt(editableLimits.value.maxQuerySizeKb * 1024),
        },
      },
    )
    limits.value = response
    originalLimits.value = { ...editableLimits.value }
    add({ description: t('limitsSaved'), color: 'success' })
  } catch (error) {
    console.error('Error saving limits:', error)
    add({ description: t('limitsSaveError'), color: 'error' })
  } finally {
    saving.value = false
  }
}

const resetLimitsAsync = async () => {
  resetting.value = true
  try {
    const response = await invoke<ExtensionLimitsResponse>(
      'reset_extension_limits',
      { extensionId: props.extensionId },
    )
    limits.value = response

    const editable: EditableLimits = {
      queryTimeoutMs: Number(response.queryTimeoutMs),
      maxResultRows: Number(response.maxResultRows),
      maxConcurrentQueries: Number(response.maxConcurrentQueries),
      maxQuerySizeKb: Math.round(Number(response.maxQuerySizeBytes) / 1024),
    }
    editableLimits.value = editable
    originalLimits.value = { ...editable }
    add({ description: t('limitsReset'), color: 'success' })
  } catch (error) {
    console.error('Error resetting limits:', error)
    add({ description: t('limitsResetError'), color: 'error' })
  } finally {
    resetting.value = false
  }
}

onMounted(() => {
  void loadLimitsAsync()
})
</script>

<i18n lang="yaml">
de:
  limits: Limits
  customLimits: Angepasst
  queryTimeout: Query-Timeout (ms)
  queryTimeoutDescription: Maximale Zeit für eine Datenbankabfrage.
  maxResultRows: Max. Ergebniszeilen
  maxResultRowsDescription: Maximale Anzahl an Zeilen pro Abfrage.
  maxConcurrentQueries: Max. parallele Abfragen
  maxConcurrentQueriesDescription: Maximale Anzahl gleichzeitiger Datenbankabfragen.
  maxQuerySize: Max. Query-Größe (KB)
  maxQuerySizeDescription: Maximale Größe einer SQL-Abfrage.
  saveLimits: Limits speichern
  resetToDefaults: Auf Standard zurücksetzen
  limitsLoadError: Fehler beim Laden der Limits
  limitsSaved: Limits gespeichert
  limitsSaveError: Fehler beim Speichern der Limits
  limitsReset: Limits auf Standard zurückgesetzt
  limitsResetError: Fehler beim Zurücksetzen der Limits
en:
  limits: Limits
  customLimits: Custom
  queryTimeout: Query Timeout (ms)
  queryTimeoutDescription: Maximum time for a database query.
  maxResultRows: Max Result Rows
  maxResultRowsDescription: Maximum number of rows per query.
  maxConcurrentQueries: Max Concurrent Queries
  maxConcurrentQueriesDescription: Maximum number of simultaneous database queries.
  maxQuerySize: Max Query Size (KB)
  maxQuerySizeDescription: Maximum size of a SQL query.
  saveLimits: Save Limits
  resetToDefaults: Reset to Defaults
  limitsLoadError: Error loading limits
  limitsSaved: Limits saved
  limitsSaveError: Error saving limits
  limitsReset: Limits reset to defaults
  limitsResetError: Error resetting limits
</i18n>
