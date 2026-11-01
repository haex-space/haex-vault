<script setup lang="ts">
/**
 * "Stimme" under KI → Sprache (spec 066 T029, FR-029, FR-030): the voices of each of the six
 * languages in a boxed list, a sample ("Probe") per voice and the speaking rate. Both are device
 * preferences, saved on selection (spec 023 FR-021): `assistant.voice.<sprache>` holds a
 * `VoiceRef`, `assistant.speechRate` the rate. A voice needs the speech model of its language; a
 * sample of a voice whose model is missing asks before the download (spec 043 FR-028).
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  assistantLabelKey,
  parseSpeechRate,
  SPEECH_LANGUAGES,
  SPEECH_RATE,
  SPEECH_RATE_KEY,
  voicePrefKey,
  type SpeechLanguage,
} from '~/lib/assistant/settings'
import { humanBytes } from '~/lib/models/format'
import {
  ASSISTANT_DOWNLOAD_PROGRESS,
  type AssistantDownloadProgress,
  type VoiceOption,
} from '~/composables/useTtsVoices'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { listAsync, previewAsync } = useTtsVoices()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const voices = ref<VoiceOption[]>([])
/** The chosen `VoiceRef` per language; a missing or unknown preference shows the default. */
const chosen = ref<Partial<Record<SpeechLanguage, string>>>({})
const rate = ref<number>(SPEECH_RATE.default)

const loading = ref(true)
const loadError = ref<string | null>(null)
const saving = ref(false)
const savedFlash = ref(false)
const opError = ref<string | null>(null)
/** The voice whose sample is being prepared or plays. */
const previewing = ref<string | null>(null)
/** Running speech model downloads by model id. */
const progress = ref<Record<string, AssistantDownloadProgress>>({})

function voicesOf(language: SpeechLanguage): VoiceOption[] {
  return voices.value.filter((voice) => voice.language === language)
}

/** The language in the words of the interface language ("Französisch", "French"). */
function languageName(language: SpeechLanguage): string {
  const names = new Intl.DisplayNames([locale.value], { type: 'language' })
  const name = names.of(language) ?? language
  return name.charAt(0).toLocaleUpperCase(locale.value) + name.slice(1)
}

const rateLabel = computed(() =>
  t('settings.voice.rateValue', {
    rate: rate.value.toLocaleString(locale.value, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }),
  }),
)

/** One line under a language: whether its speech model is here, else its size and fit. */
function modelLine(language: SpeechLanguage): string {
  const voice = voicesOf(language)[0]
  if (!voice) return ''
  if (voice.installed) return t('settings.voice.model.installed')
  return t('settings.voice.model.missing', {
    size: humanBytes(voice.sizeBytes),
    fit: t(`models.filePicker.fit.${voice.fit}`),
  })
}

function percentOf(language: SpeechLanguage): number | null {
  const voice = voicesOf(language)[0]
  const state = voice && progress.value[voice.modelId]
  if (!state) return null
  return state.total > 0 ? Math.round((state.received / state.total) * 100) : 0
}

/** A `quiet` reload (after a change from elsewhere) keeps the shown values until the new ones are read. */
async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadError.value = null
  try {
    const [list, storedRate, ...stored] = await Promise.all([
      listAsync(),
      getPrefAsync(scope.value, SPEECH_RATE_KEY),
      ...SPEECH_LANGUAGES.map((language) =>
        getPrefAsync(scope.value, voicePrefKey(language)),
      ),
    ])
    voices.value = list
    rate.value = parseSpeechRate(storedRate)
    chosen.value = Object.fromEntries(
      SPEECH_LANGUAGES.map((language, index) => {
        const own = list.filter((voice) => voice.language === language)
        const pref = stored[index]
        const ref = own.some((voice) => voice.ref === pref)
          ? pref
          : own.find((voice) => voice.isDefault)?.ref
        return [language, ref ?? '']
      }),
    )
  } catch (e) {
    loadError.value = errString(e)
  } finally {
    loading.value = false
  }
}

async function saveAsync(key: string, value: string) {
  saving.value = true
  savedFlash.value = false
  opError.value = null
  try {
    // action-exempt: how the assistant sounds is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, key, value)
    savedFlash.value = true
  } catch (e) {
    opError.value = t('settings.voice.saveFailed', { reason: errString(e) })
  } finally {
    saving.value = false
  }
}

