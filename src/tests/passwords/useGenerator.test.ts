import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  usePasswordGenerator,
  type IPasswordConfig,
} from '~/composables/passwords/useGenerator'

describe('usePasswordGenerator', () => {
  const { generate } = usePasswordGenerator()

  // Deterministic RNG: counter-based fill of the buffer.
  beforeEach(() => {
    let counter = 0
    vi.spyOn(crypto, 'getRandomValues').mockImplementation((array) => {
      if (array instanceof Uint32Array) {
        for (let i = 0; i < array.length; i++) {
          array[i] = counter++
        }
      }
      return array
    })
  })

  describe('standard generation', () => {
    it('generates password of correct length', () => {
      const config: IPasswordConfig = {
        length: 16,
        uppercase: true,
        lowercase: true,
        numbers: true,
        symbols: false,
        excludeChars: '',
        usePattern: false,
        pattern: '',
      }
      expect(generate(config).length).toBe(16)
    })

    it('generates only uppercase when specified', () => {
      const password = generate({
        length: 20,
        uppercase: true,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: false,
        pattern: '',
      })
      expect(password).toMatch(/^[A-Z]+$/)
    })

    it('generates only lowercase when specified', () => {
      const password = generate({
        length: 20,
        uppercase: false,
        lowercase: true,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: false,
        pattern: '',
      })
      expect(password).toMatch(/^[a-z]+$/)
    })

    it('generates only numbers when specified', () => {
      const password = generate({
        length: 20,
        uppercase: false,
        lowercase: false,
        numbers: true,
        symbols: false,
        excludeChars: '',
        usePattern: false,
        pattern: '',
      })
      expect(password).toMatch(/^[0-9]+$/)
    })

    it('generates only symbols when specified', () => {
      const password = generate({
        length: 20,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: true,
        excludeChars: '',
        usePattern: false,
        pattern: '',
      })
      expect(password).toMatch(/^[!@#$%^&*()_+\-=[\]{}|;:,.<>?]+$/)
    })

    it('returns empty string when no character sets selected', () => {
      const password = generate({
        length: 16,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: false,
        pattern: '',
      })
      expect(password).toBe('')
    })

    it('excludes specified characters', () => {
      const password = generate({
        length: 100,
        uppercase: true,
        lowercase: true,
        numbers: true,
        symbols: false,
        excludeChars: 'ABCabc123',
        usePattern: false,
        pattern: '',
      })
      expect(password).not.toMatch(/[ABCabc123]/)
    })
  })

  describe('pattern generation', () => {
    it('generates password matching pattern length', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'Cvccvcc',
      })
      expect(password.length).toBe(7)
    })

    it('uses lowercase consonants for c', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'ccccc',
      })
      expect(password).toMatch(/^[bcdfghjklmnpqrstvwxyz]+$/)
    })

    it('uses uppercase consonants for C', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'CCCCC',
      })
      expect(password).toMatch(/^[BCDFGHJKLMNPQRSTVWXYZ]+$/)
    })

    it('uses lowercase vowels for v', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'vvvvv',
      })
      expect(password).toMatch(/^[aeiou]+$/)
    })

    it('uses uppercase vowels for V', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'VVVVV',
      })
      expect(password).toMatch(/^[AEIOU]+$/)
    })

    it('uses digits for d', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'ddddd',
      })
      expect(password).toMatch(/^[0-9]+$/)
    })

    it('uses all lowercase for a', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'aaaaa',
      })
      expect(password).toMatch(/^[a-z]+$/)
    })

    it('uses all uppercase for A', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'AAAAA',
      })
      expect(password).toMatch(/^[A-Z]+$/)
    })

    it('uses symbols for s', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'sssss',
      })
      expect(password).toMatch(/^[!@#$%^&*()_+\-=[\]{}|;:,.<>?]+$/)
    })

    it('keeps literal characters in pattern', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'X-Y-Z',
      })
      expect(password).toBe('X-Y-Z')
    })

    it('mixes pattern characters and literals', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: 'd-d-d-d',
      })
      expect(password).toMatch(/^\d-\d-\d-\d$/)
    })
  })

  describe('edge cases', () => {
    it('handles zero length', () => {
      const password = generate({
        length: 0,
        uppercase: true,
        lowercase: true,
        numbers: true,
        symbols: true,
        excludeChars: '',
        usePattern: false,
        pattern: '',
      })
      expect(password).toBe('')
    })

    it('handles empty pattern', () => {
      const password = generate({
        length: 0,
        uppercase: false,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: '',
      })
      expect(password).toBe('')
    })

    it('falls back to standard when pattern is null with usePattern true', () => {
      const password = generate({
        length: 10,
        uppercase: true,
        lowercase: false,
        numbers: false,
        symbols: false,
        excludeChars: '',
        usePattern: true,
        pattern: '',
      })
      expect(password.length).toBe(10)
      expect(password).toMatch(/^[A-Z]+$/)
    })
  })
})
