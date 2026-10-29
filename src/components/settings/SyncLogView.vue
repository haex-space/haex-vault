<script setup lang="ts">
/**
 * "Allgemein → Sync-Protokoll" (spec 061 US4, FR-044): how many runs the log of each sync rule
 * keeps on this device, 0 to 100, 10 by default; 0 keeps no log. The value is saved when the field
 * loses focus or on Enter; a smaller one drops the older runs of every rule at once. A value
 * outside the bounds is refused and the stored one comes back.
 */
import { parseLogDepth } from '~/lib/files/syncRules'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { logDepthGetAsync, logDepthSetAsync } = useSyncRules()

const stored = ref<number | null>(null)
const draft = ref('')
const failure = ref<string | null>(null)
const busy = ref(false)

async function loadAsync() {
  try {
    stored.value = await logDepthGetAsync()
  } catch (error) {
    failure.value = errString(error)
  }
  draft.value = stored.value === null ? '' : String(stored.value)
}

async function commitAsync() {
  const current = stored.value
  if (current === null || busy.value) return
  const depth = parseLogDepth(String(draft.value))
  if (depth === null) {
    draft.value = String(current)
    failure.value = t('settings.syncLog.invalid')
    return
  }
  failure.value = null
  if (depth === current) return
  busy.value = true
  try {
    await logDepthSetAsync(depth)
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
  await loadAsync()
}

onMounted(loadAsync)
</script>

<template>
  <SettingsGroup :label="t('settings.syncLog.title')">
    <SettingsRow
      :title="t('settings.syncLog.depth')"
      :description="t('settings.syncLog.hint')"
      label-for="settings-sync-log-depth"
    >
      <div class="w-40 max-w-full">
        <UiInput
          id="settings-sync-log-depth"
          v-model="draft"
          type="number"
          inputmode="numeric"
          :labels="fieldLabels.input.value"
          :disabled="busy || stored === null"
          data-testid="settings-sync-log-depth"
          @blur="commitAsync"
          @keydown.enter.prevent="commitAsync"
        />
      </div>
    </SettingsRow>
    <li v-if="failure" class="px-4 py-3 text-sm text-destructive" role="alert">
      {{ failure }}
    </li>
  </SettingsGroup>
</template>