async function chooseVoiceAsync(language: SpeechLanguage, ref: string) {
  if (chosen.value[language] === ref) return
  chosen.value = { ...chosen.value, [language]: ref }
  await saveAsync(voicePrefKey(language), ref)
}

async function saveRateAsync() {
  await saveAsync(SPEECH_RATE_KEY, String(rate.value))
}

async function playAsync(voice: VoiceOption) {
  previewing.value = voice.ref
  opError.value = null
  try {
    await previewAsync(voice.ref, rate.value)
    if (!voice.installed) await reloadAsync(true)
  } catch (e) {
    opError.value = t('settings.voice.previewFailed', { reason: errString(e) })
  } finally {
    if (previewing.value === voice.ref) previewing.value = null
  }
}

/** The voice whose sample waits for the download question. */
const confirming = ref<VoiceOption | null>(null)
const confirmOpen = computed({
  get: () => confirming.value !== null,
  set: (value: boolean) => {
    if (!value) confirming.value = null
  },
})

function askPreview(voice: VoiceOption) {
  if (voice.installed) void playAsync(voice)
  else confirming.value = voice
}

function confirmed() {
  const voice = confirming.value
  confirming.value = null
  if (voice) void playAsync(voice)
}

let unlisten: UnlistenFn | null = null
onMounted(async () => {
  unlisten = await listen<AssistantDownloadProgress>(
    ASSISTANT_DOWNLOAD_PROGRESS,
    ({ payload }) => {
      const done = payload.total > 0 && payload.received >= payload.total
      progress.value = Object.fromEntries([
        ...Object.entries(progress.value).filter(([id]) => id !== payload.id),
        ...(done ? [] : [[payload.id, payload] as const]),
      ])
    },
  )
  await reloadAsync()
})
onUnmounted(() => unlisten?.())
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
          :title="t(assistantLabelKey('assistant.speechRate'))"
          :description="t('settings.deviceOnly')"
          label-for="settings-voice-rate"
        >
          <span class="flex items-center gap-3">
            <input
              id="settings-voice-rate"
              v-model.number="rate"
              type="range"
              :min="SPEECH_RATE.min"
              :max="SPEECH_RATE.max"
              :step="SPEECH_RATE.step"
              :aria-valuetext="rateLabel"
              class="w-36 accent-primary"
              data-testid="settings-voice-rate"
              @change="saveRateAsync"
            />
            <span class="w-12 text-right text-sm tabular-nums">
              {{ rateLabel }}
            </span>
          </span>
        </SettingsRow>
      </SettingsGroup>

      <div
        v-for="language in SPEECH_LANGUAGES"
        :key="language"
        class="flex flex-col gap-2"
      >
        <SettingsGroup
          radio
          :label="languageName(language)"
          :model-value="chosen[language]"
          :disabled="saving"
          :data-testid="`settings-voice-${language}`"
          @update:model-value="chooseVoiceAsync(language, String($event))"
        >
          <SettingsOptionRow
            v-for="voice in voicesOf(language)"
            :key="voice.ref"
            type="radio"
            :value="voice.ref"
            :title="voice.name"
            :description="
              voice.isDefault ? t('settings.voice.default') : undefined
            "
            :data-testid="`settings-voice-option-${voice.ref}`"
          >
            <template #trailing>
              <UiButton
                type="button"
                variant="outline"
                size="sm"
                :loading="previewing === voice.ref"
                :aria-label="
                  t('settings.voice.previewOf', { name: voice.name })
                "
                :data-testid="`settings-voice-preview-${voice.ref}`"
                @click.stop.prevent="askPreview(voice)"
              >
                {{ t('settings.voice.preview') }}
              </UiButton>
            </template>
          </SettingsOptionRow>
        </SettingsGroup>
        <UiProgress
          v-if="percentOf(language) !== null"
          class="h-1 rounded"
          :value="percentOf(language)"
          :label="t('settings.voice.model.downloading')"
        />
        <p v-else class="px-1 text-xs text-muted-foreground">
          {{ modelLine(language) }}
        </p>
      </div>

      <ModelsDownloadConfirmStep
        v-model:open="confirmOpen"
        :target="confirming && { kind: 'tts', id: confirming.modelId }"
        :name="
          confirming
            ? t('settings.voice.model.name', {
                language: languageName(confirming.language),
              })
            : ''
        "
        @confirm="confirmed"
      />

      <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
        {{ t('settings.voice.saved') }}
      </p>
      <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
        {{ opError }}
      </p>
    </template>
  </section>
</template>
