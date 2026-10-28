import type { Entry } from '@bindings/Entry'
import type { Selection } from '~/lib/files/clipboard'
import type { FilesCommand } from '~/lib/files/menus'
import {
  type FilesKeyCommand,
  keyboardSelection,
  moveIndex,
  resolveFilesShortcut,
  TYPE_PAUSE_MS,
  typeAhead,
} from '~/lib/files/shortcuts'
import { detectPlatform } from '~/lib/wm/keybindings'

/** What the folder view offers the keyboard (`FolderView.vue`). */
export type FolderKeys = {
  focusEntry: (path: string) => Promise<void>
  focus: () => void
  rowLength: number
}

/** The commands that act on entries: on the selection, or on the focused entry without one. */
const ENTRY_COMMANDS = [
  'copy',
  'cut',
  'delete',
  'rename',
  'copyToOther',
  'moveToOther',
] as const

const isEntryCommand = (
  command: FilesKeyCommand,
): command is (typeof ENTRY_COMMANDS)[number] =>
  (ENTRY_COMMANDS as readonly string[]).includes(command)

/**
 * The keyboard shortcuts of a file browser panel (spec 044 FR-017b, `lib/files/shortcuts.ts`): the
 * panel (`Panel.vue`) listens on its root, and the frame (`FilesApp.vue`) hands it the keys of the
 * toolbar and the sidebar (`useFilesSplit`), so they act with the focus anywhere in the window. The
 * window manager takes its chords in the capture phase before this (`useWmKeyboard`).
 */
export function useFilesKeyboard(options: {
  root: Readonly<Ref<HTMLElement | null>>
  folderView: Readonly<Ref<FolderKeys | null>>
  shown: Ref<Entry[]>
  selection: Ref<Selection>
  grid: Ref<boolean>
  /** The name as the list shows it (type-to-jump matches it). */
  nameOf: (entry: Entry) => string
  /** Runs a command under the menu's rules (`useFilesManage().runKey`). */
  runKey: (command: FilesCommand, targets: Selection | null) => boolean
  clearSelection: () => void
  /** The tab shows two panels: Tab, F5 and F6 change their meaning (spec 062). */
  split: Ref<boolean>
  switchPanel: () => void
  up: () => void
  reload: () => void
  toggleHidden: () => void
}) {
  const { root, folderView, shown, selection, grid, nameOf, runKey } = options
  const platform = detectPlatform(
    navigator as Navigator & { userAgentData?: { platform?: string } },
  )

  let typed = ''
  let typedAt = 0
  const typing = () => typed !== '' && Date.now() - typedAt < TYPE_PAUSE_MS

  /** The entry whose row has the focus, else the selection's anchor. */
  function focusedPath(target: Element | null): string | null {
    const row = target?.closest('[data-path]')?.getAttribute('data-path')
    return row ?? selection.value.anchor
  }

  const order = () => shown.value.map((entry) => entry.path)

  function focus(index: number, extend: boolean) {
    const entry = shown.value[index]
    if (!entry) return
    selection.value = keyboardSelection(
      selection.value,
      entry.path,
      order(),
      extend,
    )
    void folderView.value?.focusEntry(entry.path)
  }

  function focusSearch() {
    const field = root.value?.querySelector('[data-testid="files-search"]')
    const input = field?.matches('input')
      ? field
      : field?.querySelector('input')
    if (input instanceof HTMLElement) input.focus()
  }

  /** Runs `command`; whether it did something (the key is then consumed). */
  function runCommand(command: FilesKeyCommand, current: string | null) {
    if (isEntryCommand(command)) {
      const targets = selection.value.paths.length
        ? selection.value
        : current
          ? { paths: [current], anchor: current }
          : null
      return targets !== null && runKey(command, targets)
    }
    switch (command) {
      case 'selectAll':
        selection.value = {
          paths: order(),
          anchor: selection.value.anchor ?? order()[0] ?? null,
        }
        return true
      case 'clearSelection':
        if (!selection.value.paths.length) return false
        options.clearSelection()
        return true
      case 'paste':
      case 'newFolder':
        return runKey(command, null)
      case 'switchPanel':
        options.switchPanel()
        return true
      case 'up':
        options.up()
        return true
      case 'focusSearch':
        focusSearch()
        return true
      case 'reload':
        options.reload()
        return true
      case 'toggleHidden':
        options.toggleHidden()
        return true
    }
    return false
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented) return
    const target = event.target instanceof Element ? event.target : null
    const action = resolveFilesShortcut({
      key: event.key,
      ctrlKey: event.ctrlKey,
      metaKey: event.metaKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      target,
      platform,
      inList: Boolean(target?.closest('[data-files-list]')),
      inDialog: Boolean(
        target?.closest('[role="dialog"], [role="alertdialog"], [role="menu"]'),
      ),
      textSelected: (window.getSelection()?.toString() ?? '') !== '',
      split: options.split.value,
      grid: grid.value,
      typing: typing(),
    })
    if (!action) return
    // Any other key ends the name being typed: the next letter starts a new one.
    if (action.kind !== 'type') typed = ''
    const current = focusedPath(target)
    const index = current ? order().indexOf(current) : -1
    if (action.kind === 'move') {
      const columns = folderView.value?.rowLength ?? 1
      focus(
        moveIndex(index, action.to, shown.value.length, columns),
        action.extend,
      )
      event.preventDefault()
      return
    }
    if (action.kind === 'type') {
      typed = typing() ? typed + action.text : action.text
      typedAt = Date.now()
      const found = typeAhead(shown.value.map(nameOf), typed, index)
      if (found >= 0) focus(found, false)
      event.preventDefault()
      return
    }
    if (
      runCommand(action.command, current) ||
      (options.split.value &&
        (action.command === 'copyToOther' || action.command === 'moveToOther'))
    ) {
      event.preventDefault()
    }
  }

  return { onKeydown }
}
