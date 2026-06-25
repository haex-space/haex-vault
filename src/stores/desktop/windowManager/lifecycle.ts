import type { Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getFullscreenDimensions } from '~/utils/viewport'
import { isDesktop } from '~/utils/platform'
import { EXTENSION_AUTO_START_REQUEST, EXTENSION_WINDOW_CLOSED } from '~/constants/events'
import type { Logger } from '~/stores/logging'
import type { IWindow, IWindowTab } from './state'
import { getSystemWindow } from './state'

export interface LifecycleDeps {
  log: Logger
  windows: Ref<IWindow[]>
  activeWindowId: Ref<string | null>
  nextZIndex: Ref<number>
  windowAnimationDuration: Ref<number>
  launcherButtonPosition: Ref<{
    x: number
    y: number
    width: number
    height: number
  } | null>
  currentWorkspaceWindows: Ref<IWindow[]>
  findSingletonTab: (
    type: 'system' | 'extension',
    sourceId: string,
  ) => { window: IWindow; tab: IWindowTab } | null
  activateSingletonTab: (
    win: IWindow,
    tab: IWindowTab,
    params?: Record<string, unknown>,
  ) => string
}

export interface LifecycleActions {
  openWindowAsync: (args: {
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
  }) => Promise<string | undefined>
  closeWindow: (windowId: string) => Promise<void>
  minimizeWindow: (windowId: string) => void
  restoreWindow: (windowId: string) => void
  activateWindow: (windowId: string) => void
  updateWindowPosition: (windowId: string, x: number, y: number) => void
  updateWindowSize: (windowId: string, width: number, height: number) => void
  isWindowActive: (windowId: string) => boolean
  closeWindowsByExtensionIdAsync: (extensionId: string) => Promise<void>
  closeAllExtensionWindowsAsync: () => Promise<void>
  closeAllWindowsAsync: () => Promise<void>
  setupDesktopEventListenersAsync: () => Promise<void>
}

