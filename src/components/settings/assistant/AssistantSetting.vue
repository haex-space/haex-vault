<script setup lang="ts">
/**
 * "Sprachassistent" under KI → Sprache (spec 066 T045): until the wake word (PR D), the preview
 * switch `assistant.preview` shows the assistant's toggle in the dock and the launcher (operator
 * 2026-11-02). The pause that ends what was said (`assistant.pauseMs`, FR-012) is set here too, and
 * the language of spoken conversations (`assistant.language`, FR-002a): the interface language
 * unless the user picks another one, for example to practise it. Below, the voices of that language
 * and the speaking rate (`VoiceSetting`; one page with it and no list of what is missing since
 * operator 2026-11-03: a voice that is not here shows its own download).
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
  parsePreview,
  PAUSE_KEY,
  PAUSE_MS,
  PREVIEW_KEY,
} from '~/lib/assistant/settings'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const assistant = useAssistantStore()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const preview = ref(false)
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
const loadError = ref<string | null>(null)
const saving = ref(false)

async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadError.value = null
  try {
    const [shown, pause, language] = await Promise.all([
      getPrefAsync(scope.value, PREVIEW_KEY),
      getPrefAsync(scope.value, PAUSE_KEY),
      getPrefAsync(scope.value, CONVERSATION_LANGUAGE_KEY),
    ])
    preview.value = parsePreview(shown)
    pauseMs.value = parsePause(pause)
    conversationLanguage.value = parseConversationLanguage(language)
  } catch (e) {
    loadError.value = errString(e)
  } finally {
    loading.value = false
  }
}

async function toggleAsync(next: boolean) {
  preview.value = next
  saving.value = true
  try {
    await assistant.setPreviewAsync(next)
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

    <p v-if="loadError" class="text-sm text-destructive" role="alert">
      {{ t('errors.prefLoadFailed') }}: {{ loadError }}
    </p>

    <template v-if="!loading && !loadError">
      <SettingsGroup>
        <SettingsRow
          :title="t(assistantLabelKey('assistant.preview'))"
          :description="t('settings.assistantPage.previewHint')"
          label-for="settings-assistant-preview"
        >
          <ShadcnSwitch
            id="settings-assistant-preview"
            :model-value="preview"
            :disabled="saving"
            data-testid="settings-assistant-preview"
            @update:model-value="toggleAsync($event === true)"
          />
        </SettingsRow>
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

      <SettingsAssistantVoiceSetting
        :language="spokenLanguage(conversationLanguage, locale)"
      />
    </template>
  </section>
</template>
