import type { Ref } from 'vue'
import type { DesktopItemType, IDesktopItem } from './types'

interface WorkspaceIconsDeps {
  desktopItems: Ref<IDesktopItem[]>
}

export function useWorkspaceIcons(deps: WorkspaceIconsDeps) {
  const { desktopItems } = deps

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
          const { localizedName } = useExtensionI18n()
          label = extension ? localizedName(extension.name, extension.i18n) : 'Unknown'
          icon = extension?.iconUrl || ''
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

  const getWorkspaceIcons = (workspaceId: string) => {
    return workspaceIconsMap.value.get(workspaceId) || []
  }

  return {
    workspaceIconsMap,
    getWorkspaceIcons,
  }
}
