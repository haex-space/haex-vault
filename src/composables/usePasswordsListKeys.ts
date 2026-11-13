import type { ComputedRef, Ref } from 'vue'
import type { ListCommand } from '~/lib/passwords/shortcuts'
import type { CopyRequest } from '~/composables/usePasswordsActions'
import { headerControl } from '~/lib/wm/headerDom'

/**
 * The keyboard of the password manager's list (spec 036, US4, FR-016): one tab stop that the arrow
 * keys move (with Shift they extend the selection), and the list commands the frame hands over
 * (`usePasswordsShortcuts.ts`). A command on "the selection" without one means the focused row.
 * After a change of place the focus goes back into the list, so the shortcuts keep working.
 */
export function usePasswordsListKeys(options: {
  /** The scroll area of the list; it takes the focus when there is no row. */
  area: Readonly<Ref<HTMLElement | null>>
  /** The element around the rows. */
  list: Readonly<Ref<HTMLElement | null>>
  /** The rows as the list shows them. */
  visibleIds: ComputedRef<string[]>
  isFolder: (id: string) => boolean
  open: (id: string) => void
  askDelete: (ids: readonly string[]) => void
  /** Where a paste goes (`null` is the top level), `undefined` where none goes. */
  pasteTarget: ComputedRef<string | null | undefined>
  /** Opens the copy dialog of the window for a paste of a copied Ablage. */
  openCopy: (request: CopyRequest) => void
}) {
  const { area, list, visibleIds, isFolder } = options
  const selection = usePasswordsSelectionStore()
  const clipboard = usePasswordsClipboardStore()
  const actions = usePasswordsActions()

  const focusedId = ref<string | null>(null)
  /** The one row the Tab key reaches: the focused one while it shows, else the first. */
  const tabStopId = computed(() =>
    focusedId.value !== null && visibleIds.value.includes(focusedId.value)
      ? focusedId.value
      : (visibleIds.value[0] ?? null),
  )

  function rowElement(id: string): HTMLElement | null {
    return (
      list.value?.querySelector<HTMLElement>(
        `[data-row-id="${CSS.escape(id)}"]`,
      ) ?? null
    )
  }

  function focusRow(id: string) {
    focusedId.value = id
    void nextTick(() => rowElement(id)?.focus())
  }

  /** After a change of place the focus is often gone (the row or the crumb that was clicked is no
   * longer there); it goes to the first row, or the list area. A focus elsewhere is left alone. */
  function keepFocus() {
    void nextTick(() => {
      const active = document.activeElement
      if (active && active !== document.body && active.isConnected) return
      const first = tabStopId.value
      ;((first ? rowElement(first) : null) ?? area.value)?.focus({
        preventScroll: true,
      })
    })
  }

  // A focused row can disappear without a change of place (deleted, moved away, or a dialog gives
  // the focus back to a button that is gone); the focus then falls to the page and the shortcuts
  // see nothing. While the focus was last in this window, it comes back to the list. Dialogs and
  // menus live outside the app root (portals), so focus or clicks there change nothing.
  let ownsFocus = false
  function track(event: Event) {
    const target = event.target
    const frame = area.value?.closest('[data-passwords-app]')
    if (!(target instanceof Node) || !frame) return
    let appRoot: Element = frame
    while (appRoot.parentElement && appRoot.parentElement !== document.body)
      appRoot = appRoot.parentElement
    if (frame.contains(target)) ownsFocus = true
    else if (appRoot.contains(target)) ownsFocus = false
  }
  useEventListener(document, 'focusin', track, { capture: true })
  useEventListener(document, 'pointerdown', track, { capture: true })
  const activeElement = useActiveElement({ triggerOnRemoval: true })
  watch(activeElement, (element) => {
    if (ownsFocus && (!element || element === document.body)) keepFocus()
  })

  /** The row that has the focus, if the focus is in the list. */
  function focusedRow(): string | null {
    const active = document.activeElement
    if (!(active instanceof HTMLElement) || !list.value?.contains(active))
      return null
    return active.closest('[data-row-id]')?.getAttribute('data-row-id') ?? null
  }

  function step(delta: 1 | -1, extend: boolean) {
    const ids = visibleIds.value
    const current = focusedRow() ?? tabStopId.value
    if (current === null) return
    const next = ids[ids.indexOf(current) + delta]
    if (!next) return
    if (extend) {
      // The range grows from the row the keyboard started on.
      if (!selection.active) selection.toggleId(current)
      selection.rangeTo(ids, next)
    }
    focusRow(next)
  }

  /** What a command on "the selection" means without one: the focused row. */
  function commandIds(): string[] {
    if (selection.active) return [...selection.ids]
    const focused = focusedRow()
    return focused ? [focused] : []
  }

  /** The entry whose username or password a shortcut copies: the focused or the only selected one. */
  function singleEntry(): string | null {
    const focused = focusedRow()
    if (focused && !isFolder(focused)) return focused
    const only = selection.count === 1 ? selection.ids[0] : undefined
    return only && !isFolder(only) ? only : null
  }

  function onCommand(command: ListCommand): boolean {
    switch (command) {
      case 'selectAll':
        // Consumed even on an empty list, so the browser does not select the page's text.
        selection.selectEvery(visibleIds.value)
        return true
      case 'clearSelection':
        if (!selection.active) return false
        selection.clear()
        return true
      case 'cut': {
        const ids = commandIds()
        if (ids.length === 0) return false
        actions.cut(ids)
        selection.clear()
        return true
      }
      case 'copy': {
        // Into the Ablage; nothing goes to the system clipboard (FR-021).
        const ids = commandIds()
        if (ids.length === 0) return false
        actions.copy(ids)
        selection.clear()
        return true
      }
      case 'paste': {
        const target = options.pasteTarget.value
        if (!clipboard.filled || target === undefined) return false
        void actions.pasteAsync(target, options.openCopy)
        return true
      }
      case 'delete': {
        const ids = commandIds()
        options.askDelete(ids)
        return ids.length > 0
      }
      case 'open': {
        const focused = focusedRow()
        if (!focused) return false
        options.open(focused)
        return true
      }
      case 'focusSearch': {
        const search = headerControl<HTMLInputElement>(
          area.value,
          'passwords-search',
        )
        search?.focus()
        return Boolean(search)
      }
      case 'focusNext':
      case 'focusPrevious':
      case 'extendNext':
      case 'extendPrevious':
        step(
          command === 'focusNext' || command === 'extendNext' ? 1 : -1,
          command === 'extendNext' || command === 'extendPrevious',
        )
        return true
      case 'copyUsername':
      case 'copyPassword': {
        const id = singleEntry()
        if (!id) return false
        void actions.copyValueAsync(
          id,
          command === 'copyUsername' ? 'username' : 'password',
        )
        return true
      }
    }
  }
  useListCommands(onCommand)

  return { focusedId, tabStopId, keepFocus }
}
