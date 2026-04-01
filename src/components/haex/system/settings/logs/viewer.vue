<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    show-back
    @back="$emit('back')"
  >
    <div class="flex flex-col h-full gap-4">
      <!-- Filters (fixed) -->
      <div class="shrink-0 space-y-2">
        <UInput
          v-model="filterSearch"
          :placeholder="t('filter.search')"
          icon="i-lucide-search"
        />
        <div class="flex flex-wrap gap-2">
          <USelect
            v-model="filterLevel"
            :items="levelOptions"
            :placeholder="t('filter.level')"
            class="min-w-24"
          />
          <USelect
            v-model="filterSource"
            :items="sourceOptions"
            :placeholder="t('filter.source')"
            class="min-w-36 flex-1"
          />
          <USelect
            v-model="filterDevice"
            :items="deviceOptions"
            :placeholder="t('filter.device')"
            class="min-w-32 flex-1"
          />
          <USelect
            v-model="filterTime"
            :items="timeOptions"
            class="min-w-52 flex-1"
          />
          <UButton
            v-if="hasActiveFilters"
            :label="t('filter.reset')"
            color="neutral"
            variant="ghost"
            icon="i-heroicons-x-mark"
            @click="resetFilters"
          />
        </div>
      </div>
      <div class="shrink-0 flex items-center justify-end gap-2 px-3">
        <span class="text-sm text-muted"
          >{{ filteredLogs.length }} {{ t('entries') }}</span
        >
        <UButton
          v-if="filteredLogs.length > 0"
          icon="i-heroicons-clipboard-document"
          color="neutral"
          variant="ghost"
          :title="t('actions.copyAll')"
          @click="copyAllLogs"
        />
        <UButton
          v-if="logs.length > 0"
          icon="i-lucide-trash-2"
          color="error"
          variant="ghost"
          :title="t('actions.clearAll')"
          @click="clearAllLogsAsync"
        />
      </div>

      <!-- Log entries (scrollable) -->
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
        v-else-if="filteredLogs.length === 0"
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
        class="flex-1 min-h-0 overflow-y-auto space-y-1.5 font-mono text-xs"
      >
        <div
          v-for="log in filteredLogs"
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
            >
              {{ log.level }}
            </UBadge>
            <UBadge
              color="neutral"
              variant="outline"
            >
              {{ getSourceLabel(log) }}
            </UBadge>
            <span
              v-if="log.deviceId"
              class="text-[10px] text-muted"
            >
              {{ log.deviceId.slice(0, 8) }}...
            </span>
            <div class="flex-1" />
            <UButton
              icon="i-lucide-copy"
              color="neutral"
              variant="ghost"
              class="shrink-0"
              @click="copyLogEntry(log)"
            />
            <UButton
              icon="i-lucide-trash-2"
              color="error"
              variant="ghost"
              class="shrink-0"
              @click="deleteLogAsync(log.id)"
            />
          </div>
          <pre class="overflow-x-auto text-default">{{
            log.message
          }}</pre>
          <pre
            v-if="log.metadata"
            class="mt-1 text-muted overflow-x-auto"
            >{{ formatMetadata(log.metadata) }}</pre
          >
        </div>

        <!-- Load More -->
        <UButton
          v-if="filteredLogs.length >= pageSize"
          :label="t('actions.loadMore')"
          block
          color="neutral"
          variant="ghost"
          @click="loadMore"
        />
      </div>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { copy } = useClipboard()
const extensionStore = useExtensionsStore()
const deviceStore = useDeviceStore()

interface LogEntry {
  id: string
  timestamp: string
  level: string
  source: string
  extensionId: string | null
  message: string
  metadata: string | null
  deviceId: string
}

const isLoading = ref(true)
const logs = ref<LogEntry[]>([])
const pageSize = 100

const filterLevel = ref('warn')
const ALL = '__all__'
const filterSource = ref(ALL)
const filterDevice = ref(ALL)
const filterTime = ref('all')
const filterSearch = ref('')

const hasActiveFilters = computed(
  () =>
    filterLevel.value !== 'warn' ||
    filterSource.value !== ALL ||
    filterDevice.value !== ALL ||
    filterTime.value !== 'all' ||
    filterSearch.value !== '',
)

const timeOptions = computed(() => [
  { label: t('filter.time.all'), value: 'all' },
  { label: t('filter.time.15m'), value: '15m' },
  { label: t('filter.time.1h'), value: '1h' },
  { label: t('filter.time.6h'), value: '6h' },
  { label: t('filter.time.24h'), value: '24h' },
  { label: t('filter.time.7d'), value: '7d' },
  { label: t('filter.time.30d'), value: '30d' },
])

const getSinceTimestamp = (): string | null => {
  const now = Date.now()
  const durations: Record<string, number> = {
    '15m': 15 * 60 * 1000,
    '1h': 60 * 60 * 1000,
    '6h': 6 * 60 * 60 * 1000,
    '24h': 24 * 60 * 60 * 1000,
    '7d': 7 * 24 * 60 * 60 * 1000,
    '30d': 30 * 24 * 60 * 60 * 1000,
  }
  const ms = durations[filterTime.value]
  if (!ms) return null
  return new Date(now - ms).toISOString()
}

