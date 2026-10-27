// Keyboard shortcuts of the file browser (spec 044 FR-017b): a key event becomes a command, a move of
// the focus through the open folder, or a letter of type-to-jump. Ctrl on Linux and Windows, Cmd on
// macOS. Nothing acts while the focus is in a text field, in a dialog (the viewer is one) or on
// selected text, and the chords of the window manager (Alt+Arrow left and right: back and forward)
// are never taken. Enter and Space open the focused entry as a click on its row does
// (`FolderView.vue`). Fixed defaults, as in the password manager. Pure, so
// `scripts/check-files-shortcuts.ts` runs it without vue.
import { isEditableTarget, type Platform } from '../wm/keybindings.ts'
import { clickEntry, type Selection } from './clipboard.ts'

export type FilesKeyCommand =
  | 'selectAll'
  | 'clearSelection'
  | 'copy'
  | 'cut'
  | 'paste'
  | 'delete'
  | 'rename'
  | 'newFolder'
  | 'up'
  | 'focusSearch'
  | 'reload'
  | 'toggleHidden'
  /** Only in the split view (spec 062): Tab makes the other panel the active one, F5 copies the
   * selection into the other panel's folder, F6 moves it. */
  | 'switchPanel'
  | 'copyToOther'
  | 'moveToOther'

/** Where a move takes the focus: the next or previous entry, the entry below or above in the grid,
 * the first or the last. */
export type MoveTarget =
  'next' | 'previous' | 'below' | 'above' | 'first' | 'last'

export type FilesKeyAction =
  | { kind: 'command'; command: FilesKeyCommand }
  /** With `extend` the selection grows from its anchor to the entry (Shift). */
  | { kind: 'move'; to: MoveTarget; extend: boolean }
  | { kind: 'type'; text: string }

export type FilesKeyEvent = {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  shiftKey: boolean
  altKey: boolean
  target: unknown
  platform: Platform
  /** Whether the focus is in the open folder's list (everything that acts on entries). */
  inList: boolean
  /** Whether the event comes from inside an open dialog or menu. */
  inDialog: boolean
  /** Whether text is selected in the document. */
  textSelected: boolean
  /** The folder shows as a grid: Left and Right move along a row, Up and Down between rows. */
  grid: boolean
  /** The tab shows two panels (spec 062): Tab, F5 and F6 then change their meaning. */
  split: boolean
  /** Type-to-jump has letters pending: a Space then belongs to the name instead of opening. */
  typing: boolean
}

/** How long type-to-jump waits for the next letter before it starts a new name. */
export const TYPE_PAUSE_MS = 1000

const command = (name: FilesKeyCommand): FilesKeyAction => ({
  kind: 'command',
  command: name,
})

const MOD_CHORDS: Record<string, FilesKeyCommand> = {
  a: 'selectAll',
  c: 'copy',
  x: 'cut',
  v: 'paste',
  f: 'focusSearch',
  r: 'reload',
  h: 'toggleHidden',
}

function arrow(key: string, grid: boolean): MoveTarget | null {
  switch (key) {
    case 'ArrowDown':
      return grid ? 'below' : 'next'
    case 'ArrowUp':
      return grid ? 'above' : 'previous'
    case 'ArrowRight':
      return grid ? 'next' : null
    case 'ArrowLeft':
      return grid ? 'previous' : null
    case 'Home':
      return 'first'
    case 'End':
      return 'last'
    default:
      return null
  }
}

