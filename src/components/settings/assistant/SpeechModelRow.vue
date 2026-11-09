<script setup lang="ts">
/**
 * The speech model of the language of conversations (spec 081 T038, FR-006, FR-006a, FR-012): every
 * model that speaks the language, best-sounding first, one row each; the model that speaks without
 * a choice (Pocket TTS, operator 2026-11-06) is marked "Standard" in its own row, with
 * no separate "Automatisch" row. Each row says how the model sounds, its size, licence and
 * languages; whether a model keeps up is not shown, only the hint of FR-011 uses it. The choice is
 * the device preference `assistant.speechModel.<sprache>`, saved on selection; choosing the default
 * removes it, so the device keeps following the default. A model that is not here shows a download
 * icon and asks before downloading, like a voice; Qwen3-TTS brings the language's Pocket model
 * along. An installed model shows a delete icon, except Pocket, which always stays as the reserve
 * (FR-010a) and keeps the icon's place so the rows line up (operator 2026-11-06). Emits `changed`
 * when the model that speaks may have changed, so the voices show the right model.
 */
import { toast } from 'vue-sonner'
import {
  speechModelPrefKey,
  type SpeechLanguage,
} from '~/lib/assistant/settings'
import {
  chosenValue,
  downloadBytes,
  mayDelete,
  preferenceFor,
  reserveBytes,
  soundPhraseKey,
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
const {
  speechModelsAsync,
  applySpeechModelAsync,
  downloadModelAsync,
  deleteModelAsync,
} = useTtsVoices()
const downloads = useModelDownloadsStore()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const options = ref<SpeechModelOption[]>([])
/** The language `options` were loaded for; while it differs from `language`, they are stale. */
const loadedLanguage = ref<SpeechLanguage | null>(null)
const chosen = ref<string>('')
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
  ]
  if (downloadBytes(option, options.value) > 0) {
    const reserve = reserveBytes(option, options.value)
    if (reserve > 0)
      parts.push(
        t('settings.voice.speechModel.withReserve', {
          size: humanBytes(reserve),
        }),
      )
    parts.push(t(`models.filePicker.fit.${option.fit}`))
  }
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
    const preference = preferenceFor(value, options.value)
    // action-exempt: how the assistant sounds is this device's own choice; spec 081 gives agents no action for it.
    if (preference === null) await clearPrefAsync(scope.value, key)
    else await setPrefAsync(scope.value, key, preference)
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
  const speaking = options.value.find((option) => option.id === value)
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

/** The model whose deletion waits for the question. */
const removing = ref<SpeechModelOption | null>(null)
const removeOpen = computed({
  get: () => removing.value !== null,
  set: (value: boolean) => {
    if (!value) removing.value = null
  },
})
const deletingId = ref<string | null>(null)

async function removeAsync() {
  const option = removing.value
  removing.value = null
  if (!option) return
  deletingId.value = option.id
  try {
    // action-exempt: freeing this device's disk is its own choice; spec 081 gives agents no action for it.
    await deleteModelAsync(option.id)
    await reloadAsync()
    await applySpeechModelAsync(props.language)
    emit('changed')
  } catch (e) {
    toast.error(
      t('settings.voice.speechModel.deleteFailed', { reason: errString(e) }),
    )
  } finally {
    deletingId.value = null
  }
}

/** One model at a time: a download or delete that runs keeps the others from starting. The
 * buttons are not disabled for it, since a row holding a disabled control looks greyed out. */
function busy(): boolean {
  return downloadingId.value !== null || deletingId.value !== null
}

function askDelete(option: SpeechModelOption) {
  if (!busy()) removing.value = option
}

function askDownload(option: SpeechModelOption) {
  if (!busy()) confirming.value = option
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
      v-for="option in options"
      :key="option.id"
      type="radio"
      :value="option.id"
      :title="
        option.isDefault
          ? t('settings.voice.speechModel.default', { name: option.name })
          : option.name
      "
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
      <template #trailing>
        <DeleteButton
          v-if="option.installed"
          :label="
            t('settings.voice.speechModel.deleteOf', { name: option.name })
          "
          :placeholder="!mayDelete(option)"
          :loading="deletingId === option.id"
          :data-testid="
            mayDelete(option)
              ? `settings-speech-model-delete-${option.id}`
              : undefined
          "
          @click.stop.prevent="askDelete(option)"
        />
        <DownloadButton
          v-else
          :label="
            t('settings.voice.speechModel.downloadOf', { name: option.name })
          "
          :loading="downloadingId === option.id"
          :data-testid="`settings-speech-model-download-${option.id}`"
          @click.stop.prevent="askDownload(option)"
        />
      </template>
    </SettingsOptionRow>
  </SettingsGroup>

  <ShadcnAlertDialog v-model:open="removeOpen">
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('settings.voice.speechModel.deleteTitle', {
            name: removing?.name ?? '',
          })
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>{{
          t('settings.voice.speechModel.deleteBody')
        }}</ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('settings.voice.speechModel.deleteCancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          type="button"
          variant="destructive"
          data-testid="settings-speech-model-delete-confirm"
          @click="removeAsync"
        >
          {{ t('settings.voice.speechModel.delete') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>

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
