import { asc, eq } from 'drizzle-orm'
import {
  haexWorkspaces,
  type SelectHaexWorkspaces,
} from '~/database/schemas'
import type { Swiper } from 'swiper/types'
import { convertFileSrc } from '@tauri-apps/api/core'

export type IWorkspace = SelectHaexWorkspaces

export const useWorkspaceStore = defineStore('workspaceStore', () => {
  const vaultStore = useVaultStore()
  const windowStore = useWindowManagerStore()
  const { deviceId } = storeToRefs(useDeviceStore())

  const { currentVault } = storeToRefs(vaultStore)

  const swiperInstance = ref<Swiper | null>(null)

  const allowSwipe = ref(true)

  // Workspace Overview Mode (GNOME-style)
  const isOverviewMode = ref(false)

  const workspaces = ref<IWorkspace[]>([])

  const currentWorkspaceIndex = ref(0)

  // Load workspaces from database
  const loadWorkspacesAsync = async () => {
    if (!currentVault.value?.drizzle) {
      console.error('[WORKSPACE] Kein Vault geöffnet')
      return
    }

    if (!deviceId.value) {
      console.error('[WORKSPACE] Keine DeviceId vergeben')
      return
    }

    console.log('[WORKSPACE] Loading workspaces for deviceId:', deviceId.value)

    try {
      // First, let's see ALL workspaces in the database (for debugging)
      const allWorkspaces = await currentVault.value.drizzle
        .select()
        .from(haexWorkspaces)
        .orderBy(asc(haexWorkspaces.position))

      console.log('[WORKSPACE] ALL workspaces in database:', allWorkspaces.length)
      console.log('[WORKSPACE] ALL workspaces data:', allWorkspaces.map(w => ({ id: w.id, name: w.name, deviceId: w.deviceId })))

      // Now filter by current device
      const items = await currentVault.value.drizzle
        .select()
        .from(haexWorkspaces)
        .where(eq(haexWorkspaces.deviceId, deviceId.value))
        .orderBy(asc(haexWorkspaces.position))

      console.log('[WORKSPACE] Workspaces for this device:', items.length)
      console.log('[WORKSPACE] Workspace IDs:', items.map(w => ({ id: w.id, name: w.name, deviceId: w.deviceId })))
      workspaces.value = items

      // Create default workspace if none exist
      if (items.length === 0) {
        console.log('[WORKSPACE] No workspaces found, creating default...')
        await addWorkspaceAsync('Workspace 1')
      }

      // Log current workspace after loading
      console.log('[WORKSPACE] currentWorkspaceIndex:', currentWorkspaceIndex.value)
      console.log('[WORKSPACE] currentWorkspace:', workspaces.value[currentWorkspaceIndex.value])
    } catch (error) {
      console.error('[WORKSPACE] Fehler beim Laden der Workspaces:', error)
      throw error
    }
  }

  const currentWorkspace = computed(() => {
    const ws = workspaces.value[currentWorkspaceIndex.value]
    if (!ws && workspaces.value.length > 0) {
      console.warn('[WORKSPACE] currentWorkspace is undefined but workspaces exist!', {
        currentWorkspaceIndex: currentWorkspaceIndex.value,
        workspacesLength: workspaces.value.length,
        workspaceIds: workspaces.value.map(w => w.id),
      })
    }
    return ws
  })

  const addWorkspaceAsync = async (name?: string) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    if (!deviceId.value) {
      return
    }

    try {
      const newIndex = workspaces.value.length + 1
      const newWorkspace = {
        name: name || `Workspace ${newIndex}`,
        position: workspaces.value.length,
        deviceId: deviceId.value,
      }

      const result = await currentVault.value.drizzle
        .insert(haexWorkspaces)
        .values(newWorkspace)
        .returning()

      if (result.length > 0 && result[0]) {
        workspaces.value.push(result[0])
        currentWorkspaceIndex.value = workspaces.value.length - 1
        return result[0]
      }
    } catch (error) {
      console.error('Fehler beim Hinzufügen des Workspace:', error)
      throw error
    }
  }

  const closeWorkspaceAsync = async (workspaceId: string) => {
    const openWindows = windowStore.windowsByWorkspaceId(workspaceId)

    for (const window of openWindows.value) {
      windowStore.closeWindow(window.id)
    }

    return await removeWorkspaceAsync(workspaceId)
  }

  const removeWorkspaceAsync = async (workspaceId: string) => {
    // Don't allow removing the last workspace
    if (workspaces.value.length <= 1) return

    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    const index = workspaces.value.findIndex((ws) => ws.id === workspaceId)
    if (index === -1) return

    try {
      await currentVault.value.drizzle.transaction(async (tx) => {
        // Delete workspace
        await tx
          .delete(haexWorkspaces)
          .where(eq(haexWorkspaces.id, workspaceId))

        // Update local state
        workspaces.value.splice(index, 1)
        workspaces.value.forEach((workspace, idx) => {
          workspace.position = idx
        })

        // Update positions in database
        for (const workspace of workspaces.value) {
          await tx
            .update(haexWorkspaces)
            .set({ position: workspace.position })
            .where(eq(haexWorkspaces.id, workspace.id))
        }
      })

      // Adjust current index if needed
      if (currentWorkspaceIndex.value >= workspaces.value.length) {
        currentWorkspaceIndex.value = workspaces.value.length - 1
      }
    } catch (error) {
      console.error('Fehler beim Entfernen des Workspace:', error)
      throw error
    }
  }

  const switchToWorkspace = (workspaceId?: string) => {
    // Guard: If no workspaces loaded yet, ignore the call
    if (workspaces.value.length === 0) {
      console.log('[WORKSPACE] switchToWorkspace called but no workspaces loaded yet, ignoring')
      return currentWorkspaceIndex.value
    }

    // Guard: If no workspaceId provided, stay on current workspace
    if (!workspaceId) {
      console.log('[WORKSPACE] switchToWorkspace called with undefined workspaceId, staying on current')
      return currentWorkspaceIndex.value
    }

    const workspace = workspaces.value.find((w) => w.id === workspaceId)

    console.log('[WORKSPACE] switchToWorkspace', { workspaceId, found: !!workspace, workspacesCount: workspaces.value.length })
    if (workspace) {
      currentWorkspaceIndex.value = workspace.position
    } else {
      console.warn('[WORKSPACE] Workspace not found, defaulting to index 0:', workspaceId)
      currentWorkspaceIndex.value = 0
    }

    return currentWorkspaceIndex.value
  }

  const switchToNext = () => {
    if (currentWorkspaceIndex.value < workspaces.value.length - 1) {
      currentWorkspaceIndex.value++
    }
  }

  const switchToPrevious = () => {
    if (currentWorkspaceIndex.value > 0) {
      currentWorkspaceIndex.value--
    }
  }

  const renameWorkspaceAsync = async (workspaceId: string, newName: string) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    try {
      const result = await currentVault.value.drizzle
        .update(haexWorkspaces)
        .set({ name: newName })
        .where(eq(haexWorkspaces.id, workspaceId))
        .returning()

      if (result.length > 0 && result[0]) {
        const index = workspaces.value.findIndex((ws) => ws.id === workspaceId)
        if (index !== -1) {
          workspaces.value[index] = result[0]
        }
      }
    } catch (error) {
      console.error('Fehler beim Umbenennen des Workspace:', error)
      throw error
    }
  }

  const slideToWorkspace = (workspaceId?: string) => {
    const index = switchToWorkspace(workspaceId)
    if (swiperInstance.value) {
      swiperInstance.value.slideTo(index)
    }
    isOverviewMode.value = false
  }

  const updateWorkspaceBackgroundAsync = async (
    workspaceId: string,
    base64Image: string | null,
  ) => {
    if (!currentVault.value?.drizzle) {
      throw new Error('Kein Vault geöffnet')
    }

    try {
      const result = await currentVault.value.drizzle
        .update(haexWorkspaces)
        .set({ background: base64Image })
        .where(eq(haexWorkspaces.id, workspaceId))
        .returning()

      if (result.length > 0 && result[0]) {
        const index = workspaces.value.findIndex((ws) => ws.id === workspaceId)
        if (index !== -1) {
          workspaces.value[index] = result[0]
        }
      }
    } catch (error) {
      console.error('Fehler beim Aktualisieren des Workspace-Hintergrunds:', error)
      throw error
    }
  }

  const getWorkspaceBackgroundStyle = (workspace: IWorkspace) => {
    if (!workspace.background) return {}

    // The background field contains the absolute file path
    // Convert it to an asset URL
    const assetUrl = convertFileSrc(workspace.background)

    return {
      backgroundImage: `url(${assetUrl})`,
      backgroundSize: 'cover',
      backgroundPosition: 'center',
      backgroundRepeat: 'no-repeat',
    }
  }

  const getWorkspaceContextMenuItems = (workspaceId: string) => {
    const windowManager = useWindowManagerStore()

    return [[
      {
        label: 'Hintergrund ändern',
        icon: 'i-mdi-image',
        onSelect: async () => {
          // Store the workspace ID for settings to use
          currentWorkspaceIndex.value = workspaces.value.findIndex(
            (ws) => ws.id === workspaceId,
          )
          // Get settings window info
          const settingsWindow = windowManager.getAllSystemWindows()
            .find((win) => win.id === 'settings')

          if (settingsWindow) {
            await windowManager.openWindowAsync({
              type: 'system',
              sourceId: settingsWindow.id,
              title: settingsWindow.name,
              icon: settingsWindow.icon || undefined,
              workspaceId,
            })
          }
        },
      },
    ]]
  }

  /**
   * Resets all store state. Called when closing a vault.
   */
  const reset = () => {
    workspaces.value = []
    currentWorkspaceIndex.value = 0
    swiperInstance.value = null
    allowSwipe.value = true
    isOverviewMode.value = false
    console.log('[WORKSPACE STORE] Store reset')
  }

  return {
    addWorkspaceAsync,
    allowSwipe,
    closeWorkspaceAsync,
    currentWorkspace,
    currentWorkspaceIndex,
    getWorkspaceBackgroundStyle,
    getWorkspaceContextMenuItems,
    isOverviewMode,
    slideToWorkspace,
    loadWorkspacesAsync,
    removeWorkspaceAsync,
    renameWorkspaceAsync,
    swiperInstance,
    switchToNext,
    switchToPrevious,
    switchToWorkspace,
    updateWorkspaceBackgroundAsync,
    workspaces,
    // Reset
    reset,
  }
})
