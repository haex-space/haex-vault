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
        <!-- Header: badges -->
        <template #header>
          <div class="flex items-center gap-2">
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
          </div>
        </template>

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
        <template #footer>
          <div class="flex items-center justify-end gap-1">
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
        </template>
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

onMounted(async () => {
  await syncStore.loadRulesAsync()
  await syncStore.refreshStatusAsync()
  await syncStore.setupEventListeners()
  await peerStorageStore.loadSpaceDevicesAsync()
})

onUnmounted(() => {
  syncStore.cleanupEventListeners()
})

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
    add({
      title: t('toast.syncComplete'),
      description: `${result.filesDownloaded} ${t('toast.filesDownloaded')}`,
      color: 'success',
    })
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
  toast:
    syncComplete: Sync complete
    filesDownloaded: files synced
    syncFailed: Sync failed
    deleted: Rule deleted
</i18n>
