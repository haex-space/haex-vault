import { invoke } from '@tauri-apps/api/core'
import type { Fit } from '~/composables/useCatalog'
import type { SpeechLanguage } from '~/lib/assistant/settings'

/** Where a voice comes from (spec 066 contracts/tauri-commands.md). */
export type VoiceKind = 'builtin' | 'clone' | 'model'

/** One voice of `tts_voices_list`. `installed`, `sizeBytes` and `fit` describe the speech model
 * `modelId` the voice needs. */
export interface VoiceOption {
  ref: string
  kind: VoiceKind
  name: string
  language: SpeechLanguage
  modelId: string
  installed: boolean
  sizeBytes: number
  fit: Fit
  isDefault: boolean
}

/** `assistant-download-progress`: a speech model download of the assistant. */
export interface AssistantDownloadProgress {
  id: string
  received: number
  total: number
}

export const ASSISTANT_DOWNLOAD_PROGRESS = 'assistant-download-progress'

/** The voices of the assistant and their sample (spec 066 T028/T029). */
export function useTtsVoices() {
  async function listAsync(): Promise<VoiceOption[]> {
    return await invoke<VoiceOption[]>('tts_voices_list')
  }

  /** Plays the sample of `voice`, downloading its speech model first; resolves when it has played
   * or a newer sample replaced it. */
  async function previewAsync(voice: string, rate: number): Promise<void> {
    await invoke('assistant_tts_preview', { voice, rate })
  }

  return { listAsync, previewAsync }
}
