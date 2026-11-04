<script setup lang="ts">
/**
 * "Eigene Stimme anlegen" (spec 066 FR-049, FR-050, US8 scenarios 6 and 7, T149): the user records
 * 10–30 seconds or chooses an audio file, haex checks it and names what is wrong in plain words,
 * plays a sample, and keeps the voice with a name, a language and the confirmation that it is the
 * user's own voice or the speaker consented. Without that confirmation nothing is saved. The
 * microphone runs only while recording here; closing the dialog ends a recording. The language
 * comes first; for a recording the dialog offers a text in that language to read aloud, about
 * 20–30 seconds long, and another one on request (operator 2026-11-03). Failures and the
 * check's verdict show as toasts (operator 2026-11-04). A start, stop or file check that answers
 * after the dialog was closed or opened again is dropped, and a recording it started is stopped.
 */
import { toast } from 'vue-sonner'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  SPEECH_LANGUAGES,
  VOICE_FILE_EXTENSIONS,
  type SpeechLanguage,
} from '~/lib/assistant/settings'
import { nextReadAloud, READ_ALOUD_TEXTS } from '~/lib/assistant/readAloud'
import type {
  CloneCheck,
  CloneLevel,
  VoiceOption,
} from '~/composables/useTtsVoices'

const props = defineProps<{
  /** The language of conversations, preset for the new voice. */
  language: SpeechLanguage
  /** The speaking rate of this device, for the sample. */
  rate: number
}>()
const open = defineModel<boolean>('open', { default: false })
const emit = defineEmits<{ saved: [voice: VoiceOption] }>()

const { t, te, locale } = useI18n()
const { errString } = useErrorString()
const { pickOneAsync } = usePickedFile()
const {
  cloneRecordStartAsync,
  cloneRecordStopAsync,
  cloneFromFileAsync,
  clonePreviewAsync,
  cloneSaveAsync,
} = useTtsVoices()

const recordingId = ref<string | null>(null)
/** A recording is being started. */
const starting = ref(false)
const level = ref(0)
const heardSecs = ref(0)
const checking = ref(false)
const check = ref<CloneCheck | null>(null)
const previewing = ref(false)
const saving = ref(false)
const name = ref('')
const voiceLanguage = ref<SpeechLanguage>(props.language)
const consent = ref(false)
/** Counts openings and closings, so an answer for an earlier opening is recognised. */
let opening = 0
/** The text offered for reading aloud, an index into the texts of `voiceLanguage`. */
const textIndex = ref(nextReadAloud(props.language, null))
const readAloudText = computed(
  () => READ_ALOUD_TEXTS[voiceLanguage.value][textIndex.value] ?? '',
)
function otherText() {
  textIndex.value = nextReadAloud(voiceLanguage.value, textIndex.value)
}
// Another language offers a text of its own.
watch(voiceLanguage, (language) => {
  textIndex.value = nextReadAloud(language, null)
})

const canSave = computed(
  () =>
    check.value?.ok === true &&
    consent.value &&
    name.value.trim() !== '' &&
    !saving.value,
)

function languageName(language: SpeechLanguage): string {
  const label =
    new Intl.DisplayNames([locale.value], { type: 'language' }).of(language) ??
    language
  return label.charAt(0).toLocaleUpperCase(locale.value) + label.slice(1)
}

const languageOptions = computed(() =>
  SPEECH_LANGUAGES.map((value) => ({ value, label: languageName(value) })),
)

function seconds(value: number): string {
  return value.toLocaleString(locale.value, { maximumFractionDigits: 0 })
}

/** A backend refusal in words: the known reasons have a text, anything else its message. */
function failureText(error: unknown): string {
  const reason = (error as { reason?: unknown } | null)?.reason
  const key = `settings.voice.clone.errors.${String(reason)}`
  return typeof reason === 'string' && te(key) ? t(key) : errString(error)
}

function fail(error: unknown) {
  toast.error(failureText(error))
}