// Client-side text search (applied after server-side filters)
const filteredLogs = computed(() => {
  if (!filterSearch.value) return logs.value
  const q = filterSearch.value.toLowerCase()
  return logs.value.filter(
    (l) =>
      l.message.toLowerCase().includes(q) ||
      l.source.toLowerCase().includes(q) ||
      (l.metadata && l.metadata.toLowerCase().includes(q)),
  )
})

const levelOptions = [
  { label: 'Debug', value: 'debug' },
  { label: 'Info', value: 'info' },
  { label: 'Warn', value: 'warn' },
  { label: 'Error', value: 'error' },
]

const sourceOptions = computed(() => {
  const systemSources = new Set<string>()
  for (const log of logs.value) {
    if (!log.extensionId) {
      systemSources.add(log.source)
    }
  }

  const options: { label: string; value: string }[] = [
    { label: t('filter.all'), value: ALL },
  ]
  for (const source of systemSources) {
    options.push({ label: `System: ${source}`, value: `system:${source}` })
  }
  for (const ext of extensionStore.availableExtensions) {
    options.push({ label: ext.name, value: `ext:${ext.id}` })
  }
  return options
})

const deviceOptions = computed(() => {
  const deviceIds = new Set<string>()
  for (const log of logs.value) {
    if (log.deviceId) deviceIds.add(log.deviceId)
  }
  const options: { label: string; value: string }[] = [
    { label: t('filter.allDevices'), value: ALL },
  ]
  for (const id of deviceIds) {
    options.push({
      label: deviceStore.getDeviceName(id),
      value: id,
    })
  }
  return options
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

const getSourceLabel = (log: LogEntry) => {
  if (log.extensionId) {
    const ext = extensionStore.availableExtensions.find(
      (e) => e.id === log.extensionId,
    )
    return ext?.name ?? log.extensionId.slice(0, 12) + '...'
  }
  return log.source
}

const fetchLogs = async (offset = 0) => {
  isLoading.value = offset === 0
  try {
    let source: string | null = null
    let extensionId: string | null = null

    if (filterSource.value && filterSource.value !== ALL) {
      if (filterSource.value.startsWith('system:')) {
        source = filterSource.value.slice(7)
      } else if (filterSource.value.startsWith('ext:')) {
        extensionId = filterSource.value.slice(4)
      }
    }

    const result = await invoke<LogEntry[]>('log_read', {
      query: {
        level: filterLevel.value || null,
        extensionId,
        source,
        deviceId: filterDevice.value !== ALL ? filterDevice.value : null,
        since: getSinceTimestamp(),
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
  filterSource.value = ALL
  filterDevice.value = ALL
  filterTime.value = 'all'
  filterSearch.value = ''
}

const copyLogEntry = async (log: LogEntry) => {
  const text = `[${log.timestamp}] [${log.level.toUpperCase()}] [${getSourceLabel(log)}] ${log.message}${log.metadata ? '\n' + formatMetadata(log.metadata) : ''}`
  await copy(text)
}

const deleteLogAsync = async (id: string) => {
  try {
    await invoke('log_delete', { ids: [id] })
    logs.value = logs.value.filter((l) => l.id !== id)
  } catch (error) {
    console.error('Failed to delete log:', error)
  }
}

const clearAllLogsAsync = async () => {
  try {
    await invoke('log_clear_all')
    logs.value = []
  } catch (error) {
    console.error('Failed to clear logs:', error)
  }
}

const copyAllLogs = async () => {
  const text = filteredLogs.value
    .map(
      (l) =>
        `[${l.timestamp}] [${l.level.toUpperCase()}] [${getSourceLabel(l)}] ${l.message}`,
    )
    .join('\n')
  await copy(text)
}

// Reload on filter change
watch([filterLevel, filterSource, filterDevice, filterTime], () => fetchLogs())

onMounted(async () => {
  await deviceStore.loadKnownDevicesAsync()
  await fetchLogs()
})
</script>

<i18n lang="yaml">
de:
  title: Logs anzeigen
  entries: Einträge
  empty: Keine Logs vorhanden
  filter:
    all: Alle Quellen
    allDevices: Alle Geräte
    level: Log-Level
    source: Quelle
    search: Suche...
    device: Gerät
    reset: Filter zurücksetzen
    time:
      all: Gesamter Zeitraum
      15m: Letzte 15 Min
      1h: Letzte Stunde
      6h: Letzte 6 Stunden
      24h: Letzte 24 Stunden
      7d: Letzte 7 Tage
      30d: Letzte 30 Tage
  actions:
    loadMore: Mehr laden
    copyAll: Alle kopieren
    copyEntry: Eintrag kopieren
    clearAll: Alle Logs löschen
en:
  title: View Logs
  entries: entries
  empty: No logs found
  filter:
    all: All sources
    allDevices: All devices
    level: Log level
    source: Source
    search: Search...
    device: Device
    reset: Reset filters
    time:
      all: All time
      15m: Last 15 min
      1h: Last hour
      6h: Last 6 hours
      24h: Last 24 hours
      7d: Last 7 days
      30d: Last 30 days
  actions:
    loadMore: Load more
    copyAll: Copy all
    copyEntry: Copy entry
    clearAll: Clear all logs
</i18n>
