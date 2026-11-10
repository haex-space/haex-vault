<script setup lang="ts">
/**
 * "Stimmen, auf die haex hört" on the Sprachassistent page (spec 066 FR-016a, operator
 * 2026-11-07): the enrolled people, synced in the vault, each removable; a person is added by
 * reading a few sentences (`SpeakerEnrolDialog`). While someone is enrolled, the device setting
 * `assistant.enrolledOnly` (default on) lets haex follow only them: anyone else's request, stop word
 * or barge-in changes nothing. Saved on selection; errors as toasts.
 */
import { toast } from 'vue-sonner'
import {
  ENROLLED_ONLY_KEY,
  type EnrolledSpeaker,
} from '~/composables/useAssistantSpeakers'
import type { SpeechLanguage } from '~/lib/assistant/settings'
import { useSettingsDevice } from '~/components/settings/deviceContext'

defineProps<{
  /** The language of conversations: the enrolment sentences are in it. */
  language: SpeechLanguage
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { listAsync, deleteAsync } = useAssistantSpeakers()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const speakers = ref<EnrolledSpeaker[]>([])
const enrolledOnly = ref(true)
const enrolling = ref(false)
const deleting = ref<string | null>(null)
const saving = ref(false)

async function reloadAsync() {
  try {
    const [list, only] = await Promise.all([
      listAsync(),
      getPrefAsync(scope.value, ENROLLED_ONLY_KEY),
    ])
    speakers.value = list
    enrolledOnly.value = only !== 'false' && only !== '0'
  } catch (e) {
    toast.error(
      t('settings.assistantPage.speakers.loadFailed', { reason: errString(e) }),
    )
  }
}

async function setEnrolledOnlyAsync(next: boolean) {
  const previous = enrolledOnly.value
  enrolledOnly.value = next
  saving.value = true
  try {
    // action-exempt: whom the assistant listens to on this device is the user's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, ENROLLED_ONLY_KEY, String(next))
  } catch (e) {
    enrolledOnly.value = previous
    toast.error(
      t('settings.assistantPage.saveFailed', { reason: errString(e) }),
    )
  } finally {
    saving.value = false
  }
}

async function removeAsync(speaker: EnrolledSpeaker) {
  deleting.value = speaker.id
  try {
    // action-exempt: removing an enrolled voice is a local privacy control, not an assistant action.
    await deleteAsync(speaker.id)
    speakers.value = speakers.value.filter((s) => s.id !== speaker.id)
  } catch (e) {
    toast.error(
      t('settings.assistantPage.speakers.deleteFailed', {
        reason: errString(e),
      }),
    )
  } finally {
    deleting.value = null
  }
}

function onSaved(speaker: EnrolledSpeaker) {
  if (speakers.value.some((current) => current.id === speaker.id)) return
  speakers.value = [...speakers.value, speaker].sort((a, b) =>
    a.name.localeCompare(b.name),
  )
}

onMounted(() => reloadAsync())
onVaultTablesChanged(['haex_assistant_speakers', 'preferences'], () => {
  if (!saving.value) return reloadAsync()
})
</script>

<template>
  <SettingsGroup
    :label="t('settings.assistantPage.speakers.title')"
    data-testid="settings-assistant-speakers"
  >
    <SettingsRow
      :description="t('settings.assistantPage.speakers.description')"
    />
    <SettingsRow
      v-for="speaker in speakers"
      :key="speaker.id"
      :title="speaker.name"
      :description="
        speaker.outdated
          ? t('settings.assistantPage.speakers.outdated')
          : undefined
      "
    >
      <DeleteButton
        :label="
          t('settings.assistantPage.speakers.delete', { name: speaker.name })
        "
        :loading="deleting === speaker.id"
        data-testid="settings-assistant-speaker-delete"
        @click="removeAsync(speaker)"
      />
    </SettingsRow>
    <SettingsRow
      v-if="speakers.length > 0"
      :title="t('settings.assistantPage.speakers.enrolledOnly')"
      :description="t('settings.assistantPage.speakers.enrolledOnlyHint')"
      label-for="settings-assistant-enrolled-only"
    >
      <ShadcnSwitch
        id="settings-assistant-enrolled-only"
        :model-value="enrolledOnly"
        data-testid="settings-assistant-enrolled-only"
        @update:model-value="setEnrolledOnlyAsync($event === true)"
      />
    </SettingsRow>
    <SettingsRow :title="t('settings.assistantPage.speakers.add')">
      <UiButton
        type="button"
        variant="outline"
        size="sm"
        data-testid="settings-assistant-speaker-add"
        @click="enrolling = true"
      >
        <Icon name="lucide:user-plus" class="size-4" aria-hidden="true" />
        {{ t('settings.assistantPage.speakers.addButton') }}
      </UiButton>
    </SettingsRow>
  </SettingsGroup>
  <SettingsAssistantSpeakerEnrolDialog
    v-model:open="enrolling"
    :language="language"
    @saved="onSaved"
  />
</template>
