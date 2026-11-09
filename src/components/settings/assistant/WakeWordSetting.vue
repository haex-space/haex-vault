<script setup lang="ts">
/**
 * "Wake Word" on the Sprachassistent page (spec 066 FR-011, PR D): haex waits for "Hex" or
 * "Hey Hex" once its two small models (about 3 MB) are on this device; until then switching the
 * assistant on starts a conversation at once. The sensitivity (`assistant.sensitivity`, this
 * device) trades how often a softly said "Hex" is missed against how often haex wakes by mistake.
 * Saved on selection; errors as toasts.
 */
import type { UnlistenFn } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import {
  parseSensitivity,
  SENSITIVITIES,
  SENSITIVITY_KEY,
  WAKE_WORD_MODEL_ID,
  type Sensitivity,
} from '~/composables/useAssistantWakeWord'
import { humanBytes, progressPercent } from '~/lib/models/format'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { modelAsync, downloadModelAsync } = useAssistantWakeWord()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const installed = ref<boolean | null>(null)
const sizeBytes = ref(0)
const downloading = ref(false)
const downloaded = ref(0)
const sensitivity = ref<Sensitivity>('medium')
const saving = ref(false)

const percent = computed(
  () => progressPercent(downloaded.value, sizeBytes.value) ?? 0,
)
const options = computed(() =>
  SENSITIVITIES.map((value) => ({
    value,
    label: t(`settings.assistantPage.wakeWord.sensitivity.${value}`),
  })),
)

let unlisten: UnlistenFn | null = null
let disposed = false
void listen<{ modelId: string; bytesDownloaded: number }>(
  'model-download-progress',
  (event) => {
    if (event.payload.modelId === WAKE_WORD_MODEL_ID)
      downloaded.value = event.payload.bytesDownloaded
  },
).then((stop) => (disposed ? stop() : (unlisten = stop)))
onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
})

async function reloadAsync() {
  try {
    const [model, stored] = await Promise.all([
      modelAsync(),
      getPrefAsync(scope.value, SENSITIVITY_KEY),
    ])
    installed.value = model.installed
    sizeBytes.value = model.sizeBytes
    sensitivity.value = parseSensitivity(stored)
  } catch (e) {
    toast.error(
      t('settings.assistantPage.wakeWord.loadFailed', { reason: errString(e) }),
    )
  }
}

async function downloadAsync() {
  downloading.value = true
  downloaded.value = 0
  try {
    // action-exempt: fetching the wake word models is the user's own choice on the assistant page; spec 066 gives agents no action for it.
    await downloadModelAsync()
    installed.value = true
  } catch (e) {
    toast.error(
      t('settings.assistantPage.wakeWord.downloadFailed', {
        reason: errString(e),
      }),
    )
  } finally {
    downloading.value = false
  }
}

async function saveSensitivityAsync(value: string) {
  const previous = sensitivity.value
  sensitivity.value = parseSensitivity(value)
  saving.value = true
  try {
    // action-exempt: how readily the wake word fires on this device is the user's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, SENSITIVITY_KEY, sensitivity.value)
  } catch (e) {
    sensitivity.value = previous
    toast.error(
      t('settings.assistantPage.saveFailed', { reason: errString(e) }),
    )
  } finally {
    saving.value = false
  }
}

onMounted(() => reloadAsync())
onVaultTablesChanged(['preferences'], () => {
  if (!saving.value) return reloadAsync()
})
</script>

<template>
  <SettingsGroup
    :label="t('settings.assistantPage.wakeWord.title')"
    data-testid="settings-assistant-wake-word"
  >
    <SettingsRow
      :title="t('settings.assistantPage.wakeWord.word')"
      :description="
        installed
          ? t('settings.assistantPage.wakeWord.ready')
          : t('settings.assistantPage.wakeWord.missing', {
              size: humanBytes(sizeBytes),
            })
      "
    >
      <UiButton
        v-if="installed === false"
        type="button"
        variant="outline"
        size="sm"
        :loading="downloading"
        data-testid="settings-assistant-wake-word-download"
        @click="downloadAsync"
      >
        <Icon name="lucide:download" class="size-4" aria-hidden="true" />
        {{ t('settings.assistantPage.wakeWord.download') }}
      </UiButton>
      <template v-if="downloading" #below>
        <UiProgress
          :value="percent"
          :label="t('settings.assistantPage.wakeWord.downloading')"
        />
      </template>
    </SettingsRow>
    <SettingsRow
      v-if="installed"
      :title="t('settings.assistantPage.wakeWord.sensitivityTitle')"
      :description="t('settings.assistantPage.wakeWord.sensitivityHint')"
      label-for="settings-assistant-sensitivity"
    >
      <SettingsSelect
        id="settings-assistant-sensitivity"
        :model-value="sensitivity"
        :options="options"
        :disabled="saving"
        data-testid="settings-assistant-sensitivity"
        @update:model-value="saveSensitivityAsync"
      />
    </SettingsRow>
  </SettingsGroup>
</template>
