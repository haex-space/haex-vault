<script setup lang="ts">
/**
 * The speech model of the language of conversations (spec 081 T038, FR-006, FR-012): "Automatisch"
 * first, naming the model that speaks without a choice (Qwen3-TTS where this build runs it), then
 * every model that speaks the language, best-sounding first. Each row says how the model sounds,
 * its size, licence and languages, and whether it keeps up on this device ("hält mit" / "langsamer
 * als Echtzeit" / "noch nicht gemessen"); models that do not keep up stay selectable. The choice is
 * the device preference `assistant.speechModel.<sprache>`, saved on selection; "Automatisch"
 * removes it. A model that is not here asks before downloading, like a voice; Qwen3-TTS brings the
 * language's Pocket model along, which always stays as the reserve (FR-010a). Emits `changed`
 * when the model that speaks may have changed, so the voices show the right model.
 */
import { toast } from 'vue-sonner'
import {
  speechModelPrefKey,
  type SpeechLanguage,
} from '~/lib/assistant/settings'
import {
  AUTOMATIC,
  chosenValue,
  defaultModel,
  downloadBytes,
  soundPhraseKey,
  speedLabelKey,
  type SpeechModelOption,
} from '~/lib/assistant/speechModel'
import { humanBytes } from '~/lib/models/format'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const props = defineProps<{
  language: SpeechLanguage
}>()
const emit = defineEmits<{ changed: [] }>()

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { setPrefAsync, clearPrefAsync } = usePreferences()
const { speechModelsAsync, applySpeechModelAsync, downloadModelAsync } =
  useTtsVoices()
const downloads = useModelDownloadsStore()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const options = ref<SpeechModelOption[]>([])
/** The language `options` were loaded for; while it differs from `language`, they are stale. */
const loadedLanguage = ref<SpeechLanguage | null>(null)
const chosen = ref<string>(AUTOMATIC)
const saving = ref(false)
const downloadingId = ref<string | null>(null)
/** The model whose download waits for the question (spec 043 FR-028). */
const confirming = ref<SpeechModelOption | null>(null)
const confirmOpen = computed({
  get: () => confirming.value !== null,
  set: (value: boolean) => {
    if (!value) confirming.value = null
  },
})

const automaticTitle = computed(() =>
  t('settings.voice.speechModel.automatic', {
    name: defaultModel(options.value)?.name ?? '',
  }),
)

/** The language in the words of the interface language. */
function languageName(language: string): string {
  const names = new Intl.DisplayNames([locale.value], { type: 'language' })
  const name = names.of(language) ?? language
  return name.charAt(0).toLocaleUpperCase(locale.value) + name.slice(1)
}

/** What a row says under the model's name (FR-006). */
function modelLine(option: SpeechModelOption): string {
  const languages =
    option.languages.length > 1
      ? t('settings.voice.speechModel.languages', {
          count: option.languages.length,
        })
      : languageName(option.languages[0] ?? props.language)
  const parts = [
    t(soundPhraseKey(option.engine)),
    humanBytes(option.sizeBytes),
    option.license,
    languages,
    t(speedLabelKey(option)),
  ]
  const missing = downloadBytes(option, options.value)
  if (missing > 0)
    parts.push(
      t('settings.voice.model.available', {
        size: humanBytes(missing),
        fit: t(`models.filePicker.fit.${option.fit}`),
      }),
    )
  return parts.join(' · ')
}

let reloads = 0

async function reloadAsync() {
  const current = ++reloads
  const requested = props.language
  try {
    const list = await speechModelsAsync(requested)
    if (current !== reloads) return
    options.value = list
    loadedLanguage.value = requested
    chosen.value = chosenValue(list)
  } catch (e) {
    if (current === reloads)
      toast.error(`${t('errors.prefLoadFailed')}: ${errString(e)}`)
  }
}

