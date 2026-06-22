import type { MaybeRefOrGetter } from 'vue'

interface GroupLike {
  id: string
}

interface UseGroupRowOptions {
  /**
   * Called after a successful item-into-group or group-into-group drop.
   * `tree/item.vue` uses this to expand the destination group so the user
   * immediately sees the moved entry; `list/folder.vue` has no expansion
   * concept and leaves it unset.
   */
  onAfterDrop?: () => void | Promise<void>
}

/**
 * Shared row-interaction primitives for password-group rows — selection state,
 * long-press selection bootstrap, and drag/drop. Both `list/folder.vue` and
 * `tree/item.vue` used to ship near-identical copies of this; centralising
 * keeps the drag MIME types (`application/x-haex-item`,
 * `application/x-haex-group`), the cycle-guard (`descendantIdSet`), and the
 * selection-mode interaction in one place.
 *
 * Caller responsibilities:
 * - bind the returned `rowRef` to the row element (so long-press fires only
 *   on actual touches on that element)
 * - wire the four drag handlers + `onDropAsync` to the row's drag events
 * - compose the click handler — both rows have small variations (folder uses
 *   shift-range selection, tree adds `stopPropagation` on ctrl/meta) that we
 *   keep inline at the call site
 *
 * Layout/visual differences (indent, expand button, recursion, item-count
 * pill, active-row highlight) stay in the components.
 */
export function usePasswordsGroupRow(
  group: MaybeRefOrGetter<GroupLike>,
  options?: UseGroupRowOptions,
) {
  const rowRef = useTemplateRef<HTMLElement>('rowRef')

  const selection = usePasswordsSelectionStore()
  const { isSelectionMode } = storeToRefs(selection)
  const isMultiSelected = computed(() => selection.isSelected(toValue(group).id))
  const isCut = computed(() => selection.isCut(toValue(group).id))

  const { shouldSuppressClick } = useLongPressSelection(rowRef, () => {
    const id = toValue(group).id
    if (!isSelectionMode.value) {
      selection.enterSelectionWith(id)
    }
    else {
      selection.toggle(id)
    }
  })

  const groupsStore = usePasswordsGroupsStore()

  const isDragging = ref(false)
  const isDropTarget = ref(false)

  function onDragStart(event: DragEvent) {
    if (!event.dataTransfer) return
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('application/x-haex-group', toValue(group).id)
    isDragging.value = true
  }

  function onDragEnd() {
    isDragging.value = false
  }

  function onDragOver(event: DragEvent) {
    if (!event.dataTransfer) return
    const types = event.dataTransfer.types
    if (
      !types.includes('application/x-haex-item')
      && !types.includes('application/x-haex-group')
    ) {
      return
    }
    event.dataTransfer.dropEffect = 'move'
    isDropTarget.value = true
  }

  function onDragLeave() {
    isDropTarget.value = false
  }

  async function onDropAsync(event: DragEvent) {
    isDropTarget.value = false
    if (!event.dataTransfer) return

    const groupId = toValue(group).id

    const itemId = event.dataTransfer.getData('application/x-haex-item')
    if (itemId) {
      await groupsStore.setItemGroupAsync(itemId, groupId)
      await options?.onAfterDrop?.()
      return
    }

    const draggedGroupId = event.dataTransfer.getData('application/x-haex-group')
    if (!draggedGroupId || draggedGroupId === groupId) return
    // Cycle guard: cannot drop an ancestor into one of its own descendants.
    if (groupsStore.descendantIdSet(draggedGroupId).has(groupId)) return

    await groupsStore.moveGroupAsync(draggedGroupId, groupId)
    await options?.onAfterDrop?.()
  }

  return {
    rowRef,
    isSelectionMode,
    isMultiSelected,
    isCut,
    shouldSuppressClick,
    isDragging,
    isDropTarget,
    onDragStart,
    onDragEnd,
    onDragOver,
    onDragLeave,
    onDropAsync,
  }
}
