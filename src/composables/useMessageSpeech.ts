import { invoke } from '@tauri-apps/api/core'
import { utcOffsetMinutes } from '~/lib/assistant/wire'

/** The message being read aloud; one at a time across every chat window of this app. */
const speakingId = ref<string | null>(null)

/**
 * Reading an answer of the agent aloud (spec 066 FR-022c): `chat_message_speak` says the whole
 * answer in the language it is written in, with the speech model and voice chosen for it. Starting
 * another message stops the one playing; pressing the same one again stops it.
 */
export function useMessageSpeech() {
  const { locale } = useI18n()

  /** Reads `text` of message `id` aloud, or stops it when it is the one playing. Rejects with the
   * backend error when it cannot speak (no speech model, no speaker). */
  async function toggleAsync(id: string, text: string): Promise<void> {
    if (speakingId.value === id) {
      await stopAsync()
      return
    }
    speakingId.value = id
    try {
      await invoke<boolean>('chat_message_speak', {
        text,
        uiLanguage: locale.value,
        utcOffsetMinutes: utcOffsetMinutes(),
      })
    } finally {
      // A newer reading took the place of this one: it is that one's state now.
      if (speakingId.value === id) speakingId.value = null
    }
  }

  async function stopAsync(): Promise<void> {
    speakingId.value = null
    await invoke('chat_message_speak_stop')
  }

  return { speakingId: readonly(speakingId), toggleAsync, stopAsync }
}
