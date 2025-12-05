import { eq } from 'drizzle-orm'
import { haexDesktopItems, haexDevices } from '~/database/schemas'
import type {
  InsertHaexDesktopItems,
  SelectHaexDesktopItems,
} from '~/database/schemas'
import {
  DesktopIconSizePreset,
  iconSizePresetValues,
} from '~/stores/vault/settings'
import de from './de.json'
import en from './en.json'

export type DesktopItemType = 'extension' | 'file' | 'folder' | 'system'

export interface IDesktopItem extends SelectHaexDesktopItems {
  label?: string
  icon?: string
  referenceId: string // Computed: extensionId or systemWindowId
}

export const useDesktopStore = defineStore('desktopStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())
  const workspaceStore = useWorkspaceStore()
  const { currentWorkspace } = storeToRefs(workspaceStore)
  const { $i18n } = useNuxtApp()
  const deviceStore = useDeviceStore()
  const settingsStore = useVaultSettingsStore()

  $i18n.setLocaleMessage('de', { desktop: de })
  $i18n.setLocaleMessage('en', { desktop: en })

  const desktopItems = ref<IDesktopItem[]>([])
  const selectedItemIds = ref<Set<string>>(new Set())

  // Multi-drag state
  const isMultiDragging = ref(false)
  const multiDragOffsets = ref<Map<string, { dx: number; dy: number }>>(
    new Map(),
  )
  const multiDragLeaderId = ref<string | null>(null)

  // Desktop Grid Settings (stored in DB per device)
  const iconSizePreset = ref<DesktopIconSizePreset>(
    DesktopIconSizePreset.medium,
  )

  // Get device internal ID from DB
  const getDeviceInternalIdAsync = async () => {
    if (!deviceStore.deviceId || !currentVault.value?.drizzle) return undefined

    const device = await currentVault.value.drizzle.query.haexDevices.findFirst(
      {
        where: eq(haexDevices.deviceId, deviceStore.deviceId),
      },
    )

    return device?.id ? device.id : undefined
  }

  // Sync icon size from DB
  const syncDesktopIconSizeAsync = async () => {
    const preset = await settingsStore.syncDesktopIconSizeAsync()
    iconSizePreset.value = preset
  }

  // Update icon size in DB
  const updateDesktopIconSizeAsync = async (preset: DesktopIconSizePreset) => {
    await settingsStore.updateDesktopIconSizeAsync(preset)
    iconSizePreset.value = preset
  }

  const effectiveIconSize = computed(() => {
    return iconSizePresetValues[iconSizePreset.value]
  })

  const iconPadding = 30

  // Calculate grid cell size based on icon size
  const gridCellSize = computed(() => {
    // Add padding around icon (30px extra for spacing)
    return effectiveIconSize.value + iconPadding
  })

  // Snap position to grid (centers icon in cell)
  // iconWidth and iconHeight are optional - if provided, they're used for centering
  const snapToGrid = (
    x: number,
    y: number,
    iconWidth?: number,
    iconHeight?: number,
  ) => {
    const cellSize = gridCellSize.value
    const halfCell = cellSize / 2

    // Use provided dimensions or fall back to the effective icon size (not cell size!)
    const actualIconWidth = iconWidth || effectiveIconSize.value
    const actualIconHeight = iconHeight || effectiveIconSize.value

    // Calculate which grid cell the position falls into
    // Add half the icon size to x/y to get the center point for snapping
    const centerX = x + actualIconWidth / 2
    const centerY = y + actualIconHeight / 2

    // Find nearest grid cell center
    // Grid cells are centered at: halfCell, halfCell + cellSize, halfCell + 2*cellSize, ...
    // Which is: halfCell + (n * cellSize) for n = 0, 1, 2, ...
    const col = Math.round((centerX - halfCell) / cellSize)
    const row = Math.round((centerY - halfCell) / cellSize)

    // Calculate the center of the target grid cell
    const gridCenterX = halfCell + col * cellSize
    const gridCenterY = halfCell + row * cellSize

    // Calculate the top-left position that centers the icon in the cell
    const snappedX = gridCenterX - actualIconWidth / 2
    const snappedY = gridCenterY - actualIconHeight / 2

    return {
      x: snappedX,
      y: snappedY,
    }
  }

  const loadDesktopItemsAsync = async () => {
    if (!currentVault.value?.drizzle) {
      console.error('Kein Vault geöffnet')
      return
    }

    if (!currentWorkspace.value) {
      console.error('Kein Workspace aktiv')
      return
    }

    try {
      const items = await currentVault.value.drizzle
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
      console.error('Fehler beim Laden der Desktop-Items:', error)
      throw error
    }
  }

  const addDesktopItemAsync = async (
    itemType: DesktopItemType,
    referenceId: string,
    positionX: number = 0,
    positionY: number = 0,
    workspaceId?: string,
  ) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    const targetWorkspaceId = workspaceId || currentWorkspace.value?.id
    if (!targetWorkspaceId) {
      throw new Error('Kein Workspace aktiv')
    }

    try {
      const newItem: InsertHaexDesktopItems = {
        workspaceId: targetWorkspaceId,
        itemType: itemType,
        extensionId: itemType === 'extension' ? referenceId : null,
        systemWindowId:
          itemType === 'system' || itemType === 'file' || itemType === 'folder'
            ? referenceId
            : null,
        positionX: positionX,
        positionY: positionY,
      }

      const result = await currentVault.value.drizzle
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
      console.error('Fehler beim Hinzufügen des Desktop-Items:', {
        error,
        itemType,
        referenceId,
        workspaceId: targetWorkspaceId,
        position: { x: positionX, y: positionY },
      })

      // Check if it's a FOREIGN KEY constraint error for dev extensions
      const isDevExtension =
        itemType === 'extension' && referenceId.startsWith('dev_')
      const isForeignKeyError =
        error &&
        typeof error === 'object' &&
        'cause' in error &&
        error.cause &&
        typeof error.cause === 'object' &&
        'details' in error.cause &&
        error.cause.details &&
        typeof error.cause.details === 'object' &&
        'reason' in error.cause.details &&
        error.cause.details.reason === 'FOREIGN KEY constraint failed'

      if (isDevExtension && isForeignKeyError) {
        // Throw a custom error that the component can catch
        const devExtensionError = new Error('DEV_EXTENSION_NOT_PERSISTABLE')
        ;(devExtensionError as any).code = 'DEV_EXTENSION_NOT_PERSISTABLE'
        throw devExtensionError
      }

      throw error
    }
  }

  const updateDesktopItemPositionAsync = async (
    id: string,
    positionX: number,
    positionY: number,
  ) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    try {
      const result = await currentVault.value.drizzle
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
      console.error('Fehler beim Aktualisieren der Position:', error)
      throw error
    }
  }

  const removeDesktopItemAsync = async (id: string) => {
    console.log('removeDesktopItemAsync', id)
    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    try {
      // Soft delete using haexTombstone
      await currentVault.value.drizzle
        .delete(haexDesktopItems)
        .where(eq(haexDesktopItems.id, id))

      desktopItems.value = desktopItems.value.filter((item) => item.id !== id)
    } catch (error) {
      console.error('Fehler beim Entfernen des Desktop-Items:', error)
      throw error
    }
  }

  const removeDesktopItemsByExtensionIdAsync = async (extensionId: string) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    try {
      // Find all desktop items for this extension
      const itemsToRemove = desktopItems.value.filter(
        (item) =>
          item.itemType === 'extension' && item.extensionId === extensionId,
      )

      // Delete from database
      for (const item of itemsToRemove) {
        await currentVault.value.drizzle
          .delete(haexDesktopItems)
          .where(eq(haexDesktopItems.id, item.id))
      }

      // Update local state
      desktopItems.value = desktopItems.value.filter(
        (item) =>
          !(item.itemType === 'extension' && item.extensionId === extensionId),
      )

      console.log(
        `Removed ${itemsToRemove.length} desktop items for extension ${extensionId}`,
      )
    } catch (error) {
      console.error(
        'Fehler beim Entfernen der Desktop-Items für Extension:',
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

  const openDesktopItem = (
    itemType: DesktopItemType,
    referenceId: string,
    sourcePosition?: { x: number; y: number; width: number; height: number },
  ) => {
    const windowManager = useWindowManagerStore()

    if (itemType === 'system') {
      const systemWindow = windowManager
        .getAllSystemWindows()
        .find((win) => win.id === referenceId)

      if (systemWindow) {
        windowManager.openWindowAsync({
          sourceId: systemWindow.id,
          type: 'system',
          icon: systemWindow.icon,
          title: systemWindow.name,
          sourcePosition,
        })
      }
    } else if (itemType === 'extension') {
      const extensionsStore = useExtensionsStore()

      const extension = extensionsStore.availableExtensions.find(
        (ext) => ext.id === referenceId,
      )

      if (extension) {
        windowManager.openWindowAsync({
          sourceId: extension.id,
          type: 'extension',
          icon: extension.icon,
          title: extension.name,
          sourcePosition,
        })
      }
    }
    // Für später: file und folder handling
  }

  const uninstallDesktopItem = async (
    id: string,
    itemType: DesktopItemType,
    referenceId: string,
  ) => {
    if (itemType === 'extension') {
      try {
        const extensionsStore = useExtensionsStore()
        const extension = extensionsStore.availableExtensions.find(
          (ext) => ext.id === referenceId,
        )
        if (!extension) {
          console.error('Extension nicht gefunden')
          return
        }

        // Uninstall the extension
        await extensionsStore.removeExtensionAsync(
          extension.publicKey,
          extension.name,
          extension.version,
        )

        // Reload extensions after uninstall
        await extensionsStore.loadExtensionsAsync()

        // Remove desktop item
        await removeDesktopItemAsync(id)
      } catch (error) {
        console.error('Fehler beim Deinstallieren:', error)
      }
    }
    // Für später: file und folder handling
  }

  const toggleSelection = (id: string, ctrlKey: boolean = false) => {
    if (ctrlKey) {
      // Mit Ctrl: Toggle einzelnes Element
      if (selectedItemIds.value.has(id)) {
        selectedItemIds.value.delete(id)
      } else {
        selectedItemIds.value.add(id)
      }
    } else {
      // Ohne Ctrl: Nur dieses Element auswählen
      selectedItemIds.value.clear()
      selectedItemIds.value.add(id)
    }
  }

  const clearSelection = () => {
    selectedItemIds.value.clear()
  }

  const selectAll = () => {
    desktopItems.value.forEach((item) => {
      selectedItemIds.value.add(item.id)
    })
  }

  const isItemSelected = (id: string) => {
    return selectedItemIds.value.has(id)
  }

  // Start multi-drag: Calculate offsets from leader icon
  const startMultiDrag = (leaderId: string) => {
    if (selectedItemIds.value.size <= 1) return false
    if (!selectedItemIds.value.has(leaderId)) return false

    const leaderItem = desktopItems.value.find((item) => item.id === leaderId)
    if (!leaderItem) return false

    multiDragLeaderId.value = leaderId
    isMultiDragging.value = true
    multiDragOffsets.value.clear()

    // Calculate offset for each selected item relative to leader
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

  // Update positions during multi-drag
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

  // End multi-drag and save all positions
  const endMultiDragAsync = async (
    leaderIconWidth?: number,
    leaderIconHeight?: number,
    viewportWidth?: number,
    viewportHeight?: number,
  ) => {
    if (!isMultiDragging.value || !multiDragLeaderId.value) return

    // Find the leader item and snap it first
    const leaderItem = desktopItems.value.find(
      (i) => i.id === multiDragLeaderId.value,
    )
    if (!leaderItem) return

    // Snap leader position with its dimensions
    const leaderSnapped = snapToGrid(
      leaderItem.positionX,
      leaderItem.positionY,
      leaderIconWidth,
      leaderIconHeight,
    )

    // Calculate how much the leader moved after snapping
    const snapDeltaX = leaderSnapped.x - leaderItem.positionX
    const snapDeltaY = leaderSnapped.y - leaderItem.positionY

    // Update all positions: apply the same snap delta to maintain relative positions
    const promises: Promise<void>[] = []

    // Calculate the bounding box of all icons after applying snap delta
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

    // Calculate additional offset to keep all icons within viewport
    let viewportAdjustX = 0
    let viewportAdjustY = 0

    if (viewportWidth && viewportHeight) {
      // If any icon would be outside left/top edge, shift right/down
      if (minX < 0) {
        viewportAdjustX = -minX
      }
      if (minY < 0) {
        viewportAdjustY = -minY
      }

      // If any icon would be outside right/bottom edge, shift left/up
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
        // Apply the same snap delta to all items (this preserves relative positions)
        item.positionX = item.positionX + snapDeltaX + viewportAdjustX
        item.positionY = item.positionY + snapDeltaY + viewportAdjustY

        promises.push(
          updateDesktopItemPositionAsync(itemId, item.positionX, item.positionY),
        )
      }
    })

    await Promise.all(promises)

    // Reset multi-drag state
    isMultiDragging.value = false
    multiDragLeaderId.value = null
    multiDragOffsets.value.clear()
  }

  const selectedItems = computed(() => {
    return desktopItems.value.filter((item) =>
      selectedItemIds.value.has(item.id),
    )
  })

  // Cached workspace icons map to prevent infinite reactive loops
  // This computed caches the enriched desktop items (with label/icon) per workspace
  const workspaceIconsMap = computed(() => {
    const extensionsStore = useExtensionsStore()
    const windowManagerStore = useWindowManagerStore()
    const map = new Map<
      string,
      Array<{
        id: string
        workspaceId: string
        itemType: DesktopItemType
        referenceId: string
        positionX: number
        positionY: number
        label: string
        icon: string
      }>
    >()

    // Group items by workspace
    const itemsByWorkspace = new Map<string, IDesktopItem[]>()
    for (const item of desktopItems.value) {
      if (!itemsByWorkspace.has(item.workspaceId)) {
        itemsByWorkspace.set(item.workspaceId, [])
      }
      itemsByWorkspace.get(item.workspaceId)!.push(item)
    }

    // Map items for each workspace
    for (const [workspaceId, items] of itemsByWorkspace) {
      const enrichedItems = items.map((item) => {
        let label = item.referenceId
        let icon = ''

        if (item.itemType === 'system') {
          const systemWindow = windowManagerStore
            .getAllSystemWindows()
            .find((win) => win.id === item.referenceId)
          label = systemWindow?.name || 'Unknown'
          icon = systemWindow?.icon || ''
        } else if (item.itemType === 'extension') {
          const extension = extensionsStore.availableExtensions.find(
            (ext) => ext.id === item.referenceId,
          )
          label = extension?.name || 'Unknown'
          icon = extension?.icon || ''
        }

        return {
          id: item.id,
          workspaceId: item.workspaceId,
          itemType: item.itemType,
          referenceId: item.referenceId,
          positionX: item.positionX,
          positionY: item.positionY,
          label,
          icon,
        }
      })
      map.set(workspaceId, enrichedItems)
    }

    return map
  })

  // Get icons for a specific workspace (uses cached computed)
  const getWorkspaceIcons = (workspaceId: string) => {
    return workspaceIconsMap.value.get(workspaceId) || []
  }

  // Find a free position on the current workspace grid
  const findFreePosition = (
    viewportWidth: number,
    viewportHeight: number,
    workspaceId?: string,
  ) => {
    const targetWorkspaceId = workspaceId || currentWorkspace.value?.id
    if (!targetWorkspaceId) {
      return snapToGrid(0, 0)
    }

    // Get all items on the target workspace
    const workspaceItems = desktopItems.value.filter(
      (item) => item.workspaceId === targetWorkspaceId,
    )

    // Create a set of occupied grid positions
    const occupiedCells = new Set<string>()
    const cellSize = gridCellSize.value

    workspaceItems.forEach((item) => {
      // Calculate grid cell for this item
      const col = Math.round(item.positionX / cellSize)
      const row = Math.round(item.positionY / cellSize)
      occupiedCells.add(`${col},${row}`)
    })

    // Calculate max columns and rows based on viewport size
    const maxCols = Math.max(1, Math.floor(viewportWidth / cellSize))
    const maxRows = Math.max(1, Math.floor(viewportHeight / cellSize))

    // Find first free position (scan left-to-right, top-to-bottom)
    for (let row = 0; row < maxRows; row++) {
      for (let col = 0; col < maxCols; col++) {
        const key = `${col},${row}`
        if (!occupiedCells.has(key)) {
          // Found free position, snap to grid
          const rawX = col * cellSize
          const rawY = row * cellSize
          return snapToGrid(rawX, rawY)
        }
      }
    }

    // Fallback: return (0, 0) if no free position found (grid is full)
    return snapToGrid(0, 0)
  }

  const removeSelectedItemsAsync = async () => {
    const idsToRemove = Array.from(selectedItemIds.value)
    for (const itemId of idsToRemove) {
      await removeDesktopItemAsync(itemId)
    }
    clearSelection()
  }

  const getContextMenuItems = (
    id: string,
    itemType: DesktopItemType,
    referenceId: string,
    onUninstall: () => void,
  ) => {
    // If multiple items are selected, show bulk action menu
    if (selectedItemIds.value.size > 1 && selectedItemIds.value.has(id)) {
      return [
        [
          {
            label: $i18n.t('desktop.contextMenu.removeSelectedFromDesktop', {
              count: selectedItemIds.value.size,
            }),
            icon: 'i-heroicons-x-mark',
            onSelect: async () => {
              await removeSelectedItemsAsync()
            },
          },
        ],
      ]
    }

    const handleOpen = () => {
      openDesktopItem(itemType, referenceId)
    }

    // Build second menu group based on item type
    const secondGroup = [
      {
        label: $i18n.t('desktop.contextMenu.removeFromDesktop'),
        icon: 'i-heroicons-x-mark',
        onSelect: async () => {
          await removeDesktopItemAsync(id)
        },
      },
    ]

    // Only show uninstall option for extensions
    if (itemType === 'extension') {
      secondGroup.push({
        label: $i18n.t('desktop.contextMenu.uninstall'),
        icon: 'i-heroicons-trash',
        onSelect: async () => {
          onUninstall()
        },
      })
    }

    return [
      [
        {
          label: $i18n.t('desktop.contextMenu.open'),
          icon: 'i-heroicons-arrow-top-right-on-square',
          onSelect: handleOpen,
        },
      ],
      secondGroup,
    ]
  }

  return {
    desktopItems,
    selectedItemIds,
    selectedItems,
    loadDesktopItemsAsync,
    addDesktopItemAsync,
    updateDesktopItemPositionAsync,
    removeDesktopItemAsync,
    removeDesktopItemsByExtensionIdAsync,
    getDesktopItemByReference,
    getContextMenuItems,
    openDesktopItem,
    uninstallDesktopItem,
    toggleSelection,
    clearSelection,
    selectAll,
    isItemSelected,
    // Multi-drag
    isMultiDragging,
    multiDragLeaderId,
    startMultiDrag,
    updateMultiDragPositions,
    endMultiDragAsync,
    // Grid settings
    iconSizePreset,
    syncDesktopIconSizeAsync,
    updateDesktopIconSizeAsync,
    effectiveIconSize,
    gridCellSize,
    snapToGrid,
    findFreePosition,
    // Workspace icons (cached)
    getWorkspaceIcons,
  }
})
