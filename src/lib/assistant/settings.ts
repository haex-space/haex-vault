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
  'assistant.continueMins',
  'assistant.language',
  'assistant.voice.<sprache>',
  'assistant.speechRate',
  'assistant.speechModel.<sprache>',
  'assistant.speechModelHint',
  'assistant.ttsProvider',
  'assistant.tones.enabled',
  'assistant.tones.volume',
  'assistant.tones.custom',
  'assistant.headphones',
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

/** The language of spoken conversations on this device (FR-002a, operator 2026-11-03): `'ui'`
 * follows the interface; another one lets the user practise a language. Typed chats always answer
 * in the interface language. */
export const CONVERSATION_LANGUAGE_KEY = 'assistant.language'
export const CONVERSATION_LANGUAGES = ['ui', ...SPEECH_LANGUAGES] as const
export type ConversationLanguage = (typeof CONVERSATION_LANGUAGES)[number]

/** A stored choice; anything else follows the interface, as `reply_language::resolve` does. */
export function parseConversationLanguage(
  value: string | null,
): ConversationLanguage {
  return (CONVERSATION_LANGUAGES as readonly string[]).includes(value ?? '')
    ? (value as ConversationLanguage)
    : 'ui'
}

/** The language a conversation is held in: the choice, else the interface language when haex
 * speaks it, else English. */
export function spokenLanguage(
  choice: ConversationLanguage,
  interfaceLocale: string,
): SpeechLanguage {
  if (choice !== 'ui') return choice
  return SPEECH_LANGUAGES.find((l) => interfaceLocale.startsWith(l)) ?? 'en'
}

/** The device preference holding the voice of `language`, a `VoiceRef`. */
export function voicePrefKey(language: SpeechLanguage): string {
  return `assistant.voice.${language}`
}

/** The device preference naming the speech model of `language` (spec 081), a catalog id; missing
 * means the default (Pocket TTS, operator 2026-11-06). */
export function speechModelPrefKey(language: SpeechLanguage): string {
  return `assistant.speechModel.${language}`
}

/** `"dismissed"` once the hint on a faster speech model was closed (spec 081 FR-011). */
export const SPEECH_MODEL_HINT_KEY = 'assistant.speechModelHint'

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

/** How long a pause ends what was said (FR-012); 1.5 s since a thinking pause of 0.8 s ended it
 * too early (operator 2026-11-02). */
export const PAUSE_KEY = 'assistant.pauseMs'
export const PAUSE_MS = {
  min: 500,
  max: 3000,
  step: 100,
  default: 1500,
} as const

/** The pause the assistant uses for a stored value: the default when it is missing or no whole
 * number, else clamped to the range, as `read_settings` and `PauseDetector::new` do. */
export function parsePause(value: string | null): number {
  const text = value?.trim() ?? ''
  const ms = /^[+-]?\d+$/.test(text) ? Number(text) : Number.NaN
  return Number.isSafeInteger(ms)
    ? Math.min(Math.max(ms, PAUSE_MS.min), PAUSE_MS.max)
    : PAUSE_MS.default
}

/** How long after a conversation ended the wake word continues it instead of starting a new one
 * (`assistant.continueMins`, FR-015), in minutes; 0 = always a new one. */
export const CONTINUE_MINS_KEY = 'assistant.continueMins'
export const CONTINUE_MINS = { min: 0, max: 60, default: 5 } as const
/** What the page offers; a stored value that is not one of them is offered too. */
export const CONTINUE_MINS_CHOICES = [0, 1, 2, 5, 10, 15, 30, 60] as const

/** The time the assistant uses for a stored value: the default when it is missing or no whole
 * number, else clamped to the range, as `Timing::from_prefs` does. */
export function parseContinueMins(value: string | null): number {
  const text = value?.trim() ?? ''
  const mins = /^[+-]?\d+$/.test(text) ? Number(text) : Number.NaN
  return Number.isSafeInteger(mins)
    ? Math.min(Math.max(mins, CONTINUE_MINS.min), CONTINUE_MINS.max)
    : CONTINUE_MINS.default
}

/** The choices of the page with `current` among them, in order. */
export function continueMinsChoices(current: number): number[] {
  return [...new Set([...CONTINUE_MINS_CHOICES, current])].sort((a, b) => a - b)
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
/** Which own tone sounds for which occasion on this device (FR-045a); written by the backend. */
export const TONES_CUSTOM_KEY = 'assistant.tones.custom'
/** The file types an own tone can come from (FR-045a). */
export const TONE_FILE_EXTENSIONS = [
  'mp3',
  'wav',
  'ogg',
  'flac',
  'm4a',
] as const

/** The file types an own voice can come from (FR-049): the same decoder as the own tones. */
export const VOICE_FILE_EXTENSIONS = TONE_FILE_EXTENSIONS

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

/** The toggle and the mute of this device, written by the backend on every switch and read after
 * unlock (FR-002). */
export const ENABLED_KEY = 'assistant.enabled'
export const MUTED_KEY = 'assistant.muted'

/** `"true"` once the notice of FR-037 was confirmed on this device. */
export const NOTICE_SHOWN_KEY = 'assistant.noticeShown'
