import type { Ref, ComputedRef } from 'vue'
import { eq } from 'drizzle-orm'
import { haexDesktopItems } from '~/database/schemas'
import { requireDb } from '~/stores/vault'
import type { InsertHaexDesktopItems } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import type { DesktopItemType, IDesktopItem } from './types'

const log = createLogger('DESKTOP')

interface CrudDeps {
  desktopItems: Ref<IDesktopItem[]>
  currentWorkspace: Ref<{ id: string } | null | undefined>
  gridCellSize: ComputedRef<number>
  snapToGrid: (
    x: number,
    y: number,
    iconWidth?: number,
    iconHeight?: number,
  ) => { x: number; y: number }
  workspaceStore: { addWorkspaceAsync: () => Promise<{ id?: string } | undefined> }
}

export function useDesktopCrud(deps: CrudDeps) {
  const { desktopItems, currentWorkspace, gridCellSize, snapToGrid, workspaceStore } = deps

  const loadDesktopItemsAsync = async () => {
    const db = requireDb()

    if (!currentWorkspace.value) {
      log.error('No workspace active - cannot load desktop items')
      return
    }

    try {
      const items = await db
        .select()
        .from(haexDesktopItems)
        .where(eq(haexDesktopItems.workspaceId, currentWorkspace.value.id))

      desktopItems.value = items.map((item) => ({
        ...item,
        referenceId:
          item.itemType === 'extension'
            ? item.extensionId!
            : item.systemWindowId!,
      }))
    } catch (error) {
      log.error('Failed to load desktop items:', error)
      throw error
    }
  }

  const findFreePosition = async (
    viewportWidth: number,
    viewportHeight: number,
    workspaceId?: string,
  ): Promise<{ x: number; y: number; workspaceId: string }> => {
    const targetWorkspaceId = workspaceId || currentWorkspace.value?.id
    if (!targetWorkspaceId) {
      return { ...snapToGrid(0, 0), workspaceId: '' }
    }

    const workspaceItems = desktopItems.value.filter(
      (item) => item.workspaceId === targetWorkspaceId,
    )

    const occupiedCells = new Set<string>()
    const cellSize = gridCellSize.value

    workspaceItems.forEach((item) => {
      const col = Math.round(item.positionX / cellSize)
      const row = Math.round(item.positionY / cellSize)
      occupiedCells.add(`${col},${row}`)
    })

    const maxCols = Math.max(1, Math.floor(viewportWidth / cellSize))
    const maxRows = Math.max(1, Math.floor(viewportHeight / cellSize))

    for (let row = 0; row < maxRows; row++) {
      for (let col = 0; col < maxCols; col++) {
        const key = `${col},${row}`
        if (!occupiedCells.has(key)) {
          const rawX = col * cellSize
          const rawY = row * cellSize
          return { ...snapToGrid(rawX, rawY), workspaceId: targetWorkspaceId }
        }
      }
    }

    const newWorkspace = await workspaceStore.addWorkspaceAsync()
    if (newWorkspace?.id) {
      return { ...snapToGrid(0, 0), workspaceId: newWorkspace.id }
    }

    return { ...snapToGrid(0, 0), workspaceId: targetWorkspaceId }
  }

  const addDesktopItemAsync = async (
    itemType: DesktopItemType,
    referenceId: string,
    positionX?: number,
    positionY?: number,
    workspaceId?: string,
  ) => {
    const db = requireDb()

    const targetWorkspaceId = workspaceId || currentWorkspace.value?.id
    if (!targetWorkspaceId) {
      throw new Error('No workspace active')
    }

    let finalX = positionX ?? 0
    let finalY = positionY ?? 0
    let finalWorkspaceId = targetWorkspaceId

    if (positionX === undefined || positionY === undefined) {
      const freePos = await findFreePosition(
        window.innerWidth,
        window.innerHeight,
        targetWorkspaceId,
      )
      finalX = freePos.x
      finalY = freePos.y
      finalWorkspaceId = freePos.workspaceId || targetWorkspaceId
    }

    try {
      const newItem: InsertHaexDesktopItems = {
        workspaceId: finalWorkspaceId,
        itemType: itemType,
        extensionId: itemType === 'extension' ? referenceId : null,
        systemWindowId:
          itemType === 'system' || itemType === 'file' || itemType === 'folder'
            ? referenceId
            : null,
        positionX: finalX,
        positionY: finalY,
      }

      const result = await db
        .insert(haexDesktopItems)
        .values(newItem)
        .returning()

      if (result.length > 0 && result[0]) {
        const itemWithRef = {
          ...result[0],
          referenceId:
            itemType === 'extension'
              ? result[0].extensionId!
              : result[0].systemWindowId!,
        }
        desktopItems.value.push(itemWithRef)
        return itemWithRef
      }
    } catch (error) {
      log.error('Failed to add desktop item:', {
        error,
        itemType,
        referenceId,
        workspaceId: targetWorkspaceId,
        position: { x: positionX, y: positionY },
      })
      throw error
    }
  }

  const updateDesktopItemPositionAsync = async (
    id: string,
    positionX: number,
    positionY: number,
  ) => {
    const db = requireDb()

    try {
      const result = await db
        .update(haexDesktopItems)
        .set({
          positionX: positionX,
          positionY: positionY,
        })
        .where(eq(haexDesktopItems.id, id))
        .returning()

      if (result.length > 0 && result[0]) {
        const index = desktopItems.value.findIndex((item) => item.id === id)
        if (index !== -1) {
          const item = result[0]
          desktopItems.value[index] = {
            ...item,
            referenceId:
              item.itemType === 'extension'
                ? item.extensionId!
                : item.systemWindowId!,
          }
        }
      }
    } catch (error) {
      log.error('Failed to update desktop item position:', error)
      throw error
    }
  }

  const removeDesktopItemAsync = async (id: string) => {
    const db = requireDb()

    try {
      await db
        .delete(haexDesktopItems)
        .where(eq(haexDesktopItems.id, id))

      desktopItems.value = desktopItems.value.filter((item) => item.id !== id)
    } catch (error) {
      log.error('Failed to remove desktop item:', error)
      throw error
    }
  }

  const removeDesktopItemsByExtensionIdAsync = async (extensionId: string) => {
    const db = requireDb()

    try {
      const itemsToRemove = desktopItems.value.filter(
        (item) =>
          item.itemType === 'extension' && item.extensionId === extensionId,
      )

      for (const item of itemsToRemove) {
        await db
          .delete(haexDesktopItems)
          .where(eq(haexDesktopItems.id, item.id))
      }

      desktopItems.value = desktopItems.value.filter(
        (item) =>
          !(item.itemType === 'extension' && item.extensionId === extensionId),
      )

    } catch (error) {
      log.error(
        'Failed to remove desktop items for extension:',
        error,
      )
      throw error
    }
  }

  const getDesktopItemByReference = (
    itemType: DesktopItemType,
    referenceId: string,
  ) => {
    return desktopItems.value.find((item) => {
      if (item.itemType !== itemType) return false
      if (itemType === 'extension') {
        return item.extensionId === referenceId
      } else {
        return item.systemWindowId === referenceId
      }
    })
  }

  return {
    loadDesktopItemsAsync,
    findFreePosition,
    addDesktopItemAsync,
    updateDesktopItemPositionAsync,
    removeDesktopItemAsync,
    removeDesktopItemsByExtensionIdAsync,
    getDesktopItemByReference,
  }
}
