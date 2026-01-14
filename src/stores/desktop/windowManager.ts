import { defineAsyncComponent, type Component } from 'vue'
import { getFullscreenDimensions } from '~/utils/viewport'
import { isDesktop } from '~/utils/platform'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { EXTENSION_AUTO_START_REQUEST, EXTENSION_WINDOW_CLOSED } from '~/constants/events'

export interface IWindow {
  id: string
  workspaceId: string // Window belongs to a specific workspace
  type: 'system' | 'extension'
  sourceId: string // extensionId or systemWindowId (depends on type)
  title: string
  icon?: string | null
  x: number
  y: number
  width: number
  height: number
  isMinimized: boolean
  zIndex: number
  // Animation source position (icon position)
  sourceX?: number
  sourceY?: number
  sourceWidth?: number
  sourceHeight?: number
  // Animation state
  isOpening?: boolean
  isClosing?: boolean
  // Native webview window flag (separate OS window vs iframe)
  isNativeWebview?: boolean
  // Optional parameters passed when opening the window
  params?: Record<string, unknown>
}

export interface SystemWindowDefinition {
  id: string
  name: string
  icon: string
  component: Component
  defaultWidth: number
  defaultHeight: number
  resizable?: boolean
  singleton?: boolean // Nur eine Instanz erlaubt?
}

