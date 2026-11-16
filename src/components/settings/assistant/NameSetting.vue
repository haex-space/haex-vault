<script setup lang="ts">
/**
 * "Name" on the Sprachassistent page (design 2026-11-16): what the assistant is called in this
 * vault, on every device. The written form is what is shown and what the model writes; the
 * spoken form is how it is said (empty: as written). The wake word does not follow the name yet;
 * the page says so. Saved with the button; errors as toasts.
 */
import { toast } from 'vue-sonner'
import { useAssistantName } from '~/composables/useAssistantName'
import { DEFAULT_ASSISTANT_NAME, isValidName } from '~/lib/assistant/name'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { name, setAsync, resetAsync } = useAssistantName()

const written = ref(name.value.written)
const spoken = ref(name.value.spoken)
const busy = ref(false)

// Follows the vault: a rename on another device, a reset.
watch(name, (now) => {
  written.value = now.written
  spoken.value = now.spoken
})

const valid = computed(
  () =>
    isValidName(written.value) &&
    (spoken.value.trim() === '' || isValidName(spoken.value)),
)
const changed = computed(
  () =>
    written.value.trim() !== name.value.written ||
    (spoken.value.trim() || written.value.trim()) !== name.value.spoken,
)
const isDefault = computed(
  () =>
    name.value.written === DEFAULT_ASSISTANT_NAME.written &&
    name.value.spoken === DEFAULT_ASSISTANT_NAME.spoken,
)
const invalid = computed(() =>
  !valid.value && (written.value !== '' || spoken.value !== '')
    ? t('settings.assistantPage.name.invalid')
    : undefined,
)

async function saveAsync() {
  if (!valid.value || !changed.value || busy.value) return
  busy.value = true
  try {
    // action-exempt: the name of the assistant is the user's own choice on the assistant page; spec 066 gives agents no action for it.
    await setAsync(written.value, spoken.value)
  } catch (e) {
    toast.error(
      t('settings.assistantPage.name.saveFailed', { reason: errString(e) }),
    )
  } finally {
    busy.value = false
  }
}

async function resetNowAsync() {
  if (busy.value) return
  busy.value = true
  try {
    // action-exempt: back to the default name is the user's own choice on the assistant page.
    await resetAsync()
  } catch (e) {
    toast.error(
      t('settings.assistantPage.name.saveFailed', { reason: errString(e) }),
    )
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <SettingsGroup
    :label="t('settings.assistantPage.name.title')"
    data-testid="settings-assistant-name"
  >
    <SettingsRow
      :title="t('settings.assistantPage.name.written')"
      :description="t('settings.assistantPage.name.writtenHint')"
      label-for="settings-assistant-name-written"
    >
      <div class="w-56 max-w-full">
        <UiInput
          id="settings-assistant-name-written"
          v-model="written"
          type="text"
          :error="invalid"
          :labels="fieldLabels.input.value"
          :disabled="busy"
          data-testid="settings-assistant-name-written"
          @keydown.enter.prevent="saveAsync"
        />
      </div>
    </SettingsRow>
    <SettingsRow
      :title="t('settings.assistantPage.name.spoken')"
      :description="t('settings.assistantPage.name.spokenHint')"
      label-for="settings-assistant-name-spoken"
    >
      <div class="w-56 max-w-full">
        <UiInput
          id="settings-assistant-name-spoken"
          v-model="spoken"
          type="text"
          :labels="fieldLabels.input.value"
          :disabled="busy"
          data-testid="settings-assistant-name-spoken"
          @keydown.enter.prevent="saveAsync"
        />
      </div>
    </SettingsRow>
    <SettingsRow
      :title="t('settings.assistantPage.name.wakeTitle')"
      :description="t('settings.assistantPage.name.wakeHint')"
    >
      <span class="flex flex-wrap items-center justify-end gap-2">
        <UiButton
          v-if="!isDefault"
          type="button"
          variant="outline"
          size="sm"
          :disabled="busy"
          data-testid="settings-assistant-name-reset"
          @click="resetNowAsync"
        >
          {{ t('settings.assistantPage.name.reset') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :disabled="!valid || !changed"
          :loading="busy"
          data-testid="settings-assistant-name-save"
          @click="saveAsync"
        >
          {{ t('settings.assistantPage.name.save') }}
        </UiButton>
      </span>
    </SettingsRow>
  </SettingsGroup>
</template>
