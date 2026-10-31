<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/**
 * Voice dictation controls (spec 008-voice-control-stt, T014).
 *
 * A toggle whose icon-only buttons follow the recording state:
 * - idle/error: the auto-send switch and the mic button, which starts a
 *   recording;
 * - recording: only discard (a bin icon: nothing is transcribed or sent) and
 *   finish, which says "Senden"/"Einfügen" in words (ends the recording and
 *   transcribes it; the text is sent when auto-send is on, otherwise it is
 *   placed in the input field);
 * - transcribing: a spinner.
 * `recording` tells the composer to hide its own send button meanwhile,
 * because the send button here takes its place.
 *
 * US2 (the "stop"/"halt"/"abbrechen" interrupt fast path cancelling an
 * in-progress assistant turn) and US3 (choosing an external transcription
 * provider) are not implemented yet — `interrupt` on the result is only
 * used here to avoid writing/sending the bare interrupt word as a chat
 * message; nothing actually cancels the assistant turn (T019/T020), and
 * there is no persistent "external source" indicator yet (T027).
 */

type RecordingState = 'idle' | 'recording' | 'transcribing' | 'error'

type TranscriptionResult = {
  text: string
  interrupt: 'stop' | 'halt' | 'abbrechen' | null
}

const AUTO_SEND_PREF_KEY = 'voice.auto_send'

const emit = defineEmits<{
  transcript: [text: string, autoSend: boolean]
}>()

const recording = defineModel<boolean>('recording', { default: false })

const { t } = useI18n()
const { getPrefAsync } = usePreferences()

const state = ref<RecordingState>('idle')
const autoSend = ref(true)
const errorMessage = ref<string | null>(null)
const noSpeechDetected = ref(false)

const finishLabel = computed(() =>
  autoSend.value
    ? t('voiceControl.mic.finishAndSend')
    : t('voiceControl.mic.finishAndInsert'),
)
/** The visible text of the finish button, so it is not mistaken for discarding. */
const finishShort = computed(() =>
  autoSend.value ? t('voiceControl.mic.send') : t('voiceControl.mic.insert'),
)

watch(state, (value) => (recording.value = value === 'recording'), {
  immediate: true,
})

let unlistenCapped: (() => void) | null = null
let disposed = false
let startPending = false

async function loadAutoSendAsync() {
  try {
    const stored = await getPrefAsync({ kind: 'vault' }, AUTO_SEND_PREF_KEY)
    // No stored value yet -> the documented default (FR-005: on).
    if (!disposed) autoSend.value = stored === null ? true : stored === 'true'
  } catch {
    // A failed preference read must not block dictation — keep the shown value.
  }
}
onVaultTablesChanged(['preferences'], loadAutoSendAsync)

onMounted(async () => {
  await loadAutoSendAsync()
  const unlisten = await listen<TranscriptionResult | null>(
    'voice-recording-capped',
    ({ payload }) => {
      if (disposed || state.value !== 'recording') return
      void finishRecording(payload)
    },
  )
  if (disposed) unlisten()
  else unlistenCapped = unlisten
})

onBeforeUnmount(() => {
  disposed = true
  unlistenCapped?.()
  unlistenCapped = null
  if (startPending || state.value === 'recording') {
    void invoke('cancel_voice_recording').catch(() => {
      // The component is already going away; cancellation is best-effort.
    })
  }
})

// Spec 020 FR-024: persisting the choice runs the `chat.voice.setAutoSend` action. Best-effort:
// the toggle still reflects the user's choice for this session even if persisting it failed.
const setAutoSend = useAction('chat.voice.setAutoSend')
async function toggleAutoSend() {
  autoSend.value = !autoSend.value
  await setAutoSend({ enabled: autoSend.value })
}

function structuredVoiceError(e: unknown): { kind?: unknown } | null {
  if (e && typeof e === 'object' && 'kind' in e) {
    return e as { kind?: unknown }
  }
  if (typeof e !== 'string') return null
  try {
    const parsed: unknown = JSON.parse(e)
    if (parsed && typeof parsed === 'object' && 'kind' in parsed) {
      return parsed as { kind?: unknown }
    }
  } catch {
    // Native bridge may return a plain string; falls through to the
    // generic message below.
  }
  return null
}

