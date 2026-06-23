import { isDesktop } from '~/utils/platform'
import { createLogger } from '~/stores/logging'
import windowManagerDe from '../windowManager.de.json'
import windowManagerEn from '../windowManager.en.json'
import {
  SYSTEM_WINDOW_I18N_KEY_PREFIX,
  getAllSystemWindows,
  getSystemWindow,
  systemWindows,
} from './state'
import type { IWindow } from './state'
import { createTabActions } from './tabs'
import { createLifecycleActions } from './lifecycle'

export type { IWindow, IWindowTab, SystemWindowDefinition } from './state'

const log = createLogger('WINDOW_MGR')

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
  const windowAnimationDuration = ref(300) // in milliseconds (matches CSS transition duration)

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

  // Tab actions need lifecycle's activateWindow/closeWindow; lifecycle needs tab's
  // findSingletonTab/activateSingletonTab. Resolve the cycle by giving lifecycle
  // wrapper closures over the tab actions object, which is assigned below.
  let tabActions: ReturnType<typeof createTabActions>

  const lifecycle = createLifecycleActions({
    log,
    windows,
    activeWindowId,
    nextZIndex,
    windowAnimationDuration,
    launcherButtonPosition,
    currentWorkspaceWindows,
    findSingletonTab: (type, sourceId) => tabActions.findSingletonTab(type, sourceId),
    activateSingletonTab: (win, tab, params) =>
      tabActions.activateSingletonTab(win, tab, params),
  })

  tabActions = createTabActions({
    windows,
    activateWindow: lifecycle.activateWindow,
    closeWindow: lifecycle.closeWindow,
  })

  const getVisibleWindows = computed(() => {
    return currentWorkspaceWindows.value.filter((w) => !w.isMinimized)
  })

  const getMinimizedWindows = computed(() => {
    return currentWorkspaceWindows.value.filter((w) => w.isMinimized)
  })

  // Setup listeners on store creation (only on desktop)
  if (isDesktop()) {
    lifecycle.setupDesktopEventListenersAsync()
  }

  return {
    activateWindow: lifecycle.activateWindow,
    activeWindowId,
    closeAllExtensionWindowsAsync: lifecycle.closeAllExtensionWindowsAsync,
    closeAllWindowsAsync: lifecycle.closeAllWindowsAsync,
    closeWindow: lifecycle.closeWindow,
    closeWindowsByExtensionIdAsync: lifecycle.closeWindowsByExtensionIdAsync,
    currentWorkspaceWindows,
    draggingWindowId,
    getAllSystemWindows,
    getLocalizedSystemWindowName,
    getMinimizedWindows,
    getSystemWindow,
    getVisibleWindows,
    isWindowActive: lifecycle.isWindowActive,
    launcherButtonPosition,
    minimizeWindow: lifecycle.minimizeWindow,
    moveWindowsToWorkspace,
    openWindowAsync: lifecycle.openWindowAsync,
    openWindowsCount,
    restoreWindow: lifecycle.restoreWindow,
    setLauncherButtonPosition,
    showWindowOverview,
    updateWindowPosition: lifecycle.updateWindowPosition,
    updateWindowSize: lifecycle.updateWindowSize,
    windowAnimationDuration,
    windows,
    windowsByWorkspaceId,
    // Tab management
    addTab: tabActions.addTab,
    addNewTabFromActive: tabActions.addNewTabFromActive,
    canAddTab: tabActions.canAddTab,
    switchTab: tabActions.switchTab,
    closeTab: tabActions.closeTab,
  }
})
