import { invoke } from '@tauri-apps/api/core'
import type { Fit } from '~/composables/useCatalog'
import type { SpeechLanguage, ToneId } from '~/lib/assistant/settings'
import type { PickedFile } from '~/types/bindings/PickedFile'
import type { SpeechModelOption } from '~/lib/assistant/speechModel'

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

/** What is wrong with a recording for an own voice (spec 066 FR-049, `CloneCheck.problems`). */
export type CloneProblem =
  'tooShort' | 'tooLong' | 'tooQuiet' | 'clipped' | 'noisy'

/** The check of a recording for an own voice (`CloneCheck`). */
export interface CloneCheck {
  sessionId: string
  speechSecs: number
  ok: boolean
  problems: CloneProblem[]
}

/** Payload of `tts-clone-level` while a voice is recorded. */
export interface CloneLevel {
  sessionId: string
  /** 0 (silence) to 1 (full scale). */
  level: number
  speechSecs: number
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

  /** The speech models that speak `language` in this build, best-sounding first, with their
   * speed on this device (spec 081 `tts_speech_models`). */
  async function speechModelsAsync(
    language: SpeechLanguage,
  ): Promise<SpeechModelOption[]> {
    return await invoke<SpeechModelOption[]>('tts_speech_models', { language })
  }

  /** Applies the saved speech model to the next sentence of an open assistant conversation. */
  async function applySpeechModelAsync(
    language: SpeechLanguage,
  ): Promise<void> {
    await invoke('assistant_apply_speech_model', { language })
  }

  /** Plays the sample of `voice`, downloading its speech model first; resolves when it has played
   * or a newer sample replaced it. */
  async function previewAsync(voice: string, rate: number): Promise<void> {
    await invoke('assistant_tts_preview', { voice, rate })
  }

  /** Downloads the speech model `modelId` (a voice's `modelId`) and, for Qwen3-TTS, the Pocket
   * model of `language` as its reserve (spec 081 FR-010a). Progress arrives as the shared
   * `model-download-progress`, so the model download components can display it. */
  async function downloadModelAsync(
    modelId: string,
    language: SpeechLanguage,
  ): Promise<void> {
    await invoke('download_tts_voice', { voiceId: modelId, language })
  }

  /** Deletes the files of speech model `modelId` on this device; Pocket models are refused, they
   * stay as their language's reserve (spec 081 FR-010a). */
  async function deleteModelAsync(modelId: string): Promise<void> {
    await invoke('delete_tts_model', { modelId })
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

  /** Starts recording an own voice; the level arrives as `tts-clone-level` (FR-049). */
  async function cloneRecordStartAsync(): Promise<string> {
    const { sessionId } = await invoke<{ sessionId: string }>(
      'tts_clone_record_start',
    )
    return sessionId
  }

  async function cloneRecordStopAsync(sessionId: string): Promise<CloneCheck> {
    return await invoke<CloneCheck>('tts_clone_record_stop', { sessionId })
  }

  async function cloneRecordCancelAsync(sessionId: string): Promise<void> {
    await invoke('tts_clone_record_cancel', { sessionId })
  }

  /** Reads an own voice from a chosen audio file and checks it. */
  async function cloneFromFileAsync(file: PickedFile): Promise<CloneCheck> {
    return await invoke<CloneCheck>('tts_clone_from_file', { file })
  }

  /** Speaks a sample with the checked recording of `sessionId`. */
  async function clonePreviewAsync(
    sessionId: string,
    language: SpeechLanguage,
    rate: number,
  ): Promise<void> {
    await invoke('tts_clone_preview', { sessionId, language, rate })
  }

  /** Keeps the checked recording as an own voice of the vault; `consent` is the user's
   * confirmation that it is their voice or the speaker consented (FR-050). */
  async function cloneSaveAsync(
    sessionId: string,
    name: string,
    language: SpeechLanguage,
    consent: boolean,
  ): Promise<VoiceOption> {
    return await invoke<VoiceOption>('tts_clone_save', {
      sessionId,
      name,
      language,
      consent,
    })
  }

  /** Deletes an own voice on every device. */
  async function voiceDeleteAsync(voice: string): Promise<void> {
    await invoke('tts_voice_delete', { voice })
  }

  async function creditsAsync(): Promise<SpeechCredit[]> {
    return await invoke<SpeechCredit[]>('speech_credits')
  }

  return {
    listAsync,
    speechModelsAsync,
    applySpeechModelAsync,
    previewAsync,
    downloadModelAsync,
    deleteModelAsync,
    tonePreviewAsync,
    tonesListAsync,
    toneImportAsync,
    toneAssignAsync,
    toneDeleteAsync,
    cloneRecordStartAsync,
    cloneRecordStopAsync,
    cloneRecordCancelAsync,
    cloneFromFileAsync,
    clonePreviewAsync,
    cloneSaveAsync,
    voiceDeleteAsync,
    creditsAsync,
  }
}
