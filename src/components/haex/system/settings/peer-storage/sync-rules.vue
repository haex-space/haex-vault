<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    show-back
    @back="$emit('back')"
  >
    <template #description>
      {{ t('description') }}
    </template>
    <template #actions>
      <UiButton
        icon="i-lucide-plus"
        color="primary"
        @click="editingRule = null; showCreateDialog = true"
      >
        {{ t('addRule') }}
      </UiButton>
    </template>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-if="!syncStore.syncRules.length"
      :message="t('empty')"
      icon="i-lucide-refresh-cw-off"
    />

    <!-- Rules cards -->
    <div v-else class="space-y-4">
      <UCard
        v-for="rule in syncStore.syncRules"
        :key="rule.id"
        :class="{ 'opacity-50': !rule.enabled }"
      >
        <UCollapsible v-model:open="expandedMap[rule.id]">
          <!-- Always-visible: badges + source/target -->
          <div>
            <!-- Header: badges + expand toggle -->
            <div
              class="flex items-center gap-2 mb-3 cursor-pointer"
              @click="expandedMap[rule.id] = !expandedMap[rule.id]"
            >
              <UBadge
                :color="syncStore.isRuleRunning(rule.id) ? 'success' : 'neutral'"
                variant="subtle"
                size="sm"
              >
                {{ syncStore.isRuleRunning(rule.id) ? t('status.running') : t('status.stopped') }}
              </UBadge>
              <UBadge variant="subtle" color="neutral" size="sm">
                {{ rule.direction === 'two_way' ? t('direction.twoWay') : t('direction.oneWay') }}
              </UBadge>
              <UBadge variant="subtle" color="neutral" size="sm">
                <UIcon name="i-lucide-clock" class="w-3 h-3" />
                {{ formatInterval(rule.syncIntervalSeconds) }}
              </UBadge>
              <UBadge variant="subtle" color="neutral" size="sm">
                <UIcon name="i-lucide-trash-2" class="w-3 h-3" />
                {{ formatDeleteMode(rule.deleteMode) }}
              </UBadge>
              <UIcon
                name="i-lucide-chevron-down"
                class="w-4 h-4 text-muted ml-auto shrink-0 transition-transform duration-200"
                :class="{ 'rotate-180': expandedMap[rule.id] }"
              />
            </div>

            <!-- Body: source → target -->
            <div class="flex items-center gap-3">
              <!-- Source -->
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-1">
                  <UIcon :name="providerIcon(rule.sourceType)" class="w-4 h-4 text-muted shrink-0" />
                  <span class="text-xs text-muted">{{ t('label.source') }}</span>
                </div>
                <p class="text-sm font-medium truncate">
                  {{ formatProviderLabel(rule.sourceType, rule.sourceConfig) }}
                </p>
                <p v-if="resolveDeviceName(rule.sourceType, rule.sourceConfig)" class="text-xs text-muted truncate">
                  {{ resolveDeviceName(rule.sourceType, rule.sourceConfig) }}
                </p>
              </div>

              <!-- Arrow -->
              <UIcon
                :name="rule.direction === 'two_way' ? 'i-lucide-arrow-left-right' : 'i-lucide-arrow-right'"
                class="w-5 h-5 text-primary shrink-0"
              />

              <!-- Target -->
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-1">
                  <UIcon :name="providerIcon(rule.targetType)" class="w-4 h-4 text-muted shrink-0" />
                  <span class="text-xs text-muted">{{ t('label.target') }}</span>
                </div>
                <p class="text-sm font-medium truncate">
                  {{ formatProviderLabel(rule.targetType, rule.targetConfig) }}
                </p>
                <p v-if="resolveDeviceName(rule.targetType, rule.targetConfig)" class="text-xs text-muted truncate">
                  {{ resolveDeviceName(rule.targetType, rule.targetConfig) }}
                </p>
              </div>
            </div>

            <!-- Footer: actions -->
            <div class="flex items-center justify-end gap-1 mt-3 pt-3 border-t border-default">
              <UiButton
                icon="i-lucide-refresh-cw"
                variant="ghost"
                color="neutral"
                :loading="isSyncing === rule.id"
                @click="onSyncNowAsync(rule.id)"
              />
              <UiButton
                icon="i-lucide-pencil"
                variant="ghost"
                color="neutral"
                @click="onEdit(rule)"
              />
              <USwitch
                :model-value="rule.enabled"
                @update:model-value="(val: boolean) => onToggleAsync(rule.id, val)"
              />
              <UiButton
                icon="i-lucide-trash-2"
                variant="ghost"
                color="error"
                @click="onDeleteAsync(rule.id)"
              />
            </div>
          </div>

          <!-- Collapsible: progress + last result -->
          <template #content>
            <div class="mt-3 pt-3 border-t border-default">
              <!-- Active sync progress -->
              <div v-if="syncStore.getRuleProgress(rule.id)" class="space-y-2">
                <!-- Stats row: active + done counts + speed -->
                <div class="flex items-center justify-between text-xs">
                  <span class="text-muted">
                    <span
                      v-if="syncStore.getRuleProgress(rule.id)!.activeFiles?.length"
                      class="text-primary font-medium"
                    >
                      {{ syncStore.getRuleProgress(rule.id)!.activeFiles.length }} {{ t('progress.active') }}
                    </span>
                    <span v-if="syncStore.getRuleProgress(rule.id)!.activeFiles?.length && syncStore.getRuleProgress(rule.id)!.filesDone > 0"> · </span>
                    <span v-if="syncStore.getRuleProgress(rule.id)!.filesDone > 0">
                      {{ syncStore.getRuleProgress(rule.id)!.filesDone }}/{{ syncStore.getRuleProgress(rule.id)!.filesTotal }} {{ t('progress.done') }}
                    </span>
                  </span>
                  <span v-if="syncStore.getRuleProgress(rule.id)!.bytesPerSecond > 0" class="text-primary font-medium shrink-0 ml-2 tabular-nums">
                    {{ formatSpeed(syncStore.getRuleProgress(rule.id)!.bytesPerSecond) }}
                  </span>
                </div>
                <!-- Bytes-based progress bar (fills as data is received, not just on file completion) -->
                <UProgress
                  :value="syncStore.getRuleProgress(rule.id)!.bytesTotal > 0
                    ? syncStore.getRuleProgress(rule.id)!.bytesDone
                    : syncStore.getRuleProgress(rule.id)!.filesDone"
                  :max="syncStore.getRuleProgress(rule.id)!.bytesTotal > 0
                    ? syncStore.getRuleProgress(rule.id)!.bytesTotal
                    : Math.max(syncStore.getRuleProgress(rule.id)!.filesTotal, 1)"
                  color="primary"
                  size="sm"
                />
                <!-- Bytes transferred -->
                <div v-if="syncStore.getRuleProgress(rule.id)!.bytesTotal > 0" class="text-xs text-muted tabular-nums">
                  {{ formatBytes(syncStore.getRuleProgress(rule.id)!.bytesDone) }} / {{ formatBytes(syncStore.getRuleProgress(rule.id)!.bytesTotal) }}
                </div>
                <!-- Active files list with per-file progress -->
                <div
                  v-if="syncStore.getRuleProgress(rule.id)!.activeFiles?.length"
                  class="mt-1 space-y-1.5"
                >
                  <div
                    v-for="fp in syncStore.getRuleProgress(rule.id)!.activeFiles.slice(0, 4)"
                    :key="fp.path"
                    class="space-y-0.5"
                  >
                    <div class="flex items-center gap-1.5 text-xs text-muted">
                      <UIcon name="i-lucide-arrow-down" class="w-3 h-3 text-primary shrink-0" />
                      <span class="truncate flex-1">{{ fp.path.split(/[/\\]/).pop() }}</span>
                      <span class="shrink-0 tabular-nums">
                        {{ fp.bytesTotal > 0 ? formatBytes(fp.bytesDone) + ' / ' + formatBytes(fp.bytesTotal) : '' }}
                      </span>
                    </div>
                    <UProgress
                      v-if="fp.bytesTotal > 0"
                      :value="fp.bytesDone"
                      :max="fp.bytesTotal"
                      color="primary"
                      size="xs"
                    />
                  </div>
                  <div
                    v-if="(syncStore.getRuleProgress(rule.id)!.activeFiles?.length ?? 0) > 4"
                    class="text-xs text-muted"
                  >
                    +{{ syncStore.getRuleProgress(rule.id)!.activeFiles!.length - 4 }} {{ t('progress.moreFiles') }}
                  </div>
                </div>
              </div>

              <!-- Last sync result -->
              <div v-else-if="syncStore.getLastResult(rule.id)">
                <div class="text-xs text-muted mb-2">{{ t('lastSync.title') }}</div>
                <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                  <span v-if="syncStore.getLastResult(rule.id)!.filesDownloaded > 0" class="flex items-center gap-1">
                    <UIcon name="i-lucide-download" class="w-3 h-3 text-primary" />
                    {{ syncStore.getLastResult(rule.id)!.filesDownloaded }} {{ t('lastSync.downloaded') }}
                  </span>
                  <span v-if="syncStore.getLastResult(rule.id)!.filesDeleted > 0" class="flex items-center gap-1">
                    <UIcon name="i-lucide-trash-2" class="w-3 h-3 text-muted" />
                    {{ syncStore.getLastResult(rule.id)!.filesDeleted }} {{ t('lastSync.deleted') }}
                  </span>
                  <span v-if="syncStore.getLastResult(rule.id)!.bytesTransferred > 0" class="flex items-center gap-1">
                    <UIcon name="i-lucide-hard-drive" class="w-3 h-3 text-muted" />
                    {{ formatBytes(syncStore.getLastResult(rule.id)!.bytesTransferred) }}
                  </span>
                  <span
                    v-if="syncStore.getLastResult(rule.id)!.filesDownloaded === 0 && syncStore.getLastResult(rule.id)!.filesDeleted === 0 && syncStore.getLastResult(rule.id)!.bytesTransferred === 0 && syncStore.getLastResult(rule.id)!.directoriesCreated === 0 && syncStore.getLastResult(rule.id)!.conflictsResolved === 0"
                    class="text-muted"
                  >
                    {{ t('lastSync.upToDate') }}
                  </span>
                </div>
                <div v-if="syncStore.getLastResult(rule.id)!.errors.length > 0" class="mt-2 space-y-1">
                  <p v-for="err in syncStore.getLastResult(rule.id)!.errors.slice(0, 3)" :key="err" class="text-xs text-error truncate">
                    {{ err }}
                  </p>
                  <p v-if="syncStore.getLastResult(rule.id)!.errors.length > 3" class="text-xs text-muted">
                    +{{ syncStore.getLastResult(rule.id)!.errors.length - 3 }} {{ t('lastSync.moreErrors') }}
                  </p>
                </div>
              </div>

              <!-- No data yet -->
              <div v-else class="text-xs text-muted">
                {{ t('progress.noData') }}
              </div>
            </div>
          </template>
        </UCollapsible>
      </UCard>
    </div>

    <HaexSystemSettingsPeerStorageCreateSyncRuleDialog
      v-model:open="showCreateDialog"
      :edit-rule="editingRule"
      @created="onRuleCreated"
      @updated="onRuleCreated"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { SelectHaexSyncRules } from '~/database/schemas'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const syncStore = useFileSyncStore()
