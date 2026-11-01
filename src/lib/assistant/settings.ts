// Device settings of the voice assistant (spec 066, data-model.md "Geräteeinstellungen"). One
// list of the keys with their labels for every settings page of the assistant. Pure —
// `scripts/check-assistant-ui.ts` holds it against data-model.md and the locales.

/** The languages haex speaks; each has its own voice (research R22). */
export const SPEECH_LANGUAGES = ['de', 'en', 'fr', 'es', 'it', 'pt'] as const
export type SpeechLanguage = (typeof SPEECH_LANGUAGES)[number]

/** Every key of data-model.md; `<sprache>` stands for one of `SPEECH_LANGUAGES`. */
export const ASSISTANT_SETTING_KEYS = [
  'assistant.enabled',
  'assistant.muted',
  'assistant.wakeWord',
  'assistant.wakeWord.custom',
  'assistant.sensitivity',
  'assistant.pauseMs',
  'assistant.followUpSecs',
  'assistant.continueMins',
  'assistant.modelId',
  'assistant.voice.<sprache>',
  'assistant.speechRate',
  'assistant.ttsProvider',
  'assistant.tones.enabled',
  'assistant.tones.volume',
  'assistant.headphones',
  'assistant.bubble.position',
  'assistant.noticeShown',
] as const
export type AssistantSettingKey = (typeof ASSISTANT_SETTING_KEYS)[number]

/** The locale key of a setting's label: `assistant.tones.volume` → `settings.assistant.labels.tonesVolume`. */
export function assistantLabelKey(key: AssistantSettingKey): string {
  const name = key
    .replace(/^assistant\./, '')
    .replace(/\.<sprache>$/, '')
    .replace(/\.(\w)/g, (_, first: string) => first.toUpperCase())
  return `settings.assistant.labels.${name}`
}

/** The device preference holding the voice of `language`, a `VoiceRef`. */
export function voicePrefKey(language: SpeechLanguage): string {
  return `assistant.voice.${language}`
}

export const SPEECH_RATE_KEY = 'assistant.speechRate'
export const SPEECH_RATE = {
  min: 0.75,
  max: 1.5,
  step: 0.05,
  default: 1,
} as const

/** A stored rate, or the default when it is missing or outside the range. */
export function parseSpeechRate(value: string | null): number {
  const rate = value === null ? Number.NaN : Number(value)
  return rate >= SPEECH_RATE.min && rate <= SPEECH_RATE.max
    ? rate
    : SPEECH_RATE.default
}

/** `ToneId` of contracts/tauri-commands.md, in the order of FR-043. */
export const TONE_IDS = [
  'listening',
  'understood',
  'working',
  'question',
  'done',
  'error',
  'aborted',
  'notUnderstood',
  'conversationEnd',
  'attention',
  'secret',
  'modeOn',
  'modeOff',
  'muted',
  'unmuted',
] as const
export type ToneId = (typeof TONE_IDS)[number]

/** Tones that sound even with tones switched off (FR-045). */
export const ALWAYS_AUDIBLE_TONES: readonly ToneId[] = ['listening', 'secret']

export const TONES_ENABLED_KEY = 'assistant.tones.enabled'
export const TONES_VOLUME_KEY = 'assistant.tones.volume'
export const TONES_VOLUME = { min: 0, max: 100, step: 5, default: 60 } as const

/** A stored switch: only `"false"` turns the tones off. */
export function parseTonesEnabled(value: string | null): boolean {
  return value !== 'false'
}

/** A stored volume, or the default when it is missing or not a whole number in range. */
export function parseTonesVolume(value: string | null): number {
  const volume = value === null ? Number.NaN : Number(value)
  return Number.isInteger(volume) &&
    volume >= TONES_VOLUME.min &&
    volume <= TONES_VOLUME.max
    ? volume
    : TONES_VOLUME.default
}
