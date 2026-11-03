<script setup lang="ts">
/**
 * "Sprachassistent" under KI → Sprache (spec 066 T045): until the wake word (PR D), the preview
 * switch `assistant.preview` shows the assistant's toggle in the dock and the launcher (operator
 * 2026-11-02). Below it, what the assistant still needs on this device (`assistant_readiness`); a
 * model haex can fetch links to its page, which asks before the download and shows its progress
 * (spec 043 FR-028). The pause that ends what was said (`assistant.pauseMs`, FR-012) is set here
 * too, and the language of spoken conversations; below, the voice of each language and the
 * speaking rate (`VoiceSetting`, one page with it since operator 2026-11-03), and (`assistant.language`, FR-002a): the interface
 * language unless the user picks another one, for example to practise it. Saved on selection
 * (spec 023 FR-021).
 */
import { invoke } from '@tauri-apps/api/core'
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

type Missing =
  | 'sttModel'
  | 'vadModel'
  | 'wakeWordModel'
  | 'voice'
  | 'chatModel'
  | 'microphone'

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
/** The settings could not be read (shown as a toast); the page then shows no controls. */
const loadFailed = ref(false)
const saving = ref(false)
const savedFlash = ref(false)
/** What is missing; the wake word is left out, the preview does without it. */
const missing = ref<Missing[] | null>(null)

/** The page that fetches a missing piece. */
const PAGE: Partial<Record<Missing, string>> = {
  sttModel: '/ai/speech',
  chatModel: '/ai/local',
}

async function loadReadinessAsync() {
  try {
    const readiness = await invoke<{ missing: Missing[] }>(
      'assistant_readiness',
      { language: spokenLanguage(conversationLanguage.value, locale.value) },
    )
    missing.value = readiness.missing.filter((m) => m !== 'wakeWordModel')
  } catch {
    // Builds without `voice` have no assistant to get ready.
    missing.value = null
  }
}

async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadFailed.value = false
  try {
    const [shown, pause, language] = await Promise.all([
      getPrefAsync(scope.value, PREVIEW_KEY),
      getPrefAsync(scope.value, PAUSE_KEY),
      getPrefAsync(scope.value, CONVERSATION_LANGUAGE_KEY),
    ])
    preview.value = parsePreview(shown)
    pauseMs.value = parsePause(pause)
    conversationLanguage.value = parseConversationLanguage(language)
    await loadReadinessAsync()
  } catch (e) {
    loadFailed.value = true
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(e)}`)
  } finally {
    loading.value = false
  }
}

async function toggleAsync(next: boolean) {
  preview.value = next
  saving.value = true
  savedFlash.value = false
  try {
    await assistant.setPreviewAsync(next)
    savedFlash.value = true
  } catch (e) {
    toast.error(
      t('settings.assistantPage.saveFailed', { reason: errString(e) }),
    )
  } finally {
    saving.value = false
  }
}

async function savePauseAsync() {
  saving.value = true
  savedFlash.value = false
  try {
    // action-exempt: how long the assistant waits is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, PAUSE_KEY, String(pauseMs.value))
    savedFlash.value = true
  } catch (e) {
    toast.error(
      t('settings.assistantPage.saveFailed', { reason: errString(e) }),
    )
  } finally {
    saving.value = false
  }
}

async function saveLanguageAsync(value: string) {
  conversationLanguage.value = parseConversationLanguage(value)
  saving.value = true
  savedFlash.value = false
  opError.value = null
  try {
    // action-exempt: the language of this device's spoken conversations is the user's own choice on the assistant page; spec 066 gives agents no action for it.
    await setPrefAsync(
      scope.value,
      CONVERSATION_LANGUAGE_KEY,
      conversationLanguage.value,
    )
    savedFlash.value = true
    await loadReadinessAsync()
  } catch (e) {
    opError.value = t('settings.assistantPage.saveFailed', {
      reason: errString(e),
    })
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

      <SettingsGroup
        v-if="missing !== null"
        :label="t('settings.assistantPage.readiness')"
      >
        <SettingsRow
          v-if="missing.length === 0"
          :title="t('settings.assistantPage.ready')"
          data-testid="settings-assistant-ready"
        />
        <SettingsRow
          v-for="item in missing"
          :key="item"
          :title="t(`settings.assistantPage.missing.${item}`)"
          :to="PAGE[item]"
          :data-testid="`settings-assistant-missing-${item}`"
        />
      </SettingsGroup>

      <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
        {{ t('settings.assistantPage.saved') }}
      </p>
      <h2 class="px-1 text-sm font-semibold">
        {{ t('settings.assistantPage.voiceHeading') }}
      </h2>
      <SettingsAssistantVoiceSetting
        :language="spokenLanguage(conversationLanguage, locale)"
      />
    </template>
  </section>
</template>
