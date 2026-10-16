// How a menu names the keyboard shortcut of an action (spec 036 FR-016, spec 044 FR-017b): the key
// names of the interface language on Linux and Windows (`Strg+C`), the symbols on macOS (`⌘C`). One
// label for every app's menus. Pure, so the check scripts run it without vue.
import type { Platform } from './keybindings.ts'

/** A shortcut as the menus show it next to an action: a letter, a function key or a named key. */
export type ShortcutHint = {
  mod?: boolean
  shift?: boolean
  key: string
}

/** The words for the keys in the interface language (`Strg`/`Ctrl`, …); macOS uses symbols. */
export type KeyNames = {
  mod: string
  shift: string
  Delete: string
  Enter: string
}

const MAC_KEYS: Record<string, string> = { Delete: '⌫', Enter: '↩' }

export function shortcutLabel(
  hint: ShortcutHint,
  platform: Platform,
  names: KeyNames,
): string {
  if (platform === 'mac') {
    const key = MAC_KEYS[hint.key] ?? hint.key
    return `${hint.shift ? '⇧' : ''}${hint.mod ? '⌘' : ''}${key}`
  }
  const key =
    hint.key === 'Delete' || hint.key === 'Enter' ? names[hint.key] : hint.key
  return [hint.mod ? names.mod : null, hint.shift ? names.shift : null, key]
    .filter((part) => part !== null)
    .join('+')
}
