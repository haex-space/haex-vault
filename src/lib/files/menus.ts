// The context menus of the file browser (spec 044 FR-017 to FR-023, T050): the menu of entries and
// the menu of the open folder's empty area. The right-click menu (a long press on touch) and the
// selection bar run the same commands. Pure, so `scripts/check-files-clipboard.ts` runs it without
// vue. holzi's own places are read-only (FR-037): their changes stay disabled.

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

export type FilesMenuItem = {
  id: FilesCommand
  labelKey: string
  disabled?: boolean
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
}

const item = (id: FilesCommand, disabled = false): FilesMenuItem => ({
  id,
  labelKey: `files.actions.${id}`,
  disabled,
})

const SEPARATOR = { separator: true } as const

/** The menu of the selected entries (or the one under the pointer). */
export function entriesMenu(input: EntriesMenuInput): FilesMenuEntry[] {
  const single = input.count === 1
  const entries: FilesMenuEntry[] = []
  if (single) entries.push(item('open'))
  if (single && input.singleFile && input.systemApps !== false)
    entries.push(item('openSystem'))
  if (!input.selecting) entries.push(item('select'))
  entries.push(
    SEPARATOR,
    item('copy'),
    item('cut', input.readOnly),
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
}

/** The menu of the open folder's empty area. */
export function folderMenu(input: FolderMenuInput): FilesMenuEntry[] {
  return [
    item('newFolder', input.readOnly),
    ...(input.canEncrypt ? [item('newEncryptedFolder', input.readOnly)] : []),
    item('paste', input.readOnly || !input.canPaste),
  ]
}