export const useWindowManagerStore = defineStore('windowManager', () => {
  const windows = ref<IWindow[]>([])
  const activeWindowId = ref<string | null>(null)
  const nextZIndex = ref(100)

  // Window Overview State
  const showWindowOverview = ref(false)

  // Computed: Count of all open windows (including minimized)
  const openWindowsCount = computed(() => windows.value.length)

  // Window Dragging State (for drag & drop to workspaces)
  const draggingWindowId = ref<string | null>(null)

  // Launcher button position (fallback for animations when no source position is available)
  const launcherButtonPosition = ref<{
    x: number
    y: number
    width: number
    height: number
  } | null>(null)

  const setLauncherButtonPosition = (position: {
    x: number
    y: number
    width: number
    height: number
  }) => {
    launcherButtonPosition.value = position
  }

  // System Windows Registry
  const systemWindows: Record<string, SystemWindowDefinition> = {
    settings: {
      id: 'settings',
      name: 'Settings',
      icon: 'i-mdi-cog',
      component: defineAsyncComponent(
        () => import('@/components/haex/system/settings/index.vue'),
      ),
      defaultWidth: 800,
      defaultHeight: 600,
      resizable: true,
      singleton: true,
    },
    marketplace: {
      id: 'marketplace',
      name: 'Marketplace',
      icon: 'i-mdi-store',
      component: defineAsyncComponent(
        () => import('@/components/haex/system/marketplace.vue'),
      ),
      defaultWidth: 1000,
      defaultHeight: 700,
      resizable: true,
      singleton: false,
    },
  }

  const getSystemWindow = (id: string): SystemWindowDefinition | undefined => {
    return systemWindows[id]
  }

  const getAllSystemWindows = (): SystemWindowDefinition[] => {
    return Object.values(systemWindows)
  }

  // Window animation settings
  const windowAnimationDuration = ref(600) // in milliseconds (matches Tailwind duration-600)

  // Get windows for current workspace only
  const currentWorkspaceWindows = computed(() => {
    if (!useWorkspaceStore().currentWorkspace) return []
    return windows.value.filter(
      (w) => w.workspaceId === useWorkspaceStore().currentWorkspace?.id,
    )
  })

  const windowsByWorkspaceId = (workspaceId: string) =>
    computed(() =>
      windows.value.filter((window) => window.workspaceId === workspaceId),
    )

  const moveWindowsToWorkspace = (
    fromWorkspaceId: string,
    toWorkspaceId: string,
  ) => {
    const windowsFrom = windowsByWorkspaceId(fromWorkspaceId)
    windowsFrom.value.forEach((window) => (window.workspaceId = toWorkspaceId))
  }

  const openWindowAsync = async ({
    height = 800,
    icon = '',
    minimized = false,
    params,
    sourceId,
    sourcePosition,
    title,
    type,
    width = 600,
    workspaceId,
  }: {
    height?: number
    icon?: string | null
    minimized?: boolean
    params?: Record<string, unknown>
    sourceId: string
    sourcePosition?: { x: number; y: number; width: number; height: number }
    title?: string
    type: 'system' | 'extension'
    width?: number
    workspaceId?: string
  }) => {
    try {
      // Desktop: Check extension's display_mode preference
      if (type === 'extension') {
        const extensionsStore = useExtensionsStore()
        const extension = extensionsStore.availableExtensions.find(
          (e) => e.id === sourceId,
        )
        const finalTitle = title ?? extension?.name ?? 'Extension'

        // Determine if we should use native window based on display_mode and platform
        const displayMode = extension?.displayMode ?? 'auto'
        const shouldUseNativeWindow =
          displayMode === 'window' || (displayMode === 'auto' && isDesktop())

        console.log('[windowManager] Extension display mode check:', {
          extensionId: sourceId,
          extensionName: extension?.name,
          displayMode,
          isDesktop: isDesktop(),
          shouldUseNativeWindow,
        })

        // Desktop: Extensions can run in native WebviewWindows (separate processes)
        if (isDesktop() && shouldUseNativeWindow) {
          try {
            console.log(
              '[windowManager] Opening native window with sourceId:',
              sourceId,
            )
            console.log('[windowManager] Extension object:', extension)
            // Backend generates and returns the window_id
            const windowId = await invoke<string>(
              'open_extension_webview_window',
              {
                extensionId: sourceId,
                title: finalTitle,
                width,
                height,
                x: undefined, // Let OS handle positioning
                y: undefined,
                minimized,
              },
            )

            // Store minimal metadata for tracking (no UI management needed on desktop)
            const newWindow: IWindow = {
              id: windowId, // Use window_id from backend as ID
              workspaceId: '', // Not used on desktop
              type,
              sourceId,
              title: finalTitle,
              icon,
              x: 0,
              y: 0,
              width,
              height,
              isMinimized: false,
              zIndex: 0,
              isOpening: false,
              isClosing: false,
              isNativeWebview: true, // Mark as native OS window
            }
            windows.value.push(newWindow)

            return windowId
          } catch (error) {
            console.error('Failed to open native extension window:', error)
            throw error
          }
        }

        // If display_mode is 'iframe' or we're not on desktop, fall through to iframe logic
      }

      // Mobile: Full UI-based window management (original logic)
      // Wenn kein workspaceId angegeben ist, nutze die current workspace
      const workspaceStore = useWorkspaceStore()
      let targetWorkspaceId = workspaceId || workspaceStore.currentWorkspace?.id

      console.log('[windowManager] openWindowAsync:', {
        sourceId,
        type,
        providedWorkspaceId: workspaceId,
        currentWorkspaceId: workspaceStore.currentWorkspace?.id,
        targetWorkspaceId,
        workspacesCount: workspaceStore.workspaces?.length,
        currentWorkspaceIndex: workspaceStore.currentWorkspaceIndex,
      })

      // If no workspace is available yet (e.g., during initial sync), try to load/create one
      if (!targetWorkspaceId) {
        console.warn('[windowManager] No active workspace - attempting to load/create workspaces')
        try {
          await workspaceStore.loadWorkspacesAsync()
          targetWorkspaceId = workspaceStore.currentWorkspace?.id

          if (!targetWorkspaceId) {
            console.error('[windowManager] Cannot open window: Failed to create workspace after loading', {
              workspacesCount: workspaceStore.workspaces?.length,
              currentWorkspaceIndex: workspaceStore.currentWorkspaceIndex,
            })
            return
          }
          console.log('[windowManager] Workspace loaded/created successfully:', targetWorkspaceId)
        } catch (error) {
          console.error('[windowManager] Cannot open window: Failed to load/create workspace:', error)
          return
        }
      }

      const workspace = workspaceStore.workspaces?.find(
        (w) => w.id === targetWorkspaceId,
      )
      if (!workspace) {
        console.error('[windowManager] Cannot open window: Invalid workspace', {
          targetWorkspaceId,
          availableWorkspaceIds: workspaceStore.workspaces?.map(w => w.id),
        })
        return
      }

      // System Window specific handling
      if (type === 'system') {
        const systemWindowDef = getSystemWindow(sourceId)
        if (!systemWindowDef) {
          console.error(`System window '${sourceId}' not found in registry`)
          return
        }

        // Singleton check: If already open, activate existing window and switch to its workspace
        if (systemWindowDef.singleton) {
          // Only consider windows that:
          // - Have a valid workspaceId (invalid windows should be ignored)
          // - Are not in the process of closing (isClosing windows will be removed soon)
          const existingWindow = windows.value.find(
            (w) => w.type === 'system' && w.sourceId === sourceId && w.workspaceId && !w.isClosing,
          )
          if (existingWindow) {
            // Switch to the workspace where this window is located
            const workspaceStore = useWorkspaceStore()
            if (existingWindow.workspaceId !== workspaceStore.currentWorkspace?.id) {
              workspaceStore.slideToWorkspace(existingWindow.workspaceId)
            }
            activateWindow(existingWindow.id)
            return existingWindow.id
          }
        }

        // Use system window defaults
        title = title ?? systemWindowDef.name
        icon = icon ?? systemWindowDef.icon
        width = width ?? systemWindowDef.defaultWidth
        height = height ?? systemWindowDef.defaultHeight
      }

      // Create new window
      const windowId = crypto.randomUUID()

      // Calculate viewport-aware size
      const viewportWidth = window.innerWidth
      const viewportHeight = window.innerHeight - 60

      console.log('viewportHeight', window.innerHeight, viewportHeight)

      // Check if we're on a small screen
      const { isSmallScreen } = useUiStore()

      // Minimum dimensions for windowed mode
      const MIN_WINDOW_WIDTH = 800
      const MIN_WINDOW_HEIGHT = 600

      // Check if viewport is too small for the requested window size
      const viewportTooSmall =
        viewportWidth < MIN_WINDOW_WIDTH || viewportHeight < MIN_WINDOW_HEIGHT

      let windowWidth: number
      let windowHeight: number
      let x: number
      let y: number

      if (isSmallScreen || viewportTooSmall) {
        // On small screens or when viewport is too small, make window fullscreen
        // Use helper function to calculate correct dimensions with safe areas
        const fullscreen = getFullscreenDimensions()
        x = fullscreen.x
        y = fullscreen.y
        windowWidth = fullscreen.width
        windowHeight = fullscreen.height
      } else {
        // On larger screens, use normal sizing and positioning
        windowHeight = Math.min(height, viewportHeight)

        // Adjust width proportionally if needed (optional)
        const aspectRatio = width / height
        windowWidth = Math.min(width, viewportWidth, windowHeight * aspectRatio)

        // Calculate centered position with cascading offset (only count windows in current workspace)
        const offset = currentWorkspaceWindows.value.length * 30
        const centerX = Math.max(0, (viewportWidth - windowWidth) / 1 / 3)
        const centerY = Math.max(0, (viewportHeight - windowHeight) / 1 / 3)
        x = Math.min(centerX + offset, viewportWidth - windowWidth)
        y = Math.min(centerY + offset, viewportHeight - windowHeight)
      }

      // Use launcher button position as fallback if no source position provided
      const effectiveSourcePosition =
        sourcePosition || launcherButtonPosition.value

      const newWindow: IWindow = {
        id: windowId,
        workspaceId: workspace.id,
        type,
        sourceId,
        title: title!,
        icon,
        x,
        y,
        width: windowWidth,
        height: windowHeight,
        isMinimized: false,
        zIndex: nextZIndex.value++,
        sourceX: effectiveSourcePosition?.x,
        sourceY: effectiveSourcePosition?.y,
        sourceWidth: effectiveSourcePosition?.width,
        sourceHeight: effectiveSourcePosition?.height,
        isOpening: true,
        isClosing: false,
        params,
      }

      windows.value.push(newWindow)
      activeWindowId.value = windowId

      // Remove opening flag after animation
      setTimeout(() => {
        const window = windows.value.find((w) => w.id === windowId)
        if (window) {
          window.isOpening = false
        }
      }, windowAnimationDuration.value)

      return windowId
    } catch (error) {
      console.error('Error opening window:', error)
      // Optional: Fehler weiterwerfen wenn nötig
      throw error
    }
  }

  /*****************************************************************************************************
   * TODO: Momentan werden die Fenster einfach nur geschlossen.
   * In Zukunft sollte aber vorher ein close event an die Erweiterungen via postMessage geschickt werden,
   * so dass die Erweiterungen darauf reagieren können, um eventuell ungespeicherte Daten zu sichern
   *****************************************************************************************************/
  const closeWindow = async (windowId: string) => {
    const window = windows.value.find((w) => w.id === windowId)
    if (!window) return

    // Desktop: Close native WebviewWindow for extensions (only if it's actually a native window)
    // Check if extension is using native window mode (not iframe)
    if (isDesktop() && window.type === 'extension') {
      const extensionsStore = useExtensionsStore()
      const extension = extensionsStore.availableExtensions.find(
        (e) => e.id === window.sourceId,
      )
      const displayMode = extension?.displayMode ?? 'auto'
      const isNativeWindow =
        displayMode === 'window' || (displayMode === 'auto' && isDesktop())

      // Only try to close native window if it's actually running as native window
      if (isNativeWindow) {
        try {
          await invoke('close_extension_webview_window', { windowId })
          // Backend will emit event, our listener will update frontend tracking
        } catch (error) {
          console.error('Failed to close native extension window:', error)
        }
        return
      }
      // If not a native window, fall through to iframe cleanup below
    }

    // Mobile: Animated close with iframe cleanup
    // Start closing animation
    window.isClosing = true

    // Remove window after animation completes
    setTimeout(() => {
      const index = windows.value.findIndex((w) => w.id === windowId)
      if (index !== -1) {
        windows.value.splice(index, 1)

        // If closed window was active, activate the topmost window
        if (activeWindowId.value === windowId) {
          if (windows.value.length > 0) {
            const topWindow = windows.value.reduce((max, w) =>
              w.zIndex > max.zIndex ? w : max,
            )
            activeWindowId.value = topWindow.id
          } else {
            activeWindowId.value = null
          }
        }
      }
    }, windowAnimationDuration.value)
  }

  const minimizeWindow = (windowId: string) => {
    const window = windows.value.find((w) => w.id === windowId)
    if (window) {
      window.isMinimized = true
    }
  }

  const restoreWindow = (windowId: string) => {
    const window = windows.value.find((w) => w.id === windowId)
    if (window) {
      window.isMinimized = false
      activateWindow(windowId)
    }
  }

  const activateWindow = (windowId: string) => {
    const window = windows.value.find((w) => w.id === windowId)
    if (window) {
      window.zIndex = nextZIndex.value++
      window.isMinimized = false
      activeWindowId.value = windowId
    }
  }

  const updateWindowPosition = (windowId: string, x: number, y: number) => {
    const window = windows.value.find((w) => w.id === windowId)
    if (window) {
      window.x = x
      window.y = y
    }
  }

  const updateWindowSize = (
    windowId: string,
    width: number,
    height: number,
  ) => {
    const window = windows.value.find((w) => w.id === windowId)
    if (window) {
      window.width = width
      window.height = height
    }
  }

  const isWindowActive = (windowId: string) => {
    return activeWindowId.value === windowId
  }

  const getVisibleWindows = computed(() => {
    return currentWorkspaceWindows.value.filter((w) => !w.isMinimized)
  })

  const getMinimizedWindows = computed(() => {
    return currentWorkspaceWindows.value.filter((w) => w.isMinimized)
  })

  /**
   * Closes all windows for a specific extension (both native and iframe-based)
   * Called before uninstalling an extension
   */
  const closeWindowsByExtensionIdAsync = async (extensionId: string) => {
    const extensionWindows = windows.value.filter(
      (w) => w.type === 'extension' && w.sourceId === extensionId,
    )

    if (extensionWindows.length === 0) return

    console.log(
      `[windowManager] Closing ${extensionWindows.length} window(s) for extension ${extensionId}...`,
    )

    // Close all windows for this extension in parallel
    await Promise.all(
      extensionWindows.map(async (window) => {
        try {
          await closeWindow(window.id)
        } catch (error) {
          console.error(
            `[windowManager] Failed to close window ${window.id}:`,
            error,
          )
        }
      }),
    )

    console.log(`[windowManager] All windows for extension ${extensionId} closed`)
  }

  /**
   * Closes all extension windows (both native and iframe-based)
   * Called when the vault is closed or becomes unavailable
   */
  const closeAllExtensionWindowsAsync = async () => {
    const extensionWindows = windows.value.filter((w) => w.type === 'extension')

    console.log(
      `[windowManager] Closing ${extensionWindows.length} extension window(s)...`,
    )

    // Desktop: Call backend to close all native extension windows
    // This is more reliable than closing one by one, especially for webview reload scenarios
    if (isDesktop()) {
      try {
        await invoke('close_all_extension_webview_windows')
        console.log('[windowManager] Backend closed all native extension windows')
      } catch (error) {
        console.error('[windowManager] Failed to close native windows via backend:', error)
      }
    }

    // Close all extension windows in parallel (for iframe-based windows on mobile or mixed scenarios)
    await Promise.all(
      extensionWindows.map(async (window) => {
        try {
          await closeWindow(window.id)
        } catch (error) {
          console.error(
            `Failed to close extension window ${window.id}:`,
            error,
          )
        }
      }),
    )

    console.log('[windowManager] All extension windows closed')
  }

  // Desktop: Listen for native window close events from Tauri
  // Backend is source of truth, frontend is read-only mirror for tracking
  const setupDesktopEventListenersAsync = async () => {
    if (!isDesktop()) return

    // Listen for native WebviewWindow close events from backend
    await listen<string>(
      EXTENSION_WINDOW_CLOSED,
      (event) => {
        const windowId = event.payload
        console.log(`Native extension window closed: ${windowId}`)

        // Remove from frontend tracking (read-only mirror of backend state)
        const index = windows.value.findIndex((w) => w.id === windowId)
        if (index !== -1) {
          windows.value.splice(index, 1)
        }
      },
    )

    // Listen for extension auto-start requests from ExternalBridge
    // This is triggered when an external client sends a request to an extension
    // that is not currently loaded
    await listen<{ extensionId: string }>(
      EXTENSION_AUTO_START_REQUEST,
      async (event) => {
        const { extensionId } = event.payload
        console.log(`[windowManager] Auto-start request for extension: ${extensionId}`)

        // Check if extension is already open
        const existingWindow = windows.value.find(
          w => w.type === 'extension' && w.sourceId === extensionId,
        )
        if (existingWindow) {
          console.log(`[windowManager] Extension ${extensionId} already has an open window`)
          return
        }

        // Open the extension window minimized (auto-start runs in background)
        // This will respect the extension's display_mode setting
        try {
          await openWindowAsync({
            type: 'extension',
            sourceId: extensionId,
            minimized: true,
          })
          console.log(`[windowManager] Extension ${extensionId} started successfully (minimized)`)
        }
        catch (error) {
          console.error(`[windowManager] Failed to auto-start extension ${extensionId}:`, error)
        }
      },
    )
  }

  // Setup listeners on store creation (only on desktop)
  if (isDesktop()) {
    setupDesktopEventListenersAsync()
  }

  return {
    activateWindow,
    activeWindowId,
    closeAllExtensionWindowsAsync,
    closeWindow,
    closeWindowsByExtensionIdAsync,
    currentWorkspaceWindows,
    draggingWindowId,
    getAllSystemWindows,
    getMinimizedWindows,
    getSystemWindow,
    getVisibleWindows,
    isWindowActive,
    launcherButtonPosition,
    minimizeWindow,
    moveWindowsToWorkspace,
    openWindowAsync,
    openWindowsCount,
    restoreWindow,
    setLauncherButtonPosition,
    showWindowOverview,
    updateWindowPosition,
    updateWindowSize,
    windowAnimationDuration,
    windows,
    windowsByWorkspaceId,
  }
})
