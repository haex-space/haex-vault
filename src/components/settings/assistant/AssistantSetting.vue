<script setup lang="ts">
/**
 * "Sprachassistent" under KI → Sprache (spec 066 T045, FR-038): the pause that ends what was said
 * (`assistant.pauseMs`, FR-012) and the language of spoken conversations (`assistant.language`, FR-002a): the interface language
 * unless the user picks another one, for example to practise it. Below, the voices of that language
 * and the speaking rate (`VoiceSetting`; one page with it and no list of what is missing since
 * operator 2026-11-03: a voice that is not here shows its own download), the wake word with its
 * sensitivity (`WakeWordSetting`, FR-011), and the people haex listens to (`SpeakersSetting`,
 * FR-016a).
 * Saved on selection (spec 023 FR-021).
 */
import { toast } from 'vue-sonner'
import {
  assistantLabelKey,
  CONVERSATION_LANGUAGE_KEY,
  CONVERSATION_LANGUAGES,
  parseConversationLanguage,
  spokenLanguage,
  type ConversationLanguage,
  parsePause,
  PAUSE_KEY,
  PAUSE_MS,
} from '~/lib/assistant/settings'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const conversationLanguage = ref<ConversationLanguage>('ui')

function languageName(language: string): string {
  const name =
    new Intl.DisplayNames([locale.value], { type: 'language' }).of(language) ??
    language
  return name.charAt(0).toLocaleUpperCase(locale.value) + name.slice(1)
}

/** "Wie die Oberfläche (Deutsch)", then each language in the interface's words. */
const languageOptions = computed(() =>
  CONVERSATION_LANGUAGES.map((value) => ({
    value,
    label:
      value === 'ui'
        ? t('settings.assistantPage.languageUi', {
            language: languageName(spokenLanguage('ui', locale.value)),
          })
        : languageName(value),
  })),
)
const pauseMs = ref<number>(PAUSE_MS.default)
const pauseLabel = computed(() =>
  t('settings.assistantPage.seconds', {
    value: (pauseMs.value / 1000).toLocaleString(locale.value, {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    }),
  }),
)
const loading = ref(true)
/** The settings could not be read (shown as a toast); the page then shows no controls. */
const loadFailed = ref(false)
const saving = ref(false)

async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadFailed.value = false
  try {
    const [pause, language] = await Promise.all([
      getPrefAsync(scope.value, PAUSE_KEY),
      getPrefAsync(scope.value, CONVERSATION_LANGUAGE_KEY),
    ])
    pauseMs.value = parsePause(pause)
    conversationLanguage.value = parseConversationLanguage(language)
  } catch (e) {
    loadFailed.value = true
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(e)}`)
  } finally {
    loading.value = false
  }
}

async function savePauseAsync() {
  saving.value = true
  try {
    // action-exempt: how long the assistant waits is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, PAUSE_KEY, String(pauseMs.value))
  } catch (e) {
    toast.error(
      t('settings.assistantPage.saveFailed', {
        reason: errString(e),
      }),
    )
  } finally {
    saving.value = false
  }
}

async function saveLanguageAsync(value: string) {
  const previous = conversationLanguage.value
  conversationLanguage.value = parseConversationLanguage(value)
  saving.value = true
  try {
    // action-exempt: the language of this device's spoken conversations is the user's own choice on the assistant page; spec 066 gives agents no action for it.
    await setPrefAsync(
      scope.value,
      CONVERSATION_LANGUAGE_KEY,
      conversationLanguage.value,
    )
  } catch (e) {
    // The select shows what is stored, not a choice that was never saved.
    conversationLanguage.value = previous
    toast.error(
      t('settings.assistantPage.saveFailed', {
        reason: errString(e),
      }),
    )
  } finally {
    saving.value = false
  }
}

onMounted(() => reloadAsync())
onVaultTablesChanged(['preferences'], () => {
  if (!saving.value) return reloadAsync(true)
})
</script>

<template>
  <section class="flex flex-col gap-6">
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <template v-if="!loading && !loadFailed">
      <SettingsGroup>
        <SettingsRow
          :title="t(assistantLabelKey('assistant.language'))"
          :description="t('settings.assistantPage.languageHint')"
          label-for="settings-assistant-language"
        >
          <SettingsSelect
            id="settings-assistant-language"
            :model-value="conversationLanguage"
            :options="languageOptions"
            :disabled="saving"
            data-testid="settings-assistant-language"
            @update:model-value="saveLanguageAsync"
          />
        </SettingsRow>
        <SettingsRow
          :title="t(assistantLabelKey('assistant.pauseMs'))"
          :description="t('settings.assistantPage.pauseHint')"
          label-for="settings-assistant-pause"
        >
          <span class="flex items-center gap-3">
            <input
              id="settings-assistant-pause"
              v-model.number="pauseMs"
              type="range"
              :min="PAUSE_MS.min"
              :max="PAUSE_MS.max"
              :step="PAUSE_MS.step"
              :aria-valuetext="pauseLabel"
              class="w-36 accent-primary"
              data-testid="settings-assistant-pause"
              @change="savePauseAsync"
            />
            <span class="w-12 text-right text-sm tabular-nums">
              {{ pauseLabel }}
            </span>
          </span>
        </SettingsRow>
      </SettingsGroup>

      <SettingsAssistantWakeWordSetting />

      <SettingsAssistantVoiceSetting
        :language="spokenLanguage(conversationLanguage, locale)"
      />

      <SettingsAssistantSpeakersSetting
        :language="spokenLanguage(conversationLanguage, locale)"
      />
    </template>
  </section>
</template>
