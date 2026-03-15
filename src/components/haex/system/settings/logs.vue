<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    sticky-header
  >
    <template #description>
      <div class="flex items-center gap-4">
        <span class="text-muted">{{ logs.length }} {{ t('entries') }}</span>
        <UButton
          v-if="logs.length > 0"
          :label="t('actions.copyAll')"
          icon="i-heroicons-clipboard-document"
          color="neutral"
          variant="outline"
          size="sm"
          @click="copyAllLogs"
        />
      </div>
    </template>

    <!-- Filters -->
    <div class="flex flex-wrap gap-3">
      <USelect
        v-model="filterLevel"
        :items="levelOptions"
        :placeholder="t('filter.level')"
        class="w-36"
      />
      <USelect
        v-model="filterSourceType"
        :items="sourceTypeOptions"
        :placeholder="t('filter.sourceType')"
        class="w-40"
      />
      <USelectMenu
        v-model="filterSource"
        :items="sourceOptions"
        value-key="value"
        :placeholder="t('filter.source')"
        class="w-48"
      />
      <UButton
        v-if="hasActiveFilters"
        :label="t('filter.reset')"
        color="neutral"
        variant="ghost"
        size="sm"
        icon="i-heroicons-x-mark"
        @click="resetFilters"
      />
    </div>

    <!-- Logs -->
    <div
      v-if="isLoading"
      class="flex items-center justify-center py-16"
    >
      <UIcon
        name="i-heroicons-arrow-path"
        class="w-8 h-8 animate-spin text-muted"
      />
    </div>

    <div
      v-else-if="logs.length === 0"
      class="text-center py-16"
    >
      <UIcon
        name="i-heroicons-document-text"
        class="w-12 h-12 mx-auto mb-2 opacity-30"
      />
      <p class="text-muted">{{ t('empty') }}</p>
    </div>

    <div
      v-else
      class="space-y-1.5 font-mono text-xs"
    >
      <div
        v-for="log in logs"
        :key="log.id"
        :class="[
          'p-3 rounded-lg border-l-4 group',
          levelStyles[log.level] || levelStyles.default,
        ]"
      >
        <div class="flex items-center gap-2 mb-1 flex-wrap">
          <span class="text-[10px] text-muted shrink-0">
            {{ formatTimestamp(log.timestamp) }}
          </span>
          <UBadge
            :color="levelColors[log.level] || 'neutral'"
            variant="subtle"
            size="xs"
          >
            {{ log.level }}
          </UBadge>
          <UBadge
            color="neutral"
            variant="outline"
            size="xs"
          >
            {{ log.sourceType === 'system' ? log.source : getExtensionName(log.source) }}
          </UBadge>
          <span
            v-if="log.deviceId"
            class="text-[10px] text-muted"
          >
            {{ log.deviceId.slice(0, 8) }}...
          </span>
        </div>
        <pre class="whitespace-pre-wrap wrap-break-word text-default">{{ log.message }}</pre>
        <pre
          v-if="log.metadata"
          class="mt-1 text-muted whitespace-pre-wrap wrap-break-word"
        >{{ formatMetadata(log.metadata) }}</pre>
      </div>

      <!-- Load More -->
      <UButton
        v-if="logs.length >= pageSize"
        :label="t('actions.loadMore')"
        block
        color="neutral"
        variant="ghost"
        @click="loadMore"
      />
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const { copy } = useClipboard()
const extensionStore = useExtensionsStore()

interface LogEntry {
  id: string
  timestamp: string
  level: string
  source: string
  sourceType: string
  message: string
  metadata: string | null
  deviceId: string
}

const isLoading = ref(true)
const logs = ref<LogEntry[]>([])
const pageSize = 100

const filterLevel = ref('warn')
const filterSourceType = ref<string | undefined>()
const filterSource = ref<string | undefined>()

const hasActiveFilters = computed(() =>
  filterLevel.value !== 'warn' || filterSourceType.value || filterSource.value,
)

const levelOptions = [
  { label: 'Debug', value: 'debug' },
  { label: 'Info', value: 'info' },
  { label: 'Warn', value: 'warn' },
  { label: 'Error', value: 'error' },
]

const sourceTypeOptions = computed(() => [
  { label: t('filter.all'), value: undefined },
  { label: 'System', value: 'system' },
  { label: 'Extension', value: 'extension' },
])

const sourceOptions = computed(() => {
  const sources = new Set(logs.value.map(l => l.source))
  return Array.from(sources).map(s => ({
    label: s,
    value: s,
  }))
})

const levelColors: Record<string, 'neutral' | 'info' | 'warning' | 'error'> = {
  debug: 'neutral',
  info: 'info',
  warn: 'warning',
  error: 'error',
}

const levelStyles: Record<string, string> = {
  error: 'bg-red-50 dark:bg-red-950/30 border-red-500',
  warn: 'bg-yellow-50 dark:bg-yellow-950/30 border-yellow-500',
  info: 'bg-blue-50 dark:bg-blue-950/30 border-blue-500',
  debug: 'bg-gray-50 dark:bg-gray-800/50 border-gray-400',
  default: 'bg-gray-50 dark:bg-gray-800 border-gray-400',
}

const formatTimestamp = (ts: string) => {
  try {
    return new Date(ts).toLocaleString()
  } catch {
    return ts
  }
}

const formatMetadata = (metadata: string | null) => {
  if (!metadata) return ''
  try {
    return JSON.stringify(JSON.parse(metadata), null, 2)
  } catch {
    return metadata
  }
}

const getExtensionName = (extensionId: string) => {
  const ext = extensionStore.availableExtensions.find(e => e.id === extensionId)
  return ext?.name ?? extensionId.slice(0, 12) + '...'
}

const fetchLogs = async (offset = 0) => {
  isLoading.value = offset === 0
  try {
    const result = await invoke<LogEntry[]>('log_read', {
      query: {
        level: filterLevel.value || null,
        sourceType: filterSourceType.value || null,
        source: filterSource.value || null,
        limit: pageSize,
        offset,
      },
    })
    if (offset === 0) {
      logs.value = result
    } else {
      logs.value = [...logs.value, ...result]
    }
  } catch (error) {
    console.error('Failed to fetch logs:', error)
  } finally {
    isLoading.value = false
  }
}

const loadMore = () => fetchLogs(logs.value.length)

const resetFilters = () => {
  filterLevel.value = 'warn'
  filterSourceType.value = undefined
  filterSource.value = undefined
}

const copyAllLogs = async () => {
  const text = logs.value
    .map(l => `[${l.timestamp}] [${l.level.toUpperCase()}] [${l.source}] ${l.message}`)
    .join('\n')
  await copy(text)
}

// Reload on filter change
watch([filterLevel, filterSourceType, filterSource], () => fetchLogs())

onMounted(() => fetchLogs())
</script>

<i18n lang="yaml">
de:
  title: Logs
  entries: Einträge
  empty: Keine Logs vorhanden
  filter:
    level: Log-Level
    sourceType: Quelle
    source: Modul
    all: Alle
    reset: Filter zurücksetzen
  actions:
    loadMore: Mehr laden
    copyAll: Alle kopieren
en:
  title: Logs
  entries: entries
  empty: No logs found
  filter:
    level: Log level
    sourceType: Source type
    source: Module
    all: All
    reset: Reset filters
  actions:
    loadMore: Load more
    copyAll: Copy all
</i18n>
