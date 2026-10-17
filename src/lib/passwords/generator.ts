// Password generator (spec 034-password-manager, US3, FR-013, FR-014, research R10): length,
// character classes, excluded characters or a pattern. Written new instead of copied from haex-vault 2.x (Legacy):
// the choice has no modulo bias (rejection sampling), every chosen class occurs at least once, a
// choice that allows no output returns an error code instead of an empty password, and the pattern
// can escape its letters. The password exists only here, in the window, when the user asks for one.
// Pure with an injectable random source, so `scripts/check-passwords-generator.ts` is deterministic.

export type GeneratorConfig = {
  length: number
  uppercase: boolean
  lowercase: boolean
  numbers: boolean
  symbols: boolean
  excludeChars: string
  usePattern: boolean
  pattern: string
}

export type GeneratorError =
  | 'length_range'
  | 'no_class'
  | 'length_below_classes'
  | 'class_empty'
  | 'pattern_empty'

export type GenerateResult = { value: string } | { error: GeneratorError }

/** Fills the array with random 32 bit numbers; `crypto.getRandomValues` by default. */
export type RandomSource = (buffer: Uint32Array<ArrayBuffer>) => void

export const MIN_LENGTH = 1
export const MAX_LENGTH = 256

const CLASS_CHARS = {
  uppercase: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  lowercase: 'abcdefghijklmnopqrstuvwxyz',
  numbers: '0123456789',
  symbols: '!@#$%^&*()_+-=[]{}|;:,.<>?',
} as const

/** The letters of pattern mode (the notation of haex-vault 2.x). */
const PATTERN_CHARS: Record<string, string> = {
  c: 'bcdfghjklmnpqrstvwxyz',
  C: 'BCDFGHJKLMNPQRSTVWXYZ',
  v: 'aeiou',
  V: 'AEIOU',
  d: '0123456789',
  a: 'abcdefghijklmnopqrstuvwxyz',
  A: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  s: CLASS_CHARS.symbols,
}

const platformRandom: RandomSource = (buffer) => {
  crypto.getRandomValues(buffer)
}

const RANGE = 0x1_0000_0000

/** A uniform index in `0..max` by rejection sampling: draws that fall into the uneven tail of the
 * 32 bit range are dropped, so no choice is more likely than another. */
export function randomIndex(
  max: number,
  random: RandomSource = platformRandom,
): number {
  if (max <= 1) return 0
  const limit = RANGE - (RANGE % max)
  const buffer = new Uint32Array(1)
  for (;;) {
    random(buffer)
    const draw = buffer[0] ?? 0
    if (draw < limit) return draw % max
  }
}

function pick(chars: string, random: RandomSource): string {
  return chars.charAt(randomIndex(chars.length, random))
}

/** Fisher–Yates with the same random source. */
function shuffle(chars: string[], random: RandomSource): string[] {
  for (let i = chars.length - 1; i > 0; i -= 1) {
    const j = randomIndex(i + 1, random)
    ;[chars[i], chars[j]] = [chars[j]!, chars[i]!]
  }
  return chars
}

function generateFromPattern(
  pattern: string,
  random: RandomSource,
): GenerateResult {
  if (pattern === '') return { error: 'pattern_empty' }
  let out = ''
  const chars = Array.from(pattern)
  for (let i = 0; i < chars.length; i += 1) {
    const char = chars[i]!
    if (char === '\\' && i + 1 < chars.length) {
      i += 1
      out += chars[i]
    } else if (PATTERN_CHARS[char]) {
      out += pick(PATTERN_CHARS[char]!, random)
    } else {
      out += char
    }
  }
  return { value: out }
}

/** A password for the configuration, or the code of what has to change (FR-014). In pattern mode
 * the length and the excluded characters do not apply. */
export function generatePassword(
  config: GeneratorConfig,
  random: RandomSource = platformRandom,
): GenerateResult {
  if (config.usePattern) return generateFromPattern(config.pattern, random)
  if (
    !Number.isInteger(config.length) ||
    config.length < MIN_LENGTH ||
    config.length > MAX_LENGTH
  ) {
    return { error: 'length_range' }
  }
  const excluded = new Set(Array.from(config.excludeChars))
  const classes = (['uppercase', 'lowercase', 'numbers', 'symbols'] as const)
    .filter((name) => config[name])
    .map((name) =>
      Array.from(CLASS_CHARS[name])
        .filter((char) => !excluded.has(char))
        .join(''),
    )
  if (classes.length === 0) return { error: 'no_class' }
  if (config.length < classes.length) return { error: 'length_below_classes' }
  if (classes.some((chars) => chars === '')) return { error: 'class_empty' }
  const all = classes.join('')
  const out = classes.map((chars) => pick(chars, random))
  while (out.length < config.length) out.push(pick(all, random))
  return { value: shuffle(out, random).join('') }
}