const peerStorageStore = usePeerStorageStore()

const showCreateDialog = ref(false)
const editingRule = ref<SelectHaexSyncRules | null>(null)
const isSyncing = ref<string | null>(null)
const expandedMap = reactive<Record<string, boolean>>({})

onMounted(async () => {
  await syncStore.loadRulesAsync()
  await syncStore.refreshStatusAsync()
  await peerStorageStore.loadSpaceDevicesAsync()
})

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

const formatSpeed = (bytesPerSecond: number): string => {
  if (bytesPerSecond === 0) return t('progress.calculating')
  return `${formatBytes(bytesPerSecond)}/s`
}

const providerIcon = (type: string): string => {
  switch (type) {
    case 'local': return 'i-lucide-folder'
    case 'peer': return 'i-lucide-monitor-smartphone'
    case 'cloud': return 'i-lucide-cloud'
    default: return 'i-lucide-file'
  }
}

const deviceStore = useDeviceStore()

const resolveDeviceName = (type: string, config: unknown): string | null => {
  if (type === 'local') {
    return deviceStore.deviceName || deviceStore.hostname || null
  }
  if (type === 'peer') {
    const cfg = config as Record<string, unknown>
    const endpointId = cfg?.endpointId as string
    if (!endpointId) return null
    const device = peerStorageStore.spaceDevices.find(d => d.deviceEndpointId === endpointId)
    return device?.deviceName || endpointId.slice(0, 16) + '...'
  }
  return null
}

