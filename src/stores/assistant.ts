import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { useDevice } from '~/composables/useDevice'
import { usePreferences, type PrefScope } from '~/composables/usePreferences'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { parsePreview, PREVIEW_KEY } from '~/lib/assistant/settings'
import {
  ASSISTANT_OFF,
  sortEntries,
  utcOffsetMinutes,
  type AssistantStateWire,
  type TranscriptEntry,
} from '~/lib/assistant/wire'

export type {
  AssistantPhase,
  AssistantStateWire,
  TranscriptEntry,
} from '~/lib/assistant/wire'

/**
 * The voice assistant in the window (spec 066 T046): its state from `assistant-state`, the
 * transcript of the current conversation from `assistant-transcript`, the last error, and the
 * commands. The preview switch (`assistant.preview`, T045) decides whether the indicator and the
 * talk shortcut are there until the wake word arrives.
 *
 * ponytail: the event subscriptions live until the process ends, which is the end of the vault
 * session (one vault per process, spec 013).
 */
export const useAssistantStore = defineStore('assistant', () => {
  const { getPrefAsync, setPrefAsync } = usePreferences()
  const { currentDeviceInfoAsync } = useDevice()

  const state = ref<AssistantStateWire>(ASSISTANT_OFF)
  const preview = ref(false)
  /** The transcript of the conversation shown: the running one, or the last one. */
  const entries = ref<TranscriptEntry[]>([])
  const entriesThread = ref<string | null>(null)
  /** The last failure, raw: a backend error of a command or the message of `assistant-error`;
   * the window shows it through `useErrorString`. */
  const lastError = ref<unknown>(null)
  /** Set while "Sprechen" prepares the models and opens the microphone. */
  const starting = ref(false)
  let started: Promise<void> | null = null
  let scope: PrefScope | null = null

  const inConversation = computed(
    () => state.value.phase !== 'off' && state.value.phase !== 'ready',
  )

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

  async function showThreadAsync(threadId: string): Promise<void> {
    entriesThread.value = threadId
    const stored = await invoke<TranscriptEntry[]>(
      'assistant_transcript_list',
      { threadId },
    )
    if (entriesThread.value !== threadId) return
    const known = new Set(stored.map((e) => e.id))
    // Entries that arrived while the list was read stay.
    const live = entries.value.filter(
      (e) => e.threadId === threadId && !known.has(e.id),
    )
    entries.value = sortEntries([...stored, ...live])
  }

  function applyState(next: AssistantStateWire): void {
    state.value = next
    if (next.threadId && next.threadId !== entriesThread.value) {
      void showThreadAsync(next.threadId).catch((error: unknown) => {
        console.error('[assistant] transcript', error)
      })
    }
  }

  function addEntry(entry: TranscriptEntry): void {
    if (entriesThread.value !== entry.threadId) {
      // The first entry of a new conversation names its thread.
      entriesThread.value = entry.threadId
      entries.value = []
    }
    if (entries.value.some((e) => e.id === entry.id)) return
    entries.value = sortEntries([...entries.value, entry])
  }

  /** Subscribes and reads the state once. Idempotent. */
  function start(): Promise<void> {
    started ??= (async () => {
      await Promise.all([
        listen<AssistantStateWire>('assistant-state', (event) =>
          applyState(event.payload),
        ),
        listen<TranscriptEntry>('assistant-transcript', (event) =>
          addEntry(event.payload),
        ),
        listen<{ kind: string; message: string }>(
          'assistant-error',
          (event) => {
            lastError.value = event.payload.message
          },
        ),
      ])
      onVaultTablesChanged(['preferences'], () => loadPreviewAsync())
      await loadPreviewAsync()
      try {
        applyState(await invoke<AssistantStateWire>('assistant_get_state'))
      } catch {
        // Builds without `voice`: the assistant stays off.
        state.value = ASSISTANT_OFF
      }
    })()
    return started
  }

  /** "Sprechen" (FR-011); `language` is the window's language. */
  async function talkAsync(language: string): Promise<void> {
    lastError.value = null
    starting.value = true
    try {
      await invoke('assistant_talk', {
        language,
        utcOffsetMinutes: utcOffsetMinutes(),
      })
    } catch (error) {
      lastError.value = error
      throw error
    } finally {
      starting.value = false
    }
  }

  async function endConversationAsync(): Promise<void> {
    await invoke('assistant_end_conversation')
  }

  async function setMutedAsync(muted: boolean): Promise<void> {
    state.value = await invoke<AssistantStateWire>('assistant_set_muted', {
      muted,
    })
  }

  async function setPreviewAsync(on: boolean): Promise<void> {
    // action-exempt: the preview switch is the user's own choice on the settings page until the wake word (T045); no agent may turn on a microphone path.
    await setPrefAsync(await deviceScopeAsync(), PREVIEW_KEY, String(on))
    preview.value = on
  }

  return {
    state,
    preview,
    entries,
    entriesThread,
    lastError,
    starting,
    inConversation,
    start,
    talkAsync,
    endConversationAsync,
    setMutedAsync,
    setPreviewAsync,
  }
})
