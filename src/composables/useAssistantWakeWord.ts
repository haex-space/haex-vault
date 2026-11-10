import { invoke } from '@tauri-apps/api/core'

/** The `model-download-progress` id of the wake word models. */
export const WAKE_WORD_MODEL_ID = 'wakeword-hex'
/** The device setting that picks how readily the wake word fires (spec 066 FR-011). */
export const SENSITIVITY_KEY = 'assistant.sensitivity'
export const SENSITIVITIES = ['low', 'medium', 'high'] as const
export type Sensitivity = (typeof SENSITIVITIES)[number]

/** A stored value as one of the three; anything else is the default, `medium`. */
export function parseSensitivity(
  value: string | null | undefined,
): Sensitivity {
  return (SENSITIVITIES as readonly string[]).includes(value ?? '')
    ? (value as Sensitivity)
    : 'medium'
}

/**
 * The default wake word "Hex" / "Hey Hex" (spec 066 FR-011, PR D): whether its two small models
 * are on this device, and their download.
 */
export function useAssistantWakeWord() {
  async function modelAsync(): Promise<{
    installed: boolean
    sizeBytes: number
  }> {
    return await invoke('assistant_wake_word_model')
  }

  async function downloadModelAsync(): Promise<void> {
    await invoke('assistant_wake_word_model_download')
  }

  return { modelAsync, downloadModelAsync }
}