const formatProviderLabel = (type: string, config: unknown): string => {
  const cfg = config as Record<string, unknown>
  switch (type) {
    case 'local': {
      const path = (cfg?.path as string) || ''
      return path.split(/[/\\]/).pop() || path
    }
    case 'peer': {
      const path = (cfg?.path as string) || ''
      const id = (cfg?.endpointId as string) || ''
      return path || id.slice(0, 12) + '...'
    }
    case 'cloud': {
      const prefix = (cfg?.prefix as string) || '/'
      return `S3:${prefix}`
    }
    default:
      return type
  }
}

const formatInterval = (seconds: number): string => {
  if (seconds === 0) return t('intervals.manual')
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${seconds / 60} min`
  return `${seconds / 3600}h`
}

const formatDeleteMode = (mode: string): string => {
  switch (mode) {
    case 'trash': return t('deleteModes.trash')
    case 'permanent': return t('deleteModes.permanent')
    case 'ignore': return t('deleteModes.ignore')
    default: return mode
  }
}

const onSyncNowAsync = async (ruleId: string) => {
  isSyncing.value = ruleId
  try {
    const result = await syncStore.triggerSyncNowAsync(ruleId)
    if (result) {
      add({
        title: t('toast.syncComplete'),
        description: `${result.filesDownloaded} ${t('toast.filesDownloaded')}`,
        color: 'success',
      })
    }
  } catch (error) {
    add({
      title: t('toast.syncFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isSyncing.value = null
  }
}

const onToggleAsync = async (ruleId: string, enabled: boolean) => {
  try {
    await syncStore.toggleRuleAsync(ruleId, enabled)
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const onEdit = (rule: SelectHaexSyncRules) => {
  editingRule.value = rule
  showCreateDialog.value = true
}

const onRuleCreated = async () => {
  await syncStore.loadRulesAsync()
  await syncStore.refreshStatusAsync()
}

const onDeleteAsync = async (ruleId: string) => {
  try {
    await syncStore.deleteRuleAsync(ruleId)
    add({ title: t('toast.deleted'), color: 'neutral' })
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Sync-Regeln
  description: Dateien automatisch zwischen Geräten und Cloud-Speicher synchronisieren
  addRule: Neue Regel
  empty: Noch keine Sync-Regeln erstellt
  error: Fehler
  label:
    source: Quelle
    target: Ziel
  direction:
    oneWay: Einseitig
    twoWay: Beidseitig
  status:
    running: Aktiv
    stopped: Inaktiv
  intervals:
    manual: Nur manuell
  deleteModes:
    trash: Papierkorb
    permanent: Endgültig
    ignore: Ignorieren
  progress:
    preparing: Wird vorbereitet...
    files: Dateien
    active: aktiv
    done: fertig
    noData: Noch kein Sync durchgeführt
    moreFiles: weitere
    calculating: Berechne...
  lastSync:
    title: Letzter Sync
    downloaded: heruntergeladen
    deleted: gelöscht
    upToDate: Alles aktuell
    moreErrors: weitere Fehler
  toast:
    syncComplete: Sync abgeschlossen
    filesDownloaded: Dateien synchronisiert
    syncFailed: Sync fehlgeschlagen
    deleted: Regel gelöscht
en:
  title: Sync Rules
  description: Automatically synchronize files between devices and cloud storage
  addRule: New Rule
  empty: No sync rules created yet
  error: Error
  label:
    source: Source
    target: Target
  direction:
    oneWay: One-way
    twoWay: Two-way
  status:
    running: Active
    stopped: Inactive
  intervals:
    manual: Manual only
  deleteModes:
    trash: Trash
    permanent: Permanent
    ignore: Ignore
  progress:
    preparing: Preparing...
    files: files
    active: active
    done: done
    noData: No sync has run yet
    moreFiles: more
    calculating: Calculating...
  lastSync:
    title: Last sync
    downloaded: downloaded
    deleted: deleted
    upToDate: Everything up to date
    moreErrors: more errors
  toast:
    syncComplete: Sync complete
    filesDownloaded: files synced
    syncFailed: Sync failed
    deleted: Rule deleted
</i18n>
