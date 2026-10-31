// The context menus of the file browser (spec 044 FR-017 to FR-023, T050): the menu of entries and
// the menu of the open folder's empty area. The right-click menu (a long press on touch) and the
// selection bar run the same commands. Pure, so `scripts/check-files-clipboard.ts` runs it without
// vue. holzi's own places are read-only (FR-037): their changes stay disabled. An item names its
// keyboard shortcut (FR-017b, `lib/files/shortcuts.ts`).
import type { ShortcutHint } from '../wm/shortcutLabel.ts'

export type FilesCommand =
  | 'open'
  | 'select'
  | 'copy'
  | 'cut'
  | 'paste'
  | 'rename'
  | 'delete'
  | 'newFolder'
  | 'newEncryptedFolder'
  | 'openSystem'
  | 'share'
  /** Spec 071: the folder into the sidebar's Orte, or out of it. */
  | 'pin'
  | 'unpin'
  /** Only in the split view (spec 062): the selection into the other panel's folder. */
  | 'copyToOther'
  | 'moveToOther'

export type FilesMenuItem = {
  id: FilesCommand
  labelKey: string
  disabled?: boolean
  shortcut?: ShortcutHint
}

export type FilesMenuEntry = FilesMenuItem | { separator: true }

export type EntriesMenuInput = {
  /** How many entries the menu acts on. */
  count: number
  /** The one entry is a file (open with the system's app). */
  singleFile: boolean
  /** The open folder is one of holzi's own places. */
  readOnly: boolean
  /** The selection mode is on (touch selects through the menu). */
  selecting: boolean
  /** The system's apps can open files here; not on Android yet (spec 048 research R11). */
  systemApps?: boolean
  /** The one entry may be shared with contacts: of this device, not holzi's own (spec 056). */
  shareable?: boolean
  /** The tab shows two panels: the menu offers copying and moving into the other one (spec 062). */
  split?: boolean
  /** The other panel's folder takes no changes: holzi's own place, or a share read only. */
  readOnlyTarget?: boolean
  /** What the menu offers for the one folder entry: pin it or unpin it (spec 071); none for
   * anything else. */
  pin?: PinCommand
}

export type PinCommand = 'pin' | 'unpin'

/** The keys of the commands that have one (`resolveFilesShortcut`). */
const SHORTCUTS: Partial<Record<FilesCommand, ShortcutHint>> = {
  open: { key: 'Enter' },
  copy: { mod: true, key: 'C' },
  cut: { mod: true, key: 'X' },
  paste: { mod: true, key: 'V' },
  rename: { key: 'F2' },
  delete: { key: 'Delete' },
  newFolder: { mod: true, shift: true, key: 'N' },
  copyToOther: { key: 'F5' },
  moveToOther: { key: 'F6' },
}

const item = (id: FilesCommand, disabled = false): FilesMenuItem => ({
  id,
  labelKey: `files.actions.${id}`,
  disabled,
  shortcut: SHORTCUTS[id],
})

const SEPARATOR = { separator: true } as const

/** The menu of the selected entries (or the one under the pointer). */
export function entriesMenu(input: EntriesMenuInput): FilesMenuEntry[] {
  const single = input.count === 1
  const entries: FilesMenuEntry[] = []
  if (single) entries.push(item('open'))
  if (single && input.singleFile && input.systemApps !== false)
    entries.push(item('openSystem'))
  if (single && input.shareable) entries.push(item('share'))
  if (single && input.pin) entries.push(item(input.pin))
  if (!input.selecting) entries.push(item('select'))
  entries.push(
    SEPARATOR,
    item('copy'),
    item('cut', input.readOnly),
    ...(input.split
      ? [
          item('copyToOther', input.count === 0 || !!input.readOnlyTarget),
          item(
            'moveToOther',
            input.count === 0 || input.readOnly || !!input.readOnlyTarget,
          ),
        ]
      : []),
    SEPARATOR,
    item('rename', input.readOnly || !single),
    item('delete', input.readOnly),
  )
  return entries
}

export type FolderMenuInput = {
  readOnly: boolean
  /** Something is in holzi's clipboard and may go here. */
  canPaste: boolean
  /** An encrypted folder may be created here (spec 048 FR-001, `lib/files/encrypted.ts`). */
  canEncrypt?: boolean
  /** What the menu offers for the open folder (spec 071). */
  pin?: PinCommand
}

/** The menu of the open folder's empty area. */
export function folderMenu(input: FolderMenuInput): FilesMenuEntry[] {
  return [
    item('newFolder', input.readOnly),
    ...(input.canEncrypt ? [item('newEncryptedFolder', input.readOnly)] : []),
    item('paste', input.readOnly || !input.canPaste),
    ...(input.pin ? [SEPARATOR, item(input.pin)] : []),
  ]
}

/** The menu of a pinned entry in the sidebar (spec 071 FR-004). */
export function pinMenu(): FilesMenuEntry[] {
  return [item('unpin')]
}
