import { defineAsyncComponent, type Component } from 'vue'
import { getFullscreenDimensions } from '~/utils/viewport'
import { isDesktop } from '~/utils/platform'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { EXTENSION_AUTO_START_REQUEST, EXTENSION_WINDOW_CLOSED } from '~/constants/events'
import { createLogger } from '~/stores/logging'
import windowManagerDe from './windowManager.de.json'
import windowManagerEn from './windowManager.en.json'

const log = createLogger('WINDOW_MGR')

const SYSTEM_WINDOW_I18N_KEY_PREFIX = 'systemWindows'

export interface IWindowTab {
  id: string
  type: 'system' | 'extension'
  sourceId: string // extensionId or systemWindowId (depends on type)
  title: string
  icon?: string | null
  params?: Record<string, unknown>
  // Native webview window flag (separate OS window vs iframe)
  isNativeWebview?: boolean
}

export interface IWindow {
  id: string
  workspaceId: string // Window belongs to a specific workspace
  // Tab management
  tabs: IWindowTab[]
  activeTabId: string
  // Legacy fields (derived from active tab for backward compat)
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
  const { $i18n } = useNuxtApp()

  // Register system window translations
  $i18n.mergeLocaleMessage('de', { [SYSTEM_WINDOW_I18N_KEY_PREFIX]: windowManagerDe })
  $i18n.mergeLocaleMessage('en', { [SYSTEM_WINDOW_I18N_KEY_PREFIX]: windowManagerEn })

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
    files: {
      id: 'files',
      name: 'Files',
      icon: 'i-mdi-folder',
      component: defineAsyncComponent(
        () => import('@/components/haex/system/files/index.vue'),
      ),
      defaultWidth: 800,
      defaultHeight: 600,
      resizable: true,
      singleton: false,
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

  /** Returns the localized name for a system window, falling back to the English name */
  const getLocalizedSystemWindowName = (id: string): string => {
    const key = `${SYSTEM_WINDOW_I18N_KEY_PREFIX}.${id}`
    const translated = $i18n.t(key)
    // If translation key not found, i18n returns the key itself — fall back to English name
    if (translated === key) {
      return systemWindows[id]?.name ?? id
    }
    return translated
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
              console.warn(`[windowManager] Removing stale singleton window '${sourceId}' (workspace ${found.window.workspaceId} no longer exists)`)
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
        redo: () => { openWindowAsync({ type, sourceId, title: newWindow.title, icon: newWindow.icon, params }).catch(() => {}) },
      })

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

