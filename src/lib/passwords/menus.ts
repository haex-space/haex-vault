// The context menus of the password manager (spec 036, FR-018, FR-019, research R10): which actions
// the menu of an entry, a folder, a folder of the sidebar, the empty area and the trash of the
// sidebar offers. The right-click menu and the menu button of a row render the same list, and the
// selection bar runs the same commands, so they cannot drift apart. Pure, so
// `scripts/check-passwords-menus.ts` runs it without vue.
import type { ShortcutHint } from '../wm/shortcutLabel.ts'

export type MenuKind = 'entry' | 'folder' | 'treeFolder' | 'empty' | 'trashNode'

export type MenuCommand =
  | 'open'
  | 'edit'
  | 'newSubfolder'
  | 'moveUp'
  | 'moveDown'
  | 'copyUsername'
  | 'copyPassword'
  | 'cut'
  | 'copy'
  | 'paste'
  | 'delete'
  | 'restore'
  | 'deleteForGood'
  | 'newEntry'
  | 'newFolder'
  | 'emptyTrash'

export type MenuItem = {
  id: MenuCommand
  labelKey: string
  shortcut?: ShortcutHint
  disabled?: boolean
}

export type MenuEntry = MenuItem | { separator: true }

export type MenuInput = {
  kind: MenuKind
  /** The row lies in the trash. */
  inTrash?: boolean
  /** The Ablage holds something. */
  ablageFilled?: boolean
  /** The empty area belongs to a place that takes a paste (not trash, search or a tag view). */
  pasteHere?: boolean
  /** How many rows the menu applies to: a right click inside a selection means all of it. */
  selectionSize?: number
  hasUsername?: boolean
  hasPassword?: boolean
  /** Kopieren needs the copy dialog of stage 3; until then it is not offered. */
  copyAvailable?: boolean
  canMoveUp?: boolean
  canMoveDown?: boolean
  trashEmpty?: boolean
}

const SEPARATOR = { separator: true } as const

function item(
  id: MenuCommand,
  extra: Omit<MenuItem, 'id' | 'labelKey'> = {},
): MenuItem {
  return { id, labelKey: `passwords.menu.${id}`, ...extra }
}

/** Drops separators at the start, at the end and next to each other. */
function tidy(entries: readonly MenuEntry[]): MenuEntry[] {
  const out: MenuEntry[] = []
  for (const entry of entries) {
    const last = out.at(-1)
    if ('separator' in entry && (last === undefined || 'separator' in last))
      continue
    out.push(entry)
  }
  while (out.length > 0 && 'separator' in out.at(-1)!) out.pop()
  return out
}

/** Ausschneiden, Kopieren (once it exists) and, for a folder, Einfügen into it. */
function transfer(input: MenuInput, pasteInto: boolean): MenuEntry[] {
  return [
    item('cut', { shortcut: { mod: true, key: 'X' } }),
    ...(input.copyAvailable
      ? [item('copy', { shortcut: { mod: true, key: 'C' } })]
      : []),
    ...(pasteInto && input.ablageFilled
      ? [item('paste', { shortcut: { mod: true, key: 'V' } })]
      : []),
  ]
}

const DELETE = () => item('delete', { shortcut: { key: 'Delete' } })

export function buildMenu(input: MenuInput): MenuEntry[] {
  const many = (input.selectionSize ?? 1) > 1
  switch (input.kind) {
    case 'trashNode':
      return [item('emptyTrash', { disabled: input.trashEmpty === true })]
    case 'empty':
      return tidy([
        item('newEntry'),
        item('newFolder'),
        SEPARATOR,
        ...(input.ablageFilled && input.pasteHere
          ? [item('paste', { shortcut: { mod: true, key: 'V' } })]
          : []),
      ])
    case 'entry':
    case 'folder':
    case 'treeFolder':
      break
  }
  if (input.inTrash) return [item('restore'), item('deleteForGood')]
  if (many) return tidy([...transfer(input, false), SEPARATOR, DELETE()])
  if (input.kind === 'entry') {
    return tidy([
      item('open', { shortcut: { key: 'Enter' } }),
      SEPARATOR,
      item('copyUsername', {
        shortcut: { mod: true, key: 'B' },
        disabled: input.hasUsername !== true,
      }),
      item('copyPassword', {
        shortcut: { mod: true, shift: true, key: 'C' },
        disabled: input.hasPassword !== true,
      }),
      SEPARATOR,
      ...transfer(input, false),
      SEPARATOR,
      DELETE(),
    ])
  }
  return tidy([
    item('open'),
    item('edit'),
    item('newSubfolder'),
    ...(input.kind === 'treeFolder'
      ? [
          item('moveUp', { disabled: input.canMoveUp === false }),
          item('moveDown', { disabled: input.canMoveDown === false }),
        ]
      : []),
    SEPARATOR,
    ...transfer(input, true),
    SEPARATOR,
    DELETE(),
  ])
}
