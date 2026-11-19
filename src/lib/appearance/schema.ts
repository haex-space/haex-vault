// The appearance of a vault and its file (spec 035-appearance-and-fields, data-model.md and
// contracts/appearance-file.md). Pure — `useAppearance` stores and applies it, `derive.ts` turns it
// into CSS variables, `scripts/check-appearance*.ts` tests it.
import { parseColorScheme, type ColorScheme } from '../settings/colorScheme.ts'
import { isHexColor } from './oklch.ts'
import { ACCENT_PRESETS, TINT_PRESETS, type Preset } from './presets.ts'

/** The vault preference key of the appearance (the colour scheme keeps its own key, 023). */
export const APPEARANCE_KEY = 'appearance.theme'

export const CONTROLS = [
  'accent',
  'window',
  'container',
  'text',
  'component',
] as const
export type Control = (typeof CONTROLS)[number]

export type ColorChoice = { preset: string } | { custom: string }

export interface Appearance {
  v: 1
  accent: ColorChoice
  window: ColorChoice
  container: ColorChoice
  text: ColorChoice
  component: ColorChoice
  windowHint: boolean
}

export const DEFAULT_APPEARANCE: Appearance = {
  v: 1,
  accent: { preset: 'sky' },
  window: { preset: 'cool' },
  container: { preset: 'cool' },
  text: { preset: 'cool' },
  component: { preset: 'cool' },
  windowHint: false,
}

/** The row of preset colour fields a control offers. */
export function presetsFor(control: Control): readonly Preset[] {
  return control === 'accent' ? ACCENT_PRESETS : TINT_PRESETS
}

/** The choice as stored (custom colours in lower case), or null when it is not a valid one. */
export function validChoice(
  control: Control,
  value: unknown,
): ColorChoice | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return null
  }
  const keys = Object.keys(value)
  if (keys.length !== 1) return null
  const record = value as Record<string, unknown>
  if (keys[0] === 'preset') {
    const id = record.preset
    return typeof id === 'string' &&
      presetsFor(control).some((preset) => preset.id === id)
      ? { preset: id }
      : null
  }
  if (keys[0] === 'custom') {
    return isHexColor(record.custom)
      ? { custom: (record.custom as string).toLowerCase() }
      : null
  }
  return null
}

/**
 * The appearance from what the vault holds (FR-020): anything that is not a JSON object with `v: 1`
 * gives the default; a single invalid field falls back to its default and the others stay; unknown
 * fields are ignored.
 */
export function parseAppearance(raw: unknown): Appearance {
  let value: unknown = raw
  if (typeof raw === 'string') {
    try {
      value = JSON.parse(raw)
    } catch {
      return DEFAULT_APPEARANCE
    }
  }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return DEFAULT_APPEARANCE
  }
  const record = value as Record<string, unknown>
  if (record.v !== 1) return DEFAULT_APPEARANCE
  const result: Appearance = { ...DEFAULT_APPEARANCE }
  for (const control of CONTROLS) {
    result[control] =
      validChoice(control, record[control]) ?? DEFAULT_APPEARANCE[control]
  }
  result.windowHint =
    typeof record.windowHint === 'boolean' ? record.windowHint : false
  return result
}

/** The JSON text stored in the vault: only known fields, in a fixed order. */
export function serializeAppearance(appearance: Appearance): string {
  return JSON.stringify({
    v: 1,
    accent: appearance.accent,
    window: appearance.window,
    container: appearance.container,
    text: appearance.text,
    component: appearance.component,
    windowHint: appearance.windowHint,
  })
}

export const FILE_FORMAT = 'haex-appearance'
export const FILE_MAX_BYTES = 16 * 1024

export type ImportReason = 'notJson' | 'notAppearance' | 'version' | 'field'
export type FileResult =
  | { ok: true; appearance: Appearance; colorScheme: ColorScheme }
  | { ok: false; reason: ImportReason; field?: string }

const fail = (reason: ImportReason, field?: string): FileResult => ({
  ok: false,
  reason,
  ...(field === undefined ? {} : { field }),
})

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

const unknownKey = (value: Record<string, unknown>, known: readonly string[]) =>
  Object.keys(value).find((key) => !known.includes(key))

/**
 * The text of an appearance file, checked completely (FR-021): all or nothing, the first cause is
 * named by `reason` (and `field`), nothing is repaired or defaulted.
 */
export function parseAppearanceFile(text: string): FileResult {
  if (new TextEncoder().encode(text).length > FILE_MAX_BYTES) {
    return fail('notJson')
  }
  let file: unknown
  try {
    file = JSON.parse(text)
  } catch {
    return fail('notJson')
  }
  if (!isObject(file) || file.format !== FILE_FORMAT) {
    return fail('notAppearance')
  }
  if (file.v !== 1) return fail('version')
  const extra = unknownKey(file, ['format', 'v', 'colorScheme', 'appearance'])
  if (extra !== undefined) return fail('field', extra)

  const colorScheme = parseColorScheme(file.colorScheme)
  if (colorScheme === null) return fail('field', 'colorScheme')
  const inner = file.appearance
  if (!isObject(inner)) return fail('field', 'appearance')
  if (inner.v !== 1) return fail('version')
  const innerExtra = unknownKey(inner, ['v', ...CONTROLS, 'windowHint'])
  if (innerExtra !== undefined) return fail('field', `appearance.${innerExtra}`)

  const appearance: Appearance = { ...DEFAULT_APPEARANCE }
  for (const control of CONTROLS) {
    const choice = validChoice(control, inner[control])
    if (choice === null) return fail('field', `appearance.${control}`)
    appearance[control] = choice
  }
  if (typeof inner.windowHint !== 'boolean') {
    return fail('field', 'appearance.windowHint')
  }
  appearance.windowHint = inner.windowHint
  return { ok: true, appearance, colorScheme }
}

/** The text of an appearance file; `colorScheme` is the stored choice, not the resolved scheme. */
export function exportFile(
  appearance: Appearance,
  colorScheme: ColorScheme,
): string {
  return `${JSON.stringify(
    {
      format: FILE_FORMAT,
      v: 1,
      colorScheme,
      appearance: JSON.parse(serializeAppearance(appearance)),
    },
    null,
    2,
  )}\n`
}
