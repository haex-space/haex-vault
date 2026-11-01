<script setup lang="ts">
/**
 * "Einstellungen" of the sync rules (spec 061 US4, FR-044): how many runs the log of each rule
 * keeps on this device, 0 to 100, 10 by default; 0 keeps no log. The dialog says what the log is,
 * what the number does and what it costs, so the field is not a bare number. The value is saved
 * when the field loses focus or on Enter; a smaller one drops the older runs of every rule at
 * once. A value outside the bounds is refused and the stored one comes back.
 */
import { parseLogDepth } from '~/lib/files/syncRules'
import { toast } from 'vue-sonner'

const open = defineModel<boolean>('open', { required: true })

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
    failure.value = t('files.syncRules.settings.invalid')
    return
  }
  failure.value = null
  if (depth === current) return
  busy.value = true
  try {
    await logDepthSetAsync(depth)
  } catch (error) {
    const message = errString(error)
    failure.value = message
    toast.error(message)
  } finally {
    busy.value = false
  }
  await loadAsync()
}

watch(open, (isOpen) => {
  if (!isOpen) return
  failure.value = null
  stored.value = null
  void loadAsync()
})
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('files.syncRules.settings.title')"
  >
    <template #content>
      <div class="flex flex-col gap-3" data-testid="files-sync-rules-settings">
        <h3 class="text-sm font-semibold">
          {{ t('files.syncRules.settings.logHeading') }}
        </h3>
        <p class="text-sm text-muted-foreground">
          {{ t('files.syncRules.settings.logIntro') }}
        </p>
        <UiInput
          id="files-sync-rules-log-depth"
          v-model="draft"
          type="number"
          inputmode="numeric"
          :label="t('files.syncRules.settings.depth')"
          label-bg="var(--background)"
          :labels="fieldLabels.input.value"
          :disabled="busy || stored === null"
          data-testid="files-sync-rules-log-depth"
          @blur="commitAsync"
          @keydown.enter.prevent="commitAsync"
        />
        <p class="text-xs text-muted-foreground">
          {{ t('files.syncRules.settings.hint') }}
        </p>
        <p class="text-xs text-muted-foreground">
          {{ t('files.syncRules.settings.cost') }}
        </p>
        <p class="text-xs text-muted-foreground">
          {{ t('files.syncRules.settings.scope') }}
        </p>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </div>
    </template>
  </UiDrawerModal>
</template>
