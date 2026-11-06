<script setup lang="ts">
/**
 * The voice part of "Sprachassistent" under KI → Sprache (spec 066 T029, T160, FR-029, FR-030;
 * one page since operator 2026-11-03): the voices of the language of conversations (`language`),
 * each its own row, then the speaking rate (operator 2026-11-03: no tabs). Above the voices, the
 * speech model of the language (spec 081 T038, `SpeechModelRow`). A voice that is here is chosen and heard with "Probe"; one that is not shows its size
 * and a download icon, with the same question before the download as the other models; its
 * progress shows in the speech model row, whose model it is (operator 2026-11-06). Downloading is its own action (clarification 2026-09-26 of spec 023); voices
 * sharing a speech model are ready together. Both choices are device preferences, saved on selection
 * (spec 023 FR-021): `assistant.voice.<sprache>` holds a `VoiceRef`, `assistant.speechRate` the
 * rate. "Eigene Stimme anlegen" opens `VoiceCloneDialog` (FR-049); own voices are rows like the
 * others, with a delete that asks first and works on every device (FR-050). The list follows
 * `tts-voices-changed`, so a voice made or deleted on another device shows up at once.
 */
import { toast } from 'vue-sonner'
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
import type { VoiceOption } from '~/composables/useTtsVoices'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const props = defineProps<{
  /** The language of conversations, whose voices are shown. */
  language: SpeechLanguage
}>()

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { listAsync, previewAsync, downloadModelAsync, voiceDeleteAsync } =
  useTtsVoices()
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
/** The settings could not be read (shown as a toast); the page then shows no controls. */
const loadFailed = ref(false)
const saving = ref(false)
/** The voice whose sample is being prepared or plays. */
const previewing = ref<string | null>(null)
/** The speech model being downloaded from this page. */
const downloadingId = ref<string | null>(null)
/** The speech model row, reloaded when a voice download changed what is installed. */
const speechModelRow = ref<{ reloadAsync: () => Promise<void> } | null>(null)

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
  if (voice.kind === 'clone') parts.push(t('settings.voice.own'))
  if (!voice.installed)
    parts.push(
      t('settings.voice.model.available', {
        size: humanBytes(voice.sizeBytes),
        fit: t(`models.filePicker.fit.${voice.fit}`),
      }),
    )
  return parts.join(' · ')
}

/** Counts reloads, so an answer overtaken by a newer reload is dropped. */
let reloads = 0

/** A `quiet` reload (after a change from elsewhere) keeps the shown values until the new ones are read. */
async function reloadAsync(quiet = false) {
  const current = ++reloads
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
    if (current !== reloads) return
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
    if (current === reloads) {
      loadFailed.value = true
      toast.error(`${t('errors.prefLoadFailed')}: ${errString(e)}`)
    }
  } finally {
    if (current === reloads) loading.value = false
  }
}

/** Saves a preference; `false` when it failed (shown as a toast). */
async function saveAsync(key: string, value: string): Promise<boolean> {
  saving.value = true
  try {
    // action-exempt: how the assistant sounds is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, key, value)
    return true
  } catch (e) {
    toast.error(t('settings.voice.saveFailed', { reason: errString(e) }))
    return false
  } finally {
    saving.value = false
  }
}

/** Choosing a voice that is not here yet also asks to download it. A failed save shows the voice
 * that is still stored. */
async function chooseVoiceAsync(language: SpeechLanguage, ref: string) {
  const previous = chosen.value[language]
  if (previous === ref) return
  chosen.value = { ...chosen.value, [language]: ref }
  if (!(await saveAsync(voicePrefKey(language), ref))) {
    chosen.value = { ...chosen.value, [language]: previous }
    return
  }
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
    await downloadModelAsync(entry.modelId, entry.language)
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
    // The download installed the speech model and, for Qwen3-TTS, the Pocket reserve, even when
    // it failed half way.
    await speechModelRow.value?.reloadAsync()
  }
}

function confirmed() {
  const entry = confirming.value
  confirming.value = null
  if (entry) void fetchModelAsync(entry)
}

/** "Eigene Stimme anlegen" is open. */
const cloneOpen = ref(false)

/** A new own voice becomes the voice of its language on this device. */
async function clonedAsync(voice: VoiceOption) {
  await reloadAsync(true)
  await chooseVoiceAsync(voice.language, voice.ref)
}

/** The own voice whose delete waits for the question. */
const deleteCandidate = ref<VoiceOption | null>(null)
const deleting = ref(false)

async function confirmDeleteAsync() {
  const voice = deleteCandidate.value
  if (!voice) return
  deleting.value = true
  try {
    await voiceDeleteAsync(voice.ref)
    deleteCandidate.value = null
    await reloadAsync(true)
  } catch (e) {
    toast.error(t('settings.voice.deleteFailed', { reason: errString(e) }))
  } finally {
    deleting.value = false
  }
}

let unlistenVoices: UnlistenFn | null = null
let disposed = false
void listen('tts-voices-changed', () => reloadAsync(true)).then((stop) => {
  if (disposed) stop()
  else unlistenVoices = stop
})
onBeforeUnmount(() => {
  disposed = true
  unlistenVoices?.()
})

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
      <SettingsAssistantSpeechModelRow
        ref="speechModelRow"
        :language="language"
        @changed="reloadAsync(true)"
      />

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
          <!-- The download of a voice is its model's: its progress shows in the speech model
               row above, not behind every voice of the model (operator 2026-11-06). -->
          <template v-if="voiceLine(voice)" #description>
            {{ voiceLine(voice) }}
          </template>
          <template #trailing>
            <DeleteButton
              v-if="voice.kind === 'clone'"
              :label="t('settings.voice.deleteOf', { name: voice.name })"
              :data-testid="`settings-voice-delete-${voice.ref}`"
              @click.stop.prevent="deleteCandidate = voice"
            />
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
            <DownloadButton
              v-else
              :label="t('settings.voice.downloadOf', { name: voice.name })"
              :loading="downloadingId === voice.modelId"
              :data-testid="`settings-voice-download-${voice.ref}`"
              @click.stop.prevent="
                downloadingId === null && (confirming = voice)
              "
            />
          </template>
        </SettingsOptionRow>
        <SettingsRow
          :title="t('settings.voice.clone.open')"
          icon="lucide:plus"
          navigates
          data-testid="settings-voice-clone"
          @select="cloneOpen = true"
        />
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

      <SettingsAssistantVoiceCloneDialog
        v-model:open="cloneOpen"
        :language="language"
        :rate="rate"
        @saved="clonedAsync"
      />

      <UiDrawerModal
        v-if="deleteCandidate"
        :open="deleteCandidate !== null"
        :title="t('settings.voice.deleteTitle')"
        @update:open="
          (isOpen) => (isOpen || deleting ? null : (deleteCandidate = null))
        "
      >
        <template #content>
          <div class="space-y-3 px-6 py-2">
            <p class="text-sm">
              {{
                t('settings.voice.deleteConfirm', {
                  name: deleteCandidate.name,
                })
              }}
            </p>
          </div>
        </template>
        <template #footer>
          <div class="flex justify-end gap-2">
            <UiButton
              type="button"
              variant="outline"
              :disabled="deleting"
              @click="deleteCandidate = null"
            >
              {{ t('settings.voice.clone.cancel') }}
            </UiButton>
            <UiButton
              type="button"
              variant="destructive"
              :loading="deleting"
              data-testid="settings-voice-delete-confirm"
              @click="confirmDeleteAsync"
            >
              {{ t('settings.voice.deleteButton') }}
            </UiButton>
          </div>
        </template>
      </UiDrawerModal>

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
