// The context setting of a local model (spec 086): what the settings page keeps per model and device,
// what the backend reports about it, and how the page words it. Pure, so `pnpm check:settings` tests it.

/** The type of the KV cache: standard is 16-bit; a smaller one saves memory and can lower the
 * quality of long contexts. */
export type ContextCache = 'standard' | 'q8' | 'q4'

export const CONTEXT_CACHES: readonly ContextCache[] = ['standard', 'q8', 'q4']

/** The setting as the actions carry it: `context` is `auto` or a number of tokens as digits. */
export interface ContextSetting {
  context: string
  cache: ContextCache
}

export const DEFAULT_CONTEXT_SETTING: ContextSetting = {
  context: 'auto',
  cache: 'standard',
}

/** How the backend would place the setting (Rust `report::Verdict`). */
export type ContextVerdict =
  | 'fits'
  | 'fits-on-host'
  | 'too-large'
  | 'beyond-training'
  | 'not-stretchable'
  | 'below-floor'
  | 'unsized'

/** Why the context in effect is what it is (Rust `report::Reason`). */
export type ContextReason = 'target' | 'trained' | 'memory' | 'choice'

/** What a loaded model runs with. */
export interface ContextEffective {
  context: number
  gpuLayers: number
  layers: number
  cache: ContextCache
  yarnFrom: number | null
  reason: ContextReason
}

/** The answer of `settings.models.context.report`. */
export interface ContextReport {
  trained: number | null
  maxOnGpu: number | null
  maxAny: number | null
  maxStretched: number | null
  sized: boolean
  verdict: ContextVerdict
  needMib: number | null
  freeMib: number | null
  yarn: number | null
  effective: ContextEffective | null
}

/** The lengths the select offers besides Auto and a typed one. */
export const CONTEXT_PRESETS: readonly number[] = [
  8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576,
]

/** The least a fixed context may be (the planner's floor). */
export const MIN_CONTEXT = 2048

export function isDefaultSetting(setting: ContextSetting): boolean {
  return (
    setting.context === DEFAULT_CONTEXT_SETTING.context &&
    setting.cache === DEFAULT_CONTEXT_SETTING.cache
  )
}

/** The number of tokens of a fixed setting, `null` for Auto. */
export function tokensOf(setting: ContextSetting): number | null {
  const tokens = Number(setting.context)
  return Number.isInteger(tokens) && tokens > 0 ? tokens : null
}

/**
 * What a person typed as a context length: digits (`131072`), or a number with `k` for 1024
 * (`128k`, `64 K`, `0.5k`); `null` when it is neither. A comma counts as the decimal point.
 */
export function parseTokens(text: string): number | null {
  const match = /^\s*(\d+(?:[.,]\d+)?)\s*([kK]?)\s*$/.exec(text)
  if (!match) return null
  const value = Number(match[1]!.replace(',', '.')) * (match[2] ? 1024 : 1)
  return Number.isInteger(value) && value > 0 && value <= 0xffffffff
    ? value
    : null
}

/** The presets the model can have: up to what it can be stretched to, else its own length, else all. */
export function presetsFor(report: ContextReport | null): number[] {
  const ceiling =
    report?.maxStretched ?? report?.trained ?? CONTEXT_PRESETS.at(-1)!
  return CONTEXT_PRESETS.filter((tokens) => tokens <= ceiling)
}

/** The i18n key of the sentence about how the setting is placed. */
export function verdictKey(verdict: ContextVerdict): string {
  const name = verdict.replace(/-(\w)/g, (_, letter: string) =>
    letter.toUpperCase(),
  )
  return `models.context.verdict.${name}`
}

/** Whether the setting is one the page refuses to keep: its rules are broken, memory is not a rule. */
export function isRefused(verdict: ContextVerdict): boolean {
  return (
    verdict === 'below-floor' ||
    verdict === 'beyond-training' ||
    verdict === 'not-stretchable'
  )
}

/**
 * The notes that go with a setting besides the verdict: the stretch and the smaller cache. The 8-bit cache has a second
 * sentence from the measurement of research R9; the 4-bit cache was not measured and only gets the general one.
 */
export function noteKeys(
  setting: ContextSetting,
  report: ContextReport | null,
): string[] {
  const notes: string[] = []
  if (report?.yarn && report.yarn > 1) notes.push('models.context.note.yarn')
  if (setting.cache !== 'standard') notes.push('models.context.note.cache')
  if (setting.cache === 'q8') notes.push('models.context.note.cacheQ8')
  return notes
}

/** The i18n key of the reason for the context in effect. */
export function reasonKey(reason: ContextReason): string {
  return `models.context.reason.${reason}`
}
