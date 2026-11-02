import { invoke } from '@tauri-apps/api/core'
import type { Fit } from '~/composables/useCatalog'
import type { SpeechLanguage, ToneId } from '~/lib/assistant/settings'

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

/** One entry of the page "Lizenzen" for voices and speech models (spec 066 T032). */
export interface SpeechCredit {
  kind: 'voice' | 'speechModel' | 'recognitionModel'
  name: string
  language: SpeechLanguage | null
  license: string
  attribution: string
  source: string
}

/** The voices and tones of the assistant, their samples and credits (spec 066 T028–T032). */
export function useTtsVoices() {
  async function listAsync(): Promise<VoiceOption[]> {
    return await invoke<VoiceOption[]>('tts_voices_list')
  }

  /** Plays the sample of `voice`, downloading its speech model first; resolves when it has played
   * or a newer sample replaced it. */
  async function previewAsync(voice: string, rate: number): Promise<void> {
    await invoke('assistant_tts_preview', { voice, rate })
  }

  /** Plays `tone` once at `volume` (0–100), also while tones are switched off. */
  async function tonePreviewAsync(tone: ToneId, volume: number): Promise<void> {
    await invoke('assistant_tone_preview', { tone, volume })
  }

  /** Downloads the speech model `modelId` (a voice's `modelId`). Progress arrives as the shared
   * `model-download-progress`, so `useModelDownloadsStore` and the model download components
   * show it. */
  async function downloadModelAsync(modelId: string): Promise<void> {
    await invoke('download_tts_voice', { voiceId: modelId })
  }

  async function creditsAsync(): Promise<SpeechCredit[]> {
    return await invoke<SpeechCredit[]>('speech_credits')
  }

  return {
    listAsync,
    previewAsync,
    downloadModelAsync,
    tonePreviewAsync,
    creditsAsync,
  }
}
