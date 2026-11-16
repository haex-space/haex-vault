import type { AssistantName } from '../../types/bindings/AssistantName.ts'

/**
 * Where a text means the assistant by name, the locale files hold this token; the name that
 * counts (written form) replaces it when the text is shown (`src/i18n/i18n.config.ts`). The name
 * is one per vault, set on the assistant page (design 2026-11-16).
 */
export const ASSISTANT_TOKEN = '⟦assistant⟧'

/** The name of a vault that has none set: written "Haex", said "Hex". */
export const DEFAULT_ASSISTANT_NAME: AssistantName = {
  written: 'Haex',
  spoken: 'Hex',
}

export const NAME_MAX_CHARS = 24

/** `text` with the token replaced by `name`. */
export function withAssistantName(text: string, name: string): string {
  return text.includes(ASSISTANT_TOKEN)
    ? text.replaceAll(ASSISTANT_TOKEN, name)
    : text
}

/**
 * Whether `value` is a name the backend accepts: one or two words of letters, digits and hyphens,
 * at most {@link NAME_MAX_CHARS} characters (`assistant_name.rs` decides; this only lets the page
 * say so before saving).
 */
export function isValidName(value: string): boolean {
  const name = value.trim().split(/\s+/).join(' ')
  return (
    name.length <= NAME_MAX_CHARS &&
    /^[\p{L}\p{N}-]+( [\p{L}\p{N}-]+)?$/u.test(name)
  )
}
