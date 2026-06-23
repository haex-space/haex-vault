import type { Ref, ComputedRef } from 'vue'
import type { IDesktopItem } from './types'

interface MultiDragDeps {
  desktopItems: Ref<IDesktopItem[]>
  selectedItemIds: Ref<Set<string>>
  effectiveIconSize: ComputedRef<number>
  snapToGrid: (
    x: number,
    y: number,
    iconWidth?: number,
    iconHeight?: number,
  ) => { x: number; y: number }
  updateDesktopItemPositionAsync: (
    id: string,
    positionX: number,
    positionY: number,
  ) => Promise<void>
}

export function useDesktopMultiDrag(deps: MultiDragDeps) {
  const {
    desktopItems,
    selectedItemIds,
    effectiveIconSize,
    snapToGrid,
    updateDesktopItemPositionAsync,
  } = deps

  const isMultiDragging = ref(false)
  const multiDragOffsets = ref<Map<string, { dx: number; dy: number }>>(
    new Map(),
  )
  const multiDragLeaderId = ref<string | null>(null)

  const startMultiDrag = (leaderId: string) => {
    if (selectedItemIds.value.size <= 1) return false
    if (!selectedItemIds.value.has(leaderId)) return false

    const leaderItem = desktopItems.value.find((item) => item.id === leaderId)
    if (!leaderItem) return false

    multiDragLeaderId.value = leaderId
    isMultiDragging.value = true
    multiDragOffsets.value.clear()

    selectedItemIds.value.forEach((itemId) => {
      if (itemId === leaderId) {
        multiDragOffsets.value.set(itemId, { dx: 0, dy: 0 })
      } else {
        const item = desktopItems.value.find((i) => i.id === itemId)
        if (item) {
          multiDragOffsets.value.set(itemId, {
            dx: item.positionX - leaderItem.positionX,
            dy: item.positionY - leaderItem.positionY,
          })
        }
      }
    })

    return true
  }

  const updateMultiDragPositions = (leaderX: number, leaderY: number) => {
    if (!isMultiDragging.value || !multiDragLeaderId.value) return

    multiDragOffsets.value.forEach((offset, itemId) => {
      const item = desktopItems.value.find((i) => i.id === itemId)
      if (item) {
        item.positionX = leaderX + offset.dx
        item.positionY = leaderY + offset.dy
      }
    })
  }

  const endMultiDragAsync = async (
    leaderIconWidth?: number,
    leaderIconHeight?: number,
    viewportWidth?: number,
    viewportHeight?: number,
  ) => {
    if (!isMultiDragging.value || !multiDragLeaderId.value) return

    const leaderItem = desktopItems.value.find(
      (i) => i.id === multiDragLeaderId.value,
    )
    if (!leaderItem) return

    const leaderSnapped = snapToGrid(
      leaderItem.positionX,
      leaderItem.positionY,
      leaderIconWidth,
      leaderIconHeight,
    )

    const snapDeltaX = leaderSnapped.x - leaderItem.positionX
    const snapDeltaY = leaderSnapped.y - leaderItem.positionY

    const promises: Promise<void>[] = []

    let minX = Number.MAX_SAFE_INTEGER
    let minY = Number.MAX_SAFE_INTEGER
    let maxX = 0
    let maxY = 0

    const iconWidth = leaderIconWidth || effectiveIconSize.value
    const iconHeight = leaderIconHeight || effectiveIconSize.value

    multiDragOffsets.value.forEach((_, itemId) => {
      const item = desktopItems.value.find((i) => i.id === itemId)
      if (item) {
        const newX = item.positionX + snapDeltaX
        const newY = item.positionY + snapDeltaY
        minX = Math.min(minX, newX)
        minY = Math.min(minY, newY)
        maxX = Math.max(maxX, newX + iconWidth)
        maxY = Math.max(maxY, newY + iconHeight)
      }
    })

    let viewportAdjustX = 0
    let viewportAdjustY = 0

    if (viewportWidth && viewportHeight) {
      if (minX < 0) {
        viewportAdjustX = -minX
      }
      if (minY < 0) {
        viewportAdjustY = -minY
      }

      if (maxX > viewportWidth) {
        viewportAdjustX = Math.min(viewportAdjustX, viewportWidth - maxX)
      }
      if (maxY > viewportHeight) {
        viewportAdjustY = Math.min(viewportAdjustY, viewportHeight - maxY)
      }
    }

    multiDragOffsets.value.forEach((_, itemId) => {
      const item = desktopItems.value.find((i) => i.id === itemId)
      if (item) {
        item.positionX = item.positionX + snapDeltaX + viewportAdjustX
        item.positionY = item.positionY + snapDeltaY + viewportAdjustY

        promises.push(
          updateDesktopItemPositionAsync(itemId, item.positionX, item.positionY),
        )
      }
    })

    await Promise.all(promises)

    isMultiDragging.value = false
    multiDragLeaderId.value = null
    multiDragOffsets.value.clear()
  }

  const resetMultiDrag = () => {
    isMultiDragging.value = false
    multiDragOffsets.value.clear()
    multiDragLeaderId.value = null
  }

  return {
    isMultiDragging,
    multiDragOffsets,
    multiDragLeaderId,
    startMultiDrag,
    updateMultiDragPositions,
    endMultiDragAsync,
    resetMultiDrag,
  }
}
