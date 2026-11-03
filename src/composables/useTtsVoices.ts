import { invoke } from '@tauri-apps/api/core'
import type { Fit } from '~/composables/useCatalog'
import type { SpeechLanguage, ToneId } from '~/lib/assistant/settings'
import type { PickedFile } from '~/types/bindings/PickedFile'

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

/** An own tone of the vault (spec 066 FR-045a, `CustomTone`). */
export interface CustomTone {
  id: string
  name: string
  durationMs: number
}

/** `assistant_tones_list`: every own tone and which occasion uses one on this device. */
export interface TonesOverview {
  custom: CustomTone[]
  assigned: Partial<Record<ToneId, string>>
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

  /** Downloads the speech model `modelId` (a voice's `modelId`). Progress arrives as the shared
   * `model-download-progress`, so the model download components can display it. */
  async function downloadModelAsync(modelId: string): Promise<void> {
    await invoke('download_tts_voice', { voiceId: modelId })
  }

  /** Plays `tone` once at `volume` (0–100), also while tones are switched off: the own tone
   * `customId` when given, otherwise what sounds for `tone` on this device. */
  async function tonePreviewAsync(
    tone: ToneId,
    volume: number,
    customId: string | null = null,
  ): Promise<void> {
    await invoke('assistant_tone_preview', { tone, volume, customId })
  }

  async function tonesListAsync(): Promise<TonesOverview> {
    return await invoke<TonesOverview>('assistant_tones_list')
  }

  /** Imports a chosen audio file as an own tone; rejects with `tooLarge`, `unsupported`, `silent`
   * or `tooLong`. */
  async function toneImportAsync(file: PickedFile): Promise<CustomTone> {
    return await invoke<CustomTone>('assistant_tone_import', { file })
  }

  /** Lets `tone` sound with the own tone `customId` on this device, or the built-in one (`null`). */
  async function toneAssignAsync(
    tone: ToneId,
    customId: string | null,
  ): Promise<void> {
    await invoke('assistant_tone_assign', { tone, customId })
  }

  /** Deletes an own tone on every device. */
  async function toneDeleteAsync(customId: string): Promise<void> {
    await invoke('assistant_tone_delete', { customId })
  }

  async function creditsAsync(): Promise<SpeechCredit[]> {
    return await invoke<SpeechCredit[]>('speech_credits')
  }

  return {
    listAsync,
    previewAsync,
    downloadModelAsync,
    tonePreviewAsync,
    tonesListAsync,
    toneImportAsync,
    toneAssignAsync,
    toneDeleteAsync,
    creditsAsync,
  }
}
