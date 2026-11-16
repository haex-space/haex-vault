<script setup lang="ts">
/**
 * "Wake Word" on the Sprachassistent page (spec 066 FR-011): haex waits for the default "Haex" or
 * "Hey Haex" (spoken "Hex"); its models ship with the app and update by themselves. The
 * sensitivity (`assistant.sensitivity`, this device) trades how often a softly said "Haex" is
 * missed against how often haex wakes by mistake.
 * Saved on selection; errors as toasts.
 */
import { toast } from 'vue-sonner'
import {
  parseSensitivity,
  SENSITIVITIES,
  SENSITIVITY_KEY,
  type Sensitivity,
} from '~/composables/useAssistantWakeWord'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const sensitivity = ref<Sensitivity>('medium')
const saving = ref(false)

const options = computed(() =>
  SENSITIVITIES.map((value) => ({
    value,
    label: t(`settings.assistantPage.wakeWord.sensitivity.${value}`),
  })),
)

async function reloadAsync() {
  try {
    sensitivity.value = parseSensitivity(
      await getPrefAsync(scope.value, SENSITIVITY_KEY),
    )
  } catch (e) {
    toast.error(
      t('settings.assistantPage.wakeWord.loadFailed', { reason: errString(e) }),
    )
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
      :description="t('settings.assistantPage.wakeWord.ready')"
    />
    <SettingsRow
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