/** Takes the check of a recording or a file; its verdict is a toast, the preview stays here. */
function report(result: CloneCheck) {
  check.value = result
  if (result.ok) {
    toast.success(
      t('settings.voice.clone.ok', { seconds: seconds(result.speechSecs) }),
    )
    return
  }
  toast.error(
    [
      t('settings.voice.clone.again'),
      ...result.problems.map((problem) =>
        t(`settings.voice.clone.problems.${problem}`, {
          seconds: seconds(result.speechSecs),
        }),
      ),
    ].join(' '),
  )
}

let unlisten: UnlistenFn | null = null
let disposed = false
void listen<CloneLevel>('tts-clone-level', (event) => {
  if (event.payload.sessionId !== recordingId.value) return
  level.value = event.payload.level
  heardSecs.value = event.payload.speechSecs
}).then((stop) => {
  if (disposed) stop()
  else unlisten = stop
})

function reset() {
  recordingId.value = null
  level.value = 0
  heardSecs.value = 0
  check.value = null
  name.value = ''
  voiceLanguage.value = props.language
  textIndex.value = nextReadAloud(props.language, null)
  consent.value = false
  starting.value = false
  checking.value = false
}

/** Ends a recording nobody waits for any more. */
function stopQuietly(id: string) {
  void cloneRecordStopAsync(id).catch(() => {})
}

async function startAsync() {
  if (starting.value) return
  const current = opening
  starting.value = true
  check.value = null
  heardSecs.value = 0
  try {
    const id = await cloneRecordStartAsync()
    // Closed or opened again meanwhile: this recording belongs to nobody.
    if (disposed || current !== opening) stopQuietly(id)
    else recordingId.value = id
  } catch (error) {
    if (!disposed && current === opening) fail(error)
  } finally {
    if (current === opening) starting.value = false
  }
}

async function stopAsync() {
  const id = recordingId.value
  if (!id) return
  const current = opening
  recordingId.value = null
  level.value = 0
  checking.value = true
  try {
    const result = await cloneRecordStopAsync(id)
    if (current === opening) report(result)
  } catch (error) {
    if (current === opening) fail(error)
  } finally {
    if (current === opening) checking.value = false
  }
}

async function chooseFileAsync() {
  const current = opening
  let file: Awaited<ReturnType<typeof pickOneAsync>>
  try {
    file = await pickOneAsync([
      {
        name: t('settings.voice.clone.fileTypes'),
        extensions: [...VOICE_FILE_EXTENSIONS],
      },
    ])
  } catch (error) {
    if (current === opening) fail(error)
    return
  }
  if (!file || current !== opening) return
  if (recordingId.value) await stopAsync()
  checking.value = true
  check.value = null
  try {
    const result = await cloneFromFileAsync(file)
    if (current === opening) report(result)
  } catch (error) {
    if (current === opening) fail(error)
  } finally {
    if (current === opening) checking.value = false
  }
}

async function previewAsync() {
  if (!check.value) return
  previewing.value = true
  try {
    await clonePreviewAsync(
      check.value.sessionId,
      voiceLanguage.value,
      props.rate,
    )
  } catch (error) {
    toast.error(
      t('settings.voice.previewFailed', { reason: failureText(error) }),
    )
  } finally {
    previewing.value = false
  }
}

async function saveAsync() {
  if (!check.value || !canSave.value) return
  saving.value = true
  try {
    const voice = await cloneSaveAsync(
      check.value.sessionId,
      name.value.trim(),
      voiceLanguage.value,
      consent.value,
    )
    emit('saved', voice)
    open.value = false
  } catch (error) {
    fail(error)
  } finally {
    saving.value = false
  }
}

