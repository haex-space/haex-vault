<script setup lang="ts">
/**
 * "Person hinzufügen" (spec 066 FR-016a, operator 2026-11-07): a name, then three short passages
 * read aloud one after the other. Each recording runs on the microphone only while it is taken
 * (the own-voice sessions), becomes a voice print and is thrown away; only the mean print is kept,
 * synced in the vault. The speaker model (about 30 MB) is downloaded first if it is missing.
 * Closing the dialog drops what was recorded. Errors come as toasts.
 */
import { toast } from 'vue-sonner'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SpeechLanguage } from '~/lib/assistant/settings'
import { enrolmentSentences } from '~/lib/assistant/readAloud'
import {
  ENROL_SENTENCES,
  SPEAKER_MODEL_ID,
  type EnrolledSpeaker,
} from '~/composables/useAssistantSpeakers'
import type { CloneLevel } from '~/composables/useTtsVoices'

const props = defineProps<{ language: SpeechLanguage }>()
const open = defineModel<boolean>('open', { default: false })
const emit = defineEmits<{ saved: [speaker: EnrolledSpeaker] }>()

const { t, te } = useI18n()
const { errString } = useErrorString()
const { cloneRecordStartAsync, cloneRecordStopAsync, cloneRecordCancelAsync } =
  useTtsVoices()
const {
  modelAsync,
  downloadModelAsync,
  addSentenceAsync,
  resetEnrolmentAsync,
  saveAsync,
} = useAssistantSpeakers()

const name = ref('')
const modelInstalled = ref<boolean | null>(null)
const modelBytes = ref(0)
const downloading = ref(false)
const downloaded = ref(0)
const taken = ref(0)
const recordingId = ref<string | null>(null)
const enrolmentGeneration = ref<number | null>(null)
const starting = ref(false)
const working = ref(false)
const level = ref(0)
const saving = ref(false)
/** Counts openings and closings, so an answer for an earlier opening is recognised. */
let opening = 0
let enrolmentRun = 0
let recordingGeneration: number | null = null

function cancelRecordingAsync(id: string) {
  void cloneRecordCancelAsync(id).catch(() => {})
}

const sentences = computed(() =>
  enrolmentSentences(props.language, ENROL_SENTENCES),
)
const current = computed(() => sentences.value[taken.value] ?? '')
const done = computed(() => taken.value >= ENROL_SENTENCES)
const canSave = computed(
  () => done.value && name.value.trim() !== '' && !saving.value,
)
const downloadPercent = computed(() =>
  modelBytes.value > 0
    ? Math.round((downloaded.value / modelBytes.value) * 100)
    : 0,
)

function fail(error: unknown) {
  const reason = (error as { reason?: unknown } | null)?.reason
  const key = `settings.assistantPage.speakers.errors.${String(reason)}`
  toast.error(typeof reason === 'string' && te(key) ? t(key) : errString(error))
}

let unlistenLevel: UnlistenFn | null = null
let unlistenProgress: UnlistenFn | null = null
let disposed = false
void listen<CloneLevel>('tts-clone-level', (event) => {
  if (event.payload.sessionId === recordingId.value)
    level.value = event.payload.level
}).then((stop) => (disposed ? stop() : (unlistenLevel = stop)))
void listen<{ modelId: string; bytesDownloaded: number }>(
  'model-download-progress',
  (event) => {
    if (event.payload.modelId === SPEAKER_MODEL_ID)
      downloaded.value = event.payload.bytesDownloaded
  },
).then((stop) => (disposed ? stop() : (unlistenProgress = stop)))

async function checkModelAsync() {
  try {
    const model = await modelAsync()
    modelInstalled.value = model.installed
    modelBytes.value = model.sizeBytes
  } catch (error) {
    fail(error)
  }
}

async function downloadAsync() {
  downloading.value = true
  downloaded.value = 0
  try {
    await downloadModelAsync()
    modelInstalled.value = true
  } catch (error) {
    fail(error)
  } finally {
    downloading.value = false
  }
}

async function startAsync() {
  if (starting.value || enrolmentGeneration.value === null) return
  const at = opening
  const run = enrolmentRun
  const generation = enrolmentGeneration.value
  starting.value = true
  try {
    const id = await cloneRecordStartAsync()
    if (disposed || at !== opening || run !== enrolmentRun)
      cancelRecordingAsync(id)
    else {
      recordingId.value = id
      recordingGeneration = generation
    }
  } catch (error) {
    if (at === opening) fail(error)
  } finally {
    if (at === opening) starting.value = false
  }
}

async function stopAsync() {
  const id = recordingId.value
  if (!id) return
  const at = opening
  const run = enrolmentRun
  const generation = recordingGeneration
  recordingId.value = null
  recordingGeneration = null
  level.value = 0
  working.value = true
  try {
    await cloneRecordStopAsync(id)
    if (at !== opening || run !== enrolmentRun) {
      await cloneRecordCancelAsync(id).catch(() => {})
      return
    }
    if (generation === null) {
      await cloneRecordCancelAsync(id).catch(() => {})
      return
    }
    const count = await addSentenceAsync(id, generation)
    if (at === opening && run === enrolmentRun) taken.value = count
  } catch (error) {
    if (at === opening && run === enrolmentRun) fail(error)
  } finally {
    if (at === opening) working.value = false
  }
}

