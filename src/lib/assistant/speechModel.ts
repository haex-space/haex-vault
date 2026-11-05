// The speech model choice of spec 081 (FR-006, FR-011, FR-012): what a row says about a model and
// what the hint offers. Pure, so `scripts/check-speech-model.ts` tests it without a window.

/** Whether a model keeps up on this device (`tts_speech_models`). */
export type SpeechSpeed = 'keepsUp' | 'slow' | 'unknown'
export type SpeechEngine = 'pocket' | 'qwen3-tts'

/** One entry of `tts_speech_models` (spec 081 contracts/tauri-commands.md). */
export interface SpeechModelOption {
  id: string
  name: string
  engine: SpeechEngine
  languages: string[]
  license: string
  sizeBytes: number
  installed: boolean
  fit: 'fits' | 'tight' | 'too_big' | 'unknown'
  speed: SpeechSpeed
  /** Seconds of computing per second of speech; `null` while not measured. */
  ratio: number | null
  acceleration: 'gpu' | 'cpu'
  chosen: boolean
  isDefault: boolean
}

/** Payload of `tts-speech-model-hint`. */
export interface SpeechModelHint {
  language: string
  slowModelId: string
  fasterModelId: string
}

/** Payload of `tts-speech-model-fallback`. */
export interface SpeechModelFallback {
  reason: 'memory' | 'notInstalled' | 'loadFailed'
  modelId: string
}

/** The value of the "Automatisch" option: no preference, the default speaks. */
export const AUTOMATIC = ''

/** The locale key of a model's speed mark (FR-006). */
export function speedLabelKey(
  option: Pick<SpeechModelOption, 'speed'>,
): string {
  return `settings.voice.speechModel.speed.${option.speed}`
}

/** The locale key of how a model sounds, in one plain phrase (FR-006). */
export function soundPhraseKey(engine: SpeechEngine): string {
  return engine === 'qwen3-tts'
    ? 'settings.voice.speechModel.sound.natural'
    : 'settings.voice.speechModel.sound.small'
}

/** What choosing `option` downloads: the model and, unless it is installed already, the
 * language's Pocket model, which always stays as the reserve (FR-010a). Zero when everything is
 * here. */
export function downloadBytes(
  option: SpeechModelOption,
  options: readonly SpeechModelOption[],
): number {
  const own = option.installed ? 0 : option.sizeBytes
  if (option.engine === 'pocket') return own
  const reserve = options.find(
    (candidate) => candidate.engine === 'pocket' && !candidate.installed,
  )
  return own + (reserve?.sizeBytes ?? 0)
}

/** The value the choice shows: the chosen model, else "Automatisch". */
export function chosenValue(options: readonly SpeechModelOption[]): string {
  return options.find((option) => option.chosen)?.id ?? AUTOMATIC
}

/** The model that speaks under "Automatisch". */
export function defaultModel(
  options: readonly SpeechModelOption[],
): SpeechModelOption | undefined {
  return options.find((option) => option.isDefault)
}

/** The buttons of the hint toast and what each writes (FR-011): switching names the faster
 * model, both buttons and closing dismiss the hint for good. */
export function hintActions(hint: SpeechModelHint): {
  switchTo: { key: string; value: string }
  dismiss: { key: string; value: string }
} {
  return {
    switchTo: {
      key: `assistant.speechModel.${hint.language}`,
      value: hint.fasterModelId,
    },
    dismiss: { key: 'assistant.speechModelHint', value: 'dismissed' },
  }
}
