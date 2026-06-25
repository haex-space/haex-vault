import type { MaybeRefOrGetter } from 'vue'

/**
 * Shared icon styling for password-group rows (`list/folder.vue` +
 * `tree/item.vue`).
 *
 * Both render a colored swatch behind the folder glyph: the swatch tints to
 * `group.color` and the glyph itself flips between dark and light so it stays
 * legible on both pale and dark swatches. The luminance breakpoint (0.6) is
 * the same one the editor's color-picker preview uses.
 *
 * Malformed colour strings (anything other than `#RRGGBB` — wrong length,
 * non-hex digits) yield `undefined` so an inline style never becomes
 * `color: 'NaN'`. The component falls back to its own glyph-on-bg rule in
 * that case.
 */
interface GroupLike {
  color: string | null
}

export function usePasswordsGroupStyles(group: MaybeRefOrGetter<GroupLike>) {
  const backgroundStyle = computed(() => {
    const { color } = toValue(group)
    return color ? { backgroundColor: color } : undefined
  })

  const glyphStyle = computed(() => {
    const { color } = toValue(group)
    if (!color) return { color: 'rgb(var(--ui-primary))' }
    const hex = color.replace('#', '')
    if (hex.length !== 6) return undefined
    const r = parseInt(hex.slice(0, 2), 16)
    const g = parseInt(hex.slice(2, 4), 16)
    const b = parseInt(hex.slice(4, 6), 16)
    if (Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b)) return undefined
    // Rec. 601 luma weights. 0.6 chosen empirically — see #passwords:colors.
    const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
    return { color: luminance > 0.6 ? '#111827' : '#ffffff' }
  })

  return { backgroundStyle, glyphStyle }
}
