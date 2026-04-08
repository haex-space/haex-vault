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
        @click="showCreateDialog = true"
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

    <!-- Rules list -->
    <UiListContainer v-else>
      <div
        v-for="rule in syncStore.syncRules"
        :key="rule.id"
        class="flex items-center justify-between gap-3 py-3"
      >
        <div class="flex items-center gap-3 min-w-0 flex-1">
          <!-- Direction icon -->
          <UIcon
            :name="rule.direction === 'two_way' ? 'i-lucide-arrow-left-right' : 'i-lucide-arrow-right'"
            class="w-5 h-5 shrink-0"
            :class="rule.enabled ? 'text-primary' : 'text-muted'"
          />

          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium truncate">{{ rule.name }}</p>
            <p class="text-xs text-muted truncate">
              {{ formatProviderLabel(rule.sourceType, rule.sourceConfig) }}
              →
              {{ formatProviderLabel(rule.targetType, rule.targetConfig) }}
            </p>
          </div>
        </div>

        <div class="flex items-center gap-2 shrink-0">
          <!-- Sync status indicator -->
          <UBadge
            v-if="syncStore.isRuleRunning(rule.id)"
            variant="subtle"
            color="success"
            size="sm"
          >
            {{ t('status.running') }}
          </UBadge>

          <!-- Sync now button -->
          <UiButton
            icon="i-lucide-refresh-cw"
            variant="ghost"
            color="neutral"
            :loading="isSyncing === rule.id"
            @click="onSyncNowAsync(rule.id)"
          />

          <!-- Enable/disable toggle -->
          <USwitch
            :model-value="rule.enabled"
            @update:model-value="(val: boolean) => onToggleAsync(rule.id, val)"
          />

          <!-- Delete button -->
          <UiButton
            icon="i-lucide-trash-2"
            variant="ghost"
            color="error"
            @click="onDeleteAsync(rule.id)"
          />
        </div>
      </div>
    </UiListContainer>

    <!-- Create dialog will be a separate component -->
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const syncStore = useFileSyncStore()

const showCreateDialog = ref(false)
const isSyncing = ref<string | null>(null)

onMounted(async () => {
  await syncStore.loadRulesAsync()
  await syncStore.refreshStatusAsync()
  await syncStore.setupEventListeners()
})

onUnmounted(() => {
  syncStore.cleanupEventListeners()
})

const formatProviderLabel = (type: string, config: unknown): string => {
  const cfg = config as Record<string, unknown>
  switch (type) {
    case 'local': {
      const path = (cfg?.path as string) || ''
      return path.split(/[/\\]/).pop() || path
    }
    case 'peer': {
      const id = (cfg?.endpointId as string) || ''
      return id.slice(0, 12) + '...'
    }
    case 'cloud': {
      const prefix = (cfg?.prefix as string) || '/'
      return `S3:${prefix}`
    }
    default:
      return type
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
  status:
    running: Aktiv
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
  status:
    running: Active
  toast:
    syncComplete: Sync complete
    filesDownloaded: files synced
    syncFailed: Sync failed
    deleted: Rule deleted
</i18n>
