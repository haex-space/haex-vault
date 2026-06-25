import { ref } from 'vue'
import { describe, expect, it } from 'vitest'
import { usePasswordsGroupStyles } from '~/composables/passwords/useGroupStyles'

describe('usePasswordsGroupStyles', () => {
  describe('backgroundStyle', () => {
    it('returns the colour as backgroundColor when set', () => {
      const group = ref({ color: '#ff0000' })
      const { backgroundStyle } = usePasswordsGroupStyles(group)
      expect(backgroundStyle.value).toEqual({ backgroundColor: '#ff0000' })
    })

    it('returns undefined when colour is null', () => {
      const group = ref({ color: null })
      const { backgroundStyle } = usePasswordsGroupStyles(group)
      expect(backgroundStyle.value).toBeUndefined()
    })
  })

  describe('glyphStyle', () => {
    it('falls back to the primary CSS variable when colour is null', () => {
      const group = ref({ color: null })
      const { glyphStyle } = usePasswordsGroupStyles(group)
      expect(glyphStyle.value).toEqual({ color: 'rgb(var(--ui-primary))' })
    })

    it('uses dark glyph (#111827) on a bright swatch', () => {
      // Bright yellow — high luminance via the 0.587·G term
      const group = ref({ color: '#ffff00' })
      const { glyphStyle } = usePasswordsGroupStyles(group)
      expect(glyphStyle.value).toEqual({ color: '#111827' })
    })

    it('uses light glyph (#ffffff) on a dark swatch', () => {
      const group = ref({ color: '#003300' })
      const { glyphStyle } = usePasswordsGroupStyles(group)
      expect(glyphStyle.value).toEqual({ color: '#ffffff' })
    })

    it('returns undefined for hex strings with wrong length', () => {
      // Common malformed inputs that previously could leak into the style.
      const styles3 = usePasswordsGroupStyles(ref({ color: '#abc' }))
      expect(styles3.glyphStyle.value).toBeUndefined()
      const styles8 = usePasswordsGroupStyles(ref({ color: '#abcdef12' }))
      expect(styles8.glyphStyle.value).toBeUndefined()
    })

    it('returns undefined for non-hex characters', () => {
      // Was returning `color: 'NaN'` before the explicit guard.
      const group = ref({ color: '#zzzzzz' })
      const { glyphStyle } = usePasswordsGroupStyles(group)
      expect(glyphStyle.value).toBeUndefined()
    })

    it('accepts colour without leading #', () => {
      const group = ref({ color: 'ff0000' })
      const { glyphStyle } = usePasswordsGroupStyles(group)
      // Pure red — luminance 0.299·1 = 0.299 < 0.6 → light glyph
      expect(glyphStyle.value).toEqual({ color: '#ffffff' })
    })

    it('reacts to colour changes via the ref', () => {
      const group = ref<{ color: string | null }>({ color: '#ffff00' })
      const { glyphStyle } = usePasswordsGroupStyles(group)
      expect(glyphStyle.value).toEqual({ color: '#111827' })
      group.value = { color: '#003300' }
      expect(glyphStyle.value).toEqual({ color: '#ffffff' })
    })
  })
})
