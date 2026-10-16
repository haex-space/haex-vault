// Keyboard shortcuts of the password manager's list (spec 036, FR-016, research R10): a key event
// becomes a list command. Ctrl on Linux and Windows, Cmd on macOS. Nothing acts while the focus is
// in a text field, in a dialog or on selected text (there the usual behaviour holds, such as copying
// the selected text), and the chords of the window manager (Alt+Arrow, Cmd+Bracket) are never
// taken. Fixed defaults; rebinding is a later spec, as for the wm shortcuts. Pure, so
// `scripts/check-passwords-shortcuts.ts` runs it without vue.
import { isEditableTarget, type Platform } from '../wm/keybindings.ts'

export type ListCommand =
  | 'selectAll'
  | 'clearSelection'
  | 'cut'
  | 'copy'
  | 'paste'
  | 'delete'
  | 'open'
  | 'focusSearch'
  | 'focusNext'
  | 'focusPrevious'
  | 'extendNext'
  | 'extendPrevious'
  | 'copyUsername'
  | 'copyPassword'

export type ShortcutEvent = {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  shiftKey: boolean
  altKey: boolean
  target: unknown
  platform: Platform
  /** Whether the focus is on a row of the list (the keys that act on one row need it). */
  inList: boolean
  /** Whether the event comes from inside an open dialog or menu. */
  inDialog: boolean
  /** Whether text is selected in the document. */
  textSelected: boolean
}

/** Commands that act on the focused row and so need the focus in the list. */
const ROW_COMMANDS = new Set<ListCommand>([
  'delete',
  'open',
  'focusNext',
  'focusPrevious',
  'extendNext',
  'extendPrevious',
])

const MOD_CHORDS: Record<string, ListCommand> = {
  a: 'selectAll',
  x: 'cut',
  c: 'copy',
  v: 'paste',
  b: 'copyUsername',
  f: 'focusSearch',
}

function command(event: ShortcutEvent): ListCommand | null {
  if (event.altKey) return null
  const mac = event.platform === 'mac'
  const mod = mac ? event.metaKey : event.ctrlKey
  const other = mac ? event.ctrlKey : event.metaKey
  if (other) return null
  if (mod) {
    const letter = event.key.toLowerCase()
    if (event.shiftKey) return letter === 'c' ? 'copyPassword' : null
    return MOD_CHORDS[letter] ?? null
  }
  switch (event.key) {
    case 'Delete':
      return event.shiftKey ? null : 'delete'
    case 'Backspace':
      return mac && !event.shiftKey ? 'delete' : null
    case 'Enter':
      return event.shiftKey ? null : 'open'
    case 'Escape':
      return 'clearSelection'
    case 'ArrowDown':
      return event.shiftKey ? 'extendNext' : 'focusNext'
    case 'ArrowUp':
      return event.shiftKey ? 'extendPrevious' : 'focusPrevious'
    default:
      return null
  }
}

/** The list command for a key event, or `null` when the event is not one or must be left alone. */
export function resolveListShortcut(event: ShortcutEvent): ListCommand | null {
  if (event.inDialog || event.textSelected) return null
  if (isEditableTarget(event.target)) return null
  const found = command(event)
  if (found === null) return null
  if (ROW_COMMANDS.has(found) && !event.inList) return null
  return found
}
