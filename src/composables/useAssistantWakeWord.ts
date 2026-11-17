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
