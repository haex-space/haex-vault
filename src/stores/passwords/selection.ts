export type SelectionEntryType = 'item' | 'group'
export type ClipboardMode = 'copy' | 'cut' | null

export interface SelectionEntry {
  id: string
  type: SelectionEntryType
}

export const usePasswordsSelectionStore = defineStore(
  'passwordsSelectionStore',
  () => {
    const selectedIds = ref<Set<string>>(new Set())
    const isSelectionMode = ref(false)
    const lastAnchorId = ref<string | null>(null)

    // Desktop-only: single-click highlights an item without entering selection
    // mode (no toolbar). Cleared whenever real selection mode activates.
    const desktopFocusId = ref<string | null>(null)

    const clipboardEntries = ref<SelectionEntry[]>([])
    const clipboardMode = ref<ClipboardMode>(null)

    const selectedCount = computed(() => selectedIds.value.size)
    const hasClipboard = computed(() => clipboardEntries.value.length > 0)

    const resolveEntryType = (id: string): SelectionEntryType => {
      const groupsStore = usePasswordsGroupsStore()
      return groupsStore.groupById.get(id) ? 'group' : 'item'
    }

    const selectedEntries = computed<SelectionEntry[]>(() =>
      Array.from(selectedIds.value).map((id) => ({
        id,
        type: resolveEntryType(id),
      })),
    )

    const hasFoldersInSelection = computed(() =>
      selectedEntries.value.some((entry) => entry.type === 'group'),
    )
    const hasItemsInSelection = computed(() =>
      selectedEntries.value.some((entry) => entry.type === 'item'),
    )
    const isItemsOnly = computed(
      () =>
        selectedCount.value > 0 &&
        hasItemsInSelection.value &&
        !hasFoldersInSelection.value,
    )

    const isSelected = (id: string): boolean => selectedIds.value.has(id)

    const setMode = (active: boolean) => {
      isSelectionMode.value = active
      if (!active) {
        selectedIds.value = new Set()
        lastAnchorId.value = null
        desktopFocusId.value = null
      }
    }

    const setDesktopFocus = (id: string) => {
      desktopFocusId.value = id
    }

    const enterSelectionWith = (id: string) => {
      selectedIds.value = new Set([id])
      lastAnchorId.value = id
      isSelectionMode.value = true
      desktopFocusId.value = null
    }

    const toggle = (id: string) => {
      const next = new Set(selectedIds.value)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      selectedIds.value = next
      lastAnchorId.value = id
      isSelectionMode.value = next.size > 0
      if (isSelectionMode.value) desktopFocusId.value = null
    }

    // Range-select between anchor and id. Caller must supply the ordered list
    // of visible entries so we can compute the slice; store doesn't know about
    // the current view filter.
    const selectRange = (id: string, orderedIds: string[]) => {
      const anchor = lastAnchorId.value ?? id
      const anchorIdx = orderedIds.indexOf(anchor)
      const targetIdx = orderedIds.indexOf(id)
      if (anchorIdx === -1 || targetIdx === -1) {
        toggle(id)
        return
      }
      const [lo, hi] =
        anchorIdx <= targetIdx ? [anchorIdx, targetIdx] : [targetIdx, anchorIdx]
      const next = new Set(selectedIds.value)
      for (let i = lo; i <= hi; i++) {
        const entryId = orderedIds[i]
        if (entryId) next.add(entryId)
      }
      selectedIds.value = next
      isSelectionMode.value = next.size > 0
      if (isSelectionMode.value) desktopFocusId.value = null
    }

    const selectAll = (ids: string[]) => {
      selectedIds.value = new Set(ids)
      isSelectionMode.value = ids.length > 0
      lastAnchorId.value = ids[0] ?? null
      if (isSelectionMode.value) desktopFocusId.value = null
    }

    const clear = () => setMode(false)

    // Clipboard operations -----------------------------------------------------

    const copyToClipboard = () => {
      clipboardEntries.value = selectedEntries.value.slice()
      clipboardMode.value = 'copy'
      clear()
    }

    const cutToClipboard = () => {
      clipboardEntries.value = selectedEntries.value.slice()
      clipboardMode.value = 'cut'
      clear()
    }

    const clearClipboard = () => {
      clipboardEntries.value = []
      clipboardMode.value = null
    }

    const isInClipboard = (id: string): boolean =>
      clipboardEntries.value.some((entry) => entry.id === id)

    const isCut = (id: string): boolean =>
      clipboardMode.value === 'cut' && isInClipboard(id)

    return {
      selectedIds,
      selectedCount,
      isSelectionMode,
      lastAnchorId,
      desktopFocusId,
      selectedEntries,
      hasFoldersInSelection,
      hasItemsInSelection,
      isItemsOnly,
      clipboardEntries,
      clipboardMode,
      hasClipboard,
      isSelected,
      setDesktopFocus,
      enterSelectionWith,
      toggle,
      selectRange,
      selectAll,
      clear,
      copyToClipboard,
      cutToClipboard,
      clearClipboard,
      isInClipboard,
      isCut,
    }
  },
)
