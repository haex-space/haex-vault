<template>
  <div>
    <!-- Last sync result -->
    <div v-if="lastResult">
      <div class="text-xs text-muted mb-2">{{ t('lastSync.title') }}</div>
      <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
        <span v-if="lastResult.filesDownloaded > 0" class="flex items-center gap-1">
          <UIcon name="i-lucide-download" class="w-3 h-3 text-primary" />
          {{ lastResult.filesDownloaded }} {{ t('lastSync.downloaded') }}
        </span>
        <span v-if="lastResult.filesDeleted > 0" class="flex items-center gap-1">
          <UIcon name="i-lucide-trash-2" class="w-3 h-3 text-muted" />
          {{ lastResult.filesDeleted }} {{ t('lastSync.deleted') }}
        </span>
        <span v-if="lastResult.bytesTransferred > 0" class="flex items-center gap-1">
          <UIcon name="i-lucide-hard-drive" class="w-3 h-3 text-muted" />
          {{ state.formatBytes(lastResult.bytesTransferred) }}
        </span>
        <span
          v-if="lastResult.filesDownloaded === 0 && lastResult.filesDeleted === 0 && lastResult.bytesTransferred === 0 && lastResult.directoriesCreated === 0 && lastResult.conflictsResolved === 0"
          class="text-muted"
        >
          {{ t('lastSync.upToDate') }}
        </span>
      </div>
      <div v-if="lastResult.errors.length > 0" class="mt-2 space-y-1">
        <p v-for="err in lastResult.errors.slice(0, 3)" :key="err" class="text-xs text-error truncate">
          {{ err }}
        </p>
        <p v-if="lastResult.errors.length > 3" class="text-xs text-muted">
          +{{ lastResult.errors.length - 3 }} {{ t('lastSync.moreErrors') }}
        </p>
      </div>
    </div>

    <!-- Activity log / error history -->
    <div class="mt-3 pt-3 border-t border-default">
      <div class="flex items-center justify-between mb-2 gap-2 flex-wrap">
        <span class="text-xs text-muted font-medium">{{ t('log.title') }}</span>
        <div class="flex items-center gap-2">
          <USwitch
            :model-value="showAllDevices"
            size="xs"
            :label="t('log.allDevices')"
            @update:model-value="(val: boolean) => emit('toggleAllDevices', val)"
          />
          <UiButton
            v-if="log.length"
            icon="i-lucide-eraser"
            variant="ghost"
            color="neutral"
            size="xs"
            @click="emit('clearLog')"
          >
            {{ t('log.clear') }}
          </UiButton>
        </div>
      </div>
      <div
        v-if="log.length"
        class="space-y-1 max-h-60 overflow-y-auto"
      >
        <div
          v-for="(entry, idx) in log"
          :key="`${rule.id}-${idx}-${entry.at}`"
          class="flex items-start gap-2 text-xs"
        >
          <UIcon
            :name="entry.level === 'error' ? 'i-lucide-circle-x' : 'i-lucide-check'"
            :class="entry.level === 'error' ? 'text-error' : 'text-success'"
            class="w-3 h-3 mt-0.5 shrink-0"
          />
          <div class="flex-1 min-w-0">
            <div class="flex items-baseline gap-2 flex-wrap">
              <span
                class="break-words"
                :class="entry.level === 'error' ? 'text-error' : ''"
              >
                {{ entry.summary }}
              </span>
              <UBadge
                v-if="state.otherDeviceName(entry.deviceId)"
                color="neutral"
                variant="subtle"
                size="xs"
                :title="entry.deviceId ?? ''"
              >
                <UIcon name="i-lucide-monitor" class="w-3 h-3" />
                {{ state.otherDeviceName(entry.deviceId) }}
              </UBadge>
              <span
                v-if="entry.repeats && entry.repeats > 1"
                class="text-muted shrink-0"
                :title="t('log.repeats')"
              >
                ×{{ entry.repeats }}
              </span>
            </div>
            <span class="text-muted text-[10px]">
              {{ state.formatRelative(entry.at) }}
            </span>
          </div>
        </div>
      </div>
      <p v-else class="text-xs text-muted italic">
        {{ t('log.empty') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexSyncRules } from '~/database/schemas'
import type { SyncLogEntry } from '@/stores/file-sync'
import { useSyncRulesStateInject } from '@/composables/useSyncRulesState'

interface LastResultShape {
  filesDownloaded: number
  filesDeleted: number
  directoriesCreated: number
  bytesTransferred: number
  conflictsResolved: number
  errors: string[]
}

defineProps<{
  rule: SelectHaexSyncRules
  lastResult: LastResultShape | null | undefined
  log: SyncLogEntry[]
  showAllDevices: boolean
}>()

const emit = defineEmits<{
  toggleAllDevices: [val: boolean]
  clearLog: []
}>()

const { t } = useI18n()
const state = useSyncRulesStateInject()
</script>