export function createLifecycleActions(deps: LifecycleDeps): LifecycleActions {
  const {
    log,
    windows,
    activeWindowId,
    nextZIndex,
    windowAnimationDuration,
    launcherButtonPosition,
    currentWorkspaceWindows,
    findSingletonTab,
    activateSingletonTab,
  } = deps

  const activateWindow = (windowId: string) => {
    const window = windows.value.find((w) => w.id === windowId)
    if (window) {
      window.zIndex = nextZIndex.value++
      window.isMinimized = false
      activeWindowId.value = windowId
    }
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

        // Desktop: Extensions can run in native WebviewWindows (separate processes)
        if (isDesktop() && shouldUseNativeWindow) {
          try {
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
            const nativeTabId = crypto.randomUUID()
            const newWindow: IWindow = {
              id: windowId, // Use window_id from backend as ID
              workspaceId: '', // Not used on desktop
              tabs: [{ id: nativeTabId, type, sourceId, title: finalTitle, icon, isNativeWebview: true }],
              activeTabId: nativeTabId,
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
            log.error('Failed to open native extension window:', error)
            throw error
          }
        }

        // If display_mode is 'iframe' or we're not on desktop, fall through to iframe logic.
        // Apply the title/icon fallback so the iframe path also gets defined values
        // when the caller didn't provide them.
        title = finalTitle
        icon = icon || extension?.iconUrl || extension?.icon || ''
      }

      // Mobile: Full UI-based window management (original logic)
      // Wenn kein workspaceId angegeben ist, nutze die current workspace
      const workspaceStore = useWorkspaceStore()
      let targetWorkspaceId = workspaceId || workspaceStore.currentWorkspace?.id

      // If no workspace is available yet (e.g., during initial sync), try to load/create one
      if (!targetWorkspaceId) {
        log.warn('No active workspace - attempting to load/create workspaces')
        try {
          await workspaceStore.loadWorkspacesAsync()
          targetWorkspaceId = workspaceStore.currentWorkspace?.id

          if (!targetWorkspaceId) {
            log.error('Cannot open window: Failed to create workspace after loading', {
              workspacesCount: workspaceStore.workspaces?.length,
              currentWorkspaceIndex: workspaceStore.currentWorkspaceIndex,
            })
            return
          }
        } catch (error) {
          log.error('Cannot open window: Failed to load/create workspace:', error)
          return
        }
      }

      const workspace = workspaceStore.workspaces?.find(
        (w) => w.id === targetWorkspaceId,
      )
      if (!workspace) {
        log.error('Cannot open window: Invalid workspace', {
          targetWorkspaceId,
          availableWorkspaceIds: workspaceStore.workspaces?.map(w => w.id),
        })
        return
      }

      // System Window specific handling
      if (type === 'system') {
        const systemWindowDef = getSystemWindow(sourceId)
        if (!systemWindowDef) {
          log.error(`System window '${sourceId}' not found in registry`)
          return
        }

        // Singleton check: If already open as any tab, focus it instead of opening a new window
        if (systemWindowDef.singleton) {
          const found = findSingletonTab('system', sourceId)
          if (found) {
            // Verify the window's workspace still exists — if not, remove the stale window
            const workspaceStore = useWorkspaceStore()
            const workspaceExists = workspaceStore.workspaces?.some(
              (ws) => ws.id === found.window.workspaceId,
            )
            if (!workspaceExists) {
              log.warn(`Removing stale singleton window '${sourceId}' (workspace ${found.window.workspaceId} no longer exists)`)
              windows.value = windows.value.filter((w) => w.id !== found.window.id)
              // Fall through to create a new window below
            } else {
              return activateSingletonTab(found.window, found.tab, params)
            }
          }
        }

        // Use system window defaults
        title = title ?? systemWindowDef.name
        icon = icon || systemWindowDef.icon
        width = width ?? systemWindowDef.defaultWidth
        height = height ?? systemWindowDef.defaultHeight
      }

      // Create new window
      const windowId = crypto.randomUUID()

      // Calculate viewport-aware size
      const viewportWidth = window.innerWidth
      const viewportHeight = window.innerHeight - 60

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

      // Create initial tab
      const tabId = crypto.randomUUID()
      const initialTab: IWindowTab = {
        id: tabId,
        type,
        sourceId,
        title: title!,
        icon,
        params,
      }

      const newWindow: IWindow = {
        id: windowId,
        workspaceId: workspace.id,
        tabs: [initialTab],
        activeTabId: tabId,
        // Legacy fields (mirror active tab)
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

      // Push back/forward action so back closes and forward reopens (global stack)
      const navigationStore = useNavigationStore()
      navigationStore.pushBack({
        undo: () => { closeWindow(windowId) },
        redo: () => { openWindowAsync({ type, sourceId, title: newWindow.title, icon: newWindow.icon, params }).catch((e) => log.debug('Redo open window failed:', e)) },
      })

      // Remove opening flag after the CSS transition completes.
      // The component internally handles the two-phase animation (start → ready)
      // via requestAnimationFrame to ensure the initial state is painted first.
      setTimeout(() => {
        const window = windows.value.find((w) => w.id === windowId)
        if (window) {
          window.isOpening = false
        }
      }, windowAnimationDuration.value)

      return windowId
    } catch (error) {
      log.error('Error opening window:', error)
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
          log.error('Failed to close native extension window:', error)
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
        useNavigationStore().clearWindowStacks(windowId)
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

  /**
   * Closes all windows for a specific extension (both native and iframe-based)
   * Called before uninstalling an extension
   */
  const closeWindowsByExtensionIdAsync = async (extensionId: string) => {
    const extensionWindows = windows.value.filter(
      (w) => w.type === 'extension' && w.sourceId === extensionId,
    )

    if (extensionWindows.length === 0) return

    // Close all windows for this extension in parallel
    await Promise.all(
      extensionWindows.map(async (window) => {
        try {
          await closeWindow(window.id)
        } catch (error) {
          log.error(
            `Failed to close window ${window.id}:`,
            error,
          )
        }
      }),
    )

  }

  /**
   * Closes all extension windows (both native and iframe-based)
   * Called when the vault is closed or becomes unavailable
   */
  const closeAllExtensionWindowsAsync = async () => {
    const extensionWindows = windows.value.filter((w) => w.type === 'extension')

    // Desktop: Call backend to close all native extension windows
    // This is more reliable than closing one by one, especially for webview reload scenarios
    if (isDesktop()) {
      try {
        await invoke('close_all_extension_webview_windows')
      } catch (error) {
        log.error('Failed to close native windows via backend:', error)
      }
    }

    // Close all extension windows in parallel (for iframe-based windows on mobile or mixed scenarios)
    await Promise.all(
      extensionWindows.map(async (window) => {
        try {
          await closeWindow(window.id)
        } catch (error) {
          log.error(
            `Failed to close extension window ${window.id}:`,
            error,
          )
        }
      }),
    )

  }

  /**
   * Closes ALL windows (system + extension).
   * Called when the vault is closed to prevent stale windows with invalid workspace IDs.
   */
  const closeAllWindowsAsync = async () => {
    await closeAllExtensionWindowsAsync()
    windows.value = []
  }

  // Desktop: Listen for native window close events from Tauri
  // Backend is source of truth, frontend is read-only mirror for tracking
  const setupDesktopEventListenersAsync = async () => {
    if (!isDesktop()) return

    log.info('Setting up desktop event listeners...')
    log.debug('EXTENSION_WINDOW_CLOSED event:', EXTENSION_WINDOW_CLOSED)
    log.debug('EXTENSION_AUTO_START_REQUEST event:', EXTENSION_AUTO_START_REQUEST)

    // Listen for native WebviewWindow close events from backend.
    // Pin to the main window via target='main' — backend emits via
    // emit_to("main", …) and Tauri v2's default-Any listener loses
    // these events in production builds.
    await listen<string>(
      EXTENSION_WINDOW_CLOSED,
      (event) => {
        const windowId = event.payload
        log.info(`Native extension window closed: ${windowId}`)

        // Remove from frontend tracking (read-only mirror of backend state)
        const index = windows.value.findIndex((w) => w.id === windowId)
        if (index !== -1) {
          windows.value.splice(index, 1)
        }
      },
      { target: 'main' },
    )

    // Listen for extension auto-start requests from ExternalBridge.
    // Triggered when an external client sends a request for an extension
    // that is not currently loaded. Same target='main' pinning as above.
    await listen<{ extensionId: string }>(
      EXTENSION_AUTO_START_REQUEST,
      async (event) => {
        const { extensionId } = event.payload
        log.info('========== AUTO-START REQUEST RECEIVED ==========')
        log.info(`Extension ID: ${extensionId}`)
        log.debug('Event payload:', JSON.stringify(event.payload))
        log.debug(`Current windows count: ${windows.value.length}`)
        log.debug('Current windows:', windows.value.map(w => ({ id: w.id, type: w.type, sourceId: w.sourceId })))

        // Check if extension is already open
        const existingWindow = windows.value.find(
          w => w.type === 'extension' && w.sourceId === extensionId,
        )
        if (existingWindow) {
          log.info(`Extension ${extensionId} already has an open window: ${existingWindow.id}`)
          return
        }

        log.info('No existing window found, opening new window...')

        // Open the extension window minimized (auto-start runs in background)
        // This will respect the extension's display_mode setting
        try {
          log.debug(`Calling openWindowAsync for extension ${extensionId}...`)
          await openWindowAsync({
            type: 'extension',
            sourceId: extensionId,
            minimized: true,
          })
          log.info(`Extension ${extensionId} started successfully (minimized)`)
          log.debug(`Windows after open: ${windows.value.length}`)
        }
        catch (error) {
          log.error(`Failed to auto-start extension ${extensionId}:`, error)
        }
        log.info('========== AUTO-START REQUEST COMPLETE ==========')
      },
      { target: 'main' },
    )

    log.info('Desktop event listeners setup complete')
  }

  return {
    openWindowAsync,
    closeWindow,
    minimizeWindow,
    restoreWindow,
    activateWindow,
    updateWindowPosition,
    updateWindowSize,
    isWindowActive,
    closeWindowsByExtensionIdAsync,
    closeAllExtensionWindowsAsync,
    closeAllWindowsAsync,
    setupDesktopEventListenersAsync,
  }
}
