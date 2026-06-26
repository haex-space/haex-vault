import { DesktopIconSizePreset } from '~/stores/vault/settings'
import de from './de.json'
import en from './en.json'
import { useDesktopGrid } from './grid'
import { useDesktopCrud } from './crud'
import { useDesktopMultiDrag } from './multiDrag'
import { useWorkspaceIcons } from './workspaceIcons'
import type { DesktopItemType, IDesktopItem } from './types'
import { createLogger } from '@/stores/logging'

const log = createLogger('DESKTOP')

export const useDesktopStore = defineStore('desktopStore', () => {
  const workspaceStore = useWorkspaceStore()
  const { currentWorkspace } = storeToRefs(workspaceStore)
  const { $i18n } = useNuxtApp()

  $i18n.mergeLocaleMessage('de', { desktop: de })
  $i18n.mergeLocaleMessage('en', { desktop: en })

  const desktopItems = ref<IDesktopItem[]>([])
  const selectedItemIds = ref<Set<string>>(new Set())

  const {
    iconSizePreset,
    syncDesktopIconSizeAsync,
    updateDesktopIconSizeAsync,
    effectiveIconSize,
    gridCellSize,
    snapToGrid,
  } = useDesktopGrid()

  const {
    loadDesktopItemsAsync,
    findFreePosition,
    addDesktopItemAsync,
    updateDesktopItemPositionAsync,
    removeDesktopItemAsync,
    removeDesktopItemsByExtensionIdAsync,
    getDesktopItemByReference,
  } = useDesktopCrud({
    desktopItems,
    currentWorkspace,
    gridCellSize,
    snapToGrid,
    workspaceStore,
  })

  const {
    isMultiDragging,
    multiDragLeaderId,
    startMultiDrag,
    updateMultiDragPositions,
    endMultiDragAsync,
    resetMultiDrag,
  } = useDesktopMultiDrag({
    desktopItems,
    selectedItemIds,
    effectiveIconSize,
    snapToGrid,
    updateDesktopItemPositionAsync,
  })

  const { getWorkspaceIcons } = useWorkspaceIcons({ desktopItems })

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
          icon: extension.iconUrl || undefined,
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
          log.error('Extension not found')
          return
        }

        await extensionsStore.removeExtensionAsync(
          extension.publicKey,
          extension.name,
          extension.version,
        )

        await extensionsStore.loadExtensionsAsync()

        await removeDesktopItemAsync(id)
      } catch (error) {
        log.error('Failed to uninstall:', error)
      }
    }
    // Für später: file und folder handling
  }

  const toggleSelection = (id: string, ctrlKey: boolean = false) => {
    if (ctrlKey) {
      if (selectedItemIds.value.has(id)) {
        selectedItemIds.value.delete(id)
      } else {
        selectedItemIds.value.add(id)
      }
    } else {
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

  const selectedItems = computed(() => {
    return desktopItems.value.filter((item) =>
      selectedItemIds.value.has(item.id),
    )
  })

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

    const secondGroup = [
      {
        label: $i18n.t('desktop.contextMenu.removeFromDesktop'),
        icon: 'i-heroicons-x-mark',
        onSelect: async () => {
          await removeDesktopItemAsync(id)
        },
      },
    ]

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

  /**
   * Resets all store state. Called when closing a vault.
   */
  const reset = () => {
    desktopItems.value = []
    selectedItemIds.value.clear()
    resetMultiDrag()
    iconSizePreset.value = DesktopIconSizePreset.medium
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
    // Reset
    reset,
  }
})
