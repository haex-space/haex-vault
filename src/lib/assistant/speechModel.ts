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
  reason: 'memory' | 'notInstalled' | 'loadFailed' | 'partFailed'
  modelId: string
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

/** What choosing `option` downloads beyond the model itself: the Pocket reserve while it is
 * missing. */
export function reserveBytes(
  option: SpeechModelOption,
  options: readonly SpeechModelOption[],
): number {
  return (
    downloadBytes(option, options) - (option.installed ? 0 : option.sizeBytes)
  )
}

/** The model that speaks without a choice (FR-012). */
export function defaultModel(
  options: readonly SpeechModelOption[],
): SpeechModelOption | undefined {
  return options.find((option) => option.isDefault)
}

/** The model the choice shows: the chosen one, else the default; empty while nothing is loaded.
 * There is no own "Automatisch" row: the default is marked in its own row (operator 2026-11-06). */
export function chosenValue(options: readonly SpeechModelOption[]): string {
  return (
    options.find((option) => option.chosen)?.id ??
    defaultModel(options)?.id ??
    ''
  )
}

/** The preference choosing `id` writes: `null` removes it when `id` is the default, so the
 * device keeps following the default; any other model is stored by its id. */
export function preferenceFor(
  id: string,
  options: readonly SpeechModelOption[],
): string | null {
  return defaultModel(options)?.id === id ? null : id
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

/** Whether an installed model can be deleted from this device: every model but Pocket, which stays
 * as its language's reserve (FR-010a). */
export function mayDelete(option: SpeechModelOption): boolean {
  return option.installed && option.engine !== 'pocket'
}
