export interface IPasswordConfig {
  length: number
  uppercase: boolean
  lowercase: boolean
  numbers: boolean
  symbols: boolean
  excludeChars: string
  usePattern: boolean
  pattern: string
}

const PATTERN_MAP: Record<string, string> = {
  c: 'bcdfghjklmnpqrstvwxyz',
  C: 'BCDFGHJKLMNPQRSTVWXYZ',
  v: 'aeiou',
  V: 'AEIOU',
  d: '0123456789',
  a: 'abcdefghijklmnopqrstuvwxyz',
  A: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  s: '!@#$%^&*()_+-=[]{}|;:,.<>?',
}

const CHARSETS = {
  uppercase: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  lowercase: 'abcdefghijklmnopqrstuvwxyz',
  numbers: '0123456789',
  symbols: '!@#$%^&*()_+-=[]{}|;:,.<>?',
}

export const usePasswordGenerator = () => {
  const getRandomChar = (charset: string): string => {
    const array = new Uint32Array(1)
    crypto.getRandomValues(array)
    const randomValue = array[0] ?? 0
    const index = randomValue % charset.length
    return charset.charAt(index)
  }

  const generate = (config: IPasswordConfig): string => {
    if (config.usePattern && config.pattern) {
      return config.pattern
        .split('')
        .map((char) =>
          PATTERN_MAP[char] ? getRandomChar(PATTERN_MAP[char]) : char,
        )
        .join('')
    }

    let chars = ''
    if (config.uppercase) chars += CHARSETS.uppercase
    if (config.lowercase) chars += CHARSETS.lowercase
    if (config.numbers) chars += CHARSETS.numbers
    if (config.symbols) chars += CHARSETS.symbols

    if (config.excludeChars) {
      const excludeSet = new Set(config.excludeChars.split(''))
      chars = chars
        .split('')
        .filter((c) => !excludeSet.has(c))
        .join('')
    }

    if (!chars) return ''
    if (config.length <= 0) return ''

    const array = new Uint32Array(config.length)
    crypto.getRandomValues(array)
    return Array.from(array)
      .map((x) => chars[x % chars.length])
      .join('')
  }

  return { generate }
}
