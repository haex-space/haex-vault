<script setup lang="ts">
/**
 * The voice part of "Sprachassistent" under KI → Sprache (spec 066 T029, T160, FR-029, FR-030;
 * one page since operator 2026-11-03): the voices of the language of conversations (`language`),
 * each its own row, then the speaking rate (operator 2026-11-03: no tabs, no separate speech
 * model row). A voice that is here is chosen and heard with "Probe"; one that is not shows its size
 * and "Laden", with the same question before the download and the progress behind its row as the
 * other models. Downloading is its own action (clarification 2026-09-26 of spec 023); voices
 * sharing a speech model are ready together. Both choices are device preferences, saved on selection
 * (spec 023 FR-021): `assistant.voice.<sprache>` holds a `VoiceRef`, `assistant.speechRate` the
 * rate.
 */
import { toast } from 'vue-sonner'
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
import type { VoiceOption } from '~/composables/useTtsVoices'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const props = defineProps<{
  /** The language of conversations, whose voices are shown. */
  language: SpeechLanguage
}>()

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { listAsync, previewAsync, downloadModelAsync } = useTtsVoices()
const downloads = useModelDownloadsStore()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const voices = ref<VoiceOption[]>([])
/** The chosen `VoiceRef` per language; a missing or unknown preference shows the default. */
const chosen = ref<Partial<Record<SpeechLanguage, string>>>({})
const rate = ref<number>(SPEECH_RATE.default)
/** The voices shown: those of the language of conversations. */
const shownVoices = computed(() => voicesOf(props.language))

const loading = ref(true)
const loadError = ref<string | null>(null)
const saving = ref(false)
/** The voice whose sample is being prepared or plays. */
const previewing = ref<string | null>(null)
/** The speech model being downloaded from this page. */
const downloadingId = ref<string | null>(null)

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

/** What a voice row says under its name: default or not, and size and fit while it is not here. */
function voiceLine(voice: VoiceOption): string {
  const parts: string[] = []
  if (voice.isDefault) parts.push(t('settings.voice.default'))
  if (!voice.installed)
    parts.push(
      t('settings.voice.model.available', {
        size: humanBytes(voice.sizeBytes),
        fit: t(`models.filePicker.fit.${voice.fit}`),
      }),
    )
  return parts.join(' · ')
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
  try {
    // action-exempt: how the assistant sounds is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, key, value)
  } catch (e) {
    toast.error(t('settings.voice.saveFailed', { reason: errString(e) }))
  } finally {
    saving.value = false
  }
}

/** Choosing a voice that is not here yet also asks to download it. */
async function chooseVoiceAsync(language: SpeechLanguage, ref: string) {
  if (chosen.value[language] === ref) return
  chosen.value = { ...chosen.value, [language]: ref }
  await saveAsync(voicePrefKey(language), ref)
  const voice = voices.value.find((candidate) => candidate.ref === ref)
  if (voice && !voice.installed && downloadingId.value === null)
    confirming.value = voice
}

async function saveRateAsync() {
  await saveAsync(SPEECH_RATE_KEY, String(rate.value))
}

async function playAsync(voice: VoiceOption) {
  previewing.value = voice.ref
  try {
    await previewAsync(voice.ref, rate.value)
  } catch (e) {
    toast.error(t('settings.voice.previewFailed', { reason: errString(e) }))
  } finally {
    if (previewing.value === voice.ref) previewing.value = null
  }
}

/** The speech model whose download waits for the question (spec 043 FR-028). */
const confirming = ref<VoiceOption | null>(null)
const confirmOpen = computed({
  get: () => confirming.value !== null,
  set: (value: boolean) => {
    if (!value) confirming.value = null
  },
})

async function fetchModelAsync(entry: VoiceOption) {
  downloadingId.value = entry.modelId
  try {
    await downloadModelAsync(entry.modelId)
    await reloadAsync(true)
  } catch (e) {
    toast.error(
      t('settings.voice.model.downloadFailed', {
        reason: errString(e),
      }),
    )
  } finally {
    downloadingId.value = null
    downloads.clearDownload(entry.modelId)
  }
}

function confirmed() {
  const entry = confirming.value
  confirming.value = null
  if (entry) void fetchModelAsync(entry)
}

onMounted(async () => {
  await downloads.watchDownloads()
  await reloadAsync()
})
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
      <SettingsGroup
        radio
        :label="
          t('settings.voice.voicesOf', { language: languageName(language) })
        "
        :model-value="chosen[language]"
        :disabled="saving"
        :data-testid="`settings-voice-${language}`"
        @update:model-value="chooseVoiceAsync(language, String($event))"
      >
        <SettingsOptionRow
          v-for="voice in shownVoices"
          :key="voice.ref"
          type="radio"
          :value="voice.ref"
          :title="voice.name"
          :data-testid="`settings-voice-option-${voice.ref}`"
        >
          <template #backdrop>
            <ModelsDownloadBar :model-id="voice.modelId" />
          </template>
          <template #description>
            <span class="flex flex-col">
              <span v-if="voiceLine(voice)">{{ voiceLine(voice) }}</span>
              <ModelsDownloadStatus :model-id="voice.modelId" />
            </span>
          </template>
          <template #trailing>
            <UiButton
              v-if="voice.installed"
              type="button"
              variant="outline"
              size="sm"
              :loading="previewing === voice.ref"
              :aria-label="t('settings.voice.previewOf', { name: voice.name })"
              :data-testid="`settings-voice-preview-${voice.ref}`"
              @click.stop.prevent="playAsync(voice)"
            >
              {{ t('settings.voice.preview') }}
            </UiButton>
            <UiButton
              v-else
              type="button"
              variant="outline"
              size="sm"
              :loading="downloadingId === voice.modelId"
              :disabled="downloadingId !== null"
              :aria-label="t('settings.voice.downloadOf', { name: voice.name })"
              :data-testid="`settings-voice-download-${voice.ref}`"
              @click.stop.prevent="confirming = voice"
            >
              {{
                downloadingId === voice.modelId
                  ? t('settings.voice.model.downloading')
                  : t('settings.voice.model.download')
              }}
            </UiButton>
          </template>
        </SettingsOptionRow>
      </SettingsGroup>

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

      <ModelsDownloadConfirmStep
        v-model:open="confirmOpen"
        :target="confirming && { kind: 'tts', id: confirming.modelId }"
        :name="
          confirming
            ? t('settings.voice.downloadName', {
                name: confirming.name,
                language: languageName(confirming.language),
              })
            : ''
        "
        @confirm="confirmed"
      />
    </template>
  </section>
</template>
