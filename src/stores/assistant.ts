import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useDevice } from '~/composables/useDevice'
import { usePreferences, type PrefScope } from '~/composables/usePreferences'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { parsePreview, PREVIEW_KEY } from '~/lib/assistant/settings'
import {
  ASSISTANT_OFF,
  utcOffsetMinutes,
  type AssistantStateWire,
} from '~/lib/assistant/wire'

export type { AssistantPhase, AssistantStateWire } from '~/lib/assistant/wire'

/**
 * The voice assistant in the window (spec 066 T046): its state from `assistant-state`, the last
 * error, and the toggle (operator 2026-11-02: one on/off toggle in dock and launcher; a spoken
 * conversation is a normal chat session, shown by the chat itself). The preview switch
 * (`assistant.preview`, T045) decides whether the toggle is there until the wake word arrives.
 *
 * ponytail: the event subscriptions live until the process ends, which is the end of the vault
 * session (one vault per process, spec 013).
 */
export const useAssistantStore = defineStore('assistant', () => {
  const { getPrefAsync, setPrefAsync } = usePreferences()
  const { currentDeviceInfoAsync } = useDevice()

  const state = ref<AssistantStateWire>(ASSISTANT_OFF)
  const preview = ref(false)
  /** The last failure, raw: a backend error of a command or the message of `assistant-error`.
   * A new object each time, so the same message twice is shown twice. */
  const lastError = ref<{ error: unknown } | null>(null)
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

  async function loadPreviewAsync(): Promise<void> {
    preview.value = parsePreview(
      await getPrefAsync(await deviceScopeAsync(), PREVIEW_KEY),
    )
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
      ])
      onVaultTablesChanged(['preferences'], () => loadPreviewAsync())
      await loadPreviewAsync()
      try {
        state.value = await invoke<AssistantStateWire>('assistant_get_state')
      } catch {
        // Builds without `voice`: the assistant stays off.
        state.value = ASSISTANT_OFF
      }
    })()
    return started
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

  async function setPreviewAsync(on: boolean): Promise<void> {
    // action-exempt: the preview switch is the user's own choice on the settings page until the wake word (T045); no agent may turn on a microphone path.
    await setPrefAsync(await deviceScopeAsync(), PREVIEW_KEY, String(on))
    preview.value = on
  }

  return {
    state,
    preview,
    lastError,
    starting,
    start,
    setEnabledAsync,
    setPreviewAsync,
  }
})
