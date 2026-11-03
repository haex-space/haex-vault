<script setup lang="ts">
/**
 * The log of one extension on this device (spec 017, US5, T087): its newest entries with level,
 * time, message and metadata, filtered by level on selection, older ones on request.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { LogEntry } from '@bindings/LogEntry'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'

const props = defineProps<{ extensionId: string }>()
const { t } = useI18n()
const { errString } = useErrorString()

const PAGE = 50

const entries = ref<LogEntry[]>([])
const level = ref('all')
const more = ref(false)

const levelOptions = computed<SettingsSelectOption[]>(() =>
  ['all', 'error', 'warn', 'info', 'debug'].map((value) => ({
    value,
    label: t(`settings.extensions.logs.levels.${value}`),
  })),
)

async function readAsync(before: number | null): Promise<LogEntry[]> {
  return invoke<LogEntry[]>('extension_logs_read', {
    extensionId: props.extensionId,
    level: level.value === 'all' ? null : level.value,
    limit: PAGE,
    before,
  })
}

/** Counts reads; an answer to an earlier one (another level meanwhile) is dropped. */
let latest = 0

async function loadAsync() {
  const read = ++latest
  try {
    const first = await readAsync(null)
    if (read !== latest) return
    entries.value = first
    more.value = first.length === PAGE
  } catch (error) {
    if (read === latest) toast.error(errString(error))
  }
}

async function loadOlderAsync() {
  const oldest = entries.value.at(-1)
  if (!oldest) return
  const read = ++latest
  try {
    const older = await readAsync(oldest.id)
    if (read !== latest) return
    entries.value = [...entries.value, ...older]
    more.value = older.length === PAGE
  } catch (error) {
    if (read === latest) toast.error(errString(error))
  }
}

watch(level, loadAsync)
onMounted(loadAsync)
</script>

<template>
  <SettingsGroup :label="t('settings.extensions.logs.title')">
    <SettingsRow :title="t('settings.extensions.logs.level')">
      <SettingsSelect
        v-model="level"
        :options="levelOptions"
        class="w-40"
        data-testid="extension-logs-level"
      />
    </SettingsRow>
    <li
      v-if="entries.length === 0"
      class="px-4 py-3 text-sm text-muted-foreground"
    >
      {{ t('settings.extensions.logs.none') }}
    </li>
    <li
      v-for="entry in entries"
      :key="entry.id"
      class="flex flex-col gap-1 px-4 py-3"
      data-testid="extension-log-entry"
    >
      <span class="flex items-center gap-2 text-xs text-muted-foreground">
        <span
          class="font-medium uppercase"
          :class="{
            'text-destructive': entry.level === 'error',
            'text-warning': entry.level === 'warn',
          }"
          >{{ entry.level }}</span
        >
        <span>{{ new Date(entry.createdAt).toLocaleString() }}</span>
      </span>
      <span class="text-sm break-words whitespace-pre-wrap">{{
        entry.message
      }}</span>
      <span
        v-if="entry.metadata"
        class="font-mono text-xs break-all text-muted-foreground"
        >{{ entry.metadata }}</span
      >
    </li>
    <li v-if="more" class="px-4 py-3">
      <UiButton size="sm" variant="outline" @click="loadOlderAsync">
        {{ t('settings.extensions.logs.older') }}
      </UiButton>
    </li>
  </SettingsGroup>
</template>