// Closing ends a recording that still runs; opening starts from scratch.
watch(open, (isOpen) => {
  opening += 1
  if (isOpen) {
    reset()
    return
  }
  const id = recordingId.value
  recordingId.value = null
  if (id) stopQuietly(id)
})
onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
  const id = recordingId.value
  if (id) stopQuietly(id)
})
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('settings.voice.clone.title')"
    :description="t('settings.voice.clone.description')"
  >
    <template #content>
      <div class="flex flex-col gap-4 px-6 py-2" data-testid="voice-clone">
        <label class="flex flex-col gap-1 text-sm" for="voice-clone-language">
          {{ t('settings.voice.clone.language') }}
          <SettingsSelect
            id="voice-clone-language"
            v-model="voiceLanguage"
            class="w-full"
            :options="languageOptions"
            data-testid="voice-clone-language"
          />
        </label>

        <figure
          class="flex flex-col gap-2 rounded-lg border bg-muted/40 p-4"
          data-testid="voice-clone-text"
        >
          <figcaption
            class="flex items-center justify-between gap-2 text-xs text-muted-foreground"
          >
            <span>{{ t('settings.voice.clone.readAloud') }}</span>
            <UiButton
              type="button"
              variant="ghost"
              size="sm"
              :disabled="recordingId !== null"
              data-testid="voice-clone-other-text"
              @click="otherText"
            >
              <Icon name="lucide:shuffle" class="h-4 w-4" :aria-hidden="true" />
              {{ t('settings.voice.clone.otherText') }}
            </UiButton>
          </figcaption>
          <blockquote class="text-base leading-7" :lang="voiceLanguage">
            {{ readAloudText }}
          </blockquote>
        </figure>

        <div class="flex flex-wrap gap-2">
          <UiButton
            v-if="recordingId === null"
            type="button"
            variant="outline"
            :disabled="checking"
            :loading="starting"
            data-testid="voice-clone-record"
            @click="startAsync"
          >
            <Icon name="lucide:mic" class="h-4 w-4" :aria-hidden="true" />
            {{ t('settings.voice.clone.record') }}
          </UiButton>
          <UiButton
            v-else
            type="button"
            data-testid="voice-clone-stop"
            @click="stopAsync"
          >
            <Icon name="lucide:square" class="h-4 w-4" :aria-hidden="true" />
            {{ t('settings.voice.clone.stop') }}
          </UiButton>
          <UiButton
            type="button"
            variant="outline"
            :disabled="checking || starting || recordingId !== null"
            data-testid="voice-clone-file"
            @click="chooseFileAsync"
          >
            <Icon
              name="lucide:file-audio"
              class="h-4 w-4"
              :aria-hidden="true"
            />
            {{ t('settings.voice.clone.chooseFile') }}
          </UiButton>
        </div>

        <p class="text-xs text-muted-foreground">
          {{ t('settings.voice.clone.recordHint') }}
        </p>

        <div
          v-if="recordingId !== null"
          class="flex flex-col gap-1"
          data-testid="voice-clone-level"
        >
          <UiProgress
            :value="Math.round(level * 100)"
            :label="t('settings.voice.clone.level')"
          />
          <p class="text-xs text-muted-foreground" aria-live="polite">
            {{
              t('settings.voice.clone.heard', { seconds: seconds(heardSecs) })
            }}
          </p>
        </div>

        <p v-if="checking" class="text-sm text-muted-foreground">
          {{ t('settings.voice.clone.checking') }}
        </p>

        <div
          v-if="check"
          class="flex flex-col gap-2"
          data-testid="voice-clone-check"
        >
          <div>
            <UiButton
              type="button"
              variant="outline"
              size="sm"
              :loading="previewing"
              :disabled="check.speechSecs === 0"
              data-testid="voice-clone-preview"
              @click="previewAsync"
            >
              {{ t('settings.voice.preview') }}
            </UiButton>
          </div>
        </div>

        <label class="flex flex-col gap-1 text-sm" for="voice-clone-name">
          {{ t('settings.voice.clone.name') }}
          <UiInput
            id="voice-clone-name"
            v-model="name"
            :maxlength="80"
            data-testid="voice-clone-name"
          />
        </label>
        <label class="flex items-start gap-2 text-sm">
          <ShadcnCheckbox v-model="consent" data-testid="voice-clone-consent" />
          {{ t('settings.voice.clone.consent') }}
        </label>
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
          data-testid="voice-clone-save"
          @click="saveAsync"
        >
          {{ t('settings.voice.clone.save') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
