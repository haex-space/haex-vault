<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    sticky-header
  >
    <UTabs
      v-model="activeTab"
      :items="tabItems"
    >
      <template #content="{ item }">
        <!-- Logs Tab -->
        <div
          v-if="item.value === 'logs'"
          class="pt-4 space-y-4"
        >
          <!-- Filters -->
          <div
            class="flex flex-wrap gap-3 items-center"
          >
            <UInput
              v-model="filterSearch"
              :placeholder="t('filter.search')"
              icon="i-lucide-search"
              class="min-w-40 flex-1"
            />
            <USelect
              v-model="filterLevel"
              :items="levelOptions"
              :placeholder="t('filter.level')"
              class="min-w-24 flex-1"
            />
            <USelect
              v-model="filterSource"
              :items="sourceOptions"
              :placeholder="t('filter.source')"
              class="min-w-32 flex-1"
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
              class="min-w-28 flex-1"
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
          <div class="flex items-center justify-end gap-2 px-3">
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

          <!-- Log entries -->
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
            class="space-y-1.5 font-mono text-xs"
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
              <pre class="whitespace-pre-wrap wrap-break-word text-default">{{
                log.message
              }}</pre>
              <pre
                v-if="log.metadata"
                class="mt-1 text-muted whitespace-pre-wrap wrap-break-word"
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

        <!-- Settings Tab -->
        <div
          v-if="item.value === 'settings'"
          class="pt-4 space-y-6"
        >
          <!-- System Log Retention -->
          <UCard>
            <template #header>
              <h4 class="font-semibold">{{ t('settings.system.title') }}</h4>
              <p class="text-sm text-muted mt-1">
                {{ t('settings.system.description') }}
              </p>
            </template>
            <div class="flex items-center gap-3">
              <span class="text-sm shrink-0">{{
                t('settings.retention')
              }}</span>
              <USelect
                v-model="retentionDays"
                :items="retentionOptions"
                class="w-24"
                @update:model-value="saveRetentionAsync"
              />
              <span class="text-sm text-muted">{{ t('settings.days') }}</span>
            </div>
          </UCard>

          <!-- Extension Log Retention -->
          <UCard v-if="extensionStore.availableExtensions.length > 0">
            <template #header>
              <h4 class="font-semibold">
                {{ t('settings.extensions.title') }}
              </h4>
              <p class="text-sm text-muted mt-1">
                {{ t('settings.extensions.description') }}
              </p>
            </template>
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
                    t('settings.days')
                  }}</span>
                </div>
              </div>
            </div>
          </UCard>
        </div>
      </template>
    </UTabs>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq, and } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexVaultSettings } from '~/database/schemas'
import {
  VaultSettingsKeyEnum,
  VaultSettingsTypeEnum,
} from '~/config/vault-settings'

const { t } = useI18n()
const { add } = useToast()
const { copy } = useClipboard()
const extensionStore = useExtensionsStore()
const deviceStore = useDeviceStore()
const { currentVault } = storeToRefs(useVaultStore())

const activeTab = ref('logs')
const tabItems = computed(() => [
  { label: t('tabs.logs'), value: 'logs', icon: 'i-lucide-scroll-text' },
  { label: t('tabs.settings'), value: 'settings', icon: 'i-lucide-settings' },
])

// =========================================================================
// Log Viewer
// =========================================================================

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

// =========================================================================
// Retention Settings
// =========================================================================

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
      where: and(
        eq(haexVaultSettings.key, VaultSettingsKeyEnum.logRetentionDays),
        eq(haexVaultSettings.type, VaultSettingsTypeEnum.settings),
      ),
    })
  if (row?.value) retentionDays.value = row.value

  // Extension-specific retention
  const extRows = await currentVault.value.drizzle
    .select()
    .from(haexVaultSettings)
    .where(and(eq(haexVaultSettings.type, VaultSettingsTypeEnum.settings)))

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
        where: and(
          eq(haexVaultSettings.key, VaultSettingsKeyEnum.logRetentionDays),
          eq(haexVaultSettings.type, VaultSettingsTypeEnum.settings),
        ),
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
        type: VaultSettingsTypeEnum.settings,
        value,
      })
    }
    add({ title: t('settings.saved'), color: 'success' })
  } catch (error) {
    console.error('Failed to save retention:', error)
    add({ title: t('settings.saveFailed'), color: 'error' })
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
        where: and(
          eq(haexVaultSettings.key, key),
          eq(haexVaultSettings.type, VaultSettingsTypeEnum.settings),
        ),
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
        type: VaultSettingsTypeEnum.settings,
        value,
      })
    }
    extensionRetention.value[extensionId] = value
    add({ title: t('settings.saved'), color: 'success' })
  } catch (error) {
    console.error('Failed to save extension retention:', error)
    add({ title: t('settings.saveFailed'), color: 'error' })
  }
}

// Reload on filter change
watch([filterLevel, filterSource, filterDevice, filterTime], () => fetchLogs())

onMounted(async () => {
  await deviceStore.loadKnownDevicesAsync()
  await loadRetentionAsync()
  await fetchLogs()
})
</script>

<i18n lang="yaml">
de:
  title: Logs
  entries: Einträge
  empty: Keine Logs vorhanden
  tabs:
    logs: Logs
    settings: Einstellungen
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
  settings:
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
  actions:
    loadMore: Mehr laden
    copyAll: Alle kopieren
    copyEntry: Eintrag kopieren
    clearAll: Alle Logs löschen
en:
  title: Logs
  entries: entries
  empty: No logs found
  tabs:
    logs: Logs
    settings: Settings
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
  settings:
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
  actions:
    loadMore: Load more
    copyAll: Copy all
    copyEntry: Copy entry
    clearAll: Clear all logs
</i18n>