function action(event: FilesKeyEvent): FilesKeyAction | null {
  const mac = event.platform === 'mac'
  const mod = mac ? event.metaKey : event.ctrlKey
  const other = mac ? event.ctrlKey : event.metaKey
  if (other) return null
  if (event.altKey) {
    return !mod && !event.shiftKey && event.key === 'ArrowUp'
      ? command('up')
      : null
  }
  if (mod) {
    if (mac && event.key === 'ArrowUp' && !event.shiftKey) return command('up')
    if (mac && event.key === 'Backspace' && !event.shiftKey)
      return command('delete')
    const letter = event.key.toLowerCase()
    if (event.shiftKey) return letter === 'n' ? command('newFolder') : null
    const found = MOD_CHORDS[letter]
    return found ? command(found) : null
  }
  switch (event.key) {
    case 'F2':
      return event.shiftKey ? null : command('rename')
    // In the split view F5 copies into the other panel; reloading stays on Ctrl+R (spec 062).
    case 'F5':
      if (event.shiftKey) return null
      return command(event.split ? 'copyToOther' : 'reload')
    case 'F6':
      return event.split && !event.shiftKey ? command('moveToOther') : null
    // Shift+Tab stays the browser's: it leaves the list backwards, so the keyboard is never caught.
    case 'Tab':
      return event.split && !event.shiftKey ? command('switchPanel') : null
    case 'Delete':
      return event.shiftKey ? null : command('delete')
    case 'Backspace':
      return mac && !event.shiftKey ? command('delete') : null
    case 'Escape':
      return command('clearSelection')
    case ' ':
      return event.typing ? { kind: 'type', text: ' ' } : null
  }
  const to = arrow(event.key, event.grid)
  if (to) return { kind: 'move', to, extend: event.shiftKey }
  // One printable character: a letter of a name (Shift gives capitals).
  if ([...event.key].length === 1) return { kind: 'type', text: event.key }
  return null
}

/** The commands that act on the open folder, not on its entries: they work anywhere in the frame. */
const FRAME_COMMANDS = new Set<FilesKeyCommand>([
  'clearSelection',
  'paste',
  'newFolder',
  'up',
  'focusSearch',
  'reload',
  'toggleHidden',
])

/** The action for a key event, or `null` when the event is not one or must be left alone. */
export function resolveFilesShortcut(
  event: FilesKeyEvent,
): FilesKeyAction | null {
  if (event.inDialog || event.textSelected) return null
  if (isEditableTarget(event.target)) return null
  const found = action(event)
  if (found === null) return null
  const inFrame = found.kind === 'command' && FRAME_COMMANDS.has(found.command)
  if (!inFrame && !event.inList) return null
  return found
}

/**
 * The index a move takes the focus to, from `current` (-1: no entry has it) among `count` entries
 * in rows of `columns` (1 in the list). Down from the last full row goes to the last entry, as the
 * row below it is shorter; at an edge the focus stays.
 */
export function moveIndex(
  current: number,
  to: MoveTarget,
  count: number,
  columns: number,
): number {
  if (count === 0) return -1
  const last = count - 1
  if (to === 'first') return 0
  if (to === 'last') return last
  if (current < 0) return 0
  switch (to) {
    case 'next':
      return Math.min(current + 1, last)
    case 'previous':
      return Math.max(current - 1, 0)
    case 'below':
      if (current + columns <= last) return current + columns
      return Math.floor(current / columns) < Math.floor(last / columns)
        ? last
        : current
    case 'above':
      return current - columns >= 0 ? current - columns : current
  }
}

/**
 * The selection after the keyboard moved the focus to `path`: that entry alone, or with `extend`
 * everything from the anchor to it in the folder's `order` (as a Shift click).
 */
export function keyboardSelection(
  selection: Selection,
  path: string,
  order: readonly string[],
  extend: boolean,
): Selection {
  if (!extend || selection.anchor === null)
    return { paths: [path], anchor: path }
  return clickEntry(selection, path, order, {
    toggle: false,
    range: true,
    touch: false,
  }).selection
}

/**
 * Type-to-jump: the index of the first name after `current` that starts with `text`, ignoring
 * case, wrapping at the end; -1 when none does. A longer `text` may stay on the current entry (it
 * still matches as the name grows); one letter typed again and again steps through the names that
 * start with it.
 */
export function typeAhead(
  names: readonly string[],
  text: string,
  current: number,
): number {
  if (names.length === 0 || text === '') return -1
  const lower = text.toLocaleLowerCase()
  const first = [...lower][0] ?? ''
  const repeated = [...lower].every((char) => char === first)
  const prefix = repeated ? first : lower
  const start = repeated ? current + 1 : Math.max(current, 0)
  for (let i = 0; i < names.length; i++) {
    const index = (start + i) % names.length
    if (names[index]?.toLocaleLowerCase().startsWith(prefix)) return index
  }
  return -1
}