/** Saves the choice; a model that is not here then asks to be downloaded. */
async function chooseAsync(value: string) {
  const previous = chosen.value
  // A model of the language shown before would be saved under this language's key and dropped.
  if (previous === value || loadedLanguage.value !== props.language) return
  chosen.value = value
  saving.value = true
  try {
    const key = speechModelPrefKey(props.language)
    // action-exempt: how the assistant sounds is this device's own choice; spec 081 gives agents no action for it.
    if (value === AUTOMATIC) await clearPrefAsync(scope.value, key)
    else await setPrefAsync(scope.value, key, value)
  } catch (e) {
    chosen.value = previous
    toast.error(t('settings.voice.saveFailed', { reason: errString(e) }))
    return
  } finally {
    saving.value = false
  }
  await reloadAsync()
  try {
    await applySpeechModelAsync(props.language)
  } catch (e) {
    toast.error(t('settings.voice.saveFailed', { reason: errString(e) }))
  }
  emit('changed')
  const speaking =
    value === AUTOMATIC
      ? defaultModel(options.value)
      : options.value.find((option) => option.id === value)
  if (
    speaking &&
    downloadBytes(speaking, options.value) > 0 &&
    downloadingId.value === null
  )
    confirming.value = speaking
}

async function fetchAsync(option: SpeechModelOption) {
  downloadingId.value = option.id
  try {
    await downloadModelAsync(option.id, props.language)
    await reloadAsync()
    await applySpeechModelAsync(props.language)
    emit('changed')
  } catch (e) {
    toast.error(
      t('settings.voice.model.downloadFailed', { reason: errString(e) }),
    )
  } finally {
    downloadingId.value = null
    downloads.clearDownload(option.id)
  }
}

function confirmed() {
  const option = confirming.value
  confirming.value = null
  if (option) void fetchAsync(option)
}

watch(
  () => props.language,
  () => reloadAsync(),
)
onMounted(async () => {
  await downloads.watchDownloads()
  await reloadAsync()
})
onVaultTablesChanged(['preferences'], () => {
  if (!saving.value) return reloadAsync()
})

defineExpose({ reloadAsync })
</script>

<template>
  <SettingsGroup
    radio
    :label="t('settings.voice.speechModel.title')"
    :model-value="chosen"
    :disabled="saving || loadedLanguage !== language"
    :data-testid="`settings-speech-model-${language}`"
    @update:model-value="chooseAsync(String($event))"
  >
    <SettingsOptionRow
      type="radio"
      :value="AUTOMATIC"
      :title="automaticTitle"
      :description="t('settings.voice.speechModel.automaticDescription')"
      data-testid="settings-speech-model-automatic"
    />
    <SettingsOptionRow
      v-for="option in options"
      :key="option.id"
      type="radio"
      :value="option.id"
      :title="option.name"
      :data-testid="`settings-speech-model-option-${option.id}`"
    >
      <template #backdrop>
        <ModelsDownloadBar :model-id="option.id" />
      </template>
      <template #description>
        <span class="flex flex-col">
          <span>{{ modelLine(option) }}</span>
          <ModelsDownloadStatus :model-id="option.id" />
        </span>
      </template>
      <template v-if="downloadBytes(option, options) > 0" #trailing>
        <UiButton
          type="button"
          variant="outline"
          size="sm"
          :loading="downloadingId === option.id"
          :disabled="downloadingId !== null"
          :aria-label="
            t('settings.voice.speechModel.downloadOf', { name: option.name })
          "
          :data-testid="`settings-speech-model-download-${option.id}`"
          @click.stop.prevent="confirming = option"
        >
          {{
            downloadingId === option.id
              ? t('settings.voice.model.downloading')
              : t('settings.voice.model.download')
          }}
        </UiButton>
      </template>
    </SettingsOptionRow>
  </SettingsGroup>

  <ModelsDownloadConfirmStep
    v-model:open="confirmOpen"
    :target="
      confirming && {
        kind: 'file',
        sizeBytes: downloadBytes(confirming, options),
      }
    "
    :name="confirming?.name ?? ''"
    @confirm="confirmed"
  />
</template>
