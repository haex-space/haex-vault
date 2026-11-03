<script setup lang="ts">
/**
 * The voice part of "Sprachassistent" under KI → Sprache (spec 066 T029, T160, FR-029, FR-030;
 * one page since operator 2026-11-03): the speaking rate, then one tab per language, opened at the
 * language of conversations (`language`). A tab shows the speech model of that language, with the same download row as
 * the chat and speech recognition models (button, question before the download, progress behind
 * the row, "x of y" line), and below it the voices of the language with a sample each. A sample
 * needs the speech model, so it waits until the model is here; downloading is its own action
 * (clarification 2026-09-26 of spec 023). Both choices are device preferences, saved on selection
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
  /** The language of conversations; its tab opens and follows a new choice. */
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
/** The open tab: the language of conversations; the other tabs stay reachable. */
const tab = ref<SpeechLanguage>(props.language)
watch(
  () => props.language,
  (language) => {
    tab.value = language
  },
)

const loading = ref(true)
/** The settings could not be read (shown as a toast); the page then shows no controls. */
const loadFailed = ref(false)
const saving = ref(false)
const savedFlash = ref(false)
/** The voice whose sample is being prepared or plays. */
const previewing = ref<string | null>(null)
/** The speech model being downloaded from this page. */
const downloadingId = ref<string | null>(null)

function voicesOf(language: SpeechLanguage): VoiceOption[] {
  return voices.value.filter((voice) => voice.language === language)
}

/** The speech model of a language, as its voices describe it (they all share one). */
function modelOf(language: SpeechLanguage): VoiceOption | undefined {
  return voicesOf(language)[0]
}

/** The speech model of the open tab. */
const model = computed(() => modelOf(tab.value))

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

/** Size and fit of a speech model that is not here yet. */
function availableLine(entry: VoiceOption): string {
  return t('settings.voice.model.available', {
    size: humanBytes(entry.sizeBytes),
    fit: t(`models.filePicker.fit.${entry.fit}`),
  })
}

/** A `quiet` reload (after a change from elsewhere) keeps the shown values until the new ones are read. */
async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadFailed.value = false
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
    loadFailed.value = true
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(e)}`)
  } finally {
    loading.value = false
  }
}

async function saveAsync(key: string, value: string) {
  saving.value = true
  savedFlash.value = false
  try {
    // action-exempt: how the assistant sounds is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, key, value)
    savedFlash.value = true
  } catch (e) {
    toast.error(t('settings.voice.saveFailed', { reason: errString(e) }))
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
      t('settings.voice.model.downloadFailed', { reason: errString(e) }),
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

    <template v-if="!loading && !loadFailed">
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

      <ShadcnTabs
        :model-value="tab"
        @update:model-value="tab = $event as SpeechLanguage"
      >
        <div class="overflow-x-auto">
          <ShadcnTabsList :aria-label="t('settings.voice.languages')">
            <ShadcnTabsTrigger
              v-for="tabLanguage in SPEECH_LANGUAGES"
              :key="tabLanguage"
              :value="tabLanguage"
              :data-testid="`settings-voice-tab-${tabLanguage}`"
            >
              {{ languageName(tabLanguage) }}
            </ShadcnTabsTrigger>
          </ShadcnTabsList>
        </div>
      </ShadcnTabs>

      <div
        v-if="model"
        :key="tab"
        class="flex flex-col gap-6"
        :data-testid="`settings-voice-${tab}`"
      >
        <SettingsGroup>
          <SettingsRow
            :title="t('settings.voice.model.title')"
            :data-testid="`settings-voice-model-${tab}`"
          >
            <template #backdrop>
              <ModelsDownloadBar :model-id="model.modelId" />
            </template>
            <template #description>
              <span class="flex flex-col">
                <span>
                  {{
                    model.installed
                      ? t('settings.voice.model.ready')
                      : availableLine(model)
                  }}
                </span>
                <ModelsDownloadStatus :model-id="model.modelId" />
              </span>
            </template>
            <UiButton
              v-if="!model.installed"
              type="button"
              variant="outline"
              size="sm"
              :loading="downloadingId === model.modelId"
              :disabled="downloadingId !== null"
              :data-testid="`settings-voice-download-${tab}`"
              @click="confirming = model"
            >
              {{
                downloadingId === model.modelId
                  ? t('settings.voice.model.downloading')
                  : t('settings.voice.model.download')
              }}
            </UiButton>
          </SettingsRow>
        </SettingsGroup>

        <SettingsGroup
          radio
          :label="t('settings.voice.voicesLabel')"
          :model-value="chosen[tab]"
          :disabled="saving"
          @update:model-value="chooseVoiceAsync(tab, String($event))"
        >
          <SettingsOptionRow
            v-for="voice in voicesOf(tab)"
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
                :disabled="!voice.installed"
                :title="
                  voice.installed ? undefined : t('settings.voice.needsModel')
                "
                :aria-label="
                  t('settings.voice.previewOf', { name: voice.name })
                "
                :data-testid="`settings-voice-preview-${voice.ref}`"
                @click.stop.prevent="playAsync(voice)"
              >
                {{ t('settings.voice.preview') }}
              </UiButton>
            </template>
          </SettingsOptionRow>
        </SettingsGroup>
        <p v-if="!model.installed" class="px-1 text-xs text-muted-foreground">
          {{ t('settings.voice.needsModel') }}
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
    </template>
  </section>
</template>