function describeError(e: unknown): string {
  switch (structuredVoiceError(e)?.kind) {
    case 'DeviceUnavailable':
      return t('voiceControl.error.deviceUnavailable')
    case 'MicrophoneDenied':
      return t('voiceControl.error.microphoneDenied')
    case 'AlreadyRecording':
      return t('voiceControl.error.alreadyRecording')
    case 'TranscriptionFailed':
      return t('voiceControl.error.transcriptionFailed')
    default:
      return t('voiceControl.error.generic')
  }
}

async function startRecording() {
  if (
    disposed ||
    startPending ||
    (state.value !== 'idle' && state.value !== 'error')
  )
    return
  errorMessage.value = null
  noSpeechDetected.value = false
  startPending = true
  try {
    await invoke('start_voice_recording')
    if (disposed) {
      await invoke('cancel_voice_recording').catch(() => {
        // onBeforeUnmount also cancels; this covers a late start completion.
      })
      return
    }
    state.value = 'recording'
  } catch (e) {
    if (!disposed) {
      state.value = 'error'
      errorMessage.value = describeError(e)
    }
  } finally {
    startPending = false
  }
}

async function finishRecording(result?: TranscriptionResult | null) {
  if (disposed) return
  state.value = 'transcribing'
  try {
    const transcription =
      result === undefined
        ? await invoke<TranscriptionResult>('stop_voice_recording')
        : result
    if (disposed) return
    if (transcription === null) {
      state.value = 'error'
      errorMessage.value = t('voiceControl.error.generic')
      return
    }
    state.value = 'idle'
    if (transcription.interrupt) return
    if (!transcription.text.trim()) {
      noSpeechDetected.value = true
      return
    }
    emit('transcript', transcription.text, autoSend.value)
  } catch (e) {
    if (disposed) return
    state.value = 'error'
    errorMessage.value = describeError(e)
  }
}

async function cancelRecording() {
  try {
    await invoke('cancel_voice_recording')
  } finally {
    state.value = 'idle'
  }
}
</script>

<template>
  <div class="flex shrink-0 items-center gap-1.5">
    <template v-if="state === 'recording'">
      <span
        role="status"
        class="mr-0.5 flex h-2 w-2 shrink-0 animate-pulse rounded-full bg-destructive"
      >
        <span class="sr-only">{{ t('voiceControl.recording') }}</span>
      </span>
      <UiButton
        type="button"
        size="icon-sm"
        variant="ghost"
        :aria-label="t('voiceControl.mic.cancel')"
        :title="t('voiceControl.mic.cancel')"
        data-testid="voice-cancel"
        @click="cancelRecording"
      >
        <Icon name="lucide:trash-2" class="h-3.5 w-3.5" />
      </UiButton>
      <UiButton
        type="button"
        size="sm"
        class="h-7 gap-1 px-2.5"
        :aria-label="finishLabel"
        :title="finishLabel"
        data-testid="voice-finish"
        @click="finishRecording()"
      >
        <Icon name="lucide:check" class="h-3.5 w-3.5" />
        {{ finishShort }}
      </UiButton>
    </template>
    <template v-else>
      <button
        type="button"
        class="flex h-6 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-muted/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
        :class="{ 'text-muted-foreground/50': autoSend }"
        :aria-pressed="autoSend"
        :aria-label="
          autoSend
            ? t('voiceControl.autoSend.onLabel')
            : t('voiceControl.autoSend.offLabel')
        "
        :title="
          autoSend
            ? t('voiceControl.autoSend.onLabel')
            : t('voiceControl.autoSend.offLabel')
        "
        @click="toggleAutoSend"
      >
        <Icon
          :name="autoSend ? 'lucide:zap' : 'lucide:zap-off'"
          class="h-3.5 w-3.5"
        />
      </button>
      <UiButton
        type="button"
        size="icon-sm"
        variant="secondary"
        :disabled="state === 'transcribing'"
        :aria-label="t('voiceControl.mic.start')"
        :title="t('voiceControl.mic.start')"
        data-testid="voice-start"
        @click="startRecording"
      >
        <Icon
          :name="state === 'transcribing' ? 'lucide:loader-2' : 'lucide:mic'"
          class="h-3.5 w-3.5"
          :class="{ 'animate-spin': state === 'transcribing' }"
        />
      </UiButton>
    </template>
    <span
      v-if="noSpeechDetected"
      class="text-xs text-muted-foreground"
      role="status"
    >
      {{ t('voiceControl.noSpeechDetected') }}
    </span>
    <span
      v-if="errorMessage"
      class="text-xs text-destructive"
      role="alert"
      data-testid="voice-error"
    >
      {{ errorMessage }}
    </span>
  </div>
</template>
