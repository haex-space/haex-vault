import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useDevice } from '~/composables/useDevice'
import { usePreferences, type PrefScope } from '~/composables/usePreferences'
import { restoreWanted, type AssistantMissing } from '~/lib/assistant/indicator'
import {
  ENABLED_KEY,
  MUTED_KEY,
  NOTICE_SHOWN_KEY,
} from '~/lib/assistant/settings'
import type {
  SpeechModelFallback,
  SpeechModelHint,
} from '~/lib/assistant/speechModel'
import {
  ASSISTANT_OFF,
  utcOffsetMinutes,
  type AssistantStateWire,
} from '~/lib/assistant/wire'

export type { AssistantPhase, AssistantStateWire } from '~/lib/assistant/wire'

/**
 * The voice assistant in the window (spec 066 T046): its state from `assistant-state`, the last
 * error, and the toggle (operator 2026-11-02: one on/off toggle in dock and launcher; a spoken
 * conversation is a normal chat session, shown by the chat itself). After unlock it switches the
 * assistant on again when it was on (`assistant.enabled`, FR-002), muted when it was muted; the
 * backend stores both on every switch. Before the first switching on, the notice of FR-037
 * (`assistant.noticeShown`) asks to be confirmed.
 *
 * ponytail: the event subscriptions live until the process ends, which is the end of the vault
 * session (one vault per process, spec 013).
 */
export const useAssistantStore = defineStore('assistant', () => {
  const { getPrefAsync, setPrefAsync } = usePreferences()
  const { currentDeviceInfoAsync } = useDevice()

  const nuxtApp = useNuxtApp()
  const state = ref<AssistantStateWire>(ASSISTANT_OFF)
  /** Whether this build has the voice assistant; the toggle shows only then. Known once `start`
   * read the state. */
  const available = ref(false)
  /** Set while the notice of FR-037 waits for an answer; called with it. */
  const notice = ref<((confirmed: boolean) => void) | null>(null)
  /** The last failure, raw: a backend error of a command or the message of `assistant-error`.
   * A new object each time, so the same message twice is shown twice. */
  const lastError = ref<{ error: unknown } | null>(null)
  /** The last offer of a faster speech model (spec 081 FR-011, `tts-speech-model-hint`). */
  const speechModelHint = ref<SpeechModelHint | null>(null)
  /** The last time Pocket spoke instead of the chosen model (`tts-speech-model-fallback`); a
   * new object each time. */
  const speechModelFallback = ref<SpeechModelFallback | null>(null)
  /** Set while switching on prepares the models and opens the microphone. */
  const starting = ref(false)
  let started: Promise<void> | null = null
  let scope: PrefScope | null = null

  async function deviceScopeAsync(): Promise<PrefScope> {
    scope ??= {
      kind: 'device',
      uuid: (await currentDeviceInfoAsync()).vaultDeviceUuid,
    }
    return scope
  }

  /** Subscribes and reads the state once. Idempotent. */
  function start(): Promise<void> {
    started ??= (async () => {
      await Promise.all([
        listen<AssistantStateWire>('assistant-state', (event) => {
          state.value = event.payload
        }),
        listen<{ kind: string; message: string }>(
          'assistant-error',
          (event) => {
            lastError.value = { error: event.payload.message }
          },
        ),
        listen<SpeechModelHint>('tts-speech-model-hint', (event) => {
          speechModelHint.value = event.payload
        }),
        listen<SpeechModelFallback>('tts-speech-model-fallback', (event) => {
          speechModelFallback.value = event.payload
        }),
      ])
      try {
        state.value = await invoke<AssistantStateWire>('assistant_get_state')
        available.value = true
      } catch {
        // Builds without `voice`: the assistant stays off and shows no toggle.
        state.value = ASSISTANT_OFF
        return
      }
      await restoreAsync(String(nuxtApp.$i18n.locale.value))
    })()
    return started
  }

  /** Switches the assistant on again as the vault was left (FR-002). Quiet when something is
   * missing now: the toggle shows it is off and a click leads to what is missing. */
  async function restoreAsync(language: string): Promise<void> {
    try {
      const scope = await deviceScopeAsync()
      const [enabled, muted] = await Promise.all([
        getPrefAsync(scope, ENABLED_KEY),
        getPrefAsync(scope, MUTED_KEY),
      ])
      if (!restoreWanted(enabled, state.value.enabled)) return
      if ((await missingAsync(language)).length > 0) return
      await setEnabledAsync(true, language)
      if (muted === 'true')
        state.value = await invoke<AssistantStateWire>('assistant_set_muted', {
          muted: true,
        })
    } catch (error) {
      lastError.value = { error }
    }
  }

  /** What switching on still needs on this device (FR-005); empty when it is ready. */
  async function missingAsync(language: string): Promise<AssistantMissing[]> {
    const readiness = await invoke<{ missing: AssistantMissing[] }>(
      'assistant_readiness',
      { language },
    )
    return readiness.missing
  }

  /** `true` once the notice of FR-037 was confirmed on this device, now or before. */
  async function confirmNoticeAsync(): Promise<boolean> {
    const scope = await deviceScopeAsync()
    if ((await getPrefAsync(scope, NOTICE_SHOWN_KEY)) === 'true') return true
    const confirmed = await new Promise<boolean>((resolve) => {
      notice.value = resolve
    })
    notice.value = null
    if (confirmed)
      // action-exempt: the user confirmed the one-time notice of switching the assistant on (FR-037); spec 066 gives agents no action for it.
      await setPrefAsync(scope, NOTICE_SHOWN_KEY, 'true')
    return confirmed
  }

  /** The answer of the notice dialog. */
  function answerNotice(confirmed: boolean): void {
    notice.value?.(confirmed)
  }

  /** The toggle (FR-001); `language` is the window's language, the language of a conversation. */
  async function setEnabledAsync(
    enabled: boolean,
    language: string,
  ): Promise<void> {
    lastError.value = null
    starting.value = enabled
    try {
      state.value = await invoke<AssistantStateWire>('assistant_set_enabled', {
        enabled,
        language,
        utcOffsetMinutes: utcOffsetMinutes(),
      })
    } catch (error) {
      lastError.value = { error }
      throw error
    } finally {
      starting.value = false
    }
  }

  /** Writes a device preference of the assistant, such as the answer to the hint. */
  async function setDevicePrefAsync(key: string, value: string): Promise<void> {
    // action-exempt: the answer to a hint about this device's own speech model; spec 081 gives agents no action for it.
    await setPrefAsync(await deviceScopeAsync(), key, value)
  }

  return {
    state,
    available,
    notice,
    lastError,
    speechModelHint,
    speechModelFallback,
    setDevicePrefAsync,
    starting,
    start,
    setEnabledAsync,
    missingAsync,
    confirmNoticeAsync,
    answerNotice,
  }
})