    // Listen for native WebviewWindow close events from backend
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
    )

    // Listen for extension auto-start requests from ExternalBridge
    // This is triggered when an external client sends a request to an extension
    // that is not currently loaded
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
    )

    log.info('Desktop event listeners setup complete')
  }

  // Setup listeners on store creation (only on desktop)
  if (isDesktop()) {
    setupDesktopEventListenersAsync()
  }

  // =========================================================================
  // Tab Management
  // =========================================================================

  /** Check if a source allows multiple instances. */
  const isSourceSingleton = (type: 'system' | 'extension', sourceId: string): boolean => {
    if (type === 'system') {
      return getSystemWindow(sourceId)?.singleton === true
    }
    const extensionsStore = useExtensionsStore()
    const ext = extensionsStore.availableExtensions.find(e => e.id === sourceId)
    return ext?.singleInstance === true
  }

  /** Find an existing open tab for a singleton source across all windows. */
  const findSingletonTab = (
    type: 'system' | 'extension',
    sourceId: string,
  ): { window: IWindow; tab: IWindowTab } | null => {
    for (const win of windows.value) {
      if (win.isClosing || !win.workspaceId) continue
      const tab = win.tabs.find(t => t.type === type && t.sourceId === sourceId)
      if (tab) return { window: win, tab }
    }
    return null
  }

  /** Switch to a singleton tab, activate its window, and optionally switch workspace. */
  const activateSingletonTab = (
    win: IWindow,
    tab: IWindowTab,
    params?: Record<string, unknown>,
  ): string => {
    win.activeTabId = tab.id
    syncWindowFromActiveTab(win)
    if (params) {
      tab.params = { ...tab.params, ...params }
    }
    const workspaceStore = useWorkspaceStore()
    if (win.workspaceId !== workspaceStore.currentWorkspace?.id) {
      workspaceStore.slideToWorkspace(win.workspaceId)
    }
    activateWindow(win.id)
    return win.id
  }

  /** Add a new tab to an existing window. Returns the tab ID or null. */
  const addTab = (windowId: string, tab: Omit<IWindowTab, 'id'>): string | null => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return null

    // Singleton check: focus existing tab across all windows instead of creating a duplicate
    if (isSourceSingleton(tab.type, tab.sourceId)) {
      const found = findSingletonTab(tab.type, tab.sourceId)
      if (found) {
        activateSingletonTab(found.window, found.tab)
        return found.tab.id
      }
    }

    const tabId = crypto.randomUUID()
    win.tabs.push({ id: tabId, ...tab })
    win.activeTabId = tabId
    syncWindowFromActiveTab(win)
    return tabId
  }

  /** Add a new tab that duplicates the active tab's source (for the "+" button). */
  const addNewTabFromActive = (windowId: string): string | null => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return null
    const activeTab = win.tabs.find(t => t.id === win.activeTabId)
    if (!activeTab) return null
    if (isSourceSingleton(activeTab.type, activeTab.sourceId)) return null

    return addTab(windowId, {
      type: activeTab.type,
      sourceId: activeTab.sourceId,
      title: activeTab.title,
      icon: activeTab.icon,
    })
  }

  /** Check if the "+" button should be shown (active source is not singleton). */
  const canAddTab = (windowId: string): boolean => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return false
    const activeTab = win.tabs.find(t => t.id === win.activeTabId)
    if (!activeTab) return false
    return !isSourceSingleton(activeTab.type, activeTab.sourceId)
  }

  /** Switch to a specific tab. */
  const switchTab = (windowId: string, tabId: string) => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return
    if (!win.tabs.some(t => t.id === tabId)) return
    win.activeTabId = tabId
    syncWindowFromActiveTab(win)
  }

  /** Close a tab. Last tab closes the window. */
  const closeTab = (windowId: string, tabId: string) => {
    const win = windows.value.find(w => w.id === windowId)
    if (!win) return

    if (win.tabs.length <= 1) {
      closeWindow(windowId)
      return
    }

    const tabIndex = win.tabs.findIndex(t => t.id === tabId)
    if (tabIndex === -1) return
    win.tabs.splice(tabIndex, 1)
    useNavigationStore().clearTabStacks(tabId)

    if (win.activeTabId === tabId) {
      const newIndex = Math.min(tabIndex, win.tabs.length - 1)
      win.activeTabId = win.tabs[newIndex]!.id
      syncWindowFromActiveTab(win)
    }
  }

  /** Sync the window's legacy fields from the active tab. */
  const syncWindowFromActiveTab = (win: IWindow) => {
    const tab = win.tabs.find(t => t.id === win.activeTabId)
    if (!tab) return
    win.type = tab.type
    win.sourceId = tab.sourceId
    win.title = tab.title
    win.icon = tab.icon
    win.params = tab.params
    win.isNativeWebview = tab.isNativeWebview
  }

  return {
    activateWindow,
    activeWindowId,
    closeAllExtensionWindowsAsync,
    closeAllWindowsAsync,
    closeWindow,
    closeWindowsByExtensionIdAsync,
    currentWorkspaceWindows,
    draggingWindowId,
    getAllSystemWindows,
    getLocalizedSystemWindowName,
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
    // Tab management
    addTab,
    addNewTabFromActive,
    canAddTab,
    switchTab,
    closeTab,
  }
})