async function startOverAsync() {
  enrolmentRun += 1
  const at = opening
  const run = enrolmentRun
  enrolmentGeneration.value = null
  taken.value = 0
  try {
    const generation = await resetEnrolmentAsync()
    if (at === opening && run === enrolmentRun)
      enrolmentGeneration.value = generation
  } catch (error) {
    if (at === opening && run === enrolmentRun) fail(error)
  }
}

async function saveAsync_() {
  if (!canSave.value) return
  const at = opening
  saving.value = true
  try {
    const speaker = await saveAsync(name.value.trim())
    emit('saved', speaker)
    if (at === opening) open.value = false
  } catch (error) {
    if (at === opening) fail(error)
    // Sentences that disagree are gone; reading starts over.
    if (at === opening) taken.value = 0
  } finally {
    if (at === opening) saving.value = false
  }
}

watch(open, (isOpen) => {
  opening += 1
  enrolmentRun += 1
  const id = recordingId.value
  recordingId.value = null
  recordingGeneration = null
  if (id) cancelRecordingAsync(id)
  name.value = ''
  taken.value = 0
  level.value = 0
  starting.value = false
  working.value = false
  saving.value = false
  enrolmentGeneration.value = null
  const at = opening
  const run = enrolmentRun
  void resetEnrolmentAsync()
    .then((generation) => {
      if (!disposed && at === opening && run === enrolmentRun)
        enrolmentGeneration.value = generation
    })
    .catch(() => {})
  if (isOpen) void checkModelAsync()
})
onBeforeUnmount(() => {
  disposed = true
  unlistenLevel?.()
  unlistenProgress?.()
  const id = recordingId.value
  if (id) cancelRecordingAsync(id)
  recordingGeneration = null
  enrolmentGeneration.value = null
})
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('settings.assistantPage.speakers.dialogTitle')"
    :description="t('settings.assistantPage.speakers.dialogDescription')"
  >
    <template #content>
      <div class="flex flex-col gap-4 px-6 py-2" data-testid="speaker-enrol">
        <label class="flex flex-col gap-1 text-sm" for="speaker-enrol-name">
          {{ t('settings.assistantPage.speakers.name') }}
          <UiInput
            id="speaker-enrol-name"
            v-model="name"
            :maxlength="80"
            data-testid="speaker-enrol-name"
          />
        </label>

        <div
          v-if="modelInstalled === false"
          class="flex flex-col gap-2"
          data-testid="speaker-enrol-model"
        >
          <p class="text-sm text-muted-foreground">
            {{ t('settings.assistantPage.speakers.modelMissing') }}
          </p>
          <div>
            <UiButton
              type="button"
              variant="outline"
              size="sm"
              :loading="downloading"
              data-testid="speaker-enrol-download"
              @click="downloadAsync"
            >
              <Icon name="lucide:download" class="size-4" aria-hidden="true" />
              {{ t('settings.assistantPage.speakers.download') }}
            </UiButton>
          </div>
          <UiProgress
            v-if="downloading"
            :value="downloadPercent"
            :label="t('settings.assistantPage.speakers.downloading')"
          />
        </div>

        <template v-else-if="modelInstalled">
          <p class="text-sm text-muted-foreground" aria-live="polite">
            {{
              t('settings.assistantPage.speakers.progress', {
                taken: Math.min(taken, ENROL_SENTENCES),
                total: ENROL_SENTENCES,
              })
            }}
          </p>
          <figure
            v-if="!done"
            class="flex flex-col gap-2 rounded-lg border bg-muted/40 p-4"
            data-testid="speaker-enrol-text"
          >
            <figcaption class="text-xs text-muted-foreground">
              {{ t('settings.assistantPage.speakers.readAloud') }}
            </figcaption>
            <blockquote class="text-base leading-7" :lang="language">
              {{ current }}
            </blockquote>
          </figure>
          <div class="flex flex-wrap gap-2">
            <template v-if="!done">
              <UiButton
                v-if="recordingId === null"
                type="button"
                variant="outline"
                :disabled="enrolmentGeneration === null"
                :loading="starting || working"
                data-testid="speaker-enrol-record"
                @click="startAsync"
              >
                <Icon name="lucide:mic" class="size-4" aria-hidden="true" />
                {{ t('settings.assistantPage.speakers.record') }}
              </UiButton>
              <UiButton
                v-else
                type="button"
                data-testid="speaker-enrol-stop"
                @click="stopAsync"
              >
                <Icon name="lucide:square" class="size-4" aria-hidden="true" />
                {{ t('settings.assistantPage.speakers.stop') }}
              </UiButton>
            </template>
            <UiButton
              v-if="taken > 0 && recordingId === null"
              type="button"
              variant="ghost"
              data-testid="speaker-enrol-again"
              @click="startOverAsync"
            >
              {{ t('settings.assistantPage.speakers.startOver') }}
            </UiButton>
          </div>
          <UiProgress
            v-if="recordingId !== null"
            :value="Math.round(level * 100)"
            :label="t('settings.voice.clone.level')"
          />
        </template>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton type="button" variant="outline" @click="open = false">
          {{ t('settings.voice.clone.cancel') }}
        </UiButton>
        <UiButton
          type="button"
          :disabled="!canSave"
          :loading="saving"
          data-testid="speaker-enrol-save"
          @click="saveAsync_"
        >
          {{ t('settings.assistantPage.speakers.save') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
